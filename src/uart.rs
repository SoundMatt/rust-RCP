// fusa:req REQ-UART-001
// fusa:req REQ-UART-002
// fusa:req REQ-UART-003
// fusa:req REQ-UART-004
// fusa:req REQ-UART-005
// fusa:req REQ-UART-006
// fusa:req REQ-UART-007
// fusa:req REQ-UART-008
// fusa:req REQ-UART-009
// fusa:req REQ-UART-010

//! The UART endpoint type (`ep_type 0x05`) — `ROADMAP.md` Milestone 4
//! ("Basic Endpoint Types"), fourth checklist bullet: "independent TX/RX
//! queues sharing one functional-config block; `read_size`-or-`uart_timeout`
//! read completion; payload-less-read-only rule (`UNKNOWN_CMD` if
//! violated)."
//!
//! This follows directly on [`crate::i2c`] (Milestone 4's third item, and
//! `ep_type 0x05`'s immediate predecessor `ep_type 0x04`): same milestone,
//! same "additive standalone plumbing only" discipline, same doc-comment
//! provenance-note style for anything this crate has not yet reconciled
//! against confirmed wire behavior. Three named pieces are in scope, all
//! implemented here:
//!
//! - [`UartTxQueue`] / [`UartRxQueue`] — the independent transmit and
//!   receive byte queues, each modeled as an unstructured byte stream this
//!   module does not interpret, matching [`crate::i2c::I2cByteTransfer`]'s
//!   own unstructured-stream discipline. See "Provenance note: two queues,
//!   one shared config block" below for how their independence is modeled
//!   at the config layer.
//! - [`UartReadCompletionReason`] / [`resolve_uart_read_completion`] — the
//!   `read_size`-or-`uart_timeout` race that ends an RX read, turned into a
//!   pure, typed, testable function mirroring
//!   [`crate::spi::truncate_spi_status_for_compound_wait`]'s and
//!   [`crate::i2c::I2cSpeedMode::is_ambiguous_high_speed_row`]'s own
//!   prose-rule-to-function discipline. See "Provenance note: the
//!   `read_size`/`uart_timeout` race" below for why this crate does not
//!   silently pick a winner when both conditions are satisfied at once.
//! - [`validate_uart_read_request`] — the payload-less-read-only rule,
//!   returning `Err(RcpError::UnsupportedCmd)` for any read request that
//!   carries a payload. See "Provenance note: `UNKNOWN_CMD` and
//!   `RcpError::UnsupportedCmd`" below for why this crate reads the
//!   checklist's literal `UNKNOWN_CMD` text as this already-defined variant
//!   rather than adding a new one.
//!
//! Deliberately out of scope, for the same reasons
//! [`crate::gpio`]'s/[`crate::spi`]'s/[`crate::i2c`]'s own doc comments
//! already give:
//!
//! - The "Groups A/B/C" `evt[2:0]` sub-opcode convention as a general,
//!   cross-endpoint-type classification scheme, and any use of
//!   `evt.sub_opcode` at all. `ROADMAP.md`'s UART checklist bullet names no
//!   `sub_opcode`-keyed selection mechanism, so this module reads
//!   `sub_opcode` nowhere.
//! - [`crate::regmap::CommonFunctionalConfig`]'s fields — unchanged here, as
//!   in every prior Milestone 1-4 entry.
//! - The content of a UART frame's per-byte framing (baud rate, parity, stop
//!   bits, and so on) and any peripheral-side (as opposed to controller-
//!   side) role. `ROADMAP.md`'s UART checklist bullet names only the queue
//!   split, the read-completion race, and the payload-less-read rule — no
//!   line-framing parameters or role-selection mechanism — so none of that
//!   is modeled here.
//! - Wiring any of the below into an actual decoder, dispatch loop, or
//!   [`crate::avtp`]/[`crate::acf`]/[`crate::addressing`] caller. This
//!   module remains additive standalone plumbing only, matching the
//!   discipline every prior Milestone 1-4 entry already established.
//!
//! ## Relationship to [`crate::regmap`]
//!
//! As with [`crate::gpio::GpioFunctionalConfig`],
//! [`crate::spi::SpiFunctionalConfig`], and
//! [`crate::i2c::I2cFunctionalConfig`], UART's real functional-config
//! content gets its own dedicated type, [`UartFunctionalConfig`], rather
//! than adding UART-specific fields directly onto the still-shared,
//! thirteen-endpoint-type [`crate::regmap::PerEpTypeFunctionalConfig`]
//! placeholder. [`UartFunctionalConfig::layer_tag`] shows how a caller
//! obtains the matching generic-layer tag so the two compose through
//! [`crate::regmap::check_functional_config_matches_ep_type`] exactly as
//! that cross-layer rule already expects, without this module editing
//! [`crate::regmap`] itself.
//!
//! ## Provenance note: two queues, one shared config block
//!
//! `ROADMAP.md`'s UART checklist bullet states the TX and RX queues are
//! "independent" but nonetheless share "one functional-config block" —
//! unlike [`crate::gpio::GpioFunctionalConfig`]'s,
//! [`crate::spi::SpiFunctionalConfig`]'s, and
//! [`crate::i2c::I2cFunctionalConfig`]'s single-queue shapes, this is the
//! first Milestone 4 functional config with an internal direction split to
//! represent. [`UartFunctionalConfig`] is still exactly one type (one
//! `layer_tag`, composing against
//! [`crate::regmap::check_functional_config_matches_ep_type`] exactly once,
//! matching the checklist's "one ... block" wording), but its two fields —
//! [`UartFunctionalConfig::tx`] and [`UartFunctionalConfig::rx`] — carry
//! each queue's own config content independently, so a caller can read or
//! change one queue's settings without touching the other's. The checklist
//! text names concrete config content only for the RX side (the `read_size`/
//! `uart_timeout` read-completion thresholds, carried by
//! [`UartRxQueueConfig`]); it names none for TX, so
//! [`UartTxQueueConfig`] is left an intentionally empty placeholder — the
//! same discipline [`crate::spi::SpiChannelConfigSlot`] already applies to
//! its own unnamed per-channel content — rather than this crate guessing
//! plausible TX-side fields (baud rate, queue depth, and so on) on its own.
//! Because TX carries no modeled field at all, there is currently no way for
//! this module's own types to represent a TX/RX field-level "conflicting
//! settings" case; a future item that adds real TX-side fields would need
//! to revisit whether any of them can conflict with RX's.
//!
//! ## Provenance note: the `read_size`/`uart_timeout` race
//!
//! `ROADMAP.md`'s UART checklist bullet names an RX read completing on
//! "`read_size`-or-`uart_timeout`" without stating which of the two wins if
//! both conditions become true at the same evaluation (for example, the
//! `read_size`th byte arrives in the same instant the timeout elapses, or —
//! see below — both thresholds are left at their zero default). Per Guiding
//! Principle 5, [`resolve_uart_read_completion`] does not silently pick one:
//! [`UartReadCompletionReason`] carries an explicit third variant,
//! [`UartReadCompletionReason::Both`], for exactly that simultaneous case,
//! rather than this crate guessing that size-reached or timed-out takes
//! priority — mirroring [`crate::gpio::GpioWriteSemantics::Unnamed8th`]'s
//! and [`crate::i2c::I2cSpeedMode::HighSpeedRowA`]/
//! [`crate::i2c::I2cSpeedMode::HighSpeedRowB`]'s own treatment of unresolved
//! enum slots. Relatedly, this module does not treat a zero `read_size` or a
//! zero `uart_timeout` as a "this threshold is disabled" sentinel — no such
//! convention is stated by the checklist text, and this crate does not
//! invent unconfirmed sentinel conventions (see [`crate::acf`]'s own
//! `read_size`/`segment_num` provenance note for the same discipline applied
//! to a different field). Ordinary `>=` comparison against a zero threshold
//! is therefore already satisfied before any byte arrives or any time
//! elapses, so [`UartFunctionalConfig::default`]'s zeroed
//! [`UartRxQueueConfig`] resolves every read as immediately complete via
//! [`UartReadCompletionReason::Both`] — a consequence of not inventing a
//! disabling sentinel, not a claim about real RC Server power-on behavior.
//! [`UartRxQueueConfig::read_size`] itself reuses
//! [`crate::acf::ReadSizeOrSegmentNum`] rather than a UART-private read-size
//! type, since the checklist's `read_size` name is the same wire field
//! [`crate::acf::ByteMessageInfo::read_size_segment_num`] already carries —
//! mirroring how [`crate::timestamp::MessageTimestamp`] builds endpoint-
//! facing semantics atop an already-decoded generic ACF field rather than
//! reinventing one. `uart_timeout` has no such existing crate-level
//! counterpart, so it is carried as a plain `u32` tick count of unconfirmed
//! width and units — this crate's own working placeholder, not a
//! transcription of a confirmed wire encoding.
//!
//! ## Provenance note: `UNKNOWN_CMD` and `RcpError::UnsupportedCmd`
//!
//! `ROADMAP.md`'s UART checklist bullet names the payload-less-read
//! violation's error code as `UNKNOWN_CMD`. [`crate::RcpError`]'s own doc
//! comment (Milestone 2's "Error Model" item) already renamed every
//! provisional sentinel it introduced onto one of eleven Rust-cased,
//! spec-named variants read off the TC18 error-code checklist — and that
//! eleven-name list has no `UnknownCmd`/`UNKNOWN_CMD` entry at all. The
//! closest existing match, both lexically and by the failure it represents
//! (a command this RC Server does not recognize/support, as opposed to one
//! it recognizes but currently cannot authorize or complete), is
//! [`crate::RcpError::UnsupportedCmd`] (`UNSUPPORTED_CMD`) — already used by
//! [`crate::gpio::apply_gpio_write`] for GPIO's own unnamed-eighth-semantics
//! refusal and by [`crate::spi::resolve_spi_channel_index`] for SPI's spare-
//! channel refusal. Per Guiding Principle 5, this module flags the mismatch
//! explicitly here rather than silently treating the two names as identical
//! and rather than adding a new `UnknownCmd` variant to
//! [`crate::RcpError`] on the strength of one checklist bullet's wording,
//! which would duplicate `UnsupportedCmd`'s coverage without a
//! checklist-cited behavioral difference between the two. This crate's
//! working choice is that `UNKNOWN_CMD` is this checklist bullet's own
//! informal phrasing for the already-reconciled `UNSUPPORTED_CMD` code, not
//! a twelfth spec-named error code Milestone 2 missed —
//! [`validate_uart_read_request`] therefore returns
//! [`crate::RcpError::UnsupportedCmd`].

use crate::acf::ReadSizeOrSegmentNum;
use crate::RcpError;

// ── UartTxQueue / UartRxQueue ────────────────────────────────────────────────

/// The independent transmit queue's raw bytes: the bytes a UART write
/// request sends from controller to the wire.
///
/// Modeled as an unstructured, variable-length byte stream — this module
/// does not interpret its contents — matching how
/// [`crate::i2c::I2cByteTransfer`] modeled its own controller-to-bus byte
/// stream. Every possible byte slice, including an empty one, has a valid
/// encoding, so [`UartTxQueue::decode`] is infallible.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
// fusa:req REQ-UART-001
pub struct UartTxQueue {
    /// The raw bytes queued for transmission.
    pub bytes: Vec<u8>,
}

impl UartTxQueue {
    /// Encode this queue's bytes to their raw wire representation:
    /// `bytes`, unmodified and unframed.
    // fusa:req REQ-UART-001
    pub fn encode(&self) -> Vec<u8> {
        self.bytes.clone()
    }

    /// Decode a [`UartTxQueue`] from a byte slice.
    ///
    /// Every possible byte slice, including an empty one, is a valid TX
    /// queue payload, so this never fails and never panics for any input.
    // fusa:req REQ-UART-001
    pub fn decode(b: &[u8]) -> Self {
        Self { bytes: b.to_vec() }
    }
}

/// The independent receive queue's raw bytes: the bytes a UART read
/// response returns from the wire to the controller.
///
/// See [`UartTxQueue`]'s doc comment — this is the same unstructured,
/// variable-length byte-stream modeling for the opposite queue/direction.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
// fusa:req REQ-UART-002
pub struct UartRxQueue {
    /// The raw bytes collected from reception.
    pub bytes: Vec<u8>,
}

impl UartRxQueue {
    /// Encode this queue's bytes to their raw wire representation:
    /// `bytes`, unmodified and unframed.
    // fusa:req REQ-UART-002
    pub fn encode(&self) -> Vec<u8> {
        self.bytes.clone()
    }

    /// Decode a [`UartRxQueue`] from a byte slice.
    ///
    /// Every possible byte slice, including an empty one, is a valid RX
    /// queue payload, so this never fails and never panics for any input.
    // fusa:req REQ-UART-002
    pub fn decode(b: &[u8]) -> Self {
        Self { bytes: b.to_vec() }
    }
}

// ── UartFunctionalConfig ─────────────────────────────────────────────────────

/// The TX queue's own config content.
///
/// An intentionally empty placeholder — see this module's doc comment
/// "Provenance note: two queues, one shared config block" for why the
/// checklist names no concrete TX-side field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
// fusa:req REQ-UART-003
pub struct UartTxQueueConfig;

/// The RX queue's own config content: the `read_size`/`uart_timeout`
/// read-completion thresholds [`resolve_uart_read_completion`] races
/// against each other.
///
/// See this module's doc comment "Provenance note: the
/// `read_size`/`uart_timeout` race" for why `read_size` reuses
/// [`crate::acf::ReadSizeOrSegmentNum`] and for `uart_timeout`'s own
/// unconfirmed width/units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
// fusa:req REQ-UART-003
pub struct UartRxQueueConfig {
    /// The read-size completion threshold, reusing the same wire field
    /// [`crate::acf::ByteMessageInfo::read_size_segment_num`] already
    /// carries.
    pub read_size: ReadSizeOrSegmentNum,
    /// The read-timeout completion threshold, this crate's own unconfirmed
    /// tick-count placeholder.
    pub uart_timeout: u32,
}

/// UART's own per-EP-type functional-config content: one shared config
/// block covering both the TX and RX queues' independent settings.
///
/// See this module's doc comment "Relationship to `crate::regmap`" for why
/// this is a dedicated type rather than content added directly to
/// [`crate::regmap::PerEpTypeFunctionalConfig`], and "Provenance note: two
/// queues, one shared config block" for why this single type nonetheless
/// carries two independent, direction-tagged fields rather than the
/// simpler single-queue shape [`crate::gpio::GpioFunctionalConfig`],
/// [`crate::spi::SpiFunctionalConfig`], and
/// [`crate::i2c::I2cFunctionalConfig`] each used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
// fusa:req REQ-UART-004
pub struct UartFunctionalConfig {
    /// The TX queue's own config content.
    pub tx: UartTxQueueConfig,
    /// The RX queue's own config content.
    pub rx: UartRxQueueConfig,
}

impl UartFunctionalConfig {
    /// The [`crate::regmap::PerEpTypeFunctionalConfig`] generic-layer tag
    /// that matches this UART functional config, for use with
    /// [`crate::regmap::check_functional_config_matches_ep_type`].
    ///
    /// This module does not itself call that function — it only shows how a
    /// caller would obtain the matching tag, per this module's doc comment
    /// "Relationship to `crate::regmap`".
    // fusa:req REQ-UART-004
    pub fn layer_tag(&self) -> crate::regmap::PerEpTypeFunctionalConfig {
        crate::regmap::PerEpTypeFunctionalConfig::new(crate::regmap::EndpointType::Uart)
    }
}

// ── read_size-or-uart_timeout read completion ────────────────────────────────

/// Which of the `read_size`/`uart_timeout` completion conditions caused an
/// RX read to complete, as resolved by [`resolve_uart_read_completion`].
///
/// See this module's doc comment "Provenance note: the
/// `read_size`/`uart_timeout` race" for why [`UartReadCompletionReason::Both`]
/// exists as an explicit third outcome rather than either of the other two
/// being silently preferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// fusa:req REQ-UART-005
// fusa:req REQ-UART-006
// fusa:req REQ-UART-007
pub enum UartReadCompletionReason {
    /// Only the `read_size` byte-count threshold was met.
    ReadSizeReached,
    /// Only the `uart_timeout` elapsed-time threshold was met.
    TimedOut,
    /// Both thresholds were met at the same evaluation. See this module's
    /// doc comment "Provenance note: the `read_size`/`uart_timeout` race".
    Both,
}

/// Resolve the `read_size`-or-`uart_timeout` race for one RX read,
/// given how many bytes have been collected so far and how much time has
/// elapsed since the read began.
///
/// Returns `None` if neither threshold in `rx` has yet been met (the read
/// is still in progress). Both thresholds use ordinary `>=` comparison
/// against `rx`'s configured values — see this module's doc comment
/// "Provenance note: the `read_size`/`uart_timeout` race" for why a
/// zero-valued threshold is not treated as "disabled." Never panics for any
/// input.
// fusa:req REQ-UART-005
// fusa:req REQ-UART-006
// fusa:req REQ-UART-007
// fusa:req REQ-UART-008
pub fn resolve_uart_read_completion(
    rx: &UartRxQueueConfig,
    bytes_collected: u16,
    elapsed: u32,
) -> Option<UartReadCompletionReason> {
    let size_reached = bytes_collected >= u16::from(rx.read_size.as_read_size());
    let timed_out = elapsed >= rx.uart_timeout;

    match (size_reached, timed_out) {
        (true, true) => Some(UartReadCompletionReason::Both),
        (true, false) => Some(UartReadCompletionReason::ReadSizeReached),
        (false, true) => Some(UartReadCompletionReason::TimedOut),
        (false, false) => None,
    }
}

// ── Payload-less-read-only rule ──────────────────────────────────────────────

/// Validate the payload-less-read-only rule: a UART read request must carry
/// no payload.
///
/// Returns `Err(RcpError::UnsupportedCmd)` if `payload` is non-empty — see
/// this module's doc comment "Provenance note: `UNKNOWN_CMD` and
/// `RcpError::UnsupportedCmd`" for why this crate reads the checklist's
/// `UNKNOWN_CMD` wording onto this already-defined variant. Never panics for
/// any input.
// fusa:req REQ-UART-009
// fusa:req REQ-UART-010
pub fn validate_uart_read_request(payload: &[u8]) -> Result<(), RcpError> {
    if payload.is_empty() {
        Ok(())
    } else {
        Err(RcpError::UnsupportedCmd)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── UartTxQueue / UartRxQueue: round-trip / never-panic ─────────────────

    #[test]
    // fusa:test REQ-UART-001
    fn uart_tx_queue_round_trips_through_encode_decode() {
        for bytes in [vec![], vec![0x00], vec![0xAA; 3], (0u8..=255).collect()] {
            let queue = UartTxQueue {
                bytes: bytes.clone(),
            };
            assert_eq!(UartTxQueue::decode(&queue.encode()).bytes, bytes);
        }
    }

    #[test]
    // fusa:test REQ-UART-001
    fn uart_tx_queue_decode_never_panics_for_any_sampled_input() {
        for len in [0usize, 1, 2, 7, 64] {
            let buf = vec![0x5Au8; len];
            let _ = UartTxQueue::decode(&buf);
        }
    }

    #[test]
    // fusa:test REQ-UART-002
    fn uart_rx_queue_round_trips_through_encode_decode() {
        for bytes in [vec![], vec![0xFF], vec![0x01, 0x02, 0x03]] {
            let queue = UartRxQueue {
                bytes: bytes.clone(),
            };
            assert_eq!(UartRxQueue::decode(&queue.encode()).bytes, bytes);
        }
    }

    #[test]
    // fusa:test REQ-UART-002
    fn uart_rx_queue_decode_never_panics_for_any_sampled_input() {
        for len in [0usize, 1, 5, 32] {
            let buf = vec![0xA5u8; len];
            let _ = UartRxQueue::decode(&buf);
        }
    }

    // ── UartFunctionalConfig / layer_tag ─────────────────────────────────────

    #[test]
    // fusa:test REQ-UART-003
    fn uart_tx_and_rx_queue_configs_default_independently() {
        let config = UartFunctionalConfig::default();
        assert_eq!(config.tx, UartTxQueueConfig);
        assert_eq!(config.rx, UartRxQueueConfig::default());
        assert_eq!(config.rx.read_size, ReadSizeOrSegmentNum::default());
        assert_eq!(config.rx.uart_timeout, 0);
    }

    #[test]
    // fusa:test REQ-UART-004
    fn uart_functional_config_layer_tag_matches_ep_type_uart() {
        let functional = UartFunctionalConfig::default();
        let generic = crate::regmap::PerEpConfigBlock::new(crate::regmap::EndpointType::Uart);
        let tag = functional.layer_tag();
        assert_eq!(tag.ep_type, crate::regmap::EndpointType::Uart);
        assert!(crate::regmap::functional_config_matches_ep_type(
            &generic, &tag
        ));
        assert_eq!(
            crate::regmap::check_functional_config_matches_ep_type(&generic, &tag),
            Ok(())
        );
    }

    #[test]
    // fusa:test REQ-UART-004
    fn uart_functional_config_layer_tag_rejects_mismatched_ep_type() {
        let functional = UartFunctionalConfig::default();
        let generic = crate::regmap::PerEpConfigBlock::new(crate::regmap::EndpointType::I2c);
        let tag = functional.layer_tag();
        assert!(!crate::regmap::functional_config_matches_ep_type(
            &generic, &tag
        ));
    }

    // ── resolve_uart_read_completion: the three-way race ────────────────────

    fn rx_config(read_size: u8, uart_timeout: u32) -> UartRxQueueConfig {
        UartRxQueueConfig {
            read_size: ReadSizeOrSegmentNum(read_size),
            uart_timeout,
        }
    }

    #[test]
    // fusa:test REQ-UART-005
    fn resolve_uart_read_completion_reports_read_size_reached_only() {
        let rx = rx_config(10, 1000);
        assert_eq!(
            resolve_uart_read_completion(&rx, 10, 500),
            Some(UartReadCompletionReason::ReadSizeReached)
        );
        assert_eq!(
            resolve_uart_read_completion(&rx, 20, 999),
            Some(UartReadCompletionReason::ReadSizeReached)
        );
    }

    #[test]
    // fusa:test REQ-UART-006
    fn resolve_uart_read_completion_reports_timed_out_only() {
        let rx = rx_config(10, 1000);
        assert_eq!(
            resolve_uart_read_completion(&rx, 5, 1000),
            Some(UartReadCompletionReason::TimedOut)
        );
        assert_eq!(
            resolve_uart_read_completion(&rx, 0, 5000),
            Some(UartReadCompletionReason::TimedOut)
        );
    }

    #[test]
    // fusa:test REQ-UART-007
    fn resolve_uart_read_completion_reports_both_on_simultaneous_thresholds() {
        let rx = rx_config(10, 1000);
        assert_eq!(
            resolve_uart_read_completion(&rx, 10, 1000),
            Some(UartReadCompletionReason::Both)
        );
    }

    #[test]
    // fusa:test REQ-UART-007
    fn resolve_uart_read_completion_zeroed_config_resolves_both_immediately() {
        // See this module's doc comment: zero is not treated as a
        // "disabled" sentinel for either threshold.
        let rx = UartRxQueueConfig::default();
        assert_eq!(
            resolve_uart_read_completion(&rx, 0, 0),
            Some(UartReadCompletionReason::Both)
        );
    }

    #[test]
    // fusa:test REQ-UART-008
    fn resolve_uart_read_completion_returns_none_before_either_threshold() {
        let rx = rx_config(10, 1000);
        assert_eq!(resolve_uart_read_completion(&rx, 9, 999), None);
        // A genuinely in-progress read: some bytes in, some time elapsed,
        // neither threshold met.
        assert_eq!(resolve_uart_read_completion(&rx, 3, 200), None);
    }

    #[test]
    // fusa:test REQ-UART-008
    fn resolve_uart_read_completion_never_panics_for_any_sampled_input() {
        let configs = [
            rx_config(0, 0),
            rx_config(0, u32::MAX),
            rx_config(255, 0),
            rx_config(255, u32::MAX),
            rx_config(10, 1000),
        ];
        let byte_samples = [0u16, 1, 10, 255, u16::MAX];
        let elapsed_samples = [0u32, 1, 1000, u32::MAX];
        for rx in configs {
            for &bytes in &byte_samples {
                for &elapsed in &elapsed_samples {
                    let _ = resolve_uart_read_completion(&rx, bytes, elapsed);
                }
            }
        }
    }

    // ── validate_uart_read_request: payload-less-read-only rule ─────────────

    #[test]
    // fusa:test REQ-UART-009
    fn validate_uart_read_request_accepts_empty_payload() {
        assert_eq!(validate_uart_read_request(&[]), Ok(()));
    }

    #[test]
    // fusa:test REQ-UART-010
    fn validate_uart_read_request_rejects_any_non_empty_payload() {
        for payload in [vec![0x00], vec![0x01, 0x02], vec![0xFF; 16]] {
            assert_eq!(
                validate_uart_read_request(&payload),
                Err(RcpError::UnsupportedCmd)
            );
        }
    }

    #[test]
    // fusa:test REQ-UART-010
    fn validate_uart_read_request_never_panics_for_any_sampled_input() {
        for len in [0usize, 1, 2, 8, 64] {
            let buf = vec![0x5Au8; len];
            let _ = validate_uart_read_request(&buf);
        }
    }
}
