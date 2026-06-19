# Contributing to rust-RCP

## Developer Certificate of Origin

All contributions must be signed off per the [DCO](https://developercertificate.org/). Add `Signed-off-by: Your Name <email@example.com>` to your commit message.

## Code Style

- Follow `rustfmt` defaults (`cargo fmt --check` is enforced in CI)
- No warnings permitted (`cargo clippy -- -D warnings` must pass)
- No `unwrap()` outside `#[cfg(test)]` blocks
- `#![forbid(unsafe_code)]` — no unsafe Rust

## Safety Requirements

Every functional change must:

1. Annotate the relevant requirement: `// fusa:req REQ-XXX`
2. Add or update a test annotated: `// fusa:test REQ-XXX`
3. Pass `rsfusa check` with zero gaps

## Testing

```bash
cargo test               # unit + integration tests
cargo test --release     # check optimised build
cargo clippy -- -D warnings
cargo fmt --check
```

## Pull Request Process

1. Fork and create a feature branch
2. Write code with FuSa annotations
3. Ensure all CI checks pass
4. Open a PR against `main`
5. Request review from at least one maintainer
6. Safety-impacting changes require Safety Manager review

## Versioning

Semantic versioning: `MAJOR.MINOR.PATCH`

- MAJOR: breaking API change
- MINOR: backward-compatible new feature
- PATCH: backward-compatible bug fix

## Reporting Issues

- Security vulnerabilities: **security@example.com** (see SECURITY.md)
- Safety concerns: **safety@example.com**
- General bugs: GitHub Issues
