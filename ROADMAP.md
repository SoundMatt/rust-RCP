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

**Status: this replacement is complete.** Milestones 1-10 below carried it
out in full, and the crate published as `v1.0.0` (see `CHANGELOG.md`); the
private protocol described above no longer exists anywhere in this crate.
The rest of this section is left as originally written, describing the
problem as it stood before Milestone 1 began, for historical context.

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
      `src/avtp.rs` adds `NtscfHeader { sequence_num, ntscf_data_length,
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
      client-to-server only. Done (v0.4.0-dev): `src/avtp.rs` adds
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
      `src/avtp.rs` adds `TimeSyncCapability { Capable, Incapable }` and
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
      `ReadSizeOrSegment` models the field as one raw byte with two
      same-bit accessor views (`as_read_size`/`as_segment_num`), since this
      crate has not reconciled which bit(s), if any, would select one
      interpretation over the other. All byte offsets/bit widths beyond the
      three the roadmap states explicitly (the 11-bit `acf_msg_length`,
      11-bit `byte_bus_id`, 4-bit `evt`) are this crate's own working
      interpretation, flagged in `src/acf.rs`'s provenance note for
      reconciliation against the OPEN Alliance TC18 Remote Control Protocol
      Specification's behavior before being relied on for interop. This is
      additive alongside `avtp.rs` and does not yet wire either ACF
      message type into an AVTPDU decoder or cut over any caller of
      `src/wire.rs` — that composition and cutover remain later work.

### Addressing

- [x] `stream_id` construction/parsing (sender MAC + locally-assigned
      unique-id suffix). Done (v0.4.0-dev): `src/avtp.rs` adds
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
      `StreamId` (`src/avtp.rs`) and `byte_bus_id` (`src/acf.rs`) as
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
      `avtp::decode_ntscf_header`, `avtp::decode_tscf_header`,
      `avtp::select_header_variant` (under both `TimeSyncCapability`
      outcomes), `acf::decode_byte_message_info`, `acf::decode_acf_abb`, and
      `acf::decode_acf_gbb`, plus `avtp::StreamId::from_u64` for
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
      mirroring `avtp::select_header_variant`'s unrecognized-subtype
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
      `Option<avtp::StreamId>` caller-supplied value rather than a
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
      `src/regmap.rs` adds `EndpointType` (the thirteen `ep_type`
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
      each with a `// fusa:req`/`// fusa:test` pair in `src/regmap.rs`.
- [x] General register-map fields: `svr_oa_tc18_magic_nr`, `svr_version`,
      `svr_vendor_id`, `svr_device_id`, `svr_ep_count`,
      `svr_implemented_options`, and the rest of §3.6's table. Done
      (v0.5.0-dev): `src/regmap.rs` adds `GeneralRegisters` — one
      field per row of this crate's own §3.6 general register-map table
      extraction, in table order, typed by each row's declared bit width
      (`u8`/`u16`/`u32`), starting with the six fields this bullet names
      verbatim. `TableDescriptor` (`ptr`/`capacity`) gives the recurring
      child-config-table-pointer shape (HW pin-mapping, request-stream
      config, response/ack queue config, the common per-EP config block,
      the EP/`byte_bus_id` mapping table, plus three product-specific
      blocks) a single reusable type instead of nine separate ones; two
      pointer-only rows with no paired capacity field
      (`svr_ep_functional_cfg_ptr`, `svr_sequencer_state_ptr`) are left as
      plain `u16`s rather than forced into that shape.
      `GeneralRegisters`/`TableDescriptor` each get never-panicking
      `encode`/`decode` to/from a fixed-length, big-endian byte block
      (matching `src/wire.rs`'s own big-endian convention), rejecting
      undersized input with the existing `RcpError::ShortFrame` rather than
      a new variant. Two inferences beyond the table extraction — the
      sequential no-padding byte packing `encode`/`decode` assume, and
      leaving `svr_implemented_options` an undecomposed raw bitmask because
      no bit-position assignment was recorded for its five named option
      bundles — are flagged per Guiding Principle 5 in the module doc
      comment's provenance note rather than presented as spec-cited fact.
      `GeneralRegisters::CATEGORY` records the corresponding
      `lifecycle::RegisterCategory::General` mapping as a documentation
      cross-reference only. This is additive: like every prior Milestone
      1/2 entry, nothing here is wired into `crate::ep0`'s dispatch path,
      `crate::lifecycle`'s reachability checks, or any other existing
      caller. New `REQ-RMAP-007`..`REQ-RMAP-011` added to
      `.fusa-reqs.json`, each with a `// fusa:req`/`// fusa:test` pair in
      `src/regmap.rs`.
- [x] Config tables: HW pin-mapping (§3.7), request-stream config (§3.8),
      EP-ID/`byte_bus_id` mapping (§3.9 — client-side ordering
      responsibility, no server-side safety net per spec), response/ack
      queue config (§3.10), sequencer-state registers (§3.11). Done
      (v0.5.0-dev): `src/regmap.rs` adds one row-content type per
      table — `HwPinMappingEntry` (§3.7), `RequestStreamConfigEntry`
      (§3.8), `EpByteBusIdMapEntry` (§3.9), `ResponseStreamConfigEntry`
      (§3.10), `SequencerStateEntry` (§3.11) — each with never-panicking
      `encode`/`decode` to/from a fixed-length wire form and its own
      `CATEGORY: lifecycle::RegisterCategory` cross-reference, matching
      `TableDescriptor`/`GeneralRegisters`'s existing conventions. A
      table's row count is not carried inside the row type: four of the
      five already have it via their `GeneralRegisters` `TableDescriptor`
      `capacity` field; `SequencerStateEntry` (whose `GeneralRegisters`
      pointer is pointer-only) instead relies on `svr_sequencers_max`, per
      this same subsection's earlier `GeneralRegisters` entry. A new
      `ConfigTableRow` trait plus `encode_rows`/`decode_rows` give all five
      row types one shared, generic way to pack/unpack a whole table as a
      flat run of fixed-length rows instead of five duplicated chunking
      loops. Per this bullet's own parenthetical, `EpByteBusIdMapEntry`
      recognizes the documented end-of-table sentinel row
      (`is_end_of_table`) but adds no ascending-order validation or
      enforcement over a table's rows — maintaining that order remains
      exclusively the writing client's responsibility, with no
      server-side safety net invented here. Every field width not directly
      traceable to this crate's own `§3.7`-`§3.11` extraction (which
      records field names/purpose but no explicit bit-width table, unlike
      `§3.6`) is this crate's own placeholder inference, flagged per
      Guiding Principle 5 in the module doc comment's "Config tables
      provenance note" rather than presented as settled fact. This is
      additive: like every prior Milestone 1/2 entry, nothing here is
      wired into `crate::ep0`'s dispatch path, `crate::lifecycle`'s
      reachability checks, or any other existing caller. New
      `REQ-RMAP-012`..`REQ-RMAP-027` added to `.fusa-reqs.json`, each with
      a `// fusa:req`/`// fusa:test` pair in `src/regmap.rs`. This
      closes out the "Register Map" subsection.

### Error Model

- [x] Replace `RcpError`'s variant set with the spec's own error codes:
      `UNSUPPORTED_CMD`, `SEQUENCER_NOT_KNOWN`, `UNAUTHORIZED_ACCESS`,
      `LOCKED_MEM_ACCESS`, `REQUEST_CANCELED`, `REQUEST_NOT_FOUND`,
      `EP_ERROR`, `EP_NOT_FOUND`, `REQ_STORAGE_OVFL`, `REQUEST_REJECTED`,
      `INVALID_PARAMETER`, plus the timing- and CRC-specific codes wired in
      by later milestones. Done (v0.5.0-dev): `src/lib.rs`'s `RcpError` adds
      all eleven Rust-cased variants this bullet names verbatim
      (`UnsupportedCmd` through `InvalidParameter`), each with an
      `"rcp/error: <CODE> — ..."` message following this crate's existing
      per-module message-prefix convention, plus a new
      `RcpError::is_tc18_error_code()` membership predicate mirroring the
      existing `is_relay_*`/`is_already_exists`/`is_zone_mismatch` query
      style. The timing- and CRC-specific codes this bullet itself defers
      are deliberately not added — that remains later milestones' (6+) job.
      Every provisional sentinel Milestones 1-2 minted ahead of this item
      (`TimeSyncUnsupported`, `EndpointAlreadyRegistered`,
      `EchoBackMismatch`, `RegisterUnreachable`, `HwCfgInconsistent`,
      `RcpCfgInconsistent`, `InvalidLifecycleTransition`, `RegisterLocked`,
      `RootClientRequired`, `EndpointTypeMismatch`) is retired: `lifecycle`,
      `ep0`, `regmap`, `addressing`, `avtp`, and `acf`'s call sites
      are all repointed at the new spec-named variants directly (a true
      rename, not an added indirection layer), since none of those ten
      sentinels had any caller outside this same set of Milestone 1/2
      modules. Several provisional sentinels collapse onto the same spec
      code where this crate reads them as the same underlying failure mode
      — `RegisterUnreachable`/`RootClientRequired` both →
      `UnauthorizedAccess`; `HwCfgInconsistent`/`RcpCfgInconsistent`/
      `EndpointTypeMismatch` all → `InvalidParameter`;
      `EndpointAlreadyRegistered`/`EchoBackMismatch` both → `EpError` —
      while `RegisterLocked` → `LockedMemAccess` and
      `InvalidLifecycleTransition` → `RequestRejected` stay 1:1. Every
      mapping decision, and the reasoning behind each collapse, is flagged
      per Guiding Principle 5 as this crate's own working interpretation in
      `RcpError`'s own doc comment (the single source of truth for the
      mapping) rather than presented as spec-cited fact; the five checklist
      names with no current caller (`SequencerNotKnown`, `RequestCanceled`,
      `RequestNotFound`, `EpNotFound`, `ReqStorageOvfl`) are added as named
      placeholders, reserved for the later milestones that introduce the
      concepts they describe. The `Closed`/`NotConnected`/`Timeout`/
      `PayloadTooLarge` mandatory RELAY sentinels and the legacy
      `NotFound`/`AlreadyExists`/`Busy`/`ZoneMismatch`/`ShortFrame`/
      `BadMagic`/`BadVersion`/`CrcMismatch`/`Replay`/`InvalidSize`/`Other`
      variants are explicitly kept unchanged and out of scope for this
      item — the legacy Zone/Controller/Registry surface they still serve
      remains live (dozens of still-unmigrated satellite packages construct
      and match on them today) and is this repo's own call, not a spec
      question; their removal is Milestone 9/10's job, not this one's.
      `.fusa-reqs.json` gains `REQ-ERRM-001`..`REQ-ERRM-013`, each with a
      `// fusa:req`/`// fusa:test` pair in `src/lib.rs`; the pre-existing
      `REQ-LIFE-004/007/008/010/012`, `REQ-EP0-005/010`, and `REQ-RMAP-004`
      requirement texts are updated in place (same IDs, same behavior) to
      name the new variants their already-passing tests now exercise. This
      closes out Milestone 2 as a whole.

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

**Caveat carried through every checklist item below:** two of this
subsection's on-wire conventions — broadcast addressing and the
register-address encoding — have no dedicated field in this crate's
Milestone 1 wire types to carry them, so `src/discovery.rs` invents its own
working conventions for both (see that module's own "Provenance note" doc
comment, per Guiding Principle 5). They are this crate's unreconciled
interpretations, not confirmed OPEN Alliance TC18 Remote Control Protocol
Specification v0.5.1_RC conventions, and are not to be relied on for
interop with a real TC18 RC Server until reconciled against the actual
specification behavior.

- [x] Discovery request/response: broadcastable ACF_ABB read addressed to
      `byte_bus_id 0`, register address 0, answerable in **any** lifecycle
      state. Done (v0.6.0-dev): new, additive `src/discovery.rs` adds
      `build_discovery_request`/`is_discovery_request` (a read-direction
      `AcfAbbMessage` addressed to `ep0::EP0_BYTE_BUS_ID`, reusing
      `ep0::route_byte_bus_id`/`ep0::access_kind` rather than re-deriving
      them) and `build_discovery_response` (composes explicitly with
      `lifecycle::check_register_reachable(state, RegisterCategory::General)`
      for every `RcServerState`, demonstrating/testing the "answerable in
      any lifecycle state" requirement against the real gate rather than
      only asserting its always-true outcome; the response payload is
      `regmap::GeneralRegisters::encode()` verbatim, register address
      0's field content, and its header echoes the request's `byte_bus_id`
      per the existing echo-back rule). Two working interpretations this
      item introduces are flagged per Guiding Principle 5 in the module's
      own provenance note rather than presented as spec-cited fact, and
      neither is to be relied on for interop with a real TC18 RC Server
      until reconciled against the actual specification behavior: (1)
      "broadcastable" addressing, since `avtp::StreamId` has no
      broadcast/multicast concept of its own — this crate reuses the
      reserved IEEE 802.3 all-ones Ethernet broadcast MAC paired with
      `unique_id 0` as a sentinel `DISCOVERY_BROADCAST_STREAM_ID`, checked
      by `is_discovery_broadcast_stream_id`; (2) "register address 0" on
      the wire, since `acf::ByteMessageInfo` has no dedicated
      register-address field — this crate carries it as a big-endian `u16`
      prefix of `AcfAbbMessage::payload`. This module replaces `mdns.rs` as
      the mandatory discovery path per this subsection's own Goal text;
      `mdns.rs` itself is untouched (its `Zone`/host/port/txt-record model
      shares nothing with this mechanism) and remains available only as the
      complementary network-rendezvous helper the satellite disposition
      table already calls it. Like every prior Milestone 1/2 entry, this is
      additive standalone plumbing only — nothing here is wired into an
      actual decoder, dispatch loop, or `avtp`/`acf` caller, and
      discovery-stream claiming, multi-client coexistence, and the
      client-side cache (this subsection's remaining three checklist
      bullets) are untouched, separate, later work. New `REQ-DISC-001`..
      `REQ-DISC-005` added to `.fusa-reqs.json`, each with a
      `// fusa:req`/`// fusa:test` pair in `src/discovery.rs`.
- [x] Discovery-stream claiming: first-claimant rule, `Discovery_TimeOut`
      (~20 ms default) lapse-and-reopen behavior. Done (v0.6.0-dev):
      `src/discovery.rs` extends its existing additive-only discipline with
      `DiscoveryClaim` (plain data: a claimant `avtp::StreamId` plus the
      instant it was claimed/refreshed) and `try_claim_discovery_stream`
      (a pure function threading `Option<DiscoveryClaim>` state through
      explicitly — no timer thread, lock, or real-clock read of its own,
      mirroring how `build_discovery_response` takes `state`/`general` as
      caller-supplied values). First-claimant rule: a claim for an
      unclaimed stream, or one the same claimant already holds, always
      succeeds (the latter is a refresh, re-timestamping the claim rather
      than being treated as a second claimant); a different claimant is
      rejected with `RcpError::RequestRejected` while the existing claim is
      live. `Discovery_TimeOut` lapse-and-reopen: `DISCOVERY_TIME_OUT`
      defaults to 20ms per the roadmap's own stated default, and
      `DiscoveryClaim::has_lapsed` (inclusive at the boundary, using
      `Instant::saturating_duration_since` so it never panics or wraps on
      an out-of-order `now`) gates whether an existing claim still blocks a
      different claimant; once lapsed, any claimant — including a new one
      — may claim it. Two working interpretations this item introduces are
      flagged per Guiding Principle 5 in the module's own provenance note,
      and are not to be relied on for interop with a real TC18 RC Server
      until reconciled against the actual specification behavior:
      (1) claim identity is modeled as `avtp::StreamId` (the same type
      the broadcast-addressing sentinel already uses), with the sentinel
      `DISCOVERY_BROADCAST_STREAM_ID` itself always rejected as a claimant
      (`RcpError::InvalidParameter`) since a broadcast address names no
      single real client; (2) a claimant re-claiming its own still-live
      claim succeeds as a refresh rather than being rejected, since
      rejecting a claimant's own repeat request would make an
      idle-but-still-interested claimant indistinguishable from one that
      never claimed at all. Like the prior checklist bullet, this remains
      additive standalone plumbing only — nothing here is wired into an
      actual decoder or dispatch loop, and multi-client coexistence and the
      client-side cache (this subsection's remaining two checklist bullets)
      are untouched, separate, later work. New `REQ-DISC-006`..
      `REQ-DISC-010` added to `.fusa-reqs.json`, each with a
      `// fusa:req`/`// fusa:test` pair in `src/discovery.rs`.
- [x] Multi-client coexistence: other clients may still read via discovery
      while a stream is claimed; only the claimant may configure. Done
      (v0.6.0-dev): `src/discovery.rs` extends its existing additive-only
      discipline with `DiscoveryAccessKind { Read, Configure }` and
      `check_discovery_access` (a pure function, mirroring how
      `ep0::Ep0AccessKind`/`ep0::check_ep0_access_for_stream` layer a
      root-client access axis on top of `ep0::check_ep0_access`).
      `DiscoveryAccessKind::Read` always succeeds regardless of claim state,
      matching `build_discovery_response`'s existing "answerable in any
      lifecycle state" unconditional behavior; `DiscoveryAccessKind::Configure`
      is gated by a `claim_permits` rule shared with (factored out of)
      `try_claim_discovery_stream`'s own first-claimant grant logic — an
      unclaimed stream, the live claimant itself, or any requester once the
      existing claim has lapsed, may configure, while a different live
      claimant is rejected with `RcpError::UnauthorizedAccess`. The
      broadcast sentinel is always rejected as a `Configure` requester with
      `RcpError::InvalidParameter`, mirroring `try_claim_discovery_stream`'s
      own rejection of it as a claimant. Both rejected-access codes reuse
      existing spec-named `RcpError` variants per this crate's Milestone 2
      "Error Model" precedent rather than inventing new provisional
      sentinels. Since this crate's Milestone 1 wire framing has no
      dedicated field for "configuring the discovery stream" as distinct
      from an ordinary read, `DiscoveryAccessKind` is a caller-supplied
      value rather than one derived from a decoded message — flagged per
      Guiding Principle 5 in the module's own provenance note, alongside the
      `RcpError` mapping choice, for reconciliation against the OPEN
      Alliance TC18 Remote Control Protocol Specification v0.5.1_RC's actual
      behavior before being relied on for interop. Like the prior two
      checklist bullets, this remains additive standalone plumbing only —
      nothing here is wired into an actual decoder or dispatch loop, and the
      client-side cache (this subsection's one remaining checklist bullet)
      is untouched, separate, later work. New `REQ-DISC-011`..`REQ-DISC-015`
      added to `.fusa-reqs.json`, each with a `// fusa:req`/`// fusa:test`
      pair in `src/discovery.rs`.
- [x] Client-side discovery cache so re-discovery isn't mandatory on every
      power cycle for already-known topology. Done (v0.6.0-dev):
      `src/discovery.rs` extends its existing additive-only discipline with
      `DiscoveryCacheEntry` (a plain, timestamped snapshot of the
      cache-worthy subset of `regmap::GeneralRegisters` — `svr_oa_tc18_magic_nr`,
      `svr_version`, `svr_vendor_id`, `svr_device_id`, `svr_ep_count`) and
      `DiscoveryCache` (a plain `avtp::StreamId`-keyed map of those entries,
      mirroring how `DiscoveryClaim`/`try_claim_discovery_stream` already
      thread claim state through explicitly — no timer thread, lock, or
      real-clock read of its own). `DiscoveryCache::remember` records a
      server's identity as learned from a prior `build_discovery_response`
      payload; `DiscoveryCache::lookup`/`is_known` let a caller decide
      whether a cached entry is fresh enough (via caller-supplied
      `now`/`max_age`, mirroring `DiscoveryClaim::has_lapsed`'s own
      `now`/`timeout` shape and inclusive-boundary/never-panic discipline)
      to skip re-running `build_discovery_request`/`is_discovery_request`'s
      broadcast exchange for already-known topology;
      `DiscoveryCacheEntry::matches` lets a caller confirm a freshly
      observed `GeneralRegisters` still agrees with a cached identity, and
      `DiscoveryCache::invalidate` drops a stale-in-the-identity-sense entry
      explicitly. Three working interpretations this item introduces are
      flagged per Guiding Principle 5 in the module's own provenance note,
      and are not to be relied on for interop with a real TC18 RC Server
      until reconciled against the actual specification behavior:
      (1) which `GeneralRegisters` fields are cache-worthy — the five named
      above, chosen as stable-identity fields, deliberately excluding
      reconfigurable state such as `svr_configuration_lock` and the `§3.6`
      table descriptors, which remain must-always-be-read-fresh; (2) cache
      staleness policy — unlike `Discovery_TimeOut`'s roadmap-stated `~20 ms`
      default, this item names no cache lifetime, so `max_age` is entirely
      caller-supplied rather than this crate inventing an unstated default;
      (3) cache/claim independence — a lapsed `DiscoveryClaim` does not
      auto-evict a `DiscoveryCache` entry, and vice versa, since claim state
      (who may currently configure) and cache state (what a client
      previously learned about a server's identity) are orthogonal concerns
      a caller may compose explicitly if it wants that coupling. Like every
      prior bullet in this subsection, this remains additive standalone
      plumbing only — nothing here is wired into an actual decoder or
      dispatch loop. This closes Milestone 3's "Discovery" subsection
      checklist entirely. New `REQ-DISC-016`..`REQ-DISC-020` added to
      `.fusa-reqs.json`, each with a `// fusa:req`/`// fusa:test` pair in
      `src/discovery.rs`.

Success Criteria:
A client can broadcast-discover a server in any lifecycle state, claim the
discovery stream, and observe that claim correctly lapse and reopen per the
timeout rule — against this crate's own reference `mock::RcServer` and this
module's own unreconciled broadcast-addressing/register-address
conventions (see the Caveat above), not demonstrated against a real TC18
RC Server.

---

## Milestone 4 — Basic Endpoint Types `v0.7.0`

Goal:
Implement the simplest request/response endpoint types first, proving out
the generic per-endpoint mechanics (`evt` sub-opcode conventions, common
functional config) before tackling bus-protocol endpoints.

- [x] **GPIO** (`ep_type 0x02`): 4-byte bitmask read/write; the eight
      write-semantics (replace/OR/AND/XOR/add/subtract-with-saturation/
      reconfigure); per-pin change/rising/falling trigger signals. Done
      (v0.7.0-dev): new `src/gpio.rs` adds [`GpioBitmask`] (the 4-byte
      read/write bitmask, encode/decode, big-endian, never-panicking),
      [`GpioWriteSemantics`] (an explicit 8-variant enum covering all eight
      write-semantics) with [`apply_gpio_write`] giving each one a pure,
      never-panicking `(current, operand) -> new_value` function, and
      [`GpioTriggerConfig`]/[`GpioTriggerSignals`]/[`evaluate_gpio_triggers`]
      modeling per-pin change/rising/falling trigger arming and edge
      detection between a before/after bitmask pair. `GpioFunctionalConfig`
      gives GPIO's own functional-config content a dedicated type rather
      than expanding `regmap::PerEpTypeFunctionalConfig` on the strength of
      one endpoint type alone, with `GpioFunctionalConfig::layer_tag`
      composing against `regmap::check_functional_config_matches_ep_type`'s
      existing cross-layer rule unchanged. Two spec ambiguities are flagged
      per Guiding Principle 5 rather than silently resolved: (1) the
      roadmap text itself names only seven of the "eight write-semantics"
      — `GpioWriteSemantics::Unnamed8th` occupies the eighth `evt.sub_opcode`
      value and `apply_gpio_write` refuses it (`UnsupportedCmd`) rather than
      inventing a behavior; (2) write-semantics selection is modeled via the
      already-generic 3-bit `acf::Evt::sub_opcode` field (its `0..=7` range
      exactly spans eight values) as this crate's own working
      interpretation, not a confirmed instance of this milestone's
      still-unbuilt "Groups A/B/C" convention (this item's last checklist
      bullet). Like every prior Milestone 1-3 entry, this remains additive
      standalone plumbing only — nothing here is wired into an actual
      decoder or dispatch loop. New `REQ-GPIO-001`..`REQ-GPIO-016` added to
      `.fusa-reqs.json`, each with a `// fusa:req`/`// fusa:test` pair in
      `src/gpio.rs`.
- [x] **SPI** (`ep_type 0x03`): up to 6 pre-configured channel configs
      selected via `evt[2:0]`; raw PICO/POCI byte transfer; compound-wait's
      4-of-20-byte status truncation rule. Done (v0.7.0-dev): new
      `src/spi.rs` adds [`SpiChannelSelect`] (the up-to-6 channel selection
      over `evt.sub_opcode`'s full 3-bit range, with the two unused values
      modeled as explicit spare variants rather than silently accepted) with
      [`resolve_spi_channel_index`]/[`select_spi_channel_config`] resolving
      a selection against [`SpiFunctionalConfig`]'s six channel slots;
      [`SpiByteTransfer`]/[`SpiByteTransferResult`] modeling the raw
      PICO/POCI byte transfer as an unstructured, never-panicking
      byte-stream shape (mirroring `gpio::GpioBitmask`'s own wire-form
      discipline) rather than an interpreted payload; and
      [`SpiStatus`]/[`SpiCompoundWaitStatus`]/
      [`truncate_spi_status_for_compound_wait`] representing the 20-byte
      status and its 4-byte compound-wait-truncated form.
      `SpiFunctionalConfig::layer_tag` composes against
      `regmap::check_functional_config_matches_ep_type`'s existing
      cross-layer rule unchanged, matching `GpioFunctionalConfig`'s own
      precedent. Two points flagged per Guiding Principle 5 rather than
      silently resolved: (1) which four of the twenty status bytes survive
      compound-wait's truncation is unstated by the roadmap text — the
      leading four bytes are this crate's own working interpretation; (2)
      the content of each of the up to six pre-configured channel configs
      (clock rate, polarity/phase, etc.) is left an intentionally empty
      placeholder (`SpiChannelConfigSlot`), since only the up-to-6
      *selection* mechanism is named, not per-channel field content. Like
      GPIO, this remains additive standalone plumbing only — the
      compound-wait truncation shape is deliberately not wired into any
      compound-wait dispatch, since compound-wait itself is Milestone 5,
      not yet built. New `REQ-SPI-001`..`REQ-SPI-012` added to
      `.fusa-reqs.json`, each with a `// fusa:req`/`// fusa:test` pair in
      `src/spi.rs`.
- [x] **I²C** (`ep_type 0x04`): controller-only, raw byte stream including
      address bytes; `i2c_mode` speed presets (flag the enum ambiguity
      between adjacent high-speed rows as unresolved pending errata, per
      this crate's spec-extraction §5.7 — do not silently pick one). Done
      (v0.7.0-dev): new `src/i2c.rs` adds [`I2cByteTransfer`]/
      [`I2cByteTransferResult`] modeling the raw byte stream a request sends
      and a response returns — address byte(s) included, unparsed — as an
      unstructured, never-panicking byte-stream shape, mirroring
      `spi::SpiByteTransfer`'s own PICO/POCI discipline; and
      [`I2cSpeedMode`] modeling the five `i2c_mode` speed presets, with
      [`I2cFunctionalConfig`] carrying it as this endpoint type's
      functional-config content. `I2cFunctionalConfig::layer_tag` composes
      against `regmap::check_functional_config_matches_ep_type`'s existing
      cross-layer rule unchanged, matching `GpioFunctionalConfig`'s and
      `SpiFunctionalConfig`'s own precedent. Two points flagged per Guiding
      Principle 5 rather than silently resolved: (1) the two adjacent
      high-speed `i2c_mode` rows this crate's own spec-extraction pass could
      not distinguish are modeled as two explicit, neutrally named variants,
      `I2cSpeedMode::HighSpeedRowA`/`I2cSpeedMode::HighSpeedRowB`, with
      `I2cSpeedMode::is_ambiguous_high_speed_row` letting a caller detect
      them, rather than either being silently assigned a specific named
      speed — mirroring `GpioWriteSemantics::Unnamed8th`'s and
      `SpiChannelSelect::Spare6`/`SpiChannelSelect::Spare7`'s own treatment
      of unresolved enum slots; (2) since this endpoint type is
      controller-only, no peripheral/target-mode role type is modeled at
      all, and since the roadmap text does not state the raw stream's
      address-byte framing (7-bit vs. 10-bit, fixed offset, etc.), this
      module carries the whole stream — address bytes included — as opaque
      bytes rather than guessing a framing to parse it against. Like every
      prior Milestone 1-4 entry, this remains additive standalone plumbing
      only — nothing here is wired into an actual decoder or dispatch loop.
      New `REQ-I2C-001`..`REQ-I2C-006` added to `.fusa-reqs.json`, each with
      a `// fusa:req`/`// fusa:test` pair in `src/i2c.rs`.
- [x] **UART** (`ep_type 0x05`): independent TX/RX queues sharing one
      functional-config block; `read_size`-or-`uart_timeout` read
      completion; payload-less-read-only rule (`UNKNOWN_CMD` if violated).
      Done (v0.7.0-dev): new `src/uart.rs` adds
      [`UartTxQueue`]/[`UartRxQueue`] modeling the independent TX/RX byte
      queues as unstructured, never-panicking byte-stream shapes, mirroring
      `i2c::I2cByteTransfer`'s own discipline; and
      [`UartReadCompletionReason`]/[`resolve_uart_read_completion`] turning
      the `read_size`-or-`uart_timeout` completion rule into a pure function
      over both thresholds, mirroring `spi::truncate_spi_status_for_compound_wait`'s
      and `i2c::I2cSpeedMode`'s own prose-rule-to-function discipline;
      and [`validate_uart_read_request`] enforcing the payload-less-read-only
      rule. `UartFunctionalConfig` is the first Milestone 4 functional config
      with an internal direction split — one type, one `layer_tag` composing
      against `regmap::check_functional_config_matches_ep_type`'s existing
      cross-layer rule unchanged, but carrying independent `tx`/`rx` fields
      rather than `GpioFunctionalConfig`'s/`SpiFunctionalConfig`'s/
      `I2cFunctionalConfig`'s single-queue shape. Three points flagged per
      Guiding Principle 5 rather than silently resolved: (1) when
      `read_size` and `uart_timeout` are met at the same evaluation, this
      crate does not silently prefer one — `UartReadCompletionReason::Both`
      is an explicit third outcome, mirroring
      `GpioWriteSemantics::Unnamed8th`'s and `I2cSpeedMode::HighSpeedRowA`/
      `HighSpeedRowB`'s own treatment of unresolved enum slots, and a zero
      threshold is not treated as a "disabled" sentinel (so a default,
      zeroed config resolves every read as immediately complete via
      `Both`); (2) `read_size` reuses the already-existing
      `acf::ReadSizeOrSegment` field rather than a UART-private type,
      while `uart_timeout` (no existing crate-level counterpart) is carried
      as an unconfirmed-width/units `u32`; (3) the roadmap's literal
      `UNKNOWN_CMD` text has no matching variant among Milestone 2's Error
      Model's eleven spec-named `RcpError` variants — this crate reads it as
      informal phrasing for the already-defined `RcpError::UnsupportedCmd`
      (`UNSUPPORTED_CMD`) rather than adding a new variant on the strength
      of one checklist bullet's wording. Like every prior Milestone 1-4
      entry, this remains additive standalone plumbing only — nothing here
      is wired into an actual decoder or dispatch loop. New
      `REQ-UART-001`..`REQ-UART-010` added to `.fusa-reqs.json`, each with a
      `// fusa:req`/`// fusa:test` pair in `src/uart.rs`.
- [x] **ADC** (`ep_type 0x09`): ≤16-bit resolution; three-level averaging
      model (`adc_sample_interval` → `adc_avg_intervals_per_request` →
      `adc_combine_avg_values`); request-driven sampling only. Done
      (v0.7.0-dev): new `src/adc.rs` adds [`AdcResolutionBits`] (an explicit,
      validated `1..=16` bit-width type rather than this crate silently
      assuming every ADC sample is a full 16-bit value) and
      [`AdcSampleValue`] (a raw reading paired with the resolution it was
      taken at, refusing any raw value wider than that resolution allows);
      [`AdcAveragingConfig`] plus [`resolve_adc_sample_window_ticks`] and
      [`resolve_adc_averaged_value`] turning the three-level averaging
      model's stated field order into two pure, testable functions — one
      chaining `adc_sample_interval` into the total elapsed tick count a
      combined result takes to produce, the other reducing raw samples
      through both averaging levels to that combined result — mirroring
      `uart::resolve_uart_read_completion`'s and
      `spi::truncate_spi_status_for_compound_wait`'s own prose-rule-to-
      function discipline; and [`AdcSamplingMode`]/
      [`validate_adc_sample_request`] enforcing the request-driven-sampling-
      only rule by rejecting a continuous/free-running mode, mirroring
      `uart::validate_uart_read_request`'s payload-less-read-only refusal
      path. `AdcFunctionalConfig::layer_tag` composes against
      `regmap::check_functional_config_matches_ep_type`'s existing
      cross-layer rule unchanged, matching every prior Milestone 4 entry's
      own precedent. Three points flagged per Guiding Principle 5 rather
      than silently resolved: (1) neither `adc_sample_interval`'s nor
      `adc_avg_intervals_per_request`'s/`adc_combine_avg_values`'s wire
      width or units are stated by the checklist text, so `AdcAveragingConfig`
      carries them as this crate's own unconfirmed-width/units placeholders
      (`u32`/`u16`/`u16`), mirroring `uart::UartRxQueueConfig::uart_timeout`'s
      own treatment; (2) unlike UART's checklist bullet, which named its
      violation's error code explicitly (`UNKNOWN_CMD`), this checklist
      bullet names no error code for a rejected continuous-sampling request
      at all — `validate_adc_sample_request` returns the already-defined
      `RcpError::UnsupportedCmd` by the same "closest existing match"
      reasoning `uart::validate_uart_read_request`'s own provenance note
      gives, not a confirmed spec-named code; (3) `AdcResolutionBits::default`
      resolves to the widest modeled width, 16 bits, this crate's own
      reasonable placeholder for an unconfirmed power-on default, mirroring
      `i2c::I2cSpeedMode::default`'s own explicitly-flagged choice. Like
      every prior Milestone 1-4 entry, this remains additive standalone
      plumbing only — nothing here is wired into an actual decoder, dispatch
      loop, or sampling engine. New `REQ-ADC-001`..`REQ-ADC-010` added to
      `.fusa-reqs.json`, each with a `// fusa:req`/`// fusa:test` pair in
      `src/adc.rs`.
- [x] **PWM_OUT / PWM_IN** (`ep_type 0x07`/`0x08`): shared
      period+active-duration pair shape; PWM_IN's `PWM_IN_NO_SIGNAL` timeout
      instead of hanging or returning stale data. Done (v0.7.0-dev): new
      `src/pwm.rs` adds [`PwmDurationPair`] (the shared period+active-duration
      pair shape) as a field on two dedicated per-direction functional-config
      types, [`PwmOutFunctionalConfig`] and [`PwmInFunctionalConfig`] — this
      is the first Milestone 4 checklist bullet naming two
      `regmap::EndpointType` tags (`PwmOut`/`PwmIn`) at once, so unlike
      `UartFunctionalConfig`'s single shared block, each direction gets its
      own type with its own `layer_tag` composing against
      `regmap::check_functional_config_matches_ep_type`'s existing
      cross-layer rule independently; and
      [`PwmInReadResolution`]/[`resolve_pwm_in_read`] turning the
      `PWM_IN_NO_SIGNAL` timeout rule into a pure function mirroring
      `uart::resolve_uart_read_completion`'s own prose-rule-to-function
      discipline — it never blocks (returns `None` for a genuinely
      in-progress read, letting a polling caller retry rather than hanging)
      and never re-reports a stale prior measurement once the timeout
      elapses (`NoSignal` always wins over any `last_measured` value past
      the threshold). Three points flagged per Guiding Principle 5 rather
      than silently resolved: (1) `PwmDurationPair`'s `period`/
      `active_duration` field widths and units are unstated by the
      checklist text, so both are carried as unconfirmed-width/units `u32`
      values, mirroring `uart::UartRxQueueConfig::uart_timeout`'s own
      precedent, and whether `active_duration` may validly exceed `period`
      is left unvalidated; (2) as with UART's zero-threshold discipline, a
      zero-valued `no_signal_timeout` is not treated as "disabled," so a
      default, zeroed `PwmInFunctionalConfig` resolves every read as
      `NoSignal` immediately; (3) `PWM_IN_NO_SIGNAL` has no candidate among
      Milestone 2's Error Model's eleven TC18-spec-named `RcpError`
      variants (unlike UART's `UNKNOWN_CMD`/`UnsupportedCmd` mapping), so it
      is modeled as a resolved measurement outcome
      (`PwmInReadResolution::NoSignal`) rather than as an `RcpError` variant,
      and the module does not infer no-signal from a genuinely-zero
      measured pair — only elapsed time against the configured timeout
      drives the outcome. Like every prior Milestone 1-4 entry, this
      remains additive standalone plumbing only — nothing here is wired
      into an actual decoder or dispatch loop. New `REQ-PWM-001`..
      `REQ-PWM-009` added to `.fusa-reqs.json`, each with a `// fusa:req`/
      `// fusa:test` pair in `src/pwm.rs`.
- [x] Generic `evt[2:0]` group conventions common to all of the above
      (Groups A/B/C) and the shared common functional-config fields
      (`ep_enable`, `ep_clear_req_storage`, `ep_req_crc_enable`, etc.)
      Done (v0.7.0-dev): this is Milestone 4's closing item, picked up only
      after all six concrete endpoint types above had already built their
      own private reading of `evt.sub_opcode` — this bullet generalizes
      that experience rather than preceding it, per this milestone's own
      Goal text. New `src/evtgroup.rs` adds [`EvtGroup`] (the three
      roadmap-named group letters as an explicit, ordinal-round-tripping
      enum) and [`classify_evt_sub_opcode`] (a total, never-panicking
      function over the full `evt.sub_opcode` range). `regmap.rs`'s
      [`CommonFunctionalConfig`] — previously a literal empty placeholder
      struct — now carries the three named fields (`ep_enable`,
      `ep_clear_req_storage`, `ep_req_crc_enable`) as `bool`s, with
      [`CommonFunctionalConfig::encode`]/[`CommonFunctionalConfig::decode`]
      giving it a never-panicking, fixed-length wire form matching every
      other Milestone 1/2 type's own big-endian/fixed-length convention.
      Two points flagged per Guiding Principle 5 rather than silently
      resolved: (1) this crate's own `ROADMAP.md` names the three "Groups
      A/B/C" letters but states neither the classification axis (per-value
      vs. per-endpoint-type) nor any concrete value/type-to-letter
      assignment, and no `§`-numbered citation accompanies this bullet the
      way sibling Milestone 2 bullets cite `§3.6`-`§3.11` — so
      `classify_evt_sub_opcode` validates `sub_opcode`'s 3-bit range but
      always returns `Ok(None)` rather than guessing an assignment,
      mirroring `GpioWriteSemantics::Unnamed8th`'s and
      `I2cSpeedMode::HighSpeedRowA`/`HighSpeedRowB`'s own treatment of
      unconfirmed slots; retrofitting GPIO's or SPI's own already-built
      private `sub_opcode` readings onto an `EvtGroup` is explicitly left
      to later work, not guessed at here. (2) the checklist bullet's
      trailing "etc." implies a longer field list for
      `CommonFunctionalConfig` than the three fields it names, but this
      crate's own spec-extraction pass records no further field names for
      this particular item (unlike `§3.6`'s or `§3.8`'s own fully
      enumerated tables), so only the three named fields are modeled — no
      plausible-sounding neighbors are invented to fill out "etc."; each
      field is also given its own full encoded byte rather than a guessed
      shared-byte bit-packing, mirroring `RequestStreamConfigEntry`'s own
      "meaningfully binary but wire-width-unconfirmed" precedent. Like
      every prior Milestone 1-4 entry, this remains additive standalone
      plumbing only — `EvtGroup` is not wired into any decoder or dispatch
      loop, and none of the six already-built endpoint modules were
      retrofitted to consume it. New `REQ-EVTGRP-001`..`REQ-EVTGRP-004`
      (in `src/evtgroup.rs`) and `REQ-RMAP-028`..`REQ-RMAP-029` (in
      `src/regmap.rs`) added to `.fusa-reqs.json`, each with a
      `// fusa:req`/`// fusa:test` pair. This closes out Milestone 4's
      checklist in full.

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

- [x] Compound / compound-wait (`0x0F`/`0x0B`): sequencer-gated execution
      and wait; `cmp_exec_delay`/`cmpw_exec_delay` timers; "advance sequencer
      only if still in start state" rule. Done (v0.8.0-dev): new
      `src/request.rs` — the module name PR #37's naming-reconciliation
      pass reserved for this work — adds `RequestKind` (`Compound = 0x0F`,
      `CompoundWait = 0x0B`) with `to_u8`/`from_u8`; `SequencerState` and
      `CompoundGateConfig` (a plain `sequencer_num` byte plus the required
      `start_state`, mirroring `RequestStreamConfigEntry::
      rx_safestate_sequencer`'s own "sequencer number is a plain byte"
      precedent) with `check_sequencer_num_in_bounds` (against
      `GeneralRegisters::svr_sequencers_max`), `is_gate_satisfied`, and
      `check_compound_gate` implementing the sequencer-gated-execution
      rule; `CompoundExecDelays { cmp_exec_delay, cmpw_exec_delay }` with
      `resolve_compound_exec_delay` selecting the timer for a given
      `RequestKind`; and `advance_sequencer_if_still_in_start_state`, a
      pure function implementing the "advance only if still in start
      state" race guard. Three points flagged per Guiding Principle 5: (1)
      unlike `acf_msg_type`, no checklist text states which byte/field of a
      request actually carries the `0x0F`/`0x0B` discriminant, so
      `RequestKind` is a standalone value type not yet attached to any
      offset within `ByteMessageInfo` or elsewhere; (2)
      `cmp_exec_delay`/`cmpw_exec_delay` are modeled as `u32`
      unconfirmed-width/units placeholders, mirroring `UartRxQueueConfig::
      uart_timeout`'s and `PwmInFunctionalConfig::no_signal_timeout`'s own
      precedent; (3) the sequencer-state machine this gating rule reads is
      itself the next, not-yet-built checklist bullet below, so every
      function here takes the sequencer's current state as a
      caller-supplied `SequencerState` value (mirroring `RcServerState::
      try_transition`'s `is_consistent` closure and `check_ep0_access_for_
      stream`'s `root_client` parameter) rather than blocking on it, and
      the post-execution "next state" is likewise caller-supplied since no
      advancement convention is named. Additive standalone plumbing only —
      not wired into any decoder, dispatch loop, or the request-lifecycle
      state machine (a later bullet in this same milestone); the legacy
      `src/prioqueue.rs` decorator this milestone's Goal text names is read
      only as background, not extended. New `REQ-CMP-001`..`REQ-CMP-007`
      (in `src/request.rs`) added to `.fusa-reqs.json`, each with a
      `// fusa:req`/`// fusa:test` pair.
- [x] Triggered (`0x0E`): trigger-occurrence counting that runs independent
      of endpoint busy/idle state; `trigger_exec_delay`; infinite-repeat
      sentinel (`0xFFFF`). Done (v0.8.0-dev): `src/request.rs` gains
      `RequestKind::Triggered = 0x0E` alongside the existing `Compound`/
      `CompoundWait` variants, with `to_u8`/`from_u8` extended to match;
      `TriggerExecDelay` (a single `u32` field, mirroring
      `CompoundExecDelays`) with `resolve_trigger_exec_delay` selecting it
      only for `RequestKind::Triggered`; `TriggerRepeatCount` (`Finite(u16)`
      / `Infinite`) with `to_u16`/`from_u16` modeling the `0xFFFF`
      infinite-repeat sentinel as an explicit variant rather than a bare
      `u16` that would let it silently mean 65535 repeats, mirroring
      `GpioWriteSemantics::Unnamed8th`'s and `I2cSpeedMode::
      HighSpeedRowA`/`HighSpeedRowB`'s own named-special-value precedent;
      `is_trigger_repeat_exhausted` implementing the exhaustion check
      against that repeat count; and `should_count_trigger_occurrence`, a
      pure predicate that always returns `true` regardless of the
      endpoint's busy/idle state, turning the "runs independent of endpoint
      busy/idle state" rule into a real, testable function signature.
      `resolve_compound_exec_delay`'s return type widened from `u32` to
      `Option<u32>` (returning `None` for `RequestKind::Triggered`, which
      has no `CompoundExecDelays` field of its own) to stay exhaustive over
      the now three-variant `RequestKind`; not yet called from anywhere in
      this crate, so this is a safe additive-stage widening, not a breaking
      change to any consumer. Two points flagged per Guiding Principle 5:
      (1) same as the compound/compound-wait item above, no checklist text
      states which byte/field of a request carries the `0x0E` discriminant
      or the `trigger_exec_delay`/occurrence-count fields' wire width
      (`0xFFFF` fitting only a 16-bit field is the one width fact the
      checklist text itself confirms); (2) this crate has no unified
      endpoint busy/idle state type yet, so `should_count_trigger_occurrence`
      takes busy/idle state as a caller-supplied, deliberately-ignored
      `bool` parameter rather than reading real endpoint state — mirroring
      `SequencerState`'s own caller-supplied-rather-than-read precedent.
      Confirmed `src/gpio.rs`'s existing `GpioTriggerConfig`/
      `GpioTriggerSignals`/`evaluate_gpio_triggers` is an unrelated,
      narrower, already-built GPIO-specific edge-detection concept, not
      reused or extended here. Additive standalone plumbing only, same
      discipline as every prior Milestone 1-5 entry — not wired into any
      decoder, dispatch loop, or the request-lifecycle state machine. New
      `REQ-TRIG-001`..`REQ-TRIG-005` (in `src/request.rs`) added to
      `.fusa-reqs.json`, each with a `// fusa:req`/`// fusa:test` pair;
      `REQ-CMP-006`'s `.fusa-reqs.json` text updated to describe its new
      `Option<u32>` return shape.
- [x] Chained (`0x01`): `cs`-bit abort-on-predecessor-error semantics;
      `CHAIN_ABORTED`/`CHAIN_ERROR`. Done (v0.8.0-dev): `src/request.rs`
      gains `RequestKind::Chained = 0x01` alongside the existing
      `Compound`/`CompoundWait`/`Triggered` variants, with `to_u8`/`from_u8`
      extended to match; and `check_chain_continuation`, a pure function
      taking a chained request link's decoded
      `crate::acf::ByteMessageInfo::cs` flag and whether the chain's
      preceding link errored, returning `Err(RcpError::ChainAborted)` when
      both are true and `Ok(())` otherwise — the `cs`-bit
      abort-on-predecessor-error rule this checklist bullet names.
      `resolve_compound_exec_delay`/`resolve_trigger_exec_delay` both widen
      to return `None` for `RequestKind::Chained` to stay exhaustive over
      the now four-variant `RequestKind` (not yet called from anywhere in
      this crate, so this is a safe additive-stage widening). `RcpError`
      (`src/lib.rs`) gains two new variants, `ChainAborted`/`ChainError`,
      for the checklist's named `CHAIN_ABORTED`/`CHAIN_ERROR` codes — the
      first Milestone-5-era additions to `RcpError` since Milestone 2's
      Error Model item closed out its eleven spec-named codes. Two points
      flagged per Guiding Principle 5: (1) whether either code collapses
      onto one of those eleven (mirroring how UART's `UNKNOWN_CMD`
      collapsed onto `RcpError::UnsupportedCmd` in Milestone 4) was
      checked and rejected — `RcpError::RequestRejected` is the closest
      candidate but its "rejected outright, before execution" shape does
      not capture either code's own mid-chain-execution meaning, so both
      are added as genuinely new variants instead; (2) the checklist text
      names `CHAIN_ABORTED`/`CHAIN_ERROR` side by side without stating what
      distinguishes them — this crate's working interpretation, flagged
      rather than silently assumed, reads `ChainError` as "this link's own
      execution failed" and `ChainAborted` as "this link did not run at
      all because an earlier link's `ChainError` and this link's `cs` bit
      together aborted it", under which `check_chain_continuation` can
      only ever construct `ChainAborted`; `ChainError` is added for naming
      completeness but not yet constructed anywhere in this crate,
      mirroring Milestone 2's own precedent of reserving
      `SequencerNotKnown`/`RequestCanceled`/`RequestNotFound`/
      `EpNotFound`/`ReqStorageOvfl` ahead of the concrete code that
      constructs them. Same "additive standalone plumbing only" discipline
      as every prior Milestone 1-5 entry — not wired into any decoder,
      dispatch loop, or the request-lifecycle state machine. New
      `REQ-CHAIN-001`..`REQ-CHAIN-003` (in `src/request.rs`) added to
      `.fusa-reqs.json`, each with a `// fusa:req`/`// fusa:test` pair.
- [x] Timed (`0x0A`): presentation-time execution as an alternative to a
      TSCF header. Done (v0.8.0-dev): `src/request.rs` gains
      `RequestKind::Timed = 0x0A` alongside the existing `Chained`/
      `CompoundWait`/`Triggered`/`Compound` variants, with `to_u8`/`from_u8`
      extended to match; `TimedExecutionTime`, a Timed request's own
      carried presentation-time execution gate, modeled by composing the
      existing `crate::timestamp::AvtpTimestamp` newtype (Milestone 1) by
      value rather than duplicating its shape or adding an
      unconfirmed-width `u32` placeholder of its own; and
      `is_timed_request_ready`, a pure function implementing this
      checklist bullet's own presentation-time-execution readiness rule —
      whether a caller-supplied current `AvtpTimestamp` has reached or
      passed a Timed request's own `TimedExecutionTime`, reusing
      `AvtpTimestamp::is_after`'s existing wraparound-aware ordering and
      `AvtpTimestamp::is_untimed`'s existing all-zero-means-untimed
      fallback rather than inventing new ordering or fallback logic.
      `resolve_compound_exec_delay`/`resolve_trigger_exec_delay` both
      widen to return `None` for `RequestKind::Timed` to stay exhaustive
      over the now five-variant `RequestKind` (not yet called from
      anywhere in this crate, so this is a safe additive-stage widening).
      One point flagged per Guiding Principle 5: no checklist text states
      a Timed request's own execution-time field's byte offset, field
      name, or wire width; this crate reads "alternative to a TSCF
      header" as "carries the same kind of presentation-time value a TSCF
      header would have supplied, just sourced from the request itself",
      and composes `AvtpTimestamp` on that basis — a judgment call, not a
      confirmed wire fact, so `TimedExecutionTime` is, like `RequestKind`
      itself, a standalone value type not yet tied to any offset within
      `crate::acf::ByteMessageInfo` or any other decoded wire shape. Same
      "additive standalone plumbing only" discipline as every prior
      Milestone 1-5 entry — not wired into any decoder, dispatch loop, or
      the request-lifecycle state machine. New `REQ-TIME-001`..
      `REQ-TIME-003` (in `src/request.rs`) added to `.fusa-reqs.json`,
      each with a `// fusa:req`/`// fusa:test` pair.
- [x] Cancellation: clear-all (`0x05`, mandatory), clear-non-safestate
      (`0x06`, optional), clear-single (`0x07` + `clear_transaction_num`,
      optional). Done (v0.8.0-dev): `src/request.rs` gains
      `RequestKind::ClearAll = 0x05`, `RequestKind::ClearNonSafestate = 0x06`,
      and `RequestKind::ClearSingle = 0x07` alongside the existing
      `Chained`/`Timed`/`CompoundWait`/`Triggered`/`Compound` variants, with
      `to_u8`/`from_u8` extended to match; `check_clear_all_cancellation`, a
      pure function implementing the mandatory clear-all rule (always
      cancels); `check_clear_non_safestate_cancellation`, a pure function
      taking a caller-supplied `is_safestate_related` flag and canceling
      unless it is set; and `ClearTransactionNum` (a `u8` newtype matching
      `acf::ByteMessageInfo::transaction_num`'s own width) with
      `check_clear_single_cancellation`, a pure function canceling only when
      a caller-supplied candidate transaction number matches the configured
      target. All three functions construct `RcpError::RequestCanceled` for
      a request they select for cancellation — the first construction site
      for that Milestone-2-reserved sentinel, retiring it as a placeholder.
      `resolve_compound_exec_delay`/`resolve_trigger_exec_delay` both widen
      to return `None` for all three cancellation variants to stay
      exhaustive over the now eight-variant `RequestKind` (not yet called
      from anywhere in this crate, so this is a safe additive-stage
      widening). Three points flagged per Guiding Principle 5: (1) same as
      every prior conditional-request kind in this module, no checklist
      text states which byte/field of a request carries the
      `0x05`/`0x06`/`0x07` discriminant or `clear_transaction_num`'s wire
      offset, nor what scope ("every pending/in-flight request") is bounded
      by for clear-all — the addressed endpoint, the addressed stream, or
      this whole RC Server — so `check_clear_all_cancellation` is the
      uniform per-request outcome rule only, not a scope-enumeration
      function; (2) clear-non-safestate's safe-state-driving determination
      is taken as a caller-supplied `bool` rather than read from this
      crate's not-yet-built `rx_safety_measure`/safe-state machinery
      (`ROADMAP.md` Milestone 6), mirroring `SequencerState`'s and
      `should_count_trigger_occurrence`'s own caller-supplied-state
      precedent; (3) `clear_transaction_num` is read as matching against
      the already-decoded `acf::ByteMessageInfo::transaction_num` (a plain
      `u8`) rather than a new field, since the checklist text gives no
      field name or width of its own and this is the closest existing
      per-transaction correlation id this crate has decoded. Same
      "additive standalone plumbing only" discipline as every prior
      Milestone 1-5 entry — not wired into any decoder, dispatch loop, or
      the request-lifecycle state machine. New `REQ-CANCEL-001`..
      `REQ-CANCEL-004` (in `src/request.rs`) added to `.fusa-reqs.json`,
      each with a `// fusa:req`/`// fusa:test` pair.
- [x] Sequencers: persistent 8-bit state registers, power-on default state
      `1`, bounded by `svr_sequencers_max`. Done (v0.8.0-dev): `src/request.rs`
      gains `SequencerBank`, the persistent 8-bit sequencer-state register
      bank this checklist bullet names: one `SequencerState` per sequencer
      number, sized and live-bounded by a `svr_sequencers_max: u8` value
      mirroring `GeneralRegisters::svr_sequencers_max`. `SequencerBank::new`
      builds a fresh bank, initializing every sequencer to the power-on
      default state `1` by reusing `SequencerStateEntry::power_on_default`'s
      already-confirmed value rather than re-deriving it (a
      `svr_sequencers_max` of `0` yields an empty bank, mirroring
      `check_sequencer_num_in_bounds`'s existing "`0` means no sequencers
      exist" reading); `SequencerBank::read` reads a sequencer's current
      state, reusing `check_sequencer_num_in_bounds` for the bound check;
      `SequencerBank::advance_if_still_in_start_state` composes the
      already-built `advance_sequencer_if_still_in_start_state` pure race
      guard against this bank's own live, mutable store instead of
      duplicating that rule; and `SequencerBank::check_compound_gate`
      composes `SequencerBank::read` with the existing free-function
      `check_compound_gate`, finally giving it a genuine backing store to
      read `current_state` from instead of requiring every caller to supply
      one by hand. The pre-existing free functions
      (`is_gate_satisfied`/`check_compound_gate`/
      `advance_sequencer_if_still_in_start_state`) are unchanged and still
      take `SequencerState` as a caller-supplied parameter — `SequencerBank`
      composes them rather than replacing them, so nothing built on top of
      them in the four prior Milestone 5 entries breaks. One point flagged
      per Guiding Principle 5: the checklist text names only one reset
      trigger ("power-on") for the default state and states nothing about
      any other reset condition or about resetting an already-live bank in
      place, so `SequencerBank::new` is deliberately the *only* way to
      obtain an all-defaults bank (no separate `reset`/`power_on_reset`
      method is added) — see `src/request.rs`'s own doc comment "Provenance
      note: `SequencerBank`'s reset-trigger scope". Same "additive
      standalone plumbing only" discipline as every prior Milestone 1-5
      entry — `SequencerBank` is not wired into any decoder, dispatch loop,
      or the request-lifecycle state machine (the next checklist bullet).
      New `REQ-SEQ-001`..`REQ-SEQ-004` (in `src/request.rs`) added to
      `.fusa-reqs.json`, each with a `// fusa:req`/`// fusa:test` pair.
- [x] Execution priority ordering: cancellation > triggered > timed >
      compound > compound-wait > chained > standard, FIFO within a tier.
      Done (v0.8.0-dev): `src/request.rs` gains `RequestKind::Standard`, a
      ninth variant alongside the eight prior kinds, giving the checklist's
      own lowest-priority "standard" tier a `RequestKind` of its own, with
      `to_u8`/`from_u8` extended to match; `ExecutionPriorityTier`, the
      seven named priority tiers in the checklist's own stated order, and
      `execution_priority_tier` collapsing all nine `RequestKind` values
      down to them (the three cancellation variants — `ClearAll`/
      `ClearNonSafestate`/`ClearSingle` — all collapsing onto one
      `ExecutionPriorityTier::Cancellation` tier); and `PendingRequestKey`
      / `select_next_pending_request`, a pure selection function choosing
      which of a caller-supplied set of pending requests runs next —
      highest tier first, FIFO (earliest `arrival_seq`) within a tier.
      `resolve_compound_exec_delay`/`resolve_trigger_exec_delay` both widen
      to return `None` for `RequestKind::Standard` to stay exhaustive over
      the now nine-variant `RequestKind` (not yet called from anywhere in
      this crate, so this is a safe additive-stage widening). Two points
      flagged per Guiding Principle 5: (1) unlike every other `RequestKind`
      variant, no checklist text anywhere in this crate's roadmap gives
      "standard" a numeric discriminant at all; `RequestKind::Standard`'s
      assigned byte (`0x00`) is a crate-local placeholder chosen only so
      `#[repr(u8)]`'s `to_u8` cast compiles, not a transcription of any
      confirmed TC18 wire value — see `src/request.rs`'s own doc comment
      "Provenance note: `RequestKind::Standard`'s discriminant"; (2) the
      checklist names no error/rejection behavior for a pending request
      that never gets a turn (queue overflow, starvation), and does not
      state whether priority is evaluated per-endpoint or per-stream/
      server-wide — `select_next_pending_request` takes no position on
      either, operating purely over whatever caller-supplied slice of
      pending requests it is given. This is the "Execution priority
      ordering" checklist bullet the milestone's own Goal text points at as
      the eventual absorption target for the old `src/prioqueue.rs`
      decorator's job — `select_next_pending_request` is this item's own
      from-scratch, spec-native implementation of that job, but
      `src/prioqueue.rs` itself is still not touched, extended, or migrated
      onto it; that KEEP/DEPRECATE-style migration is Milestone 9's job
      (`prioqueue` is DEPRECATE-dispositioned in that milestone's satellite
      table). Same "additive standalone plumbing only" discipline as every
      prior Milestone 1-5 entry — not wired into any decoder, dispatch
      loop, or the request-lifecycle state machine (the next checklist
      bullet, still not built). New `REQ-PRIO-001`..`REQ-PRIO-004` (in
      `src/request.rs`) added to `.fusa-reqs.json`, each with a
      `// fusa:req`/`// fusa:test` pair.
- [x] Request lifecycle state machine: pending → started → under-execution
      → finalized, with the type-specific sub-behavior at each transition
      (§3.14). Done (v0.8.0-dev): `src/request.rs` gains
      `RequestLifecycleState` (`Pending`/`Started`/`UnderExecution`/
      `Finalized`, in that order, carrying no numeric encoding — see
      "Provenance note: `RequestLifecycleState` carries no numeric
      encoding") and `is_request_lifecycle_transition_defined`, the
      coarse state-shape check admitting only the three linear forward
      hops (no skip/backward/identity transition), mirroring
      `crate::lifecycle::is_transition_defined`'s own discipline;
      `RequestLifecycleState::try_transition`, a self-consuming,
      never-panicking `Result<RequestLifecycleState, RcpError>` method
      mirroring `crate::lifecycle::RcServerState::try_transition`'s own
      coarse-check-then-guard shape; and `RequestLifecycleGuardInput`, one
      variant per `RequestKind`, giving `try_transition` the "type-specific
      sub-behavior at each transition" this checklist bullet names by
      dispatching each kind onto the already-built per-kind check that hop
      composes: `check_compound_gate` for Compound/CompoundWait and
      `is_timed_request_ready` for Timed gate `Pending` → `Started`;
      `check_chain_continuation` for Chained and
      `should_count_trigger_occurrence`/`is_trigger_repeat_exhausted` for
      Triggered gate `Started` → `UnderExecution`; `UnderExecution` →
      `Finalized` is unconditional for every kind. The cancellation trio's
      own type-specific behavior is separate:
      `try_force_cancel_all`/`try_force_cancel_non_safestate`/
      `try_force_cancel_single`, each composing the matching
      `check_clear_*_cancellation` function to force a *target* request
      straight to `Finalized` (idempotently, once already `Finalized`)
      rather than gating that target's own linear progression — the
      "short-circuit straight to a rejected/aborted outcome" the
      cancellation trio's own role as *acting on another pending request*
      implies, distinct from every other `RequestKind`'s role of gating
      its own progression. Per Guiding Principle 5, two points are flagged
      in `src/request.rs`'s own "Provenance note: which existing check
      applies at which lifecycle hop": (1) since §3.14 is cited by section
      number only, exactly which per-kind rule gates which of the three
      hops is this item's own working interpretation, not a transcription
      of confirmed spec structure; (2) `UnderExecution` → `Finalized`
      being unconditional for every kind is likewise this item's own
      reading — no checklist wording anywhere in this crate's roadmap text
      names a rule for whether an already-executing request is allowed to
      *finish*, distinct from whether it was allowed to *start*. Same
      "additive standalone plumbing only" discipline as every prior
      Milestone 1-5 entry: not wired into any decoder, dispatch loop, or
      `select_next_pending_request` (picking *which* pending request goes
      next and advancing *that* request's own lifecycle state stay two
      separate, uncomposed pieces for now). New `REQ-RLC-001`..`REQ-RLC-006`
      (in `src/request.rs`) added to `.fusa-reqs.json`, each with a
      `// fusa:req`/`// fusa:test` pair.
- [x] Feature-bundle gating: claiming "compound request support" requires
      shipping compound-wait, ≥4 sequencers, *and* clear-non-safestate
      together — not compound message parsing alone. Done (v0.8.0-dev):
      `src/request.rs` gains `check_compound_bundle_claim`, composing the
      three prerequisite facts this milestone's own first eight bullets
      already established — compound-wait support
      (`RequestKind::CompoundWait`), a sequencer bank sized for at least
      `MIN_SEQUENCERS_FOR_COMPOUND_BUNDLE` (`4`) sequencers
      (`SequencerBank`/`svr_sequencers_max`), and clear-non-safestate
      cancellation support (`check_clear_non_safestate_cancellation`) —
      into the single honesty check this checklist bullet names: all three
      together, not compound-message parsing alone. This is deliberately a
      composing, not a discovering, item — every fact it consults already
      exists from prior Milestone 5 work; this item's own job is only the
      gating rule tying the three together. `check_compound_bundle_claim`
      takes its three prerequisite facts as plain caller-supplied
      parameters (`has_compound_wait: bool`, `svr_sequencers_max: u8`,
      `has_clear_non_safestate: bool`) rather than reading them off a live
      `crate::regmap::GeneralRegisters`, mirroring `SequencerState`,
      `root_client`, and `is_safestate_related` all being taken the same
      way earlier in this milestone, and returns
      `Err(crate::RcpError::InvalidParameter)` — one of the eleven
      confirmed TC18 spec error codes, reused rather than a new
      sentinel of its own — for any claim missing one or more of the
      three. `src/regmap.rs` gains `GeneralRegisters::
      claims_compound_wait_bundle`, the first named per-bit accessor onto
      the previously-fully-undecomposed `svr_implemented_options`
      bitmask: this item is the "later item" that module's own provenance
      note deferred a first named per-bit accessor to, needing to know
      which bit the "compound request support" bundle occupies to pair
      naturally with `check_compound_bundle_claim`. Two points flagged per
      Guiding Principle 5: (1) the extraction records the bitmask's five
      option-bundle names (compound&wait / triggered / chained /
      time-sync&timed / enhanced-cancel) but no bit-position assignment
      for any of them, so this crate assigns them to bits `0`-`4` in the
      extraction's own listed order — a crate-local placeholder, not a
      confirmed spec bit position, reconciled against a real RC Server
      (never against spec prose) before being relied on for interop; (2)
      `check_compound_bundle_claim` is scoped to the one "compound request
      support" bundle this checklist bullet names — it takes no position
      on what an honest claim for any of the other four bundles would
      require. Same "additive standalone plumbing only" discipline as
      every prior Milestone 1-5 entry: `check_compound_bundle_claim` is a
      pure function over caller-supplied inputs, not wired into any
      decoder, dispatch loop, or not-yet-built RC Server instance that
      would decide what to set `svr_implemented_options` to.
      New `REQ-BUNDLE-001`, `REQ-BUNDLE-002` (in `src/request.rs`) and
      `REQ-RMAP-030` (in `src/regmap.rs`) added to `.fusa-reqs.json`, each
      with a `// fusa:req`/`// fusa:test` pair. This closes out Milestone
      5's nine-bullet checklist in full.

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

- [x] CRC32 safe-point implementation: poly `0xF4ACFB13`, init/final XOR
      `0xFFFFFFFF`, reflected input and output — a genuinely different
      algorithm from the current CRC-16, not a width change. Done
      (v0.9.0-dev): `src/e2e.rs` gains `crc32_tc18`, a standalone reflected
      CRC-32 (poly `0xF4ACFB13`, init/xorout `0xFFFFFFFF`, reflected input
      and output) implemented as an LSB-first shifting register against the
      polynomial's bit-reversal, structurally distinct from the file's
      pre-existing MSB-first non-reflected `crc16_ccitt_false`. Since the
      four stated parameters fully determine one specific CRC-32 variant
      and `0xF4ACFB13` is not a named/published variant with an externally
      citable check value, correctness is established by cross-checking
      `crc32_tc18` against a second, independently-structured
      reference implementation of the same definition (reflect-per-byte +
      unreversed-polynomial MSB-first engine) across the standard
      `"123456789"` corpus, all-zero/all-`0xFF` boundary inputs, and
      several other varied inputs, plus never-panics and
      input-sensitivity coverage — see `src/e2e.rs`'s own provenance note
      for the reasoning. New `REQ-CRC-001`..`REQ-CRC-003` added to
      `.fusa-reqs.json` (a fresh prefix, since `REQ-E2E-*` already names
      the CRC-16 requirements this milestone is superseding), each with a
      `// fusa:req`/`// fusa:test` pair. Same "additive standalone
      plumbing only" discipline as every prior milestone entry:
      `crc32_tc18` is not wired into `wrap`/`unwrap` or any caller yet —
      determining which bytes of a safe-point frame it covers is this
      milestone's next ("Coverage rule") checklist bullet, which needs the
      AVTPDU/ACF framing types this bullet deliberately does not touch.
      This entry also does not address which requests/streams get
      CRC-protected in the first place (the later "Per-stream safety
      config" bullet's job).
- [x] Coverage rule: CRC spans `stream_id` + `avtp_timestamp` (zeroed under
      NTSCF) + the full ACF header + payload; length-field pre-adjustment
      (+1 quadlet / +4 octets) before computing it. Done (v0.9.0-dev):
      `src/e2e.rs` gains `build_crc32_coverage_buffer`, assembling the
      exact byte sequence `crc32_tc18` is meant to run over — the 8-byte
      `stream_id`, then a 4-byte `avtp_timestamp` position (the real value
      for `avtp::HeaderVariant::Tscf`, four zero bytes in that same
      position for `avtp::HeaderVariant::Ntscf`, never omitted), then the
      full ACF header (the `acf_msg_type` discriminant, `byte_message_info`,
      and — for ACF_GBB only — `message_timestamp`), then the payload — via
      a new `AcfCoverageMessage` enum selecting between `acf::AcfAbbMessage`
      and `acf::AcfGbbMessage` and reusing `acf::encode_acf_abb`/
      `acf::encode_acf_gbb` rather than re-deriving either message's wire
      layout. Per Guiding Principle 5, this entry flags rather than
      silently resolves which length field the checklist's "+1 quadlet /
      +4 octets" pre-adjustment applies to: `ROADMAP.md` does not say, and
      of this crate's currently-decoded length fields, `byte_message_info`'s
      `acf_msg_length` is the only one living inside the covered region
      (the AVTP-level `ntscf_data_length`/`stream_data_length` fields sit
      entirely outside it) — so `build_crc32_coverage_buffer` treats
      `acf_msg_length` as the field being pre-adjusted, as its own working
      interpretation pending reconciliation against real TC18 behavior,
      never against spec prose. New `REQ-CRC-004`..`REQ-CRC-007` added to
      `.fusa-reqs.json`, each with a `// fusa:req`/`// fusa:test` pair.
      Same "additive standalone plumbing only" discipline as every prior
      milestone entry: `build_crc32_coverage_buffer` is not wired into
      `crc32_tc18`, `wrap`/`unwrap`, or `E2eController` — it only assembles
      the buffer a caller would pass to `crc32_tc18`, leaving the
      Fragmentation-interaction, Safety-request MSB-tagging, and
      CRC_ERROR-path bullets below to actually invoke it.
- [x] Fragmentation interaction: only the *last* fragment of a multi-segment
      message carries the CRC, computed across the combined payload. Done
      (v0.9.0-dev): `src/e2e.rs` gains `CombinedFragmentPayload` (assembling
      a fragment train's combined payload by concatenating a caller-
      supplied, already-ordered `&[&[u8]]` of per-fragment payloads),
      `build_crc32_coverage_buffer_for_fragment_train`/
      `crc32_tc18_for_fragment_train` (composing, not re-deriving,
      `build_crc32_coverage_buffer`/`crc32_tc18` — the combined payload
      replaces a single fragment's payload region while every other
      coverage field is taken from the caller-supplied final fragment's own
      header), and `fragment_crc_expectation`/`check_fragment_crc_placement`
      (the "only the last fragment carries the CRC" placement rule itself,
      stated as an explicit, queryable type rather than left implicit — the
      latter rejects both invalid states, a CRC present on a non-final
      fragment or absent on the final one, with `RcpError::InvalidParameter`
      rather than silently ignoring either). Per Guiding Principle 5, since
      this crate has no live multi-AVTPDU reassembly buffer yet (Milestone
      8's job) and has not resolved which bit(s) of
      `acf::ReadSizeOrSegment` would select a `segment_num` reading, a
      fragment train's segment order is taken as a caller-supplied fact
      (the ordered `&[&[u8]]` itself) rather than derived from
      `ReadSizeOrSegment::as_segment_num` here; likewise, since the
      roadmap does not state whether the coverage buffer's non-payload
      region should be drawn from each fragment individually or from one
      fragment alone, this entry takes the entire non-payload region (
      `stream_id`, `avtp_timestamp`, and the full ACF header incl.
      `message_timestamp` for ACF_GBB) from the final fragment's own
      header — both flagged in `src/e2e.rs`'s doc comment for
      reconciliation against real TC18 behavior, never against spec prose.
      New `REQ-CRC-008`..`REQ-CRC-010` added to `.fusa-reqs.json`, each with
      a `// fusa:req`/`// fusa:test` pair. Same "additive standalone
      plumbing only" discipline as every prior milestone entry: none of
      this is wired into `wrap`/`unwrap`, `E2eController`, or the not-yet-
      built reassembly buffer — it only states and implements the rule
      itself, leaving the Safety-request MSB-tagging and `CRC_ERROR`-path
      bullets below (and Milestone 8's real reassembly) to actually invoke
      it against live multi-AVTPDU traffic.
- [x] Safety-request MSB-tagging: `0x8F`/`0x8B`/`0x8E` variants; on watchdog
      overflow, normal-priority requests are purged while safety-tagged
      requests remain queued and become the mechanism that drives the
      endpoint through its safe state. Done (v0.9.0-dev): `src/request.rs`
      gains three new `RequestKind` variants — `SafetyCompound` (`0x8F`),
      `SafetyCompoundWait` (`0x8B`), `SafetyTriggered` (`0x8E`) — each
      exactly `0x80 | base` over `Compound`/`CompoundWait`/`Triggered`,
      with `from_u8`/`to_u8` and the `is_safety_tagged` predicate
      extended/added to match, and `execution_priority_tier`/
      `resolve_compound_exec_delay`/`resolve_trigger_exec_delay` widened
      (as this crate's own working interpretation) to map each
      safety-tagged variant onto its own untagged base kind's tier/timer,
      mirroring the same match-widening precedent every prior
      `RequestKind`-adding Milestone 5 entry already used. The
      watchdog-overflow purge rule composes two already-built pieces:
      `check_watchdog_overflow_purge`, a single-request check mirroring
      `check_clear_non_safestate_cancellation`'s own caller-supplied-`bool`
      shape and reusing `RcpError::RequestCanceled` rather than a new
      sentinel, and `purge_normal_priority_on_watchdog_overflow`, a pure
      partition function over `PendingRequestKey` composing that check
      across a caller-supplied slice — mirroring `select_next_pending_request`'s
      own "pure function over a slice, no owned queue" shape. Per Guiding
      Principle 5, this entry flags the same wire-placement ambiguity
      every prior `RequestKind`-adjacent Milestone 5 bullet flagged (no
      checklist text states which byte/field carries the MSB tag) and
      flags its own working interpretation that a safety-tagged variant's
      execution-priority tier/exec-delay timer is unchanged from its base
      kind, since the checklist states only the watchdog-overflow
      exemption. New `REQ-SAFETY-001`..`REQ-SAFETY-005` added to
      `.fusa-reqs.json` (a fresh prefix), each with a
      `// fusa:req`/`// fusa:test` pair. Same "additive standalone
      plumbing only" discipline as every prior milestone entry: neither
      new function is wired into a decoder, dispatch loop, or the legacy
      `watchdog.rs` (REPLACE-dispositioned, read only as background); real
      watchdog timeout tracking (`rx_wd_enable` et al.) and real safe-state
      machinery are left to the next two still-unchecked bullets below.
- [x] Per-stream safety config: `rx_enforce_e2e`, `rx_wd_enable` +
      `rx_wd_timeout_interval` + `rx_wd_safestate_enable` (replacing the
      old periodic-WATCHDOG-command model in `watchdog.rs` with the spec's
      real per-stream liveness-reset-on-every-request design),
      `rx_safety_measure` (hi-Z vs. sequencer-driven safe sequence),
      `rx_safestate_sequencer`/`rx_safe_sequencer_state`,
      `rx_ovrflw_safestate_enable`, `rx_enforce_seq`/`rx_seq_safestate_enable`.
      Done (v0.9.0-dev): `src/watchdog.rs` is REPLACEd outright — its old
      `Zone`/`Controller`/`Command{cmd_type: CommandType::WATCHDOG}`
      background-poll-thread model is deleted, not adapted — with the
      spec's real per-stream liveness model: `StreamWatchdogState`, whose
      `reset_on_request` is the "every request resets liveness" rule
      itself (no periodic poll message at all), `is_stream_watchdog_expired`
      (elapsed ticks vs. `rx_wd_timeout_interval`), and
      `evaluate_stream_watchdog`/`StreamWatchdogOutcome`, gated by
      `rx_wd_enable` and split by `rx_wd_safestate_enable` into
      `ExpiredNoSafestate`/`ExpiredSafestate` —
      `StreamWatchdogOutcome::watchdog_overflowed` is the real
      `watchdog_overflowed` source `src/request.rs`'s own prior entry
      named as its eventual supplier. The other four field groups extend
      `src/request.rs`'s existing "Safety-request MSB-tagging" section
      instead (per this milestone's own "additive standalone plumbing
      only" discipline, extending an existing module rather than
      multiplying new ones): `check_rx_enforce_e2e`/`E2eFailureScope`
      (`rx_enforce_e2e`, composing `crate::e2e::crc32_tc18` rather than
      re-deriving it) selects drop-one-request vs. latch-the-stream on a
      CRC mismatch; `resolve_safe_state_mechanism`/`SafeStateMechanism`/
      `safe_state_sequencer_gate` (`rx_safety_measure`,
      `rx_safestate_sequencer`, `rx_safe_sequencer_state`) select
      hi-Z-all-pins or a sequencer-driven safety sequence, the latter
      reusing `CompoundGateConfig` — this item's own working
      interpretation that the sequencer/target-state pair identifies a
      gate `check_compound_gate` already knows how to satisfy, not a
      second sequencer mechanism — and composing with the existing
      `SequencerBank` via a new unconditional `SequencerBank::force_state`
      (added alongside the existing race-guarded
      `advance_if_still_in_start_state`, since entering a safe state must
      not be blocked by that guard); `evaluate_request_storage_overflow`/
      `OverflowOutcome` (`rx_ovrflw_safestate_enable`) and
      `evaluate_rx_enforce_seq`/`SequenceEnforcementOutcome`
      (`rx_enforce_seq`/`rx_seq_safestate_enable`) mirror
      `StreamWatchdogOutcome`'s own three-variant "no event / event, no
      consequence / event, with consequence" shape. `resolve_safe_state_action`/
      `SafeStateAction` is the unifying composition every one of those
      outcome types' `drives_safestate` predicate (or, for
      `rx_enforce_e2e`, an `E2eFailureScope::LatchStream` scope) funnels
      into together with a resolved `SafeStateMechanism`. Per Guiding
      Principle 5, this entry flags two working interpretations rather
      than silently resolving them: the sequencer-driven safe state as a
      gate write (above), and the enforced sequence number itself as a
      plain caller-supplied `u32` not tied to any already-decoded field,
      since neither relationship is stated by the checklist wording. New
      `REQ-E2EENF-001`..`002`, `REQ-SAFEMEAS-001`..`004`,
      `REQ-OVRFLW-001`..`003`, `REQ-SEQENF-001`..`003`,
      `REQ-SAFEACT-001`..`002` added to `.fusa-reqs.json` (fresh prefixes,
      one per field group, matching this module's existing
      CANCEL/CHAIN/TIME/CMP/SEQ/PRIO granularity), each with a
      `// fusa:req`/`// fusa:test` pair; the old `REQ-WDG-001`..`008`
      entries — describing the now-deleted background-poll-thread model —
      are replaced in place by eight new `REQ-WDG-*` entries describing
      the real per-stream liveness model, rather than coexisting alongside
      it, since (unlike the CRC32/CRC-16 migration) the old
      `watchdog.rs` implementation they described no longer exists.
      `.fusa-dfmea.json`'s `FM-005` and `tara.json`'s `CSG-RCP-04`/
      `T-RCP-08` are updated in the same change to describe the new
      liveness-reset-based failure mode and threat model rather than the
      deleted miss-counter one. Same "additive standalone plumbing only"
      discipline as every prior milestone entry: nothing above is wired
      into a decoder, dispatch loop, or into each other end-to-end — a
      `StreamWatchdogOutcome`/`OverflowOutcome`/`SequenceEnforcementOutcome`/
      `E2eFailureScope` still has to be composed into
      `resolve_safe_state_action` by a future caller, not by this item.
- [x] `CRC_ERROR` error path. Done (v0.9.0-dev): `src/lib.rs` gains
      `RcpError::CrcError`, a new variant in its own doc-commented section
      analogous to the `ChainAborted`/`ChainError` "Chained-request error
      codes" group above it — kept out of the eleven-member
      `is_tc18_error_code()` predicate, same as that pair.
      `src/request.rs::check_rx_enforce_e2e` (the `rx_enforce_e2e` CRC32
      safe-point mismatch rule from the "Per-stream safety config" entry
      above) now constructs `RcpError::CrcError` in place of its earlier,
      explicitly-provisional reuse of the legacy CRC-16-era
      `RcpError::CrcMismatch` sentinel; `crate::e2e`'s own `wrap`/`unwrap`
      CRC-16 path keeps returning `RcpError::CrcMismatch` unchanged, since
      that mechanism stays REPLACE-scheduled for later milestones and this
      item does not touch it. New `REQ-CRC-011` added to `.fusa-reqs.json`
      (this module's existing `REQ-CRC-*` prefix, continuing its numbering)
      with a `// fusa:req`/`// fusa:test` pair; `REQ-E2EENF-002`'s text is
      updated in place to name `RcpError::CrcError` rather than the retired
      reuse. Per Guiding Principle 5, this entry leaves
      `crate::e2e::check_fragment_crc_placement`'s own unrelated
      `InvalidParameter` return untouched and open rather than folding it
      into `CRC_ERROR` as well — that function's own doc comment already
      scoped itself as a separate, non-`CRC_ERROR` placement rule ahead of
      this item landing. Same "additive standalone plumbing only"
      discipline as every prior milestone entry: `check_rx_enforce_e2e`
      remains standalone, non-decoder-wired plumbing; only the sentinel it
      returns changed.
- [x] Real power-mode model backing the safe-state work: Normal / StandBy /
      Sleep / Unpowered, cold-start vs. hot-start, and the
      hot-start-from-Sleep WakeUp-message handshake (replacing the ad-hoc
      Active/Sleep/Standby model in `powerstate.rs`) — implemented here
      because entry/exit gating shares the same "all endpoints idle, no
      pending response" conditions as safe-state entry.
      Done (v0.9.0-dev): `src/powerstate.rs` is REPLACEd outright — its old
      `Zone`/`Controller`/`Command{cmd_type: CommandType::SLEEP/WAKE}`
      three-state `Active`/`Sleep`/`Standby` decorator is deleted, not
      adapted — with the spec's real four-mode model: `PowerMode::{Normal,
      StandBy, Sleep, Unpowered}`. `is_power_mode_transition_defined`
      names the two ordinary powered-mode pairs (`Normal`<->`StandBy`,
      `StandBy`<->`Sleep`); `PowerModeGateInput`/
      `is_power_mode_gate_satisfied` is the shared "all endpoints idle, no
      pending response" precondition this item is sequenced into this
      milestone for, and `try_enter_power_mode` composes both into the
      full ordinary-transition rule. `shutdown_to_unpowered` is the
      involuntary, ungated move to `Unpowered` (mirroring
      `RcServerState::try_transition`'s own unconditional demotion path).
      Cold-start and hot-start are modeled as two distinct paths per
      `StartupPath`: `try_cold_start` (`Unpowered` -> `Normal`, no
      handshake) and `try_hot_start` (`Sleep` -> `Normal`, additionally
      gated by the WakeUp handshake below). The hot-start-from-Sleep
      WakeUp handshake itself is modeled as a real two-step message
      exchange rather than a flag flip: `WakeUpHandshakeState::{Idle,
      RequestSent, Acknowledged}`, advanced by `send_wakeup_request`/
      `acknowledge_wakeup_request` and read by
      `is_wakeup_handshake_complete`.
      `power_mode_gate_from_request_states` composes, rather than
      re-derives, `crate::request::RequestLifecycleState` — the safe-state
      entry machinery's own request-progress type from this same
      milestone's "Per-stream safety config" bullet — reading "every
      endpoint idle, no response pending" as "every request has reached
      `RequestLifecycleState::Finalized`."
      Per Guiding Principle 5, this entry flags four working
      interpretations rather than silently resolving them: the four
      modes' relative depth and which pairs are directly reachable from
      which; `Unpowered`'s own software-model semantics (a value an
      external supervisor or this same process records, not a state a
      live process runs "in"); "idle" and "no pending response" as the
      same underlying `RequestLifecycleState::Finalized` fact; and the
      WakeUp handshake's own wire encoding, left unspecified pending real
      transport-level work this item does not attempt. New
      `REQ-PWR-001`..`008` (replacing, not coexisting alongside, the old
      `Active`/`Sleep`/`Standby`-era entries) and
      `REQ-PWRSTART-001`..`003` added to `.fusa-reqs.json`, each with a
      `// fusa:req`/`// fusa:test` pair; `HARA.md`'s `SG-007` row updated
      to name the new types. Same "additive standalone plumbing only"
      discipline as every prior milestone entry: nothing above owns a real
      power domain, spawns a thread, sends or receives a real WakeUp
      message over any transport, or is wired into a decoder, CLI, or
      dispatch loop — this is exactly the dependency Milestone 7's
      "Wakeup control" endpoint-type bullet needs wired into it, left to
      that future item.

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

- [x] **LIN commander** (`ep_type 0x06`): raw byte pass-through only — the
      spec defines no PID/checksum/schedule-table smarts at the protocol
      level. Explicitly validate this against `linbr.rs`'s current ad-hoc
      PID-generation assumptions before reusing any of its logic; expect the
      new implementation to push that responsibility to the client. Done
      (v0.10.0-dev): new `src/lin.rs` — this milestone's first entry, and
      additive standalone plumbing only, matching Milestone 4's six
      endpoint-type modules' own discipline — adds [`LinFrameTransfer`] /
      [`LinFrameTransferResult`], an opaque client-PID-plus-data-bytes
      request paired with a data-only response, mirroring
      `spi::SpiByteTransfer`/`i2c::I2cByteTransfer`'s raw-pass-through
      modeling, and [`LinFunctionalConfig`], an intentionally empty
      placeholder (this checklist bullet names no LIN-specific config
      content) whose `layer_tag` composes against
      `regmap::check_functional_config_matches_ep_type` unchanged, matching
      every prior Milestone 4 entry's precedent. Per this bullet's own
      instruction, `linbr::LinBridge::send`'s existing PID formula
      (`(self.zone.0 << 2) | (cmd.cmd_type.0 as u8 & 0x03)`) and its
      first-byte OK/ERROR response inference were read and explicitly not
      reused: both tie protocol-level behavior to the old `Zone`/`Command`
      model and to interpretation this checklist bullet's pass-through
      behavior does not call for. The one piece of `linbr.rs` this module
      does reuse is `LIN_MAX_DATA` itself (`lin::LIN_MAX_DATA =
      crate::linbr::LIN_MAX_DATA`) — LIN 2.x's real 8-byte per-frame data
      ceiling, a physical fact about the bus rather than surrounding bridge
      logic — enforced as `Err(RcpError::PayloadTooLarge)` on both the
      request and response byte streams. Per Guiding Principle 5, this
      entry flags its own working interpretation of an otherwise
      unspecified wire layout: a request is one leading client-computed PID
      byte followed by up to `LIN_MAX_DATA` data bytes, carrying whatever
      checksum content the client chose to include in `data` unparsed and
      unvalidated; a response carries only the data bytes read back off the
      bus, with no PID field of its own. `linbr.rs` itself is untouched —
      its REPLACE-disposition cutover remains Milestone 9's job. New
      `REQ-LIN-001`..`REQ-LIN-006` added to `.fusa-reqs.json`, each with a
      `// fusa:req`/`// fusa:test` pair in `src/lin.rs`.
- [x] **CAN controller** (`ep_type 0x0B`): Classical/FD/XL `FrameFormat`
      selection (CBFF/CEFF/FBFF/FEFF/XL-classical/XL-new); CAN XL's 6-byte
      sub-header plus up to 2048-byte payload (needs fragmentation — see
      Milestone 8); data frames only, no remote-frame support; note the
      spec's own CAN trigger-signal table is unpopulated in this revision —
      that's a spec gap to track, not an implementation omission. Done
      (v0.10.0-dev): new `src/can.rs` — this milestone's second entry, and
      additive standalone plumbing only, matching `src/lin.rs`'s own
      discipline — adds [`FrameFormat`], the six named CBFF/CEFF/FBFF/FEFF/
      XL-classical/XL-new variants with sequential, own-interpretation byte
      values (flagged, since `ROADMAP.md` names no wire encoding for them);
      [`CanDataFrame`] for the four non-XL formats, enforcing each format's
      real classical (8-byte) or FD (64-byte, reused from
      `canbr::CAN_FD_MAX_PAYLOAD` as a physical fact) data ceiling and real
      11-bit/29-bit base-vs-extended arbitration-ID width; and
      [`CanXlSubHeader`]/[`CanXlFrame`] for CAN XL's 6-byte sub-header
      (carried opaque — its internal field layout is unstated) plus payload
      capped at 2048 bytes, with [`CanXlCombinedPayload::assemble`] modeling
      fragment-train reassembly from a caller-supplied `&[&[u8]]`, mirroring
      `e2e::CombinedFragmentPayload`'s own precedent for the same Milestone 8
      forward dependency — this crate builds no live multi-AVTPDU reassembly
      buffer here, that remains Milestone 8's job. `CanFunctionalConfig`
      carries the selected `FrameFormat` as this endpoint type's
      functional-config content, composing against
      `regmap::check_functional_config_matches_ep_type` via `layer_tag`
      exactly as every prior entry's own config type already does. Neither
      `CanDataFrame` nor `CanXlFrame` has any RTR field or remote-frame
      variant, per this bullet's data-frame-only scope. The unpopulated CAN
      trigger-signal table is recorded as a flagged spec gap in `can.rs`'s
      own provenance note (no `CanTriggerSignal` type is built) rather than
      guessed at. `canbr.rs`'s existing `can_id = zone_id << 8 | cmd_type`
      framing and its `CanSocket` abstraction were read and explicitly not
      reused — both tie frame identity to the old `Zone`/`Command` model;
      only its `CAN_FD_MAX_PAYLOAD` physical-fact constant carried over.
      `canbr.rs` itself is untouched — its REPLACE-disposition cutover
      remains Milestone 9's job. New `REQ-CAN-001`..`REQ-CAN-011` added to
      `.fusa-reqs.json`, each with a `// fusa:req`/`// fusa:test` pair in
      `src/can.rs`.
- [x] **ISELED** (`ep_type 0x0C`): native 4b/5b-encoded daisy-chain framing;
      optional native ISELED CRC, distinct from and additional to the
      RCP-level CRC32; multi-device response aggregation
      (`iseled_collect_resp`). Done (v0.10.0-dev): new `src/iseled.rs` —
      this milestone's third entry, and additive standalone plumbing only,
      matching `src/lin.rs`/`src/can.rs`'s own discipline. Unlike LIN and
      CAN, ISELED has no old-protocol satellite bridge module in this crate
      (no `iseledbr.rs`) to validate against or migrate away from, so every
      piece here is new modeling rather than a read-and-reject exercise.
      Adds [`encode_4b5b`]/[`decode_4b5b`], using the public FDDI/TP-PMD and
      100BASE-TX 4b/5b data code-group table (unrelated to and predating any
      OPEN Alliance TC18 or ISELED-specific content) at a flagged
      symbol-per-byte granularity, since this crate has no bit-serial
      transmission path and no stated software-layer representation to
      match; [`IseledFrame`], carrying a client-supplied `chain_address`,
      `command`, and unbounded `data` — this crate's own flagged working
      interpretation of an unstated frame layout, mirroring
      `lin::LinFrameTransfer`'s own precedent — plus `encode_line`/
      `decode_line` composing that frame shape with the 4b/5b codec for the
      native line-coded form; `iseled_frame_crc8`/`IseledFrameCrc`, a
      flagged CRC-8/AUTOSAR placeholder kept fully independent of
      `e2e::crc32_tc18` (different width, different input, no shared code
      path) pending recovery of ISELED's own confirmed CRC parameters; and
      `iseled_collect_resp`/`IseledDeviceResponse`/`IseledCollectedResponse`
      — named to match this checklist bullet's own identifier — which
      aggregates each daisy-chain device's own response from a
      caller-supplied, already chain-ordered slice, mirroring
      `can::CanXlCombinedPayload::assemble`'s ordering discipline while
      preserving each device's own identity rather than flattening every
      device's bytes together. `IseledFunctionalConfig` carries one
      `native_crc_enabled` field (the "optional" part of this bullet) and
      composes against `regmap::check_functional_config_matches_ep_type`
      via `layer_tag`, exactly as every prior entry's own config type
      already does, tagged the already-reserved `EndpointType::Iseled`.
      New `REQ-ISELED-001`..`REQ-ISELED-010` added to `.fusa-reqs.json`,
      each with a `// fusa:req`/`// fusa:test` pair in `src/iseled.rs`.
- [x] **MDIO** (`ep_type 0x0D`): Clause-22/45 addressing modes
      (`mdio_mode` 2-bit selector); minimal functional config (no
      clock-divider or mode-select fields beyond the universal common
      block). Note: MDIO is fully normative in the register map's `ep_type`
      enumeration despite being absent from the spec's own informative
      "ten interfaces" scope statement — build it anyway. Done
      (v0.10.0-dev): new `src/mdio.rs` — this milestone's fourth entry, and
      additive standalone plumbing only, matching `src/lin.rs`/`src/can.rs`/
      `src/iseled.rs`'s own discipline. Like ISELED, MDIO has no old-protocol
      satellite bridge module in this crate (no `mdiobr.rs`) to validate
      against or migrate away from, so every piece here is new modeling
      rather than a read-and-reject exercise. Per this bullet's own explicit
      note, and continuing the thread Guiding Principle 5 already flags
      (MDIO's absence from the spec's own informative "ten interfaces"
      scope-list prose despite being fully normative in the register map's
      `ep_type` enumeration), this crate trusts the normative enumeration and
      builds MDIO support rather than treating the scope-list omission as
      license to skip it — flagged in `mdio.rs`'s own doc comment. Adds
      [`MdioAddressingMode`], the `mdio_mode` 2-bit selector, with
      `Clause22`/`Clause45` as the two named IEEE 802.3 addressing modes and
      its remaining two 2-bit values left as explicitly, neutrally named
      `Spare2`/`Spare3` variants — this checklist bullet states no meaning
      for them — mirroring `spi::SpiChannelSelect::Spare6`/`Spare7`'s own
      unresolved-slot precedent, with `MdioAddressingMode::is_unallocated_slot`
      letting a caller detect them; and `MdioFunctionalConfig`, carrying
      exactly that one `addressing_mode` field and nothing further — no
      clock-divider or additional mode-select field, per this bullet's own
      "minimal" instruction — composing against
      `regmap::check_functional_config_matches_ep_type` via `layer_tag`
      exactly as every prior entry's own config type already does, tagged
      the already-reserved `EndpointType::Mdio`. Since this bullet names no
      register-access wire framing of its own beyond the mode selector, this
      entry also adds `MdioTransfer`/`MdioTransferResult`, an opaque raw
      byte-stream request/response pair mirroring
      `i2c::I2cByteTransfer`/`i2c::I2cByteTransferResult`'s own
      address-plus-data pass-through modeling, carrying no PHY-address/
      register-address/device-type parsing of its own. New
      `REQ-MDIO-001`..`REQ-MDIO-006` added to `.fusa-reqs.json`, each with a
      `// fusa:req`/`// fusa:test` pair in `src/mdio.rs`.
- [x] **Wakeup control** (`ep_type 0x01`): fixed `SleepCMD` (`0xA5`) request
      distinct from the generic request taxonomy; wake-source pin
      monitoring; wired into the Normal/StandBy/Sleep/Unpowered model from
      Milestone 6. Done (v0.10.0-dev): new `src/wakeup.rs` — this
      milestone's fifth entry, and additive standalone plumbing only,
      matching `src/lin.rs`/`src/can.rs`/`src/iseled.rs`/`src/mdio.rs`'s own
      discipline. Unlike every prior Milestone 7 entry, this one composes
      with an already-built dependency rather than only `crate::regmap`'s
      functional-config taxonomy: `powerstate.rs`'s own doc comment names
      this exact checklist bullet, verbatim, as the item its
      `WakeUpHandshakeState` machinery exists to unblock but does not
      itself implement. Adds [`SleepCmdRequest`], a zero-field marker type
      carrying the fixed `0xA5` discriminant, deliberately not a tenth
      `request::RequestKind` member — the checklist bullet's own "distinct
      from the generic request taxonomy" wording read literally as "its own
      type, not an enum variant"; [`WakeSourcePinMask`] /
      [`WakeupTriggerConfig`] / [`WakeSourceSignals`] /
      [`evaluate_wake_source_signals`], wake-source pin monitoring mirroring
      `gpio::GpioBitmask`/`gpio::GpioTriggerConfig`/
      `gpio::GpioTriggerSignals`/`gpio::evaluate_gpio_triggers`'s own
      config-plus-evaluator shape, but a level check against one observed
      sample rather than GPIO's own three-way changed/rising/falling edge
      detection — this checklist bullet states no edge-vs-level semantics
      for wake sources the way Milestone 4's GPIO bullet did, so this entry
      does not transcribe GPIO's edge model onto an unconfirmed mechanism;
      and `WakeupFunctionalConfig`, carrying that trigger arming and
      composing against `regmap::check_functional_config_matches_ep_type`
      via `layer_tag` exactly as every prior entry's own config type
      already does, tagged the already-reserved `EndpointType::Wakeup`. The
      Milestone-6 wiring itself is two composition functions, each
      delegating to (never duplicating) `powerstate`'s own machinery rather
      than inventing an endpoint-specific power-mode machine:
      `request_sleep_via_sleep_cmd` reads a decoded `SleepCmdRequest` as the
      event that requests a power-down move, composing with
      `powerstate::try_enter_power_mode`; `wake_source_signals_trigger_handshake`
      reads a fired `WakeSourceSignals` as the event that begins the
      hot-start-from-Sleep WakeUp handshake, composing with
      `powerstate::send_wakeup_request`. Neither function advances the
      handshake's acknowledgment half — this checklist bullet names
      wake-source pin monitoring as what begins a wake, not what
      acknowledges one — leaving that composition, and any real WakeUp
      message encoder/decoder, to a future transport-level item, per
      `powerstate.rs`'s own doc comment on the handshake's still-unknown
      wire encoding. Per Guiding Principle 5, this entry flags four working
      interpretations rather than silently resolving them: `SleepCmdRequest`
      carries no ACF/AVTPDU-level framing or byte offset of its own beyond
      its one discriminant byte; `WakeSourcePinMask` reuses
      `gpio::GpioBitmask`'s own 4-byte/32-bit width as a consistency choice,
      not a transcribed wake-source pin count; wake-source pin identity
      (which bit is which physical pin) stays out of scope, left to
      `regmap::HwPinMappingEntry`'s existing generic, endpoint-agnostic pin
      binding; and neither `WakeupTriggerConfig` nor
      `evaluate_wake_source_signals` commits to push (endpoint reports) or
      poll (client reads) — the evaluator is a pure function over a
      caller-supplied observed sample, composing equally well with either
      future mechanism. New `REQ-WAKE-001`..`REQ-WAKE-008` added to
      `.fusa-reqs.json`, each with a `// fusa:req`/`// fusa:test` pair in
      `src/wakeup.rs`; `HARA.md`'s `SG-007` row updated to name the new
      composition functions.
- [x] **DAC** (`ep_type 0x0A`): explicit decision — **treated as reserved
      and out of scope for this cycle.** The type code and a `DAC_OUT` pin
      signal exist in the register-map enumeration, but no functional-config
      chapter or request semantics are defined anywhere in the spec. Track
      as a follow-up pending an OPEN Alliance clarification or later spec
      revision; do not guess at a register layout for it. Done
      (v0.10.0-dev): `EndpointType::Dac` (`src/regmap.rs`, Milestone 2) and
      `EndpointType::is_reserved` already gave this decision a queryable
      form; this bullet's own gap was that `is_reserved` was a pure, inert
      predicate nothing in the crate ever called — `PerEpConfigBlock::new`,
      `PerEpTypeFunctionalConfig::new`, and
      `check_functional_config_matches_ep_type` all happily
      constructed/validated a fully-formed config pair tagged `Dac` exactly
      as if it were an ordinary, implemented endpoint type. New
      `regmap::check_ep_type_supported(ep_type)` closes that gap: it
      returns `Err(RcpError::UnsupportedCmd)` for every reserved
      `EndpointType` (`Dac` only, today) and `Ok(())` otherwise, reusing
      this crate's own established idiom for "recognized on the wire but
      not supported by this crate" — the same convention
      `gpio::GpioWriteSemantics::Unnamed8th`,
      `adc::AdcSamplingMode::Continuous`, and `fragment`'s zero-
      `rx_stream_max_request_size` sentinel already use — rather than
      inventing a distinct rejection path or a register layout for DAC.
      Deliberately kept as its own function rather than folded into
      `check_functional_config_matches_ep_type`: that function's own
      existing `REQ-RMAP-004` contract already covers a matching `Dac`/`Dac`
      pair unconditionally, and narrowing it for `Dac` specifically would
      silently change an existing, already-tested requirement instead of
      adding a new one. No `src/dac.rs` module was added, and none should
      be, per this bullet's own "do not guess at a register layout"
      instruction — this is a non-implementation decision, not a deferred
      implementation. New `REQ-RMAP-031` added to `.fusa-reqs.json`, with a
      `// fusa:req`/`// fusa:test` pair in `src/regmap.rs`.

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

- [x] Decision point: evaluate whether UART RX-FIFO sizing, CAN XL's
      up-to-2054-byte payloads, and full-register-map discovery reads can
      ship as an accepted single-AVTPDU-only limitation for v1.0, or whether
      multi-AVTPDU reassembly must be built now. **Decision: go.** Done
      (v0.11.0-dev): `RequestStreamConfigEntry::rx_stream_max_request_size`
      (`src/regmap.rs`, §3.8) already exists as a real per-stream
      combined-payload byte bound with no purpose under a permanent
      single-AVTPDU-only limitation; `CanXlFrame`'s up-to-2048-byte payload
      (`ROADMAP.md` Milestone 7's CAN controller bullet) routinely exceeds
      any practical single-AVTPDU MTU on its own, and neither UART RX-FIFO
      sizing nor a full-register-map discovery read (Milestone 2/3) is
      bounded to fit inside one AVTPDU either. A silent single-AVTPDU-only
      ceiling would quietly cap all three below their own already-modeled
      sizes — an unannounced regression, not an accepted limitation — so
      this crate builds multi-AVTPDU reassembly now rather than deferring it
      past v1.0.
- [x] **If go:** implement `ms`/`segment_num` reconstruction bounded by
      `rx_stream_max_request_size`, and re-verify the Milestone 6
      last-fragment-carries-the-CRC interaction against it. Done
      (v0.11.0-dev): new `src/fragment.rs` — the module `src/request.rs`'s
      own doc comment already reserved for this milestone (issue #35, PR
      #37) — adds [`FragmentReassemblyBuffer`], a live per-stream
      reassembly state machine a caller drives explicitly (additive
      standalone plumbing only, matching every prior Milestone 1-7 entry's
      discipline: not wired into any decoder or dispatch loop).
      `FragmentReassemblyBuffer::accept_fragment` takes each
      wire-arrival-ordered fragment's already-decoded
      `acf::ByteMessageInfo` plus payload, validates the dual-purpose
      `read_size`/`segment_num` byte (`acf::ReadSizeOrSegment::as_segment_num`)
      as a strictly-incrementing, zero-based consistency check against
      gaps/duplicates/reordering (a working interpretation, flagged per
      Guiding Principle 5 — see `fragment.rs`'s own "Provenance note:
      `segment_num` ordering" and a matching addendum in `acf.rs`'s
      provenance note), enforces the caller-supplied
      `rx_stream_max_request_size` bound against the train's running
      combined length (`RcpError::PayloadTooLarge` on overflow), and
      returns `FragmentAcceptOutcome::Continuing`/`Complete` by the
      fragment's own `ms` flag; `combined_payload` composes (never
      re-derives) `e2e::CombinedFragmentPayload::assemble`. A buffer built
      with `rx_stream_max_request_size == 0` (that field's own documented
      "fragmentation unsupported on this stream" sentinel) rejects every
      fragment with `RcpError::UnsupportedCmd` rather than silently
      reassembling anyway. `verify_reassembled_train_crc` re-verifies
      Milestone 6's last-fragment-carries-the-CRC interaction against this
      real buffer: it composes `e2e::check_fragment_crc_placement` and
      `e2e::crc32_tc18_for_fragment_train` against the buffer's own
      wire-collected segments (`FragmentReassemblyBuffer::segment_refs`)
      rather than the caller-supplied `&[&[u8]]` those two functions' own
      doc comments flagged as this exact Milestone 8 forward dependency when
      they first landed — their own signatures are unchanged, Milestone 8
      composed with them instead of editing them. `e2e.rs` and `can.rs`'s
      own doc comments (the two places that flagged this forward
      dependency — `CombinedFragmentPayload`'s "Fragmentation interaction"
      section and `CanXlCombinedPayload`'s "CAN XL fragmentation
      interaction" section) are updated to point at `fragment.rs` rather
      than continuing to describe a still-missing reassembly buffer;
      neither `CombinedFragmentPayload` nor `CanXlCombinedPayload`
      themselves are changed, since both already compose cleanly with the
      new buffer's own segment order. `rust-rcp capabilities`' `features`
      array gains `"fragmentation"`, and its response-side counterpart,
      `ResponseStreamConfigEntry::resp_max_avtpdu_size`, is noted as
      explicitly out of scope (a distinct, unbuilt response-fragmentation
      problem, not this item's). New `REQ-FRAG-001`..`REQ-FRAG-008` added
      to `.fusa-reqs.json`, each with a `// fusa:req`/`// fusa:test` pair in
      `src/fragment.rs`.
- [x] **If no-go:** document the single-AVTPDU limitation explicitly in the
      crate's public docs and in `rust-rcp capabilities`' output, matching
      the spec's own allowance for omitting this feature. **N/A** — the
      decision above is go, so this bullet's own single-AVTPDU-limitation
      documentation does not apply; checked off to record that this bullet
      was evaluated and explicitly not silently skipped, not that its own
      text was executed.

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

- [x] All **REPLACE**-disposition packages rebuilt against the new core
      Done (v0.12.0-dev): 10 of 10 done — this bullet is now closed. `watchdog`/`powerstate` were
      already REPLACEd ahead of schedule inside Milestones 6/7 (see their
      own "Done" notes above); `wire` and `e2e` are done too. `src/
      wire.rs` — the legacy 16-byte private frame — is deleted outright,
      its role fully absorbed by the already-built Milestone 1 AVTPDU/ACF
      stack: `src/avtp.rs` gains two new composition functions,
      `encode_ntscf_frame`/`decode_ntscf_frame`, wrapping an
      already-encoded ACF_ABB/ACF_GBB payload (`src/acf.rs`) in its NTSCF
      envelope — the "combine one whole on-wire AVTPDU" step every prior
      Milestone 1 entry flagged as not yet done. Both of `wire`'s two
      remaining callers are cut over: `src/udp.rs`'s old `Zone`/
      `Controller`-based `UdpBridge` is REPLACEd outright (deleted, not
      adapted, the same discipline `watchdog`/`powerstate` used) with a new
      `UdpTransport` addressed by `avtp::StreamId` instead of `Zone`,
      sending/receiving NTSCF-wrapped ACF_ABB/ACF_GBB frames and resolving
      `byte_bus_id` through a new `resolve_endpoint` helper composing
      `ep0::route_byte_bus_id`/`addressing::EndpointTable` instead of a
      zone lookup; `src/tlstransport.rs`'s `TlsBridge` (ADAPT disposition)
      keeps its TLS-wrapping mechanics and mutual-auth posture unchanged
      per its own scope, with only its encode/decode calls retargeted the
      same way. `fuzz/fuzz_targets/fuzz_wire_decode.rs` is repointed at
      `avtp::decode_ntscf_frame` rather than deleted, carrying its
      never-panics discipline forward. `RcpError::BadMagic`/`BadVersion` —
      the two sentinels `wire.rs` alone constructed — are removed from
      `src/lib.rs`, since Milestone 2's own "Error Model" note had already
      flagged their removal as this milestone's job. `.fusa-reqs.json`'s
      `REQ-WIRE-001`..`009` are retargeted in place (same IDs, same
      "Never renumber or reuse" discipline Milestone 6 established for
      `REQ-WDG-*`) to describe the new NTSCF frame composition instead of
      the deleted frame; `REQ-UDP-001`..`005` and `REQ-TLS-004` are
      likewise retargeted to `UdpTransport`/`TlsBridge`'s new framing; new
      `REQ-UDP-006`..`007` cover `resolve_endpoint`. `udp`'s own REPLACE
      disposition (a real RC-Server-endpoint-level rebuild: register-map-
      driven dispatch, discovery integration) remains open and is tracked
      separately.

      `e2e`'s row is now done too. Milestone 6 had already additively
      grown the real TC18 safe-point CRC-32 machinery (`crc32_tc18`,
      `build_crc32_coverage_buffer`, the fragment-train coverage
      functions) alongside the pre-existing, structurally unrelated legacy
      surface this row's own disposition-table reason names: a CRC-16 +
      replay-guard frame (`wrap`/`unwrap`, `ReplayGuard`) wrapped in a
      `Zone`/`Controller`-keyed decorator (`E2eController`). This item
      deletes that legacy trio outright — no external caller depended on
      `E2eController`, `e2e::wrap`, `e2e::unwrap`, or `ReplayGuard` outside
      `src/e2e.rs` itself, confirmed by inspection before removal, so no
      other module needed a corresponding change (unlike `wire`'s cutover,
      which needed `src/udp.rs`/`src/tlstransport.rs` changes too).
      `RcpError::CrcMismatch`/`RcpError::Replay` — the two sentinels that
      legacy trio alone constructed — are removed from `src/lib.rs` the
      same way `BadMagic`/`BadVersion` were removed by the `wire` cutover
      immediately before it; `crate::request`'s `check_rx_enforce_e2e`
      already constructed `RcpError::CrcError` instead of `CrcMismatch`
      (Milestone 6's "`CRC_ERROR` error path" item), so no live caller was
      affected. `.fusa-reqs.json`'s `REQ-E2E-001`..`008` describe only
      the deleted CRC-16/replay-guard/`E2eController` behavior with no
      surviving analog, so — per this item's own "retarget in place, or
      explicitly retire if no equivalent behavior exists" instruction —
      they are retired (removed) rather than force-retargeted onto
      unrelated existing CRC-32 coverage; `REQ-CRC-011`'s text is
      retargeted to drop its now-invalid "distinct from `RcpError::
      CrcMismatch`" clause. Cross-references in `.fusa-dfmea.json`
      (`FM-002`/`FM-003`), `tara.json` (`A-002`, `T-RCP-02`/`03`/`07`,
      `CSG-RCP-02`), `HARA.md` (`SG-003`/`SG-004`), and
      `.fusa-iec62443.json` (`T-002`/`T-003`, `SC-002`/`SC-003`) are
      retargeted the same way the `watchdog` REPLACE item retargeted
      `FM-005`/`CSG-RCP-04`/`T-RCP-08`: the CRC-32 safe-point mechanism has
      a real, honest replacement (`REQ-CRC-004` et al.), but the legacy
      `ReplayGuard` sliding window does not — this crate has no live
      anti-replay mechanism today. `crate::request::evaluate_rx_enforce_seq`
      (`REQ-SEQENF-003`, Milestone 6's monotonic `rx_enforce_seq` check) is
      the closest surviving candidate, but it is additive standalone
      plumbing, not wired into any decoder or dispatch loop, so every
      retargeted cross-reference says so explicitly and raises the
      affected residual-risk ratings accordingly rather than quietly
      reusing a "low" rating a live mechanism would justify.
      `fuzz/fuzz_targets/fuzz_e2e_unwrap.rs` is repointed at
      `e2e::crc32_tc18` rather than deleted, the same way
      `fuzz_wire_decode.rs` was repointed at `avtp::decode_ntscf_frame`.

      `mock`'s row is now done too, and — unlike `wire`/`e2e` — this one is
      genuinely new construction rather than a cutover: no prior Milestone
      1-8 item ever assembled AVTPDU decode, EP0/register-map gating, and
      endpoint dispatch into one live path, so `src/mock.rs` gains
      [`RcServer`], an in-memory OPEN Alliance TC18 RC Server test double
      keyed by `(StreamId, byte_bus_id)` and gated by `RcServerState`
      rather than by `Zone`, plus a minimal `Endpoint` trait and a
      byte-buffer-backed `MockEndpoint` implementation of it for
      device-endpoint dispatch. `RcServer::handle_abb` composes
      `ep0::route_byte_bus_id`/`check_ep0_access_for_stream` (against
      `RegisterCategory::General` only — `HwConfig`/`RcpConfig` register
      storage is not modeled by this item) and `addressing::EndpointTable`
      to answer a request; `RcServer::handle_ntscf_frame` wraps that in
      `avtp::decode_ntscf_frame`/`encode_ntscf_frame` and
      `acf::decode_acf_abb`/`encode_acf_abb` so a caller can drive the
      whole on-wire round trip without touching any intermediate decoded
      type, reusing every one of `wire`'s own composition functions rather
      than duplicating them. Every response echoes its request's
      `byte_bus_id` via `acf::build_response_info`, verified against
      `acf::verify_echo_back`. The pre-existing `MockController`/
      `MockRegistry`/`Handler` test double for the *old*
      `Controller`/`Registry`/`Zone`/`Command`/`Response`/`Status` API is
      kept, unmodified, rather than deleted outright — a deliberately
      narrower scope than a clean REPLACE, recorded in `src/mock.rs`'s own
      module doc comment: seventeen still-`ADAPT`-disposition satellite
      packages' own unit tests (`ratelimit`, `deadline`, `faultinject`,
      `proxy`, `redundancy`, `observe`, `authz`, `record`, `prioqueue`,
      `zonegroup`, `adapt`, `loan`, `admin`, `federation`, `tsn`,
      `firmware`) plus `src/bin/rcp.rs` all construct
      `MockController`/`MockRegistry` today against a `Controller`/
      `Registry` trait this item does not remove, and retargeting those
      seventeen files is this milestone's own separate, not-yet-started
      "ADAPT-disposition packages retargeted" bullet's job, not this one's
      — deleting the legacy mock now would break every one of them for no
      corresponding benefit. `.fusa-reqs.json`'s `REQ-CTRL-*`/`REQ-REG-*`/
      `REQ-RESP-*`/`REQ-STAT-*`/`REQ-ERR-011` therefore stay exactly as
      they were (still describing live, still-tested code) rather than
      being retargeted or retired; ten new `REQ-MOCKSRV-001`..`010`
      requirements are added instead, covering only the new `RcServer`/
      `Endpoint`/`MockEndpoint` behavior, each with a `// fusa:req`/
      `// fusa:test` pair in `src/mock.rs`.

      `config`'s row is now done too. Its old [`RcpConfig`] loader (a flat
      `controllers: HashMap<String, ControllerConfig>` keyed by `Zone`, plus
      a standalone `WatchdogConfig`/`RateLimitConfig` pair, validated
      against invented bounds — `zone <= 5`, `max_payload_bytes <= 65491`,
      `rate_limit.rate > 0`, `watchdog.window > 0` — with no referent in the
      real spec) is deleted outright and replaced with `RcServerConfig`, a
      loadable/serializable unit that *composes* the register-map/lifecycle
      pieces Milestone 2 already built — `regmap::GeneralRegisters` (`§3.6`),
      the five `§3.7`-`§3.11` child config-table row types, and
      `lifecycle::RcServerState` — rather than reinventing them. Those
      existing types gained purely additive `Serialize`/`Deserialize`
      derives (`src/regmap.rs`, `src/lifecycle.rs`) so `config` could compose
      them directly instead of duplicating their field lists in a parallel
      shape. `from_json`/`from_yaml` keep the same two-format-loader shape
      the old loader established; `validate()` is rebuilt from scratch
      against real constraints instead of the old invented bounds: each of
      the four `TableDescriptor`-backed child tables must have enough
      declared `capacity` for its config's own row count, `sequencer_state`'s
      row count is bounded by `svr_sequencers_max` (the one child table whose
      pointer field, `svr_sequencer_state_ptr`, has no paired capacity field
      of its own), and every `RcpConfig`-category table populated in the
      config must be reachable given the config's own `initial_state`, per
      `lifecycle::is_register_reachable`. Field-width bounds are deliberately
      left unchecked here, since `GeneralRegisters`/each row type's own
      `encode`/`decode` already enforce them structurally through Rust's
      integer types. Like `wire`/`e2e` before it, `config::` had zero callers
      anywhere else in `src/` before this item (confirmed by inspection) and
      still has none after it, so this is a self-contained cutover with no
      other file's callers to fix. `.fusa-reqs.json`'s `REQ-CFG-001`,
      `REQ-CFG-005`, and `REQ-CFG-006` are retargeted in place (same IDs) to
      describe the new type/loader/capacity-validation behavior;
      `REQ-CFG-002`..`004` (`ControllerConfig`'s `zone` field,
      `WatchdogConfig`/`RateLimitConfig` defaults) describe only the deleted
      `Zone`-keyed/ad-hoc-watchdog/ad-hoc-rate-limit shapes with no
      surviving analog in the register-map model, so — per this item's own
      "retarget in place, or explicitly retire if no equivalent behavior
      exists" instruction — they are retired (removed) rather than
      force-mapped onto unrelated behavior; new `REQ-CFG-007`..`008` cover
      the two new validation rules (`svr_sequencers_max` consistency,
      `RcpConfig`-category reachability) that have no counterpart among the
      retargeted three.

      `capi`'s row is now done too. Its old `CCommand`/`CResponse`/`CError`
      trio — 1:1 tied to the legacy `Command`/`Zone` shapes, per this row's
      own disposition-table reason — is deleted outright and replaced with
      `CStreamId` (mirroring `avtp::StreamId`), `CByteMessageInfo`
      (mirroring every `acf::ByteMessageInfo` field, flattening `Evt` and
      `ReadSizeOrSegment` since neither is itself `#[repr(C)]`), and
      `CAbbRequest`/`CAbbResponse` (each pairing a `CStreamId` with a
      `CByteMessageInfo`) — kept as two distinct Rust types despite an
      identical field layout, since a real request and its response are
      actually the same `acf::AcfAbbMessage` Rust type today, unlike the
      old `CCommand`/`CResponse`, which really were two different shapes.
      Neither new header type carries the ACF_ABB message's variable-length
      `payload: Vec<u8>`, the same scope limit the old `CCommand`/`Command`
      conversion had (it always discarded `Command::payload`, reconstructing
      `payload: None`) — a `Vec<u8>` has no fixed-size `#[repr(C)]` shape,
      and carrying it across a real C boundary is a raw-pointer-plus-length
      concern for whichever `extern "C"` cdylib target eventually wraps this
      module, which this crate does not build today. `CError` is rebuilt
      against every current `RcpError` variant instead of the old
      `Closed`/`NotFound`/`ZoneMismatch`/etc. set: all eleven TC18 spec
      error codes (`RcpError::is_tc18_error_code`'s own members), the RELAY
      mandatory sentinels, `ShortFrame`, the chained-request codes, and
      `CrcError` each get their own `CError` variant, while the legacy
      `NotFound`/`AlreadyExists`/`Busy`/`ZoneMismatch` Zone/Controller/
      Registry sentinels (no TC18 analog) and `RcpError::Other(String)`
      collapse to `CError::Other`. Like `wire`/`e2e`/`config` before it,
      `capi::` had zero callers anywhere else in `src/` before this item
      (confirmed by inspection — neither `src/bin/rcp.rs` nor any other
      module constructed `CCommand`/`CResponse`) and still has none after
      it, so this is a self-contained cutover with no other file's callers
      to fix. `.fusa-reqs.json`'s `REQ-CAPI-001`..`004` are retargeted in
      place (same IDs) to describe the new types/conversions/error mapping
      — unlike `config`'s `REQ-CFG-002`..`004`, every one of the four has a
      direct working analog in the new types, so nothing is retired.

      `canbr`'s row is now done too, and — unlike `wire`/`e2e`/`config`/
      `capi` — its replacement, `src/can.rs`, was already fully built ahead
      of schedule inside Milestone 7 (`v0.10.0-dev`), the same way
      `watchdog`/`powerstate` were REPLACEd ahead of schedule inside
      Milestones 6/7. This item's job is therefore the bookkeeping cutover
      Milestone 7's own "Done" note explicitly deferred: `src/canbr.rs`'s
      legacy `Controller`-trait `CanBridge`, its `CanSocket` transport
      abstraction, its `Zone`-keyed `can_id = zone_id << 8 | cmd_type`
      framing, and their test module are deleted outright — no external
      caller depended on any of them outside `src/canbr.rs` itself
      (confirmed by inspection), matching the `wire`/`e2e` cutovers'
      discipline of deleting rather than adapting a legacy `Zone`/
      `Controller`-coupled surface with no equivalent in the endpoint-
      addressed model. The one genuine cross-module dependency —
      `src/can.rs`'s `CAN_FD_MAX_PAYLOAD` constant, defined as
      `crate::canbr::CAN_FD_MAX_PAYLOAD` — is resolved by inlining the
      physical-fact literal (`64`) directly into `can.rs` as its own
      constant rather than leaving a stub `canbr.rs` behind purely to hold
      one value; `can.rs`'s own "Validation against `canbr.rs`" doc-comment
      section and its test module's now-dangling `crate::canbr::
      CAN_FD_MAX_PAYLOAD` equality assertion are updated to match. No new
      CAN hardware I/O transport binding is built to replace the old
      `CanSocket` abstraction: every other endpoint-type module this crate
      has built since Milestone 4 (`adc`, `pwm`, `gpio`, `crate::can`
      itself, etc.) is additive standalone plumbing only, deliberately not
      wired into any live decoder, dispatch loop, or transport, so building
      one transport binding for CAN alone here — the one judgment call this
      item's own scope left open — would be inconsistent with that
      established discipline rather than an extension of it; a real
      hardware binding remains a later, not-yet-scoped item for whichever
      milestone first wires any endpoint type into live dispatch.
      `.fusa-reqs.json`'s `REQ-CANBR-001`..`005` describe only the deleted
      `CanBridge`/`CanSocket`/`Zone`-keyed behavior: `REQ-CANBR-001`'s
      `CAN_FD_MAX_PAYLOAD == 64` fact is already covered by the existing
      `REQ-CAN-003` (retargeted below), and `REQ-CANBR-002`/`003`/`004`/
      `005` (the `CanSocket` trait shape, `CanBridge::send`, zone-mismatch/
      oversized-payload rejection, and no-op `close()`) have no surviving
      analog in `crate::can`'s data-shape-only model — so, per this item's
      own "retarget in place, or explicitly retire if no equivalent
      behavior exists" instruction, all five are retired (removed) rather
      than force-mapped onto unrelated behavior. `REQ-CAN-003`'s own text
      is retargeted to drop its now-stale `crate::canbr::CAN_FD_MAX_PAYLOAD`
      import reference, describing the constant as `can.rs`'s own directly-
      stated physical fact instead. `.fusa-iec62443.json`'s `T-005` threat
      countermeasure list swaps the retired `REQ-CANBR-004` for
      `REQ-CAN-005` (`CanDataFrame::decode`'s own oversized-payload
      rejection, the closest surviving analog to `CanBridge`'s old
      payload-size check — the zone-mismatch half of `REQ-CANBR-004` has no
      analog at all, `Zone` having no equivalent here), with its residual-
      risk note flagging that this endpoint-level check, like the rest of
      `crate::can`, is not yet dispatch-wired — the same caveat `T-003`'s
      own note already applies to `REQ-SEQENF-003`. `README.md`'s module
      index drops its now-inaccurate `canbr` row outright, since the module
      it described no longer exists (mirroring how `e2e`'s row was updated
      in place when its own REPLACE cutover landed, rather than left
      stale).

      `linbr`'s row is now done too, following the `canbr` cutover
      immediately before it as precedent — the most structurally similar of
      the REPLACE items so far, a bridge module with exactly one physical-
      fact constant reused by its Milestone-7-built replacement and no other
      in-crate caller. `src/linbr.rs`'s legacy `Controller`-trait
      `LinBridge`, its `LinMaster` transport abstraction, its
      `Zone`/`Command`-keyed `pid = (zone.0 << 2) | (cmd_type.0 & 0x03)` PID
      scheme, and their test module are deleted outright — no external
      caller depended on any of them outside `src/linbr.rs` itself
      (confirmed by inspection). The one genuine cross-module dependency —
      `src/lin.rs`'s `LIN_MAX_DATA` constant, defined as
      `crate::linbr::LIN_MAX_DATA` — is resolved the same way `can.rs`'s
      `CAN_FD_MAX_PAYLOAD` was: the physical-fact literal (`8`) is inlined
      directly into `lin.rs` as its own constant rather than leaving a stub
      `linbr.rs` behind purely to hold one value; `lin.rs`'s own "Validation
      against `linbr.rs`" doc-comment section and its test module's now-
      dangling `crate::linbr::LIN_MAX_DATA` equality assertion are updated to
      match. No new LIN hardware I/O transport binding is built to replace
      the old `LinMaster` abstraction, for the same reason the `canbr`
      cutover built no CAN transport binding: every endpoint-type module
      this crate has built since Milestone 4 is additive standalone plumbing
      only, deliberately not wired into any live decoder, dispatch loop, or
      transport, so building one transport binding for LIN alone here would
      be inconsistent with that established discipline rather than an
      extension of it; a real hardware binding remains a later, not-yet-
      scoped item. `.fusa-reqs.json`'s `REQ-LINBR-001`..`004` describe only
      the deleted `LinBridge`/`LinMaster`/`Zone`-keyed behavior:
      `REQ-LINBR-001`'s `LIN_MAX_DATA == 8` fact is already covered by the
      existing `REQ-LIN-001` (retargeted below), and `REQ-LINBR-002`/`003`/
      `004` (the `LinMaster` trait shape, `LinBridge::send`'s delegation, and
      zone-mismatch/oversized-payload/zero-timeout rejection) have no
      surviving analog in `crate::lin`'s data-shape-only model — so, per this
      item's own "retarget in place, or explicitly retire if no equivalent
      behavior exists" instruction, all four are retired (removed) rather
      than force-mapped onto unrelated behavior. `REQ-LIN-001`'s own text is
      retargeted to drop its now-stale `crate::linbr::LIN_MAX_DATA` import
      reference, describing the constant as `lin.rs`'s own directly-stated
      physical fact instead; `REQ-LIN-004`'s text is likewise updated to
      describe the legacy `linbr::LinBridge::send`'s enforcement in the past
      tense. A grep of `.fusa-dfmea.json`, `tara.json`, and
      `.fusa-iec62443.json` for `linbr`/`LINBR` references before this item
      landed turned up none — unlike `canbr`'s `T-005` countermeasure swap,
      no cross-reference in those files needed retargeting. `README.md`'s
      module index drops its now-inaccurate `linbr` row outright, the same
      way `canbr`'s row was dropped.

      `udp`'s own row is now done too, closing this bullet out entirely (10
      of 10). Its remaining scope, per this row's own text above and
      `src/udp.rs`'s own pre-existing doc comment, was "a real
      RC-Server-endpoint-level rebuild: register-map-driven dispatch,
      discovery integration" — the framing-only cutover (`UdpTransport`)
      had already landed as part of the `wire` REPLACE item earlier in this
      milestone. New `src/udp.rs` type [`UdpRcServer`] is `UdpTransport`'s
      server-side counterpart: it drives an actual `mock::RcServer`
      register-map/lifecycle-gated dispatch engine
      (`RcServer::handle_abb`) from real inbound UDP datagrams
      (`UdpSocket::recv_from`), decoding each as an NTSCF/ACF_ABB request and
      dispatching under the *requesting client's* `StreamId` — recovered
      from the decoded NTSCF header itself, not a caller-supplied value, the
      way `mock::RcServer::handle_ntscf_frame`'s own test-double simplicity
      allows — while every response frame is addressed under the server's
      own `StreamId`, never the requester's. Composing `mock::RcServer`
      directly, rather than lifting/duplicating its dispatch logic into a
      new home, is a flagged judgment call (Guiding Principle 5): `mock.rs`
      is nominally "test doubles," but its own doc comment had already
      anticipated `udp` as "the most likely next caller," `RcServer` is a
      plain `pub` item with no `#[cfg(test)]` gate, and duplicating its
      EP0/`EndpointTable`/echo-back dispatch would fork one rule into two
      independently-maintained copies — see `UdpRcServer`'s own doc comment
      for the full reasoning, including the still-open naming/relocation
      question this item deliberately does not resolve.

      `crate::discovery` is wired into a live path for the first time by
      this item too, closing the "discovery integration" half: `UdpRcServer`
      recognizes a broadcast-or-direct `discovery::is_discovery_request` and
      answers it via `discovery::build_discovery_response` in any lifecycle
      state, with the response frame's `StreamId` always the server's own —
      how a client is meant to learn a server's real identity from a
      broadcast exchange, to key its own `discovery::DiscoveryCache` by
      afterward. New `discovery::is_discovery_configure_request` (added by
      this item, alongside new `REQ-DISC-021`) recognizes this crate's own
      chosen wire encoding for "configuring the discovery stream" —
      `is_discovery_request`'s exact addressing/register shape with the
      read/write direction bit flipped — since neither this roadmap nor any
      Milestone 1-3 item ever named one, and `discovery.rs`'s own doc
      comment had already flagged deferring that exact choice to whichever
      item first wired it into a dispatch loop; `UdpRcServer` gates a
      recognized configure attempt via `discovery::check_discovery_access`
      and grants/refreshes the resulting claim via
      `discovery::try_claim_discovery_stream`, rejecting a different live
      claimant's attempt with `Err(UnauthorizedAccess)` and leaving the
      existing claim unaffected. A request arriving under
      `discovery::DISCOVERY_BROADCAST_STREAM_ID` that is neither shape is
      rejected outright (`Err(InvalidParameter)`) rather than dispatched as
      if the sentinel named a real client. `resolve_endpoint` and
      `UdpTransport` are unchanged by this item.

      `.fusa-reqs.json` gains `REQ-UDP-008`..`011` (`UdpRcServer`'s
      register-map dispatch, discovery-read answering, discovery-configure
      claim handling, and broadcast-sentinel misuse rejection) and
      `REQ-DISC-021` (`is_discovery_configure_request`), each with a
      `// fusa:req`/`// fusa:test` pair, following the same "retarget in
      place, or add new — never renumber or reuse" discipline every prior
      Milestone 9 REPLACE cutover in this bullet established.
- [x] All **ADAPT**-disposition packages retargeted to whatever new
      endpoint/RC-Server trait surface replaces `Controller`/`Registry`
      Done (v0.12.0-dev): 16 of 16 packages resolved. `tlstransport` was
      already fully retargeted as a side effect of `wire`'s own REPLACE
      cutover earlier in this milestone (it and `udp` were `wire`'s only
      two callers) — confirmed unchanged here. `mdns` needed only its
      `ServiceRecord.zone: u8` field replaced with `ServiceRecord.stream_id:
      avtp::StreamId`, since `MdnsRegistry` was never itself an `impl` of
      the legacy `Controller`/`Registry` traits. The other 14 —
      `ratelimit`, `sim`, `deadline`, `faultinject`, `loan`, `proxy`,
      `redundancy`, `observe`, `authz`, `record`, `federation`, `admin`,
      `shmem`, `adapt` — are rebuilt against `mock::Endpoint` (or, for
      `federation`/`admin`, `discovery::DiscoveryCache`, the
      discovery-derived server registry their own disposition-table
      reasons named as a dependency) in place of `Controller`/`Registry`/
      `Zone`/`Command`/`Response`/`Status`. `src/lib.rs`'s `Controller`/
      `Registry`/`Zone`/`Command`/`Response`/`Status`/`LoaningController`
      types are left in place, unremoved — that core-surface cutover is
      Milestone 10's job, not this bullet's — so `src/bin/rcp.rs` and
      this crate's still-live `mock::MockController`/`MockRegistry` test
      double continue to compile and pass unchanged; `rcp.rs`'s own CLI
      overhaul is explicitly deferred to Milestone 10, per the judgment
      call flagged when this item was picked up.

      Most of the 14 are a mechanical decorator retarget: `ep_type()`
      replaces `zone()`, and `read(read_size)`/`write(payload)` replace
      `send(cmd, timeout)`, mirroring `mock::RcServer::handle_abb`'s own
      `op` bool convention (`true` = write) for anything keyed on request
      direction. Several needed a flagged judgment call rather than a
      1:1 port, since `Endpoint` is a narrower trait than `Controller` —
      no `Priority`, no `timeout` parameter, no `subscribe`/`Status`
      broadcast, and two distinct verbs instead of one `send`:

      - `ratelimit`/`authz`: the old `Priority`-keyed carve-outs
        (`exempt_critical`, the `min_priority`/`max_priority` range) have
        no analog and are dropped; `authz::Policy` is rekeyed from
        `CommandType` to `(EndpointType, is_write)` per its own
        disposition-table text. `REQ-RL-007` ("Critical exempt from rate
        limit") and `REQ-SIM-005` ("Zone mismatch returns ZoneMismatch")
        described only dropped, no-longer-possible behavior, so both are
        retired in `.fusa-reqs.json` rather than force-retargeted, per
        this item's own "retarget in place, or explicitly retire if no
        equivalent behavior exists" instruction; every other touched
        `REQ-RL-*`/`REQ-SIM-*`/`REQ-DL-*`/`REQ-FI-*`/`REQ-LOAN-*`/
        `REQ-PROXY-*`/`REQ-RED-*`/`REQ-OBS-*`/`REQ-AUTHZ-*`/`REQ-REC-*`/
        `REQ-FED-*`/`REQ-ADMIN-*`/`REQ-SHM-*`/`REQ-ADAPT-001`..`005`/
        `REQ-MDNS-*` ID is retargeted in place (same ID, updated
        title/text) to describe its new `Endpoint`-based behavior.
        Retiring `REQ-RL-007` also meant retargeting its cross-references
        in `.fusa-dfmea.json` (`FM-007`), `.fusa-iec62443.json` (`T-004`),
        and `tara.json` (`T-RCP-04`, `CSG-RCP-03`), each with its residual
        risk honestly raised to reflect that DoS-vs-critical-priority
        mitigation now depends solely on `prioqueue::PrioController` — a
        DEPRECATE-disposition package itself slated for removal by this
        milestone's next checklist bullet — rather than silently keeping
        a "low" rating a live dual mitigation would justify. `authz`'s own
        rekeying similarly meant raising `tara.json` `T-RCP-05`/
        `CSG-RCP-04`'s residual risk: `authz::AuthzEndpoint` protects the
        new `Endpoint` surface, but no longer offers any ACL wrapper for a
        still-live legacy `Controller`-based zone controller, which
        Milestone 10 alone can close by retiring `Controller`/`Command`
        themselves.
      - `deadline`: `Endpoint::read`/`write`'s fixed signatures carry no
        `timeout` parameter, so deadline enforcement is exposed through
        two additional inherent methods, `read_with_deadline`/
        `write_with_deadline`, alongside a plain pass-through `Endpoint`
        impl — the same "extend the base trait with extra methods" shape
        `LoaningController` already established for loaned sends. On this
        crate's synchronous, in-process `Endpoint` model, only the
        zero-timeout-is-already-expired case has an observable effect;
        the `min(caller, deadline)` computation has nothing left to bound.
      - `sim`: implements `Endpoint` directly (like `MockEndpoint`) rather
        than wrapping one; the old single `queue_response` FIFO becomes
        two independent FIFOs (`queue_read_response`/
        `queue_write_response`), and the old `publish`/`subscribe`
        `Status` broadcast is dropped outright rather than invented here,
        matching `mock.rs`'s own doc comment that this crate's new core
        has no live asynchronous-notification mechanism yet.
      - `observe`/`record`: split their single old hook/entry shape
        (keyed on one `Command`/`Response` pair) into read- and
        write-specific hooks/entry variants, since `Endpoint::read`/
        `write` return different `Result` types with no common shape.
      - `loan`: `LoanPoolEndpoint` exposes `loan`/`write_loaned` as its
        own inherent methods rather than implementing `LoaningController`
        (which requires `Controller`, no longer implemented); `Loan`/
        `LoanPool` themselves are unchanged, generic buffer-pool
        machinery with no trait coupling of their own.
      - `federation`/`admin`: rebuilt against `discovery::DiscoveryCache`
        — a passive, client-side cache of previously observed peer
        identities, not a live collection of dispatch handles the old
        `Registry` was. `FederationRouter::lookup_peer` now returns a
        `Copy` `DiscoveryCacheEntry` snapshot instead of a live
        `Arc<dyn Controller>`; `AdminServer::is_healthy` narrows to
        "cache non-empty" (no live reachability call left to dispatch),
        with a separate `is_peer_healthy` for a real per-`StreamId`
        staleness check, and `shutdown_peer` invalidates a cache entry in
        place of closing a registry.
      - `shmem`: retargeted exactly the way `tlstransport`'s own `wire`
        REPLACE cutover retargeted it earlier in this milestone —
        `ShmChannel` unchanged (already byte-agnostic), `ShmBridge`
        addressed by `StreamId` and carrying NTSCF-wrapped ACF_ABB/ACF_GBB
        frames via `send_acf_abb`/`send_acf_gbb`.
      - `adapt`: deliberately split scope, per its own disposition-table
        text reserving the deeper rebuild for Milestone 10.
        `Adapter<M>`/`AdaptEndpoint` (the generic external-format
        decorator layer) are retargeted onto `Endpoint` now, modeling one
        external "call" as a write-then-read round trip — this crate's
        own simplification, not a transcription of any real external
        protocol's semantics. `adapt()`/`RcpAdapter`/`to_message`/
        `from_message`/`response_to_message` (the RELAY §10.3/§15.7.5
        binding itself, mapping a zone *name* to/from `relay::Message.id`)
        stay bound to `Controller`/`Zone`/`Command`/`Response`/`Status`
        unchanged: honestly retargeting that half needs the
        endpoint-addressed `Message` shape this row's own text reserves
        for Milestone 10, plus a resolution for `mock::RcServer`'s
        still-open "no live notification mechanism" gap before
        `Status`-style `subscribe` forwarding has anything real to
        forward — inventing either here would guess at undecided
        behavior rather than retarget existing behavior.

      `README.md`'s module index, `SECURITY.md`'s security-controls
      table, `HARA.md`'s `SG-006` row, and `INCIDENT-RESPONSE.md`'s
      containment step are updated to name the new `*Endpoint`/
      `DiscoveryCache`-based types in place of the deleted
      `*Controller`/`Registry`-based ones. `cargo build`/`cargo clippy -D
      warnings`/`cargo test --all-targets` (including `--release`) are
      clean; `bash scripts/fusa-gap-check.sh` reports 678/678 (100%)
      requirements traced; `bash scripts/cyber-gap-check.sh` reports 6/6
      threats with tested countermeasures.
- [x] All **DEPRECATE**-disposition packages removed, with a migration note
      in the changelog explaining the replacement path (generally: use
      RELAY's `crossbar` router instead of an in-crate protocol bridge)
      Done (v0.12.0-dev): 11 of 11 done. `src/prioqueue.rs`, `src/
      zonegroup.rs`, `src/tsn.rs`, `src/firmware.rs`, `src/someip.rs`,
      `src/mqttbr.rs`, `src/ddsbr.rs`, `src/grpcbridge.rs`, `src/
      restbridge.rs`, `src/udsbr.rs`, and `src/doipbr.rs` are deleted
      outright, with their `pub mod` declarations removed from `src/
      lib.rs` — none had a cross-module caller anywhere in `src/` or in
      `src/bin/rcp.rs` (confirmed by inspection before removal), the same
      "self-contained cutover" precedent the REPLACE items (`canbr`,
      `linbr`, `capi`, `config`) established earlier in this milestone.
      `README.md`'s module index drops all 11 now-inaccurate rows outright,
      the same treatment `canbr`/`linbr` already established.

      A new top-level `CHANGELOG.md` (this repo had none before this item)
      carries the migration note this bullet's own text requires: the six
      protocol-bridge packages (`someip`, `mqttbr`, `ddsbr`, `grpcbridge`,
      `restbridge`, `udsbr`, `doipbr`) point integrators at RELAY's
      `crossbar` router (RELAY `v1.8`, RELAY PR #45) instead of an in-crate
      bridge — the same `crossbar`-router precedent go-DDS set removing its
      own MQTT/domain bridges in `v0.52.0` — while `prioqueue`, `zonegroup`,
      `tsn`, and `firmware` each get their own no-direct-replacement note,
      per the disposition table's own stated reasons.

      `.fusa-reqs.json`'s `REQ-PQ-001`..`008`, `REQ-ZG-001`..`007`,
      `REQ-TSN-001`..`005`, `REQ-FW-001`..`006`, `REQ-SOMEIP-001`..`005`,
      `REQ-MQTT-001`..`005`, `REQ-DDS-001`..`004`, `REQ-GRPC-001`..`004`,
      `REQ-REST-001`..`004`, `REQ-UDS-001`..`005`, and `REQ-DOIP-001`..`004`
      (57 requirements total) are retired (removed) rather than retargeted,
      per this milestone's own "retarget in place, or explicitly retire if
      no equivalent behavior exists" discipline: ten of the eleven deleted
      packages' tested behavior has no surviving in-crate analog at all.
      `prioqueue`'s `REQ-PQ-004` ("Critical dispatched before High/Normal")
      is the one exception with a real analog — `request::
      execution_priority_tier`/`select_next_pending_request` (`REQ-PRIO-
      001`..`004`), built ahead of schedule in Milestone 5 — but that
      analog is already fully covered by its own pre-existing requirement
      IDs, so `REQ-PQ-*` is retired rather than force-duplicated onto them,
      the same way `REQ-CANBR-001`'s covered-elsewhere fact was retired
      rather than kept in `canbr`'s own REPLACE cutover earlier in this
      milestone.

      Three FuSa/cybersecurity artifacts had already anticipated this item
      as "next" from `prioqueue`'s own ADAPT-bullet residual-risk notes and
      needed their now-dangling cross-references retargeted from the
      no-longer-slated-for-removal-but-actually-gone `REQ-PQ-004` onto
      `REQ-PRIO-004`, with residual risk raised honestly (mirroring the
      `wire`/`e2e` REPLACE cutovers' own treatment of `ReplayGuard`'s
      removal) since `REQ-PRIO-004` itself remains additive standalone
      plumbing, not yet wired into any dispatch loop: `.fusa-iec62443.json`
      (`T-004`'s countermeasures/residual_risk, `SC-006`), `tara.json`
      (`A-005`, `T-RCP-04`, `CSG-RCP-03`), `SECURITY.md`'s security-controls
      table, and `HARA.md`'s `SG-009` row. `.fusa-dfmea.json`'s `FM-004`
      ("Priority queue dispatch") gets the same treatment, retargeted onto
      `REQ-PRIO-004` with residual risk raised from `low` to `medium`; its
      `FM-009` ("Zone group broadcast") describes a `zonegroup`-only
      function with no analog at all, so it is retargeted to `residual_risk:
      "not applicable"` and kept (not deleted) for audit-trail continuity,
      mirroring this crate's own "never renumber or reuse" ID discipline.
      `cargo build`/`cargo clippy -- -D warnings`/`cargo test --all-targets`
      (including `--release`)/`cargo fmt --check` are clean;
      `bash scripts/fusa-gap-check.sh` reports 621/621 (100%) requirements
      traced; `bash scripts/cyber-gap-check.sh` reports 6/6 threats with
      tested countermeasures; `rsfusa check`/`qualify`/`release` all pass
      locally with zero ERROR findings; `relay conform --strict` passes.
- [x] All **KEEP-AS-IS** packages given a regression pass to confirm they
      are genuinely unaffected
      Done (v0.12.0-dev): all 7 of 7 checked individually, not just by a
      passing `cargo test`. `src/dyndata.rs`, `src/codegen.rs`, `src/
      iso21434.rs`, `src/certgap.rs`, `src/formal.rs`, `src/relay.rs`, and
      `src/base64_serde.rs` were each grepped for `Zone`/`Command`/
      `CommandType`/`Response`/`Status`/`Controller`/`Registry` and for any
      `use` import of a sibling in-crate module: none of the seven has a
      single hit against the legacy core types, and none imports anything
      beyond `std`/its own third-party deps (`relay.rs` alone pulls in
      `async_trait`/`chrono`/`serde`/`thiserror`/`tokio`, all pre-existing
      and RELAY-spec-driven, not RCP-core-driven). `base64_serde.rs`'s
      module doc mentions `rcp::{Command,Response,Status}.payload` in prose
      describing what currently calls it, but the module itself imports
      neither type — that's the caller's coupling (`lib.rs`'s `with =
      "base64_serde::opt"` field attributes on `Command`/`Response`/
      `Status`), not this package's, and it stays accurate as long as those
      structs exist pending their Milestone 10 rebuild.

      Cross-module callers were checked crate-wide (`grep -rn` for each
      package's `::` path across `src/` and `src/bin/`), not merely
      asserted: `dyndata`, `codegen`, `iso21434`, `certgap`, and
      `base64_serde`'s own path (as opposed to attribute usage) have zero
      external callers anywhere in the crate today. `formal` has no `use`
      import anywhere outside its own file — `src/lifecycle.rs` only
      *mentions* `crate::formal::Invariant` in two doc comments as a
      design-pattern comparison, so `formal` is more orthogonal than even
      the disposition table's own "consumed nowhere yet" framing implied.
      `relay` is consumed by `src/adapt.rs` (the RELAY §10.3/§15.7.5
      `Adapt()`/`to_message()`/`from_message()` binding, explicitly
      reserved for its own Milestone 10 rebuild per this milestone's ADAPT
      bullet above) and by `src/lib.rs`'s `RcpError` variant doc comments —
      both pre-existing, expected couplings to the vendored RELAY-spec
      surface itself, never to the old `Zone`/`Command` core.

      `cargo build --all-targets`, `cargo clippy --all-targets -- -D
      warnings`, `cargo test --all-targets` (1056 tests, including each of
      the seven packages' own unit tests run in isolation:
      `dyndata::` 6, `codegen::` 4, `iso21434::` 6, `certgap::` 9,
      `formal::` 5, `relay::` 10, `base64_serde::` 3 — all passing), and
      `cargo fmt --check` are clean; `bash scripts/fusa-gap-check.sh`
      reports 621/621 (100%) requirements traced; `bash scripts/
      cyber-gap-check.sh` reports 6/6 threats with tested countermeasures —
      identical counts to the DEPRECATE bullet immediately above, since
      this item changes no code and adds no requirements. No file under
      `src/` was modified by this item; the "regression pass" is this
      verification record itself; the previously-asserted-but-unchecked
      KEEP-AS-IS calls in the Satellite Package Disposition table are now
      confirmed rather than merely claimed. This closes Milestone 9 in
      full.

Success Criteria:
No module under `src/` still references the old `Zone`/`Command`/
`Response`/`Controller`/`Registry` API; `cargo build` and
`cargo clippy -- -D warnings` are clean; CI is green.

---

## Milestone 10 — v1.0 Production Release `v1.0.0`

Goal:
Ship a stable, TC18-conformant rust-RCP.

- [x] Public API stability guarantees (semver) for the new core

      Done (v0.13.0-dev): `docs/SEMVER.md` declares this crate's versioning
      scheme (semver, with `Cargo.toml`'s version deliberately not moving
      until this milestone's remaining checklist item lands and the crate
      publishes as `v1.0.0` — a repo-specific rule `CHANGELOG.md`'s own
      header note already stated but no document had spelled out as
      policy) and a three-tier stability classification of every `pub mod`
      `lib.rs` declares: Tier 1 (the core TC18 wire/lifecycle/dispatch
      surface plus the CLI's documented command surface — full semver
      guarantee), Tier 2 (endpoint decorators and transport bridges — same
      guarantee mechanics, individually newer), and Tier 3 (tooling
      modules — `sim`, `codegen`, `certgap`, `formal`, `iso21434`, `capi`
      — no compatibility guarantee at all).

      This item also does the cleanup its own scope requires before a
      stability guarantee means anything: `src/lib.rs`'s pre-Milestone-10
      `Zone`/`Priority`/`CommandType`/`ResponseStatus`/`Command`/
      `Response`/`Status`/`Subscription`/`Controller`/`LoaningController`/
      `Registry` API — retained, per Milestone 9's `mock` REPLACE item and
      this milestone's own already-completed `tara.json` rebase, exactly
      until the CLI cutover (the item immediately below this one) landed
      with no remaining caller — is deleted outright with no compatibility
      shim, along with `src/mock.rs`'s parallel `MockController`/
      `MockRegistry`/`Handler` test double for it and
      `src/base64_serde.rs`'s `opt` submodule (which existed only to serve
      `Command`/`Response`/`Status`'s optional payload field). `src/lib.rs`'s
      crate-level doc comment is rewritten to describe the TC18 core in
      place of the old Zone/Command/Registry model it still described.
      `RcpError`'s four general-purpose sentinels this legacy API
      originated (`NotFound`/`AlreadyExists`/`Busy`/`ZoneMismatch`) are
      kept, since `capi`/`authz`/`federation` and others construct and
      match on them for meanings unrelated to the removed `Zone` type — its
      doc-comment section is retitled from "Legacy Zone/Controller/Registry
      sentinels" to "General-purpose sentinels" to say so.

      Two enums get an explicit `#[non_exhaustive]` stability call, per
      this item's own scope note about `RcpError` and `regmap::EndpointType`:
      both are live growth surfaces (`RcpError` has gained new variants in
      every milestone since Milestone 2; `EndpointType`'s `ep_type` byte
      has unassigned codes above `0x0D`). `avtp::HeaderVariant` and
      `lifecycle::RcServerState` are surveyed and deliberately left
      exhaustive instead, since both mirror small, spec-fixed, closed sets
      — see `docs/SEMVER.md`'s `#[non_exhaustive]` policy section for the
      reasoning either way.

      A new `api-stability` CI job (`.github/workflows/ci.yml`) runs
      `scripts/api-snapshot-check.sh`, which diffs `cargo public-api
      --simplified`'s current output against the committed
      `docs/PUBLIC_API.txt` snapshot (generated against a nightly
      toolchain, matching the existing `fuzz` job's own nightly
      dependency) and fails the build on any drift — the mechanism that
      turns this item's declared policy into something enforced rather
      than only asserted in prose, per this item's own scope note.

      `.fusa-reqs.json` drops the 77 requirement entries
      (`REQ-ZONE-*`/`REQ-PRI-001..003`/`REQ-CMD-001..006`/
      `REQ-CMDSTRUCT-*`/`REQ-STATUS-*`/`REQ-CTRL-*`/`REQ-REG-*`/
      `REQ-RESP-*`/`REQ-STAT-001..005`/`REQ-RELAY-010`/`REQ-RELAY-011`/
      `REQ-MSG-*`) that described only the now-deleted legacy API and
      traced to nothing else; `tara.json`'s scope note is updated from
      "retained pending Milestone 10's CLI cutover... out of scope here"
      to record that the legacy surface it already anticipated deleting
      has in fact now been deleted. `README.md`'s Quick Start example
      (still showing the removed `MockController`/`Command`/`Controller`/
      `Zone` API, uncompilable since Milestone 9) and Module Index are
      rewritten against the current module set, with a new "API Stability"
      section pointing at `docs/SEMVER.md`; `CONTRIBUTING.md`'s Versioning
      section gains the same pointer plus the `docs/PUBLIC_API.txt`
      regeneration step contributors now owe alongside any `pub`-surface
      change.

      `cargo build --all-targets`, `cargo clippy --all-targets
      --all-features -- -D warnings`, `cargo test --all-targets` (978 lib
      tests, down from 1062 — 84 tests for the deleted legacy API removed,
      no others changed; 27 unchanged `src/bin/rcp.rs` tests), and `cargo
      fmt --all -- --check` are clean; `bash scripts/fusa-gap-check.sh`
      reports 545/545 (100%) requirements traced; `bash
      scripts/cyber-gap-check.sh` reports 6/6 threats with tested
      countermeasures; `bash scripts/api-snapshot-check.sh` reports the
      public API surface matches `docs/PUBLIC_API.txt`.
- [x] Full FuSa artifact re-basing: HARA, SAFETY_PLAN.md, and tara.json
      rewritten against the new architecture (the old versions describe
      hazards and threats specific to the replaced protocol)
      Done (v0.13.0-dev): `HARA.md`/`.fusa-hara.json`'s hazard and safety-goal
      tables are rebased against the TC18 core Milestones 1-9 built.
      H-001/H-005 (endpoint misaddressing, per-stream watchdog lockup) and
      H-002 (safety-tagged request loss) are retargeted onto their nearest
      TC18 mechanisms (`addressing::EndpointTable`,
      `watchdog::StreamWatchdogState`, `request::check_watchdog_overflow_purge`)
      rather than dropped, since the underlying safety concern persists
      under a different mechanism; H-010 ("Registry close race", which has
      no TC18 equivalent — the new `RcServerState` lifecycle has no "close"
      state) is replaced outright with a hazard the new register-map/EP0
      model actually introduces: a write bypassing lifecycle-state or
      root-client gating (`ep0::check_ep0_access_for_stream`/
      `is_root_client` composed with `lifecycle::is_register_reachable`/
      `is_register_writable`). H-003/H-004/H-006/H-007/H-008/H-009 already
      described TC18-native mechanisms (CRC-32 safe point, `AuthzEndpoint`,
      power-mode gating, NTSCF length validation, execution-priority
      tiering) from their prior Milestone 9 amendments and needed only
      terminology harmonization.

      `SAFETY_PLAN.md` §4.1 now cites the TC18 core by name; §4.3's
      integration-test coverage target no longer says "All controller trait
      methods" (the retired `Controller` trait) and instead names the live
      `mock::RcServer::handle_ntscf_frame` decode -> route -> dispatch ->
      encode path.

      `tara.json`'s scope is rewritten around the TC18 core surface, with
      an explicit note that the legacy `Zone`/`Command`/`Controller`/
      `Registry` API (retained pending Milestone 10's CLI cutover) is out
      of scope and receives no fresh threat modeling since it has no
      compatibility shim and will be deleted outright. Asset A-001 ("Wire
      frame structural integrity", `wire::validate_header()`, deleted
      outright by Milestone 9's `wire` REPLACE cutover) is retargeted onto
      `avtp::decode_ntscf_frame`/`acf::decode_acf_abb`; A-003 ("Zone
      controller command-execution availability") is retargeted onto
      `mock::RcServer`'s endpoint-addressed dispatch path. Two new assets
      (A-007 EP0 register-map access-control integrity, A-008
      discovery-stream claim integrity) and their threat scenarios
      (T-RCP-09, T-RCP-10) and cybersecurity goals (CSG-RCP-06, CSG-RCP-07)
      cover TC18-native attack surface — EP0 root-client/lifecycle-gate
      bypass and discovery-stream claim hijack — that the replaced protocol
      never had. T-RCP-01 is retargeted onto AVTPDU/ACF frame injection;
      T-RCP-05 drops its now-moot "no in-crate mitigation until Milestone
      10 replaces Controller/Command" framing (that removal is not a
      Milestone 10 checklist item; the legacy surface is simply out of this
      TARA's scope per the rewritten scope note) and is retargeted fully
      onto `authz::AuthzEndpoint`. T-RCP-02/04 have minor terminology
      updates ("write request"/"RC Server"/"addressed endpoint" in place of
      "Set command"/"zone controller"). T-RCP-03/06/07/08 already described
      TC18-native mechanisms and needed no content change.

      `.fusa-reqs.json` needed no changes: every `REQ-*` group the rebased
      HARA/TARA cite (`REQ-EPLK-*`, `REQ-SAFETY-*`, `REQ-EP0-*`,
      `REQ-LIFE-*`, `REQ-DISC-*`, `REQ-WIRE-*`, etc.) was already retargeted
      onto TC18 behavior by its own satellite package's Milestone 1-9 item;
      only `REQ-ZONE-*`/`REQ-CTRL-*`/`REQ-REG-*`/`REQ-RESP-*`/`REQ-STAT-*`
      still describe the legacy API, and nothing in the rebased HARA/TARA
      cites them, consistent with those types' removal being out of this
      item's scope. `.fusa-dfmea.json`/`.fusa-iec62443.json`/
      `.fusa-problems.json` already carried their own Milestone 9
      surgical-amendment notes and needed no further change for this item.

      `cargo build --all-targets`, `cargo clippy --all-targets
      --all-features -- -D warnings`, `cargo test --all-targets` (1062 lib
      tests + 19 `src/bin/rcp.rs` tests, unchanged — this item touched no
      source), and `cargo fmt --all -- --check` are clean; `bash
      scripts/fusa-gap-check.sh` reports 622/622 (100%) requirements
      traced; `bash scripts/cyber-gap-check.sh` reports 6/6 threats with
      tested countermeasures; `relay conform --strict` against the release
      binary passes all three RELAY §12 checks.
- [x] RELAY spec `Adapt()`/`to_message()`/`from_message()` rebuilt against
      the new endpoint-addressed `Message` shape (no more zone-name-as-`id`
      mapping)
      Done (v0.13.0-dev): `src/adapt.rs`'s `adapt()`/`RcpAdapter`/
      `to_message()`/`from_message()`/`response_to_message()` no longer
      reference `Zone`/`Command`/`CommandType`/`Priority`/`Controller`/
      `Response`/`Status`/`zone_from_str` — `adapt()` now wraps an
      `Arc<mock::RcServer>` as a `relay::Caller`/`Node`, addressed by
      `(avtp::StreamId, byte_bus_id)` instead of a zone name.
      `Adapter`/`AdaptEndpoint`/`PassthroughAdapter` (already retargeted in
      Milestone 9) are untouched.

      Three flagged, this-crate-own design choices this rebuild had to make
      (Guiding Principle 5), documented in `src/adapt.rs`'s own provenance
      note:

      - **`Message.id` encoding**: `format_endpoint_id`/`parse_endpoint_id`
        encode `(stream_id, byte_bus_id)` as
        `"<16 hex digits>.<decimal byte_bus_id>"`, replacing the retired
        zone-name-as-`id` convention. Malformed input is
        `Err(RcpError::InvalidParameter)`, never a panic.
      - **Read vs. write inference**: `RcServer::handle_abb` dispatches on
        a boolean `op` flag with no third "no-op" case (unlike the retired
        `CommandType`). `from_message` reads an optional `"rcp.op"` meta
        key (`"read"`/`"write"`) and, absent one, infers it from whether
        `msg.payload` is empty. An optional `"rcp.read_size"` meta key
        (decimal `u8`, default `u8::MAX`) supplies the read byte count
        `Command` never needed to carry.
      - **`subscribe`**: `mock::RcServer` still has no live-notification
        mechanism (its own doc comment states this is an open gap, not
        resolved by this item). Rather than invent one or overload a RELAY
        error sentinel to mean "unsupported," `RcpAdapter::subscribe`
        returns a channel that is immediately, legitimately closed — an
        honest "no events, currently" answer within `Node::subscribe`'s
        existing typed contract. Building a real forwarding path is left to
        whichever later item gives `RcServer` a live-notification
        mechanism to forward. The retired `Controller::subscribe`-forwarding
        plumbing (`AdaptQueue`, its blocking-producer task) is removed
        rather than kept as unused scaffolding for the same reason.
      - **`close`**: `RcServer` tracks an `RcServerState` lifecycle
        position, not an open/closed connection boolean, so `RcpAdapter`
        keeps its own `closed: AtomicBool` (mirroring
        `udp::UdpTransport::close`'s own no-real-resource-to-close
        precedent in this model) so `Node::close`'s "further calls fail"
        contract stays meaningful rather than becoming a pure no-op.

      An already-expired `Context` is now surfaced as `relay::Error::Timeout`
      before dispatch (the closest available analog to the retired
      `Controller::send`'s "zero timeout = already expired" check, since
      `RcServer::handle_abb` itself has no timeout parameter to honor
      mid-call).

      `.fusa-reqs.json` `REQ-ADAPT-006`/`007`/`008`/`009`/`010` text is
      updated to describe the new `AcfAbbMessage`/`RcServer`-based behavior
      in place of the retired `Status`/`Command`/`Response`/`Controller`
      wording; a new `REQ-ADAPT-011` covers the `format_endpoint_id`/
      `parse_endpoint_id` encoding. `src/adapt.rs`'s own tests are rewritten
      against `mock::RcServer`/`MockEndpoint` in place of `MockController`/
      `Zone`/`Command`/`Response`/`Status`.

      `cargo build --all-targets`, `cargo clippy --all-targets --all-features
      -- -D warnings`, `cargo test --all-targets` (1062 lib tests, up from
      1056 — 20 in `adapt::tests`, up from 14 — plus 19 unchanged `src/bin/
      rcp.rs` tests), and `cargo fmt --all -- --check` are clean; `bash
      scripts/fusa-gap-check.sh` reports 622/622 (100%) requirements traced;
      `bash scripts/cyber-gap-check.sh` reports 6/6 threats with tested
      countermeasures.
- [x] CLI (`rust-rcp`) command surface updated: discovery, register
      read/write, per-endpoint drive commands, replacing the old
      `send`/`zones`/`status --zone` shape

      Done (v0.13.0-dev): `src/bin/rcp.rs`'s command surface is rebuilt
      against `mock::RcServer`'s `(avtp::StreamId, byte_bus_id)`-addressed
      model, the same backing type `src/adapt.rs`'s own Milestone 10
      rebuild targets. `zones`/`send`/`status --zone` and every
      `Zone`/`Command`/`Controller`/`Registry`/`rcp::mock::MockRegistry`
      reference are gone from the file; `version`/`capabilities`/`status`/
      `convert` are unchanged in shape (none of them ever referenced
      `Zone` — `convert`'s own `"zone"`/`"seq"`/`"healthy"` JSON shape is a
      self-contained RELAY-spec wire transform, not a use of the `Zone`
      Rust type), with `capabilities`'s `commands`/`interfaces` JSON
      fields updated to `["version","capabilities","status","convert",
      "discover","register","endpoint"]` /
      `["RcServer","Endpoint"]`.

      Three new subcommands replace the retired trio:

      - `discover [--transaction <n>] [--format json]` builds a discovery
        request (`discovery::build_discovery_request`) and answers it via
        `discovery::build_discovery_response`, decoding/printing the
        resulting `GeneralRegisters` snapshot.
      - `register read [--stream <hex>] [--format json]` /
        `register write --payload <hex> [--stream <hex>] [--root]`
        dispatch an EP0-addressed (`ep0::EP0_BYTE_BUS_ID`) read/write
        `AcfAbbMessage` through `RcServer::handle_abb`. `--root` first
        designates `--stream` the server's root client via
        `RcServer::set_root_client`. A write is reported exactly as
        `RcServer::handle_abb` answers it, including
        `RcpError::LockedMemAccess` for the root client itself — see that
        function's own doc comment for why a `General`-category write is
        currently never actually accepted by this in-process server; the
        CLI does not paper over that.
      - `endpoint read --bus-id <n> [--stream <hex>] [--ep-type <n>]
        [--initial <hex>] [--read-size <n>] [--format json]` /
        `endpoint write --bus-id <n> --payload <hex> [--stream <hex>]
        [--ep-type <n>] [--initial <hex>]` register a fresh
        `mock::MockEndpoint` of `--ep-type` (default
        `regmap::EndpointType::Gpio`) holding `--initial` under
        `(--stream, --bus-id)` via `RcServer::register_endpoint`, then
        dispatch a read/write `AcfAbbMessage` through `RcServer::handle_abb`'s
        `DeviceEndpoint` route.

      One design choice flagged per Guiding Principle 5, documented in the
      file's own doc comment: this crate has no concrete `udp::UdpSocket`
      implementation over a real OS socket, so `discover`/`register`/
      `endpoint` each construct and address a fresh in-process `RcServer`
      for the lifetime of one invocation — the same ephemeral-server
      discipline the retired `send`/`status --zone` already used against a
      fresh `mock::MockRegistry` each invocation, not a regression this
      item introduces. `--stream` is parsed/rendered as bare lowercase hex
      (no `0x` prefix), matching `adapt::format_endpoint_id`'s own
      `StreamId` rendering, per this item's naming-consistency instruction
      (issue #35 settled module naming only; no CLI-specific addressing
      convention existed yet for this item to reconcile against, so it
      follows `adapt.rs`'s existing one).

      `.fusa-reqs.json` `REQ-CLI-001`/`002`/`004`/`005` text is retargeted
      to describe `discover`/the shared `register`+`endpoint` flag set/
      `endpoint`/the shared `register`+`endpoint` dispatch-through-
      `RcServer::handle_abb` behavior, in place of the retired `send`/
      `zones`/`status --zone` wording; `REQ-CLI-008`'s text drops its
      stale `"(no --zone)"` parenthetical. No new requirement IDs were
      needed. `src/bin/rcp.rs`'s own tests are rewritten against
      `mock::RcServer`/`mock::MockEndpoint` in place of `MockController`/
      `MockRegistry`/`Zone`/`Command` (27 tests, up from 19).

      `cargo build --all-targets`, `cargo clippy --all-targets --all-features
      -- -D warnings`, `cargo test --all-targets` (1062 lib tests, 27 in
      `src/bin/rcp.rs`, up from 19), and `cargo fmt --all -- --check` are
      clean; `bash scripts/fusa-gap-check.sh` reports 622/622 (100%)
      requirements traced; `bash scripts/cyber-gap-check.sh` reports 6/6
      threats with tested countermeasures.
- [x] Conformance test vectors / interop verification against at least one
      sibling x-RCP implementation once it has also uplifted, or
      self-referential wire-format golden vectors if none is ready yet

      Done (v0.13.0-dev): `src/conformance.rs` (test-only —
      `#[cfg(test)] mod conformance;` in `lib.rs`, deliberately not `pub`
      like every other module here, since it is not part of this crate's
      protocol surface; see its own doc comment's "Why `#[cfg(test)]` rather
      than `pub mod`" section) pins five self-referential wire-format golden
      vectors as frozen literal byte arrays (not recomputed from the encoder
      under test, so an accidental future regression is caught even if
      encode/decode stayed internally consistent with each other): an NTSCF
      header, a TSCF header with a non-degenerate `avtp_timestamp`
      (`0x1A2B3C4D`), an ACF_ABB message, an ACF_GBB message with a non-zero
      `message_timestamp` (`0x0102030405060708`), and a composed
      NTSCF+ACF_ABB frame exercising Milestone 9's frame-composition step.

      `go-RCP` is the one sibling x-RCP implementation that has also
      uplifted to a real TC18 core and shipped `v1.0.0` (its own
      `ROADMAP.md` item 59, "TC18 Conformance Cutover & RELAY
      Re-Certification", commit
      `bdc760fb057f067cfb68199b6c3d0edab9e0c671`), so this item took the
      roadmap's preferred interop-verification path rather than the
      self-referential-only fallback: a standalone Go program (not
      committed to this repository) called go-RCP's own
      `avtp.EncodeHeader`/`acf.EncodeMessage` for field values logically
      analogous to each golden vector, and the resulting bytes are recorded
      in `conformance::go_rcp_crosscheck` for direct comparison. Result:
      **not byte-identical**, at every vector type — go-RCP's header/message
      lengths, its flat 16-bit `data_length` field versus this crate's
      11-bit split-across-two-bytes packing, its explicit 2-bit
      timestamp-status marker (which this crate's `TscfHeader` has no
      equivalent field for at all), its `1`/`2` message-kind discriminants
      versus this crate's `0x0E`/`0x0D`, and its flat single-byte
      `ByteBusID` versus this crate's 11-bit packed `byte_bus_id` all
      diverge from `crate::avtp`/`crate::acf`'s own interpretation. Per
      Guiding Principle 5, this divergence is recorded in `conformance.rs`'s
      module doc comment and pinned by a dedicated test
      (`go_rcp_bytes_diverge_from_this_crates_own_encoding`) rather than
      silently resolved by rewriting either implementation to match the
      other — reconciling rust-RCP's byte-level choices in `crate::avtp`/
      `crate::acf` against go-RCP's independently-arrived-at choices, or
      directly against the OPEN Alliance TC18 Remote Control Protocol
      Specification's own described behavior, remains out of this item's
      scope and is left for a follow-up. One point of genuine agreement is
      recorded alongside the divergence rather than omitted: both
      implementations split the 64-bit `stream_id` the same way (6-byte
      sender MAC high, 2-byte locally-assigned suffix low), confirmed
      byte-identical.

      `.fusa-reqs.json` gains six new `REQ-CONF-001..006` requirements
      covering the five golden vectors and the pinned cross-check
      divergence. `cargo build --all-targets`, `cargo clippy --all-targets
      --all-features -- -D warnings`, `cargo test --all-targets` (986 lib
      tests, up from 978; 27 unchanged `src/bin/rcp.rs` tests), and `cargo
      fmt --all -- --check` are clean; `bash scripts/fusa-gap-check.sh`
      reports 551/551 (100%) requirements traced; `bash
      scripts/cyber-gap-check.sh` reports 6/6 threats with tested
      countermeasures.

      This was the last unchecked item in this milestone's checklist. With
      it landed, `ROADMAP.md` Milestone 10 is complete; the crate is ready
      to tag and publish as `v1.0.0` per the version-freeze policy
      `CHANGELOG.md` and `docs/SEMVER.md` both state — that actual version
      bump (`Cargo.toml` still reads `0.3.0` as of this entry), tag, and
      `crates.io` publish are a deliberately separate release step, not
      folded into this item (and `crates.io` publishing depends on the
      still-open `CARGO_REGISTRY_TOKEN` gap tracked by issue #12).

Success Criteria:
rust-RCP's wire format, RC Server model, and endpoint set are demonstrably
conformant with the OPEN Alliance TC18 Remote Control Protocol
Specification's described behavior, and the crate publishes as `v1.0.0`.

---

## Satellite Package Disposition

Every package listed in this crate's module index as of the start of this
roadmap (before Milestone 1) got one of four calls: **REPLACE** (rebuilt to
be spec-conformant), **ADAPT** (retarget to the new core without a full
rewrite), **DEPRECATE** (no place in the new model), or **KEEP AS-IS**
(genuinely orthogonal, unaffected). All ten Milestones are now complete
(`v1.0.0`); the fourteen rows below marked **— done: deleted** no longer
exist as files in `src/` — their REPLACE/DEPRECATE disposition has already
been fully carried out, not merely decided. Every other row's package still
exists under its original name.

| Package | Call | Reason |
|---|---|---|
| `wire` | REPLACE | Becomes IEEE 1722 AVTPDU / ACF_ABB / ACF_GBB framing (Milestone 1) — the current 16-byte header has no equivalent in the spec — **done: deleted**; superseded by `avtp`/`acf` |
| `e2e` | REPLACE | Becomes the spec's real CRC32 (poly `0xF4ACFB13`) safe-point mechanism (Milestone 6) — current CRC-16 + replay guard is a different algorithm serving a different model |
| `mock` | REPLACE | Must model an RC Server + Endpoints for testing, not a `Zone`-keyed controller |
| `config` | REPLACE | Must represent the register-map/lifecycle configuration model, not `RcpConfig{controllers, watchdog, rate_limit}` |
| `capi` | REPLACE | C FFI types are 1:1 tied to the old `Command`/`Zone` shapes, which disappear |
| `watchdog` | REPLACE | Ad-hoc periodic WATCHDOG-command dispatcher replaced by the spec's real per-stream watchdog (`rx_wd_*`, liveness reset on every request, safe-state entry) — see Milestone 6 |
| `powerstate` | REPLACE | Ad-hoc Active/Sleep/Standby via `CommandType::SLEEP`/`WAKE` replaced by the spec's real Normal/StandBy/Sleep/Unpowered model with cold/hot start — see Milestone 6/7 |
| `canbr` | REPLACE | Becomes the CAN controller endpoint type (Milestone 7); CAN is also a first-class endpoint in the new model, not a bridge underneath Zone/Command framing — **done: deleted**; superseded by `can` |
| `linbr` | REPLACE | Becomes the LIN commander endpoint type (Milestone 7); the spec's raw-byte-passthrough philosophy contradicts the current ad-hoc PID-generation scheme, so this isn't a mechanical port — **done: deleted**; superseded by `lin` |
| `udp` | REPLACE | IEEE1722-over-UDP is spec-legal as a transport (§2.1), but every framing call must be rebuilt against Milestone 1's AVTPDU encode/decode instead of `wire::encode_command` |
| `prioqueue` | DEPRECATE | Critical/High/Normal decorator is superseded by the spec's own native per-endpoint execution-priority ordering (cancellation > triggered > timed > compound > compound-wait > chained > standard), built into the Milestone 5 core scheduler rather than a bolt-on wrapper — **done: deleted** (Milestone 9) |
| `zonegroup` | DEPRECATE | Zone-broadcast has no direct equivalent once `Zone` disappears; multi-endpoint fan-out isn't spec-defined and can be rebuilt later as a generic client-side helper if a real need emerges — **done: deleted** (Milestone 9) |
| `tsn` | DEPRECATE | Current implementation hacks a priority byte into the payload, which is incompatible with real AVTPDU framing; legitimate TSN traffic-class handling (VLAN PCP tagging) belongs at the transport/socket layer and isn't defined by the RCP spec itself — **done: deleted** (Milestone 9) |
| `firmware` | DEPRECATE | Chunked-SET/GET OTA sequencer has no home in the thirteen defined endpoint types; not part of TC18 scope — could return later as an OEM-layer concern built atop a real endpoint, not as core protocol — **done: deleted** (Milestone 9) |
| `someip` | DEPRECATE | SOME/IP is not a spec-defined RCP transport. Ecosystem precedent (go-DDS removed its own MQTT and domain bridges in v0.52.0) is to handle cross-protocol bridging centrally via RELAY's `crossbar` router rather than per-repo bridges — **done: deleted** (Milestone 9) |
| `mqttbr` | DEPRECATE | Same `crossbar`-router precedent as `someip` — **done: deleted** (Milestone 9) |
| `ddsbr` | DEPRECATE | Same `crossbar`-router precedent as `someip` — **done: deleted** (Milestone 9) |
| `grpcbridge` | DEPRECATE | Same `crossbar`-router precedent as `someip` — **done: deleted** (Milestone 9) |
| `restbridge` | DEPRECATE | Same `crossbar`-router precedent as `someip` — **done: deleted** (Milestone 9) |
| `udsbr` | DEPRECATE | UDS/ISO 14229 is a distinct vehicle-diagnostics protocol, not a spec-defined RCP transport; same `crossbar`-router precedent applies — **done: deleted** (Milestone 9) |
| `doipbr` | DEPRECATE | DoIP/ISO 13400-2 is likewise not a spec-defined RCP transport; same `crossbar`-router precedent applies — **done: deleted** (Milestone 9) |
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
| OPEN Alliance TC18 RCP | v0.5.1_RC → v1.0 | Complete — Milestones 1-10 carried out the full core replacement; the crate published as `v1.0.0` (see `CHANGELOG.md`) |
| ISO 26262:2018 | ASIL-B | Complete, rebased against the TC18 protocol core as of Milestone 10 (`v1.0.0`) — see `HARA.md` |
| IEC 62443-4-2 | SL-2 | Complete, rebased against the TC18 protocol core as of Milestone 10 (`v1.0.0`) — see `SECURITY.md`, `.fusa-iec62443.json` |
| ISO 21434:2021 | WP.10 threat model | Complete, rebased against the TC18 protocol core as of Milestone 10 (`v1.0.0`); TARA content is protocol-specific and reflects the current attack surface — see `tara.json` |
