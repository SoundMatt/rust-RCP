# Hazard Analysis and Risk Assessment — rust-RCP

ASIL classification follows ISO 26262-3 §6: ASIL = Severity × Exposure × Controllability.

Full machine-readable HARA is in `.fusa-hara.json`.

This HARA is rebased against the OPEN Alliance TC18 Remote Control Protocol
core (AVTPDU/ACF wire framing, the RC Server lifecycle/register-map model,
EP0, discovery, the endpoint-type set, the conditional-request/sequencer
model, and the E2E CRC-32 safe-point mechanism) that Milestones 1-9 of
`ROADMAP.md` built. It supersedes the previous HARA, which described the
now-replaced private `Zone`/`Command`/`Controller`/`Registry` protocol; see
`ROADMAP.md`'s breaking-change notice for why that surface has no
compatibility shim and is out of scope here.

## Hazard Summary

| ID | Description | S | E | C | ASIL | Safety Goal |
|---|---|---|---|---|---|---|
| H-001 | Endpoint misaddressing — request resolves to the wrong `(stream_id, byte_bus_id)`-addressed endpoint | S2 | E4 | C2 | ASIL-B | SG-001 |
| H-002 | Safety-tagged request lost — a Critical-priority/safety-tagged request is silently dropped instead of executed or explicitly failed | S2 | E4 | C2 | ASIL-B | SG-002 |
| H-003 | Replayed request | S2 | E3 | C2 | ASIL-B | SG-003 |
| H-004 | Payload corruption | S3 | E4 | C1 | ASIL-B | SG-004 |
| H-005 | Endpoint/stream lockup undetected — per-stream watchdog fails to flag an unresponsive endpoint | S2 | E4 | C2 | ASIL-B | SG-005 |
| H-006 | Unauthorized request execution — a caller dispatches an `(endpoint-type, request-type)` pair outside its granted allowlist | S2 | E3 | C2 | ASIL-B | SG-006 |
| H-007 | Power-state transition race | S1 | E3 | C3 | ASIL-A | SG-007 |
| H-008 | Frame/payload length overflow | S3 | E4 | C0 | ASIL-B | SG-008 |
| H-009 | Request flooding DoS — a flood of Standard-priority requests starves Critical-priority/safety-tagged request dispatch | S2 | E4 | C2 | ASIL-B | SG-009 |
| H-010 | Register-map integrity bypass — a write reaches a register the RC Server's lifecycle state or root-client policy should have blocked | S1 | E4 | C3 | ASIL-A | SG-010 |

## Safety Goals

| ID | Description | ASIL | FTTI (ms) | Implementation |
|---|---|---|---|---|
| SG-001 | Requests shall only be delivered to the correct addressed endpoint | ASIL-B | 200 | `REQ-EPLK-*`, `addressing::EndpointTable::register`/`lookup` (per-stream `byte_bus_id` keyspace, per-pair uniqueness) |
| SG-002 | Safety-tagged requests shall not be silently dropped | ASIL-B | 100 | `REQ-SAFETY-001..005`, `request::check_watchdog_overflow_purge`/`purge_normal_priority_on_watchdog_overflow` (safety-tagged requests are exempt from watchdog-overflow purge) |
| SG-003 | Replay detection | ASIL-B | 500 | `REQ-SEQENF-003`, `evaluate_rx_enforce_seq` (not yet wired into a live request-acceptance path — see `tara.json` T-RCP-03) |
| SG-004 | Payload integrity | ASIL-B | 200 | `REQ-CRC-004`, TC18 safe-point CRC-32 (`crc32_tc18`) |
| SG-005 | Watchdog monitoring | ASIL-B | 3000 | `REQ-WDG-*`, per-stream `StreamWatchdogState`/`evaluate_stream_watchdog` |
| SG-006 | Auth enforcement | ASIL-B | 0 | `REQ-AUTHZ-*`, `authz::Policy`/`AuthzEndpoint` (`(ep_type, is_write)`-keyed allowlist) |
| SG-007 | Atomic power transitions | ASIL-A | 500 | `REQ-PWR-*`/`REQ-PWRSTART-*`/`REQ-WAKE-*`, `try_enter_power_mode`/`try_cold_start`/`try_hot_start` gated by `PowerModeGateInput`, driven at the Wakeup endpoint by `wakeup::request_sleep_via_sleep_cmd`/`wakeup::wake_source_signals_trigger_handshake` |
| SG-008 | Frame/payload size validation | ASIL-B | 0 | `REQ-WIRE-*`, NTSCF `ntscf_data_length` field-width check in `avtp::encode_ntscf_frame` |
| SG-009 | Critical-priority preemption | ASIL-B | 100 | `REQ-PRIO-004`, `execution_priority_tier`/`select_next_pending_request` (not yet wired into a live dispatch loop — `prioqueue`/`PrioController` removed by Milestone 9's DEPRECATE disposition, see `tara.json` T-RCP-04) |
| SG-010 | Register-map access control | ASIL-A | 200 | `REQ-EP0-*`/`REQ-LIFE-*`, `ep0::check_ep0_access_for_stream`/`is_root_client` composed with `lifecycle::is_register_reachable`/`is_register_writable` |

## Rationale for Retired Hazards

The previous HARA's H-001 ("Wrong zone routing"), H-005 ("Controller
lockup"), and H-010 ("Registry close race") described the `Zone`/
`Controller`/`Registry` model this crate is replacing; H-001 and H-005 have
been retargeted onto their nearest TC18 equivalents (endpoint addressing,
per-stream watchdog) above rather than dropped, since the underlying safety
concern — misdelivery, undetected unresponsiveness — persists in the new
model under a different mechanism. H-010 had no direct TC18 equivalent
(the new RC Server lifecycle has no "close" state — see `lifecycle.rs`'s
three-state `RcServerState` model) and has been replaced outright with a
hazard the new register-map/EP0 model actually introduces: a write bypassing
lifecycle-state or root-client gating.
