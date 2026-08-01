# Architecture

rust-RCP's architecture follows the canonical cross-repo design at
[RELAY's `docs/RCP-ARCHITECTURE.md`](https://github.com/SoundMatt/RELAY/blob/main/docs/RCP-ARCHITECTURE.md),
shared with go-RCP, c-RCP, and cpp-RCP.

## File-path mapping

| Lexicon term | This repo |
|---|---|
| wire / ACF layer | `src/acf.rs` |
| framing / AVTP layer | `src/avtp.rs` |
| response classification | *(missing — see below)* |
| conditional-request layer | `src/request.rs` |
| Table 30 / evt[2:0] write semantics | `src/evtgroup.rs` |
| endpoint-type modules | `src/gpio.rs`, `src/spi.rs`, `src/pwm.rs`, `src/adc.rs`, `src/i2c.rs`, `src/lin.rs`, `src/can.rs`, `src/uart.rs`, `src/iseled.rs`, `src/mdio.rs`, `src/wakeup.rs` |
| dispatch/routing | `dispatch_request()` in `src/udp.rs` |

## Conformance status against the canonical architecture

| Canonical choice | Status |
|---|---|
| Response classification (evt-first) | **not conformant** — no classifier exists at all; tracked |
| Table 30 centralization | **conformant** (this repo is a cross-repo reference implementation) |
| Conditional-request module unification | **conformant** (this repo is a reference shape, alongside cpp-RCP) |
| Per-function requirement tagging | **not conformant** — tags are collected at file level (top of each `.rs`), not per-function |
| `.fusa-reqs.json` schema (`tc18`/`tc18_master_id`/`status`) | **partial** — has `verificationMethod` but no citation field (TC18 citations live only in code doc-comments); has a working `status: "not-implemented"` exemption mechanism (`scripts/fusa-gap-check.sh`) that go-RCP/cpp-RCP lack |
| Conditional-request req-id grouping | not yet resolved — needs re-inventory once schema unification starts |
