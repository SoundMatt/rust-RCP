//fusa:req REQ-MDIO-001
//fusa:req REQ-MDIO-002
//fusa:req REQ-MDIO-003
//fusa:req REQ-MDIO-004
//fusa:req REQ-MDIO-005
//fusa:req REQ-MDIO-006
//fusa:req REQ-MDIO-007
//fusa:req REQ-MDIO-008

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
//! prior code. Two named pieces were originally in scope, both implemented
//! here; a third, [`MdioRequest`]/[`MdioRequest::from_evt_sub_opcode`], was
//! added afterward (see "Provenance note: evt[2:0] request validation"
//! below):
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
//! - [`MdioRequest`]/[`MdioRequest::from_evt_sub_opcode`] — MDIO's own
//!   request-decode entry point, validating an incoming request's
//!   `evt.sub_opcode` against [`crate::evtgroup::evt_row2_kind_of`]'s TC18
//!   §13.5 Table 33 Row-2 rule. See "Provenance note: evt[2:0] request
//!   validation" below — this piece was added after this module's own
//!   original two-piece scope note above (still accurate for why no
//!   `sub_opcode` reading existed here originally) as this crate's eighth
//!   and last Row-2 endpoint-type module, following
//!   [`crate::i2c::I2cRequest`]/[`crate::lin::LinRequest`]/
//!   [`crate::adc::AdcRequest`]/[`crate::pwm::PwmInRequest`]/
//!   [`crate::uart::UartRequest`]'s own prior applications of the same
//!   shared predicate and [`crate::can::CanRequest`]/
//!   [`crate::iseled::IseledRequest`]'s own deliberate departure from their
//!   shared `Ok(Self::ConfigWrite)` precedent for `evt[2:0] == 111b`. This
//!   module's own judgment call is explained in "Provenance note: evt[2:0]
//!   request validation" below — MDIO's [`MdioRequest::from_evt_sub_opcode`]
//!   takes an already-decoded [`MdioTransfer`], matching
//!   [`crate::can::CanRequest`]'s/[`crate::iseled::IseledRequest`]'s own
//!   signature shape, but keeps the majority `Ok(Self::ConfigWrite)`
//!   outcome rather than following their `Err` departure, for reasons
//!   specific to [`MdioTransfer`]'s own always-valid, uninterpreted-bytes
//!   shape. This closes out all eight Row-2 endpoint types
//!   (`{ADC, PWM_IN, I2C, LIN, CAN, UART, ISELED, MDIO}`).
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
//! - The "Groups A/B/C" `evt[2:0]` sub-opcode convention
//!   ([`crate::evtgroup::EvtGroup`]) as a general, cross-endpoint-type
//!   classification scheme — [`crate::evtgroup`]'s own doc comment already
//!   flags that broader scheme as unresolved, independent of the narrower,
//!   unambiguous Table 33 Row-2 rule this module's [`MdioRequest`] now
//!   implements (see "Provenance note: evt[2:0] request validation" below).
//! - Decoding [`MdioRequest::ConfigWrite`]'s own TC18 §12.7.1 payload shape.
//!   [`MdioRequest::from_evt_sub_opcode`] recognizes a config-write request
//!   as distinct from a [`Plain`](MdioRequest::Plain) one, but does not
//!   itself interpret what the config-write payload contains — that is
//!   separate, later work, same as [`crate::i2c::I2cRequest`]/
//!   [`crate::lin::LinRequest`]/[`crate::adc::AdcRequest`]/
//!   [`crate::pwm::PwmInRequest`]/[`crate::uart::UartRequest`]'s own
//!   identical `ConfigWrite` arms.
//! - Wiring [`MdioRequest::from_evt_sub_opcode`] into an actual decoder,
//!   dispatch loop, or [`crate::mock::Endpoint`] implementation.
//!   [`crate::mock::Endpoint`]'s own trait signature still does not carry an
//!   `evt` value to any implementation at all — that gap is not specific to
//!   MDIO, it applies identically to every other Row-2 endpoint-type
//!   module's own `from_evt_sub_opcode` (each confirmed still unwired
//!   against [`crate::mock::Endpoint`]'s own doc comment). [`MdioRequest`]
//!   is built to that same "additive standalone plumbing only" level.
//! - Wiring any of this module's other, original two pieces into an actual
//!   decoder, dispatch loop, or
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
//! ## Provenance note: evt[2:0] request validation
//!
//! MDIO is the eighth and last of the endpoint types TC18 §13.5 Table 33
//! groups into one shared "Row 2" `evt[2:0]` rule (TC18.txt lines
//! 4085-4092, `MDIO` itself named at line 4091) — see [`crate::evtgroup`]'s
//! own doc comment "Provenance note: TC18 §13.5 Table 33's Row-2 rule
//! (`evt_row2_kind_of`)" for the full citation, including the literal-text
//! discrepancy that module's doc comment flags and resolves (Table 33's own
//! printed Row-2 cell reads "000b to 110b reserved", including 000b, which
//! this crate does not implement literally). [`MdioRequest::from_evt_sub_opcode`]
//! is this module's own caller of that shared
//! [`crate::evtgroup::evt_row2_kind_of`] predicate — MDIO's own request
//! format (TC18 §13.7.13.3, Figure 43, TC18.txt line 6077) carries the same
//! `evt` field in its Message Info header every other endpoint type's
//! request does, and TC18 names no MDIO-specific override of Table 33's
//! generic rule anywhere in §13.7.13. (This citation is independently
//! verified against `TC18.txt` for this item — see this module's own doc
//! comment "Editorial note: pre-existing §13.7.13 citation drift" below for
//! why the *pre-existing* Table 57/Figure 42 citations elsewhere in this
//! module's "Divergence note" are a separate, not-yet-corrected matter.)
//!
//! **`MdioRequest::from_evt_sub_opcode` takes an already-decoded
//! [`MdioTransfer`], not raw `byte_msg_payload` bytes — matching
//! [`crate::can::CanRequest::from_evt_sub_opcode`]'s/
//! [`crate::iseled::IseledRequest::from_evt_sub_opcode`]'s own shape, not
//! [`crate::i2c::I2cRequest::from_evt_sub_opcode`]'s/
//! [`crate::lin::LinRequest::from_evt_sub_opcode`]'s/
//! [`crate::adc::AdcRequest::from_evt_sub_opcode`]'s/
//! [`crate::pwm::PwmInRequest::from_evt_sub_opcode`]'s/
//! [`crate::uart::UartRequest::from_evt_sub_opcode`]'s own raw-bytes
//! shape.** [`MdioTransfer`] already has its own dedicated decode entry
//! point, [`MdioTransfer::decode`] — pre-existing this item and unchanged by
//! it. Rather than [`MdioRequest::from_evt_sub_opcode`] re-deriving that
//! (trivial) byte-layout logic a second time internally, this function
//! instead requires its caller to have already called [`MdioTransfer::decode`]
//! and supply the resulting [`MdioTransfer`] directly — mirroring
//! [`crate::can::CanRequest::from_evt_sub_opcode`]'s/
//! [`crate::iseled::IseledRequest::from_evt_sub_opcode`]'s own identical
//! choice for [`crate::can::CanDataFrame`]/[`crate::iseled::IseledFrame`].
//!
//! **Unlike [`crate::can::CanRequest::from_evt_sub_opcode`]/
//! [`crate::iseled::IseledRequest::from_evt_sub_opcode`],
//! `MdioRequest::from_evt_sub_opcode` returns
//! `Ok(`[`MdioRequest::ConfigWrite`]`)` for `evt[2:0] == 111b`, following
//! [`crate::i2c::I2cRequest::from_evt_sub_opcode`]/
//! [`crate::lin::LinRequest::from_evt_sub_opcode`]/
//! [`crate::adc::AdcRequest::from_evt_sub_opcode`]/
//! [`crate::pwm::PwmInRequest::from_evt_sub_opcode`]/
//! [`crate::uart::UartRequest::from_evt_sub_opcode`]'s majority precedent
//! rather than [`crate::can::CanRequest`]'s/[`crate::iseled::IseledRequest`]'s
//! own `Err(`[`RcpError::ConfigWriteNotImplemented`]`)` departure — despite
//! sharing their already-decoded-frame signature shape.** This is a
//! deliberate, independent judgment call for MDIO, not a mechanical copy of
//! either prior precedent: `can.rs`'s and `iseled.rs`'s own departure rests
//! on their frame types making a real, specific structural claim about
//! their bytes that a genuine TC18 §12.7.1 config-write payload cannot
//! honestly satisfy — [`crate::can::CanDataFrame::decode`] parses a specific
//! `FrameFormat`/`id`/`data` shape (and can fail or, worse, silently
//! misinterpret bytes that are not really a CAN frame), and
//! [`crate::iseled::IseledFrame::decode`] parses a specific
//! `chain_address`/`command`/`data` shape and can itself fail with
//! [`RcpError::ShortFrame`]. Constructing either type from a real
//! config-write payload would either fail outright or silently mislabel
//! unrelated bytes as if they were a real data frame — the dishonesty their
//! own doc comments decline to paper over.
//!
//! [`MdioTransfer`] makes no such claim. Per this module's own doc comment
//! "Provenance note: register-access framing is carried opaque" above,
//! [`MdioTransfer::decode`] is infallible and totally uninterpreted — *every*
//! byte slice, including an empty one, is a valid [`MdioTransfer`], and the
//! type asserts nothing about what its `bytes` mean beyond "these are the
//! bytes of this request", exactly matching
//! [`crate::i2c::I2cByteTransfer`]'s own raw pass-through discipline (which
//! this module's own doc comment already cites `MdioTransfer` against). A
//! caller can therefore decode a genuine TC18 §12.7.1 config-write payload
//! through [`MdioTransfer::decode`] with zero information loss and zero
//! misrepresentation — unlike a [`CanDataFrame`](crate::can::CanDataFrame)
//! or an [`IseledFrame`](crate::iseled::IseledFrame), an [`MdioTransfer`]
//! never claims to be anything more than an opaque byte carrier, so wrapping
//! one in [`MdioRequest::ConfigWrite`] and discarding it (exactly as
//! [`crate::i2c::I2cRequest::from_evt_sub_opcode`] discards its own raw
//! `payload: &[u8]` for the identical case) is no less honest than any of
//! the five sibling modules that already return `Ok(Self::ConfigWrite)`.
//! The "no caller can honestly construct one" pressure that drove `can.rs`'s
//! and `iseled.rs`'s own departure from this precedent does not apply here.
//!
//! Every `Reserved` sub_opcode value (`evt[2:0]` in `001b..=110b`, or any
//! value outside the 3-bit field's representable range) is rejected with
//! `Err(`[`RcpError::UnsupportedCmd`]`)`, matching Table 33's own stated
//! error code and every prior Row-2 endpoint-type module's identical
//! refusal of their own table's reserved code — this part is unchanged
//! across all eight Row-2 endpoint-type modules, MDIO included.
//!
//! ## Editorial note: pre-existing §13.7.13 citation drift (out of scope for
//! this item)
//!
//! While adding the citations above, this module's own pre-existing
//! "Divergence note: `mdio_mode` does **not** select Clause 22 vs Clause 45"
//! section (above) was spot-checked against the current `TC18.txt` and found
//! to have drifted, the same way this crate's own `iseled.rs` found ISELED's
//! pre-existing citations had drifted (~400 lines) against a stale
//! reference-file version (see that module's own Table 30/33 Row-2 item).
//! Concretely: that section cites "Table 57" at "TC18.txt line 5676" and
//! "Figure 42" at "TC18.txt line 5664" for MDIO's own request format and
//! `mdio_mode` field table; against the current `TC18.txt`, MDIO's request
//! format is actually **Figure 43** (line 6077) and its `mdio_mode` field
//! table is actually **Table 60** ("Usage of ABB message for mdio
//! requests", line 6088) — both the line numbers *and* the table/figure
//! numbers have drifted, by roughly the same ~400-line/3-number offset
//! `iseled.rs` already found for its own section. The "Provenance note:
//! register-access framing is carried opaque" section's own "TC18 Table
//! 56" (line 5639) has the same issue: MDIO's functional-config register
//! layout is actually **Table 59** ("MDIO functional configuration", line
//! 6061). This item deliberately does **not** correct those pre-existing
//! citations — fixing them is a separate, later item, exactly as this
//! module's own instructions require; the citations newly added by this
//! item (Table 33 Row-2, TC18.txt L4085-4092; §13.7.13.3, TC18.txt L6065;
//! Figure 43, TC18.txt L6077) were independently re-verified against
//! `TC18.txt` rather than copied from this module's own pre-existing,
//! now-known-stale citations.
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

use crate::evtgroup::{evt_row2_kind_of, EvtRow2Kind};
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

// ── MdioRequest: evt[2:0] request validation ─────────────────────────────────

/// The decoded shape of an incoming MDIO request, after validating its
/// `evt[2:0]` sub-opcode against TC18 §13.5 Table 33's Row-2 rule (MDIO is
/// one of that row's eight endpoint types —
/// `{ADC, PWM_IN, I²C, LIN, CAN, UART, ISELED, MDIO}`).
///
/// See this module's doc comment "Provenance note: evt[2:0] request
/// validation" for the full citation, why
/// [`MdioRequest::from_evt_sub_opcode`] takes an already-decoded
/// [`MdioTransfer`] rather than raw `byte_msg_payload` bytes (matching
/// [`crate::can::CanRequest`]'s/[`crate::iseled::IseledRequest`]'s own
/// shape, not
/// [`crate::i2c::I2cRequest`]'s/[`crate::lin::LinRequest`]'s/
/// [`crate::adc::AdcRequest`]'s/[`crate::pwm::PwmInRequest`]'s/
/// [`crate::uart::UartRequest`]'s raw-bytes shape), and why it nonetheless
/// keeps those five siblings' `Ok(Self::ConfigWrite)` outcome rather than
/// following [`crate::can::CanRequest`]'s/[`crate::iseled::IseledRequest`]'s
/// own `Err(`[`RcpError::ConfigWriteNotImplemented`]`)` departure, and
/// [`crate::evtgroup`]'s own doc comment for the literal-text discrepancy
/// this crate resolves `evt[2:0] == 000b` against.
#[derive(Debug, Clone, PartialEq, Eq)]
//fusa:req REQ-MDIO-007
pub enum MdioRequest {
    /// `evt[2:0] == 000b`: an ordinary MDIO register-access request — the
    /// caller-decoded [`MdioTransfer`] this endpoint is to send onto, or has
    /// received from, the MDIO bus.
    Plain(MdioTransfer),
    /// `evt[2:0] == 111b`: a functional-config write (TC18 §12.7.1) rather
    /// than an ordinary transfer. This crate does not yet decode the
    /// config-write payload shape itself — see this module's doc comment
    /// "Deliberately out of scope" — so a caller receiving this variant
    /// knows only that the request *is* a config-write, not its content.
    ConfigWrite,
}

impl MdioRequest {
    /// Decode an incoming MDIO request from its `evt.sub_opcode`
    /// ([`crate::acf::Evt::sub_opcode`]) and an already-decoded
    /// [`MdioTransfer`] (see this module's doc comment "Provenance note:
    /// evt[2:0] request validation" for why this takes a decoded
    /// [`MdioTransfer`] rather than raw bytes, and why the `ConfigWrite`
    /// arm still returns `Ok`, unlike
    /// [`crate::can::CanRequest::from_evt_sub_opcode`]/
    /// [`crate::iseled::IseledRequest::from_evt_sub_opcode`]).
    ///
    /// Returns `Err(`[`RcpError::UnsupportedCmd`]`)` for every
    /// [`EvtRow2Kind::Reserved`] sub_opcode value — TC18 §13.5 Table 33's
    /// Row-2 rule requires the request be rejected with error code
    /// `UNSUPPORTED_CMD`, matching every prior Row-2 endpoint-type module's
    /// identical refusal of their own table's reserved code. Never panics
    /// for any `sub_opcode`/`transfer` combination.
    //fusa:req REQ-MDIO-007
    //fusa:req REQ-MDIO-008
    pub fn from_evt_sub_opcode(sub_opcode: u8, transfer: MdioTransfer) -> Result<Self, RcpError> {
        match evt_row2_kind_of(sub_opcode) {
            EvtRow2Kind::Plain => Ok(Self::Plain(transfer)),
            EvtRow2Kind::ConfigWrite => Ok(Self::ConfigWrite),
            EvtRow2Kind::Reserved => Err(RcpError::UnsupportedCmd),
        }
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

    // ── MdioRequest::from_evt_sub_opcode ─────────────────────────────────────

    fn sample_transfer() -> MdioTransfer {
        MdioTransfer {
            bytes: vec![0xDE, 0xAD, 0xBE, 0xEF],
        }
    }

    #[test]
    //fusa:test REQ-MDIO-007
    //fusa:test REQ-MDIO-008
    fn mdio_request_plain_evt_wraps_the_given_transfer_unchanged() {
        // Unlike I2cRequest::Plain/LinRequest::Plain/AdcRequest::Plain/
        // PwmInRequest::Plain/UartRequest::Write, MdioRequest::Plain does
        // not decode raw bytes itself — it threads the caller's
        // already-decoded MdioTransfer through unchanged, matching
        // CanRequest::Plain/IseledRequest::Plain. See this module's doc
        // comment "Provenance note: evt[2:0] request validation".
        let transfer = sample_transfer();
        let request = MdioRequest::from_evt_sub_opcode(0b000, transfer.clone()).unwrap();
        assert_eq!(request, MdioRequest::Plain(transfer));
    }

    #[test]
    //fusa:test REQ-MDIO-007
    //fusa:test REQ-MDIO-008
    fn mdio_request_plain_evt_accepts_an_empty_transfer() {
        let transfer = MdioTransfer { bytes: vec![] };
        let request = MdioRequest::from_evt_sub_opcode(0b000, transfer.clone()).unwrap();
        assert_eq!(request, MdioRequest::Plain(transfer));
    }

    #[test]
    //fusa:test REQ-MDIO-007
    //fusa:test REQ-MDIO-008
    fn mdio_request_config_write_evt_is_recognized_without_interpreting_transfer() {
        // Deliberate choice to keep the Ok(Self::ConfigWrite) precedent
        // I2cRequest/LinRequest/AdcRequest/PwmInRequest/UartRequest each
        // follow, rather than CanRequest's/IseledRequest's own
        // Err(RcpError::ConfigWriteNotImplemented) departure — see this
        // module's doc comment "Provenance note: evt[2:0] request
        // validation" for why MdioTransfer's always-valid, uninterpreted
        // bytes shape does not carry the same "no caller can honestly
        // construct one" pressure CanDataFrame/IseledFrame do. The given
        // transfer is not a real config-write payload — it is passed only
        // because the signature requires *some* MdioTransfer — and is not
        // echoed back or otherwise used.
        let request =
            MdioRequest::from_evt_sub_opcode(0b111, sample_transfer()).unwrap();
        assert_eq!(request, MdioRequest::ConfigWrite);
    }

    #[test]
    //fusa:test REQ-MDIO-008
    fn mdio_request_reserved_evt_values_are_rejected_with_unsupported_cmd() {
        for sub_opcode in 0b001..=0b110u8 {
            assert_eq!(
                MdioRequest::from_evt_sub_opcode(sub_opcode, sample_transfer()),
                Err(RcpError::UnsupportedCmd)
            );
        }
    }

    #[test]
    //fusa:test REQ-MDIO-008
    fn mdio_request_values_above_the_3_bit_field_are_also_rejected_with_unsupported_cmd() {
        for sub_opcode in (crate::acf::EVT_SUB_OPCODE_MAX + 1)..=u8::MAX {
            assert_eq!(
                MdioRequest::from_evt_sub_opcode(sub_opcode, sample_transfer()),
                Err(RcpError::UnsupportedCmd)
            );
        }
    }

    #[test]
    //fusa:test REQ-MDIO-008
    fn mdio_request_from_evt_sub_opcode_never_panics_for_any_sampled_input() {
        let transfers = [
            MdioTransfer { bytes: vec![] },
            sample_transfer(),
            MdioTransfer {
                bytes: vec![0xAAu8; 64],
            },
        ];
        for sub_opcode in 0..=u8::MAX {
            for transfer in &transfers {
                let _ = MdioRequest::from_evt_sub_opcode(sub_opcode, transfer.clone());
            }
        }
    }
}
