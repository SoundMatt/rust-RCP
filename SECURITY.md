# Security Policy — rust-RCP

## Supported Versions

| Version | Supported |
|---|---|
| 0.1.x | ✅ Yes |

## Reporting a Vulnerability

**Do not open a public GitHub issue for security vulnerabilities.**

Email: **security@example.com**

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
| CRC-16 payload integrity | `e2e` | REQ-E2E-002 |
| Anti-replay window | `e2e` | REQ-E2E-005 |
| Command ACL | `authz` | REQ-AUTHZ-005 |
| Rate limiting | `ratelimit` | REQ-RL-006 |
| Payload size cap (65491 B) | `wire` | REQ-WIRE-007 |
| Priority preemption (Critical exempt) | `ratelimit`, `prioqueue` | REQ-RL-007, REQ-PQ-004 |

## Memory Safety

The crate uses `#![forbid(unsafe_code)]`. All memory handling is provided by the Rust type system and checked at compile time. No raw pointers, no `unsafe` blocks.

## Dependency Policy

Dependencies are minimised and pinned in `Cargo.lock`. All transitive dependencies are reviewed for security advisories via `cargo audit` in CI.
