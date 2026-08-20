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
| Table 30 centralization | **partial** — `evtgroup.rs`'s `EvtRow2Kind`/`evt_row2_kind_of` implement Table 33's unambiguous Row-2 `evt[2:0]` rule (`{ADC, PWM_IN, I2C, LIN, CAN, UART, ISELED, MDIO}`); `i2c.rs` (v5.4.0 pilot), `adc.rs` (v5.5.0), `pwm.rs` (v5.6.0, PWM_IN only — PWM_OUT is not a Row-2 endpoint type), `lin.rs` (v5.7.0), `can.rs` (v5.8.0 — the first of these whose `evt[2:0] == 111b`/`ConfigWrite` arm returns a new dedicated error, `RcpError::ConfigWriteNotImplemented`, rather than `Ok(Self::ConfigWrite)`; see `can.rs`'s own doc comment "Provenance note: evt[2:0] request validation" for why), `uart.rs` (v5.9.0 — reverts to the `Ok(Self::ConfigWrite)` precedent I2C/LIN/ADC/PWM_IN share rather than following `can.rs`'s departure, since `UartRequest`'s own `ConfigWrite` arm constructs no UART-specific value and is under no equivalent pressure; `UartRequest`'s `evt[2:0] == 000b`/`Plain` case additionally splits into `Write`/`Read` variants reflecting UART's own independent TX/RX EP-request-storage split, TC18 §13.7.8.1, confirmed orthogonal to `evt[2:0]` classification — see `uart.rs`'s own doc comment "Provenance note: evt[2:0] request validation" for why), and `iseled.rs` (v5.10.0 — follows `can.rs`'s `Err(RcpError::ConfigWriteNotImplemented)` departure rather than the majority `Ok(Self::ConfigWrite)` precedent, since `IseledRequest::from_evt_sub_opcode` takes an already-decoded `IseledFrame` — matching `can.rs`'s `CanDataFrame`-accepting shape, not `i2c.rs`/`lin.rs`/`adc.rs`/`pwm.rs`/`uart.rs`'s own raw-bytes shape — so the same "no caller can honestly construct a config-write payload as one" pressure `can.rs`'s own doc comment names applies here too; see `iseled.rs`'s own doc comment "Provenance note: evt[2:0] request validation" for the full reasoning), and `mdio.rs` (v5.11.0 — the eighth and last Row-2 endpoint type; `MdioRequest::from_evt_sub_opcode` takes an already-decoded `MdioTransfer`, matching `can.rs`'s/`iseled.rs`'s own decoded-frame shape rather than `i2c.rs`/`lin.rs`/`adc.rs`/`pwm.rs`/`uart.rs`'s raw-bytes shape, but its `evt[2:0] == 111b`/`ConfigWrite` arm stays `Ok(Self::ConfigWrite)` — the majority precedent — rather than following `can.rs`'s/`iseled.rs`'s `Err(RcpError::ConfigWriteNotImplemented)` departure, since `MdioTransfer::decode` is infallible and interprets no structure at all, so the "no caller can honestly construct one" pressure that drove that departure does not apply to MDIO's own always-valid, opaque-bytes shape; see `mdio.rs`'s own doc comment "Provenance note: evt[2:0] request validation" for the full reasoning) are its callers. All eight Row-2 endpoint types (`{ADC, PWM_IN, I2C, LIN, CAN, UART, ISELED, MDIO}`) now call `evt_row2_kind_of`; GPIO/SPI's own `sub_opcode` readers remain their own private, unclassified reading, and the broader roadmap-named "Groups A/B/C" classification (`EvtGroup`/`classify_evt_sub_opcode`) is still unresolved. This entry previously read "conformant", which was stale: `classify_evt_sub_opcode` was a stub that always returned `Ok(None)`, by its own doc comment's admission |
| Conditional-request module unification | **conformant** (this repo is a reference shape, alongside cpp-RCP) |
| Per-function requirement tagging | **not conformant** — tags are collected at file level (top of each `.rs`), not per-function |
| `.fusa-reqs.json` schema (`tc18`/`tc18_master_id`/`status`) | **partial** — has `verificationMethod` but no citation field (TC18 citations live only in code doc-comments); has a working `status: "not-implemented"` exemption mechanism (`scripts/fusa-gap-check.sh`) that go-RCP/cpp-RCP lack |
| Conditional-request req-id grouping | not yet resolved — needs re-inventory once schema unification starts |
