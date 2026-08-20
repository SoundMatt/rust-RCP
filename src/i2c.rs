//fusa:req REQ-I2C-001
//fusa:req REQ-I2C-002
//fusa:req REQ-I2C-003
//fusa:req REQ-I2C-004
//fusa:req REQ-I2C-005
//fusa:req REQ-I2C-006
//fusa:req REQ-I2C-012
//fusa:req REQ-I2C-013

//! The I²C endpoint type (`ep_type 0x04`) — `ROADMAP.md` Milestone 4
//! ("Basic Endpoint Types"), third checklist bullet: "controller-only, raw
//! byte stream including address bytes; `i2c_mode` speed presets (flag the
//! enum ambiguity between adjacent high-speed rows as unresolved pending
//! errata, per this crate's spec-extraction §5.7 — do not silently pick
//! one)".
//!
//! This follows directly on [`crate::spi`] (Milestone 4's second item, and
//! `ep_type 0x04`'s immediate predecessor `ep_type 0x03`): same milestone,
//! same "additive standalone plumbing only" discipline, same doc-comment
//! provenance-note style for anything this crate has not yet reconciled
//! against confirmed wire behavior. Two named pieces are in scope, both
//! implemented here:
//!
//! - [`I2cByteTransfer`] / [`I2cByteTransferResult`] — the raw byte stream
//!   an I²C request sends and an I²C response returns, modeled as an
//!   unstructured byte stream rather than an interpreted payload, matching
//!   [`crate::spi::SpiByteTransfer`]/[`crate::spi::SpiByteTransferResult`]'s
//!   own PICO/POCI discipline. See "Provenance note: address bytes are
//!   carried inline, unparsed" below for why this module does not split an
//!   address out of that stream.
//! - [`I2cSpeedMode`] — the `i2c_mode` speed-preset field, plus
//!   [`I2cFunctionalConfig`] carrying it as this endpoint type's
//!   functional-config content. See "Provenance note: the ambiguous
//!   high-speed rows" below for why two of [`I2cSpeedMode`]'s variants are
//!   deliberately left unresolved rather than each assigned a specific named
//!   speed.
//! - [`I2cRequest`]/[`I2cRequest::from_evt_sub_opcode`] — I²C's own
//!   request-decode entry point, validating an incoming request's
//!   `evt.sub_opcode` against [`crate::evtgroup::evt_row2_kind_of`]'s TC18
//!   §13.5 Table 33 Row-2 rule and dispatching a [`Plain`](I2cRequest::Plain)
//!   request to [`I2cByteTransfer::decode`]. See "Provenance note: evt[2:0]
//!   request validation" below — this piece was added after this module's
//!   own original scope note below (which still accurately describes why no
//!   `sub_opcode` reading existed here originally) as this crate's pilot
//!   implementation of TC18 §13.5 Table 33's Row-2 rule, the reference
//!   pattern the other seven Row-2 endpoint types
//!   (`{ADC, PWM_IN, LIN, CAN, UART, ISELED, MDIO}`) are expected to follow
//!   in their own later items.
//!
//! Deliberately out of scope, for the same reasons [`crate::gpio`]'s and
//! [`crate::spi`]'s own doc comments already give:
//!
//! - Any peripheral/target-mode role for this endpoint. `ROADMAP.md`'s I²C
//!   checklist bullet states this endpoint type is "controller-only", so
//!   unlike some I²C hardware this module models no role-selection type at
//!   all (no `I2cRole` enum, no peripheral-address-match config) — there is
//!   only ever the one, controller, role.
//! - The "Groups A/B/C" `evt[2:0]` sub-opcode convention
//!   ([`crate::evtgroup::EvtGroup`]) as a general, cross-endpoint-type
//!   classification scheme — [`crate::evtgroup`]'s own doc comment already
//!   flags that broader scheme as unresolved, independent of the narrower,
//!   unambiguous Table 33 Row-2 rule this module's [`I2cRequest`] now
//!   implements (see "Provenance note: evt[2:0] request validation" below).
//! - [`crate::regmap::CommonFunctionalConfig`]'s fields — unchanged here, as
//!   in every prior Milestone 1-4 entry.
//! - Decoding [`I2cRequest::ConfigWrite`]'s own TC18 §12.7.1 payload shape.
//!   [`I2cRequest::from_evt_sub_opcode`] recognizes a config-write request
//!   as distinct from a [`Plain`](I2cRequest::Plain) one, but does not
//!   itself interpret what the config-write payload contains — that is
//!   separate, later work, same as every Row-2 endpoint-type module this
//!   predicate lands in.
//! - Wiring [`I2cRequest::from_evt_sub_opcode`] into an actual decoder,
//!   dispatch loop, or [`crate::mock::Endpoint`] implementation.
//!   [`crate::mock::Endpoint`]'s own trait signature does not carry an
//!   `evt` value to any implementation at all yet — that gap is not
//!   specific to I²C, it applies identically to
//!   [`crate::gpio::GpioWriteSemantics::from_sub_opcode`]/
//!   [`crate::spi::SpiChannelSelect::from_sub_opcode`], neither of which is
//!   wired into [`crate::mock`]'s dispatch either (see
//!   [`crate::mock::Endpoint`]'s own doc comment). [`I2cRequest`] is built
//!   to the same "additive standalone plumbing only" level GPIO/SPI's own
//!   `sub_opcode` readers are at today, ready for whichever later item
//!   first threads a live `evt` value through an actual dispatch loop.
//!
//! ## Provenance note: evt[2:0] request validation
//!
//! I²C is one of the eight endpoint types TC18 §13.5 Table 33 groups into
//! one shared "Row 2" `evt[2:0]` rule — see
//! [`crate::evtgroup`]'s own doc comment "Provenance note: TC18 §13.5 Table
//! 33's Row-2 rule (`evt_row2_kind_of`)" for the full citation, including
//! the literal-text discrepancy that module's doc comment flags and
//! resolves (Table 33's own printed Row-2 cell reads "000b to 110b
//! reserved", including 000b, which this crate does not implement
//! literally). [`I2cRequest::from_evt_sub_opcode`] is this module's own
//! caller of that shared [`crate::evtgroup::evt_row2_kind_of`] predicate: a
//! [`Plain`](I2cRequest::Plain) request (`evt[2:0] == 000b`) decodes its
//! payload bytes as an [`I2cByteTransfer`] exactly as TC18 §13.7.7.3
//! describes ("The byte msg payload is the I2C payload including the
//! address"); every `Reserved` sub_opcode is rejected with
//! `Err(`[`RcpError::UnsupportedCmd`]`)`, matching Table 33's own stated
//! error code and [`crate::gpio::apply_gpio_write`]'s identical refusal of
//! its own table's reserved code.
//!
//! ## Relationship to [`crate::regmap`]
//!
//! As with [`crate::gpio::GpioFunctionalConfig`] and
//! [`crate::spi::SpiFunctionalConfig`], I²C's real functional-config content
//! gets its own dedicated type, [`I2cFunctionalConfig`], rather than adding
//! I²C-specific fields directly onto the still-shared, thirteen-endpoint-type
//! [`crate::regmap::PerEpTypeFunctionalConfig`] placeholder.
//! [`I2cFunctionalConfig::layer_tag`] shows how a caller obtains the matching
//! generic-layer tag so the two compose through
//! [`crate::regmap::check_functional_config_matches_ep_type`] exactly as
//! that cross-layer rule already expects, without this module editing
//! [`crate::regmap`] itself.
//!
//! ## Provenance note: address bytes are carried inline, unparsed
//!
//! `ROADMAP.md`'s I²C checklist bullet states the raw byte stream includes
//! "address bytes" without stating their framing — in particular, without
//! stating whether every transfer carries a 7-bit address, a 10-bit
//! address, or a mix, or at what fixed offset (if any) the address byte(s)
//! sit within the stream. Per Guiding Principle 5, [`I2cByteTransfer`] and
//! [`I2cByteTransferResult`] do not attempt to parse an address out of the
//! stream or otherwise distinguish addressing schemes — the entire stream,
//! address bytes included, is carried as this module's already-established
//! unstructured `Vec<u8>` shape, exactly as [`crate::spi::SpiByteTransfer`]
//! carries SPI's PICO bytes without interpreting them. A future item that
//! needs to reason about the address specifically (for example, to satisfy
//! this milestone's still-outstanding common functional-config or
//! Groups A/B/C work) can add that parsing later without this module having
//! guessed at a framing this crate cannot yet confirm.
//!
//! ## Provenance note: the ambiguous high-speed rows
//!
//! `ROADMAP.md`'s I²C checklist bullet directs this crate to "flag the enum
//! ambiguity between adjacent high-speed rows as unresolved pending errata,
//! per this crate's spec-extraction §5.7 — do not silently pick one" rather
//! than resolving it. [`I2cSpeedMode`] models the lower, unambiguous speed
//! presets as ordinarily named variants
//! ([`I2cSpeedMode::Standard`]/[`I2cSpeedMode::Fast`]/[`I2cSpeedMode::FastPlus`]),
//! then represents the two adjacent high-speed rows this crate's own
//! spec-extraction pass could not distinguish as two explicitly, neutrally
//! named variants, [`I2cSpeedMode::HighSpeedRowA`] and
//! [`I2cSpeedMode::HighSpeedRowB`], rather than guessing which row is (for
//! example) "High-speed mode" versus some faster or slower neighboring
//! preset — mirroring [`crate::gpio::GpioWriteSemantics::Unnamed8th`]'s own
//! treatment of GPIO's single unnamed write-semantics slot, and
//! [`crate::spi::SpiChannelSelect::Spare6`]/
//! [`crate::spi::SpiChannelSelect::Spare7`]'s treatment of SPI's two spare
//! selection values. [`I2cSpeedMode::is_ambiguous_high_speed_row`] lets a
//! caller detect this rather than treat either variant as confirmed.
//! Resolving which row is which, and what wire value each of the five
//! `i2c_mode` presets actually carries, is left to errata reconciliation
//! against confirmed wire behavior — never against spec prose — matching
//! every other still-open provenance note in this crate. The specific
//! `0..=4` wire-value ordering [`I2cSpeedMode::to_u8`] assigns (ascending by
//! this module's own best-effort speed ordering, with the two ambiguous rows
//! placed last and adjacent to each other) is this crate's own working
//! choice, not a transcription of a confirmed wire encoding.

use crate::evtgroup::{evt_row2_kind_of, EvtRow2Kind};
use crate::RcpError;

// ── I2cSpeedMode ─────────────────────────────────────────────────────────────

/// The `i2c_mode` speed preset this endpoint is configured for.
///
/// See this module's doc comment "Provenance note: the ambiguous high-speed
/// rows" for why [`I2cSpeedMode::HighSpeedRowA`]/[`I2cSpeedMode::HighSpeedRowB`]
/// are deliberately left unresolved rather than each given a specific named
/// speed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
//fusa:req REQ-I2C-001
pub enum I2cSpeedMode {
    /// Standard-mode speed preset (this crate's slowest, most broadly
    /// compatible preset).
    Standard = 0,
    /// Fast-mode speed preset.
    Fast = 1,
    /// Fast-mode-plus speed preset.
    FastPlus = 2,
    /// The first of the two adjacent high-speed `i2c_mode` rows this
    /// crate's spec-extraction pass could not distinguish. See this
    /// module's doc comment.
    HighSpeedRowA = 3,
    /// The second of the two adjacent high-speed `i2c_mode` rows this
    /// crate's spec-extraction pass could not distinguish. See this
    /// module's doc comment.
    HighSpeedRowB = 4,
}

impl I2cSpeedMode {
    /// Encode this speed preset as its `i2c_mode` wire byte value.
    //fusa:req REQ-I2C-001
    //fusa:req REQ-I2C-009
    pub fn to_u8(self) -> u8 {
        self as u8
    }

    /// Decode an `i2c_mode` wire byte value into an [`I2cSpeedMode`].
    ///
    /// Returns `Err(RcpError::InvalidParameter)` for any byte outside
    /// `0..=4`, matching [`crate::spi::SpiChannelSelect::from_sub_opcode`]'s
    /// own range-check discipline. Never panics for any input.
    //fusa:req REQ-I2C-002
    //fusa:req REQ-I2C-009
    //fusa:req REQ-I2C-010
    pub fn from_u8(raw: u8) -> Result<Self, RcpError> {
        match raw {
            0 => Ok(Self::Standard),
            1 => Ok(Self::Fast),
            2 => Ok(Self::FastPlus),
            3 => Ok(Self::HighSpeedRowA),
            4 => Ok(Self::HighSpeedRowB),
            _ => Err(RcpError::InvalidParameter),
        }
    }

    /// True for [`I2cSpeedMode::HighSpeedRowA`]/[`I2cSpeedMode::HighSpeedRowB`]
    /// — the two adjacent high-speed `i2c_mode` rows this module's doc
    /// comment flags as unresolved pending errata. False for
    /// [`I2cSpeedMode::Standard`]/[`I2cSpeedMode::Fast`]/[`I2cSpeedMode::FastPlus`].
    //fusa:req REQ-I2C-003
    //fusa:req REQ-I2C-010
    pub fn is_ambiguous_high_speed_row(self) -> bool {
        matches!(self, Self::HighSpeedRowA | Self::HighSpeedRowB)
    }
}

impl Default for I2cSpeedMode {
    /// Defaults to [`I2cSpeedMode::Standard`] — this module's own reasonable
    /// choice of the least capability-demanding preset, not a confirmed
    /// power-on default from the source spec.
    fn default() -> Self {
        Self::Standard
    }
}

// ── I2cFunctionalConfig ──────────────────────────────────────────────────────

/// I²C's own per-EP-type functional-config content: this endpoint's
/// [`I2cSpeedMode`].
///
/// See this module's doc comment "Relationship to `crate::regmap`" for why
/// this is a dedicated type rather than content added directly to
/// [`crate::regmap::PerEpTypeFunctionalConfig`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-I2C-004
pub struct I2cFunctionalConfig {
    /// This endpoint's configured `i2c_mode` speed preset.
    pub speed_mode: I2cSpeedMode,
}

impl I2cFunctionalConfig {
    /// The [`crate::regmap::PerEpTypeFunctionalConfig`] generic-layer tag
    /// that matches this I²C functional config, for use with
    /// [`crate::regmap::check_functional_config_matches_ep_type`].
    ///
    /// This module does not itself call that function — it only shows how a
    /// caller would obtain the matching tag, per this module's doc comment
    /// "Relationship to `crate::regmap`".
    //fusa:req REQ-I2C-004
    pub fn layer_tag(&self) -> crate::regmap::PerEpTypeFunctionalConfig {
        crate::regmap::PerEpTypeFunctionalConfig::new(crate::regmap::EndpointType::I2c)
    }
}

// ── Raw I2C byte transfer ────────────────────────────────────────────────────

/// A raw I²C byte transfer: the bytes an I²C request sends from controller
/// to bus, including address byte(s).
///
/// Modeled as an unstructured, variable-length byte stream — this module
/// does not interpret its contents, including the address byte(s) it
/// contains — matching how [`crate::spi::SpiByteTransfer`] modeled its own
/// PICO byte stream. See this module's doc comment "Provenance note: address
/// bytes are carried inline, unparsed". Every possible byte slice, including
/// an empty one, has a valid encoding, so [`I2cByteTransfer::decode`] is
/// infallible.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
//fusa:req REQ-I2C-005
pub struct I2cByteTransfer {
    /// The raw bytes sent from controller to bus, address byte(s) included.
    pub bytes: Vec<u8>,
}

impl I2cByteTransfer {
    /// Encode this transfer to its raw wire representation: `bytes`,
    /// unmodified and unframed.
    //fusa:req REQ-I2C-005
    //fusa:req REQ-I2C-011
    pub fn encode(&self) -> Vec<u8> {
        self.bytes.clone()
    }

    /// Decode an [`I2cByteTransfer`] from a byte slice.
    ///
    /// Every possible byte slice, including an empty one, is a valid I²C
    /// transfer, so this never fails and never panics for any input.
    //fusa:req REQ-I2C-005
    //fusa:req REQ-I2C-011
    pub fn decode(b: &[u8]) -> Self {
        Self { bytes: b.to_vec() }
    }
}

/// A raw I²C byte transfer result: the bytes an I²C response returns from
/// bus to controller.
///
/// See [`I2cByteTransfer`]'s doc comment — this is the same unstructured,
/// variable-length byte-stream modeling for the opposite transfer
/// direction.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
//fusa:req REQ-I2C-006
pub struct I2cByteTransferResult {
    /// The raw bytes returned from bus to controller.
    pub bytes: Vec<u8>,
}

impl I2cByteTransferResult {
    /// Encode this transfer result to its raw wire representation: `bytes`,
    /// unmodified and unframed.
    //fusa:req REQ-I2C-006
    pub fn encode(&self) -> Vec<u8> {
        self.bytes.clone()
    }

    /// Decode an [`I2cByteTransferResult`] from a byte slice.
    ///
    /// Every possible byte slice, including an empty one, is a valid I²C
    /// transfer result, so this never fails and never panics for any input.
    //fusa:req REQ-I2C-006
    //fusa:req REQ-I2C-011
    pub fn decode(b: &[u8]) -> Self {
        Self { bytes: b.to_vec() }
    }
}

// ── I2cRequest: evt[2:0] request validation ─────────────────────────────────

/// The decoded shape of an incoming I²C request, after validating its
/// `evt[2:0]` sub-opcode against TC18 §13.5 Table 33's Row-2 rule (I²C is
/// one of that row's eight endpoint types —
/// `{ADC, PWM_IN, I²C, LIN, CAN, UART, ISELED, MDIO}`).
///
/// See this module's doc comment "Provenance note: evt[2:0] request
/// validation" for the full citation, and
/// [`crate::evtgroup`]'s own doc comment for the literal-text discrepancy
/// this crate resolves `evt[2:0] == 000b` against.
#[derive(Debug, Clone, PartialEq, Eq)]
//fusa:req REQ-I2C-012
pub enum I2cRequest {
    /// `evt[2:0] == 000b`: an ordinary I²C transfer — `byte_msg_payload` is
    /// this transaction's raw bytes, decoded as an [`I2cByteTransfer`] per
    /// [`I2cByteTransfer::decode`].
    Plain(I2cByteTransfer),
    /// `evt[2:0] == 111b`: a functional-config write (TC18 §12.7.1) rather
    /// than an ordinary transfer. This crate does not yet decode the
    /// config-write payload shape itself — see this module's doc comment
    /// "Deliberately out of scope" — so a caller receiving this variant
    /// knows only that the request *is* a config-write, not its content.
    ConfigWrite,
}

impl I2cRequest {
    /// Decode an incoming I²C request from its `evt.sub_opcode`
    /// ([`crate::acf::Evt::sub_opcode`]) and raw `byte_msg_payload` bytes.
    ///
    /// Returns `Err(`[`RcpError::UnsupportedCmd`]`)` for every
    /// [`EvtRow2Kind::Reserved`] sub_opcode value — TC18 §13.5 Table 33's
    /// Row-2 rule requires the request be rejected with error code
    /// `UNSUPPORTED_CMD`, matching
    /// [`crate::gpio::apply_gpio_write`]'s identical refusal of its own
    /// table's reserved `100b` code. Never panics for any
    /// `sub_opcode`/`payload` combination — [`I2cByteTransfer::decode`] is
    /// itself infallible and total over every byte slice.
    //fusa:req REQ-I2C-012
    //fusa:req REQ-I2C-013
    pub fn from_evt_sub_opcode(sub_opcode: u8, payload: &[u8]) -> Result<Self, RcpError> {
        match evt_row2_kind_of(sub_opcode) {
            EvtRow2Kind::Plain => Ok(Self::Plain(I2cByteTransfer::decode(payload))),
            EvtRow2Kind::ConfigWrite => Ok(Self::ConfigWrite),
            EvtRow2Kind::Reserved => Err(RcpError::UnsupportedCmd),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── I2cSpeedMode: to_u8/from_u8 round-trip ──────────────────────────────

    const ALL_SPEED_MODES: [I2cSpeedMode; 5] = [
        I2cSpeedMode::Standard,
        I2cSpeedMode::Fast,
        I2cSpeedMode::FastPlus,
        I2cSpeedMode::HighSpeedRowA,
        I2cSpeedMode::HighSpeedRowB,
    ];

    #[test]
    //fusa:test REQ-I2C-001
    fn i2c_speed_mode_round_trips_through_to_u8_from_u8_for_all_five_values() {
        for mode in ALL_SPEED_MODES {
            let raw = mode.to_u8();
            assert_eq!(I2cSpeedMode::from_u8(raw), Ok(mode));
        }
    }

    #[test]
    //fusa:test REQ-I2C-001
    fn i2c_speed_mode_to_u8_values_are_the_full_0_to_4_range() {
        let mut raws: Vec<u8> = ALL_SPEED_MODES.iter().map(|m| m.to_u8()).collect();
        raws.sort_unstable();
        assert_eq!(raws, (0u8..=4).collect::<Vec<_>>());
    }

    #[test]
    //fusa:test REQ-I2C-002
    fn i2c_speed_mode_from_u8_rejects_out_of_range() {
        for raw in [5u8, 6, 0x7F, 0xFF] {
            assert_eq!(I2cSpeedMode::from_u8(raw), Err(RcpError::InvalidParameter));
        }
    }

    #[test]
    //fusa:test REQ-I2C-003
    fn i2c_speed_mode_is_ambiguous_high_speed_row_true_only_for_the_two_flagged_rows() {
        for mode in ALL_SPEED_MODES {
            let expected = matches!(
                mode,
                I2cSpeedMode::HighSpeedRowA | I2cSpeedMode::HighSpeedRowB
            );
            assert_eq!(mode.is_ambiguous_high_speed_row(), expected);
        }
    }

    #[test]
    //fusa:test REQ-I2C-003
    fn i2c_speed_mode_default_is_standard_and_not_an_ambiguous_row() {
        let mode = I2cSpeedMode::default();
        assert_eq!(mode, I2cSpeedMode::Standard);
        assert!(!mode.is_ambiguous_high_speed_row());
    }

    // ── TC18 Table 46: i2c_mode preset wire values ──────────────────────────

    #[test]
    //fusa:test REQ-I2C-009
    fn i2c_mode_wire_values_match_tc18_table_46_unambiguous_rows() {
        // TC18 §13.7.7.2 Table 46 (TC18.txt lines 4815-4817), i2c_mode
        // (relative address 0x0007, 8 bit R/W):
        //   0: Standard Mode 100kbit/s
        //   1: Fast Mode 400kbit/s
        //   2: Fast Mode plus 1Mbit/s
        assert_eq!(I2cSpeedMode::from_u8(0), Ok(I2cSpeedMode::Standard));
        assert_eq!(I2cSpeedMode::from_u8(1), Ok(I2cSpeedMode::Fast));
        assert_eq!(I2cSpeedMode::from_u8(2), Ok(I2cSpeedMode::FastPlus));
        assert_eq!(I2cSpeedMode::Standard.to_u8(), 0);
        assert_eq!(I2cSpeedMode::Fast.to_u8(), 1);
        assert_eq!(I2cSpeedMode::FastPlus.to_u8(), 2);
        // None of these three rows is one of Table 46's duplicated
        // high-speed rows, so none may be reported as unresolved.
        for mode in [
            I2cSpeedMode::Standard,
            I2cSpeedMode::Fast,
            I2cSpeedMode::FastPlus,
        ] {
            assert!(!mode.is_ambiguous_high_speed_row());
        }
    }

    #[test]
    //fusa:test REQ-I2C-010
    fn i2c_mode_value_three_is_not_resolved_to_a_single_high_speed_rate() {
        // TC18 §13.7.7.2 Table 46 (TC18.txt lines 4818-4819) lists two
        // adjacent High-speed rows that both carry the same i2c_mode wire
        // value 3:
        //   3: High-speed mode 1.7Mbit/s
        //   3: High-speed mode 3.4Mbit/s
        // Decoding value 3 must therefore be flagged as unresolved rather
        // than silently picking either bit rate.
        let decoded = I2cSpeedMode::from_u8(3).expect("3 is an enumerated Table 46 i2c_mode value");
        assert!(decoded.is_ambiguous_high_speed_row());
        assert_eq!(decoded.to_u8(), 3);
    }

    // ── TC18 §13.7.7.3: address-format transparency ─────────────────────────

    #[test]
    //fusa:test REQ-I2C-011
    fn i2c_byte_transfer_is_transparent_to_seven_and_ten_bit_addressing() {
        // TC18 §13.7.7.3 (TC18.txt line 4830): "The byte msg payload is the
        // I2C payload including the address. The I2C endpoint does not know
        // whether there is a 7- or 10-bit address, since the endpoint is just
        // transparent." The worked example there (Figure 29) is an I²C
        // transfer with a 10-bit address and 5 bytes of data — 2 address
        // bytes + 5 data bytes = a 7-byte byte_msg_payload.
        let ten_bit_addressed = vec![0xF2, 0x34, 0x11, 0x22, 0x33, 0x44, 0x55];
        assert_eq!(ten_bit_addressed.len(), 7);
        let transfer = I2cByteTransfer {
            bytes: ten_bit_addressed.clone(),
        };
        // Emitted verbatim: no length prefix, no address framing, no
        // reordering, nothing stripped.
        assert_eq!(transfer.encode(), ten_bit_addressed);
        assert_eq!(
            I2cByteTransfer::decode(&ten_bit_addressed).bytes,
            ten_bit_addressed
        );

        // The same 5 data bytes behind a single 7-bit address byte are
        // carried identically — exactly one byte shorter, nothing else
        // differs, and no addressing scheme is inferred either way.
        let seven_bit_addressed = vec![0xA0, 0x11, 0x22, 0x33, 0x44, 0x55];
        assert_eq!(seven_bit_addressed.len(), ten_bit_addressed.len() - 1);
        assert_eq!(
            I2cByteTransfer::decode(&seven_bit_addressed).bytes,
            seven_bit_addressed
        );

        // The bus-to-controller direction is equally transparent.
        assert_eq!(
            I2cByteTransferResult::decode(&ten_bit_addressed).encode(),
            ten_bit_addressed
        );
    }

    // ── I2cFunctionalConfig / layer_tag ─────────────────────────────────────

    #[test]
    //fusa:test REQ-I2C-004
    fn i2c_functional_config_default_uses_default_speed_mode() {
        let config = I2cFunctionalConfig::default();
        assert_eq!(config.speed_mode, I2cSpeedMode::default());
    }

    #[test]
    //fusa:test REQ-I2C-004
    fn i2c_functional_config_layer_tag_matches_ep_type_i2c() {
        let functional = I2cFunctionalConfig::default();
        let generic = crate::regmap::PerEpConfigBlock::new(crate::regmap::EndpointType::I2c);
        let tag = functional.layer_tag();
        assert_eq!(tag.ep_type, crate::regmap::EndpointType::I2c);
        assert!(crate::regmap::functional_config_matches_ep_type(
            &generic, &tag
        ));
        assert_eq!(
            crate::regmap::check_functional_config_matches_ep_type(&generic, &tag),
            Ok(())
        );
    }

    #[test]
    //fusa:test REQ-I2C-004
    fn i2c_functional_config_layer_tag_rejects_mismatched_ep_type() {
        let functional = I2cFunctionalConfig::default();
        let generic = crate::regmap::PerEpConfigBlock::new(crate::regmap::EndpointType::Spi);
        let tag = functional.layer_tag();
        assert!(!crate::regmap::functional_config_matches_ep_type(
            &generic, &tag
        ));
    }

    // ── I2cByteTransfer: round-trip / never-panic ───────────────────────────

    #[test]
    //fusa:test REQ-I2C-005
    fn i2c_byte_transfer_round_trips_through_encode_decode() {
        for bytes in [
            vec![],
            vec![0x00],
            // A plausible 7-bit-address-plus-data shape: this module does
            // not interpret it as such, it is exercised only as opaque
            // bytes.
            vec![0xA0, 0x01, 0x02],
            (0u8..=255).collect(),
        ] {
            let transfer = I2cByteTransfer {
                bytes: bytes.clone(),
            };
            assert_eq!(I2cByteTransfer::decode(&transfer.encode()).bytes, bytes);
        }
    }

    #[test]
    //fusa:test REQ-I2C-005
    fn i2c_byte_transfer_decode_never_panics_for_any_sampled_input() {
        for len in [0usize, 1, 2, 7, 64] {
            let buf = vec![0x5Au8; len];
            let _ = I2cByteTransfer::decode(&buf);
        }
    }

    // ── I2cByteTransferResult: round-trip / never-panic ─────────────────────

    #[test]
    //fusa:test REQ-I2C-006
    fn i2c_byte_transfer_result_round_trips_through_encode_decode() {
        for bytes in [vec![], vec![0xFF], vec![0x01, 0x02, 0x03]] {
            let result = I2cByteTransferResult {
                bytes: bytes.clone(),
            };
            assert_eq!(I2cByteTransferResult::decode(&result.encode()).bytes, bytes);
        }
    }

    #[test]
    //fusa:test REQ-I2C-006
    fn i2c_byte_transfer_result_decode_never_panics_for_any_sampled_input() {
        for len in [0usize, 1, 5, 32] {
            let buf = vec![0xA5u8; len];
            let _ = I2cByteTransferResult::decode(&buf);
        }
    }

    // ── I2cRequest::from_evt_sub_opcode ─────────────────────────────────────

    #[test]
    //fusa:test REQ-I2C-012
    //fusa:test REQ-I2C-013
    fn i2c_request_plain_evt_decodes_payload_as_byte_transfer() {
        // TC18 §13.7.7.3 Figure 30's own worked example: a 10-bit-addressed
        // transfer, address bytes included verbatim in the payload.
        let payload = [0xF2, 0x34, 0x11, 0x22, 0x33, 0x44, 0x55];
        let request = I2cRequest::from_evt_sub_opcode(0b000, &payload).unwrap();
        assert_eq!(
            request,
            I2cRequest::Plain(I2cByteTransfer {
                bytes: payload.to_vec()
            })
        );
    }

    #[test]
    //fusa:test REQ-I2C-012
    //fusa:test REQ-I2C-013
    fn i2c_request_plain_evt_accepts_an_empty_payload() {
        let request = I2cRequest::from_evt_sub_opcode(0b000, &[]).unwrap();
        assert_eq!(request, I2cRequest::Plain(I2cByteTransfer::default()));
    }

    #[test]
    //fusa:test REQ-I2C-012
    //fusa:test REQ-I2C-013
    fn i2c_request_config_write_evt_is_recognized_without_interpreting_payload() {
        // The payload is not decoded as an I2cByteTransfer for a
        // config-write request — the variant carries no payload at all,
        // so garbage bytes here cannot be silently misread as a transfer.
        let request = I2cRequest::from_evt_sub_opcode(0b111, &[0xDE, 0xAD, 0xBE, 0xEF]).unwrap();
        assert_eq!(request, I2cRequest::ConfigWrite);
    }

    #[test]
    //fusa:test REQ-I2C-013
    fn i2c_request_reserved_evt_values_are_rejected_with_unsupported_cmd() {
        for sub_opcode in 0b001..=0b110u8 {
            assert_eq!(
                I2cRequest::from_evt_sub_opcode(sub_opcode, &[]),
                Err(RcpError::UnsupportedCmd)
            );
            assert_eq!(
                I2cRequest::from_evt_sub_opcode(sub_opcode, &[1, 2, 3]),
                Err(RcpError::UnsupportedCmd)
            );
        }
    }

    #[test]
    //fusa:test REQ-I2C-013
    fn i2c_request_values_above_the_3_bit_field_are_also_rejected_with_unsupported_cmd() {
        for sub_opcode in (crate::acf::EVT_SUB_OPCODE_MAX + 1)..=u8::MAX {
            assert_eq!(
                I2cRequest::from_evt_sub_opcode(sub_opcode, &[]),
                Err(RcpError::UnsupportedCmd)
            );
        }
    }

    #[test]
    //fusa:test REQ-I2C-013
    fn i2c_request_from_evt_sub_opcode_never_panics_for_any_sampled_input() {
        let payloads: [&[u8]; 3] = [&[], &[0x00], &[0xAA; 32]];
        for sub_opcode in 0..=u8::MAX {
            for payload in payloads {
                let _ = I2cRequest::from_evt_sub_opcode(sub_opcode, payload);
            }
        }
    }
}
