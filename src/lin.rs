//fusa:req REQ-LIN-001
//fusa:req REQ-LIN-002
//fusa:req REQ-LIN-003
//fusa:req REQ-LIN-004
//fusa:req REQ-LIN-005
//fusa:req REQ-LIN-006
//fusa:req REQ-LIN-008
//fusa:req REQ-LIN-009

//! The LIN commander endpoint type (`ep_type 0x06`) — `ROADMAP.md`
//! Milestone 7 ("Remaining Endpoint Types"), opening checklist bullet: raw
//! byte pass-through only, with no PID, checksum, or schedule-table
//! intelligence performed at the protocol level, and that responsibility
//! pushed entirely onto the client.
//!
//! This is this crate's first Milestone 7 entry, following the same
//! "additive standalone plumbing only" discipline Milestone 4's six
//! endpoint-type modules ([`crate::gpio`], [`crate::spi`], [`crate::i2c`],
//! [`crate::uart`], [`crate::adc`], [`crate::pwm`]) already established.
//! Three named pieces are in scope, all implemented here:
//!
//! - [`LinFrameTransfer`] / [`LinFrameTransferResult`] — the raw PID+data
//!   bytes a LIN commander request sends onto the bus, and the raw data
//!   bytes a response returns from it, modeled as opaque byte content this
//!   module does not interpret, matching
//!   [`crate::spi::SpiByteTransfer`]/[`crate::i2c::I2cByteTransfer`]'s own
//!   "raw pass-through, no parsing" discipline. See "Provenance note: PID
//!   and checksum are entirely client-owned bytes" below for why this
//!   module computes neither.
//! - [`LinFunctionalConfig`] — this endpoint type's functional-config
//!   content. See "Relationship to `crate::regmap`" below.
//! - [`LinRequest`]/[`LinRequest::from_evt_sub_opcode`] — LIN's own
//!   request-decode entry point, validating an incoming request's
//!   `evt.sub_opcode` against [`crate::evtgroup::evt_row2_kind_of`]'s TC18
//!   §13.5 Table 33 Row-2 rule. See "Provenance note: evt[2:0] request
//!   validation" below — this piece was added after this module's own
//!   original scope note above and below (which still accurately describe
//!   why no `sub_opcode` reading existed here originally, before this
//!   addition) as this crate's fourth Row-2 endpoint-type module, following
//!   [`crate::i2c::I2cRequest`]/[`crate::i2c::I2cRequest::from_evt_sub_opcode`]'s
//!   pilot pattern,
//!   [`crate::adc::AdcRequest`]/[`crate::adc::AdcRequest::from_evt_sub_opcode`]'s
//!   second application of it, and
//!   [`crate::pwm::PwmInRequest`]/[`crate::pwm::PwmInRequest::from_evt_sub_opcode`]'s
//!   third. The remaining four Row-2 endpoint types (`CAN, UART, ISELED,
//!   MDIO`) are expected to follow the same pattern in their own later
//!   items.
//!
//! Deliberately out of scope, for the same reasons every prior Milestone 4
//! entry's own doc comment already gives:
//!
//! - Any PID computation, checksum computation, or LIN schedule-table
//!   management. `ROADMAP.md`'s LIN commander checklist bullet states the
//!   spec itself defines none of this at the protocol level, so this module
//!   builds none of it — see "Validation against `linbr.rs`" below.
//! - The "Groups A/B/C" `evt[2:0]` sub-opcode convention
//!   ([`crate::evtgroup::EvtGroup`]) as a general, cross-endpoint-type
//!   classification scheme — [`crate::evtgroup`]'s own doc comment already
//!   flags that broader scheme as unresolved, independent of the narrower,
//!   unambiguous Table 33 Row-2 rule this module's [`LinRequest`] now
//!   implements (see "Provenance note: evt[2:0] request validation" below).
//!   `ROADMAP.md`'s LIN commander checklist bullet itself names no
//!   `sub_opcode`-keyed selection mechanism of its own (unlike
//!   [`crate::gpio`]'s write-semantics selection or [`crate::spi`]'s
//!   up-to-6 channel selection) — the Row-2 rule [`LinRequest`] implements
//!   comes from TC18 §13.5 Table 33, a separate, later-discovered item, not
//!   from this checklist bullet.
//! - [`crate::regmap::CommonFunctionalConfig`]'s fields — unchanged here, as
//!   in every prior Milestone 1-4 entry.
//! - Decoding [`LinRequest::ConfigWrite`]'s own TC18 §12.7.1 payload shape.
//!   [`LinRequest::from_evt_sub_opcode`] recognizes a config-write request
//!   as distinct from a [`Plain`](LinRequest::Plain) one, but does not
//!   itself interpret what the config-write payload contains — that is
//!   separate, later work, same as every Row-2 endpoint-type module this
//!   predicate lands in.
//! - Wiring [`LinRequest::from_evt_sub_opcode`] into an actual decoder,
//!   dispatch loop, or [`crate::mock::Endpoint`] implementation.
//!   [`crate::mock::Endpoint`]'s own trait signature still does not carry
//!   an `evt` value to any implementation at all — that gap is not
//!   specific to LIN, it applies identically to
//!   [`crate::i2c::I2cRequest::from_evt_sub_opcode`]/
//!   [`crate::adc::AdcRequest::from_evt_sub_opcode`]/
//!   [`crate::pwm::PwmInRequest::from_evt_sub_opcode`] (each confirmed
//!   still unwired against [`crate::mock::Endpoint`]'s own doc comment).
//!   [`LinRequest`] is built to that same "additive standalone plumbing
//!   only" level.
//! - Wiring any of the below into an actual decoder, dispatch loop, or
//!   [`crate::avtp`]/[`crate::acf`]/[`crate::addressing`] caller, and —
//!   distinct from every Milestone 4 entry — touching the legacy `linbr.rs`
//!   bridge itself while it still existed. Its REPLACE-disposition cutover
//!   (deletion/rebuild against the new core) was `ROADMAP.md` Milestone 9's
//!   job, not this module's; see "Validation against `linbr.rs`
//!   (historical — see below for its outcome)" below.
//!
//! ## Provenance note: evt[2:0] request validation
//!
//! LIN is one of the eight endpoint types TC18 §13.5 Table 33 groups into
//! one shared "Row 2" `evt[2:0]` rule — see [`crate::evtgroup`]'s own doc
//! comment "Provenance note: TC18 §13.5 Table 33's Row-2 rule
//! (`evt_row2_kind_of`)" for the full citation, including the literal-text
//! discrepancy that module's doc comment flags and resolves (Table 33's own
//! printed Row-2 cell reads "000b to 110b reserved", including 000b, which
//! this crate does not implement literally).
//! [`LinRequest::from_evt_sub_opcode`] is this module's own caller of that
//! shared [`crate::evtgroup::evt_row2_kind_of`] predicate.
//!
//! Unlike [`crate::adc::AdcRequest::Plain`] (no payload struct at all,
//! since TC18 §13.7.9.3 states the ADC request itself has none) and
//! [`crate::pwm::PwmInRequest::Plain`] (a raw, uninterpreted byte transfer,
//! since PWM_IN's own request-side payload framing is unconfirmed —
//! `pwm.rs`'s own "Provenance note: field widths and units" flags this),
//! [`LinRequest::Plain`] (`evt[2:0] == 000b`) decodes its payload through
//! this module's own pre-existing, already-confirmed
//! [`LinFrameTransfer::decode`]: TC18 §13.7.10.1 states the LIN EP "sends
//! the bytes provided by the RC Client in the byte_msg_payload on the bus"
//! — an ordinary request genuinely carries a payload — and this module's
//! own "TC18 reconciliation note (§13.7.10)" below already established that
//! payload's byte layout without any open question left to bridge: `pid`
//! followed by `data`, byte-for-byte identical to TC18 §13.7.10.3's own
//! Figure 39 wire example. There is nothing left to guess by reusing that
//! existing decode here, unlike PWM_IN's still-open request-payload
//! question — this is a difference in what was already confirmed before
//! this item, not a new per-endpoint-type rule invented for it.
//!
//! One consequence of reusing [`LinFrameTransfer::decode`] rather than an
//! infallible byte-transfer decode (as
//! [`crate::i2c::I2cByteTransfer::decode`]/
//! [`crate::pwm::PwmInByteTransfer::decode`] both are) is that
//! [`LinRequest::from_evt_sub_opcode`] can itself fail on a
//! [`Plain`](LinRequest::Plain) request: an empty payload returns
//! `Err(`[`RcpError::ShortFrame`]`)` (no PID byte present — TC18 §13.7.10.1's
//! "sends the bytes provided ... in the byte_msg_payload" wording describes
//! an ordinary request as genuinely carrying bytes, so this module does not
//! treat an empty payload as a degenerate but valid zero-byte transfer the
//! way [`crate::i2c::I2cByteTransfer::decode`] treats an empty I²C
//! payload), and a payload whose data portion exceeds [`LIN_MAX_DATA`]
//! bytes returns `Err(`[`RcpError::PayloadTooLarge`]`)`. Both outcomes
//! match [`LinFrameTransfer::decode`]'s own pre-existing, already-tested
//! behavior rather than this item inventing a new failure mode for the same
//! bytes. Every `Reserved` sub_opcode is rejected with
//! `Err(`[`RcpError::UnsupportedCmd`]`)`, matching Table 33's own stated
//! error code and
//! [`crate::i2c::I2cRequest::from_evt_sub_opcode`]'s/
//! [`crate::adc::AdcRequest::from_evt_sub_opcode`]'s/
//! [`crate::pwm::PwmInRequest::from_evt_sub_opcode`]'s identical refusal of
//! their own table's reserved code.
//!
//! ## Validation against `linbr.rs` (historical — see below for its outcome)
//!
//! Per this milestone's checklist bullet's own instruction, this module was
//! written only after reading the legacy `linbr::LinBridge::send`'s existing
//! PID derivation
//! (`(self.zone.0 << 2) | (cmd.cmd_type.0 as u8 & 0x03)`) and confirming it
//! is not reusable logic: that formula derives a PID from the old
//! `Zone`/`Command` model's zone index and command-type discriminant, both
//! of which have no equivalent in the endpoint-addressed model this crate
//! is replacing them with, and — independent of that mismatch — deriving a
//! PID from anything other than a value the client itself supplies
//! contradicts the raw-byte-pass-through behavior this checklist bullet
//! describes: a real LIN commander must be free to place whatever PID
//! (parity bits included) and whatever data the client computed onto the
//! bus, not have the RC Server re-derive one from its own state. Nor is
//! `linbr::LinBridge::send`'s response-status inference (treating the
//! response's first returned byte as an implicit OK/ERROR flag) reused
//! here — that is itself a piece of protocol-level interpretation this
//! checklist bullet's pass-through behavior does not call for.
//! `linbr::LIN_MAX_DATA` was reused at that time, but only as the plain
//! physical fact it stated (LIN 2.x's real per-frame data ceiling).
//!
//! Milestone 9's own `linbr` REPLACE cutover has since deleted `linbr.rs`
//! outright (its `LinBridge`/`LinMaster`/`Zone`-keyed PID scheme had no
//! surviving analog in this endpoint-addressed model, matching the
//! `canbr` REPLACE cutover immediately before it), leaving [`LIN_MAX_DATA`]
//! below as this crate's one live external caller of the deleted module.
//! Per Guiding Principle 5, that cross-module dependency is resolved by
//! inlining the physical-fact literal directly here rather than leaving a
//! stub module behind purely to hold one constant — the same resolution
//! `can.rs`'s own `CAN_FD_MAX_PAYLOAD` used when `canbr.rs` was deleted.
//!
//! ## TC18 reconciliation note (§13.7.10)
//!
//! TC18 §13.7.10.3 (TC18.txt line 5304) states only that "the Byte Msg
//! Payload is the payload to be used on the Lin bus", and Figure 38 shows
//! that payload as one undifferentiated "Lin payload" field followed by
//! padding — it defines no PID sub-field, no checksum sub-field, and no
//! per-frame length ceiling of its own. This module's split of the leading
//! byte into [`LinFrameTransfer::pid`] is therefore a crate-local modeling
//! convenience that changes no wire byte: `pid` followed by `data` is the
//! same byte sequence, in the same order, that TC18 calls the Lin payload.
//! The [`LIN_MAX_DATA`] ceiling is likewise this crate's own (real LIN 2.x)
//! physical fact, not a TC18 clause.
//!
//! Three normative §13.7.10.1 behaviors are **not** implemented here and are
//! recorded as explicit not-implemented requirement entries rather than
//! silently omitted: matching each received LIN message against the pending
//! read request's `byte_msg_payload` under the conditions given by
//! `evt[2:0]` and replying when `op = 0` (TC18.txt lines 5276-5277); issuing
//! a trigger once a transmission has been finalized and the configured
//! trailing time has expired (line 5278); and the cyclic-transmission
//! pattern built from a repeated trigger request on the endpoint's own
//! trigger (line 5279). All three are RC-Server run-time endpoint behaviors,
//! outside this module's codec-only scope. TC18 Table 52's own
//! functional-config register layout (§13.7.10.2, lines 5287-5298) is
//! likewise unimplemented — see [`LinFunctionalConfig`].
//!
//! ## Relationship to [`crate::regmap`]
//!
//! As with every Milestone 4 endpoint-type module, LIN's real
//! functional-config content gets its own dedicated type,
//! [`LinFunctionalConfig`], rather than adding LIN-specific fields directly
//! onto the still-shared, thirteen-endpoint-type
//! [`crate::regmap::PerEpTypeFunctionalConfig`] placeholder.
//! [`LinFunctionalConfig::layer_tag`] shows how a caller obtains the
//! matching generic-layer tag so the two compose through
//! [`crate::regmap::check_functional_config_matches_ep_type`] exactly as
//! that cross-layer rule already expects, without this module editing
//! [`crate::regmap`] itself.
//!
//! `ROADMAP.md`'s LIN commander checklist bullet names no functional-config
//! content beyond raw byte pass-through — no channel selection, no speed
//! preset, nothing comparable to [`crate::spi::SpiFunctionalConfig`]'s
//! channel slots or [`crate::i2c::I2cFunctionalConfig`]'s speed mode. Per
//! Guiding Principle 5, [`LinFunctionalConfig`] is therefore left an
//! intentionally empty placeholder, the same discipline
//! [`crate::spi::SpiChannelConfigSlot`] already applies to its own
//! still-unnamed per-channel fields, rather than this crate guessing at
//! plausible LIN configuration content on its own.
//!
//! ## Provenance note: PID and checksum are entirely client-owned bytes
//!
//! `ROADMAP.md`'s LIN commander checklist bullet states the spec defines no
//! PID/checksum/schedule-table intelligence at the protocol level and
//! expects that responsibility pushed to the client, without itself stating
//! the exact byte layout an RCP request/response carries. Per Guiding
//! Principle 5, this module's working interpretation is: a request carries
//! the client-computed PID as one leading byte followed by the frame's data
//! bytes ([`LinFrameTransfer::pid`] / [`LinFrameTransfer::data`]), and a
//! response carries only the data bytes actually read back off the bus
//! ([`LinFrameTransferResult::data`]) — no PID field, since a LIN response
//! frame is identified by the request's own PID, not a second one. Neither
//! field is validated for parity, and `data` is not scanned for a trailing
//! checksum byte the client may or may not have included in it — this
//! module carries both PID and data exactly as supplied, unparsed, matching
//! [`crate::spi::SpiByteTransfer`]'s own refusal to interpret its PICO
//! bytes. The one constraint this module does enforce —
//! [`LIN_MAX_DATA`]-byte data ceiling, returning
//! `Err(RcpError::PayloadTooLarge)` above it — is not new protocol-level
//! smarts; it is the same real LIN 2.x physical ceiling the legacy
//! `linbr::LinBridge::send` already enforced, stated here as this module's
//! own fact about the bus rather than re-derived.

use crate::evtgroup::{evt_row2_kind_of, EvtRow2Kind};
use crate::RcpError;

// ── LIN_MAX_DATA ──────────────────────────────────────────────────────────────

/// Maximum LIN frame data length — a genuine physical ceiling of the LIN
/// 2.x bus, not a spec-defined or otherwise interpreted value. Originally
/// reused from the legacy `linbr::LIN_MAX_DATA` (see this module's doc
/// comment "Validation against `linbr.rs` (historical — see below for its
/// outcome)"); stated directly as this module's own constant since
/// Milestone 9's `linbr` REPLACE cutover deleted that module.
//fusa:req REQ-LIN-001
pub const LIN_MAX_DATA: usize = 8;

// ── LinFrameTransfer ─────────────────────────────────────────────────────────

/// A raw LIN commander frame transfer: the client-computed PID byte plus
/// the frame's data bytes, sent from RC Server onto the LIN bus.
///
/// See this module's doc comment "Provenance note: PID and checksum are
/// entirely client-owned bytes" for this type's wire-layout interpretation
/// and for why neither field is parsed or validated beyond the
/// [`LIN_MAX_DATA`] data-length ceiling.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
//fusa:req REQ-LIN-002
pub struct LinFrameTransfer {
    /// The client-computed PID byte, carried unparsed and unvalidated —
    /// this module performs no parity check or PID derivation of its own.
    pub pid: u8,
    /// The frame's data bytes, carried unparsed — may or may not include a
    /// trailing checksum byte the client computed; this module does not
    /// distinguish either way. Never longer than [`LIN_MAX_DATA`] bytes.
    pub data: Vec<u8>,
}

impl LinFrameTransfer {
    /// Encode this transfer to its raw wire representation: the PID byte
    /// followed by `data`, unmodified and unframed.
    ///
    /// This is the `byte_msg_payload` TC18 §13.7.10.3 (TC18.txt line 5304)
    /// calls "the payload to be used on the Lin bus": the bytes are emitted
    /// verbatim, in supplied order, with nothing inserted, removed, or
    /// reordered — see this module's doc comment "TC18 reconciliation note".
    //fusa:req REQ-LIN-002
    //fusa:req REQ-LIN-007
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(1 + self.data.len());
        buf.push(self.pid);
        buf.extend_from_slice(&self.data);
        buf
    }

    /// Decode a [`LinFrameTransfer`] from a byte slice: the first byte is
    /// `pid`, the remainder is `data`.
    ///
    /// Returns `Err(RcpError::ShortFrame)` for an empty slice (no PID byte
    /// present), matching [`crate::spi::SpiStatus::decode`]'s own
    /// too-short-input handling. Returns `Err(RcpError::PayloadTooLarge)`
    /// when the remaining data would exceed [`LIN_MAX_DATA`] bytes, the same
    /// error variant the legacy `linbr::LinBridge::send` already used for
    /// the same physical ceiling. Never panics for any input.
    //fusa:req REQ-LIN-003
    //fusa:req REQ-LIN-004
    pub fn decode(b: &[u8]) -> Result<Self, RcpError> {
        let (pid, data) = b.split_first().ok_or(RcpError::ShortFrame)?;
        if data.len() > LIN_MAX_DATA {
            return Err(RcpError::PayloadTooLarge);
        }
        Ok(Self {
            pid: *pid,
            data: data.to_vec(),
        })
    }
}

/// A raw LIN commander frame transfer result: the data bytes a response
/// returns from the LIN bus back to the RC Server.
///
/// See [`LinFrameTransfer`]'s doc comment — this carries only `data`, since
/// a LIN response frame is identified by the request's own PID rather than
/// carrying a second one of its own.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
//fusa:req REQ-LIN-005
pub struct LinFrameTransferResult {
    /// The data bytes read back off the bus, carried unparsed. Never longer
    /// than [`LIN_MAX_DATA`] bytes.
    pub data: Vec<u8>,
}

impl LinFrameTransferResult {
    /// Encode this transfer result to its raw wire representation: `data`,
    /// unmodified and unframed.
    //fusa:req REQ-LIN-005
    pub fn encode(&self) -> Vec<u8> {
        self.data.clone()
    }

    /// Decode a [`LinFrameTransferResult`] from a byte slice.
    ///
    /// Returns `Err(RcpError::PayloadTooLarge)` for input longer than
    /// [`LIN_MAX_DATA`] bytes; an empty slice is a valid (zero-length) LIN
    /// response and decodes successfully. Never panics for any input.
    //fusa:req REQ-LIN-005
    pub fn decode(b: &[u8]) -> Result<Self, RcpError> {
        if b.len() > LIN_MAX_DATA {
            return Err(RcpError::PayloadTooLarge);
        }
        Ok(Self { data: b.to_vec() })
    }
}

// ── LinFunctionalConfig ──────────────────────────────────────────────────────

/// LIN commander's own per-EP-type functional-config content.
///
/// An intentionally empty placeholder — see this module's doc comment
/// "Relationship to `crate::regmap`" for why the checklist text names no
/// LIN-specific configuration content to model here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-LIN-006
pub struct LinFunctionalConfig;

impl LinFunctionalConfig {
    /// The [`crate::regmap::PerEpTypeFunctionalConfig`] generic-layer tag
    /// that matches this LIN functional config, for use with
    /// [`crate::regmap::check_functional_config_matches_ep_type`].
    ///
    /// This module does not itself call that function — it only shows how a
    /// caller would obtain the matching tag, per this module's doc comment
    /// "Relationship to `crate::regmap`".
    //fusa:req REQ-LIN-006
    pub fn layer_tag(&self) -> crate::regmap::PerEpTypeFunctionalConfig {
        crate::regmap::PerEpTypeFunctionalConfig::new(crate::regmap::EndpointType::Lin)
    }
}

// ── LinRequest: evt[2:0] request validation ─────────────────────────────────

/// The decoded shape of an incoming LIN commander request, after validating
/// its `evt[2:0]` sub-opcode against TC18 §13.5 Table 33's Row-2 rule (LIN
/// is one of that row's eight endpoint types —
/// `{ADC, PWM_IN, I²C, LIN, CAN, UART, ISELED, MDIO}`).
///
/// See this module's doc comment "Provenance note: evt[2:0] request
/// validation" for the full citation, why [`LinRequest::Plain`] reuses
/// [`LinFrameTransfer::decode`] rather than either an infallible raw byte
/// transfer or no payload at all, and [`crate::evtgroup`]'s own doc comment
/// for the literal-text discrepancy this crate resolves `evt[2:0] == 000b`
/// against.
#[derive(Debug, Clone, PartialEq, Eq)]
//fusa:req REQ-LIN-008
pub enum LinRequest {
    /// `evt[2:0] == 000b`: an ordinary LIN commander request —
    /// `byte_msg_payload` is this frame's PID+data bytes, decoded as a
    /// [`LinFrameTransfer`] per [`LinFrameTransfer::decode`].
    Plain(LinFrameTransfer),
    /// `evt[2:0] == 111b`: a functional-config write (TC18 §12.7.1) rather
    /// than an ordinary request. This crate does not yet decode the
    /// config-write payload shape itself — see this module's doc comment
    /// "Deliberately out of scope" — so a caller receiving this variant
    /// knows only that the request *is* a config-write, not its content.
    ConfigWrite,
}

impl LinRequest {
    /// Decode an incoming LIN request from its `evt.sub_opcode`
    /// ([`crate::acf::Evt::sub_opcode`]) and raw `byte_msg_payload` bytes.
    ///
    /// Returns `Err(`[`RcpError::UnsupportedCmd`]`)` for every
    /// [`EvtRow2Kind::Reserved`] sub_opcode value — TC18 §13.5 Table 33's
    /// Row-2 rule requires the request be rejected with error code
    /// `UNSUPPORTED_CMD`, matching
    /// [`crate::i2c::I2cRequest::from_evt_sub_opcode`]'s/
    /// [`crate::adc::AdcRequest::from_evt_sub_opcode`]'s/
    /// [`crate::pwm::PwmInRequest::from_evt_sub_opcode`]'s identical refusal
    /// of their own table's reserved code. A [`Plain`](LinRequest::Plain)
    /// request additionally propagates [`LinFrameTransfer::decode`]'s own
    /// `Result` — `Err(`[`RcpError::ShortFrame`]`)` for an empty payload,
    /// `Err(`[`RcpError::PayloadTooLarge`]`)` for data beyond
    /// [`LIN_MAX_DATA`] bytes — rather than this function inventing a
    /// second, different validation of the same bytes; see this module's
    /// doc comment "Provenance note: evt[2:0] request validation" for why.
    /// Never panics for any `sub_opcode`/`payload` combination.
    //fusa:req REQ-LIN-008
    //fusa:req REQ-LIN-009
    pub fn from_evt_sub_opcode(sub_opcode: u8, payload: &[u8]) -> Result<Self, RcpError> {
        match evt_row2_kind_of(sub_opcode) {
            EvtRow2Kind::Plain => Ok(Self::Plain(LinFrameTransfer::decode(payload)?)),
            EvtRow2Kind::ConfigWrite => Ok(Self::ConfigWrite),
            EvtRow2Kind::Reserved => Err(RcpError::UnsupportedCmd),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── LIN_MAX_DATA ─────────────────────────────────────────────────────────

    #[test]
    //fusa:test REQ-LIN-001
    fn lin_max_data_is_eight() {
        assert_eq!(LIN_MAX_DATA, 8);
    }

    // ── LinFrameTransfer: round-trip / never-panic ──────────────────────────

    #[test]
    //fusa:test REQ-LIN-002
    fn lin_frame_transfer_round_trips_through_encode_decode() {
        for (pid, data) in [
            (0x00u8, vec![]),
            (0x53, vec![0x00]),
            (0xFF, vec![0xAA; 3]),
            (0x12, (0u8..8).collect::<Vec<_>>()),
        ] {
            let transfer = LinFrameTransfer {
                pid,
                data: data.clone(),
            };
            let decoded = LinFrameTransfer::decode(&transfer.encode()).unwrap();
            assert_eq!(decoded.pid, pid);
            assert_eq!(decoded.data, data);
        }
    }

    #[test]
    //fusa:test REQ-LIN-007
    fn lin_byte_msg_payload_is_carried_verbatim_onto_the_bus() {
        // TC18 §13.7.10.3 (TC18.txt line 5304): "The Byte Msg Payload is the
        // payload to be used on the Lin bus." Figure 38's own on-wire example
        // (line 5305) carries three payload bytes with no PID/checksum
        // sub-structure and no length/format byte of its own, so the encoded
        // form must be byte-for-byte identical to the supplied payload.
        //
        // Literal payload: LIN 2.x diagnostic master-request frame identifier
        // 0x3C followed by three data bytes.
        const PAYLOAD: [u8; 4] = [0x3C, 0x01, 0x02, 0x03];

        let transfer = LinFrameTransfer {
            pid: PAYLOAD[0],
            data: PAYLOAD[1..].to_vec(),
        };
        assert_eq!(transfer.encode(), PAYLOAD.to_vec());

        let decoded = LinFrameTransfer::decode(&PAYLOAD).unwrap();
        assert_eq!(decoded.pid, 0x3C);
        assert_eq!(decoded.data, vec![0x01, 0x02, 0x03]);

        // Nothing is appended (no checksum byte is synthesised) even at the
        // full LIN 2.x data ceiling: 1 payload-leading byte + 8 data bytes.
        let full = LinFrameTransfer {
            pid: 0x3C,
            data: vec![0xA5; LIN_MAX_DATA],
        };
        assert_eq!(full.encode().len(), 9);
        assert_eq!(&full.encode()[1..], &[0xA5u8; LIN_MAX_DATA][..]);
    }

    #[test]
    //fusa:test REQ-LIN-003
    fn lin_frame_transfer_decode_rejects_empty_input() {
        assert_eq!(LinFrameTransfer::decode(&[]), Err(RcpError::ShortFrame));
    }

    #[test]
    //fusa:test REQ-LIN-004
    fn lin_frame_transfer_decode_rejects_data_longer_than_lin_max_data() {
        let mut buf = vec![0x00u8]; // pid
        buf.extend(vec![0xAAu8; LIN_MAX_DATA + 1]);
        assert_eq!(
            LinFrameTransfer::decode(&buf),
            Err(RcpError::PayloadTooLarge)
        );
    }

    #[test]
    //fusa:test REQ-LIN-004
    fn lin_frame_transfer_decode_accepts_data_at_exactly_lin_max_data() {
        let mut buf = vec![0x00u8]; // pid
        buf.extend(vec![0xAAu8; LIN_MAX_DATA]);
        let decoded = LinFrameTransfer::decode(&buf).unwrap();
        assert_eq!(decoded.data.len(), LIN_MAX_DATA);
    }

    #[test]
    //fusa:test REQ-LIN-002
    fn lin_frame_transfer_decode_never_panics_for_any_sampled_input() {
        for len in [0usize, 1, 2, 9, 64] {
            let buf = vec![0x5Au8; len];
            let _ = LinFrameTransfer::decode(&buf);
        }
    }

    // ── LinFrameTransferResult: round-trip / never-panic ────────────────────

    #[test]
    //fusa:test REQ-LIN-005
    fn lin_frame_transfer_result_round_trips_through_encode_decode() {
        for data in [vec![], vec![0xFF], vec![0x01, 0x02, 0x03]] {
            let result = LinFrameTransferResult { data: data.clone() };
            assert_eq!(
                LinFrameTransferResult::decode(&result.encode())
                    .unwrap()
                    .data,
                data
            );
        }
    }

    #[test]
    //fusa:test REQ-LIN-005
    fn lin_frame_transfer_result_decode_rejects_longer_than_lin_max_data() {
        let buf = vec![0xAAu8; LIN_MAX_DATA + 1];
        assert_eq!(
            LinFrameTransferResult::decode(&buf),
            Err(RcpError::PayloadTooLarge)
        );
    }

    #[test]
    //fusa:test REQ-LIN-005
    fn lin_frame_transfer_result_decode_accepts_exactly_lin_max_data() {
        let buf = vec![0xAAu8; LIN_MAX_DATA];
        assert_eq!(
            LinFrameTransferResult::decode(&buf).unwrap().data.len(),
            LIN_MAX_DATA
        );
    }

    #[test]
    //fusa:test REQ-LIN-005
    fn lin_frame_transfer_result_decode_never_panics_for_any_sampled_input() {
        for len in [0usize, 1, 5, 32] {
            let buf = vec![0xA5u8; len];
            let _ = LinFrameTransferResult::decode(&buf);
        }
    }

    // ── LinFunctionalConfig / layer_tag ─────────────────────────────────────

    #[test]
    //fusa:test REQ-LIN-006
    fn lin_functional_config_layer_tag_matches_ep_type_lin() {
        let functional = LinFunctionalConfig;
        let generic = crate::regmap::PerEpConfigBlock::new(crate::regmap::EndpointType::Lin);
        let tag = functional.layer_tag();
        assert_eq!(tag.ep_type, crate::regmap::EndpointType::Lin);
        assert!(crate::regmap::functional_config_matches_ep_type(
            &generic, &tag
        ));
        assert_eq!(
            crate::regmap::check_functional_config_matches_ep_type(&generic, &tag),
            Ok(())
        );
    }

    #[test]
    //fusa:test REQ-LIN-006
    fn lin_functional_config_layer_tag_rejects_mismatched_ep_type() {
        let functional = LinFunctionalConfig;
        let generic = crate::regmap::PerEpConfigBlock::new(crate::regmap::EndpointType::Spi);
        let tag = functional.layer_tag();
        assert!(!crate::regmap::functional_config_matches_ep_type(
            &generic, &tag
        ));
    }

    // ── LinRequest::from_evt_sub_opcode ──────────────────────────────────────

    #[test]
    //fusa:test REQ-LIN-008
    //fusa:test REQ-LIN-009
    fn lin_request_plain_evt_decodes_payload_as_frame_transfer() {
        // TC18 §13.7.10.3 Figure 39's own worked example: a 3-byte LIN
        // payload (PID byte followed by two data bytes).
        let payload = [0x3C, 0x01, 0x02];
        let request = LinRequest::from_evt_sub_opcode(0b000, &payload).unwrap();
        assert_eq!(
            request,
            LinRequest::Plain(LinFrameTransfer {
                pid: 0x3C,
                data: vec![0x01, 0x02],
            })
        );
    }

    #[test]
    //fusa:test REQ-LIN-008
    //fusa:test REQ-LIN-009
    fn lin_request_plain_evt_rejects_an_empty_payload() {
        // Unlike I2cRequest::Plain/AdcRequest::Plain/PwmInRequest::Plain,
        // LinRequest::Plain propagates LinFrameTransfer::decode's own
        // Result rather than an infallible decode — an empty payload has no
        // PID byte, matching lin_frame_transfer_decode_rejects_empty_input
        // above.
        assert_eq!(
            LinRequest::from_evt_sub_opcode(0b000, &[]),
            Err(RcpError::ShortFrame)
        );
    }

    #[test]
    //fusa:test REQ-LIN-008
    //fusa:test REQ-LIN-009
    fn lin_request_plain_evt_rejects_data_longer_than_lin_max_data() {
        let mut payload = vec![0x3Cu8];
        payload.extend(vec![0xAAu8; LIN_MAX_DATA + 1]);
        assert_eq!(
            LinRequest::from_evt_sub_opcode(0b000, &payload),
            Err(RcpError::PayloadTooLarge)
        );
    }

    #[test]
    //fusa:test REQ-LIN-008
    //fusa:test REQ-LIN-009
    fn lin_request_plain_evt_accepts_data_at_exactly_lin_max_data() {
        let mut payload = vec![0x3Cu8];
        payload.extend(vec![0xAAu8; LIN_MAX_DATA]);
        let request = LinRequest::from_evt_sub_opcode(0b000, &payload).unwrap();
        assert_eq!(
            request,
            LinRequest::Plain(LinFrameTransfer {
                pid: 0x3C,
                data: vec![0xAAu8; LIN_MAX_DATA],
            })
        );
    }

    #[test]
    //fusa:test REQ-LIN-008
    //fusa:test REQ-LIN-009
    fn lin_request_config_write_evt_is_recognized_without_interpreting_payload() {
        // The payload is not decoded as a LinFrameTransfer for a
        // config-write request — the variant carries no payload at all, so
        // garbage bytes here cannot be silently misread as a frame.
        let request = LinRequest::from_evt_sub_opcode(0b111, &[0xDE, 0xAD, 0xBE, 0xEF]).unwrap();
        assert_eq!(request, LinRequest::ConfigWrite);
    }

    #[test]
    //fusa:test REQ-LIN-009
    fn lin_request_reserved_evt_values_are_rejected_with_unsupported_cmd() {
        for sub_opcode in 0b001..=0b110u8 {
            assert_eq!(
                LinRequest::from_evt_sub_opcode(sub_opcode, &[]),
                Err(RcpError::UnsupportedCmd)
            );
            assert_eq!(
                LinRequest::from_evt_sub_opcode(sub_opcode, &[1, 2, 3]),
                Err(RcpError::UnsupportedCmd)
            );
        }
    }

    #[test]
    //fusa:test REQ-LIN-009
    fn lin_request_values_above_the_3_bit_field_are_also_rejected_with_unsupported_cmd() {
        for sub_opcode in (crate::acf::EVT_SUB_OPCODE_MAX + 1)..=u8::MAX {
            assert_eq!(
                LinRequest::from_evt_sub_opcode(sub_opcode, &[]),
                Err(RcpError::UnsupportedCmd)
            );
        }
    }

    #[test]
    //fusa:test REQ-LIN-009
    fn lin_request_from_evt_sub_opcode_never_panics_for_any_sampled_input() {
        let payloads: [&[u8]; 4] = [&[], &[0x00], &[0x3C, 0x01, 0x02], &[0xAA; 32]];
        for sub_opcode in 0..=u8::MAX {
            for payload in payloads {
                let _ = LinRequest::from_evt_sub_opcode(sub_opcode, payload);
            }
        }
    }
}
