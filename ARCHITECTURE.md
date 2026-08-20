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
| Table 30 centralization | **partial** — `evtgroup.rs`'s `EvtRow2Kind`/`evt_row2_kind_of` implement Table 33's unambiguous Row-2 `evt[2:0]` rule (`{ADC, PWM_IN, I2C, LIN, CAN, UART, ISELED, MDIO}`) and `i2c.rs` is its first caller (v5.4.0 pilot); the other seven Row-2 endpoint types don't call it yet, GPIO/SPI's own `sub_opcode` readers remain their own private, unclassified reading, and the broader roadmap-named "Groups A/B/C" classification (`EvtGroup`/`classify_evt_sub_opcode`) is still unresolved. This entry previously read "conformant", which was stale: `classify_evt_sub_opcode` was a stub that always returned `Ok(None)`, by its own doc comment's admission |
| Conditional-request module unification | **conformant** (this repo is a reference shape, alongside cpp-RCP) |
| Per-function requirement tagging | **not conformant** — tags are collected at file level (top of each `.rs`), not per-function |
| `.fusa-reqs.json` schema (`tc18`/`tc18_master_id`/`status`) | **partial** — has `verificationMethod` but no citation field (TC18 citations live only in code doc-comments); has a working `status: "not-implemented"` exemption mechanism (`scripts/fusa-gap-check.sh`) that go-RCP/cpp-RCP lack |
| Conditional-request req-id grouping | not yet resolved — needs re-inventory once schema unification starts |
