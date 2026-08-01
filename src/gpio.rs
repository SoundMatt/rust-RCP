//fusa:req REQ-GPIO-001
//fusa:req REQ-GPIO-002
//fusa:req REQ-GPIO-003
//fusa:req REQ-GPIO-004
//fusa:req REQ-GPIO-005
//fusa:req REQ-GPIO-006
//fusa:req REQ-GPIO-007
//fusa:req REQ-GPIO-008
//fusa:req REQ-GPIO-009
//fusa:req REQ-GPIO-010
//fusa:req REQ-GPIO-011
//fusa:req REQ-GPIO-012
//fusa:req REQ-GPIO-013
//fusa:req REQ-GPIO-014
//fusa:req REQ-GPIO-015
//fusa:req REQ-GPIO-016

//! The GPIO endpoint type (`ep_type 0x02`) — `ROADMAP.md` Milestone 4
//! ("Basic Endpoint Types"), first checklist bullet: "4-byte bitmask
//! read/write; the eight write-semantics (replace/OR/AND/XOR/add/
//! subtract-with-saturation/reconfigure); per-pin change/rising/falling
//! trigger signals."
//!
//! This is Milestone 4's opening item, chosen first per the milestone's own
//! Goal text ("prov[e] out the generic per-endpoint mechanics ... before
//! tackling bus-protocol endpoints") and because GPIO's bitmask model is the
//! simplest of the six endpoint types this milestone covers. Three named
//! pieces are in scope, all implemented here:
//!
//! - [`GpioBitmask`] — the 4-byte read/write bitmask shape itself, plus
//!   [`GpioBitmask::encode`]/[`GpioBitmask::decode`] giving it a
//!   never-panicking, fixed-length, big-endian wire form, matching every
//!   other Milestone 1-3 wire type's own encode/decode discipline (see e.g.
//!   [`crate::acf::ByteMessageInfo`]).
//! - [`GpioWriteSemantics`] — the eight write-semantics as an explicit enum,
//!   with [`apply_gpio_write`] giving each one a pure `(current, operand) ->
//!   new_value` function. See "Provenance note: the spec-reserved
//!   write-semantics code" below for why one of the eight variants
//!   ([`GpioWriteSemantics::Reserved4`]) is refused outright rather than
//!   given an operation.
//! - [`GpioTriggerConfig`]/[`GpioTriggerSignals`]/[`evaluate_gpio_triggers`]
//!   — per-pin change/rising/falling trigger-signal modeling: which pins
//!   have each of the three trigger kinds armed
//!   ([`GpioTriggerConfig`]), and, given a before/after bitmask pair, which
//!   pins actually fired ([`evaluate_gpio_triggers`] ->
//!   [`GpioTriggerSignals`]).
//!
//! Deliberately out of scope, per every prior Milestone 1-3 entry's own
//! discipline and per this milestone's own last checklist bullet ("Generic
//! `evt[2:0]` group conventions ... and the shared common functional-config
//! fields"), which is separate, later work within this same milestone:
//!
//! - The "Groups A/B/C" `evt[2:0]` sub-opcode convention as a general,
//!   cross-endpoint-type classification scheme. This module *does* read the
//!   already-generic 3-bit [`crate::acf::Evt::sub_opcode`] field — see
//!   "Provenance note: write-semantics selection via `evt.sub_opcode`"
//!   below — but only as GPIO's own private interpretation of a field that
//!   already exists from Milestone 1, not as an instance of the shared
//!   Groups A/B/C framework itself.
//! - [`crate::regmap::CommonFunctionalConfig`]'s fields (`ep_enable`,
//!   `ep_clear_req_storage`, `ep_req_crc_enable`) — this module neither adds
//!   to nor consumes that still-empty placeholder.
//! - Wiring any of the below into an actual decoder, dispatch loop, or
//!   [`crate::avtp`]/[`crate::acf`]/[`crate::addressing`] caller. This
//!   module remains additive standalone plumbing only, matching the
//!   discipline every prior Milestone 1-3 entry already established — there
//!   is no live dispatch loop anywhere in this crate yet.
//!
//! ## Relationship to [`crate::regmap`]
//!
//! [`crate::regmap::PerEpTypeFunctionalConfig`] is a deliberately empty,
//! [`crate::regmap::EndpointType`]-tagged placeholder shared by all thirteen
//! endpoint types (Milestone 2's "Register Map" work). Rather than turning
//! that generic, cross-endpoint-type shared struct into a thirteen-variant
//! enum on the strength of only one endpoint type's (GPIO's) concrete
//! content — which would misrepresent the other twelve endpoint types
//! (Milestones 4 and 7's remaining eleven, plus EP0) as scoped when they are
//! not — this module instead gives GPIO's real functional-config content
//! its own dedicated type, [`GpioFunctionalConfig`], and
//! [`GpioFunctionalConfig::layer_tag`] shows how a caller obtains the
//! matching [`crate::regmap::PerEpTypeFunctionalConfig`] generic-layer tag
//! for it, so the two compose through
//! [`crate::regmap::check_functional_config_matches_ep_type`] exactly as
//! that cross-layer rule already expects — without this module editing
//! [`crate::regmap`] itself.
//!
//! ## Provenance note: write-semantics selection via `evt.sub_opcode`
//!
//! [`crate::acf::Evt::sub_opcode`] is a 3-bit field
//! ([`crate::acf::EVT_SUB_OPCODE_MAX`] is `0x07`), which spans exactly eight
//! values — the same count `ROADMAP.md`'s GPIO checklist bullet names for
//! its write-semantics. This module reads that as GPIO selecting its write
//! semantics directly through the already-generic `sub_opcode` field
//! ([`GpioWriteSemantics::to_sub_opcode`]/
//! [`GpioWriteSemantics::from_sub_opcode`]) rather than inventing a
//! separate GPIO-private semantics-selector byte of its own. That
//! `sub_opcode` is the selecting field remains this crate's own working
//! interpretation, flagged per Guiding Principle 5, since `ROADMAP.md`
//! itself does not say so explicitly — it names the eight-way selection and
//! the eight semantics in the same breath but does not itself state that
//! `sub_opcode` is the selecting field. The specific `0..=7` numeric
//! assignment [`GpioWriteSemantics::to_sub_opcode`] gives each of the eight
//! values is not this crate's own choice, though: an earlier revision of
//! this module guessed a "roadmap listed order" placement for the four
//! higher codes that contradicted the GPIO/PWM_OUT endpoint-specific
//! evt-bits table in the OPEN Alliance TC18 Remote Control Protocol
//! Specification v0.5.1_RC (filed and corrected as issue #99); the mapping
//! below now follows that table's own numeric order for all eight values.
//!
//! ## Provenance note: the spec-reserved write-semantics code
//!
//! `ROADMAP.md`'s GPIO bullet states "the eight write-semantics" but then
//! names only seven in its parenthetical list: replace, OR, AND, XOR, add,
//! subtract-with-saturation, reconfigure — it does not itself say which
//! `sub_opcode` value is the unnamed eighth one, nor that one of the eight
//! is reserved rather than a real operation. Issue #99 pins that down: the
//! GPIO/PWM_OUT endpoint-specific evt-bits table in the OPEN Alliance TC18
//! Remote Control Protocol Specification v0.5.1_RC reserves `sub_opcode`
//! value `4` specifically — ahead of the add/subtract/reconfigure codes,
//! not after them as an earlier revision of this module guessed.
//! [`GpioWriteSemantics`] carries that reserved slot as an explicit
//! variant, [`GpioWriteSemantics::Reserved4`], so the type's `sub_opcode`
//! round-trip stays total over the field's full 3-bit range.
//! [`apply_gpio_write`] refuses (`Err(RcpError::UnsupportedCmd)`) for it
//! rather than treating it as an operation — see
//! [`GpioWriteSemantics::is_named`].
//!
//! ## Provenance note: `AddSaturating` and `Reconfigure`
//!
//! - `ROADMAP.md` names saturation only for the subtract semantics
//!   ("subtract-with-saturation"), not for add, and an earlier revision of
//!   this module read that as license to give add ordinary wrapping
//!   behavior instead. Issue #99 corrects that: the GPIO/PWM_OUT
//!   endpoint-specific evt-bits table in the OPEN Alliance TC18 Remote
//!   Control Protocol Specification v0.5.1_RC requires both the add and
//!   subtract write semantics to saturate at the field's bounds rather than
//!   wrap, so [`apply_gpio_write`] now saturates
//!   [`GpioWriteSemantics::AddSaturating`] with [`u32::saturating_add`],
//!   symmetric with [`GpioWriteSemantics::SubtractSaturating`]'s existing
//!   [`u32::saturating_sub`]. This module does not itself know what the
//!   specification's own field bounds are: [`GpioBitmask`] is a fixed
//!   4-byte/`u32` value established in an earlier milestone, and widening
//!   it to match some narrower spec-defined field width is out of scope for
//!   this fix. [`apply_gpio_write`] therefore saturates at `u32`'s own
//!   natural bounds (`0`/[`u32::MAX`]) — flagged per Guiding Principle 5 as
//!   this crate's own scope-narrowing choice, not a confirmed statement
//!   that the specification's field bounds are exactly `u32`'s.
//! - "Reconfigure" is named as a write semantics distinct from "replace",
//!   which this module reads as it most plausibly reconfiguring per-pin
//!   direction/function alongside (or instead of) writing output levels —
//!   a side effect entirely outside a pure bitmask-arithmetic function's
//!   reach. At the bitmask-value level modeled here, [`apply_gpio_write`]
//!   treats [`GpioWriteSemantics::Reconfigure`] identically to
//!   [`GpioWriteSemantics::Replace`] (the operand becomes the new value),
//!   flagged per Guiding Principle 5 as a placeholder pending reconciliation
//!   against the specification's actual behavior — not a claim that the two
//!   semantics are otherwise equivalent. Issue #99 only corrects
//!   `Reconfigure`'s `sub_opcode` position (`0x07`, not the earlier, wrong
//!   `0x06`); this module's placeholder-level treatment of what
//!   reconfiguring actually does at the bitmask-value level is unchanged.
//!
//! ## Provenance note: per-pin trigger modeling
//!
//! `ROADMAP.md`'s own Milestone 5 note elsewhere in this document already
//! flags "unpopulated trigger tables" as a known open ambiguity for this
//! crate's spec extraction. This module does not resolve that broader
//! ambiguity; it models only the one trigger shape GPIO's own checklist
//! bullet names — three independent per-pin enable masks (change/rising/
//! falling) plus edge detection between a before/after bitmask pair
//! ([`evaluate_gpio_triggers`]) — as this crate's own structural reading of
//! "per-pin change/rising/falling trigger signals," not a transcription of
//! a confirmed register layout or wire encoding for how a real RC Server
//! reports trigger occurrences.

use crate::RcpError;

// ── GpioBitmask ────────────────────────────────────────────────────────────

/// Length, in bytes, of the GPIO 4-byte read/write bitmask.
///
/// Four bytes is exactly 32 bit positions, matching TC18 §13.7.4.1's "Each
/// GPIO endpoint can handle up to 32 IOs".
//fusa:req REQ-GPIO-017
pub const GPIO_BITMASK_LEN: usize = 4;

/// The GPIO endpoint's 4-byte read/write bitmask: one bit per pin.
///
/// See this module's doc comment for the checklist wording this models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-GPIO-001
pub struct GpioBitmask(pub u32);

impl GpioBitmask {
    /// Encode this bitmask to its 4-byte big-endian wire representation.
    ///
    /// Big-endian places IO0 in the last byte's least-significant bit and
    /// IO31 in the first byte's most-significant bit, so IO `n` is bit `n`
    /// of the `byte_msg_payload` exactly as TC18 §13.7.4.1's Figure 24 lays
    /// the payload out (msb on the left, IO23..IO0 in the low positions for
    /// that figure's own 24-pin example endpoint).
    //fusa:req REQ-GPIO-001
    //fusa:req REQ-GPIO-018
    pub fn encode(self) -> [u8; GPIO_BITMASK_LEN] {
        self.0.to_be_bytes()
    }

    /// Decode a [`GpioBitmask`] from a byte slice.
    ///
    /// Never panics on short, truncated, or arbitrary input — always
    /// returns `Err(RcpError::ShortFrame)` for input shorter than
    /// [`GPIO_BITMASK_LEN`] instead. Trailing bytes beyond the first four
    /// are ignored, matching [`crate::acf::decode_byte_message_info`]'s own
    /// handling of a longer-than-required slice.
    //fusa:req REQ-GPIO-002
    pub fn decode(b: &[u8]) -> Result<Self, RcpError> {
        if b.len() < GPIO_BITMASK_LEN {
            return Err(RcpError::ShortFrame);
        }
        Ok(Self(u32::from_be_bytes([b[0], b[1], b[2], b[3]])))
    }
}

// ── GpioWriteSemantics ──────────────────────────────────────────────────────

/// The eight GPIO write-semantics, selected via [`crate::acf::Evt::sub_opcode`].
///
/// See this module's doc comment "Provenance note: write-semantics
/// selection via `evt.sub_opcode`", "Provenance note: the spec-reserved
/// write-semantics code", and "Provenance note: `AddSaturating` and
/// `Reconfigure`" for the working interpretations this type embodies.
/// These eight `evt[2:0]` codes are the endpoint's *complete* write
/// surface: TC18 §13.7.4.1 states plainly that "Generating a pulse for a
/// defined time is NOT a function of the GPIO endpoint" — it must instead be
/// managed by the Client via two commands, a compound, or a trigger
/// operation — so no duration-carrying ninth operation exists here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
//fusa:req REQ-GPIO-003
//fusa:req REQ-GPIO-020
pub enum GpioWriteSemantics {
    /// Overwrite the current value with the operand.
    Replace = 0,
    /// Bitwise OR the operand into the current value.
    Or = 1,
    /// Bitwise AND the operand into the current value.
    ///
    /// TC18 §13.7.4.1's prose words the value-changing operations as
    /// "(NAND, OR, XOR)", but TC18 §13.5's Table 30 — the normative
    /// per-endpoint `evt[2:0]` table — defines code `010b` as the
    /// *byte_msg_payload bitwise AND current interface status*, with the
    /// worked example "with a byte_msg_payload of 0xFFFF FFFE the first IO
    /// pin will be reset, while other IO pins remain unchanged". This crate
    /// follows Table 30's AND; §13.7.4.1's "NAND" wording contradicts it and
    /// is recorded as a TC18-internal inconsistency, not implemented as
    /// written.
    //fusa:req REQ-GPIO-021
    And = 2,
    /// Bitwise XOR the operand into the current value.
    Xor = 3,
    /// The GPIO/PWM_OUT endpoint-specific evt-bits table's reserved
    /// `sub_opcode` value. See this module's doc comment "Provenance note:
    /// the spec-reserved write-semantics code" — [`apply_gpio_write`]
    /// refuses this rather than treating it as an operation.
    Reserved4 = 4,
    /// Add the operand to the current value, saturating at [`u32::MAX`]
    /// rather than wrapping/overflowing. See this module's doc comment
    /// "Provenance note: `AddSaturating` and `Reconfigure`" for its
    /// saturating behavior and the `u32`-bounds scope-narrowing choice it
    /// makes.
    AddSaturating = 5,
    /// Subtract the operand from the current value, saturating at zero
    /// rather than wrapping/underflowing.
    SubtractSaturating = 6,
    /// Reconfigure this endpoint using the operand. See this module's doc
    /// comment "Provenance note: `AddSaturating` and `Reconfigure`" for how
    /// [`apply_gpio_write`] models this at the bitmask-value level.
    Reconfigure = 7,
}

impl GpioWriteSemantics {
    /// Encode this write semantics as its `evt.sub_opcode` value (`0..=7`).
    //fusa:req REQ-GPIO-003
    pub fn to_sub_opcode(self) -> u8 {
        self as u8
    }

    /// Decode an `evt.sub_opcode` value into a [`GpioWriteSemantics`].
    ///
    /// Returns `Err(RcpError::InvalidParameter)` for any value outside the
    /// 3-bit `sub_opcode` field's range
    /// (`> `[`crate::acf::EVT_SUB_OPCODE_MAX`]``). Never panics for any
    /// input.
    //fusa:req REQ-GPIO-004
    pub fn from_sub_opcode(raw: u8) -> Result<Self, RcpError> {
        match raw {
            0 => Ok(Self::Replace),
            1 => Ok(Self::Or),
            2 => Ok(Self::And),
            3 => Ok(Self::Xor),
            4 => Ok(Self::Reserved4),
            5 => Ok(Self::AddSaturating),
            6 => Ok(Self::SubtractSaturating),
            7 => Ok(Self::Reconfigure),
            _ => Err(RcpError::InvalidParameter),
        }
    }

    /// True for every variant except [`GpioWriteSemantics::Reserved4`] —
    /// the seven write-semantics `ROADMAP.md`'s checklist text actually
    /// names. See this module's doc comment "Provenance note: the
    /// spec-reserved write-semantics code".
    //fusa:req REQ-GPIO-009
    pub fn is_named(self) -> bool {
        !matches!(self, Self::Reserved4)
    }
}

/// Apply a [`GpioWriteSemantics`] write to `current`, producing the new
/// bitmask value.
///
/// Returns `Err(RcpError::UnsupportedCmd)` for
/// [`GpioWriteSemantics::Reserved4`] rather than treating it as an
/// operation — see this module's doc comment "Provenance note: the
/// spec-reserved write-semantics code". Never panics for any input,
/// including at the
/// [`GpioWriteSemantics::AddSaturating`]/[`GpioWriteSemantics::SubtractSaturating`]
/// `u32` saturation boundaries.
///
/// [`GpioWriteSemantics::Or`] with an all-zero operand is the identity, so
/// TC18 §13.7.4.3's "A request with data '0x0000 0000' and evt[2:0] = 0x001
/// (OR) results in 'no change'" holds for every current value.
//fusa:req REQ-GPIO-005
//fusa:req REQ-GPIO-006
//fusa:req REQ-GPIO-007
//fusa:req REQ-GPIO-008
//fusa:req REQ-GPIO-009
//fusa:req REQ-GPIO-010
//fusa:req REQ-GPIO-019
pub fn apply_gpio_write(
    semantics: GpioWriteSemantics,
    current: GpioBitmask,
    operand: GpioBitmask,
) -> Result<GpioBitmask, RcpError> {
    let (current, operand) = (current.0, operand.0);
    let result = match semantics {
        GpioWriteSemantics::Replace => operand,
        GpioWriteSemantics::Or => current | operand,
        GpioWriteSemantics::And => current & operand,
        GpioWriteSemantics::Xor => current ^ operand,
        GpioWriteSemantics::Reserved4 => return Err(RcpError::UnsupportedCmd),
        GpioWriteSemantics::AddSaturating => current.saturating_add(operand),
        GpioWriteSemantics::SubtractSaturating => current.saturating_sub(operand),
        GpioWriteSemantics::Reconfigure => operand,
    };
    Ok(GpioBitmask(result))
}

// ── Per-pin trigger signals ─────────────────────────────────────────────────

/// Length, in bytes, of the encoded [`GpioTriggerConfig`]: three 4-byte
/// per-pin enable masks (change, rising, falling).
pub const GPIO_TRIGGER_CONFIG_LEN: usize = 3 * GPIO_BITMASK_LEN;

/// Per-pin trigger-signal arming: which pins have each of the three trigger
/// kinds enabled.
///
/// See this module's doc comment "Provenance note: per-pin trigger
/// modeling".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-GPIO-011
pub struct GpioTriggerConfig {
    /// Per-pin bitmask: change (either-edge) trigger enabled.
    pub change_enable: GpioBitmask,
    /// Per-pin bitmask: rising-edge (`0` -> `1`) trigger enabled.
    pub rising_enable: GpioBitmask,
    /// Per-pin bitmask: falling-edge (`1` -> `0`) trigger enabled.
    pub falling_enable: GpioBitmask,
}

impl GpioTriggerConfig {
    /// Encode this config to its 12-byte big-endian wire representation:
    /// `change_enable` then `rising_enable` then `falling_enable`, each
    /// 4 bytes.
    //fusa:req REQ-GPIO-011
    pub fn encode(self) -> [u8; GPIO_TRIGGER_CONFIG_LEN] {
        let mut buf = [0u8; GPIO_TRIGGER_CONFIG_LEN];
        buf[0..4].copy_from_slice(&self.change_enable.encode());
        buf[4..8].copy_from_slice(&self.rising_enable.encode());
        buf[8..12].copy_from_slice(&self.falling_enable.encode());
        buf
    }

    /// Decode a [`GpioTriggerConfig`] from a byte slice.
    ///
    /// Never panics on short, truncated, or arbitrary input — always
    /// returns `Err(RcpError::ShortFrame)` for input shorter than
    /// [`GPIO_TRIGGER_CONFIG_LEN`] instead.
    //fusa:req REQ-GPIO-012
    pub fn decode(b: &[u8]) -> Result<Self, RcpError> {
        if b.len() < GPIO_TRIGGER_CONFIG_LEN {
            return Err(RcpError::ShortFrame);
        }
        Ok(Self {
            change_enable: GpioBitmask::decode(&b[0..4])?,
            rising_enable: GpioBitmask::decode(&b[4..8])?,
            falling_enable: GpioBitmask::decode(&b[8..12])?,
        })
    }
}

/// Which pins actually fired a trigger between one [`GpioBitmask`] sample
/// and the next, as reported by [`evaluate_gpio_triggers`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-GPIO-013
pub struct GpioTriggerSignals {
    /// Per-pin bitmask: this pin changed and had `change_enable` armed.
    pub changed: GpioBitmask,
    /// Per-pin bitmask: this pin rose (`0` -> `1`) and had `rising_enable`
    /// armed.
    pub rising: GpioBitmask,
    /// Per-pin bitmask: this pin fell (`1` -> `0`) and had `falling_enable`
    /// armed.
    pub falling: GpioBitmask,
}

/// Compute which per-pin trigger signals fire between `previous` and
/// `current`, given `config`'s per-pin arming.
///
/// A pin's bit contributes to [`GpioTriggerSignals::changed`] only if it
/// differs between `previous`/`current` *and* has `change_enable` armed for
/// that pin in `config` — likewise for `rising`/`falling`, each additionally
/// gated on the transition direction. Never panics for any input.
//fusa:req REQ-GPIO-013
//fusa:req REQ-GPIO-014
//fusa:req REQ-GPIO-015
pub fn evaluate_gpio_triggers(
    config: &GpioTriggerConfig,
    previous: GpioBitmask,
    current: GpioBitmask,
) -> GpioTriggerSignals {
    let diff = previous.0 ^ current.0;
    // Bits that differ and are now set: 0 -> 1 (rising) transitions.
    let rose = diff & current.0;
    // Bits that differ and were previously set: 1 -> 0 (falling) transitions.
    let fell = diff & previous.0;

    GpioTriggerSignals {
        changed: GpioBitmask(diff & config.change_enable.0),
        rising: GpioBitmask(rose & config.rising_enable.0),
        falling: GpioBitmask(fell & config.falling_enable.0),
    }
}

// ── GpioFunctionalConfig ─────────────────────────────────────────────────────

/// GPIO's own per-EP-type functional-config content: this endpoint's
/// [`GpioTriggerConfig`] arming.
///
/// See this module's doc comment "Relationship to `crate::regmap`" for why
/// this is a dedicated type rather than content added directly to
/// [`crate::regmap::PerEpTypeFunctionalConfig`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-GPIO-016
pub struct GpioFunctionalConfig {
    /// This endpoint's per-pin trigger arming.
    pub trigger: GpioTriggerConfig,
}

impl GpioFunctionalConfig {
    /// The [`crate::regmap::PerEpTypeFunctionalConfig`] generic-layer tag
    /// that matches this GPIO functional config, for use with
    /// [`crate::regmap::check_functional_config_matches_ep_type`].
    ///
    /// This module does not itself call that function — it only shows how a
    /// caller would obtain the matching tag, per this module's doc comment
    /// "Relationship to `crate::regmap`".
    //fusa:req REQ-GPIO-016
    pub fn layer_tag(&self) -> crate::regmap::PerEpTypeFunctionalConfig {
        crate::regmap::PerEpTypeFunctionalConfig::new(crate::regmap::EndpointType::Gpio)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── GpioBitmask: round-trip / never-panic ───────────────────────────────

    #[test]
    //fusa:test REQ-GPIO-001
    fn gpio_bitmask_round_trips_through_encode_decode() {
        for raw in [0u32, 1, 0xFFFF_FFFF, 0x8000_0001, 0x0102_0304] {
            let mask = GpioBitmask(raw);
            let decoded = GpioBitmask::decode(&mask.encode()).unwrap();
            assert_eq!(decoded, mask);
        }
    }

    #[test]
    //fusa:test REQ-GPIO-001
    fn gpio_bitmask_encode_is_big_endian() {
        let mask = GpioBitmask(0x0102_0304);
        assert_eq!(mask.encode(), [0x01, 0x02, 0x03, 0x04]);
    }

    #[test]
    //fusa:test REQ-GPIO-002
    fn gpio_bitmask_decode_rejects_short_input() {
        for len in 0..GPIO_BITMASK_LEN {
            let short = vec![0xAAu8; len];
            assert_eq!(GpioBitmask::decode(&short), Err(RcpError::ShortFrame));
        }
    }

    #[test]
    //fusa:test REQ-GPIO-002
    fn gpio_bitmask_decode_ignores_trailing_bytes() {
        let b = [0x00, 0x00, 0x00, 0x2A, 0xFF, 0xFF];
        assert_eq!(GpioBitmask::decode(&b).unwrap(), GpioBitmask(42));
    }

    // ── GpioWriteSemantics: sub_opcode round-trip ───────────────────────────

    const ALL_WRITE_SEMANTICS: [GpioWriteSemantics; 8] = [
        GpioWriteSemantics::Replace,
        GpioWriteSemantics::Or,
        GpioWriteSemantics::And,
        GpioWriteSemantics::Xor,
        GpioWriteSemantics::Reserved4,
        GpioWriteSemantics::AddSaturating,
        GpioWriteSemantics::SubtractSaturating,
        GpioWriteSemantics::Reconfigure,
    ];

    #[test]
    //fusa:test REQ-GPIO-003
    fn gpio_write_semantics_sub_opcode_round_trips_for_all_eight_values() {
        for semantics in ALL_WRITE_SEMANTICS {
            let raw = semantics.to_sub_opcode();
            assert!(raw <= crate::acf::EVT_SUB_OPCODE_MAX);
            assert_eq!(GpioWriteSemantics::from_sub_opcode(raw), Ok(semantics));
        }
    }

    #[test]
    //fusa:test REQ-GPIO-003
    fn gpio_write_semantics_sub_opcode_values_are_the_full_0_to_7_range() {
        let mut raws: Vec<u8> = ALL_WRITE_SEMANTICS
            .iter()
            .map(|s| s.to_sub_opcode())
            .collect();
        raws.sort_unstable();
        assert_eq!(raws, (0u8..=7).collect::<Vec<_>>());
    }

    #[test]
    //fusa:test REQ-GPIO-004
    fn gpio_write_semantics_from_sub_opcode_rejects_out_of_range() {
        for raw in [8u8, 9, 0x7F, 0xFF] {
            assert_eq!(
                GpioWriteSemantics::from_sub_opcode(raw),
                Err(RcpError::InvalidParameter)
            );
        }
    }

    #[test]
    //fusa:test REQ-GPIO-009
    fn gpio_write_semantics_is_named_true_only_for_the_seven_named_variants() {
        for semantics in ALL_WRITE_SEMANTICS {
            assert_eq!(
                semantics.is_named(),
                semantics != GpioWriteSemantics::Reserved4
            );
        }
    }

    // ── apply_gpio_write: per-semantics correctness ─────────────────────────

    #[test]
    //fusa:test REQ-GPIO-005
    fn apply_gpio_write_replace_overwrites_current() {
        let result = apply_gpio_write(
            GpioWriteSemantics::Replace,
            GpioBitmask(0xFFFF_FFFF),
            GpioBitmask(0x1234),
        );
        assert_eq!(result, Ok(GpioBitmask(0x1234)));
    }

    #[test]
    //fusa:test REQ-GPIO-005
    fn apply_gpio_write_or_and_xor_match_bitwise_ops() {
        let current = GpioBitmask(0b1010);
        let operand = GpioBitmask(0b0110);
        assert_eq!(
            apply_gpio_write(GpioWriteSemantics::Or, current, operand),
            Ok(GpioBitmask(0b1010 | 0b0110))
        );
        assert_eq!(
            apply_gpio_write(GpioWriteSemantics::And, current, operand),
            Ok(GpioBitmask(0b1010 & 0b0110))
        );
        assert_eq!(
            apply_gpio_write(GpioWriteSemantics::Xor, current, operand),
            Ok(GpioBitmask(0b1010 ^ 0b0110))
        );
    }

    #[test]
    //fusa:test REQ-GPIO-006
    fn apply_gpio_write_add_saturating_clamps_at_u32_max() {
        let result = apply_gpio_write(
            GpioWriteSemantics::AddSaturating,
            GpioBitmask(u32::MAX),
            GpioBitmask(1),
        );
        assert_eq!(result, Ok(GpioBitmask(u32::MAX)));
    }

    #[test]
    //fusa:test REQ-GPIO-006
    fn apply_gpio_write_add_saturating_exact_u32_max_boundary() {
        let result = apply_gpio_write(
            GpioWriteSemantics::AddSaturating,
            GpioBitmask(u32::MAX - 10),
            GpioBitmask(10),
        );
        assert_eq!(result, Ok(GpioBitmask(u32::MAX)));
    }

    #[test]
    //fusa:test REQ-GPIO-006
    fn apply_gpio_write_add_saturating_ordinary_case() {
        let result = apply_gpio_write(
            GpioWriteSemantics::AddSaturating,
            GpioBitmask(5),
            GpioBitmask(10),
        );
        assert_eq!(result, Ok(GpioBitmask(15)));
    }

    #[test]
    //fusa:test REQ-GPIO-007
    fn apply_gpio_write_subtract_saturating_clamps_at_zero() {
        let result = apply_gpio_write(
            GpioWriteSemantics::SubtractSaturating,
            GpioBitmask(5),
            GpioBitmask(10),
        );
        assert_eq!(result, Ok(GpioBitmask(0)));
    }

    #[test]
    //fusa:test REQ-GPIO-007
    fn apply_gpio_write_subtract_saturating_exact_zero_boundary() {
        let result = apply_gpio_write(
            GpioWriteSemantics::SubtractSaturating,
            GpioBitmask(10),
            GpioBitmask(10),
        );
        assert_eq!(result, Ok(GpioBitmask(0)));
    }

    #[test]
    //fusa:test REQ-GPIO-007
    fn apply_gpio_write_subtract_saturating_ordinary_case() {
        let result = apply_gpio_write(
            GpioWriteSemantics::SubtractSaturating,
            GpioBitmask(10),
            GpioBitmask(3),
        );
        assert_eq!(result, Ok(GpioBitmask(7)));
    }

    #[test]
    //fusa:test REQ-GPIO-008
    fn apply_gpio_write_reconfigure_matches_replace_at_the_bitmask_level() {
        let current = GpioBitmask(0xFFFF_FFFF);
        let operand = GpioBitmask(0x1234);
        assert_eq!(
            apply_gpio_write(GpioWriteSemantics::Reconfigure, current, operand),
            apply_gpio_write(GpioWriteSemantics::Replace, current, operand)
        );
    }

    #[test]
    //fusa:test REQ-GPIO-009
    fn apply_gpio_write_refuses_the_spec_reserved_semantics() {
        let result = apply_gpio_write(
            GpioWriteSemantics::Reserved4,
            GpioBitmask(0),
            GpioBitmask(0),
        );
        assert_eq!(result, Err(RcpError::UnsupportedCmd));
    }

    #[test]
    //fusa:test REQ-GPIO-010
    fn apply_gpio_write_never_panics_for_any_sampled_input() {
        let samples = [0u32, 1, 2, 0x7FFF_FFFF, 0x8000_0000, 0xFFFF_FFFE, u32::MAX];
        for semantics in ALL_WRITE_SEMANTICS {
            for &current in &samples {
                for &operand in &samples {
                    let _ = apply_gpio_write(semantics, GpioBitmask(current), GpioBitmask(operand));
                }
            }
        }
    }

    // ── GpioTriggerConfig: round-trip / never-panic ─────────────────────────

    #[test]
    //fusa:test REQ-GPIO-011
    fn gpio_trigger_config_round_trips_through_encode_decode() {
        let config = GpioTriggerConfig {
            change_enable: GpioBitmask(0x1111_1111),
            rising_enable: GpioBitmask(0x2222_2222),
            falling_enable: GpioBitmask(0x3333_3333),
        };
        let decoded = GpioTriggerConfig::decode(&config.encode()).unwrap();
        assert_eq!(decoded, config);
    }

    #[test]
    //fusa:test REQ-GPIO-011
    fn gpio_trigger_config_encode_field_order_is_change_rising_falling() {
        let config = GpioTriggerConfig {
            change_enable: GpioBitmask(0x0000_0001),
            rising_enable: GpioBitmask(0x0000_0002),
            falling_enable: GpioBitmask(0x0000_0003),
        };
        let encoded = config.encode();
        assert_eq!(&encoded[0..4], &[0, 0, 0, 1]);
        assert_eq!(&encoded[4..8], &[0, 0, 0, 2]);
        assert_eq!(&encoded[8..12], &[0, 0, 0, 3]);
    }

    #[test]
    //fusa:test REQ-GPIO-012
    fn gpio_trigger_config_decode_rejects_short_input() {
        for len in 0..GPIO_TRIGGER_CONFIG_LEN {
            let short = vec![0xAAu8; len];
            assert_eq!(GpioTriggerConfig::decode(&short), Err(RcpError::ShortFrame));
        }
    }

    // ── evaluate_gpio_triggers: edge detection + arming ─────────────────────

    #[test]
    //fusa:test REQ-GPIO-013
    fn evaluate_gpio_triggers_detects_rising_edge_on_armed_pin() {
        let config = GpioTriggerConfig {
            change_enable: GpioBitmask(0xFFFF_FFFF),
            rising_enable: GpioBitmask(0xFFFF_FFFF),
            falling_enable: GpioBitmask(0xFFFF_FFFF),
        };
        let signals = evaluate_gpio_triggers(&config, GpioBitmask(0), GpioBitmask(1));
        assert_eq!(signals.changed, GpioBitmask(1));
        assert_eq!(signals.rising, GpioBitmask(1));
        assert_eq!(signals.falling, GpioBitmask(0));
    }

    #[test]
    //fusa:test REQ-GPIO-013
    fn evaluate_gpio_triggers_detects_falling_edge_on_armed_pin() {
        let config = GpioTriggerConfig {
            change_enable: GpioBitmask(0xFFFF_FFFF),
            rising_enable: GpioBitmask(0xFFFF_FFFF),
            falling_enable: GpioBitmask(0xFFFF_FFFF),
        };
        let signals = evaluate_gpio_triggers(&config, GpioBitmask(1), GpioBitmask(0));
        assert_eq!(signals.changed, GpioBitmask(1));
        assert_eq!(signals.rising, GpioBitmask(0));
        assert_eq!(signals.falling, GpioBitmask(1));
    }

    #[test]
    //fusa:test REQ-GPIO-013
    fn evaluate_gpio_triggers_no_signal_when_value_unchanged() {
        let config = GpioTriggerConfig {
            change_enable: GpioBitmask(0xFFFF_FFFF),
            rising_enable: GpioBitmask(0xFFFF_FFFF),
            falling_enable: GpioBitmask(0xFFFF_FFFF),
        };
        let signals = evaluate_gpio_triggers(&config, GpioBitmask(0x5555), GpioBitmask(0x5555));
        assert_eq!(signals, GpioTriggerSignals::default());
    }

    #[test]
    //fusa:test REQ-GPIO-014
    fn evaluate_gpio_triggers_masks_out_disarmed_pins() {
        // Pin 0 rises but only pin 1's rising trigger is armed.
        let config = GpioTriggerConfig {
            change_enable: GpioBitmask(0),
            rising_enable: GpioBitmask(0b10),
            falling_enable: GpioBitmask(0),
        };
        let signals = evaluate_gpio_triggers(&config, GpioBitmask(0b00), GpioBitmask(0b01));
        assert_eq!(signals, GpioTriggerSignals::default());
    }

    #[test]
    //fusa:test REQ-GPIO-014
    fn evaluate_gpio_triggers_per_pin_independence() {
        // Pin 0 rises (armed), pin 1 falls (not armed for falling).
        let config = GpioTriggerConfig {
            change_enable: GpioBitmask(0xFFFF_FFFF),
            rising_enable: GpioBitmask(0b01),
            falling_enable: GpioBitmask(0b00),
        };
        let signals = evaluate_gpio_triggers(&config, GpioBitmask(0b10), GpioBitmask(0b01));
        assert_eq!(signals.changed, GpioBitmask(0b11));
        assert_eq!(signals.rising, GpioBitmask(0b01));
        assert_eq!(signals.falling, GpioBitmask(0b00));
    }

    #[test]
    //fusa:test REQ-GPIO-015
    fn evaluate_gpio_triggers_never_panics_for_any_sampled_input() {
        let samples = [0u32, 1, 0x5555_5555, 0xAAAA_AAAA, 0x8000_0000, u32::MAX];
        let config = GpioTriggerConfig {
            change_enable: GpioBitmask(0xFFFF_0000),
            rising_enable: GpioBitmask(0x0F0F_0F0F),
            falling_enable: GpioBitmask(0xF0F0_F0F0),
        };
        for &previous in &samples {
            for &current in &samples {
                let _ =
                    evaluate_gpio_triggers(&config, GpioBitmask(previous), GpioBitmask(current));
            }
        }
    }

    // ── GpioFunctionalConfig / crate::regmap composition ────────────────────

    #[test]
    //fusa:test REQ-GPIO-016
    fn gpio_functional_config_layer_tag_matches_ep_type_gpio() {
        let functional = GpioFunctionalConfig::default();
        let generic = crate::regmap::PerEpConfigBlock::new(crate::regmap::EndpointType::Gpio);
        let tag = functional.layer_tag();
        assert_eq!(tag.ep_type, crate::regmap::EndpointType::Gpio);
        assert!(crate::regmap::functional_config_matches_ep_type(
            &generic, &tag
        ));
        assert_eq!(
            crate::regmap::check_functional_config_matches_ep_type(&generic, &tag),
            Ok(())
        );
    }

    // ── TC18 §13.7.4 spec-literal conformance checks ────────────────────────

    /// TC18 §13.7.4.1 (TC18.txt line 4381): "Each GPIO endpoint can handle
    /// up to 32 IOs." The 4-byte bitmask is exactly 32 bit positions wide,
    /// so IO0..IO31 are all representable and there is no IO32.
    #[test]
    //fusa:test REQ-GPIO-017
    fn gpio_bitmask_spans_exactly_the_32_ios_tc18_admits() {
        // 4 bytes x 8 bits = the 32 IO positions TC18 §13.7.4.1 names.
        assert_eq!(GPIO_BITMASK_LEN, 4);
        assert_eq!(GPIO_BITMASK_LEN * 8, 32);
        // IO31 — the highest IO a 32-IO endpoint has — is representable and
        // round-trips.
        let io31 = GpioBitmask(1u32 << 31);
        assert_eq!(io31.encode(), [0x80, 0x00, 0x00, 0x00]);
        assert_eq!(GpioBitmask::decode(&io31.encode()), Ok(io31));
        // All 32 IOs asserted at once is the all-ones 4-byte payload.
        assert_eq!(GpioBitmask(u32::MAX).encode(), [0xFF, 0xFF, 0xFF, 0xFF]);
        // A fifth payload byte would be outside the endpoint's 32-IO reach:
        // decode reads exactly four.
        assert_eq!(
            GpioBitmask::decode(&[0x80, 0x00, 0x00, 0x00, 0xFF]),
            Ok(io31)
        );
    }

    /// TC18 §13.7.4.1 / Figure 24 (TC18.txt lines 4389-4398): "Each GPIO pin
    /// can be assigned to a bit position of the byte_msg_payload", laid out
    /// msb-first with IO23..IO0 in the low bit positions for that figure's
    /// own 24-pin example endpoint. IO `n` is therefore bit `n`, and the
    /// 4-byte payload is big-endian.
    #[test]
    //fusa:test REQ-GPIO-018
    fn gpio_bitmask_maps_io_n_to_bit_n_per_tc18_figure_24() {
        // IO0 is the least-significant bit: last payload byte, bit 0.
        assert_eq!(GpioBitmask(1u32).encode(), [0x00, 0x00, 0x00, 0x01]);
        // IO7 completes the last payload byte.
        assert_eq!(GpioBitmask(1u32 << 7).encode(), [0x00, 0x00, 0x00, 0x80]);
        // IO8 opens the next byte up.
        assert_eq!(GpioBitmask(1u32 << 8).encode(), [0x00, 0x00, 0x01, 0x00]);
        // IO23 — the highest IO drawn in Figure 24's 24-pin example.
        assert_eq!(GpioBitmask(1u32 << 23).encode(), [0x00, 0x80, 0x00, 0x00]);
        // Figure 24's whole 24-pin example EP: IO0..IO23 asserted, the
        // "don't care" positions above IO23 clear.
        assert_eq!(GpioBitmask(0x00FF_FFFF).encode(), [0x00, 0xFF, 0xFF, 0xFF]);
        // IO31 tops out the 32-IO range.
        assert_eq!(GpioBitmask(1u32 << 31).encode(), [0x80, 0x00, 0x00, 0x00]);
    }

    /// TC18 §13.7.4.3 (TC18.txt line 4480): "A request with data
    /// '0x0000 0000' and evt[2:0] = 0x001 (OR) results in 'no change'."
    #[test]
    //fusa:test REQ-GPIO-019
    fn gpio_or_with_all_zero_payload_is_no_change_per_tc18_13_7_4_3() {
        // evt[2:0] = 0x001 is the OR semantics.
        assert_eq!(
            GpioWriteSemantics::from_sub_opcode(0x001),
            Ok(GpioWriteSemantics::Or)
        );
        // The literal 0x0000_0000 payload leaves every current interface
        // state untouched, bit for bit.
        for current in [0x0000_0000u32, 0x0000_0002, 0x5555_5555, 0xFFFF_FFFF] {
            assert_eq!(
                apply_gpio_write(
                    GpioWriteSemantics::Or,
                    GpioBitmask(current),
                    GpioBitmask(0x0000_0000)
                ),
                Ok(GpioBitmask(current))
            );
        }
    }

    /// TC18 §13.7.4.1 (TC18.txt lines 4403-4404): "Generating a pulse for a
    /// defined time is NOT a function of the GPIO endpoint. This either
    /// needs to be managed by the Client ... by a compound or a trigger
    /// operation." The endpoint's whole write surface is therefore exactly
    /// TC18 §13.5 Table 30's eight `evt[2:0]` value-level codes, with no
    /// ninth, duration-carrying operation.
    #[test]
    //fusa:test REQ-GPIO-020
    fn gpio_write_surface_is_tc18_eight_codes_with_no_pulse_operation() {
        // TC18 §13.5 Table 30's GPIO/PWM_OUT rows, code by code.
        let table_30: [(u8, GpioWriteSemantics); 8] = [
            (0b000, GpioWriteSemantics::Replace),
            (0b001, GpioWriteSemantics::Or),
            (0b010, GpioWriteSemantics::And),
            (0b011, GpioWriteSemantics::Xor),
            (0b100, GpioWriteSemantics::Reserved4),
            (0b101, GpioWriteSemantics::AddSaturating),
            (0b110, GpioWriteSemantics::SubtractSaturating),
            (0b111, GpioWriteSemantics::Reconfigure),
        ];
        for (code, semantics) in table_30 {
            assert_eq!(GpioWriteSemantics::from_sub_opcode(code), Ok(semantics));
        }
        // Those eight are the complete set — nothing outside `evt[2:0]`'s
        // 3-bit range decodes, so there is no pulse-shaped ninth operation.
        assert_eq!(table_30.len(), ALL_WRITE_SEMANTICS.len());
        for raw in 8u8..=0x1F {
            assert_eq!(
                GpioWriteSemantics::from_sub_opcode(raw),
                Err(RcpError::InvalidParameter)
            );
        }
    }

    /// TC18 §13.7.4.1 (TC18.txt line 4402) words the value-changing
    /// operations as "(NAND, OR, XOR)", but TC18 §13.5 Table 30 (TC18.txt
    /// lines 3699-3702) defines `evt[2:0] = 010b` as the byte_msg_payload
    /// *bitwise AND* the current interface status, with the worked example
    /// "with a byte_msg_payload of 0xFFFF FFFE the first IO pin will be
    /// reset, while other IO pins remain unchanged". This crate follows
    /// Table 30's AND.
    #[test]
    //fusa:test REQ-GPIO-021
    fn gpio_evt_010b_is_bitwise_and_per_tc18_table_30_worked_example() {
        assert_eq!(
            GpioWriteSemantics::from_sub_opcode(0b010),
            Ok(GpioWriteSemantics::And)
        );
        // Table 30's own worked example: every IO currently set, payload
        // 0xFFFF_FFFE -> IO0 reset, IO1..IO31 unchanged.
        let current = GpioBitmask(0xFFFF_FFFF);
        let payload = GpioBitmask(0xFFFF_FFFE);
        assert_eq!(
            apply_gpio_write(GpioWriteSemantics::And, current, payload),
            Ok(GpioBitmask(0xFFFF_FFFE))
        );
        // A NAND, as §13.7.4.1's prose would have it, would instead have
        // produced the complement — 0x0000_0001.
        assert_ne!(
            apply_gpio_write(GpioWriteSemantics::And, current, payload),
            Ok(GpioBitmask(0x0000_0001))
        );
    }

    #[test]
    //fusa:test REQ-GPIO-016
    fn gpio_functional_config_layer_tag_rejects_mismatched_ep_type() {
        let functional = GpioFunctionalConfig::default();
        let generic = crate::regmap::PerEpConfigBlock::new(crate::regmap::EndpointType::Spi);
        let tag = functional.layer_tag();
        assert!(!crate::regmap::functional_config_matches_ep_type(
            &generic, &tag
        ));
    }
}
