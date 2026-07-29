# Public API stability guarantees

`ROADMAP.md` Milestone 10, "Public API stability guarantees (semver) for
the new core". This document is the source of truth for which parts of
this crate's public surface carry a semver stability commitment, and how
that commitment is enforced.

## Versioning scheme

This crate follows [Semantic Versioning 2.0.0](https://semver.org/), with
one repo-specific rule recorded here rather than left implicit: per
`CHANGELOG.md`'s own header note, `Cargo.toml`'s `version` field does not
move while the OPEN Alliance TC18 Remote Control Protocol Specification
v0.5.1_RC uplift (`ROADMAP.md` Milestones 1-10) is in progress — every
milestone's work lands under `CHANGELOG.md`'s `## Unreleased` heading
against the crate's last real release, `v0.3.0`. The version jumps directly
from `0.3.0` to `1.0.0` once Milestone 10's own two remaining checklist
items (this one, and conformance test vectors / interop verification) are
both done and the milestone's Success Criteria are met. This is a
deliberate choice, not an oversight: bumping `0.x` versions incrementally
for each milestone's intentionally breaking rewrite would claim a
stability signal ("this is now a coherent, checked-out point") that
Milestones 1-9's work did not yet provide, since earlier milestones each
built additive, standalone plumbing not yet wired into a live decode ->
route -> dispatch -> encode path (see e.g. `src/mock.rs`'s own doc comment
history). `1.0.0` is the first version number this crate assigns after
that path exists and this stability policy is enforced by CI (see
"Enforcement" below).

After `v1.0.0`:

- **MAJOR**: any change that breaks a Tier 1 or Tier 2 stability guarantee
  below (removing/renaming a public item, adding a required field to a
  `pub` struct that isn't `#[non_exhaustive]`, narrowing an accepted input
  range, etc.).
- **MINOR**: backward-compatible additions — a new `pub` item, a new
  variant on a `#[non_exhaustive]` enum, a new optional field on a
  `#[non_exhaustive]` struct.
- **PATCH**: backward-compatible fixes with no public API surface change.

Tier 3 modules (below) are explicitly exempted from this: a Tier 3 change
is versioned as a MINOR bump even when it removes or restructures items,
since Tier 3 carries no stability guarantee in the first place. This must
still be called out in `CHANGELOG.md` so it isn't mistaken for a PATCH.

## Stability tiers

Every `pub mod` `lib.rs` declares falls into exactly one tier. A module not
listed by name below defaults to Tier 2.

### Tier 1 — core TC18 protocol surface (full semver guarantee from v1.0.0)

The wire format, RC Server model, and endpoint-addressing/dispatch path
`ROADMAP.md`'s Milestone 10 Success Criteria names directly:

- `avtp`, `acf` — AVTPDU/ACF frame encode/decode
- `addressing` — `(StreamId, byte_bus_id)` endpoint-table addressing
- `timestamp` — AVTP presentation-timestamp handling
- `lifecycle`, `regmap`, `ep0` — RC Server lifecycle/register-map model and
  EP0 root-client access control
- `discovery` — discovery-stream request/response and claim handling
- `request` — chained-request/execution-priority/watchdog-purge dispatch
  primitives
- `e2e`, `fragment` — end-to-end safe-point CRC-32 integrity and AVTPDU
  fragmentation
- `mock` — specifically [`mock::RcServer`], the [`mock::Endpoint`] trait,
  and [`mock::MockEndpoint`], this crate's reference dispatch
  implementation and the trait every other endpoint decorator composes
  against. `mock`'s own internal buffering details for `MockEndpoint`
  (e.g. its whole-buffer-replace `write` semantics) are not themselves
  spec-mandated and are documented there as this test double's own
  simplification — Tier 1 covers the trait shape and `RcServer`'s
  dispatch/lifecycle behavior, not that one simplification.
- `adapt` — the RELAY specification's `Adapt()`/`to_message()`/
  `from_message()` binding
- `relay` — the vendored RELAY protocol types (`Message`, `Node`,
  `Caller`, error sentinels) this crate's `adapt` binds against
- The `rust-rcp` CLI's documented command surface (`src/bin/rcp.rs`'s own
  module doc comment `Usage:` block) — not a library API, but held to the
  same stability bar since external tooling scripts against it.
- The crate root: [`RcpError`], [`SPEC_VERSION`], [`RELAY_SPEC_VERSION`].

### Tier 2 — composable endpoint decorators and transport bridges (semver-tracked, individually less settled pre-`v1.0.0`)

Everything else that is `pub`, currently including the device-endpoint
wire-codec modules (`can`, `lin`, `gpio`, `i2c`, `spi`, `uart`, `adc`,
`pwm`, `iseled`, `mdio`, `wakeup`), the `mock::Endpoint`-wrapping decorators
(`ratelimit`, `deadline`, `faultinject`, `proxy`, `redundancy`, `observe`,
`authz`, `record`, `loan`, `admin`, `powerstate`, `watchdog`, `evtgroup`,
`federation`, `dyndata`, `config`), and the transport bridges (`udp`,
`tlstransport`, `shmem`, `mdns`). These get the same semver *mechanics* as
Tier 1 (a breaking change here is still a MAJOR bump post-`v1.0.0`) but are
individually newer and less exercised end-to-end than Tier 1, so expect
more of them to gain `#[non_exhaustive]`/other stability annotations as
each is exercised against a real integration.

### Tier 3 — explicitly unstable, no compatibility guarantee

Tooling and analysis modules that are not part of the wire protocol or its
endpoint model at all: `sim` (deterministic simulation test double),
`codegen` (Rust struct generator from JSON schema), `certgap`
(certification gap analysis), `formal` (runtime-checkable formal
invariants), `iso21434` (TARA threat/risk types), and `capi` (C FFI types
— explicitly documented as having no `extern "C"` cdylib target yet, per
that module's own doc comment). These may change shape, gain required
parameters, or be removed in a MINOR release without notice beyond
`CHANGELOG.md`.

## `#[non_exhaustive]` policy

A `pub enum` or `pub struct` gets `#[non_exhaustive]` when this crate
expects to add variants/fields to it over time without that addition being
a breaking change — typically because it mirrors a specification-defined
value space with room left for future codes. Applied so far:

- [`RcpError`] — every completed milestone since Milestone 2 has added new
  variants to it (`ChainAborted`/`ChainError` in Milestone 5, `CrcError` in
  Milestone 6), and several named-but-unconstructed variants are already
  reserved for later call sites. See its own doc comment's "Stability"
  section.
- [`regmap::EndpointType`] — the `ep_type` byte is a specification-defined
  enumeration with codes above `0x0D` not yet assigned by any OPEN
  Alliance TC18 revision this crate's spec-extraction pass has read. See
  its own doc comment.

Enums that mirror a small, spec-fixed, closed set are deliberately **not**
marked `#[non_exhaustive]` — e.g. [`avtp::HeaderVariant`] (the AVTPDU
header-variant selection rule distinguishes exactly NTSCF vs. TSCF, a
binary decode outcome tied to one decode function, not an open
specification enumeration) and [`lifecycle::RcServerState`] (the RC Server
lifecycle is specified as exactly three states). If a later revision of
the specification changes either of those closed sets, adding a variant to
either enum is a MAJOR-version change, same as any other breaking API
change — that is the intended, honest consequence of leaving them
exhaustive rather than defensively marking every enum `#[non_exhaustive]`
regardless of whether it is actually expected to grow.

Downstream code matching on any `#[non_exhaustive]` enum from this crate
**MUST** include a wildcard arm (`_ => ...`); this compiles today and will
continue to compile across MINOR releases even as new variants are added.

## Enforcement

`.github/workflows/ci.yml`'s `api-stability` job runs
`scripts/api-snapshot-check.sh` (`cargo public-api --simplified`, diffed
against the committed `docs/PUBLIC_API.txt` snapshot). Any change to the
crate's public API surface — Tier 1, 2, or 3 alike, since the tool has no
way to know a module's tier — fails that job until `docs/PUBLIC_API.txt`
is regenerated and committed alongside the change. This forces every
public-surface change through a conscious "does this need a version-bump
decision under the scheme above" step rather than landing silently; it
does not by itself distinguish an intentional Tier 3 restructuring from an
accidental Tier 1 break — that judgment call is this document's job, made
by whoever reviews the regenerated snapshot in the diff.
