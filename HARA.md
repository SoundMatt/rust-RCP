# Hazard Analysis and Risk Assessment — rust-RCP

ASIL classification follows ISO 26262-3 §6: ASIL = Severity × Exposure × Controllability.

Full machine-readable HARA is in `.fusa-hara.json`.

## Hazard Summary

| ID | Description | S | E | C | ASIL | Safety Goal |
|---|---|---|---|---|---|---|
| H-001 | Wrong zone routing | S2 | E4 | C2 | ASIL-B | SG-001 |
| H-002 | Lost critical command | S2 | E4 | C2 | ASIL-B | SG-002 |
| H-003 | Replayed command | S2 | E3 | C2 | ASIL-B | SG-003 |
| H-004 | Payload corruption | S3 | E4 | C1 | ASIL-B | SG-004 |
| H-005 | Controller lockup | S2 | E4 | C2 | ASIL-B | SG-005 |
| H-006 | Unauthorised command | S2 | E3 | C2 | ASIL-B | SG-006 |
| H-007 | Power state race | S1 | E3 | C3 | ASIL-A | SG-007 |
| H-008 | Payload length overflow | S3 | E4 | C0 | ASIL-B | SG-008 |
| H-009 | Command flooding DoS | S2 | E4 | C2 | ASIL-B | SG-009 |
| H-010 | Registry close race | S1 | E4 | C3 | ASIL-A | SG-010 |

## Safety Goals

| ID | Description | ASIL | FTTI (ms) | Implementation |
|---|---|---|---|---|
| SG-001 | Correct zone delivery | ASIL-B | 200 | `REQ-ZONE-*`, zone mismatch check |
| SG-002 | No silent command drop | ASIL-B | 100 | `REQ-CTRL-*`, error propagation |
| SG-003 | Replay detection | ASIL-B | 500 | `REQ-E2E-005`, `ReplayGuard` |
| SG-004 | Payload integrity | ASIL-B | 200 | `REQ-E2E-002`, CRC-16/CCITT-FALSE |
| SG-005 | Watchdog monitoring | ASIL-B | 3000 | `REQ-WDG-*`, `WatchdogMonitor` |
| SG-006 | Auth enforcement | ASIL-B | 0 | `REQ-AUTHZ-*`, `AuthzController` |
| SG-007 | Atomic power transitions | ASIL-A | 500 | `REQ-PWR-*`, `PowerStateController` |
| SG-008 | Payload size validation | ASIL-B | 0 | `REQ-WIRE-007`, MAX_PAYLOAD check |
| SG-009 | Critical preemption | ASIL-B | 100 | `REQ-PQ-004`, `PrioController` |
| SG-010 | Defined close behaviour | ASIL-A | 200 | `REQ-ERR-007`, `Closed` sentinel |
