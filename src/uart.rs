//fusa:req REQ-UART-001
//fusa:req REQ-UART-002
//fusa:req REQ-UART-003
//fusa:req REQ-UART-004
//fusa:req REQ-UART-005
//fusa:req REQ-UART-006
//fusa:req REQ-UART-007
//fusa:req REQ-UART-008
//fusa:req REQ-UART-009
//fusa:req REQ-UART-010
//fusa:req REQ-UART-011
//fusa:req REQ-UART-012

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
//! against confirmed wire behavior. Three named pieces were originally in
//! scope, all implemented here; a fourth,
//! [`UartRequest`]/[`UartRequest::from_evt_sub_opcode`], was added
//! afterward (see "Provenance note: evt[2:0] request validation" below):
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
//! - [`UartRequest`]/[`UartRequest::from_evt_sub_opcode`] — UART's own
//!   request-decode entry point, validating an incoming request's
//!   `evt.sub_opcode` against [`crate::evtgroup::evt_row2_kind_of`]'s TC18
//!   §13.5 Table 33 Row-2 rule. See "Provenance note: evt[2:0] request
//!   validation" below — this piece was added after this module's own
//!   original scope note (still accurate for why no `sub_opcode` reading
//!   existed here originally) as this crate's sixth Row-2 endpoint-type
//!   module, following
//!   [`crate::i2c::I2cRequest`]/[`crate::lin::LinRequest`]/
//!   [`crate::adc::AdcRequest`]/[`crate::pwm::PwmInRequest`]'s own prior
//!   applications of the same shared predicate and
//!   [`crate::can::CanRequest`]'s own deliberate departure from their
//!   shared `Ok(Self::ConfigWrite)` precedent for `evt[2:0] == 111b`. This
//!   module does **not** depart the same way CAN did — see "Provenance
//!   note: evt[2:0] request validation" below for why UART's own situation
//!   does not force the same choice. The remaining two Row-2 endpoint types
//!   (`ISELED, MDIO`) are expected to follow the same pattern in their own
//!   later items.
//!
//! Deliberately out of scope, for the same reasons
//! [`crate::gpio`]'s/[`crate::spi`]'s/[`crate::i2c`]'s own doc comments
//! already give:
//!
//! - The "Groups A/B/C" `evt[2:0]` sub-opcode convention
//!   ([`crate::evtgroup::EvtGroup`]) as a general, cross-endpoint-type
//!   classification scheme — [`crate::evtgroup`]'s own doc comment already
//!   flags that broader scheme as unresolved, independent of the narrower,
//!   unambiguous Table 33 Row-2 rule this module's [`UartRequest`] now
//!   implements (see "Provenance note: evt[2:0] request validation" below).
//!   `ROADMAP.md`'s UART checklist bullet itself names no
//!   `sub_opcode`-keyed selection mechanism of its own — the Row-2 rule
//!   [`UartRequest`] implements comes from TC18 §13.5 Table 33, a separate,
//!   later-discovered item, not from this checklist bullet.
//! - [`crate::regmap::CommonFunctionalConfig`]'s fields — unchanged here, as
//!   in every prior Milestone 1-4 entry.
//! - The content of a UART frame's per-byte framing (baud rate, parity, stop
//!   bits, and so on) and any peripheral-side (as opposed to controller-
//!   side) role. `ROADMAP.md`'s UART checklist bullet names only the queue
//!   split, the read-completion race, and the payload-less-read rule — no
//!   line-framing parameters or role-selection mechanism — so none of that
//!   is modeled here. [`UartRequest`] carries this forward: its
//!   [`Write`](UartRequest::Write) variant wraps a raw, unframed
//!   [`UartTxQueue`].
//! - Decoding [`UartRequest::ConfigWrite`]'s own TC18 §12.7.1 payload shape.
//!   [`UartRequest::from_evt_sub_opcode`] recognizes a config-write request
//!   as distinct from a [`Write`](UartRequest::Write)/[`Read`](UartRequest::Read)
//!   one, but does not itself interpret what the config-write payload
//!   contains — that is separate, later work, same as every Row-2
//!   endpoint-type module this predicate lands in except
//!   [`crate::can::CanRequest`], whose own `CanDataFrame`-accepting
//!   signature could not honestly afford the same leniency (see
//!   "Provenance note: evt[2:0] request validation" below).
//! - Wiring [`UartRequest::from_evt_sub_opcode`] into an actual decoder,
//!   dispatch loop, or [`crate::mock::Endpoint`] implementation.
//!   [`crate::mock::Endpoint`]'s own trait signature already has separate
//!   `read`/`write` methods (unlike every other Row-2 endpoint-type
//!   module's own single-request-storage shape), but neither method
//!   carries an `evt` value through to any implementation yet — that gap
//!   is not specific to UART, it applies identically to
//!   [`crate::i2c::I2cRequest::from_evt_sub_opcode`]/
//!   [`crate::lin::LinRequest::from_evt_sub_opcode`]/
//!   [`crate::adc::AdcRequest::from_evt_sub_opcode`]/
//!   [`crate::pwm::PwmInRequest::from_evt_sub_opcode`]/
//!   [`crate::can::CanRequest::from_evt_sub_opcode`] (each confirmed still
//!   unwired against [`crate::mock::Endpoint`]'s own doc comment).
//!   [`UartRequest`] is built to that same "additive standalone plumbing
//!   only" level.
//! - Wiring any of this module's other, original three pieces into an
//!   actual decoder, dispatch loop, or
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
//! [`crate::acf::ReadSizeOrSegment`] rather than a UART-private read-size
//! type, since the checklist's `read_size` name is the same wire field
//! [`crate::acf::ByteMessageInfo::read_size_segment`] already carries —
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
//!
//! ## Provenance note: evt[2:0] request validation
//!
//! UART is one of the eight endpoint types TC18 §13.5 Table 33 groups into
//! one shared "Row 2" `evt[2:0]` rule (TC18.txt lines 4085-4092, `UART`
//! itself named at line 4090) — see [`crate::evtgroup`]'s own doc comment
//! "Provenance note: TC18 §13.5 Table 33's Row-2 rule (`evt_row2_kind_of`)"
//! for the full citation, including the literal-text discrepancy that
//! module's doc comment flags and resolves (Table 33's own printed Row-2
//! cell reads "000b to 110b reserved", including 000b, which this crate
//! does not implement literally). [`UartRequest::from_evt_sub_opcode`] is
//! this module's own caller of that shared
//! [`crate::evtgroup::evt_row2_kind_of`] predicate — UART's own request
//! format (TC18 §13.7.8.3, TC18.txt lines 5391-5406) carries the same `evt`
//! field in its Message Info header every other endpoint type's request
//! does, and TC18 names no UART-specific override of Table 33's generic
//! rule anywhere in §13.7.8.
//!
//! **UART's own read/write queue split (TC18 §13.7.8.1) is a genuinely
//! separate, orthogonal concern from `evt[2:0]` classification, confirmed
//! independently against TC18.txt rather than assumed.** §13.7.8.1
//! (TC18.txt lines 5292-5293) states the TX/RX split as "these two
//! processes are independent from each other, thus the UART EP has two EP
//! request storages" — a statement about which of two request storages a
//! request targets, with no reference to `evt` anywhere in that section.
//! Table 33's Row-2 rule (§13.5) is, symmetrically, stated once for all
//! eight Row-2 endpoint types with no per-type carve-out for UART's own
//! two-queue structure — `evt[2:0]` classifies a request's payload handling
//! (ordinary/reserved/config-write) the same way regardless of which queue
//! it targets. Neither section cites the other. Because of that, this
//! module's own [`UartTxQueue`]/[`UartRxQueue`] split (already established
//! before this item) and [`crate::evtgroup::evt_row2_kind_of`]'s Row-2
//! classification are two independent axes a real UART request sits on at
//! once, not one derived from the other.
//!
//! **`UartRequest::from_evt_sub_opcode` therefore takes an explicit
//! `is_write: bool` direction argument, rather than inventing a
//! UART-private direction type or guessing direction from the payload's
//! shape.** This reuses this crate's own existing, already-confirmed
//! direction convention: [`crate::acf::ByteMessageInfo::op`] is the generic
//! ACF-level flag every endpoint type's request header already carries to
//! select between read and write handling (TC18 §11.2.1 Table 4, TC18.txt
//! line 1235: "if op = 0 this is read_size, else segment_num"; see
//! [`crate::acf::ByteMessageInfo::read_size`]/
//! [`crate::acf::ByteMessageInfo::segment_num`]/
//! [`crate::acf::ByteMessageInfo::response_kind`]'s own `op`-gated
//! Read/Write reading), and [`crate::authz::Policy`]'s own doc comment
//! already names the exact convention this module reuses verbatim:
//! "`is_write` mirrors [`crate::acf::ByteMessageInfo::op`]'s own
//! true-is-write convention." `UartRequest::from_evt_sub_opcode` follows
//! that same naming and polarity rather than inventing a new
//! `UartRequestDirection`-shaped enum for a distinction this crate has
//! already named once. [`crate::mock::Endpoint`]'s own separate `read`/
//! `write` methods (see "Deliberately out of scope" above) are this same
//! direction split's other existing expression in this crate, one layer
//! further from the wire.
//!
//! Given `is_write`, [`UartRequest::from_evt_sub_opcode`]'s
//! [`EvtRow2Kind::Plain`] arm dispatches to one of two structurally
//! different outcomes rather than one shared payload type, honestly
//! reflecting the two-queue split above: `is_write == true` decodes
//! `payload` as a [`UartTxQueue`] (TC18 §13.7.8.3: "The byte_msg_payload in
//! the request is the UART payload"), exactly the payload TC18 §13.7.8.1
//! says "leads to a transmission of data to an external connected device";
//! `is_write == false` instead re-enforces this module's own pre-existing
//! [`validate_uart_read_request`] payload-less-read-only rule (TC18
//! §13.7.8.1, TC18.txt line 5303: "A read request having a byte_msg_payload
//! will be rejected with error code = UNKNOWN_CMD") and, on success,
//! constructs [`UartRequest::Read`] with no payload at all — there is
//! nothing UART-specific to decode on a valid read request's request side;
//! its `byte_msg_payload` arrives later, in the *response*.
//! [`UartTxQueue::decode`] is itself infallible over every byte slice, so
//! the only way [`Write`](UartRequest::Write) fails is through
//! [`EvtRow2Kind::Reserved`]'s own rejection below; the only way
//! [`Read`](UartRequest::Read) fails is [`validate_uart_read_request`]'s
//! own pre-existing `Err(`[`RcpError::UnsupportedCmd`]`)` for a non-empty
//! payload, propagated unchanged rather than re-derived.
//!
//! **Unlike [`crate::can::CanRequest::from_evt_sub_opcode`],
//! `UartRequest::from_evt_sub_opcode` returns `Ok(`[`UartRequest::ConfigWrite`]`)`
//! for `evt[2:0] == 111b`, following
//! [`crate::i2c::I2cRequest::from_evt_sub_opcode`]'s/
//! [`crate::lin::LinRequest::from_evt_sub_opcode`]'s/
//! [`crate::adc::AdcRequest::from_evt_sub_opcode`]'s/
//! [`crate::pwm::PwmInRequest::from_evt_sub_opcode`]'s original precedent
//! rather than CAN's own departure from it.** [`crate::can`]'s own doc
//! comment "Provenance note: evt[2:0] request validation" explains CAN's
//! departure as following directly from its signature: CAN's
//! `from_evt_sub_opcode` requires its caller to supply an already-decoded
//! [`crate::can::CanDataFrame`] *before* the function is even called, and a
//! genuine TC18 §12.7.1 config-write payload is definitionally not a CAN
//! data frame at all, so silently accepting whatever frame was supplied
//! and returning `Ok(CanRequest::ConfigWrite)` regardless was judged
//! dishonest. `UartRequest::from_evt_sub_opcode` is not under that same
//! pressure: its `evt[2:0] == 111b` arm does not need to construct
//! [`UartTxQueue`] (or anything else derived from `payload`) at all to
//! produce [`UartRequest::ConfigWrite`] — exactly like
//! [`crate::i2c::I2cRequest`]'s/[`crate::lin::LinRequest`]'s/
//! [`crate::adc::AdcRequest`]'s/[`crate::pwm::PwmInRequest`]'s own
//! `ConfigWrite` arms, it can harmlessly decline to interpret `payload`
//! (and, here, `is_write` too — a config-write is an EP-level functional-
//! config operation per §12.7.1, not a per-queue one, so which queue would
//! have been targeted is not itself meaningful for this arm) rather than
//! being structurally forced to either misuse a required value or reject
//! it. There is no CAN-style "no caller can honestly construct one to pass
//! in" problem here, since nothing UART-specific is constructed on this
//! arm at all. [`UartRequest::ConfigWrite`] therefore behaves exactly like
//! its four non-CAN siblings: recognized, not yet decoded — TC18 §12.7.1's
//! config-write payload shape remains deferred crate-wide (see
//! "Deliberately out of scope" above).
//!
//! Every `Reserved` sub_opcode value (`evt[2:0]` in `001b..=110b`, or any
//! value outside the 3-bit field's representable range) is rejected with
//! `Err(`[`RcpError::UnsupportedCmd`]`)`, matching Table 33's own stated
//! error code and
//! [`crate::i2c::I2cRequest::from_evt_sub_opcode`]'s/
//! [`crate::lin::LinRequest::from_evt_sub_opcode`]'s/
//! [`crate::adc::AdcRequest::from_evt_sub_opcode`]'s/
//! [`crate::pwm::PwmInRequest::from_evt_sub_opcode`]'s/
//! [`crate::can::CanRequest::from_evt_sub_opcode`]'s identical refusal of
//! their own table's reserved code — this part is unchanged from every
//! prior Row-2 endpoint-type module, CAN included.

use crate::acf::ReadSizeOrSegment;
use crate::evtgroup::{evt_row2_kind_of, EvtRow2Kind};
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
//fusa:req REQ-UART-001
pub struct UartTxQueue {
    /// The raw bytes queued for transmission.
    pub bytes: Vec<u8>,
}

impl UartTxQueue {
    /// Encode this queue's bytes to their raw wire representation:
    /// `bytes`, unmodified and unframed.
    //fusa:req REQ-UART-001
    pub fn encode(&self) -> Vec<u8> {
        self.bytes.clone()
    }

    /// Decode a [`UartTxQueue`] from a byte slice.
    ///
    /// Every possible byte slice, including an empty one, is a valid TX
    /// queue payload, so this never fails and never panics for any input.
    //fusa:req REQ-UART-001
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
//fusa:req REQ-UART-002
pub struct UartRxQueue {
    /// The raw bytes collected from reception.
    pub bytes: Vec<u8>,
}

impl UartRxQueue {
    /// Encode this queue's bytes to their raw wire representation:
    /// `bytes`, unmodified and unframed.
    //fusa:req REQ-UART-002
    pub fn encode(&self) -> Vec<u8> {
        self.bytes.clone()
    }

    /// Decode a [`UartRxQueue`] from a byte slice.
    ///
    /// Every possible byte slice, including an empty one, is a valid RX
    /// queue payload, so this never fails and never panics for any input.
    //fusa:req REQ-UART-002
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
//fusa:req REQ-UART-003
pub struct UartTxQueueConfig;

/// The RX queue's own config content: the `read_size`/`uart_timeout`
/// read-completion thresholds [`resolve_uart_read_completion`] races
/// against each other.
///
/// See this module's doc comment "Provenance note: the
/// `read_size`/`uart_timeout` race" for why `read_size` reuses
/// [`crate::acf::ReadSizeOrSegment`] and for `uart_timeout`'s own
/// unconfirmed width/units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-UART-003
pub struct UartRxQueueConfig {
    /// The read-size completion threshold, reusing the same wire field
    /// [`crate::acf::ByteMessageInfo::read_size_segment`] already
    /// carries.
    pub read_size: ReadSizeOrSegment,
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
//fusa:req REQ-UART-004
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
    //fusa:req REQ-UART-004
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
//fusa:req REQ-UART-005
//fusa:req REQ-UART-006
//fusa:req REQ-UART-007
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
//fusa:req REQ-UART-005
//fusa:req REQ-UART-006
//fusa:req REQ-UART-007
//fusa:req REQ-UART-008
pub fn resolve_uart_read_completion(
    rx: &UartRxQueueConfig,
    bytes_collected: u16,
    elapsed: u32,
) -> Option<UartReadCompletionReason> {
    let size_reached = bytes_collected >= rx.read_size.as_read_size();
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
//fusa:req REQ-UART-009
//fusa:req REQ-UART-010
pub fn validate_uart_read_request(payload: &[u8]) -> Result<(), RcpError> {
    if payload.is_empty() {
        Ok(())
    } else {
        Err(RcpError::UnsupportedCmd)
    }
}

// ── UartRequest: evt[2:0] request validation ─────────────────────────────────

/// The decoded shape of an incoming UART request, after validating its
/// `evt[2:0]` sub-opcode against TC18 §13.5 Table 33's Row-2 rule (UART is
/// one of that row's eight endpoint types —
/// `{ADC, PWM_IN, I²C, LIN, CAN, UART, ISELED, MDIO}`).
///
/// Unlike [`crate::i2c::I2cRequest`]/[`crate::lin::LinRequest`]/
/// [`crate::adc::AdcRequest`]/[`crate::pwm::PwmInRequest`]/
/// [`crate::can::CanRequest`], each of which model one unified request
/// shape for their endpoint type's single EP request storage,
/// [`UartRequest`]'s `evt[2:0] == 000b` case splits into two structurally
/// distinct variants — [`Write`](UartRequest::Write)/[`Read`](UartRequest::Read)
/// — honestly reflecting this module's own pre-existing
/// [`UartTxQueue`]/[`UartRxQueue`] two-EP-request-storage split (TC18
/// §13.7.8.1). See this module's doc comment "Provenance note: evt[2:0]
/// request validation" for the full citation, why
/// [`UartRequest::from_evt_sub_opcode`] takes an explicit `is_write: bool`
/// argument rather than guessing direction from `payload`, why it follows
/// [`crate::i2c::I2cRequest`]'s/[`crate::lin::LinRequest`]'s/
/// [`crate::adc::AdcRequest`]'s/[`crate::pwm::PwmInRequest`]'s own
/// `Ok(Self::ConfigWrite)` precedent rather than
/// [`crate::can::CanRequest`]'s departure from it, and
/// [`crate::evtgroup`]'s own doc comment for the literal-text discrepancy
/// this crate resolves `evt[2:0] == 000b` against.
#[derive(Debug, Clone, PartialEq, Eq)]
//fusa:req REQ-UART-011
pub enum UartRequest {
    /// `evt[2:0] == 000b`, `is_write == true`: an ordinary write request
    /// targeting the TX queue — `byte_msg_payload` is the bytes to
    /// transmit, decoded as a [`UartTxQueue`] per [`UartTxQueue::decode`].
    /// TC18 §13.7.8.1: "A write request leads to a transmission of data to
    /// an external connected device."
    Write(UartTxQueue),
    /// `evt[2:0] == 000b`, `is_write == false`: an ordinary read request
    /// targeting the RX queue. Carries no payload —
    /// [`UartRequest::from_evt_sub_opcode`] already enforces this module's
    /// own pre-existing [`validate_uart_read_request`] payload-less-read-only
    /// rule (TC18 §13.7.8.1: "A read request having a byte_msg_payload will
    /// be rejected with error code = UNKNOWN_CMD") before constructing this
    /// variant.
    Read,
    /// `evt[2:0] == 111b`: a functional-config write (TC18 §12.7.1) rather
    /// than an ordinary request on either queue. This crate does not yet
    /// decode the config-write payload shape itself — see this module's
    /// doc comment "Deliberately out of scope" — so a caller receiving this
    /// variant knows only that the request *is* a config-write, not its
    /// content. Unlike [`crate::can::CanRequest::ConfigWrite`], this
    /// variant *is* constructed by [`UartRequest::from_evt_sub_opcode`] —
    /// see this module's doc comment "Provenance note: evt[2:0] request
    /// validation" for why UART's own situation does not force CAN's same
    /// departure.
    ConfigWrite,
}

impl UartRequest {
    /// Decode an incoming UART request from its `evt.sub_opcode`
    /// ([`crate::acf::Evt::sub_opcode`]), which of UART's two independent
    /// EP request storages it targets (`is_write` — `true` for the TX
    /// queue/a write request, `false` for the RX queue/a read request,
    /// reusing [`crate::acf::ByteMessageInfo::op`]'s own true-is-write
    /// polarity per [`crate::authz::Policy`]'s identically-named
    /// convention), and its raw `byte_msg_payload` bytes.
    ///
    /// Returns `Err(`[`RcpError::UnsupportedCmd`]`)` for every
    /// [`EvtRow2Kind::Reserved`] sub_opcode value — TC18 §13.5 Table 33's
    /// Row-2 rule requires the request be rejected with error code
    /// `UNSUPPORTED_CMD`, matching
    /// [`crate::i2c::I2cRequest::from_evt_sub_opcode`]'s/
    /// [`crate::lin::LinRequest::from_evt_sub_opcode`]'s/
    /// [`crate::adc::AdcRequest::from_evt_sub_opcode`]'s/
    /// [`crate::pwm::PwmInRequest::from_evt_sub_opcode`]'s/
    /// [`crate::can::CanRequest::from_evt_sub_opcode`]'s identical refusal
    /// of their own table's reserved code. For an
    /// [`EvtRow2Kind::Plain`] sub_opcode, `is_write` selects between
    /// [`Write`](UartRequest::Write) (`payload` decoded as a
    /// [`UartTxQueue`], infallibly) and [`Read`](UartRequest::Read)
    /// (`payload` validated via [`validate_uart_read_request`], whose own
    /// `Err(`[`RcpError::UnsupportedCmd`]`)` for a non-empty payload is
    /// propagated unchanged). Returns `Ok(`[`UartRequest::ConfigWrite`]`)`
    /// for every [`EvtRow2Kind::ConfigWrite`] sub_opcode value, ignoring
    /// both `is_write` and `payload` — see this module's doc comment
    /// "Provenance note: evt[2:0] request validation" for why this follows
    /// [`crate::i2c::I2cRequest`]'s/[`crate::lin::LinRequest`]'s/
    /// [`crate::adc::AdcRequest`]'s/[`crate::pwm::PwmInRequest`]'s own
    /// precedent rather than [`crate::can::CanRequest`]'s departure from
    /// it. Never panics for any `sub_opcode`/`is_write`/`payload`
    /// combination.
    //fusa:req REQ-UART-011
    //fusa:req REQ-UART-012
    pub fn from_evt_sub_opcode(
        sub_opcode: u8,
        is_write: bool,
        payload: &[u8],
    ) -> Result<Self, RcpError> {
        match evt_row2_kind_of(sub_opcode) {
            EvtRow2Kind::Plain => {
                if is_write {
                    Ok(Self::Write(UartTxQueue::decode(payload)))
                } else {
                    validate_uart_read_request(payload)?;
                    Ok(Self::Read)
                }
            }
            EvtRow2Kind::ConfigWrite => Ok(Self::ConfigWrite),
            EvtRow2Kind::Reserved => Err(RcpError::UnsupportedCmd),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── UartTxQueue / UartRxQueue: round-trip / never-panic ─────────────────

    #[test]
    //fusa:test REQ-UART-001
    fn uart_tx_queue_round_trips_through_encode_decode() {
        for bytes in [vec![], vec![0x00], vec![0xAA; 3], (0u8..=255).collect()] {
            let queue = UartTxQueue {
                bytes: bytes.clone(),
            };
            assert_eq!(UartTxQueue::decode(&queue.encode()).bytes, bytes);
        }
    }

    #[test]
    //fusa:test REQ-UART-001
    fn uart_tx_queue_decode_never_panics_for_any_sampled_input() {
        for len in [0usize, 1, 2, 7, 64] {
            let buf = vec![0x5Au8; len];
            let _ = UartTxQueue::decode(&buf);
        }
    }

    #[test]
    //fusa:test REQ-UART-002
    fn uart_rx_queue_round_trips_through_encode_decode() {
        for bytes in [vec![], vec![0xFF], vec![0x01, 0x02, 0x03]] {
            let queue = UartRxQueue {
                bytes: bytes.clone(),
            };
            assert_eq!(UartRxQueue::decode(&queue.encode()).bytes, bytes);
        }
    }

    #[test]
    //fusa:test REQ-UART-002
    fn uart_rx_queue_decode_never_panics_for_any_sampled_input() {
        for len in [0usize, 1, 5, 32] {
            let buf = vec![0xA5u8; len];
            let _ = UartRxQueue::decode(&buf);
        }
    }

    // ── UartFunctionalConfig / layer_tag ─────────────────────────────────────

    #[test]
    //fusa:test REQ-UART-003
    fn uart_tx_and_rx_queue_configs_default_independently() {
        let config = UartFunctionalConfig::default();
        assert_eq!(config.tx, UartTxQueueConfig);
        assert_eq!(config.rx, UartRxQueueConfig::default());
        assert_eq!(config.rx.read_size, ReadSizeOrSegment::default());
        assert_eq!(config.rx.uart_timeout, 0);
    }

    #[test]
    //fusa:test REQ-UART-004
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
    //fusa:test REQ-UART-004
    fn uart_functional_config_layer_tag_rejects_mismatched_ep_type() {
        let functional = UartFunctionalConfig::default();
        let generic = crate::regmap::PerEpConfigBlock::new(crate::regmap::EndpointType::I2c);
        let tag = functional.layer_tag();
        assert!(!crate::regmap::functional_config_matches_ep_type(
            &generic, &tag
        ));
    }

    // ── resolve_uart_read_completion: the three-way race ────────────────────

    fn rx_config(read_size: u16, uart_timeout: u32) -> UartRxQueueConfig {
        UartRxQueueConfig {
            read_size: ReadSizeOrSegment(read_size),
            uart_timeout,
        }
    }

    #[test]
    //fusa:test REQ-UART-005
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
    //fusa:test REQ-UART-006
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
    //fusa:test REQ-UART-007
    fn resolve_uart_read_completion_reports_both_on_simultaneous_thresholds() {
        let rx = rx_config(10, 1000);
        assert_eq!(
            resolve_uart_read_completion(&rx, 10, 1000),
            Some(UartReadCompletionReason::Both)
        );
    }

    #[test]
    //fusa:test REQ-UART-007
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
    //fusa:test REQ-UART-008
    fn resolve_uart_read_completion_returns_none_before_either_threshold() {
        let rx = rx_config(10, 1000);
        assert_eq!(resolve_uart_read_completion(&rx, 9, 999), None);
        // A genuinely in-progress read: some bytes in, some time elapsed,
        // neither threshold met.
        assert_eq!(resolve_uart_read_completion(&rx, 3, 200), None);
    }

    #[test]
    //fusa:test REQ-UART-008
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
    //fusa:test REQ-UART-009
    fn validate_uart_read_request_accepts_empty_payload() {
        assert_eq!(validate_uart_read_request(&[]), Ok(()));
    }

    #[test]
    //fusa:test REQ-UART-010
    fn validate_uart_read_request_rejects_any_non_empty_payload() {
        for payload in [vec![0x00], vec![0x01, 0x02], vec![0xFF; 16]] {
            assert_eq!(
                validate_uart_read_request(&payload),
                Err(RcpError::UnsupportedCmd)
            );
        }
    }

    #[test]
    //fusa:test REQ-UART-010
    fn validate_uart_read_request_never_panics_for_any_sampled_input() {
        for len in [0usize, 1, 2, 8, 64] {
            let buf = vec![0x5Au8; len];
            let _ = validate_uart_read_request(&buf);
        }
    }

    // ── UartRequest::from_evt_sub_opcode ─────────────────────────────────────

    #[test]
    //fusa:test REQ-UART-011
    //fusa:test REQ-UART-012
    fn uart_request_plain_write_decodes_payload_as_tx_queue() {
        // TC18 §13.7.8.3: "The byte_msg_payload in the request is the UART
        // payload."
        let payload = [0x01, 0x02, 0x03, 0x04];
        let request = UartRequest::from_evt_sub_opcode(0b000, true, &payload).unwrap();
        assert_eq!(
            request,
            UartRequest::Write(UartTxQueue {
                bytes: payload.to_vec()
            })
        );
    }

    #[test]
    //fusa:test REQ-UART-011
    //fusa:test REQ-UART-012
    fn uart_request_plain_write_accepts_an_empty_payload() {
        let request = UartRequest::from_evt_sub_opcode(0b000, true, &[]).unwrap();
        assert_eq!(request, UartRequest::Write(UartTxQueue::default()));
    }

    #[test]
    //fusa:test REQ-UART-011
    //fusa:test REQ-UART-012
    fn uart_request_plain_read_accepts_an_empty_payload() {
        let request = UartRequest::from_evt_sub_opcode(0b000, false, &[]).unwrap();
        assert_eq!(request, UartRequest::Read);
    }

    #[test]
    //fusa:test REQ-UART-011
    //fusa:test REQ-UART-012
    fn uart_request_plain_read_rejects_a_non_empty_payload() {
        // TC18 §13.7.8.1: "A read request having a byte_msg_payload will be
        // rejected with error code = UNKNOWN_CMD" -- UartRequest re-enforces
        // validate_uart_read_request's own pre-existing rule rather than
        // silently accepting a payload on the RX-queue side.
        for payload in [&[0x00][..], &[0x01, 0x02], &[0xFF; 16]] {
            assert_eq!(
                UartRequest::from_evt_sub_opcode(0b000, false, payload),
                Err(RcpError::UnsupportedCmd)
            );
        }
    }

    #[test]
    //fusa:test REQ-UART-011
    //fusa:test REQ-UART-012
    fn uart_request_config_write_evt_is_recognized_without_interpreting_payload_or_direction() {
        // Unlike CanRequest::ConfigWrite (which UartRequest::ConfigWrite
        // deliberately does not mirror -- see this module's doc comment
        // "Provenance note: evt[2:0] request validation"), UartRequest's own
        // ConfigWrite arm is constructed regardless of is_write, and the
        // payload is not decoded as a UartTxQueue at all -- the variant
        // carries no payload, so garbage bytes here cannot be silently
        // misread as a transfer.
        for is_write in [true, false] {
            let request =
                UartRequest::from_evt_sub_opcode(0b111, is_write, &[0xDE, 0xAD, 0xBE, 0xEF])
                    .unwrap();
            assert_eq!(request, UartRequest::ConfigWrite);
        }
    }

    #[test]
    //fusa:test REQ-UART-012
    fn uart_request_reserved_evt_values_are_rejected_with_unsupported_cmd() {
        for sub_opcode in 0b001..=0b110u8 {
            for is_write in [true, false] {
                assert_eq!(
                    UartRequest::from_evt_sub_opcode(sub_opcode, is_write, &[]),
                    Err(RcpError::UnsupportedCmd)
                );
                assert_eq!(
                    UartRequest::from_evt_sub_opcode(sub_opcode, is_write, &[1, 2, 3]),
                    Err(RcpError::UnsupportedCmd)
                );
            }
        }
    }

    #[test]
    //fusa:test REQ-UART-012
    fn uart_request_values_above_the_3_bit_field_are_also_rejected_with_unsupported_cmd() {
        for sub_opcode in (crate::acf::EVT_SUB_OPCODE_MAX + 1)..=u8::MAX {
            assert_eq!(
                UartRequest::from_evt_sub_opcode(sub_opcode, true, &[]),
                Err(RcpError::UnsupportedCmd)
            );
            assert_eq!(
                UartRequest::from_evt_sub_opcode(sub_opcode, false, &[]),
                Err(RcpError::UnsupportedCmd)
            );
        }
    }

    #[test]
    //fusa:test REQ-UART-012
    fn uart_request_from_evt_sub_opcode_never_panics_for_any_sampled_input() {
        let payloads: [&[u8]; 3] = [&[], &[0x00], &[0xAA; 32]];
        for sub_opcode in 0..=u8::MAX {
            for is_write in [true, false] {
                for payload in payloads {
                    let _ = UartRequest::from_evt_sub_opcode(sub_opcode, is_write, payload);
                }
            }
        }
    }
}
