# rust-RCP

[![CI](https://github.com/SoundMatt/rust-RCP/actions/workflows/ci.yml/badge.svg)](https://github.com/SoundMatt/rust-RCP/actions/workflows/ci.yml)
[![ASIL-B](https://img.shields.io/badge/ISO%2026262-ASIL--B-orange)](SAFETY_PLAN.md)
[![IEC 62443](https://img.shields.io/badge/IEC%2062443-SL--2-blue)](SECURITY.md)

Rust implementation of the **OPEN Alliance TC18 Remote Control Protocol Specification v0.5.1_RC** for automotive zonal architecture, adapted to the **RELAY specification v2.0**.

An RC Client addresses an RC Server's device endpoints by `(stream_id, byte_bus_id)` and exchanges AVTPDU/ACF-framed requests over the RC Server's lifecycle/register-map model — reading and writing endpoints such as GPIO, SPI, CAN, PWM, and others, and reading/writing the RC Server's own register map through EP0.

## Features

- Full RELAY spec v2.0 compliance, including the `Adapt()` RELAY adapter (§10.3) — `rcp::adapt(server)` wraps an [`mock::RcServer`] as a `relay::Caller`
- **ASIL-B** (ISO 26262:2018) with full FuSa artifact set, including a TARA (ISO/SAE 21434, see [tara.json](tara.json))
- **IEC 62443 SL-2** cybersecurity controls
- `#![forbid(unsafe_code)]` — 100% safe Rust
- The core RC Server dispatch path ([`mock::RcServer::handle_abb`]/[`handle_ntscf_frame`](mock::RcServer::handle_ntscf_frame)) is a plain blocking `fn` interface; the RELAY-facing `relay::Node`/`Caller` adapter is `async fn` per §18.3 and runs on `tokio`
- 52 modules (51 public + 1 crate-internal) covering core protocol, device endpoints, transport bridges, safety, and observability — see the Module Index below; the crate additionally has one `#[cfg(test)]`-only module (`conformance`) deliberately excluded from this count since it carries no public API surface
- Public API stability guarantees as of `v1.0.0` — see [docs/SEMVER.md](docs/SEMVER.md)

## Quick Start

```rust
use rcp::acf::{AcfAbbMessage, ByteMessageInfo, ReadSizeOrSegment};
use rcp::avtp::StreamId;
use rcp::mock::{MockEndpoint, RcServer};
use rcp::regmap::{EndpointType, GeneralRegisters};

let stream = StreamId::from_u64(1);
let server = RcServer::new(GeneralRegisters::default());
let endpoint = MockEndpoint::new(EndpointType::Gpio, vec![0xAA]);
server.register_endpoint(stream, /* byte_bus_id */ 1, endpoint).unwrap();

// Read one byte back from the endpoint through an ACF_ABB request.
let request = AcfAbbMessage {
    info: ByteMessageInfo {
        byte_bus_id: 1,
        op: false, // false = read
        read_size_segment: ReadSizeOrSegment(1),
        ..ByteMessageInfo::default()
    },
    payload: Vec::new(),
};
let response = server.handle_abb(stream, &request).unwrap();
assert_eq!(response.payload, vec![0xAA]);
```

## Architecture

```
┌──────────────────────────────────────────────────────────┐
│                      RC Client (HPC)                      │
└───────────────────────────┬────────────────────────────────┘
                             │  AVTPDU (NTSCF/TSCF) / ACF_ABB
                             │  addressed by (stream_id, byte_bus_id)
┌───────────────────────────▼────────────────────────────────┐
│                          RC Server                          │
│  ┌─────────────┐   ┌────────────────┐   ┌────────────────┐ │
│  │  Lifecycle  │   │  EP0 register  │   │  EndpointTable  │ │
│  │   state     │   │      map       │   │  (device eps)   │ │
│  └─────────────┘   └────────────────┘   └───────┬────────┘ │
└───────────────────────────────────────────────────┼─────────┘
                                                      │
                                    ┌─────────────────┼─────────────────┐
                                    │                 │                 │
                                ┌───▼───┐         ┌───▼───┐         ┌───▼───┐
                                │ GPIO  │         │  CAN  │         │  ...  │  device endpoints
                                └───────┘         └───────┘         └───────┘
```

`mock::RcServer` is this crate's in-process reference implementation of the RC Server side; `udp`/`tlstransport`/`shmem` compose it over real transports.

## Module Index

See [docs/SEMVER.md](docs/SEMVER.md) for which modules carry a semver stability guarantee (Tier 1/2) versus none at all (Tier 3).

| Module | Purpose |
|---|---|
| `avtp` | IEEE 1722 AVTPDU framing — TC18 wire format core |
| `acf` | ACF (AVTP Control Format) messages — TC18 wire format core |
| `addressing` | `(stream_id, byte_bus_id)` → endpoint lookup — TC18 wire format core |
| `timestamp` | AVTP presentation-timestamp semantics — TC18 wire format core |
| `lifecycle` | RC Server lifecycle state machine — TC18 register-map model |
| `regmap` | Three-layer per-endpoint config taxonomy, the RC Server's general register map |
| `ep0` | EP0 (RC-Server-as-endpoint) whole-register-map read/write addressing |
| `discovery` | Discovery request/response, discovery-stream claiming, multi-client arbitration — **broadcast addressing and the on-wire register-address encoding are this crate's own unreconciled working interpretations, not confirmed spec conventions; see the module's own "Provenance note" doc comment before relying on either for interop** |
| `request` | Conditional-request taxonomy: compound / compound-wait, chained requests |
| `e2e` | End-to-end protection: the OPEN Alliance TC18 safe-point CRC-32 |
| `fragment` | Multi-AVTPDU fragmentation reassembly |
| `mock` | In-process OPEN Alliance TC18 RC Server test double — [`mock::RcServer`], [`mock::Endpoint`], [`mock::MockEndpoint`] |
| `adapt` | RELAY specification's `Adapt()`/`to_message()`/`from_message()` binding over `mock::RcServer` |
| `relay` | Vendored RELAY protocol types — `Message`, `Node`, `Caller`, error sentinels (§18.3) |
| `wakeup` | The Wakeup control endpoint type (`ep_type 0x01`) |
| `gpio` | The GPIO endpoint type (`ep_type 0x02`) |
| `spi` | The SPI endpoint type (`ep_type 0x03`) |
| `i2c` | The I²C endpoint type (`ep_type 0x04`) |
| `uart` | The UART endpoint type (`ep_type 0x05`) |
| `lin` | The LIN commander endpoint type (`ep_type 0x06`) |
| `pwm` | The PWM_OUT / PWM_IN endpoint types (`ep_type 0x07`/`0x08`) |
| `adc` | The ADC endpoint type (`ep_type 0x09`) |
| `can` | The CAN controller endpoint type (`ep_type 0x0B`) |
| `iseled` | The ISELED endpoint type (`ep_type 0x0C`). Its native per-frame CRC (`iseled_frame_crc8`/`IseledFrameCrc`) is an unconfirmed stand-in algorithm, gated behind the opt-in `iseled-unconfirmed-crc` Cargo feature and excluded from the default build — see the module's own doc comment |
| `mdio` | The MDIO endpoint type (`ep_type 0x0D`) |
| `evtgroup` | The "Groups A/B/C" `evt[2:0]` sub-opcode convention |
| `ratelimit` | Token-bucket rate limiter endpoint decorator (`RateLimitEndpoint`, over `mock::Endpoint`) |
| `deadline` | Endpoint call-deadline enforcement (`DeadlineEndpoint`) |
| `powerstate` | SLEEP/WAKE power-mode model |
| `watchdog` | Per-stream watchdog liveness model |
| `faultinject` | Deterministic fault injection for testing (`FaultInjectEndpoint`) |
| `loan` | Zero-copy payload pool (`LoanPoolEndpoint`) |
| `proxy` | Hot-swappable proxy endpoint (`ProxyEndpoint`) |
| `redundancy` | 1-of-2 hot-standby failover endpoint (`RedundancyEndpoint`) |
| `observe` | Latency histogram and event hooks (`ObserveEndpoint`) |
| `authz` | (endpoint-type, request-type) ACL policy enforcement (`AuthzEndpoint`) |
| `record` | Read/write call audit logger (`RecordEndpoint`) |
| `federation` | Multi-vehicle routing over each peer's own `DiscoveryCache` |
| `dyndata` | Runtime key/value parameter store |
| `config` | JSON/YAML configuration loader and validator for the RC Server register-map |
| `admin` | Discovered-peer health/staleness reporting and graceful shutdown (`AdminServer`, over `discovery::DiscoveryCache`) |
| `udp` | UDP unicast transport (`UdpTransport`, client) + RC-Server-endpoint dispatch with discovery integration (`UdpRcServer`, server) |
| `shmem` | Shared-memory IPC bridge, `StreamId`-addressed (`ShmBridge`) |
| `mdns` | mDNS/DNS-SD pre-discovery rendezvous helper |
| `tlstransport` | TLS 1.2+ secured transport |
| `capi` | C FFI types and error codes (no `extern "C"` cdylib target yet) |
| `sim` | Deterministic simulation endpoint for integration and hardware-in-loop testing |
| `codegen` | Rust struct code generator from JSON schema |
| `iso21434` | ISO 21434 TARA threat and risk types |
| `certgap` | Certification gap analysis |
| `formal` | Runtime-checkable formal invariants |
| `base64_serde` | (crate-internal) Base64 serde helper for `relay::Message`'s payload field |

## API Stability

- [docs/SEMVER.md](docs/SEMVER.md) — public API stability tiers, `#[non_exhaustive]` policy, and versioning scheme
- [docs/PUBLIC_API.txt](docs/PUBLIC_API.txt) — committed `cargo public-api` snapshot, enforced in CI

## Safety & Security

- [SAFETY_PLAN.md](SAFETY_PLAN.md) — ISO 26262 safety plan
- [HARA.md](HARA.md) — Hazard analysis
- [tara.json](tara.json) — Threat analysis and risk assessment (ISO/SAE 21434)
- [SECURITY.md](SECURITY.md) — Security policy and controls
- [INCIDENT-RESPONSE.md](INCIDENT-RESPONSE.md) — Incident response plan
- [.fusa.json](.fusa.json) — FuSa project manifest
- [.fusa-reqs.json](.fusa-reqs.json) — Requirements database
- [.fusa-hara.json](.fusa-hara.json) — HARA machine-readable
- [.fusa-iec62443.json](.fusa-iec62443.json) — IEC 62443 controls

## Requirements Coverage

Run `rsfusa check` (or `cargo xtask fusa`) to verify all requirements are traced and tested.

## License

Mozilla Public License 2.0 — see [LICENSE](LICENSE).
