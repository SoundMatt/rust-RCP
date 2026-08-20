# Changelog

All notable changes to rust-RCP are documented here. Entries below `v1.0.0`
are grouped by the roadmap milestone that produced them (see `ROADMAP.md`),
because `Cargo.toml`'s version was deliberately held still for the whole
OPEN Alliance TC18 core replacement; from `v1.0.0` on, each entry is a real
release. See `docs/SEMVER.md` for the versioning scheme, including why a
wire-format change is a MAJOR bump even when it is a fix.

## v5.5.0 (Table 30/33 Row-2 evt[2:0] validation — ADC, 2nd endpoint type) — closed

Direct follow-up to v5.4.0's pilot: `adc.rs` becomes the second of the eight
TC18 §13.5 Table 33 Row-2 endpoint types (`{ADC, PWM_IN, I2C, LIN, CAN,
UART, ISELED, MDIO}`) to call the shared `evtgroup::evt_row2_kind_of`
predicate. `evtgroup.rs` itself is unchanged — this release only adds
`adc.rs`'s own caller.

New, purely additive `pub` items (MINOR bump per `docs/SEMVER.md`):

- `adc::AdcRequest` / `adc::AdcRequest::from_evt_sub_opcode` — ADC's own
  request-decode entry point, mirroring `i2c::I2cRequest`/
  `i2c::I2cRequest::from_evt_sub_opcode`'s exact shape. Unlike
  `I2cRequest::Plain`, which decodes its payload as an `I2cByteTransfer`,
  `AdcRequest::Plain` carries no decoded payload struct at all — TC18
  §13.7.9.3 states plainly "The ADC request has no byte_msg_payload, while
  a wait-request needs a byte_msg_payload." `from_evt_sub_opcode` enforces
  that: a `Plain` (`000b`) request requires an empty payload (a non-empty
  one returns `Err(RcpError::InvalidParameter)`, since TC18 names no
  dedicated violation code for that specific case), a `ConfigWrite`
  (`111b`) request is recognized but not decoded further (TC18 §12.7.1's
  payload shape is still deferred, not guessed at), and every `Reserved`
  sub_opcode is rejected with `Err(RcpError::UnsupportedCmd)`, matching
  Table 33's own stated error code. See `adc.rs`'s own doc comment
  "Provenance note: evt[2:0] request validation" for the full citation and
  reasoning.

Not in this release: wiring `AdcRequest::from_evt_sub_opcode` into
`mock::RcServer`'s actual dispatch — `mock::Endpoint`'s trait signature
still does not carry an `evt` value to any implementation at all, the same
gap v5.4.0's pilot found and left as-is (confirmed unchanged here). The
remaining six Row-2 endpoint types (`PWM_IN`, `LIN`, `CAN`, `UART`,
`ISELED`, `MDIO`) are expected to add their own `evt_row2_kind_of`-based
request-decode entry point the same way, in later items.

## v5.4.0 (Table 30/33 Row-2 evt[2:0] validation — pilot: shared predicate + I2C dispatch wiring) — closed

TC18 §13.5 Table 33 groups the 13 RCP endpoint types into 3 rows for
request-side `evt[2:0]` semantics. `evtgroup.rs`'s `classify_evt_sub_opcode`
(the still-unresolved "Groups A/B/C" roadmap classification) is unchanged
and untouched — it answers a different, broader question this crate has
not yet resolved. This release adds a *narrower*, already-unambiguous
predicate alongside it: Row 2 — `{ADC, PWM_IN, I2C, LIN, CAN, UART, ISELED,
MDIO}` — has a simple three-way rule (`000b` plain, `001b`-`110b` reserved
=`UNSUPPORTED_CMD`, `111b` config-write per §12.7.1), and none of this
crate's eight Row-2 endpoint-type modules read `evt` at all before this
release.

New, purely additive `pub` items (MINOR bump per `docs/SEMVER.md`):

- `evtgroup::EvtRow2Kind` / `evtgroup::evt_row2_kind_of(sub_opcode: u8)` —
  the shared Row-2 classifier every Row-2 endpoint-type module is expected
  to call, mirroring `gpio::GpioWriteSemantics::from_sub_opcode`/
  `spi::SpiChannelSelect::from_sub_opcode`'s own per-endpoint-type
  `sub_opcode` readers. Notably, this does **not** implement Table 33's
  own literal Row-2 cell text ("000b to 110b reserved", `000b` included) —
  that literal reading is resolved as a drafting defect against TC18
  §12.9.1's general `evt[2:0] != 0` rule, §13.7.7.3's I2C request-handling
  description, and the cross-repo resolution c-RCP centralizes as
  `rcp_acf_evt_row2_is_plain()` (recorded in RELAY's
  `docs/RCP-ARCHITECTURE.md`). See `evtgroup.rs`'s own doc comment for the
  full citation and reasoning.
- `i2c::I2cRequest` / `i2c::I2cRequest::from_evt_sub_opcode` — I2C's own
  request-decode entry point (this crate's pilot Row-2 endpoint type): a
  `Plain` request decodes its payload as an `I2cByteTransfer`, a
  `ConfigWrite` request is recognized but not yet decoded further (TC18
  §12.7.1's payload shape is deferred, not guessed at), and every
  `Reserved` sub_opcode is rejected with `Err(RcpError::UnsupportedCmd)`.

Not in this release: wiring either into `mock::RcServer`'s actual dispatch
— `mock::Endpoint`'s trait signature does not carry an `evt` value to any
implementation at all yet, a gap that applies identically to GPIO/SPI's
own `sub_opcode` readers (neither is wired into `mock` dispatch either).
`I2cRequest` is built to that same "additive standalone plumbing" level.
The other seven Row-2 endpoint types (`ADC`, `PWM_IN`, `LIN`, `CAN`,
`UART`, `ISELED`, `MDIO`) are expected to add their own
`evt_row2_kind_of`-based request-decode entry point the same way, in
later items.

## v5.3.0 (response-kind surfaced in adapt.rs) — closed

`ByteMessageInfo::response_kind()` (TC18 §11.3 Table 15/§11.3.1-§11.3.4)
was implemented and tested but never called from any production code
path — `adapt.rs`'s `to_message`/`response_to_message` (the RELAY-adapter
conversion of an ACF response into a `relay::Message`) built its `meta`
map from `op` alone, unlike cpp-RCP's equivalent, which already surfaces
`meta["rcp.response_kind"]`. `to_message` now classifies via
`response_kind()` and adds the same `"rcp.response_kind"` meta key
(`"acknowledge"`/`"write"`/`"read"`/`"error"`, via a new
`ResponseKind::as_str()` — a new `pub` item, hence the MINOR bump per
`docs/SEMVER.md`). Purely additive: the existing `"rcp.op"` meta key and
every other field are unchanged.

## v5.2.0 (2026-08-02 SHOULD/MAY extraction + references) — closed

Mirrors c-RCP's and cpp-RCP's own identical SHOULD/MAY audits. Grepped
the full TC18 spec text for every SHOULD (12) and MAY (44) occurrence,
excluded 6 legal-boilerplate hits, and individually classified the
remaining 51. Seven already-implemented optional capabilities got a real
`tc18` citation added to their existing requirement entry — a field this
crate's `.fusa-reqs.json` had never used before (0/608 requirements cited
TC18 prior to this pass). The non-testable lines are individually cited
in new `docs/TC18-NON-NORMATIVE-CLAUSES.md`.

This pass also honestly flags four MAY-described capabilities as
genuinely uncertain rather than papering over them with a citation —
most notably, `REQ-TIME-002`/`REQ-TIME-003`'s Timed-request readiness
check uses `AvtpTimestamp`, a 32-bit/~4.3-second-rollover type, for what
TC18 §11.2.2.5 defines as a 48-bit/3.25-day-rollover `presentation_time`
value — the same class of time-domain conflation bug found and fixed
this session in go-RCP's conditional-request envelopes. Not fixed here;
recorded for its own dedicated investigation. The other three flagged
gaps (multi-request-per-frame's citation, the EP_USED bit, and
integrated-PHY-via-MDIO) are recorded the same way.

No code behavior changed. A fuller MUST-clause citation backfill remains
separate, larger, future work.

## v5.0.0 (2026-07-31 TC18-conformant power-mode model + register-map config tables) — closed

**Breaking**, on the wire for three register-map config-table row types and
behaviourally for the power-mode model. Four independently-confirmed
findings, each verified against the specification's own tables and figures
before being changed — and, where the layout question was one of bit
packing or table structure that text extraction cannot settle, against a
600/300 dpi render of the relevant page rather than extracted text.

The register-map row types are not yet reachable from any live decode path
(nothing in this crate performs register I/O against a real RC Server), so
the interop urgency is lower than `v3.0.0`/`v4.0.0`; the power-mode finding
is a real behavioural bug in code that is callable today.

A common root cause runs through three of the four: this crate's own
provenance notes asserted that the specification records these tables'
field names and purpose "in prose" with "no explicit per-field bit-width or
byte-offset table" and "no textual basis for a specific bit-position
assignment". That was false — §12.7.7 Table 22, §12.7.8 Table 23 and
§12.7.10 Table 25 each carry explicit "Relative address" and "Type"
columns, and Table 22 additionally gives `0x000D.0`-`0x000D.7` bit
addresses. The layouts built on that false scarcity were wrong, and the
notes have been corrected rather than merely the code.

A second contributing cause: every affected encoder's tests asserted only
`decode(encode(x)) == x`. A round trip through one's own encoder cannot
detect a wrong layout. Every fix below adds literal expected-byte vectors
laid out by hand from the specification's address columns, chosen so that a
transposition or a mis-sized row cannot pass.

- **rust-RCP-P01 (BREAKING, ASIL-B):** the cold-/hot-start mapping in
  `src/powerstate.rs` was inverted. §12.4.1 "Power-On / Wake-Up / Start-Up
  behavior" (p.46) states it in one sentence — "There are two types of
  start-up: a cold start (after power-on **or wake-up from sleep**) and a
  hot start (**=wake-up from StandBy**)" — and §12.4 Figure 17 labels the
  arrows to match. `try_cold_start` accepted only `Unpowered -> Normal`,
  omitting the `Sleep -> Normal` cold start entirely; `try_hot_start`
  claimed `Sleep -> Normal` and gated it behind the WakeUp handshake, when
  TC18's hot start is `StandBy -> Normal` and it is the *hot*-start
  procedure that §12.4.1 attaches that handshake to. Net effect on a caller
  wiring this up: every wake-from-sleep was rejected until a handshake that
  the specification does not require there had completed, and every
  wake-from-standby was rejected outright, there being no path admitting
  that origin at all. `try_cold_start` now admits both documented origins
  and `try_hot_start` admits `StandBy`.
- **rust-RCP-P02 (BREAKING):** `is_power_mode_transition_defined` accepted
  `StandBy <-> Sleep` as an ordinary transition. Figure 17 draws no edge of
  any kind between the two low-power modes — it places `Normal`/`StandBy`
  in a "Powered" box and `Sleep` in a separate "Only part of PHY powered"
  box, with both low-power modes entered from and returned to `Normal`
  only. It also omitted `Normal -> Sleep`, which Figure 17 *does* draw
  ("Go to Sleep"). The function now returns `true` for exactly Figure 17's
  two "Go to ..." edges, `Normal -> StandBy` and `Normal -> Sleep`, and
  `false` for all fourteen other ordered pairs. The prior set was derived
  from reading the four mode *names* as a depth ordering, which the
  specification never states; that inference is what produced P01 as well,
  and the module's doc comment now reproduces Figure 17's edge list
  directly instead.
- **rust-RCP-P03 (BREAKING, wire):** `regmap::RequestStreamConfigEntry`
  used the wrong row layout. §12.7.7 Table 22 (pp.57-58) packs eight
  per-stream flags (`rx_enforce_e2e` .. `rx_wd_info_enable`) into the
  single bit-addressed byte at `0x000D` — they are the only fields in the
  table addressed with a `.bit` suffix — and closes each row with a 16-bit
  reserved word at `0x0012` and a 32-bit reserved block at `0x0014`, the
  next row's `rx_stream_id2` at `0x0018` fixing the stride at **24 bytes**.
  This crate gave each flag a whole byte of its own and dropped all six
  reserved bytes, for `ENCODED_LEN = 25` and a wrong offset for every field
  from `0x000D` onward. `ENCODED_LEN` is now 24, the eight flag fields are
  typed `bool` to match their 1-bit width (a `u8` could not round-trip
  losslessly through one bit), and `FLAGS_OFFSET` plus eight `FLAG_*` mask
  constants and a `flags_byte()` accessor expose the packing.
- **rust-RCP-P04 (BREAKING, wire):** `regmap::EpByteBusIdMapEntry`
  transposed two fields. §12.7.8 Table 23 "EP_ID_config" (p.59) tabulates
  `Request_Stream_Index` at `0x0000`, `EP_Nr` at `0x0001` and `BBID` at
  `0x0002`; this crate emitted `[stream_index, BBID_hi, BBID_lo, EP_Nr]`.
  The row length was coincidentally right; the middle three bytes were not.
- **rust-RCP-P05 (BREAKING, wire):** `regmap::SequencerStateEntry` modeled
  one of the row's two fields. §12.7.10 Table 25 "SEQUENCER_config" (p.61)
  gives each sequencer `Seq_state` at `0x0000` **and**
  `Request_stream_index` at `0x0001` — the latter being the access-control
  binding the section describes ("Each sequencer is dedicated to a specific
  RC Client and its bound endpoints"; the field "refers the Client Nr
  allowed to access this sequencer") — with `Seq_2` at `0x0002` fixing the
  stride at 2 bytes. This crate carried only `seq_state` with
  `ENCODED_LEN = 1`, so a multi-sequencer table read through `decode_rows`
  both lost every sequencer's client binding and misaligned every row after
  the first. The field is now present and `ENCODED_LEN` is 2.

Not changed, and still to reconcile: `regmap::ResponseStreamConfigEntry`'s
layout has not been checked against §12.7.9 in this pass and remains this
crate's own inference; and `RequestStreamConfigEntry::default()` is still
all-zero, where Table 22 documents a default of `1` for
`rx_resp_stream_index`.

## v4.0.0 (2026-07-31 TC18-conformant NTSCF/TSCF AVTPDU header) — closed

**Breaking, on the wire.** `src/avtp.rs`'s NTSCF and TSCF header
encode/decode were never reconciled against the specification — that
module's own provenance note said as much, calling `TSCF_SUBTYPE` and "the
header's total length" this crate's "own placeholder values pending that
reconciliation" — and both were wrong. This matters more than the
comparable `v3.0.0` ACF finding did: TC18 §12.2 lists "NTSCF header
processing" as the first of exactly four **mandatory** features, and every
transport this crate ships (`udp`, `l2`, `shmem`, `tlstransport`, `mock`)
frames through it unconditionally. Every NTSCF/TSCF frame rust-RCP has ever
emitted was malformed; no release before this one could have interoperated
with a conformant RC Server, and there is correspondingly no
backward-compatibility constraint to preserve.

Verified against the specification's own **normative** field diagrams,
§11.1 "Usage of IEEE1722 for RCP", page 22 — Figure 5 "TSCF-Header Version
0" and Figure 6 "NTSCF-Header Version 0" — and cross-checked against the
worked examples on page 79, Figure 19 (ACF_ABB under TSCF,
`stream_data_length(octets) = 0x003C`) and Figure 20 (ACF_GBB under NTSCF,
`ntscf_data_length = 0x038`). All four are vector images with no
extractable text layer in the source PDF; the bit-boundary tick marks were
counted from a 600 dpi render of both pages.

- **rust-RCP-H01 (BREAKING):** `avtp::NTSCF_HEADER_LEN` is **12**, was 16.
  Figure 6's NTSCF header is exactly three quadlets — one packed quadlet
  plus a 64-bit `stream_id` — with `acf_payload_data` starting immediately
  at octet 12. The previous layout inserted three fabricated reserved
  octets between the length field and `stream_id` that the specification
  does not have.
- **rust-RCP-H02 (BREAKING):** NTSCF's first quadlet is reordered to
  Figure 6's actual field order. `ntscf_data_length` (11 bits) sits at bits
  13-23, i.e. *before* `sequence_num` (bits 24-31) — packed as the low 3
  bits of octet 1 followed by all of octet 2. The previous layout put
  `sequence_num` at octet 2 and split the length across octets 3-4 with its
  low 3 bits left-justified into octet 4's top 3 bits. Neither the order
  nor the packing was right.
- **rust-RCP-H03 (BREAKING):** `avtp::TSCF_SUBTYPE` is **`0x05`**, was
  `0x83`. Figures 5 and 19 both give `subtype(0x05)`. `0x83` was invented
  as "`NTSCF_SUBTYPE` plus one"; the two subtypes are unrelated IEEE 1722
  code points and TSCF's is the smaller.
- **rust-RCP-H04 (BREAKING):** TSCF's field positions are corrected
  throughout to Figure 5's six quadlets — `stream_id` at octets 4-11 (was
  8-15), `avtp_timestamp` at 12-15 (was 16-19), a reserved quadlet at
  16-19, and `stream_data_length` at 20-21 (was split across octets 3-4).
  The 24-octet *total* was coincidentally right; nothing inside it was.
- **rust-RCP-H05 (BREAKING):** `avtp::TSCF_DATA_LENGTH_MAX` is
  **`0xFFFF`**, was `0x07FF`. Figure 5 gives `stream_data_length` a full
  16-bit half-quadlet of its own; only NTSCF's `ntscf_data_length` is
  11 bits. `encode_tscf_header` previously rejected every legal length from
  2048 upward with `InvalidSize`, and now accepts the whole `u16` range —
  it can no longer fail, though it keeps its `Result` return for symmetry
  with `encode_ntscf_header` and for future validation headroom.
- **rust-RCP-H06:** `encode_tscf_header` now emits Figure 5's `tv`
  ("timestamp valid") bit at bit 15, derived from `avtp_timestamp` being
  non-zero — the same all-zero-is-untimed sentinel `timestamp::AvtpTimestamp`
  already defines. Previously that bit was always transmitted as zero, so
  every TSCF frame this crate sent declared its own timestamp invalid.
- **rust-RCP-H07 (test quality):** `conformance::golden`'s NTSCF/TSCF/frame
  byte arrays were *captured from this crate's own encoder output*, which
  made them tautological — they could only ever catch drift away from
  whatever the encoder did first, never that it was wrong to begin with,
  and in practice they certified all six defects above as correct. Every
  golden array is now derived by hand from the TC18 figures, and each
  constant's doc comment carries an octet-by-octet derivation table naming
  the exact figure, page, and field. New spec-anchored tests
  `ntscf_header_matches_figure_20_worked_example`,
  `tscf_header_matches_figure_19_worked_example` and
  `tscf_tv_bit_tracks_avtp_timestamp_presence` pin the worked examples
  directly, mirroring `v3.0.0`'s `acf_*_matches_figure_*` tests.
- **rust-RCP-H08 (docs):** `src/conformance.rs`'s module doc comment
  recorded this exact byte divergence against go-RCP (13 vs. 16 octets) and
  explicitly declined to resolve it, calling reconciliation "out of scope
  for this item". That was the wrong call: it was the real bug. The section
  is rewritten as a resolved finding, including the observation that both
  implementations *agreed* on the wrong `subtype 0x83` — a cross-
  implementation comparison can show that one side is wrong, never which,
  and when both agree it cannot show even that. go-RCP's own 13-octet
  untimed header is likewise non-conformant and is now recorded as such.
- `.fusa-reqs.json`: `REQ-NTSCF-001..004`, `REQ-TSCF-001..004`,
  `REQ-WIRE-001/004/007/009` and `REQ-CONF-001/002/005` are rewritten to
  state the real wire format and cite the TC18 figure and page each derives
  from, replacing text that specified the wrong constants as correct (e.g.
  `REQ-NTSCF-004` read "rejects frames shorter than 16 bytes"). 564/564
  requirements still fully traced.
- `docs/PUBLIC_API.txt`: `encode_ntscf_header`'s return type narrows to
  `Result<[u8; 12], RcpError>`. `docs/SEMVER.md` gains an explicit rule
  that a wire-format change is a MAJOR bump even when it is a fix, and its
  stale "the version does not move until `v1.0.0`" scheme note is retired.

## v3.2.0 (2026-07-31 real UDP socket + new L2 raw-Ethernet transport) — closed

This crate's transport layer had two real gaps, confirmed by direct
inspection rather than assumption: no raw-Ethernet/L2 transport existed at
all (the same gap every other RCP-family repo — `go-RCP`, `cpp-RCP`,
`c-RCP` — has), and `src/udp.rs`'s `UdpSocket` trait had no implementation
over a real OS socket either — only the in-process `EchoUdp`/
`QueuedUdpSocket` test doubles, and `src/bin/rcp.rs`'s own prior doc
comment admitted this plainly. TC18 §10.1 names both a layer-2 EtherType
(`0x22F0`) and UDP/IP encapsulation ("described in Annex J", of the base
IEEE 1722-2016 standard) as legal transports; this item builds both as
permanent, first-class, equally-supported options, closing all three real
gaps rather than just one — this is the first real network I/O this crate
has ever shipped for RCP.

- **rust-RCP-NET-01 (feature):** `src/udp.rs` gains
  [`StdUdpSocket`], a real `UdpSocket` implementation over a bound
  `std::net::UdpSocket`, corrected to IEEE 1722-2016 Annex J framing from
  the start (there was no legacy UDP wire format to preserve). Every
  `send_to` prepends, and every `recv_from` strips, a 4-byte big-endian
  "encapsulation sequence number" ([`encode_annex_j_udp_payload`]/
  [`decode_annex_j_udp_payload`]) — a per-`StdUdpSocket` monotonically
  increasing counter with no invented receiver-side semantics (e.g. loss
  detection) beyond that. New constants `ANNEX_J_CONTROL_PORT` (17221,
  the applicable port for RCP's control-plane request/response/
  acknowledgement traffic, and `StdUdpSocket::new_default_port`'s
  default) and `ANNEX_J_CONTINUOUS_PORT` (17220, streaming traffic, named
  but unused). **Provenance note**, stated once here and referenced from
  every touchpoint in code: this crate has no access to the paywalled
  IEEE 1722-2016 standard text: the port numbers and the sequence-number
  field are taken from two independent public secondary sources instead
  — a Wireshark issue tracker discussion of the real Annex J framing, and
  the COVESA Open1722 open-source reference implementation's `Avtp_Udp_t`
  header struct (`include/avtp/Udp.h`, BSD-3-Clause,
  <https://github.com/COVESA/Open1722>) — and are flagged as such rather
  than presented with false certainty. New `REQ-UDP-012`/`REQ-UDP-013`/
  `REQ-UDP-014`.
- **rust-RCP-NET-02 (feature):** new `src/l2.rs` — a raw-Ethernet (layer
  2) transport, Linux only, mirroring `src/udp.rs`'s own
  `UdpSocket`/`UdpTransport` abstraction one wire layer down:
  [`encode_ethernet_frame`]/[`decode_ethernet_frame`] (destination MAC +
  source MAC + EtherType `0x22F0` big-endian + the AVTPDU bytes directly
  — no encapsulation sequence number; that field is Annex J/UDP-specific
  and has no L2 counterpart), an [`L2Socket`] trait mirroring `UdpSocket`
  (`SocketAddr` replaced by a raw `[u8; 6]` MAC), [`L2Transport`]
  mirroring `UdpTransport`'s `send_acf_abb`/`send_acf_gbb` client shape,
  and — `target_os = "linux"` only — [`RawEthernetSocket`], a real
  `AF_PACKET`/`SOCK_RAW` production `L2Socket` that reads its own
  interface's MAC via `getifaddrs` rather than requiring the caller to
  supply one (a caller-supplied destination MAC is still required —
  multicast-MAC derivation is a base-IEEE-1722 algorithm this crate does
  not have). Every other target gets a same-named stub whose `bind`
  always returns a clear `Err` rather than silently no-op-ing, so the
  type can be referenced unconditionally. Server-side L2 dispatch (an
  `L2RcServer` mirroring `UdpRcServer`) is out of scope for this item —
  flagged as a deliberate follow-up, not bundled in silently; this item's
  server-facing wiring is `UdpRcServer` run over `StdUdpSocket` (see
  rust-RCP-NET-03 below). New `REQ-L2-001` through `REQ-L2-008`.
- **A flagged judgment call — `nix`, not raw `libc` `unsafe` syscalls:**
  this crate is `#![forbid(unsafe_code)]` crate-wide, and `forbid` cannot
  be locally overridden (E0453) — `src/capi.rs`'s own doc comment already
  named this rule as the reason this crate has never built a raw-pointer
  FFI boundary. A direct `libc` `socket()`/`bind()`/`sendto()`/
  `recvfrom()` implementation would require `unsafe extern "C"` calls in
  this crate's own source, which is not available at all here, not a
  style choice. `RawEthernetSocket` is instead built on the `nix` crate
  (`target_os = "linux"`-only dependency, new to `Cargo.toml`), whose
  `socket`/`bind`/`sendto`/`recvfrom`/`setsockopt`/`getifaddrs` functions
  are all safe Rust `fn`s — `unsafe` lives inside `nix`'s own crate,
  never this one's — confirmed against `nix` 0.31's published API before
  writing the module, not assumed. `nix` is a narrowly-scoped
  POSIX-bindings crate, not a heavyweight packet-crafting framework like
  `pnet`, matching this item's own minimal-footprint intent.
- **rust-RCP-NET-03 (feature):** `src/bin/rcp.rs` gains a new `serve --udp
  <bind-ip> [--port <n>] [--stream <hex>] [--max-requests <n>]` command —
  the first `rust-rcp` command backed by a real OS socket instead of an
  in-process `RcServer` invoked directly. It binds a real `StdUdpSocket`
  and runs `UdpRcServer` (previously only ever exercised against mock
  sockets in this crate's own unit tests) against it. `discover`/
  `register`/`endpoint` remain deliberately ephemeral/in-process, per
  this file's own pre-existing "Provenance note" (unchanged by this
  item); `serve` is a new, additive, real-network-facing command, not a
  replacement for them. The module doc comment's prior "no concrete
  `rcp::udp::UdpSocket` implementation over a real OS socket" note is
  updated accordingly. New `REQ-CLI-010`.
- **Tests, no privileges/Linux required:** pure byte-manipulation round
  trips for both the Annex J encapsulation
  (`annex_j_encode_decode_round_trips`, short-buffer rejection) and the
  Ethernet frame encode/decode (`ethernet_frame_encode_decode_round_trips`,
  short-frame/wrong-EtherType rejection), plus mock-socket-backed
  `L2Transport`/`UdpTransport` request/response tests (`EchoL2`/`QueuedL2`,
  the `L2Socket` analogs of `udp`'s own `EchoUdp`/`QueuedUdpSocket`) — all
  run everywhere, no real socket involved.
- **Tests, real sockets:** a real loopback `StdUdpSocket` round trip
  (`std_udp_socket_round_trips_over_real_loopback_socket`), a test
  proving the encapsulation sequence number actually increments on the
  wire by inspecting raw bytes with a bypass `std::net::UdpSocket`, a
  real receive-timeout test, and a new end-to-end test composing a real
  `StdUdpSocket` client against a real `StdUdpSocket` + `UdpRcServer`
  server over real loopback sockets
  (`std_udp_socket_and_udp_rc_server_serve_a_real_discovery_request_end_to_end`)
  — all run in the normal cross-platform `test` CI job (ubuntu/macos/
  windows), no privileges required.
- **New Linux-only CI job (`l2-veth`):** creates a real `veth0`/`veth1`
  pair under `sudo`, then runs a `#[cfg(target_os = "linux")]`,
  `#[ignore]`d-by-default test
  (`real_raw_ethernet_socket_round_trips_a_frame_over_a_veth_pair`) with
  `-- --ignored`, proving a real `RawEthernetSocket` frame round-trips
  byte-for-byte over a real (virtual) Ethernet link — not just that the
  framing/trait logic type-checks.
- This is a MINOR (additive, non-breaking) release: `StdUdpSocket`,
  `ANNEX_J_CONTROL_PORT`/`ANNEX_J_CONTINUOUS_PORT`,
  `encode_annex_j_udp_payload`/`decode_annex_j_udp_payload`, and the
  entire new `l2` module are new `pub` items only — no existing item
  changed shape. `docs/PUBLIC_API.txt` is regenerated accordingly (purely
  additive diff) per `docs/SEMVER.md`; `.fusa-reqs.json` gains
  `REQ-UDP-012`-`REQ-UDP-014`, `REQ-L2-001`-`REQ-L2-008`, and
  `REQ-CLI-010` (564/564 traced).

## v3.1.0 (2026-07-31 E2E CRC trailer wire-order fix) — closed

While independently verifying `v3.0.0`'s `acf` wire-format rework byte-for-
byte against the real TC18 v0.5.1_RC PDF, a pre-existing correctness bug
(not introduced by `v3.0.0`) was found in this crate's E2E safety
mechanism: the two golden-vector tests meant to pin the specification's
own Figure 19 (ACF_ABB) / Figure 20 (ACF_GBB) worked examples
(`acf_abb_matches_figure_19_worked_example`/
`acf_gbb_matches_figure_20_worked_example`, in `src/acf.rs`) built their
test payload as `real_payload + crc_bytes` concatenated together and
handed that combined blob straight to `encode_acf_abb`/`encode_acf_gbb` —
which then appended their own automatically-derived padding *after* that
whole blob, producing the wire order `payload, CRC, pad`. TC18's own two
worked examples show the real order is `payload, pad, THEN the CRC32
trailer` — pad strictly *before* the CRC, not after. Both tests only
asserted total frame length, `acf_msg_length`, and `pad` count — never
actual byte positions — so this passed silently: those three values are
identical either way, since the CRC trailer is always exactly one quadlet
and doesn't change any padding-count arithmetic mod 4. The same bug class
was independently found and is being fixed in `go-RCP`'s equivalent
module; `cpp-RCP`/`c-RCP` already get this right.

- **rust-RCP-E2E-01 (fix):** `src/e2e.rs` gains
  [`finalize_crc_trailer`]/[`split_crc_trailer`] — the correct, composable
  encode/decode primitives for a CRC-protected ACF_ABB/ACF_GBB wire frame.
  `finalize_crc_trailer(frame, crc)` takes an already-encoded, CRC-free
  frame (built from the real payload alone, so `acf::encode_acf_abb`/
  `acf::encode_acf_gbb`'s own automatic `pad` already lands immediately
  after the real payload), bumps its `acf_msg_length` by one quadlet, and
  appends the CRC — producing TC18's real `payload, pad, CRC` order.
  `split_crc_trailer(frame)` is the mirror-image decode-side operation:
  it un-adjusts `acf_msg_length` and strips the trailing CRC octets
  *before* handing the remaining bytes to `acf::decode_acf_abb`/
  `acf::decode_acf_gbb`, so their existing `pad`-stripping logic (which
  strips exactly `byte_message_info.pad` octets from the end of the
  `acf_msg_length`-described region) recovers the real payload correctly
  instead of misreading the CRC trailer's own trailing bytes as padding.
  New `REQ-CRC-012`/`REQ-CRC-013` cover both.
- **rust-RCP-E2E-02 (test fix):** the two golden-vector tests move from
  `src/acf.rs` to `src/e2e.rs` (as
  `finalize_crc_trailer_matches_figure_19_worked_example`/
  `finalize_crc_trailer_matches_figure_20_worked_example`, since only
  `e2e.rs` actually assembles a CRC-protected frame — `acf.rs`'s own
  encoders have no CRC-trailer concept of their own) and now assert the
  ACTUAL byte sequence at every offset, not just totals — proving
  `payload, pad, CRC` ordering explicitly, byte for byte, against both
  worked examples. A new regression-guard test,
  `finalize_crc_trailer_never_places_pad_after_crc`, reproduces the old,
  buggy `payload + crc_bytes` concatenation pattern side by side with the
  fixed construction and asserts they differ.
- No change to the CRC32 polynomial, algorithm, or `byte_message_info`
  bit layout — `v3.0.0`'s wire-format rework is untouched. This is a MINOR
  (additive, non-breaking) release: `finalize_crc_trailer`/
  `split_crc_trailer` and their two constants are new `pub` items only;
  `docs/PUBLIC_API.txt` is regenerated accordingly per `docs/SEMVER.md`.

## v3.0.0 (2026-07-31 TC18 wire-format conformance fix pass) — closed

**Breaking.** A cross-repo gap-audit pass found this crate's `acf` module —
`byte_message_info`'s bit layout and `acf_msg_length`'s unit — was an
invented placeholder, never actually reconciled against the real OPEN
Alliance TC18 v0.5.1_RC specification text despite that module's own
long-standing provenance note flagging it as unconfirmed. This release
replaces that placeholder with the specification's real layout, pixel-
verified against TC18 Figure 7 / Table 4 (page 24) and cross-checked
against the specification's own two worked examples (Figure 19, Figure
20). No prior rust-RCP release — including `v1.0.0`/`v2.0.0`, both of
which claimed the Milestone 10 TC18 uplift was complete — actually
produced TC18-conformant bytes on the wire; this is the first release
that does.

- **rust-RCP-W01 (BREAKING):** `acf::ByteMessageInfo`'s wire layout is
  fully rebuilt to match TC18 §11.2.1 Figure 7 / Table 4 exactly, field by
  field:
  - `acf_msg_type` (7 bits) is now folded into `byte_message_info`'s own
    first octet, not a separate leading discriminant byte —
    `ACF_ABB_HEADER_LEN`/`ACF_GBB_HEADER_LEN` both shrink by one byte (8 and
    16, was 9 and 17). `ByteMessageInfo` gains a new `acf_msg_type: u8`
    field; `encode_acf_abb`/`encode_acf_gbb` always overwrite it with the
    correct discriminant.
  - `acf_msg_length` is now a 9-bit field (was modeled as 11 bits).
  - `pad` is now a 2-bit octet *count* (`u8`, `ByteMessageInfo::pad`), not a
    1-bit presence flag (`bool`) — a real, breaking type change on a public
    struct field. Same change mirrored onto `capi::CByteMessageInfo::pad`.
  - `byte_bus_id` (still 11 bits) and `evt` (still a 1-bit `ack` + 3-bit
    `sub_opcode` pair) move from row 1 to their correct row-2 position
    relative to the other fields.
  - `transaction_num` now comes *before* the `op`/`rsp`/`err`/`ms` flag
    group on the wire (previously placed after).
  - `read_size_segment` (`ReadSizeOrSegment`) is now a 12-bit field (was
    modeled as a full 16 bits) sharing its trailing octet with the
    `op`/`rsp`/`err`/`ms` flags.
- **rust-RCP-W02 (BREAKING):** `acf_msg_length` is now correctly counted in
  **quadlets over the entire ACF message** (header + `message_timestamp`
  for ACF_GBB + payload + pad), confirmed against TC18's own Figure 19
  (ACF_ABB: 8-byte header + 6 payload + 2 pad + 4-byte CRC32 = 20 bytes = 5
  quadlets) and Figure 20 (ACF_GBB: 8-byte header + 8-byte timestamp + 7
  payload + 1 pad + 4-byte CRC32 = 28 bytes = 7 quadlets) worked examples —
  not the payload-only count the previous placeholder used.
  `encode_acf_abb`/`encode_acf_gbb` now compute and append real, non-zero
  padding octets to round the message up to a quadlet boundary (rather than
  the strict "never transmit padding" rule the placeholder implementation
  enforced), and the decoder honors a peer's real `pad` count instead of
  rejecting anything but zero padding. Golden-vector tests
  (`acf_abb_matches_figure_19_worked_example`/
  `acf_gbb_matches_figure_20_worked_example`) pin both worked examples
  byte-for-byte.
- **rust-RCP-W03:** an RC Server must support multiple ACF_ABB requests
  concatenated in a single frame (TC18 §12.9.1.1). `acf::decode_acf_abb_messages`
  splits a frame body into as many self-delimited ACF_ABB messages as it
  actually contains (using each message's own `acf_msg_length` as the
  delimiter, the same self-describing scheme the wire format itself uses);
  `mock::RcServer::handle_ntscf_frame` and `udp::UdpRcServer::serve_one`
  both now dispatch every request in a frame and concatenate every response
  into the one outgoing frame, instead of processing only the first (and
  only) message a frame was previously assumed to carry.
- **rust-RCP-W04/W05:** wire-level `err=1` error responses. `RcpError`
  gains a `tc18_wire_code(&self) -> Option<u8>` method mapping every one of
  TC18 Table 27's seventeen named error codes to its real wire value
  (independent of `capi::CError`'s own, incompatible internal C-ABI
  numbering) — including four codes with no prior `RcpError` variant at
  all (`PwmInNoSignal`, `PociFailure`, `PresentationTimeTooFar`,
  `GptpFail`). `acf::build_error_response` builds a real `err=1` ACF_ABB
  response (echoing `byte_bus_id`/`transaction_num`, `byte_msg_payload` =
  the Table 27 code) for any dispatch failure that has one. Both
  `RcServer::handle_ntscf_frame` and `UdpRcServer::serve_one` now answer a
  per-request dispatch failure this way instead of only ever surfacing it
  as a local `Result` to their own caller — per TC18 §12.9.1.1's "check
  each of them individually if to be processed or not" and §12.9.6
  "Handling errors", this also means one bad request inside a
  multi-request frame (rust-RCP-W03) no longer silently drops the
  responses to its neighbors.

Every existing golden-vector/round-trip test that baked in the old
(invented) layout is updated to the corrected one, not deleted —
`conformance.rs`'s frozen golden byte arrays are recomputed against the
new encoder and re-pinned, with their previous byte values kept out of the
file (not silently dropped: see this entry and `docs/PUBLIC_API.txt`'s own
diff for what changed).

**Deferred, not part of this pass** (tracked, not silently dropped):
rust-RCP-W06 (no peripheral endpoint type has a concrete `Endpoint` trait
implementation reachable from live dispatch), rust-RCP-W09 (the
conditional-request/execution-priority machinery in `request.rs` is not
wired into `RcServer`'s real dispatch path), rust-RCP-W10 (`e2e`'s CRC
verification and `fragment`'s reassembly buffer are not wired into the
real dispatch path), rust-RCP-W07/W08 (GPIO/PWM request-level
exactly-N-bytes enforcement) — all four are large, standalone dispatch-
architecture rebuilds blocked on rust-RCP-W06 existing first, out of scope
for a single wire-format-focused pass. rust-RCP-W11 (CI action SHA-pinning,
bench-fallback masking) and rust-RCP-W12 (DAC endpoint, correctly
out-of-scope) are low-severity/non-mandatory and also deferred.

## v2.0.0 (2026-07-29/30 ecosystem audit fix pass) — closed

A 23-issue fix pass against the 2026-07-29/30 cross-repo ecosystem audit,
covering wire-conformance defects, HARA/requirements-traceability
corrections, documentation-honesty fixes, and the removal of the last
retired-model (`Zone`/`Controller`/`Registry`) residue from the frozen
public API. Landed as nine PRs, merged individually once each was
independently green.

**Post-release correction (2026-07-30):** an independent verification pass
of this milestone found `HARA.md`'s Safety Goals table had drifted out of
sync with the already-correct `.fusa-hara.json` for three rows (SG-003,
SG-006, SG-010 all showed a stale ASIL one band above the JSON source of
truth) — a documentation-only inconsistency, not a defect in the enforced
JSON data or its S/E/C derivation. `scripts/hara_asil_check.py` is
extended to cross-check both of `HARA.md`'s tables against
`.fusa-hara.json` directly, so this specific drift (doc vs. already-correct
JSON, as opposed to a bad S/E/C-to-ASIL derivation) can't silently
reoccur. Also corrected two leftover stale strings from mid-pass drafts:
a `ci.yml` comment that still described the `convert` CLI subcommand as
removed (it was rebuilt, not removed — see rust-RCP-FS-01 above) instead
of explaining the real reason `relay interop` currently shows red
(`SoundMatt/RELAY#70`, an external blocker), and a CI job still named
"RELAY spec v1.11 unit conformance" despite this milestone's own
`SPEC_VERSION` fix to `"2.0"`.

**Post-release correction (2026-07-30):** `SoundMatt/RELAY#70` — the
`go.mod` module-path bug that made `go install .../relay@latest` silently
resolve to a stale pre-`/v2` `v1.14.0` reference binary — is fixed
upstream as `SoundMatt/RELAY` `v2.0.4`. `ci.yml`'s `relay-conform` and
`relay-interop` jobs now install
`github.com/SoundMatt/RELAY/v2/cmd/relay@v2.0.4` (pinned, not
`@latest`) instead of the unversioned `github.com/SoundMatt/RELAY/cmd/relay@latest`
path. Verified locally against a from-source build of the RELAY v2.0.4
tag: `relay conform --strict` passes and `relay interop --strict
--protocol RCP` now reports `EQUIVALENT` for `rcp-message` (previously
`ERROR` under the stale `v1.14.0` binary) against this crate's `convert`
CLI subcommand. Also fixes an independent, previously-latent bug this
exposed: `ci.yml`'s `relay-interop` job grepped its output for the
retired `rcp-status` vector name rather than `rcp-message`, the name
RELAY's own v2.0 canonical-type replacement renamed it to (see
`spec/vectors/rcp-status.json` -> `rcp-message.json` in RELAY's
CHANGELOG) — so even with the `/v2` path fixed, the check would have
silently no-opped ("no RCP vectors found") instead of ever asserting
`EQUIVALENT`. No source or behavior change in this crate; CI-only fix.

### Breaking

- **`RcpError::ZoneMismatch` and `is_zone_mismatch()` are removed.**
  Neither had a TC18 protocol counterpart — every live construction site
  was the pre-Milestone-10 `Zone` model itself — and freezing them into
  the semver-stable public API (`docs/PUBLIC_API.txt`) was itself the
  defect (rust-RCP-FS-02). No replacement or compatibility shim is
  provided, matching this crate's established "no shim for retired-model
  surface" precedent. The three `.fusa-reqs.json` requirements that
  existed solely to hold this sentinel in the verified ASIL-B baseline
  (`REQ-ERR-011`/`018`/`021`) are retired alongside it (rust-RCP-FS-04).
- **`acf::ReadSizeOrSegmentNum` is renamed to `acf::ReadSizeOrSegment` and
  widened from `u8` to `u16`**, matching the RELAY specification's
  canonical §15.5 cross-language type. Ripples into every module that
  reused the type (`uart`, `capi`, `fragment`, every `mock::Endpoint`
  decorator's `read_size` parameter).
- **`gpio::GpioWriteSemantics` and `spi::SpiChannelSelect` variants
  renamed/reassigned** to correct their `evt[2:0]` sub-opcode mapping
  against the endpoint-specific evt-bits table (rust-RCP-02/03):
  `GpioWriteSemantics::Add` → `AddSaturating` (now saturating, not
  wrapping) at a different sub-opcode, a new `Reserved4` rejects the
  spec-reserved code; `SpiChannelSelect::Spare6`/`Spare7` →
  `Reserved6`/`Reconfigure7`.
- **`iseled::iseled_frame_crc8`/`IseledFrameCrc` are removed from the
  default feature set**, gated behind the new opt-in
  `iseled-unconfirmed-crc` Cargo feature (rust-RCP-06) — this crate never
  recovered ISELED's own confirmed CRC parameters, so the invented
  CRC-8/AUTOSAR stand-in is no longer presented as an ordinary shipped
  primitive.
- **`RcpError::Closed`/`Timeout`/`NotFound`/`AlreadyExists`/`Busy`'s
  `Display` text** drops retired `Zone`-model wording (rust-RCP-FS-03) —
  e.g. `"rcp: zone not found"` → `"rcp: not found"`. The variants
  themselves are unchanged.
- The `rust-rcp convert` CLI subcommand is rebuilt against the real
  canonical `rcp.Message` type (RELAY spec §15.5) and its `ToMessage()`
  conversion (rust-RCP-FS-01), addressing by decimal `byte_bus_id`
  (matching `rcp.EndpointIDString`/`ParseEndpointID` in RELAY's own Go
  reference package), in place of the retired placeholder `Zone`-numbered
  `Status` document (`zone`/`seq`/`healthy`/`payload`) that used to map a
  `zone` field to a `FrontLeft`/…/`Central` positional-speaker name with
  no TC18 counterpart. Verified byte-for-byte equivalent (per `relay
  interop`'s own comparison method) against a from-source build of the
  RELAY v2.0 reference `relay convert --protocol RCP` across several
  cases, including the published `spec/vectors/rcp-message.json` vector.
  **External blocker (resolved 2026-07-30)**: this repo's CI installed
  the reference `relay` tool via `go install .../relay@latest`, which
  resolved to a stale, still-Zone-based `v1.14.0` — `SoundMatt/RELAY`'s
  `go.mod` had not added the `/v2` module-path suffix Go's semantic
  import versioning rules require before any `v2.x` tag becomes
  `go install`-able at all (confirmed by direct testing: `go install
  github.com/SoundMatt/RELAY/cmd/relay@v2.0.2`, an explicit pinned
  version, was refused with the same "module path must match major
  version" error `@latest` implicitly hit — there was no `go install`
  invocation from this side that could reach it). Tracked upstream as
  [`SoundMatt/RELAY#70`](https://github.com/SoundMatt/RELAY/issues/70).
  Until that landed, the `RELAY interop` CI job (not a required
  branch-protection check) showed `ERROR` for `convert` against the stale
  reference rather than `EQUIVALENT`. RELAY shipped the `/v2` fix as
  `v2.0.4`; this repo's CI now installs `github.com/SoundMatt/RELAY/v2/cmd/relay@v2.0.4`
  and `relay interop` shows `EQUIVALENT` for `rcp-message` (RELAY v2.0
  renamed the vector from `rcp-status`) against a genuine RELAY v2.0
  reference build (see "Post-release correction" below).

### Fixed

- `acf::encode_acf_abb`/`encode_acf_gbb` now derive `acf_msg_length` from
  the real payload length instead of trusting an unvalidated caller value;
  `decode_acf_abb`/`decode_acf_gbb` cross-check it against the actual
  payload present (rust-RCP-N2-05).
- `ByteMessageInfo::read_size()`/`segment_num()` select the
  `ReadSizeOrSegment` field's interpretation by the `op` bit, rather than
  two accessors that returned the same value regardless of which
  interpretation applied (rust-RCP-05).
- `lifecycle::lock_policy(RegisterCategory::General)` is now
  `Some(LockPolicy::W)` — the EP0 register-map write path was previously
  dead by construction for every caller, including the root client
  (rust-RCP-12).
- `Cargo.lock` is committed (previously gitignored despite this crate
  shipping a binary) and `--locked` is enforced across the release build,
  cross-platform test job, and SBOM generation (rust-RCP-N2-02).
- `HARA.md`/`.fusa-hara.json` ASIL misclassifications corrected against
  ISO 26262-3:2018 Table 4: H-010/SG-010 (S1/E4/C3) was under-classified
  ASIL-A, corrected to ASIL-B; H-003/SG-003 and H-006/SG-006 (S2/E3/C2)
  were over-classified ASIL-B, corrected to ASIL-A; H-008/SG-008's
  untenable `C0` controllability input is corrected to `C1`
  (rust-RCP-N2-03/N2-04). A new `scripts/hara_asil_check.py`, run in CI,
  mechanically re-derives every hazard's ASIL from its own S/E/C fields.
- `RequestKind` is bound to `AcfGbbMessage::message_timestamp`'s leading
  byte for GBB conditional requests, rather than existing as a
  wire-unattached value enum (rust-RCP-04).
- `src/bin/rcp.rs`'s `capabilities` output now carries a
  `"no-live-subscribe"` entry in `"features"`, documenting that
  `RcpAdapter::subscribe` has no live asynchronous-notification mechanism
  (rust-RCP-15).

### Documentation

- Declared RELAY spec version corrected from the stale `"1.11"` (and an
  internally-inconsistent stray `"v1.14"` citation) to the current `v2.0`
  across `lib.rs`, `relay.rs`, `README.md`, `.fusa.json`, and the CLI's
  `capabilities`/`version` output (rust-RCP-07).
- `ROADMAP.md`'s Compliance Targets table, which contradicted its own
  milestone checklists by describing the TC18 uplift as "Not started",
  reconciled to reflect the actual v1.0.0-complete state; its Satellite
  Package Disposition table's 14 references to already-deleted modules
  corrected (rust-RCP-08/09).
- Module count reconciled to 52 (51 public + `base64_serde`) across
  `README.md`'s prose and module-index table (rust-RCP-10).
- `SECURITY.md`/`SAFETY_PLAN.md`/`CONTRIBUTING.md`/`.fusa.json` placeholder
  `*@example.com` contacts replaced; `SECURITY.md`'s supported-versions
  table updated for the then-current `1.x` release (rust-RCP-11).
- `discovery.rs`'s invented broadcast-addressing/register-prefix
  conventions and `ROADMAP.md`'s Milestone 3 section now carry a
  consistent "unreconciled, not for interop reliance" caveat
  (rust-RCP-14).
- Unit tests that had locked in the pre-fix GPIO/SPI evt-mapping bugs and
  the always-fails EP0 write rewritten to assert the corrected behavior
  (rust-RCP-13).

## v1.0.0 (Milestone 10) — closed

### Changed

`src/adapt.rs`'s RELAY spec `Adapt()`/`to_message()`/`from_message()`/
`response_to_message()` binding is rebuilt against `mock::RcServer`'s
`(avtp::StreamId, byte_bus_id)`-addressed model, replacing the
zone-name-as-`Message.id` mapping this module's own Milestone 9 doc comment
had explicitly deferred to this item. `adapt()` now takes an
`Arc<mock::RcServer>` in place of `Arc<dyn Controller>`; no reference to
`Zone`/`Command`/`CommandType`/`Priority`/`Controller`/`Response`/`Status`/
`zone_from_str` remains in the module. `Adapter`/`AdaptEndpoint`/
`PassthroughAdapter` (already retargeted in Milestone 9) are untouched.

Three flagged design choices this rebuild made where neither the RELAY spec
nor `ROADMAP.md` pin one down (see `src/adapt.rs`'s own provenance note for
the full reasoning):

- `Message.id` now encodes `(stream_id, byte_bus_id)` as
  `"<16 hex digits>.<decimal byte_bus_id>"` (`format_endpoint_id`/
  `parse_endpoint_id`), rejecting malformed input with
  `RcpError::InvalidParameter` rather than panicking.
- `from_message` infers read vs. write from an optional `"rcp.op"` meta key,
  defaulting to whether `msg.payload` is empty (`RcServer::handle_abb` has
  no third "no-op" case); an optional `"rcp.read_size"` meta key (default
  `u8::MAX`) supplies the read byte count.
- `RcpAdapter::subscribe` returns an immediately-closed channel rather than
  inventing a notification source: `mock::RcServer` still has no live
  asynchronous-notification mechanism (a gap that module's own doc comment
  already named, and this item does not resolve). The retired
  `Controller::subscribe`-forwarding plumbing (`AdaptQueue` and its
  blocking-producer task) is removed rather than kept as dead code;
  whichever later item gives `RcServer` a live-notification mechanism can
  reintroduce the same `relay::BackPressurePolicy`-driven shape once it has
  something real to forward.
- `RcpAdapter` tracks its own `closed` flag, since `RcServer` has an
  `RcServerState` lifecycle position rather than an open/closed connection
  boolean for `Node::close` to delegate to.

### Traceability

`.fusa-reqs.json` `REQ-ADAPT-006`/`007`/`008`/`009`/`010` text is updated
to describe the new `AcfAbbMessage`/`RcServer`-based behavior; a new
`REQ-ADAPT-011` covers the endpoint-address encode/decode pair.
`src/adapt.rs`'s tests are rewritten against `mock::RcServer`/
`MockEndpoint` in place of `MockController`/`Zone`/`Command`/`Response`/
`Status` (20 tests, up from 14).

`cargo build --all-targets`, `cargo clippy --all-targets --all-features --
-D warnings`, `cargo test --all-targets` (1062 lib tests, up from 1056;
19 unchanged `src/bin/rcp.rs` tests), and `cargo fmt --all -- --check` are
clean; `bash scripts/fusa-gap-check.sh` reports 622/622 (100%) requirements
traced; `bash scripts/cyber-gap-check.sh` reports 6/6 threats with tested
countermeasures.

### Changed — FuSa artifact re-basing

`HARA.md`/`.fusa-hara.json`, `SAFETY_PLAN.md`, and `tara.json` are rewritten
against the TC18 core Milestones 1-9 built, superseding the versions that
described the replaced private `Zone`/`Command`/`Controller`/`Registry`
protocol.

`HARA.md`/`.fusa-hara.json`: H-001/H-005 (endpoint misaddressing, per-stream
watchdog lockup) and H-002 (safety-tagged request loss) are retargeted onto
`addressing::EndpointTable`, `watchdog::StreamWatchdogState`, and
`request::check_watchdog_overflow_purge`. H-010 ("Registry close race" — no
equivalent in the new three-state `RcServerState` lifecycle, which has no
"close" state) is replaced outright with a register-map write bypassing
lifecycle-state or root-client gating (`ep0::check_ep0_access_for_stream`/
`is_root_client` composed with `lifecycle::is_register_reachable`/
`is_register_writable`).

`SAFETY_PLAN.md` §4.3's integration-test coverage target no longer names
the retired `Controller` trait; it now names the live
`mock::RcServer::handle_ntscf_frame` decode -> route -> dispatch -> encode
path.

`tara.json`'s scope explicitly excludes the legacy `Zone`/`Command`/
`Controller`/`Registry` API (retained pending Milestone 10's CLI cutover)
from fresh threat modeling, since it has no compatibility shim and will be
deleted outright. Asset A-001 is retargeted from `wire::validate_header()`
(deleted by Milestone 9's `wire` REPLACE cutover) onto
`avtp::decode_ntscf_frame`/`acf::decode_acf_abb`; A-003 is retargeted onto
`mock::RcServer`'s endpoint-addressed dispatch path. Two new assets (A-007
EP0 register-map access-control integrity, A-008 discovery-stream claim
integrity) with new threat scenarios T-RCP-09/T-RCP-10 and cybersecurity
goals CSG-RCP-06/CSG-RCP-07 cover TC18-native attack surface the replaced
protocol never had. T-RCP-01 is retargeted onto AVTPDU/ACF frame injection;
T-RCP-05 is retargeted fully onto `authz::AuthzEndpoint`, dropping its
now-out-of-scope framing around the still-present legacy `Controller`
surface.

`.fusa-reqs.json`/`.fusa-dfmea.json`/`.fusa-iec62443.json`/
`.fusa-problems.json` needed no changes: every `REQ-*` group the rebased
HARA/TARA cite was already retargeted onto TC18 behavior by its own
satellite package's Milestone 1-9 item.

`cargo build --all-targets`, `cargo clippy --all-targets --all-features --
-D warnings`, `cargo test --all-targets` (1062 lib tests + 19 `src/bin/
rcp.rs` tests, unchanged), and `cargo fmt --all -- --check` are clean;
`bash scripts/fusa-gap-check.sh` reports 622/622 (100%) requirements
traced; `bash scripts/cyber-gap-check.sh` reports 6/6 threats with tested
countermeasures; `relay conform --strict` against the release binary passes
all three RELAY §12 checks.

### Changed — CLI command surface

`src/bin/rcp.rs`'s command surface is rebuilt against `mock::RcServer`'s
`(avtp::StreamId, byte_bus_id)`-addressed model, the same backing type
`src/adapt.rs`'s own Milestone 10 rebuild targets. `zones`/`send`/
`status --zone` and every `Zone`/`Command`/`Controller`/`Registry`/
`mock::MockRegistry` reference are gone from the file.
`version`/`capabilities`/`status`/`convert` are unchanged in shape (none of
them ever referenced `Zone`), with `capabilities`'s `commands`/`interfaces`
JSON fields updated to
`["version","capabilities","status","convert","discover","register","endpoint"]`
/ `["RcServer","Endpoint"]`.

Three new subcommands replace the retired trio:

- `discover [--transaction <n>] [--format json]` builds a discovery
  request (`discovery::build_discovery_request`) and answers it via
  `discovery::build_discovery_response`, printing the decoded
  `GeneralRegisters` snapshot.
- `register read [--stream <hex>] [--format json]` / `register write
  --payload <hex> [--stream <hex>] [--root]` dispatch an EP0-addressed
  read/write `AcfAbbMessage` through `RcServer::handle_abb`. `--root`
  first designates `--stream` the server's root client. A write is
  reported exactly as `RcServer::handle_abb` answers it, including
  `RcpError::LockedMemAccess` for the root client itself — see that
  function's own doc comment for why a `General`-category write is never
  currently accepted by this in-process server.
- `endpoint read --bus-id <n> [...]` / `endpoint write --bus-id <n>
  --payload <hex> [...]` register a fresh `mock::MockEndpoint` of
  `--ep-type` (default `regmap::EndpointType::Gpio`) holding `--initial`
  under `(--stream, --bus-id)`, then dispatch a read/write `AcfAbbMessage`
  through `RcServer::handle_abb`'s `DeviceEndpoint` route.

One flagged design choice (Guiding Principle 5): this crate has no
concrete `udp::UdpSocket` implementation over a real OS socket, so
`discover`/`register`/`endpoint` each construct and address a fresh
in-process `RcServer` for the lifetime of one invocation — the same
ephemeral-server discipline the retired `send`/`status --zone` already
used against a fresh `mock::MockRegistry` each invocation. `--stream` is
parsed/rendered as bare lowercase hex (no `0x` prefix), matching
`adapt::format_endpoint_id`'s own `StreamId` rendering.

`.fusa-reqs.json` `REQ-CLI-001`/`002`/`004`/`005` text is retargeted to
describe the new `discover`/shared flag/`endpoint`/shared dispatch
behavior in place of the retired `send`/`zones`/`status --zone` wording;
`REQ-CLI-008`'s text drops its stale `"(no --zone)"` parenthetical. No new
requirement IDs were needed. `src/bin/rcp.rs`'s own tests are rewritten
against `mock::RcServer`/`mock::MockEndpoint` in place of
`MockController`/`MockRegistry`/`Zone`/`Command` (27 tests, up from 19).

`cargo build --all-targets`, `cargo clippy --all-targets --all-features --
-D warnings`, `cargo test --all-targets` (1062 lib tests + 27 `src/bin/
rcp.rs` tests, up from 19), and `cargo fmt --all -- --check` are clean;
`bash scripts/fusa-gap-check.sh` reports 622/622 (100%) requirements
traced; `bash scripts/cyber-gap-check.sh` reports 6/6 threats with tested
countermeasures.

### Removed — legacy Zone/Command/Controller/Registry API

`src/lib.rs`'s pre-Milestone-10 `Zone`/`Priority`/`CommandType`/
`ResponseStatus`/`Command`/`Response`/`Status`/`Subscription`/`Controller`/
`LoaningController`/`Registry` types and `zone_from_str` — kept in place
through Milestone 9 only because the CLI (the item immediately above this
one) and `Adapt()` cutovers still depended on them — are deleted outright,
with no compatibility shim, now that neither does. `src/mock.rs`'s
parallel `MockController`/`MockRegistry`/`Handler` test double for that
API, and `src/base64_serde.rs`'s `opt` submodule (which existed only to
serve `Command`/`Response`/`Status`'s optional payload field), are deleted
with it. `RcpError`'s four sentinels this legacy API originated
(`NotFound`/`AlreadyExists`/`Busy`/`ZoneMismatch`) are kept — `capi`/
`authz`/`federation` and others construct and match on them for meanings
unrelated to the removed `Zone` type — and that enum's doc-comment section
is retitled from "Legacy Zone/Controller/Registry sentinels" to
"General-purpose sentinels" to say so. `src/lib.rs`'s crate-level doc
comment is rewritten to describe the TC18 core in place of the old
Zone/Command/Registry model it still described.

`.fusa-reqs.json` drops the 77 requirement entries (`REQ-ZONE-*`/
`REQ-PRI-001..003`/`REQ-CMD-001..006`/`REQ-CMDSTRUCT-*`/`REQ-STATUS-*`/
`REQ-CTRL-*`/`REQ-REG-*`/`REQ-RESP-*`/`REQ-STAT-001..005`/
`REQ-RELAY-010`/`REQ-RELAY-011`/`REQ-MSG-*`) that described only the
deleted API; `tara.json`'s scope note is updated to record that the legacy
surface it already anticipated deleting has in fact now been deleted.

### Added — public API stability guarantees

`docs/SEMVER.md` (`ROADMAP.md` Milestone 10, "Public API stability
guarantees") declares this crate's versioning scheme and a three-tier
stability classification of every `pub mod`. `RcpError` and
`regmap::EndpointType` gain `#[non_exhaustive]`, each a live growth
surface tied to a specification-defined value space with room left for
future codes; `avtp::HeaderVariant` and `lifecycle::RcServerState` are
surveyed and deliberately left exhaustive, since both mirror small,
spec-fixed, closed sets.

A new `api-stability` CI job runs `scripts/api-snapshot-check.sh`, which
diffs `cargo public-api --simplified`'s current output against the
committed `docs/PUBLIC_API.txt` snapshot and fails the build on drift.
`README.md`'s Quick Start example (uncompilable since Milestone 9's `mock`
REPLACE — it still showed the removed `MockController`/`Command`/
`Controller`/`Zone` API) and Module Index are rewritten against the
current module set; `CONTRIBUTING.md`'s Versioning section points at
`docs/SEMVER.md` and the `docs/PUBLIC_API.txt` regeneration step.

`cargo build --all-targets`, `cargo clippy --all-targets --all-features --
-D warnings`, `cargo test --all-targets` (978 lib tests, down from 1062 —
84 tests for the deleted legacy API removed, no others changed; 27
unchanged `src/bin/rcp.rs` tests), and `cargo fmt --all -- --check` are
clean; `bash scripts/fusa-gap-check.sh` reports 545/545 (100%)
requirements traced; `bash scripts/cyber-gap-check.sh` reports 6/6 threats
with tested countermeasures; `bash scripts/api-snapshot-check.sh` reports
the public API surface matches `docs/PUBLIC_API.txt`.

### Added — conformance test vectors (Milestone 10 closed)

`src/conformance.rs` (`ROADMAP.md` Milestone 10's last checklist item,
"Conformance test vectors / interop verification") is test-only
(`#[cfg(test)] mod conformance;` in `lib.rs`, not `pub` like every other
module here) and pins five self-referential wire-format golden vectors —
frozen literal byte arrays, not recomputed from the encoder under test — for
an NTSCF header, a TSCF header with a non-degenerate `avtp_timestamp`, an
ACF_ABB message, an ACF_GBB message with a non-zero `message_timestamp`,
and a composed NTSCF+ACF_ABB frame.

`go-RCP`, the one sibling x-RCP implementation that has also uplifted to a
real TC18 core and shipped `v1.0.0`, was cross-checked directly: a
standalone Go program (not committed here) called go-RCP's own
`avtp.EncodeHeader`/`acf.EncodeMessage` (commit
`bdc760fb057f067cfb68199b6c3d0edab9e0c671`) for field values logically
analogous to each golden vector. The two implementations' wire bytes are
**not byte-identical** at any of the four vector types — divergent header/
message lengths, `data_length` field packing, a go-RCP timestamp-status
marker this crate's `TscfHeader` has no equivalent for, differing
message-kind discriminants, and differing `byte_bus_id` width — recorded in
`conformance.rs`'s module doc comment and pinned by a dedicated test rather
than silently resolved, per Guiding Principle 5; reconciling either
implementation's byte-level choices against the other, or against the OPEN
Alliance TC18 spec's own behavior directly, is left for a follow-up. Both
implementations' `stream_id` sender-MAC/suffix split agrees byte-for-byte,
which is also recorded.

`.fusa-reqs.json` gains `REQ-CONF-001..006`. `cargo build --all-targets`,
`cargo clippy --all-targets --all-features -- -D warnings`, `cargo test
--all-targets` (986 lib tests, up from 978; 27 unchanged
`src/bin/rcp.rs` tests), and `cargo fmt --all -- --check` are clean; `bash
scripts/fusa-gap-check.sh` reports 551/551 (100%) requirements traced;
`bash scripts/cyber-gap-check.sh` reports 6/6 threats with tested
countermeasures.

This was Milestone 10's last unchecked checklist item. `ROADMAP.md`
Milestone 10 is now complete.

### Released — v1.0.0

`Cargo.toml`'s `version` moves from `0.3.0` to `1.0.0` in this
follow-up commit, per the version-freeze policy stated above and in
`docs/SEMVER.md`: this is the first version number assigned after the
OPEN Alliance TC18 core replacement (`ROADMAP.md` Milestones 1-10)
reaches a coherent, checked-out point. This is a deliberately separate
commit from the conformance-test-vectors item itself (kept out of PR
#85), tagged as `v1.0.0` immediately after. `crates.io` publication via
the tag-triggered `Release` workflow remains blocked on the still-open
`CARGO_REGISTRY_TOKEN` secret gap tracked by issue #12 — the GitHub
Release and tag are real; the `cargo publish` step is expected to fail
until a maintainer provisions that secret, same as it did on the prior
`v0.2.0`/`v0.3.0` tags.

## v0.12.0-dev (Milestone 9) — closed

### Removed

All 11 **DEPRECATE**-disposition satellite packages named in `ROADMAP.md`'s
Satellite Package Disposition table are removed outright: `prioqueue`,
`zonegroup`, `tsn`, `firmware`, `someip`, `mqttbr`, `ddsbr`, `grpcbridge`,
`restbridge`, `udsbr`, `doipbr`. None had a live caller anywhere else in
`src/` (confirmed by inspection before removal), so no other module needed
a corresponding code change.

### Migration path

- **`someip`, `mqttbr`, `ddsbr`, `grpcbridge`, `restbridge`, `udsbr`,
  `doipbr`** (the six protocol-bridge packages): none of these is a
  spec-defined OPEN Alliance TC18 RCP transport. Cross-protocol bridging is
  no longer handled per-repo — integrate via RELAY's `crossbar` router
  (landed in RELAY `v1.8`, RELAY PR #45) instead of an in-crate protocol
  bridge. This follows the same ecosystem precedent go-DDS set when it
  removed its own MQTT and domain bridges in `v0.52.0`.
- **`prioqueue`**: its Critical/High/Normal dispatch-ordering decorator is
  superseded by this crate's own native execution-priority scheduler,
  built additively in Milestone 5 —
  [`request::execution_priority_tier`]/[`request::select_next_pending_request`]
  (`REQ-PRIO-001`..`004`). That scheduler is not yet wired into a live
  decoder or dispatch loop, so integrators relying on `prioqueue` for
  Critical-priority responsiveness under load currently have no live
  in-crate replacement to compose against — see `tara.json` `T-RCP-04`/
  `CSG-RCP-03` and `.fusa-iec62443.json` `T-004` for the honestly-raised
  residual risk this leaves open.
- **`zonegroup`**: has no equivalent now that `Zone` disappears from the
  endpoint-addressed core. Multi-endpoint fan-out is not defined by the
  OPEN Alliance TC18 spec; it can be rebuilt later as a generic
  client-side helper if a real need emerges, but nothing in this crate
  does so today.
- **`tsn`**: its priority-byte-in-payload hack is incompatible with real
  IEEE 1722 AVTPDU framing. Legitimate TSN traffic-class handling (VLAN
  PCP tagging) belongs at the transport/socket layer, outside what the
  RCP spec itself defines, and this crate does not provide it.
- **`firmware`**: its chunked-SET/GET OTA sequencer has no home among the
  thirteen spec-defined endpoint types and is out of TC18 scope. It could
  return later as an OEM-layer concern built atop a real endpoint, but is
  not part of this crate's core today.

### Traceability

`.fusa-reqs.json` requirement IDs `REQ-PQ-001`..`008`, `REQ-ZG-001`..`007`,
`REQ-TSN-001`..`005`, `REQ-FW-001`..`006`, `REQ-SOMEIP-001`..`005`,
`REQ-MQTT-001`..`005`, `REQ-DDS-001`..`004`, `REQ-GRPC-001`..`004`,
`REQ-REST-001`..`004`, `REQ-UDS-001`..`005`, and `REQ-DOIP-001`..`004` are
retired (removed) rather than retargeted — the deleted packages' tested
behavior has no surviving in-crate analog, except `prioqueue`'s, which is
already fully covered by the pre-existing `REQ-PRIO-001`..`004`. Cross-
references in `.fusa-iec62443.json` (`T-004`, `SC-006`), `tara.json`
(`A-005`, `T-RCP-04`, `CSG-RCP-03`), `.fusa-dfmea.json` (`FM-004`,
`FM-009`), `HARA.md` (`SG-009`), and `SECURITY.md`'s security-controls
table are updated to describe the packages as actually removed, rather
than merely slated for removal, with residual risk raised honestly where
no live replacement mechanism exists.

### Verified

All 7 **KEEP-AS-IS**-disposition satellite packages named in `ROADMAP.md`'s
Satellite Package Disposition table — `dyndata`, `codegen`, `iso21434`,
`certgap`, `formal`, `relay`, `base64_serde` — completed a regression pass
confirming they are genuinely unaffected by the Milestone 9 core migration.
Each was checked individually for any coupling to the legacy `Zone`/
`Command`/`Response`/`Status`/`Controller`/`Registry` core (none found) and
for its actual cross-module callers crate-wide (also confirmed: `dyndata`,
`codegen`, `iso21434`, `certgap`, and `base64_serde`'s own path have none;
`formal` is only mentioned in `src/lifecycle.rs` doc comments, not
imported; `relay` is consumed by `src/adapt.rs` and `src/lib.rs`, both
pre-existing RELAY-spec bindings, not RCP-core ones). No source file was
modified — this entry records the verification itself. `cargo build
--all-targets`, `cargo clippy --all-targets -- -D warnings`, `cargo test
--all-targets` (1056 tests), and `cargo fmt --check` are clean; `bash
scripts/fusa-gap-check.sh` reports 621/621 (100%) requirements traced;
`bash scripts/cyber-gap-check.sh` reports 6/6 threats with tested
countermeasures. This closes Milestone 9 in full.
