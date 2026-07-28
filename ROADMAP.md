# rust-RCP Roadmap

## Vision

rust-RCP implements the **Remote Control Protocol (RCP)** for automotive
zonal architecture. Historically that meant a bespoke, Zone-enum-addressed
Command/Response/Status model over a 16-byte private wire header, plus a
wide ring of satellite packages (discovery, power-state management,
watchdog, E2E CRC, priority queueing, and bridges to CAN/LIN/DoIP/UDS/
SOME-IP/MQTT/DDS/gRPC/REST/etc.) built up around it.

A conformance gap analysis against the real industry specification this
crate was always meant to track — the **OPEN Alliance TC18 Remote Control
Protocol Specification** — found that the two share nothing at the wire
level. RCP as implemented here is a different, unrelated protocol that
happens to reuse the name. The project has decided on a **full replacement**:
rust-RCP's core is going to become the real OPEN Alliance TC18 RCP, not a
gap-patched version of the private protocol it is today.

This document sequences that replacement, milestone by milestone, and gives
an explicit disposition — replace, adapt, deprecate, or keep — for every one
of the 44 satellite packages currently living alongside the core protocol.

### ⚠️ This is a breaking change. There is no compatibility shim.

Every consumer of the current `Zone` / `Command` / `Response` / `Status` /
`Controller` / `Registry` API — and every satellite package built on top of
it — will stop compiling once the core lands. This is deliberate, and we are
not building a shim to soften it, for two reasons:

1. **The wire formats share nothing.** A shim that translated between the
   old 16-byte frame and a real IEEE 1722 AVTPDU would be pure fiction — it
   couldn't talk to a real TC18 RC Server, so it would only give old callers
   the false impression that their integration still works.
2. **There is no real deployed base to protect.** The current protocol is a
   private, ad-hoc design with no independent implementations in the field
   beyond this ecosystem's own sibling repos (`go-RCP`, `cpp-RCP`, `c-RCP`),
   all of which are undergoing (or will need) the same replacement. A shim
   would add real maintenance cost to preserve compatibility with something
   nobody outside this project ever depended on as a wire contract.

Consumers should expect a major version bump when the new core ships, and
should treat any code written against `rcp::Zone`, `rcp::Command`,
`rcp::Controller`, etc. as needing a rewrite, not a recompile.

---

## Guiding Principles

1. Wire-format conformance is not optional — if it doesn't match the spec's
   framing byte-for-byte, it isn't done.
2. Sequence work so nothing is built on a foundation that will itself change
   later (lifecycle model and register-map split before endpoints; endpoints
   before conditional requests; conditional requests before safety requests).
3. Every satellite package gets an explicit, justified call — silence on a
   package is not an acceptable outcome of this effort.
4. Prefer citing this crate's own spec-extraction section numbers in code
   comments and commit messages over restating spec text; never commit the
   source specification itself.
5. Flag spec ambiguities (DAC, MDIO's scope-list omission, the I²C speed
   enum, unpopulated trigger tables) rather than silently guessing at them.
6. Fragmentation gets one explicit go/no-go decision, not an implicit
   omission — the spec itself treats it as optional for "RCP version 1.0."
7. FuSa/cybersecurity artifact rigor (traceability, HARA, TARA) carries
   forward unchanged as a project value, even though its *content* will need
   a full rebase once the protocol underneath it changes.

---

## Release Plan

| Milestone | Version | Theme |
|---|---|---|
| 0.1 – 0.3 | Released | Original (private-protocol) RCP implementation |
| 1 | v0.4.0 | Wire format core (AVTPDU / ACF_ABB / ACF_GBB) |
| 2 | v0.5.0 | RC Server lifecycle & register-map model |
| 3 | v0.6.0 | Discovery |
| 4 | v0.7.0 | Basic endpoint types (GPIO, SPI, I²C, UART, ADC, PWM) |
| 5 | v0.8.0 | Conditional requests & sequencers |
| 6 | v0.9.0 | E2E CRC safe points & safety requests |
| 7 | v0.10.0 | Remaining endpoint types (LIN, CAN/CAN XL, ISELED, MDIO, Wakeup) |
| 8 | v0.11.0 | Fragmentation go/no-go |
| 9 | v0.12.0 | Satellite package migration |
| 10 | v1.0.0 | TC18 RCP-conformant production release |

---

## Milestone 1 — Wire Format Core `v0.4.0`

Goal:
Replace the bespoke 16-byte frame (`wire.rs`) with the real TC18 wire
protocol: IEEE 1722 AVTPDU framing, the two ACF message types, and the
shared request-descriptor header that carries all per-message addressing.

### AVTPDU Framing

- [x] NTSCF header encode/decode (`ntscf_data_length`, `sequence_num`) — the
      only header variant an RC Server ever sends. Done (v0.4.0-dev):
      `src/avtpdu.rs` adds `NtscfHeader { sequence_num, ntscf_data_length,
      stream_id }` with `encode_ntscf_header`/`decode_ntscf_header`,
      round-tripping and never panicking on truncated/arbitrary input. This
      is a new, additive module — it does not yet replace `wire.rs` (no
      existing caller is cut over) and does not implement `stream_id`
      construction/parsing, which remains the separate "Addressing" item
      below. The specific byte offsets/bit widths are this crate's own
      working interpretation of IEEE 1722 AVTPDU control-format framing
      (see the module's provenance note) and are flagged, per Guiding
      Principle 5, for reconciliation against the OPEN Alliance TC18 Remote
      Control Protocol Specification's behavior before being relied on for
      interop.
- [x] TSCF header encode/decode (`avtp_timestamp`, `stream_data_length`) —
      client-to-server only. Done (v0.4.0-dev): `src/avtpdu.rs` adds
      `TscfHeader { sequence_num, avtp_timestamp, stream_data_length,
      stream_id }` with `encode_tscf_header`/`decode_tscf_header`,
      round-tripping and never panicking on truncated/arbitrary input,
      mirroring the NTSCF work above. `encode_tscf_header` exists for
      symmetry/testing; per the module's doc comment, an RC Server's own
      send path has no occasion to call it, since TSCF is a client-to-server
      header. This is additive alongside `NtscfHeader` — no existing caller
      is cut over, and `stream_id` construction/parsing and full timestamp
      semantics (`message_timestamp`, invalid-timestamp fallback) remain
      separate, later checklist items. The specific byte offsets/bit widths
      are, like the NTSCF header's, this crate's own working interpretation
      of IEEE 1722 AVTPDU control-format framing, flagged per Guiding
      Principle 5 for reconciliation against the OPEN Alliance TC18 Remote
      Control Protocol Specification's behavior before being relied on for
      interop.
- [x] Header-variant selection/rejection rules: drop TSCF-headed AVTPDUs
      outright at a server with no time-sync support. Done (v0.4.0-dev):
      `src/avtpdu.rs` adds `TimeSyncCapability { Capable, Incapable }` and
      `HeaderVariant { Ntscf(NtscfHeader), Tscf(TscfHeader) }`, plus
      `select_header_variant(bytes, TimeSyncCapability) ->
      Result<HeaderVariant, RcpError>`, which peeks the leading subtype
      byte and dispatches to `decode_ntscf_header`/`decode_tscf_header`
      accordingly. NTSCF is always accepted; TSCF is decoded only when the
      server is `Capable`, and rejected outright — before any attempt to
      decode the rest of the header — with the new `RcpError::
      TimeSyncUnsupported` sentinel when `Incapable`. This crate does not
      yet model how a server learns its own time-sync capability (that is
      later server-lifecycle work); `TimeSyncCapability` exists here solely
      to make this selection rule callable and testable against both
      outcomes now. Additive alongside the existing NTSCF/TSCF decoders —
      no existing caller is cut over.

### ACF Messages

- [x] ACF_ABB (`acf_msg_type = 0x0E`) encode/decode — no timestamp field at
      all, not just a zeroed one. Done (v0.4.0-dev): `src/acf.rs` adds
      `AcfAbbMessage { info: ByteMessageInfo, payload }` with
      `encode_acf_abb`/`decode_acf_abb`, round-tripping and never panicking
      on truncated/arbitrary input. `ACF_ABB_HEADER_LEN` (9 bytes: the
      discriminant plus `byte_message_info`) is structurally 8 bytes
      narrower than `ACF_GBB_HEADER_LEN` — there is no reserved gap sized
      for a timestamp, not merely a zeroed one.
- [x] ACF_GBB (`acf_msg_type = 0x0D`) encode/decode — carries the 64-bit
      `message_timestamp`. Done (v0.4.0-dev): `src/acf.rs` adds
      `AcfGbbMessage { info: ByteMessageInfo, message_timestamp: u64,
      payload }` with `encode_acf_gbb`/`decode_acf_gbb`, round-tripping the
      full `u64` range (including the all-zero and all-`0xFF` extremes) and
      never panicking on truncated/arbitrary input. `message_timestamp` is
      carried as a raw passthrough value only — its width/rollover
      semantics remain the separate "Timestamp Semantics" item below.
- [x] Shared `byte_message_info` header: `acf_msg_length`, `pad`, `mtv`,
      11-bit `byte_bus_id`, 4-bit `evt` (ack flag + 3-bit sub-opcode), `hs`,
      `cs`, `transaction_num`, `op`, `rsp`, `err`, `ms`, and the dual-purpose
      `read_size`/`segment_num` field. Done (v0.4.0-dev): `src/acf.rs` adds
      `ByteMessageInfo` with all of the above fields, shared by both
      `AcfAbbMessage` and `AcfGbbMessage`, plus `encode_byte_message_info`/
      `decode_byte_message_info`. Per Guiding Principle 5, the
      `read_size`/`segment_num` ambiguity is resolved with an explicit,
      documented convention rather than a single ambiguous field:
      `ReadSizeOrSegmentNum` models the field as one raw byte with two
      same-bit accessor views (`as_read_size`/`as_segment_num`), since this
      crate has not reconciled which bit(s), if any, would select one
      interpretation over the other. All byte offsets/bit widths beyond the
      three the roadmap states explicitly (the 11-bit `acf_msg_length`,
      11-bit `byte_bus_id`, 4-bit `evt`) are this crate's own working
      interpretation, flagged in `src/acf.rs`'s provenance note for
      reconciliation against the OPEN Alliance TC18 Remote Control Protocol
      Specification's behavior before being relied on for interop. This is
      additive alongside `avtpdu.rs` and does not yet wire either ACF
      message type into an AVTPDU decoder or cut over any caller of
      `src/wire.rs` — that composition and cutover remain later work.

### Addressing

- [x] `stream_id` construction/parsing (sender MAC + locally-assigned
      unique-id suffix). Done (v0.4.0-dev): `src/avtpdu.rs` adds
      `StreamId { sender_mac: [u8; 6], unique_id: u16 }` plus the
      `build_stream_id`/`parse_stream_id` free-function pair it wraps
      (`StreamId::to_u64`/`StreamId::from_u64`, and `From`/`Into`
      conversions both ways), decomposing/composing the opaque 64-bit value
      already carried by `NtscfHeader::stream_id`/`TscfHeader::stream_id`
      into a sender MAC address (upper 48 bits) and a locally-assigned
      unique-id suffix (lower 16 bits), round-tripping across zero/max
      values and never panicking (`parse_stream_id` takes a plain `u64`, so
      there is no truncated-input shape to reject). This is additive:
      `NtscfHeader`/`TscfHeader`'s `stream_id` fields remain plain `u64` —
      no existing caller is cut over — and interop between `StreamId` and
      both header types' opaque field is covered by round-trip tests. Per
      Guiding Principle 5, the sender-MAC-high/unique-id-low bit-layout
      split is this crate's own working interpretation of the common IEEE
      1722 AVTP stream_id convention, not a transcription of or confirmed
      match against the OPEN Alliance TC18 Remote Control Protocol
      Specification's own construction rule, and is flagged in the module's
      provenance note for reconciliation before being relied on for
      interop. The other two Addressing bullets below —
      `(stream_id, byte_bus_id)` endpoint lookup and the echo-back rule —
      remain separate items; both are now also done (see below).
- [x] `(stream_id, byte_bus_id)` → endpoint lookup, with the stream-relative
      (not global) uniqueness of `byte_bus_id` modeled explicitly. Done
      (v0.4.0-dev): new module `src/addressing.rs` adds `EndpointTable`, a
      lookup keyed on the `(stream_id, byte_bus_id)` pair, plus
      `EndpointTable::register`/`EndpointTable::lookup` and a placeholder
      `EndpointId` handle standing in for the concrete endpoint
      representation later milestones (Milestone 4 onward) will introduce.
      The stream-relative uniqueness rule is modeled structurally, not just
      documented: `EndpointTable` is internally a map from `StreamId` to a
      per-stream map from `byte_bus_id` to `EndpointId`, so two streams each
      get their own independent `byte_bus_id` keyspace and the same
      `byte_bus_id` value under two different streams can never collide.
      `register` rejects re-registering an already-registered pair with the
      new `RcpError::EndpointAlreadyRegistered` sentinel (without
      overwriting the existing entry) rather than silently allowing an
      ambiguous double-registration, and rejects a `byte_bus_id` wider than
      the 11-bit field width already enforced by
      `acf::encode_byte_message_info`. Covered by round-trip, cross-stream
      non-collision, duplicate-registration-rejection, and
      never-panics-on-arbitrary-input tests. This is additive: it consumes
      `StreamId` (`src/avtpdu.rs`) and `byte_bus_id` (`src/acf.rs`) as
      inputs only, does not change either type's shape, and is not yet
      wired into any AVTPDU/ACF decoder or existing caller. The echo-back
      rule below is a separate item, now also done (see below).
- [x] Echo-back rule: a response/ack must carry the same `byte_bus_id` it
      was received under. Done (v0.4.0-dev): `src/acf.rs` adds
      `build_response_info`/`verify_echo_back`, a construction/validation
      pair operating on `ByteMessageInfo` alone rather than on
      `src/addressing.rs`'s `StreamId`/`EndpointTable` machinery, since the
      rule itself is stated purely in terms of `byte_bus_id`.
      `build_response_info` takes a caller-populated response
      `ByteMessageInfo` plus the `request` it answers, and returns the
      response with `byte_bus_id` copied from `request` and `rsp` forced
      `true`, leaving every other field as the caller set it.
      `verify_echo_back` checks an already-built response against its
      `request` and rejects a mismatched `byte_bus_id` with the new
      `RcpError::EchoBackMismatch` sentinel, without requiring
      `response.rsp` to be set (a separate concern from the byte_bus_id
      rule itself) and without inspecting field widths (already
      `encode_byte_message_info`'s job at encode time). Both functions
      operate on already-decoded `ByteMessageInfo` values only, so neither
      can panic on malformed input the way a byte-slice decoder could.
      This Milestone 1 item is scoped to the byte_bus_id-echoing rule
      itself, not to *when* in a request/response lifecycle it gets
      enforced — encode time, decode time, or purely as an
      application-level helper are all left open, and which one is correct
      is this crate's own interpretation per Guiding Principle 5, since the
      specification's own text is cited by section number only. Covered by
      round-trip (`build_response_info` output passes
      `verify_echo_back`), match/mismatch, rsp-flag-independence, and
      never-panics-on-arbitrary-field-value tests. This is additive: like
      every other Milestone 1 entry, neither function is wired into any
      AVTPDU/ACF decoder, `src/addressing.rs`'s `EndpointTable`, or any
      existing caller. This closes out the "Addressing" subsection.

### Timestamp Semantics

- [x] `avtp_timestamp` (32-bit, TSCF-only) vs `message_timestamp` (64-bit,
      ACF_GBB-only) — distinct widths, distinct rollover periods. Done
      (v0.4.0-dev): new module `src/timestamp.rs` adds `AvtpTimestamp`
      (wrapping the raw `u32`) and `MessageTimestamp` (wrapping the raw
      `u64`) as two distinct newtypes with no shared trait or cross-type
      comparison between them, so the two can never be confused with one
      another. Each carries its own `ROLLOVER_PERIOD` constant (2^32 vs
      2^64 raw ticks) and its own wraparound-aware `wrapping_delta`/
      `is_after` comparison pair, covering the operational half of "distinct
      rollover periods" — comparing two timestamps correctly across a
      rollover — not just the field-width half. Covered by round-trip,
      non-wraparound-delta, wraparound-boundary, and exactly-half-period
      tests for both types.
- [x] Invalid/uncertain timestamp fallback: an all-zero timestamp region
      folds down to "treat as untimed," matching the spec's stated
      leniency. Done (v0.4.0-dev): `AvtpTimestamp::semantics`/`is_untimed`
      and `MessageTimestamp::semantics`/`is_untimed` fold an exact all-zero
      raw value down to the new `TimestampMeaning::Untimed`, and every
      other raw value (including the widest representable one) to
      `TimestampMeaning::Timed`. Per Guiding Principle 5, both the exact
      fallback trigger condition (all-zero only, not a wider sentinel band)
      and the two rollover-period lengths (each field's full bit width, in
      raw ticks — not a real-world time unit) are flagged in the module's
      provenance note as this crate's own working interpretation, since the
      roadmap states them by rule only and the underlying OPEN Alliance
      TC18 Remote Control Protocol Specification v0.5.1_RC is cited by
      section number only, pending reconciliation before real interop.
      Covered by round-trip, zero/non-zero, and never-panics-on-
      arbitrary-input tests. This is additive, matching every other
      Milestone 1 entry: `TscfHeader::avtp_timestamp` and
      `AcfGbbMessage::message_timestamp` keep their raw `u32`/`u64` field
      types unchanged — `AvtpTimestamp`/`MessageTimestamp` consume those
      raw values as conversion inputs/outputs only and are not wired into
      either type's encode/decode path or any other existing caller. This
      closes out the "Timestamp Semantics" subsection.

### Validation

- [x] Decode functions never panic on arbitrary/truncated input (carry
      forward the existing fuzz-style discipline from `wire.rs`). Done
      (v0.4.0-dev): new `fuzz/fuzz_targets/fuzz_avtpdu_acf_decode.rs`
      libFuzzer target, registered in `fuzz/Cargo.toml`, mirroring
      `fuzz_wire_decode.rs`'s existing structure and CI wiring. It feeds the
      same arbitrary/truncated `data: &[u8]` slice through every
      byte-slice-accepting decode function this milestone added:
      `avtpdu::decode_ntscf_header`, `avtpdu::decode_tscf_header`,
      `avtpdu::select_header_variant` (under both `TimeSyncCapability`
      outcomes), `acf::decode_byte_message_info`, `acf::decode_acf_abb`, and
      `acf::decode_acf_gbb`, plus `avtpdu::StreamId::from_u64` for
      belt-and-suspenders coverage even though it takes a plain `u64` rather
      than a byte slice and so has no truncated-input shape to panic on.
      Every call is `let _ = ...;`, matching `fuzz_wire_decode.rs`'s
      discipline exactly: the only failure mode under test is a panic inside
      the crate's own decode logic. `.github/workflows/ci.yml`'s existing
      `fuzz` job runs it for a 30s smoke test alongside `fuzz_wire_decode`,
      same as the pre-existing target. This is distinct from (and in
      addition to) the unit-test-level "never panics on arbitrary input"
      coverage each decoder's own `Done` note above already claims — this
      item specifically closes the gap between that unit-test claim and a
      genuine fuzz harness. This closes out the "Validation" subsection and
      Milestone 1 as a whole.

Success Criteria:
A conformant AVTPDU can be built, parsed, and round-tripped for both header
variants and both ACF message types, matching the layouts described in this
crate's spec extraction §2.

---

## Milestone 2 — RC Server Lifecycle & Register-Map Model `v0.5.0`

Goal:
Model the RC Server as a first-class entity with the mandatory three-state
lifecycle machine and the register-map configuration model, replacing the
`Zone`/`Controller`/`Registry` abstraction entirely.

### Lifecycle State Machine

- [x] `HW_UNCONFIGURED` (`0x00`) / `HW_CONFIGURED` (`0x55`) /
      `RCP_CONFIGURED` (`0xAA`) states with correct per-state register
      reachability. Done (v0.5.0-dev): new `src/lifecycle.rs` adds
      `RcServerState`, a `#[repr(u8)]` enum with exactly these three
      variants and encodings, plus never-panicking `to_u8`/`from_u8`
      round-trip helpers (invalid bytes reject via `RcpError::Other`,
      mirroring `avtpdu::select_header_variant`'s unrecognized-subtype
      handling). Register reachability is modeled as an explicit, queryable
      rule — `RegisterCategory` (`General`/`HwConfig`/`RcpConfig`, an
      abstract placeholder standing in for the not-yet-built Register Map)
      and `is_register_reachable`/`check_register_reachable`, which gate
      `RcpConfig` unreachable while `HW_UNCONFIGURED` and leave `General`/
      `HwConfig` reachable in every state. The module's doc comment flags,
      per Guiding Principle 5, that this reachability rule and the
      `HW_UNCONFIGURED`-default are this crate's own working interpretation
      (inferred from the `HW_CFG_INCONSISTENT`/`RCP_CFG_INCONSISTENT` guard
      naming and Milestone 3's discovery needs, respectively), and that no
      `§3.x` section number is yet recorded anywhere in this crate for the
      lifecycle state machine itself, unlike the Register Map subsection's
      already-cited `§3.6`–`§3.11`. Deliberately does not yet implement the
      other three "Lifecycle State Machine" items (transition guards,
      `W`/`W*` register-locking, or the `HW_CONFIGURED`→`HW_UNCONFIGURED`
      demotion path) or anything from the "EP0"/"Register Map" subsections
      — see the module doc comment for the explicit boundary. New
      `RcpError::RegisterUnreachable` sentinel added, following the
      existing "Wire / E2E errors" grouping convention, as a provisional
      name pending this milestone's later "Error Model" item.
- [x] Transition guard checks: `HW_CFG_INCONSISTENT` (HW→HW_CONFIGURED),
      `RCP_CFG_INCONSISTENT` (HW_CONFIGURED→RCP_CONFIGURED). Done
      (v0.5.0-dev): `src/lifecycle.rs` adds `RcServerState::try_transition`
      (a `self`-consuming, never-panicking method returning
      `Result<RcServerState, RcpError>`) plus a free-function counterpart
      `is_transition_defined`, mirroring the existing
      `is_register_reachable`/`check_register_reachable` peek-vs-validate
      pairing. Only the two forward transitions the two guards are named
      for — `HW_UNCONFIGURED`→`HW_CONFIGURED` and
      `HW_CONFIGURED`→`RCP_CONFIGURED` — are structurally defined; every
      other `(from, to)` pair (identity, backward, skip, and the
      `HW_CONFIGURED`→`HW_UNCONFIGURED` demotion path from the next
      checklist item) is rejected with the new
      `RcpError::InvalidLifecycleTransition` sentinel without the guard
      ever being consulted. Since this crate has no register map yet to
      derive a real `HW_CFG_INCONSISTENT`/`RCP_CFG_INCONSISTENT` pass/fail
      criterion from, `try_transition` takes the consistency check as a
      caller-supplied `is_consistent: impl FnOnce() -> bool` closure
      (mirroring `formal::Invariant`'s predicate shape) rather than
      inventing one; on failure it returns one of two new guard-named
      sentinels, `RcpError::HwCfgInconsistent` /
      `RcpError::RcpCfgInconsistent`, both provisional pending this
      milestone's later "Error Model" item, same as
      `RcpError::RegisterUnreachable` before them. Deliberately does not
      implement register-locking-by-state or the demotion path — see the
      module doc comment for the explicit boundary.
- [x] Register-locking-by-state, including the `W` vs `W*` (permanently
      locked once `RCP_CONFIGURED`) distinction. Done (v0.5.0-dev):
      `src/lifecycle.rs` adds a `LockPolicy` enum (`W`/`WStar`) plus a
      `lock_policy(RegisterCategory) -> Option<LockPolicy>` mapping, and
      `is_register_writable`/`check_register_writable` free functions
      mirroring the existing `is_register_reachable`/`check_register_reachable`
      peek-vs-validate pairing. This is a new axis layered on top of, not a
      replacement for, reachability: a category must first be reachable
      before write-locking is even considered. `RegisterCategory::General`
      maps to `None` (never writable through this module),
      `RegisterCategory::HwConfig` maps to `W*` (writable while reachable,
      permanently locked the moment `RCP_CONFIGURED` is reached), and
      `RegisterCategory::RcpConfig` maps to `W` (writable whenever
      reachable, including while `RCP_CONFIGURED`, with no permanent lock
      this module adds). New `RcpError::RegisterLocked` sentinel added,
      following the existing "Wire / E2E errors" grouping convention, as a
      provisional name pending this milestone's later "Error Model" item.
      Since no concrete Register Map exists yet, the per-category `W`/`W*`
      assignment is this crate's own working interpretation (like
      `RegisterCategory` itself was for the reachability item) — see the
      module doc comment's provenance note for the reasoning and its
      Guiding-Principle-5 flag. Deliberately does not implement the demotion
      path — see the module doc comment for the explicit boundary.
- [x] Demotion path from `HW_CONFIGURED` back to `HW_UNCONFIGURED`. Done
      (v0.5.0-dev): `src/lifecycle.rs`'s `RcServerState::try_transition`
      adds a `HwConfigured -> HwUnconfigured` match arm (and
      `is_transition_defined` a matching case), completing the "Lifecycle
      State Machine" subsection's four checklist items. Two judgment calls,
      flagged per Guiding Principle 5 in the module doc comment's
      Provenance note rather than silently assumed: (1) the demotion is
      modeled as **unconditional** — `is_consistent` is accepted by
      `try_transition`'s signature but never invoked for this pair, since
      the roadmap names no `..._INCONSISTENT`-style guard for it and there
      is no newly-admitted configuration for a guard to plausibly validate
      against (demoting discards configuration rather than accepting new
      configuration); (2) only the single `HW_CONFIGURED` ->
      `HW_UNCONFIGURED` hop this bullet literally names is implemented —
      `RCP_CONFIGURED` -> `HW_CONFIGURED` and `RCP_CONFIGURED` ->
      `HW_UNCONFIGURED` remain undefined and continue to reject with
      `RcpError::InvalidLifecycleTransition`, left as an explicit open
      question for a later item rather than folded in here. A consequence
      of that second call: since `LockPolicy::WStar`'s permanent lock only
      ever engages at `RCP_CONFIGURED`, and no transition this crate
      implements moves a server *out of* `RCP_CONFIGURED`, this item's
      narrowly-scoped hop does not actually unlock a `RCP_CONFIGURED`-locked
      `HwConfig` register — that remains a still-unbuilt, separate concern.
      Covered by new round-trip/never-consults-guard/never-panics tests in
      `src/lifecycle.rs`'s test module, plus new `REQ-LIFE-012` in
      `.fusa-reqs.json` (and updated text for `REQ-LIFE-006`/`REQ-LIFE-008`,
      which previously described only the two forward transitions). This
      closes out the "Lifecycle State Machine" subsection.

### EP0 (RC-Server-as-Endpoint)

- [x] Whole-register-map read/write addressed through EP0. Done
      (v0.5.0-dev): new `src/ep0.rs` adds `EP0_BYTE_BUS_ID` (`0`) /
      `is_ep0_address` naming the reserved address explicitly, and
      `RequestRoute`/`route_byte_bus_id` making the routing consequence
      structural — a `byte_bus_id` of `0` decides `RequestRoute::Ep0` from
      the `byte_bus_id` value alone, without ever taking or consulting a
      `crate::addressing::EndpointTable`, so a request addressed to EP0 can
      never be resolved through that table's per-stream device-endpoint
      keyspace. The read/write path itself, `check_ep0_access`, composes
      with (rather than duplicates) the "Lifecycle State Machine"
      subsection's already-implemented gates: `Ep0AccessKind`/`access_kind`
      derives a read/write direction from `acf::ByteMessageInfo::op`, a read
      is checked against `lifecycle::check_register_reachable`, and a write
      is additionally checked against `lifecycle::check_register_writable`
      — both at `RegisterCategory` granularity, since the concrete Register
      Map subsection (register addresses, field layout) remains later work
      this item does not anticipate. No new `RcpError` variant was needed:
      `check_ep0_access` surfaces the same `RegisterUnreachable`/
      `RegisterLocked` sentinels `lifecycle` already defined. The existing
      echo-back rule (`acf::build_response_info`/`acf::verify_echo_back`)
      needed no EP0-specific counterpart either — both already operate
      purely on `byte_bus_id`, and `0` passes through unchanged, covered by
      a dedicated round-trip test. Per Guiding Principle 5, `access_kind`'s
      `op = false` → read / `op = true` → write convention is flagged in
      the module's provenance note as this crate's own working
      interpretation, since `acf.rs`'s own provenance note documents `op`
      only as "Operation flag" with no direction assigned either way.
      Deliberately does not implement the root-client concept (the next
      checklist item — every caller is currently treated as equally
      privileged) or any concrete register content (the sibling "Register
      Map" subsection). This is additive: like every prior Milestone 1/2
      entry, neither `route_byte_bus_id` nor `check_ep0_access` is wired
      into any decoder, dispatch loop, or existing `EndpointTable` caller,
      and `EndpointTable::register` itself is left unchanged (it still
      structurally permits registering a device endpoint at `byte_bus_id
      0`; a dedicated test demonstrates that this does not affect
      `route_byte_bus_id`'s routing decision either way). New
      `REQ-EP0-001`..`REQ-EP0-006` added to `.fusa-reqs.json`, each with a
      `// fusa:req`/`// fusa:test` pair in `src/ep0.rs`.
- [x] Root-client concept (`svr_root_client_index`): full-server write
      access for exactly one stream, per-endpoint-restricted access for
      everyone else. Done (v0.5.0-dev): `src/ep0.rs` adds `is_root_client`
      and `check_ep0_access_for_stream`, a second, orthogonal
      access-control axis layered on top of (not replacing)
      `check_ep0_access`'s lifecycle-state gating from the first bullet.
      `check_ep0_access_for_stream` leaves EP0 reads identical to
      `check_ep0_access` for every stream regardless of root-client status
      — root-client status gates *writes* only, per this bullet's own
      "full-server **write** access" wording. For a write, the requesting
      stream must equal the designated root client
      (`is_root_client(root_client, stream)`); if it does, the write is
      decided exactly as `check_ep0_access` would (still subject to
      lifecycle-state reachability/locking); if it does not — including
      when no root client is designated at all — the write is rejected
      with new `RcpError::RootClientRequired` without even consulting
      `check_ep0_access`. Since the concrete Register Map subsection that
      would define a real `svr_root_client_index` field is still unbuilt,
      the root client is represented as a plain
      `Option<avtpdu::StreamId>` caller-supplied value rather than a
      dedicated register type — this crate's own working interpretation,
      flagged per Guiding Principle 5 in the module doc comment's
      Provenance note, along with the read/write scoping call and the
      "no root client designated rejects every write" default. This is
      additive: like the first EP0 bullet, `check_ep0_access_for_stream`
      is not wired into any decoder, dispatch loop, or existing caller,
      and nothing here designates a root client against a real RC Server
      instance — it takes `root_client` as a caller-supplied value.
      New `REQ-EP0-007`..`REQ-EP0-011` added to `.fusa-reqs.json`, each
      with a `// fusa:req`/`// fusa:test` pair in `src/ep0.rs`. This
      closes out the "EP0 (RC-Server-as-Endpoint)" subsection.

### Register Map

- [x] Generic (server-owned) per-EP config block vs. common functional-config
      block vs. per-EP-type functional config — three distinct layers, not
      the old crate's single flat `ep_type`-less model. Done (v0.5.0-dev):
      `src/register_map.rs` adds `EndpointType` (the thirteen `ep_type`
      codes `0x01`-`0x0D` named in Milestones 4 and 7, with
      `to_u8`/`from_u8` and `is_reserved` for `Dac`), `PerEpConfigBlock`
      (the generic per-EP layer, tagged by `ep_type`), `CommonFunctionalConfig`
      (an empty placeholder for the layer shared across every `EndpointType`),
      and `PerEpTypeFunctionalConfig` (an `EndpointType`-tagged placeholder
      for the third, per-type layer), plus `functional_config_matches_ep_type`/
      `check_functional_config_matches_ep_type` as the one cross-layer rule
      the three already have to each other. No concrete field beyond the
      `ep_type` tag is invented — that is this same subsection's next two
      bullets' job. `ConfigLayer`/`register_category` give this crate's own
      flagged, provisional mapping from a taxonomy layer to
      `lifecycle::RegisterCategory`, reconciling the two without asserting
      one replaces the other; see the module doc comment's "Relationship to
      `crate::lifecycle::RegisterCategory`" section for the two different
      confidence levels behind that mapping's branches. `EndpointType`
      deliberately has no EP0 variant — see the module doc comment's
      "Relationship to `crate::ep0`" section for how this crate reconciles
      this subsection's "thirteen" (numeric `ep_type` codes `0x01`-`0x0D`)
      against Milestone 7's own differently-scoped "thirteen defined
      endpoint types (EP0 + Wakeup + eleven device-facing types)" headcount.
      This is additive: like every prior Milestone 1/2 entry, nothing here
      is wired into `crate::ep0`, `crate::lifecycle`, or any other existing
      caller. New `REQ-RMAP-001`..`REQ-RMAP-006` added to `.fusa-reqs.json`,
      each with a `// fusa:req`/`// fusa:test` pair in `src/register_map.rs`.
- [ ] General register-map fields: `svr_oa_tc18_magic_nr`, `svr_version`,
      `svr_vendor_id`, `svr_device_id`, `svr_ep_count`,
      `svr_implemented_options`, and the rest of §3.6's table
- [ ] Config tables: HW pin-mapping (§3.7), request-stream config (§3.8),
      EP-ID/`byte_bus_id` mapping (§3.9 — client-side ordering
      responsibility, no server-side safety net per spec), response/ack
      queue config (§3.10), sequencer-state registers (§3.11)

### Error Model

- [ ] Replace `RcpError`'s variant set with the spec's own error codes:
      `UNSUPPORTED_CMD`, `SEQUENCER_NOT_KNOWN`, `UNAUTHORIZED_ACCESS`,
      `LOCKED_MEM_ACCESS`, `REQUEST_CANCELED`, `REQUEST_NOT_FOUND`,
      `EP_ERROR`, `EP_NOT_FOUND`, `REQ_STORAGE_OVFL`, `REQUEST_REJECTED`,
      `INVALID_PARAMETER`, plus the timing- and CRC-specific codes wired in
      by later milestones

Success Criteria:
An RC Server can be constructed in-memory, walked through all three
lifecycle states with correct guard/rejection behavior, and have its
register map read and written through EP0 exactly as specified.

---

## Milestone 3 — Discovery `v0.6.0`

Goal:
Implement the spec's own discovery mechanism. This replaces `mdns.rs` as
the *mandatory* discovery path (mDNS may continue to exist as a
complementary network-rendezvous helper — see the satellite disposition
table — but it is not a substitute for this).

- [ ] Discovery request/response: broadcastable ACF_ABB read addressed to
      `byte_bus_id 0`, register address 0, answerable in **any** lifecycle
      state
- [ ] Discovery-stream claiming: first-claimant rule, `Discovery_TimeOut`
      (~20 ms default) lapse-and-reopen behavior
- [ ] Multi-client coexistence: other clients may still read via discovery
      while a stream is claimed; only the claimant may configure
- [ ] Client-side discovery cache so re-discovery isn't mandatory on every
      power cycle for already-known topology

Success Criteria:
A client can broadcast-discover a server in any lifecycle state, claim the
discovery stream, and observe that claim correctly lapse and reopen per the
timeout rule.

---

## Milestone 4 — Basic Endpoint Types `v0.7.0`

Goal:
Implement the simplest request/response endpoint types first, proving out
the generic per-endpoint mechanics (`evt` sub-opcode conventions, common
functional config) before tackling bus-protocol endpoints.

- [ ] **GPIO** (`ep_type 0x02`): 4-byte bitmask read/write; the eight
      write-semantics (replace/OR/AND/XOR/add/subtract-with-saturation/
      reconfigure); per-pin change/rising/falling trigger signals
- [ ] **SPI** (`ep_type 0x03`): up to 6 pre-configured channel configs
      selected via `evt[2:0]`; raw PICO/POCI byte transfer; compound-wait's
      4-of-20-byte status truncation rule
- [ ] **I²C** (`ep_type 0x04`): controller-only, raw byte stream including
      address bytes; `i2c_mode` speed presets (flag the enum ambiguity
      between adjacent high-speed rows as unresolved pending errata, per
      this crate's spec-extraction §5.7 — do not silently pick one)
- [ ] **UART** (`ep_type 0x05`): independent TX/RX queues sharing one
      functional-config block; `read_size`-or-`uart_timeout` read
      completion; payload-less-read-only rule (`UNKNOWN_CMD` if violated)
- [ ] **ADC** (`ep_type 0x09`): ≤16-bit resolution; three-level averaging
      model (`adc_sample_interval` → `adc_avg_intervals_per_request` →
      `adc_combine_avg_values`); request-driven sampling only
- [ ] **PWM_OUT / PWM_IN** (`ep_type 0x07`/`0x08`): shared
      period+active-duration pair shape; PWM_IN's `PWM_IN_NO_SIGNAL` timeout
      instead of hanging or returning stale data
- [ ] Generic `evt[2:0]` group conventions common to all of the above
      (Groups A/B/C) and the shared common functional-config fields
      (`ep_enable`, `ep_clear_req_storage`, `ep_req_crc_enable`, etc.)

Success Criteria:
A client can configure, enable, and drive each of these six endpoint types
end-to-end against an in-memory mock RC Server.

---

## Milestone 5 — Conditional Requests & Sequencers `v0.8.0`

Goal:
Implement the full conditional-request taxonomy and the sequencer primitive
that gates it. This is also where the old `prioqueue.rs` decorator's job —
picking which pending request runs next — gets absorbed into the core
per-endpoint scheduler, since the spec defines that ordering natively.

- [ ] Compound / compound-wait (`0x0F`/`0x0B`): sequencer-gated execution
      and wait; `cmp_exec_delay`/`cmpw_exec_delay` timers; "advance sequencer
      only if still in start state" rule
- [ ] Triggered (`0x0E`): trigger-occurrence counting that runs independent
      of endpoint busy/idle state; `trigger_exec_delay`; infinite-repeat
      sentinel (`0xFFFF`)
- [ ] Chained (`0x01`): `cs`-bit abort-on-predecessor-error semantics;
      `CHAIN_ABORTED`/`CHAIN_ERROR`
- [ ] Timed (`0x0A`): presentation-time execution as an alternative to a
      TSCF header
- [ ] Cancellation: clear-all (`0x05`, mandatory), clear-non-safestate
      (`0x06`, optional), clear-single (`0x07` + `clear_transaction_num`,
      optional)
- [ ] Sequencers: persistent 8-bit state registers, power-on default state
      `1`, bounded by `svr_sequencers_max`
- [ ] Execution priority ordering: cancellation > triggered > timed >
      compound > compound-wait > chained > standard, FIFO within a tier
- [ ] Request lifecycle state machine: pending → started → under-execution
      → finalized, with the type-specific sub-behavior at each transition
      (§3.14)
- [ ] Feature-bundle gating: claiming "compound request support" requires
      shipping compound-wait, ≥4 sequencers, *and* clear-non-safestate
      together — not compound message parsing alone

Success Criteria:
All five request kinds and three cancellation kinds execute with correct
priority ordering and lifecycle transitions against the Milestone 4
endpoints.

---

## Milestone 6 — E2E CRC Safe Points & Safety Requests `v0.9.0`

Goal:
Replace the ad-hoc CRC-16/CCITT-FALSE + replay-guard wrapper (`e2e.rs`) with
the spec's real safety mechanism, and wire the watchdog and power-state
concepts into it rather than treating them as unrelated decorators.

- [ ] CRC32 safe-point implementation: poly `0xF4ACFB13`, init/final XOR
      `0xFFFFFFFF`, reflected input and output — a genuinely different
      algorithm from the current CRC-16, not a width change
- [ ] Coverage rule: CRC spans `stream_id` + `avtp_timestamp` (zeroed under
      NTSCF) + the full ACF header + payload; length-field pre-adjustment
      (+1 quadlet / +4 octets) before computing it
- [ ] Fragmentation interaction: only the *last* fragment of a multi-segment
      message carries the CRC, computed across the combined payload
- [ ] Safety-request MSB-tagging: `0x8F`/`0x8B`/`0x8E` variants; on watchdog
      overflow, normal-priority requests are purged while safety-tagged
      requests remain queued and become the mechanism that drives the
      endpoint through its safe state
- [ ] Per-stream safety config: `rx_enforce_e2e`, `rx_wd_enable` +
      `rx_wd_timeout_interval` + `rx_wd_safestate_enable` (replacing the
      old periodic-WATCHDOG-command model in `watchdog.rs` with the spec's
      real per-stream liveness-reset-on-every-request design),
      `rx_safety_measure` (hi-Z vs. sequencer-driven safe sequence),
      `rx_safestate_sequencer`/`rx_safe_sequencer_state`,
      `rx_ovrflw_safestate_enable`, `rx_enforce_seq`/`rx_seq_safestate_enable`
- [ ] `CRC_ERROR` error path
- [ ] Real power-mode model backing the safe-state work: Normal / StandBy /
      Sleep / Unpowered, cold-start vs. hot-start, and the
      hot-start-from-Sleep WakeUp-message handshake (replacing the ad-hoc
      Active/Sleep/Standby model in `powerstate.rs`) — implemented here
      because entry/exit gating shares the same "all endpoints idle, no
      pending response" conditions as safe-state entry

Success Criteria:
A stream configured for CRC-secured "safe mode" rejects tampered requests
with `CRC_ERROR`, and a simulated watchdog overflow correctly purges
normal-priority requests while safety-tagged requests drive the endpoint to
its configured safe state.

---

## Milestone 7 — Remaining Endpoint Types `v0.10.0`

Goal:
Complete the endpoint-type roster: the bus-protocol and power-management
types deferred out of Milestone 4.

- [ ] **LIN commander** (`ep_type 0x06`): raw byte pass-through only — the
      spec defines no PID/checksum/schedule-table smarts at the protocol
      level. Explicitly validate this against `linbr.rs`'s current ad-hoc
      PID-generation assumptions before reusing any of its logic; expect the
      new implementation to push that responsibility to the client
- [ ] **CAN controller** (`ep_type 0x0B`): Classical/FD/XL `FrameFormat`
      selection (CBFF/CEFF/FBFF/FEFF/XL-classical/XL-new); CAN XL's 6-byte
      sub-header plus up to 2048-byte payload (needs fragmentation — see
      Milestone 8); data frames only, no remote-frame support; note the
      spec's own CAN trigger-signal table is unpopulated in this revision —
      that's a spec gap to track, not an implementation omission
- [ ] **ISELED** (`ep_type 0x0C`): native 4b/5b-encoded daisy-chain framing;
      optional native ISELED CRC, distinct from and additional to the
      RCP-level CRC32; multi-device response aggregation
      (`iseled_collect_resp`)
- [ ] **MDIO** (`ep_type 0x0D`): Clause-22/45 addressing modes
      (`mdio_mode` 2-bit selector); minimal functional config (no
      clock-divider or mode-select fields beyond the universal common
      block). Note: MDIO is fully normative in the register map's `ep_type`
      enumeration despite being absent from the spec's own informative
      "ten interfaces" scope statement — build it anyway
- [ ] **Wakeup control** (`ep_type 0x01`): fixed `SleepCMD` (`0xA5`) request
      distinct from the generic request taxonomy; wake-source pin
      monitoring; wired into the Normal/StandBy/Sleep/Unpowered model from
      Milestone 6
- [ ] **DAC** (`ep_type 0x0A`): explicit decision — **treated as reserved
      and out of scope for this cycle.** The type code and a `DAC_OUT` pin
      signal exist in the register-map enumeration, but no functional-config
      chapter or request semantics are defined anywhere in the spec. Track
      as a follow-up pending an OPEN Alliance clarification or later spec
      revision; do not guess at a register layout for it

Success Criteria:
All thirteen defined endpoint types (EP0 + Wakeup + eleven device-facing
types) are implemented or explicitly deferred (DAC only), matching the
register map's own `ep_type` enumeration.

---

## Milestone 8 — Fragmentation Go/No-Go `v0.11.0`

Goal:
Make, execute, and document an explicit decision on multi-AVTPDU
fragmentation support before v1.0 — the spec itself treats this as optional
for "RCP version 1.0," so silence on it is not acceptable.

- [ ] Decision point: evaluate whether UART RX-FIFO sizing, CAN XL's
      up-to-2054-byte payloads, and full-register-map discovery reads can
      ship as an accepted single-AVTPDU-only limitation for v1.0, or whether
      multi-AVTPDU reassembly must be built now
- [ ] **If go:** implement `ms`/`segment_num` reconstruction bounded by
      `rx_stream_max_request_size`, and re-verify the Milestone 6
      last-fragment-carries-the-CRC interaction against it
- [ ] **If no-go:** document the single-AVTPDU limitation explicitly in the
      crate's public docs and in `rust-rcp capabilities`' output, matching
      the spec's own allowance for omitting this feature

Success Criteria:
The roadmap records a written go/no-go decision — not a silent omission —
and the crate's conformance-facing docs and capabilities output accurately
reflect whichever was chosen.

---

## Milestone 9 — Satellite Package Migration `v0.12.0`

Goal:
Execute the REPLACE / ADAPT / DEPRECATE / KEEP-AS-IS dispositions from the
package-by-package audit below, now that the core protocol (Milestones 1–8)
exists to migrate them onto.

- [ ] All **REPLACE**-disposition packages rebuilt against the new core
- [ ] All **ADAPT**-disposition packages retargeted to whatever new
      endpoint/RC-Server trait surface replaces `Controller`/`Registry`
- [ ] All **DEPRECATE**-disposition packages removed, with a migration note
      in the changelog explaining the replacement path (generally: use
      RELAY's `crossbar` router instead of an in-crate protocol bridge)
- [ ] All **KEEP-AS-IS** packages given a regression pass to confirm they
      are genuinely unaffected

Success Criteria:
No module under `src/` still references the old `Zone`/`Command`/
`Response`/`Controller`/`Registry` API; `cargo build` and
`cargo clippy -- -D warnings` are clean; CI is green.

---

## Milestone 10 — v1.0 Production Release `v1.0.0`

Goal:
Ship a stable, TC18-conformant rust-RCP.

- [ ] Public API stability guarantees (semver) for the new core
- [ ] Full FuSa artifact re-basing: HARA, SAFETY_PLAN.md, and tara.json
      rewritten against the new architecture (the old versions describe
      hazards and threats specific to the replaced protocol)
- [ ] RELAY spec `Adapt()`/`to_message()`/`from_message()` rebuilt against
      the new endpoint-addressed `Message` shape (no more zone-name-as-`id`
      mapping)
- [ ] CLI (`rust-rcp`) command surface updated: discovery, register
      read/write, per-endpoint drive commands, replacing the old
      `send`/`zones`/`status --zone` shape
- [ ] Conformance test vectors / interop verification against at least one
      sibling x-RCP implementation once it has also uplifted, or
      self-referential wire-format golden vectors if none is ready yet

Success Criteria:
rust-RCP's wire format, RC Server model, and endpoint set are demonstrably
conformant with the OPEN Alliance TC18 Remote Control Protocol
Specification's described behavior, and the crate publishes as `v1.0.0`.

---

## Satellite Package Disposition

Every package currently listed in this crate's module index gets one of
four calls: **REPLACE** (rebuilt to be spec-conformant), **ADAPT** (retarget
to the new core without a full rewrite), **DEPRECATE** (no place in the new
model), or **KEEP AS-IS** (genuinely orthogonal, unaffected).

| Package | Call | Reason |
|---|---|---|
| `wire` | REPLACE | Becomes IEEE 1722 AVTPDU / ACF_ABB / ACF_GBB framing (Milestone 1) — the current 16-byte header has no equivalent in the spec |
| `e2e` | REPLACE | Becomes the spec's real CRC32 (poly `0xF4ACFB13`) safe-point mechanism (Milestone 6) — current CRC-16 + replay guard is a different algorithm serving a different model |
| `mock` | REPLACE | Must model an RC Server + Endpoints for testing, not a `Zone`-keyed controller |
| `config` | REPLACE | Must represent the register-map/lifecycle configuration model, not `RcpConfig{controllers, watchdog, rate_limit}` |
| `capi` | REPLACE | C FFI types are 1:1 tied to the old `Command`/`Zone` shapes, which disappear |
| `watchdog` | REPLACE | Ad-hoc periodic WATCHDOG-command dispatcher replaced by the spec's real per-stream watchdog (`rx_wd_*`, liveness reset on every request, safe-state entry) — see Milestone 6 |
| `powerstate` | REPLACE | Ad-hoc Active/Sleep/Standby via `CommandType::SLEEP`/`WAKE` replaced by the spec's real Normal/StandBy/Sleep/Unpowered model with cold/hot start — see Milestone 6/7 |
| `canbr` | REPLACE | Becomes the CAN controller endpoint type (Milestone 7); CAN is also a first-class endpoint in the new model, not a bridge underneath Zone/Command framing |
| `linbr` | REPLACE | Becomes the LIN commander endpoint type (Milestone 7); the spec's raw-byte-passthrough philosophy contradicts the current ad-hoc PID-generation scheme, so this isn't a mechanical port |
| `udp` | REPLACE | IEEE1722-over-UDP is spec-legal as a transport (§2.1), but every framing call must be rebuilt against Milestone 1's AVTPDU encode/decode instead of `wire::encode_command` |
| `prioqueue` | DEPRECATE | Critical/High/Normal decorator is superseded by the spec's own native per-endpoint execution-priority ordering (cancellation > triggered > timed > compound > compound-wait > chained > standard), built into the Milestone 5 core scheduler rather than a bolt-on wrapper |
| `zonegroup` | DEPRECATE | Zone-broadcast has no direct equivalent once `Zone` disappears; multi-endpoint fan-out isn't spec-defined and can be rebuilt later as a generic client-side helper if a real need emerges |
| `tsn` | DEPRECATE | Current implementation hacks a priority byte into the payload, which is incompatible with real AVTPDU framing; legitimate TSN traffic-class handling (VLAN PCP tagging) belongs at the transport/socket layer and isn't defined by the RCP spec itself |
| `firmware` | DEPRECATE | Chunked-SET/GET OTA sequencer has no home in the thirteen defined endpoint types; not part of TC18 scope — could return later as an OEM-layer concern built atop a real endpoint, not as core protocol |
| `someip` | DEPRECATE | SOME/IP is not a spec-defined RCP transport. Ecosystem precedent (go-DDS removed its own MQTT and domain bridges in v0.52.0) is to handle cross-protocol bridging centrally via RELAY's `crossbar` router rather than per-repo bridges |
| `mqttbr` | DEPRECATE | Same `crossbar`-router precedent as `someip` |
| `ddsbr` | DEPRECATE | Same `crossbar`-router precedent as `someip` |
| `grpcbridge` | DEPRECATE | Same `crossbar`-router precedent as `someip` |
| `restbridge` | DEPRECATE | Same `crossbar`-router precedent as `someip` |
| `udsbr` | DEPRECATE | UDS/ISO 14229 is a distinct vehicle-diagnostics protocol, not a spec-defined RCP transport; same `crossbar`-router precedent applies |
| `doipbr` | DEPRECATE | DoIP/ISO 13400-2 is likewise not a spec-defined RCP transport; same `crossbar`-router precedent applies |
| `ratelimit` | ADAPT | Generic token-bucket decorator; retarget to whatever new endpoint-request dispatch trait replaces `Controller` |
| `sim` | ADAPT | Deterministic test-double concept persists; rebuild against the new endpoint trait |
| `deadline` | ADAPT | Generic client-side call-timeout decorator; retarget to the new API, distinct from the spec's own presentation-timestamp semantics |
| `faultinject` | ADAPT | Generic fault-injection decorator for safety test campaigns; retarget to the new trait |
| `loan` | ADAPT | Zero-copy buffer-pool concept remains useful for endpoint payload buffers (SPI/UART/CAN); retarget to the new API |
| `proxy` | ADAPT | Fully generic hot-swap decorator; trivially retargeted to any new base trait |
| `redundancy` | ADAPT | Generic 1-of-2 failover decorator; retarget to the new trait once defined |
| `observe` | ADAPT | Generic metrics/latency decorator, entirely protocol-agnostic |
| `authz` | ADAPT | Generic ACL decorator; retarget its key space from `CommandType` to endpoint-type/request-type |
| `record` | ADAPT | Generic audit-log decorator, protocol-agnostic |
| `federation` | ADAPT | Multi-vehicle routing-by-name concept can be rebuilt once a discovery-derived server registry exists (Milestone 3) — has a real dependency, so lands after core discovery, not before |
| `admin` | ADAPT | Health-check/graceful-shutdown wrapper over a `Registry`; concept persists once a `Registry`-equivalent (discovered-server set) exists |
| `shmem` | ADAPT | Transport is byte-agnostic; just needs to carry new AVTPDU bytes instead of old wire frames |
| `mdns` | ADAPT | Retained as an optional pre-discovery network-rendezvous helper (find hosts on the LAN); does not replace the mandatory spec discovery mechanism, which is new core work in Milestone 3 |
| `tlstransport` | ADAPT | TLS-wrapping mechanics and mutual-auth posture are transport-layer and survive; only the encode/decode calls need updating to the new wire format |
| `adapt` | ADAPT | The RELAY `Adapt()`/`to_message()`/`from_message()` pattern itself persists; the mapping needs to be rebuilt against the new endpoint-addressed `Message` shape (Milestone 10) |
| `dyndata` | KEEP AS-IS | Generic key/value runtime store with no protocol coupling at all |
| `codegen` | KEEP AS-IS | Generic JSON-schema→Rust-struct tool, orthogonal to wire format; could later be pointed at register-map schemas but needs no change itself |
| `iso21434` | KEEP AS-IS | TARA data types (Feasibility/Impact/RiskLevel) are a generic compliance framework, orthogonal to the protocol underneath |
| `certgap` | KEEP AS-IS | Requirement/test traceability gap-analysis tooling, orthogonal to protocol content |
| `formal` | KEEP AS-IS | Generic runtime-invariant-checking harness, orthogonal to any specific state machine it's pointed at |
| `relay` | KEEP AS-IS | Vendored RELAY-spec-generic types (`Message`, `Node`, `Caller`, error sentinels) — defined by the RELAY spec itself, not by RCP's internals |
| `base64_serde` | KEEP AS-IS | Generic serde helper with no protocol coupling |

**44 packages: 10 REPLACE, 16 ADAPT, 11 DEPRECATE, 7 KEEP AS-IS.**

The crate root (`lib.rs`'s `Zone`/`Command`/`Response`/`Status`/
`Controller`/`Registry` types) and the CLI binary (`bin/rcp.rs`) are not
satellite packages — they *are* the core protocol surface being replaced,
covered by Milestones 1–2 and 10 respectively, and are called out in the
breaking-change notice above rather than in this table.

---

## Compliance Targets

| Standard | Target | Status |
|----------|--------|--------|
| OPEN Alliance TC18 RCP | v0.5.1_RC → v1.0 | Not started — this roadmap |
| ISO 26262:2018 | ASIL-B | Complete for the current (private) protocol; requires full re-basing once the core replacement lands (Milestone 10) |
| IEC 62443-4-2 | SL-2 | Complete for the current (private) protocol; requires re-basing alongside ISO 26262 |
| ISO 21434:2021 | WP.10 threat model | Complete for the current (private) protocol; TARA content is protocol-specific and needs a fresh pass once the attack surface changes |
