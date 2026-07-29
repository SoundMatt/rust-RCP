# Changelog

All notable changes to rust-RCP are documented here. Entries are grouped by
the roadmap milestone that produced them (see `ROADMAP.md`), since this
crate's `Cargo.toml` version does not move until the OPEN Alliance TC18
core replacement reaches `v1.0.0`.

## Unreleased — v0.13.0-dev (Milestone 10)

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
Milestone 10 is now complete; the actual `Cargo.toml` version bump, tag,
and `crates.io` publish to `v1.0.0` are a deliberately separate release
step (`Cargo.toml` still reads `0.3.0` as of this entry), not folded into
this change.

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
