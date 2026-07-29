# rust-RCP

[![CI](https://github.com/SoundMatt/rust-RCP/actions/workflows/ci.yml/badge.svg)](https://github.com/SoundMatt/rust-RCP/actions/workflows/ci.yml)
[![ASIL-B](https://img.shields.io/badge/ISO%2026262-ASIL--B-orange)](SAFETY_PLAN.md)
[![IEC 62443](https://img.shields.io/badge/IEC%2062443-SL--2-blue)](SECURITY.md)

Rust implementation of the **Remote Control Protocol (RCP)** for automotive zonal architecture, compliant with the **RELAY specification v1.11**.

RCP is used by a central HPC to dispatch `Command`s to zone controllers (front-left, front-right, rear-left, rear-right, central) and receive `Response`s and periodic `Status` telemetry.

## Features

- Full RELAY spec v1.11 compliance, including the `Adapt()` RELAY adapter (§10.3) — `rcp::adapt(ctrl)` wraps a `Controller` as a `relay::Caller`
- **ASIL-B** (ISO 26262:2018) with full FuSa artifact set, including a TARA (ISO/SAE 21434, see [tara.json](tara.json))
- **IEC 62443 SL-2** cybersecurity controls
- `#![forbid(unsafe_code)]` — 100% safe Rust
- Core `Controller`/`Registry` API is a plain blocking `fn` interface; the RELAY-facing `relay::Node`/`Caller` adapter is `async fn` per §18.3 and runs on `tokio`
- 44 modules covering core protocol, bridges, safety, and observability

## Quick Start

```rust
use rcp::{mock::MockController, Command, Controller, Zone};
use std::sync::Arc;

let ctrl: Arc<dyn Controller> = MockController::new(Zone::FRONT_LEFT, None);
let cmd = Command { zone: Zone::FRONT_LEFT, ..Default::default() };
let resp = ctrl.send(&cmd, None).unwrap();
assert_eq!(resp.zone, Zone::FRONT_LEFT);
```

## Architecture

```
┌────────────────────────────────────────────────┐
│                    HPC                         │
│  ┌─────────────────────────────────────────┐  │
│  │              Registry                   │  │
│  └──┬──────────┬────────────┬──────────────┘  │
│     │          │            │                  │
│  ┌──▼──┐  ┌───▼──┐  ┌──────▼──┐              │
│  │ FL  │  │  FR  │  │   ...   │  Controllers  │
│  └──┬──┘  └───┬──┘  └─────────┘              │
└─────┼──────────┼──────────────────────────────┘
      │  (wire)  │
┌─────▼──────────▼─────────────────────────────┐
│         Zone Controllers (ECUs)               │
└───────────────────────────────────────────────┘
```

## Module Index

| Module | Purpose |
|---|---|
| `mock` | In-process mock controller and registry for testing |
| `wire` | Binary wire-frame encoder/decoder (RELAY spec §10) |
| `e2e` | End-to-end protection: OPEN Alliance TC18 safe-point CRC-32 |
| `prioqueue` | Priority-queue controller (Critical > High > Normal) |
| `ratelimit` | Token-bucket rate limiter (`RateLimitEndpoint`, over `mock::Endpoint`) |
| `sim` | Deterministic simulation endpoint (`SimEndpoint`) |
| `watchdog` | Periodic WATCHDOG command dispatcher |
| `deadline` | Endpoint call-deadline enforcement (`DeadlineEndpoint`) |
| `powerstate` | SLEEP/WAKE power state machine |
| `faultinject` | Deterministic fault injection for testing (`FaultInjectEndpoint`) |
| `loan` | Zero-copy payload pool (`LoanPoolEndpoint`) |
| `zonegroup` | Broadcast commands to multiple zones in parallel |
| `proxy` | Hot-swappable proxy endpoint (`ProxyEndpoint`) |
| `redundancy` | 1-of-2 hot-standby failover endpoint (`RedundancyEndpoint`) |
| `observe` | Latency histogram and event hooks (`ObserveEndpoint`) |
| `tsn` | IEEE 802.1Qav traffic-class tagging |
| `authz` | (endpoint-type, request-type) ACL policy enforcement (`AuthzEndpoint`) |
| `firmware` | Chunked firmware update sequencer |
| `record` | Read/write call audit logger (`RecordEndpoint`) |
| `federation` | Multi-vehicle routing over each peer's own `DiscoveryCache` |
| `dyndata` | Runtime key/value parameter store |
| `config` | JSON/YAML configuration loader and validator |
| `codegen` | Rust struct code generator from JSON schema |
| `iso21434` | ISO 21434 TARA threat and risk types |
| `certgap` | Certification gap analysis |
| `formal` | Runtime-checkable formal invariants |
| `admin` | Discovered-peer health/staleness reporting and graceful shutdown (`AdminServer`, over `discovery::DiscoveryCache`) |
| `someip` | SOME/IP bridge |
| `mqttbr` | MQTT bridge |
| `ddsbr` | DDS / AUTOSAR Adaptive bridge |
| `udp` | UDP unicast transport (`UdpTransport`, client) + RC-Server-endpoint dispatch with discovery integration (`UdpRcServer`, server) |
| `shmem` | Shared-memory IPC bridge, `StreamId`-addressed (`ShmBridge`) |
| `mdns` | mDNS/DNS-SD pre-discovery rendezvous helper |
| `tlstransport` | TLS 1.2+ secured transport |
| `grpcbridge` | gRPC stub bridge |
| `restbridge` | REST/HTTP bridge |
| `udsbr` | UDS (ISO 14229) bridge |
| `doipbr` | DoIP (ISO 13400-2) bridge |
| `capi` | C FFI types and error codes |
| `adapt` | External message format adapter over `mock::Endpoint` (`AdaptEndpoint`), and the still-`Controller`-bound RELAY `Adapt()`/`to_message()`/`from_message()` entry point (§10.3, §15.7.5), pending its own Milestone 10 endpoint-addressed rebuild |
| `relay` | Vendored RELAY protocol types — `Message`, `Node`, `Caller`, error sentinels (§18.3) |
| `base64_serde` | Base64 serde helpers for `Message`/`Command`/`Response`/`Status` payload fields |

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
