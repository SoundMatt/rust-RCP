//fusa:req REQ-EVTGRP-001
//fusa:req REQ-EVTGRP-002
//fusa:req REQ-EVTGRP-003
//fusa:req REQ-EVTGRP-004
//fusa:req REQ-EVTGRP-005
//fusa:req REQ-EVTGRP-006
//fusa:req REQ-EVTGRP-007

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
//! own unconfirmed slots. [`classify_evt_sub_opcode`] is this half of the
//! module's one function touching [`crate::acf::Evt::sub_opcode`] directly:
//! it validates the field's 3-bit range (mirroring every other
//! `sub_opcode` consumer's own bounds check, e.g.
//! [`crate::gpio::GpioWriteSemantics::from_sub_opcode`]) but always
//! returns `Ok(None)` for every in-range value, honestly reporting "no
//! group assignment is confirmed for this value" rather than guessing one.
//! A later item that does reconcile the classification — against this
//! crate's own spec-extraction pass, never against restated spec prose —
//! is expected to replace that constant `None` with a real mapping then,
//! not guess it now. This is a *separate* question from the narrower,
//! already-unambiguous Table 33 Row-2 rule below
//! ([`EvtRow2Kind`]/[`evt_row2_kind_of`]) — resolving one does not resolve
//! the other.
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
//!   decoder, dispatch loop, or [`crate::acf::Evt`] itself. This half of
//!   the module remains additive standalone plumbing only — there is no
//!   live dispatch loop anywhere in this crate yet.
//!
//! ## Provenance note: TC18 §13.5 Table 33's Row-2 rule (`evt_row2_kind_of`)
//!
//! Unlike the still-unresolved "Groups A/B/C" classification above, TC18
//! §13.5 Table 33 ("EP specific usage of evt-field", TC18.txt lines
//! 4076-4116) gives one specific row of endpoint types — `{ADC, PWM_IN,
//! I²C, LIN, CAN, UART, ISELED, MDIO}`, this document's own "Row 2" — a
//! request-side `evt[2:0]` rule with only three outcomes, independent of
//! the endpoint type: `111b` reconfigures the endpoint directly rather than
//! presenting `byte_msg_payload` to the interface (TC18 §12.7.1), every
//! other in-range value is reserved and must be rejected with error code
//! `UNSUPPORTED_CMD`, and `000b` presents `byte_msg_payload` to the
//! interface as an ordinary request. [`EvtRow2Kind`]/[`evt_row2_kind_of`]
//! give that three-way rule a shared, reusable predicate — every Row-2
//! endpoint type's own module calls the same function rather than
//! re-deriving the rule, mirroring
//! [`crate::gpio::GpioWriteSemantics::from_sub_opcode`]/
//! [`crate::spi::SpiChannelSelect::from_sub_opcode`]'s own per-endpoint-type
//! `sub_opcode` readers. [`crate::i2c`] is this predicate's first caller
//! ([`crate::i2c::I2cRequest::from_evt_sub_opcode`]); the other seven
//! Row-2 endpoint types are expected to call [`evt_row2_kind_of`] the same
//! way in their own follow-up items rather than each reinventing the rule.
//!
//! **The literal-text discrepancy this crate resolves.** Table 33's own
//! printed Row-2 cell reads "`000b` to `110b` reserved – request to be
//! rejected with error code = UNSUPPORTED_CMD" (TC18.txt line 4085) —
//! `000b` included in the reserved range, on a literal reading. This
//! crate does not implement that literal reading: [`evt_row2_kind_of`]
//! treats `000b` as [`EvtRow2Kind::Plain`], not
//! [`EvtRow2Kind::Reserved`]. Several independent signals point at the
//! literal cell text being a drafting defect (most likely a dropped
//! leading digit — "`001b` to `110b`" — rather than "`000b` to `110b`"),
//! not a deliberate design:
//!
//! - TC18 §12.9.1 states a general rule spanning every endpoint type: "If
//!   evt[2:0] ≠ 0 and no byte_msg_payload is present, then an error
//!   response shall be sent with the error code = UNSUPPORTED_CMD"
//!   (TC18.txt line 3611) — worded as though `evt[2:0] == 0` is every
//!   endpoint type's own normal, payload-carrying case, not a universally
//!   reserved one.
//! - TC18 §13.7.7.3 ("I²C EP request handling", the very section
//!   [`crate::i2c`] itself implements) describes I²C's ordinary
//!   `byte_msg_payload` transfer with no mention of any reserved or
//!   unavailable `evt[2:0]` value at all.
//! - Every other row of the same table gives `000b` a real, non-reserved
//!   behavior (SPI's row: channel 0 selection; the GPIO/PWM_OUT row:
//!   "byte_msg_payload is presented at the interface" — the literal
//!   behavior this module gives Row 2's own `000b` too). Row 2 being the
//!   sole row where even `000b` is reserved would leave all eight of its
//!   endpoint types with no way to ever issue an ordinary request — only
//!   [`EvtRow2Kind::ConfigWrite`]'s register-map access — which
//!   contradicts those endpoint types' own dedicated sections describing
//!   normal data-transfer behavior.
//!
//! This resolution is not this crate's own invention: it is the reference
//! shape this crate's sibling implementation c-RCP centralizes as
//! `rcp_acf_evt_row2_is_plain()` (`include/rcp/acf.h`/`src/acf.c`),
//! recorded as a cross-repo "finding, resolved" in the RELAY umbrella
//! repo's `docs/RCP-ARCHITECTURE.md` ("Table 30 / evt[2:0] write
//! semantics"). Per Guiding Principle 5, this crate follows that
//! resolution rather than Table 33's own literal cell text, and flags the
//! discrepancy explicitly here rather than silently picking one reading —
//! the same discipline [`crate::gpio::GpioWriteSemantics::And`]'s own doc
//! comment already applies to Table 33's GPIO row (`010b`'s AND wording
//! contradicting §13.7.4.1's prose "NAND").
//!
//! [`EvtRow2Kind::ConfigWrite`]'s own payload shape (TC18 §12.7.1) is
//! deliberately out of scope here and in every Row-2 endpoint-type module
//! this predicate lands in — [`evt_row2_kind_of`] only classifies a
//! `sub_opcode` as [`EvtRow2Kind::ConfigWrite`], it does not decode what
//! that config-write payload contains.

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
//fusa:req REQ-EVTGRP-001
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
    //fusa:req REQ-EVTGRP-001
    pub fn to_u8(self) -> u8 {
        self as u8
    }

    /// Decode an ordinal value into an [`EvtGroup`].
    ///
    /// Returns `Err(RcpError::InvalidParameter)` for any value outside
    /// `0..=2`. Never panics for any input.
    //fusa:req REQ-EVTGRP-002
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
/// 3-bit `sub_opcode` field's range (`>`[`crate::acf::EVT_SUB_OPCODE_MAX`]),
/// mirroring every other `sub_opcode` consumer's own bounds check (e.g.
/// [`crate::gpio::GpioWriteSemantics::from_sub_opcode`]). For every
/// in-range value, always returns `Ok(None)`: no group assignment is
/// confirmed for any `sub_opcode` value, per this module's doc comment
/// "Provenance note: the Groups A/B/C classification" — this is not a
/// bug, it is this function's honest total answer given the ambiguity.
/// Never panics for any input.
///
/// This is a distinct question from [`evt_row2_kind_of`]'s own, narrower,
/// already-unambiguous TC18 §13.5 Table 33 Row-2 rule below — see this
/// module's doc comment "Provenance note: TC18 §13.5 Table 33's Row-2 rule
/// (`evt_row2_kind_of`)".
//fusa:req REQ-EVTGRP-003
//fusa:req REQ-EVTGRP-004
pub fn classify_evt_sub_opcode(sub_opcode: u8) -> Result<Option<EvtGroup>, RcpError> {
    if sub_opcode > crate::acf::EVT_SUB_OPCODE_MAX {
        return Err(RcpError::InvalidParameter);
    }
    Ok(None)
}

/// TC18 §13.5 Table 33's three-way classification of an `evt[2:0]`
/// sub-opcode for the "Row 2" endpoint-type group — `{ADC, PWM_IN, I²C,
/// LIN, CAN, UART, ISELED, MDIO}`. See this module's doc comment
/// "Provenance note: TC18 §13.5 Table 33's Row-2 rule (`evt_row2_kind_of`)"
/// for the full citation and the literal-text discrepancy this crate
/// resolves against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
//fusa:req REQ-EVTGRP-005
pub enum EvtRow2Kind {
    /// `evt[2:0] == 000b`: `byte_msg_payload` is presented transparently
    /// at the endpoint's interface — an ordinary, unmodified request.
    Plain,
    /// `evt[2:0] == 111b`: `byte_msg_payload` is not presented to the
    /// interface at all; it instead reconfigures the endpoint's
    /// EP_functional configuration directly (TC18 §12.7.1). Decoding what
    /// that config-write payload contains is out of scope for
    /// [`evt_row2_kind_of`] and every Row-2 endpoint-type module built on
    /// it so far — see this module's doc comment.
    ConfigWrite,
    /// `evt[2:0]` in `001b..=110b`, or any value outside the 3-bit field's
    /// representable range (`sub_opcode`'s type, [`u8`], does not itself
    /// enforce that range) — reserved. TC18 §13.5 Table 33 requires the
    /// request be rejected with error code `UNSUPPORTED_CMD`
    /// ([`crate::RcpError::UnsupportedCmd`]).
    Reserved,
}

/// Classify a Row-2 endpoint's `evt.sub_opcode` value
/// ([`crate::acf::Evt::sub_opcode`]) under TC18 §13.5 Table 33's Row-2
/// rule.
///
/// Infallible and total over the full `u8` range: `0` is
/// [`EvtRow2Kind::Plain`], `7` is [`EvtRow2Kind::ConfigWrite`], and every
/// other value — including every value above [`crate::acf::EVT_SUB_OPCODE_MAX`], since
/// `sub_opcode`'s `u8` type does not itself constrain it to 3 bits —
/// classifies as [`EvtRow2Kind::Reserved`]. This function only classifies;
/// it does not itself construct [`crate::RcpError::UnsupportedCmd`] for
/// [`EvtRow2Kind::Reserved`] — a caller enforcing the rule (e.g.
/// [`crate::i2c::I2cRequest::from_evt_sub_opcode`]) does that. Never
/// panics for any input.
//fusa:req REQ-EVTGRP-006
//fusa:req REQ-EVTGRP-007
pub fn evt_row2_kind_of(sub_opcode: u8) -> EvtRow2Kind {
    match sub_opcode {
        0 => EvtRow2Kind::Plain,
        7 => EvtRow2Kind::ConfigWrite,
        _ => EvtRow2Kind::Reserved,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── EvtGroup: round-trip / never-panic ──────────────────────────────────

    #[test]
    //fusa:test REQ-EVTGRP-001
    fn evt_group_to_u8_round_trips_through_from_u8() {
        for group in EvtGroup::ALL {
            assert_eq!(EvtGroup::from_u8(group.to_u8()).unwrap(), group);
        }
    }

    #[test]
    //fusa:test REQ-EVTGRP-001
    fn evt_group_ordinal_values_match_roadmap_listed_order() {
        assert_eq!(EvtGroup::A.to_u8(), 0);
        assert_eq!(EvtGroup::B.to_u8(), 1);
        assert_eq!(EvtGroup::C.to_u8(), 2);
    }

    #[test]
    //fusa:test REQ-EVTGRP-002
    fn evt_group_from_u8_rejects_out_of_range_values() {
        for raw in 3..=u8::MAX {
            assert_eq!(EvtGroup::from_u8(raw), Err(RcpError::InvalidParameter));
        }
    }

    // ── classify_evt_sub_opcode ──────────────────────────────────────────────

    #[test]
    //fusa:test REQ-EVTGRP-003
    fn classify_evt_sub_opcode_returns_none_for_every_in_range_value() {
        for sub_opcode in 0..=crate::acf::EVT_SUB_OPCODE_MAX {
            assert_eq!(classify_evt_sub_opcode(sub_opcode), Ok(None));
        }
    }

    #[test]
    //fusa:test REQ-EVTGRP-004
    fn classify_evt_sub_opcode_rejects_out_of_range_values() {
        for sub_opcode in (crate::acf::EVT_SUB_OPCODE_MAX + 1)..=u8::MAX {
            assert_eq!(
                classify_evt_sub_opcode(sub_opcode),
                Err(RcpError::InvalidParameter)
            );
        }
    }

    #[test]
    //fusa:test REQ-EVTGRP-004
    fn classify_evt_sub_opcode_never_panics_across_full_u8_range() {
        for sub_opcode in 0..=u8::MAX {
            let _ = classify_evt_sub_opcode(sub_opcode);
        }
    }

    // ── evt_row2_kind_of ─────────────────────────────────────────────────────

    #[test]
    //fusa:test REQ-EVTGRP-005
    fn evt_row2_kind_of_classifies_zero_as_plain_and_seven_as_config_write() {
        assert_eq!(evt_row2_kind_of(0), EvtRow2Kind::Plain);
        assert_eq!(evt_row2_kind_of(7), EvtRow2Kind::ConfigWrite);
    }

    #[test]
    //fusa:test REQ-EVTGRP-006
    fn evt_row2_kind_of_classifies_one_through_six_as_reserved() {
        for sub_opcode in 1..=6u8 {
            assert_eq!(evt_row2_kind_of(sub_opcode), EvtRow2Kind::Reserved);
        }
    }

    #[test]
    //fusa:test REQ-EVTGRP-006
    fn evt_row2_kind_of_classifies_every_value_above_seven_as_reserved() {
        // `sub_opcode`'s `u8` type does not itself enforce the 3-bit
        // `EVT_SUB_OPCODE_MAX` range, so every value beyond it must still
        // classify definitively rather than panic or fall through unhandled.
        for sub_opcode in (crate::acf::EVT_SUB_OPCODE_MAX + 1)..=u8::MAX {
            assert_eq!(evt_row2_kind_of(sub_opcode), EvtRow2Kind::Reserved);
        }
    }

    #[test]
    //fusa:test REQ-EVTGRP-006
    fn evt_row2_kind_of_never_panics_across_full_u8_range() {
        for sub_opcode in 0..=u8::MAX {
            let _ = evt_row2_kind_of(sub_opcode);
        }
    }

    // ── evt_row2_kind_of: TC18 §13.5 Table 33 Row-2 spec-literal checks ─────

    /// TC18 §13.5 Table 33 (TC18.txt lines 4085-4092): Row 2's `111b` cell
    /// reads "The byte_msg_payload is not presented to the interface but
    /// used to change the configuration of the endpoint (see 12.7.1)."
    #[test]
    //fusa:test REQ-EVTGRP-007
    fn evt_row2_kind_of_111b_is_config_write_per_tc18_table_33() {
        assert_eq!(evt_row2_kind_of(0b111), EvtRow2Kind::ConfigWrite);
    }

    /// TC18 §13.5 Table 33 (TC18.txt line 4085): Row 2's own printed cell
    /// text reads "000b to 110b reserved – request to be rejected with
    /// error code = UNSUPPORTED_CMD" — literally including `000b`. This
    /// crate does not implement that literal text: see this module's doc
    /// comment "Provenance note: TC18 §13.5 Table 33's Row-2 rule
    /// (`evt_row2_kind_of`)" for the reconciliation this test locks in —
    /// `000b` classifies as [`EvtRow2Kind::Plain`], and only `001b..=110b`
    /// classify as [`EvtRow2Kind::Reserved`].
    #[test]
    //fusa:test REQ-EVTGRP-007
    fn evt_row2_kind_of_000b_is_plain_not_reserved_despite_table_33s_literal_cell_text() {
        assert_eq!(evt_row2_kind_of(0b000), EvtRow2Kind::Plain);
        for sub_opcode in 0b001..=0b110u8 {
            assert_eq!(evt_row2_kind_of(sub_opcode), EvtRow2Kind::Reserved);
        }
    }
}
