//fusa:req REQ-MDIO-001
//fusa:req REQ-MDIO-002
//fusa:req REQ-MDIO-003
//fusa:req REQ-MDIO-004
//fusa:req REQ-MDIO-005
//fusa:req REQ-MDIO-006

//! The MDIO endpoint type (`ep_type 0x0D`) — `ROADMAP.md` Milestone 7
//! ("Remaining Endpoint Types"), fourth checklist bullet: IEEE 802.3
//! Clause-22/Clause-45 addressing-mode selection via a 2-bit `mdio_mode`
//! selector, and a minimal functional config — no clock-divider field and
//! no mode-select fields beyond [`crate::regmap::CommonFunctionalConfig`]'s
//! own universal common block.
//!
//! This follows directly on [`crate::lin`], [`crate::can`], and
//! [`crate::iseled`] (this milestone's first three entries): same
//! milestone, same "additive standalone plumbing only" discipline, same
//! doc-comment provenance-note style for anything this crate has not yet
//! reconciled against confirmed wire behavior. Like [`crate::iseled`], MDIO
//! has no old-protocol satellite bridge module in this crate (no
//! `mdiobr.rs`) to validate against or migrate away from, so every piece
//! below is new modeling rather than a read-and-reject exercise against
//! prior code. Two named pieces are in scope, both implemented here:
//!
//! - [`MdioAddressingMode`] — the `mdio_mode` 2-bit Clause-22/Clause-45
//!   selector, plus [`MdioFunctionalConfig`] carrying it as this endpoint
//!   type's (deliberately minimal) functional-config content. See
//!   "Provenance note: the two unallocated `mdio_mode` slots" below for why
//!   two of [`MdioAddressingMode`]'s four 2-bit values are deliberately
//!   left unresolved rather than each assigned a specific meaning.
//! - [`MdioTransfer`] / [`MdioTransferResult`] — the raw register-access
//!   byte stream an MDIO request sends and an MDIO response returns. See
//!   "Provenance note: register-access framing is carried opaque" below.
//!
//! Deliberately out of scope, for the same reasons every prior Milestone
//! 4/7 entry's own doc comment already gives:
//!
//! - Any clock-divider field, or any mode-select field beyond `mdio_mode`
//!   itself. `ROADMAP.md`'s checklist bullet explicitly calls for a
//!   "minimal" functional config, so [`MdioFunctionalConfig`] carries
//!   exactly the one [`MdioAddressingMode`] field and nothing else —
//!   deliberately not mirroring [`crate::spi::SpiFunctionalConfig`]'s
//!   multi-channel shape or [`crate::adc::AdcFunctionalConfig`]'s
//!   multi-field averaging shape.
//! - [`crate::regmap::CommonFunctionalConfig`]'s fields — unchanged here, as
//!   in every prior Milestone 1-4/7 entry, and explicitly the "universal
//!   common block" this checklist bullet's own text names as the ceiling
//!   [`MdioFunctionalConfig`] must not duplicate or exceed.
//! - Any PHY register-map semantics (what a given register address or
//!   device-type field actually controls). `ROADMAP.md`'s checklist bullet
//!   names addressing-mode selection only, no register semantics, so this
//!   module carries [`MdioTransfer::bytes`]/[`MdioTransferResult::bytes`]
//!   as opaque bytes it does not interpret, matching
//!   [`crate::i2c::I2cByteTransfer`]'s own raw pass-through discipline.
//! - Wiring any of the below into an actual decoder, dispatch loop, or
//!   [`crate::avtp`]/[`crate::acf`]/[`crate::addressing`] caller — matching
//!   the discipline every prior Milestone 1-4/7 entry already established.
//!
//! ## Editorial note: MDIO's absence from the spec's own informative scope
//! statement
//!
//! Per Guiding Principle 5, and continuing the thread that principle itself
//! already names (spec ambiguities including "MDIO's scope-list omission"):
//! this crate's spec-extraction pass finds `ep_type 0x0D` fully normative in
//! the register map's own `ep_type` enumeration (see
//! [`crate::regmap::EndpointType::Mdio`]), yet the source spec's separate,
//! informative "ten interfaces" scope-statement prose does not count MDIO
//! among that ten. This module resolves that inconsistency by trusting the
//! normative enumeration over the informative prose count — building MDIO
//! support here rather than treating the scope-list omission as license to
//! skip it — and flags the discrepancy itself in this doc comment rather
//! than silently guessing which of the two conflicting spec passages is
//! authoritative.
//!
//! ## Divergence note: `mdio_mode` does **not** select Clause 22 vs Clause 45
//!
//! **This module's [`MdioAddressingMode`] contradicts TC18 and must not be
//! relied on for wire conformance.** TC18 §13.7.13.3 Table 57 "Usage of ABB
//! message for mdio requests" (TC18.txt line 5676) defines `mdio_mode` as an
//! MMD-vs-MMS access-kind and access-width selector, not an IEEE 802.3
//! clause selector:
//!
//! | `mdio_mode` | meaning (TC18 Table 57) |
//! |-------------|-------------------------|
//! | `01b`       | MMD, single word access |
//! | `01b` *(as printed — see below)* | MMD, multiple byte access |
//! | `10b`       | MMS, single word access |
//! | `11b`       | MMS, multiple (double) word access |
//!
//! Table 57 as printed lists `01b` twice and never lists `00b`, so one of
//! the two MMD rows is a spec typo whose intended code point (`00b` for one
//! of them) this crate cannot resolve from the text alone. Either way, the
//! `Clause22 = 0` / `Clause45 = 1` / `Spare2` / `Spare3` mapping below is
//! **wrong** against Table 57: TC18 assigns no `mdio_mode` value to a
//! Clause-22-vs-Clause-45 choice at all, and it leaves at most one code
//! point unallocated rather than two. Correcting this is a behavior change
//! deliberately not made in the requirements-completeness pass that
//! discovered it; the accompanying requirement entry records the divergence
//! as not-implemented, and the surrounding provenance note is retained below
//! only as the historical record of how the wrong mapping arose (it was
//! derived from `ROADMAP.md`'s restatement, never from TC18 itself).
//! Table 57 also fixes the payload widths this module does not model:
//! `mdio_address` "as per IEEE & OA SPI spec", and `mdio_payload` data
//! fields of 16 bits for MMD, 32 bits for MMS0 and MMS1, and 16 bits for
//! every other MMS.
//!
//! Two further §13.7.13 gaps are recorded as not-implemented entries: TC18
//! Figure 42's request-payload layout (line 5664: `reserved`, `mdio_mode`,
//! `mdio_address`, `mdio_payload`), which [`MdioTransfer`] carries opaque;
//! and TC18 Table 56's functional-config register layout (§13.7.13.2, line
//! 5639), whose §13.7.13.2 prose additionally states "The MDIO EP does not
//! have any configurable parameters" — which [`MdioFunctionalConfig`]'s own
//! `addressing_mode` field contradicts by carrying a per-EP-type
//! configurable parameter TC18 does not define.
//!
//! ## Provenance note: the two unallocated `mdio_mode` slots
//!
//! `ROADMAP.md`'s checklist bullet states `mdio_mode` selects between IEEE
//! 802.3 Clause 22 and Clause 45 addressing via a 2-bit selector, without
//! stating what (if anything) the other two of that selector's four
//! possible values mean. Per Guiding Principle 5, [`MdioAddressingMode`]
//! models the two named addressing modes as ordinarily named variants
//! ([`MdioAddressingMode::Clause22`]/[`MdioAddressingMode::Clause45`]), then
//! represents the two remaining 2-bit values this crate's own
//! spec-extraction pass found no named meaning for as two explicitly,
//! neutrally named variants, [`MdioAddressingMode::Spare2`] and
//! [`MdioAddressingMode::Spare3`], rather than silently folding them into
//! one of the two real addressing modes or rejecting them outright at
//! decode time — mirroring [`crate::spi::SpiChannelSelect::Spare6`]/
//! [`crate::spi::SpiChannelSelect::Spare7`]'s own treatment of SPI's two
//! spare `evt.sub_opcode` values, and
//! [`crate::gpio::GpioWriteSemantics::Unnamed8th`]'s own treatment of
//! GPIO's single unnamed write-semantics slot.
//! [`MdioAddressingMode::is_unallocated_slot`] lets a caller detect this
//! rather than treat either spare variant as a confirmed third or fourth
//! addressing mode. The specific `0..=3` wire-value assignment
//! [`MdioAddressingMode::to_u8`] uses — `Clause22 = 0`, `Clause45 = 1`, the
//! two spare values `2`/`3` — is this crate's own working choice (Clause 22
//! ordered first as IEEE 802.3's older, simpler addressing scheme), not a
//! transcription of a confirmed wire encoding. Resolving what (if anything)
//! the two spare values mean, and confirming the real `mdio_mode` wire
//! encoding, is left to errata reconciliation against confirmed wire
//! behavior — never against spec prose — matching every other still-open
//! provenance note in this crate.
//!
//! ## Provenance note: register-access framing is carried opaque
//!
//! `ROADMAP.md`'s checklist bullet names `mdio_mode` addressing-mode
//! selection and a minimal functional config, but states no register-access
//! wire framing of its own — no confirmed field layout for how a PHY
//! address, register address, device-type field (Clause 45's own
//! addressing extension), or read/write direction are carried within an
//! MDIO transfer. Per Guiding Principle 5, [`MdioTransfer`] and
//! [`MdioTransferResult`] do not attempt to parse any such structure out of
//! a transfer — the entire byte stream is carried as this module's
//! unstructured `Vec<u8>` shape, exactly as
//! [`crate::i2c::I2cByteTransfer`]/[`crate::i2c::I2cByteTransferResult`]
//! carry I²C's own address-plus-data stream without interpreting it. A
//! future item that needs to reason about PHY address, register address, or
//! device-type specifically can add that parsing later without this module
//! having guessed at a framing this crate cannot yet confirm.
//!
//! ## Relationship to [`crate::regmap`]
//!
//! As with every Milestone 4/7 endpoint-type module, MDIO's real
//! functional-config content gets its own dedicated type,
//! [`MdioFunctionalConfig`], rather than adding MDIO-specific fields
//! directly onto the still-shared, thirteen-endpoint-type
//! [`crate::regmap::PerEpTypeFunctionalConfig`] placeholder.
//! [`MdioFunctionalConfig::layer_tag`] shows how a caller obtains the
//! matching generic-layer tag so the two compose through
//! [`crate::regmap::check_functional_config_matches_ep_type`] exactly as
//! that cross-layer rule already expects, without this module editing
//! [`crate::regmap`] itself. Per this checklist bullet's explicit "minimal"
//! instruction, [`MdioFunctionalConfig`] carries the one `addressing_mode`
//! field that selection needs and nothing further — no empty placeholder
//! like [`crate::lin::LinFunctionalConfig`], but also no multi-field shape
//! like [`crate::iseled::IseledFunctionalConfig`].

use crate::RcpError;

// ── MdioAddressingMode ───────────────────────────────────────────────────────

/// The `mdio_mode` 2-bit addressing-mode selector this endpoint is
/// configured for.
///
/// See this module's doc comment "Provenance note: the two unallocated
/// `mdio_mode` slots" for why [`MdioAddressingMode::Spare2`]/
/// [`MdioAddressingMode::Spare3`] are deliberately left unresolved rather
/// than each given a specific addressing-mode meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
//fusa:req REQ-MDIO-001
pub enum MdioAddressingMode {
    /// IEEE 802.3 Clause 22 addressing: the simple, 5-bit PHY-address /
    /// 5-bit register-address scheme.
    Clause22 = 0,
    /// IEEE 802.3 Clause 45 addressing: the extended scheme adding a
    /// device-type field beyond Clause 22's simple PHY/register pair.
    Clause45 = 1,
    /// The first of `mdio_mode`'s two 2-bit values this crate's
    /// spec-extraction pass found no named addressing-mode meaning for. See
    /// this module's doc comment.
    Spare2 = 2,
    /// The second of `mdio_mode`'s two 2-bit values this crate's
    /// spec-extraction pass found no named addressing-mode meaning for. See
    /// this module's doc comment.
    Spare3 = 3,
}

impl MdioAddressingMode {
    /// Encode this addressing mode as its `mdio_mode` 2-bit wire value.
    //fusa:req REQ-MDIO-001
    pub fn to_u8(self) -> u8 {
        self as u8
    }

    /// Decode an `mdio_mode` wire byte value into an [`MdioAddressingMode`].
    ///
    /// Returns `Err(RcpError::InvalidParameter)` for any byte outside
    /// `0..=3` — `mdio_mode`'s full 2-bit range — matching
    /// [`crate::i2c::I2cSpeedMode::from_u8`]'s own range-check discipline.
    /// Never panics for any input.
    //fusa:req REQ-MDIO-002
    pub fn from_u8(raw: u8) -> Result<Self, RcpError> {
        match raw {
            0 => Ok(Self::Clause22),
            1 => Ok(Self::Clause45),
            2 => Ok(Self::Spare2),
            3 => Ok(Self::Spare3),
            _ => Err(RcpError::InvalidParameter),
        }
    }

    /// True for [`MdioAddressingMode::Spare2`]/[`MdioAddressingMode::Spare3`]
    /// — the two `mdio_mode` 2-bit values this module's doc comment flags as
    /// unallocated pending errata. False for
    /// [`MdioAddressingMode::Clause22`]/[`MdioAddressingMode::Clause45`].
    //fusa:req REQ-MDIO-003
    pub fn is_unallocated_slot(self) -> bool {
        matches!(self, Self::Spare2 | Self::Spare3)
    }
}

impl Default for MdioAddressingMode {
    /// Defaults to [`MdioAddressingMode::Clause22`] — IEEE 802.3's older,
    /// simpler addressing scheme, this module's own reasonable choice of
    /// the least capability-demanding named mode, not a confirmed power-on
    /// default from the source spec.
    fn default() -> Self {
        Self::Clause22
    }
}

// ── MdioFunctionalConfig ─────────────────────────────────────────────────────

/// MDIO's own per-EP-type functional-config content: this endpoint's
/// [`MdioAddressingMode`], and deliberately nothing else.
///
/// See this module's doc comment "Relationship to `crate::regmap`" for why
/// this stays a single-field type rather than growing a clock-divider or
/// further mode-select fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-MDIO-004
pub struct MdioFunctionalConfig {
    /// This endpoint's configured `mdio_mode` addressing-mode selector.
    pub addressing_mode: MdioAddressingMode,
}

impl MdioFunctionalConfig {
    /// The [`crate::regmap::PerEpTypeFunctionalConfig`] generic-layer tag
    /// that matches this MDIO functional config, for use with
    /// [`crate::regmap::check_functional_config_matches_ep_type`].
    ///
    /// This module does not itself call that function — it only shows how a
    /// caller would obtain the matching tag, per this module's doc comment
    /// "Relationship to `crate::regmap`".
    //fusa:req REQ-MDIO-004
    pub fn layer_tag(&self) -> crate::regmap::PerEpTypeFunctionalConfig {
        crate::regmap::PerEpTypeFunctionalConfig::new(crate::regmap::EndpointType::Mdio)
    }
}

// ── Raw MDIO register-access transfer ────────────────────────────────────────

/// A raw MDIO register-access transfer: the bytes an MDIO request sends
/// (PHY address, register/device-type addressing, and any write data, per
/// the configured [`MdioAddressingMode`]).
///
/// Modeled as an unstructured, variable-length byte stream — this module
/// does not interpret its contents — matching how
/// [`crate::i2c::I2cByteTransfer`] modeled its own address-plus-data byte
/// stream. See this module's doc comment "Provenance note: register-access
/// framing is carried opaque". Every possible byte slice, including an
/// empty one, has a valid encoding, so [`MdioTransfer::decode`] is
/// infallible.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
//fusa:req REQ-MDIO-005
pub struct MdioTransfer {
    /// The raw bytes sent for this MDIO register-access request, unparsed.
    pub bytes: Vec<u8>,
}

impl MdioTransfer {
    /// Encode this transfer to its raw wire representation: `bytes`,
    /// unmodified and unframed.
    //fusa:req REQ-MDIO-005
    pub fn encode(&self) -> Vec<u8> {
        self.bytes.clone()
    }

    /// Decode an [`MdioTransfer`] from a byte slice.
    ///
    /// Every possible byte slice, including an empty one, is a valid MDIO
    /// transfer, so this never fails and never panics for any input.
    //fusa:req REQ-MDIO-005
    pub fn decode(b: &[u8]) -> Self {
        Self { bytes: b.to_vec() }
    }
}

/// A raw MDIO register-access transfer result: the bytes an MDIO response
/// returns (read-back register data, per the configured
/// [`MdioAddressingMode`]).
///
/// See [`MdioTransfer`]'s doc comment — this is the same unstructured,
/// variable-length byte-stream modeling for the opposite transfer
/// direction.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
//fusa:req REQ-MDIO-006
pub struct MdioTransferResult {
    /// The raw bytes returned by this MDIO register-access response,
    /// unparsed.
    pub bytes: Vec<u8>,
}

impl MdioTransferResult {
    /// Encode this transfer result to its raw wire representation: `bytes`,
    /// unmodified and unframed.
    //fusa:req REQ-MDIO-006
    pub fn encode(&self) -> Vec<u8> {
        self.bytes.clone()
    }

    /// Decode an [`MdioTransferResult`] from a byte slice.
    ///
    /// Every possible byte slice, including an empty one, is a valid MDIO
    /// transfer result, so this never fails and never panics for any input.
    //fusa:req REQ-MDIO-006
    pub fn decode(b: &[u8]) -> Self {
        Self { bytes: b.to_vec() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── MdioAddressingMode: to_u8/from_u8 round-trip ────────────────────────

    const ALL_ADDRESSING_MODES: [MdioAddressingMode; 4] = [
        MdioAddressingMode::Clause22,
        MdioAddressingMode::Clause45,
        MdioAddressingMode::Spare2,
        MdioAddressingMode::Spare3,
    ];

    #[test]
    //fusa:test REQ-MDIO-001
    fn mdio_addressing_mode_round_trips_through_to_u8_from_u8_for_all_four_values() {
        for mode in ALL_ADDRESSING_MODES {
            let raw = mode.to_u8();
            assert_eq!(MdioAddressingMode::from_u8(raw), Ok(mode));
        }
    }

    #[test]
    //fusa:test REQ-MDIO-001
    fn mdio_addressing_mode_to_u8_values_are_the_full_2_bit_0_to_3_range() {
        let mut raws: Vec<u8> = ALL_ADDRESSING_MODES.iter().map(|m| m.to_u8()).collect();
        raws.sort_unstable();
        assert_eq!(raws, (0u8..=3).collect::<Vec<_>>());
    }

    #[test]
    //fusa:test REQ-MDIO-002
    fn mdio_addressing_mode_from_u8_rejects_out_of_range() {
        for raw in [4u8, 5, 0x7F, 0xFF] {
            assert_eq!(
                MdioAddressingMode::from_u8(raw),
                Err(RcpError::InvalidParameter)
            );
        }
    }

    #[test]
    //fusa:test REQ-MDIO-003
    fn mdio_addressing_mode_is_unallocated_slot_true_only_for_the_two_spare_values() {
        for mode in ALL_ADDRESSING_MODES {
            let expected = matches!(
                mode,
                MdioAddressingMode::Spare2 | MdioAddressingMode::Spare3
            );
            assert_eq!(mode.is_unallocated_slot(), expected);
        }
    }

    #[test]
    //fusa:test REQ-MDIO-003
    fn mdio_addressing_mode_default_is_clause22_and_not_unallocated() {
        let mode = MdioAddressingMode::default();
        assert_eq!(mode, MdioAddressingMode::Clause22);
        assert!(!mode.is_unallocated_slot());
    }

    // ── MdioFunctionalConfig / layer_tag ────────────────────────────────────

    #[test]
    //fusa:test REQ-MDIO-004
    fn mdio_functional_config_default_is_clause22_and_layer_tag_matches_ep_type_mdio() {
        let functional = MdioFunctionalConfig::default();
        assert_eq!(functional.addressing_mode, MdioAddressingMode::Clause22);

        let generic = crate::regmap::PerEpConfigBlock::new(crate::regmap::EndpointType::Mdio);
        let tag = functional.layer_tag();
        assert_eq!(tag.ep_type, crate::regmap::EndpointType::Mdio);
        assert!(crate::regmap::functional_config_matches_ep_type(
            &generic, &tag
        ));
        assert_eq!(
            crate::regmap::check_functional_config_matches_ep_type(&generic, &tag),
            Ok(())
        );
    }

    #[test]
    //fusa:test REQ-MDIO-004
    fn mdio_functional_config_layer_tag_rejects_mismatched_ep_type() {
        let functional = MdioFunctionalConfig {
            addressing_mode: MdioAddressingMode::Clause45,
        };
        let generic = crate::regmap::PerEpConfigBlock::new(crate::regmap::EndpointType::Iseled);
        let tag = functional.layer_tag();
        assert!(!crate::regmap::functional_config_matches_ep_type(
            &generic, &tag
        ));
    }

    // ── MdioTransfer / MdioTransferResult: round-trip, never panic ─────────

    #[test]
    //fusa:test REQ-MDIO-005
    fn mdio_transfer_round_trips_through_encode_decode_for_any_byte_slice() {
        for bytes in [
            vec![],
            vec![0x00u8],
            vec![0xFFu8],
            vec![0x03, 0x0A, 0x12, 0x34],
            (0u8..=255).collect::<Vec<_>>(),
        ] {
            let transfer = MdioTransfer {
                bytes: bytes.clone(),
            };
            assert_eq!(MdioTransfer::decode(&transfer.encode()).bytes, bytes);
        }
    }

    #[test]
    //fusa:test REQ-MDIO-005
    fn mdio_transfer_decode_never_panics_for_any_sampled_input() {
        for len in [0usize, 1, 2, 3, 9, 64] {
            let buf = vec![0x5Au8; len];
            let _ = MdioTransfer::decode(&buf);
        }
    }

    #[test]
    //fusa:test REQ-MDIO-006
    fn mdio_transfer_result_round_trips_through_encode_decode_for_any_byte_slice() {
        for bytes in [
            vec![],
            vec![0x00u8],
            vec![0xFFu8],
            vec![0xAB, 0xCD],
            (0u8..=255).collect::<Vec<_>>(),
        ] {
            let result = MdioTransferResult {
                bytes: bytes.clone(),
            };
            assert_eq!(MdioTransferResult::decode(&result.encode()).bytes, bytes);
        }
    }

    #[test]
    //fusa:test REQ-MDIO-006
    fn mdio_transfer_result_decode_never_panics_for_any_sampled_input() {
        for len in [0usize, 1, 2, 3, 9, 64] {
            let buf = vec![0x5Au8; len];
            let _ = MdioTransferResult::decode(&buf);
        }
    }
}
