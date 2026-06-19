# Safety Plan — rust-RCP

## 1. Scope

This safety plan covers the **rust-RCP** crate — a Rust implementation of the Remote Control Protocol (RCP) for automotive zonal architectures. The crate targets **ASIL-B** per ISO 26262:2018 and **IEC 62443 SL-2** for cybersecurity.

## 2. Safety Manager

| Role | Contact |
|---|---|
| Safety Manager | safety@example.com |
| Security Contact | security@example.com |
| CI Owner | devops@example.com |

## 3. Applicable Standards

| Standard | Version | Target Level |
|---|---|---|
| ISO 26262 | 2018 | ASIL-B |
| IEC 62443 | 2019 | SL-2 |
| RELAY Spec | 1.10 | Full compliance |

## 4. Safety Lifecycle Activities

### 4.1 Hazard Analysis and Risk Assessment (HARA)

See `HARA.md` and `.fusa-hara.json` for the full HARA. Ten hazards (H-001 to H-010) and ten safety goals (SG-001 to SG-010) are identified.

### 4.2 Requirements Tracing

All safety requirements are annotated with `// fusa:req REQ-XXX` in source files. Test cases are annotated with `// fusa:test REQ-XXX`. The `rsfusa` tool validates traceability in CI.

### 4.3 Verification Strategy

| Method | Coverage Target | Tool |
|---|---|---|
| Unit tests | ≥ 90% line coverage | `cargo test` + `cargo llvm-cov` |
| Integration tests | All controller trait methods | `cargo test` |
| Fuzz testing | Wire decoder, E2E unwrap | `cargo fuzz` |
| Static analysis | All warnings as errors | `cargo clippy -- -D warnings` |
| Formal model | State invariants | `formal.rs` runtime checks |
| Gap analysis | 0 untested requirements | `rsfusa check` |

### 4.4 Coding Guidelines

- `#![forbid(unsafe_code)]` — no unsafe code in the crate
- All public API documented with doc comments
- All error paths return typed `RcpError` variants
- No `unwrap()` or `expect()` in non-test code (only in tests and examples)
- No integer overflow — checked arithmetic or u64 promotion for length calculations
- No dynamic allocation in hot paths (buffer pool via `loan.rs`)

## 5. Configuration Management

Git is used for all source and artifact versioning. The `CHANGELOG.md` tracks all changes. Semantic versioning is used; breaking changes increment the major version.

## 6. Qualification of Software Tools

| Tool | Version | Qualification |
|---|---|---|
| Rust compiler (rustc) | ≥ 1.75 | ISO 26262-8 Part 11 tool class TC3 — qualification kit maintained by the Rust Foundation |
| cargo | bundled with rustc | TC1 |
| cargo-llvm-cov | latest | TC1 |
| rsfusa | latest | Internal tool, design specification in `.fusa.json` |

## 7. Problem Resolution

Open and resolved problems are tracked in `.fusa-problems.json`. All open problems must be resolved or risk-accepted before a production release.

## 8. Release Criteria

- All CI checks pass (lint, test, fuzz-short, gap-check)
- Zero `untested` requirements in `rsfusa check` output
- Code coverage ≥ 90%
- No open ASIL-B problems in `.fusa-problems.json`
- Safety Manager approval
