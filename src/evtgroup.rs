// fusa:req REQ-EVTGRP-001
// fusa:req REQ-EVTGRP-002
// fusa:req REQ-EVTGRP-003
// fusa:req REQ-EVTGRP-004

//! The "Groups A/B/C" `evt[2:0]` sub-opcode convention — `ROADMAP.md`
//! Milestone 4 ("Basic Endpoint Types"), final checklist bullet: "Generic
//! `evt[2:0]` group conventions common to all of the above (Groups A/B/C)
//! and the shared common functional-config fields (`ep_enable`,
//! `ep_clear_req_storage`, `ep_req_crc_enable`, etc.)."
//!
//! This is Milestone 4's closing item, picked up only after all six
//! concrete endpoint types (GPIO, SPI, I²C, UART, PWM_OUT/PWM_IN, ADC) had
//! already built their own private reading of
//! [`crate::acf::Evt::sub_opcode`] — see this milestone's own Goal text
//! ("proving out the generic per-endpoint mechanics ... before tackling
//! bus-protocol endpoints"). The checklist bullet names two pieces; this
//! module implements the first ([`EvtGroup`], below). The second — the
//! shared common functional-config fields — is
//! [`crate::regmap::CommonFunctionalConfig`]'s own doc comment's job, not
//! this module's; see that type's provenance note.
//!
//! ## Provenance note: the Groups A/B/C classification
//!
//! This crate's own `ROADMAP.md` names the three group letters ("Groups
//! A/B/C") but states neither which axis they classify against (whether a
//! group is assigned per `sub_opcode` *value*, per
//! [`crate::regmap::EndpointType`], or something else entirely) nor, for
//! whichever axis is correct, which concrete values/types land in which
//! group. Searching this crate's own `ROADMAP.md` for "Group A"/"Group
//! B"/"Group C" turns up nothing beyond this bullet's own bare mention, and
//! no `§`-numbered citation accompanies it the way sibling bullets elsewhere
//! in the same subsection cite `§3.6`-`§3.11`. Per Guiding Principle 5
//! ("flag spec ambiguities ... rather than silently guessing at them"),
//! this module does not invent a classification rule.
//!
//! [`EvtGroup`] gives the three roadmap-named letters an explicit,
//! round-trippable type — the same "give the named slots a real type now,
//! resolve their content later" move
//! [`crate::regmap::PerEpTypeFunctionalConfig`] already made for its own
//! still-unbuilt per-[`crate::regmap::EndpointType`] shape, and the same
//! "list the named slots without inventing the missing one" discipline
//! [`crate::gpio::GpioWriteSemantics::Unnamed8th`] and
//! [`crate::i2c::I2cSpeedMode::HighSpeedRowA`]/
//! [`crate::i2c::I2cSpeedMode::HighSpeedRowB`] already followed for their
//! own unconfirmed slots. [`classify_evt_sub_opcode`] is this module's one
//! function touching [`crate::acf::Evt::sub_opcode`] directly: it validates
//! the field's 3-bit range (mirroring every other `sub_opcode` consumer's
//! own bounds check, e.g.
//! [`crate::gpio::GpioWriteSemantics::from_sub_opcode`]) but always
//! returns `Ok(None)` for every in-range value, honestly reporting "no
//! group assignment is confirmed for this value" rather than guessing one.
//! A later item that does reconcile the classification — against this
//! crate's own spec-extraction pass, never against restated spec prose —
//! is expected to replace that constant `None` with a real mapping then,
//! not guess it now.
//!
//! [`EvtGroup`]'s three variants are named directly from `ROADMAP.md`'s own
//! bare-letter wording and carry no other invented meaning. They are
//! deliberately *not* named after this crate's own two existing private
//! `sub_opcode` readings (GPIO's eight-way write-semantics selector,
//! SPI's up-to-six channel selector) — asserting that those two shapes
//! *are* two of the three roadmap groups (and guessing which two, and what
//! the third covers) would itself be unconfirmed invention, not a reading
//! of stated text.
//!
//! Deliberately out of scope, per every prior Milestone 1-4 entry's own
//! discipline:
//!
//! - Assigning any [`EvtGroup`] to any of the six already-built endpoint
//!   types' own `sub_opcode` usage. [`crate::gpio::GpioWriteSemantics`] and
//!   [`crate::spi::SpiChannelSelect`] each remain their own private,
//!   unclassified reading of the field, exactly as their own doc comments
//!   already flagged when this item was still outstanding — retrofitting
//!   them is separate work this item does not authorize, matching the
//!   additive-only discipline every prior milestone entry has followed.
//! - Wiring [`EvtGroup`]/[`classify_evt_sub_opcode`] into an actual
//!   decoder, dispatch loop, or [`crate::acf::Evt`] itself. This module
//!   remains additive standalone plumbing only — there is no live dispatch
//!   loop anywhere in this crate yet.

use crate::acf::EVT_SUB_OPCODE_MAX;
use crate::RcpError;

// ── EvtGroup ─────────────────────────────────────────────────────────────────

/// One of the roadmap-named "Groups A/B/C" `evt[2:0]` classification
/// labels.
///
/// See this module's doc comment "Provenance note: the Groups A/B/C
/// classification" for why this type carries no `sub_opcode`-to-group
/// assignment logic of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
// fusa:req REQ-EVTGRP-001
pub enum EvtGroup {
    /// Group A.
    A = 0,
    /// Group B.
    B = 1,
    /// Group C.
    C = 2,
}

impl EvtGroup {
    /// All three groups, in `ROADMAP.md`'s own listed order (A, B, C).
    pub const ALL: [EvtGroup; 3] = [EvtGroup::A, EvtGroup::B, EvtGroup::C];

    /// Encode this group as its ordinal value (`0`/`1`/`2`). This is an
    /// internal identity for the label itself, not a `sub_opcode` value —
    /// see this module's doc comment for why no `sub_opcode`-keyed
    /// encoding is provided.
    // fusa:req REQ-EVTGRP-001
    pub fn to_u8(self) -> u8 {
        self as u8
    }

    /// Decode an ordinal value into an [`EvtGroup`].
    ///
    /// Returns `Err(RcpError::InvalidParameter)` for any value outside
    /// `0..=2`. Never panics for any input.
    // fusa:req REQ-EVTGRP-002
    pub fn from_u8(raw: u8) -> Result<Self, RcpError> {
        match raw {
            0 => Ok(Self::A),
            1 => Ok(Self::B),
            2 => Ok(Self::C),
            _ => Err(RcpError::InvalidParameter),
        }
    }
}

/// Classify an `evt.sub_opcode` value ([`crate::acf::Evt::sub_opcode`])
/// against the roadmap-named "Groups A/B/C" convention.
///
/// Returns `Err(RcpError::InvalidParameter)` for any value outside the
/// 3-bit `sub_opcode` field's range (`>`[`EVT_SUB_OPCODE_MAX`]), mirroring
/// every other `sub_opcode` consumer's own bounds check (e.g.
/// [`crate::gpio::GpioWriteSemantics::from_sub_opcode`]). For every
/// in-range value, always returns `Ok(None)`: no group assignment is
/// confirmed for any `sub_opcode` value, per this module's doc comment
/// "Provenance note: the Groups A/B/C classification" — this is not a
/// bug, it is this function's honest total answer given the ambiguity.
/// Never panics for any input.
// fusa:req REQ-EVTGRP-003
// fusa:req REQ-EVTGRP-004
pub fn classify_evt_sub_opcode(sub_opcode: u8) -> Result<Option<EvtGroup>, RcpError> {
    if sub_opcode > EVT_SUB_OPCODE_MAX {
        return Err(RcpError::InvalidParameter);
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── EvtGroup: round-trip / never-panic ──────────────────────────────────

    #[test]
    // fusa:test REQ-EVTGRP-001
    fn evt_group_to_u8_round_trips_through_from_u8() {
        for group in EvtGroup::ALL {
            assert_eq!(EvtGroup::from_u8(group.to_u8()).unwrap(), group);
        }
    }

    #[test]
    // fusa:test REQ-EVTGRP-001
    fn evt_group_ordinal_values_match_roadmap_listed_order() {
        assert_eq!(EvtGroup::A.to_u8(), 0);
        assert_eq!(EvtGroup::B.to_u8(), 1);
        assert_eq!(EvtGroup::C.to_u8(), 2);
    }

    #[test]
    // fusa:test REQ-EVTGRP-002
    fn evt_group_from_u8_rejects_out_of_range_values() {
        for raw in 3..=u8::MAX {
            assert_eq!(EvtGroup::from_u8(raw), Err(RcpError::InvalidParameter));
        }
    }

    // ── classify_evt_sub_opcode ──────────────────────────────────────────────

    #[test]
    // fusa:test REQ-EVTGRP-003
    fn classify_evt_sub_opcode_returns_none_for_every_in_range_value() {
        for sub_opcode in 0..=EVT_SUB_OPCODE_MAX {
            assert_eq!(classify_evt_sub_opcode(sub_opcode), Ok(None));
        }
    }

    #[test]
    // fusa:test REQ-EVTGRP-004
    fn classify_evt_sub_opcode_rejects_out_of_range_values() {
        for sub_opcode in (EVT_SUB_OPCODE_MAX + 1)..=u8::MAX {
            assert_eq!(
                classify_evt_sub_opcode(sub_opcode),
                Err(RcpError::InvalidParameter)
            );
        }
    }

    #[test]
    // fusa:test REQ-EVTGRP-004
    fn classify_evt_sub_opcode_never_panics_across_full_u8_range() {
        for sub_opcode in 0..=u8::MAX {
            let _ = classify_evt_sub_opcode(sub_opcode);
        }
    }
}
