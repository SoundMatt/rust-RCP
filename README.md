# rust-RCP

[![CI](https://github.com/SoundMatt/rust-RCP/actions/workflows/ci.yml/badge.svg)](https://github.com/SoundMatt/rust-RCP/actions/workflows/ci.yml)
[![ASIL-B](https://img.shields.io/badge/ISO%2026262-ASIL--B-orange)](SAFETY_PLAN.md)
[![IEC 62443](https://img.shields.io/badge/IEC%2062443-SL--2-blue)](SECURITY.md)

Rust implementation of the **Remote Control Protocol (RCP)** for automotive zonal architecture, compliant with the **RELAY specification v1.6**.

RCP is used by a central HPC to dispatch `Command`s to zone controllers (front-left, front-right, rear-left, rear-right, central) and receive `Response`s and periodic `Status` telemetry.

## Features

- Full RELAY spec v1.10 compliance
- **ASIL-B** (ISO 26262:2018) with full FuSa artifact set
- **IEC 62443 SL-2** cybersecurity controls
- `#![forbid(unsafe_code)]` — 100% safe Rust
- Blocking synchronous API (`no_std`-compatible core, no tokio required)
- 42 modules covering core protocol, bridges, safety, and observability

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
| `e2e` | End-to-end protection: CRC-16/CCITT-FALSE + replay guard |
| `prioqueue` | Priority-queue controller (Critical > High > Normal) |
| `ratelimit` | Token-bucket rate limiter |
| `sim` | Deterministic simulation controller |
| `watchdog` | Periodic WATCHDOG command dispatcher |
| `deadline` | Hard response deadline enforcement |
| `powerstate` | SLEEP/WAKE power state machine |
| `faultinject` | Deterministic fault injection for testing |
| `loan` | Zero-copy payload pool (LoanPoolController) |
| `zonegroup` | Broadcast commands to multiple zones in parallel |
| `proxy` | Hot-swappable proxy controller |
| `redundancy` | 1-of-2 hot-standby failover controller |
| `observe` | Latency histogram and event hooks |
| `tsn` | IEEE 802.1Qav traffic-class tagging |
| `authz` | Command ACL policy enforcement |
| `firmware` | Chunked firmware update sequencer |
| `record` | Command/response audit logger |
| `federation` | Multi-vehicle registry router |
| `dyndata` | Runtime key/value parameter store |
| `config` | JSON/YAML configuration loader and validator |
| `codegen` | Rust struct code generator from JSON schema |
| `iso21434` | ISO 21434 TARA threat and risk types |
| `certgap` | Certification gap analysis |
| `formal` | Runtime-checkable formal invariants |
| `admin` | Health checks and graceful shutdown |
| `canbr` | CAN FD bridge |
| `linbr` | LIN 2.x bridge |
| `someip` | SOME/IP bridge |
| `mqttbr` | MQTT bridge |
| `ddsbr` | DDS / AUTOSAR Adaptive bridge |
| `udp` | UDP unicast transport |
| `shmem` | Shared-memory IPC bridge |
| `mdns` | mDNS/DNS-SD service discovery |
| `tlstransport` | TLS 1.2+ secured transport |
| `grpcbridge` | gRPC stub bridge |
| `restbridge` | REST/HTTP bridge |
| `udsbr` | UDS (ISO 14229) bridge |
| `doipbr` | DoIP (ISO 13400-2) bridge |
| `capi` | C FFI types and error codes |
| `adapt` | External message format adapter |

## Safety & Security

- [SAFETY_PLAN.md](SAFETY_PLAN.md) — ISO 26262 safety plan
- [HARA.md](HARA.md) — Hazard analysis
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
