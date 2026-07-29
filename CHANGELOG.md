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
