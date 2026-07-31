# Security Policy — rust-RCP

## Supported Versions

| Version | Supported |
|---|---|
| 3.x | ✅ Yes |
| 2.x | ❌ No (see `CHANGELOG.md` — TC18-conformant ACF wire format, a breaking change) |
| 1.x | ❌ No (see `CHANGELOG.md` — `RcpError::ZoneMismatch` removed, a breaking change) |
| 0.1.x | ❌ No |

## Reporting a Vulnerability

**Do not open a public GitHub issue for security vulnerabilities.**

Email: **security@soundmatt.dev**

Include:
- Description of the vulnerability
- Steps to reproduce
- Affected versions
- Suggested fix (if available)

We will acknowledge receipt within **48 hours** and provide a status update within **5 business days**.

## Security Design

rust-RCP targets **IEC 62443 SL-2** (see `.fusa-iec62443.json`).

Key security controls:

| Control | Module | Requirement |
|---|---|---|
| Mutual TLS | `tlstransport` | REQ-TLS-002 |
| CRC-32 (TC18 safe-point) payload integrity | `e2e` | REQ-CRC-004 |
| Monotonic sequence check (not yet dispatch-wired) | `request` | REQ-SEQENF-003 |
| Endpoint ACL ((ep_type, request-type) allowlist) | `authz` | REQ-AUTHZ-005 |
| Rate limiting | `ratelimit` | REQ-RL-006 |
| Payload size cap (65491 B) | `wire` | REQ-WIRE-007 |
| Execution-priority ordering (not yet dispatch-wired; `prioqueue` removed by Milestone 9's DEPRECATE disposition — `ratelimit` no longer exempts Critical priority either, see `.fusa-iec62443.json` T-004) | `request` | REQ-PRIO-004 |

## Memory Safety

The crate uses `#![forbid(unsafe_code)]`. All memory handling is provided by the Rust type system and checked at compile time. No raw pointers, no `unsafe` blocks.

## Dependency Policy

Dependencies are minimised and pinned in `Cargo.lock`. All transitive dependencies are reviewed for security advisories via `cargo audit` in CI.
