// fusa:req REQ-LIFE-001
// fusa:req REQ-LIFE-002
// fusa:req REQ-LIFE-003
// fusa:req REQ-LIFE-004
// fusa:req REQ-LIFE-005

//! RC Server lifecycle state machine — TC18 register-map model
//! (`ROADMAP.md` Milestone 2, "Lifecycle State Machine" subsection, first
//! checklist item).
//!
//! This module begins Milestone 2, which replaces the legacy
//! `Zone`/`Controller`/`Registry` abstraction with a first-class RC Server
//! entity model. Per Guiding Principle 2 ("sequence work so nothing is
//! built on a foundation that will itself change later ... lifecycle model
//! and register-map split before endpoints"), the lifecycle model comes
//! first: [`RcServerState`] is the three-state machine every RC Server is
//! in at all times, and [`RegisterCategory`]/[`is_register_reachable`] give
//! that state machine its first, coarsest teeth — which broad class of
//! register can be touched at all in each state.
//!
//! This is deliberately the *first* of four "Lifecycle State Machine"
//! checklist items and stops well short of the other three, which are
//! separate, later work:
//!
//! - Transition **guard checks** (`HW_CFG_INCONSISTENT`,
//!   `RCP_CFG_INCONSISTENT`) that gate whether a state transition is
//!   actually allowed to happen. This module models the states and what is
//!   reachable *within* a state; it does not yet model transitions between
//!   them at all — there is intentionally no `RcServerState::advance()` or
//!   similar here.
//! - **Register-locking-by-state**, including the `W`/`W*` distinction
//!   (permanently locked once `RCP_CONFIGURED`). [`is_register_reachable`]
//!   answers "is this category of register touchable at all right now" —
//!   a coarser question than locking's "given that it's reachable, is a
//!   *write* to it still permitted, or has it become permanently locked".
//!   A category this module reports reachable in a given state may still
//!   be write-locked by that later rule; this module has no opinion on
//!   that distinction.
//! - The **demotion path** from `HW_CONFIGURED` back to `HW_UNCONFIGURED`.
//!
//! Also out of scope, as later subsections of the same milestone: EP0 (the
//! RC-Server-as-endpoint whole-register-map read/write path) and the
//! concrete Register Map itself (the actual field layout of
//! `svr_oa_tc18_magic_nr`, HW pin-mapping tables, etc.). [`RegisterCategory`]
//! is intentionally an abstract placeholder standing in for "whichever
//! concrete registers the Register Map subsection eventually defines,
//! grouped this way" — it names no concrete field, mirroring how
//! [`crate::addressing::EndpointId`] stood in for a concrete endpoint type
//! ahead of Milestone 4.
//!
//! This module is additive and does not touch [`crate::config`] (the old,
//! disposition-REPLACE `RcpConfig` model) or any other existing caller,
//! matching the discipline every Milestone 1 entry already established.
//!
//! ## Provenance note
//!
//! The three state names, their numeric encodings (`HW_UNCONFIGURED` =
//! `0x00`, `HW_CONFIGURED` = `0x55`, `RCP_CONFIGURED` = `0xAA`), and the
//! `HW_CFG_INCONSISTENT`/`RCP_CFG_INCONSISTENT` guard names referenced above
//! are taken directly from `ROADMAP.md`'s own Milestone 2 checklist text,
//! which in turn cites the OPEN Alliance TC18 Remote Control Protocol
//! Specification v0.5.1_RC by name only, never by section number, for this
//! particular subsection (contrast the "Register Map" subsection a few
//! lines further down the same milestone, whose bullets already cite
//! `§3.6`–`§3.11`). This module's own doc comments therefore do not cite a
//! `§3.x` section number for the lifecycle state machine itself — none is
//! yet recorded anywhere in this crate — and per Guiding Principle 5 that
//! absence is flagged here explicitly rather than papered over with a
//! guessed number.
//!
//! [`RegisterCategory`] and the [`is_register_reachable`] reachability rule
//! it drives go a step further than the roadmap text and are this crate's
//! own working interpretation, not a transcription of the specification's
//! behavior:
//!
//! - The `HwConfig`/`RcpConfig` split, and `RcpConfig` becoming reachable
//!   only from `HW_CONFIGURED` onward, is inferred from the *existence* of
//!   the `HW_CFG_INCONSISTENT` guard (named for the `HW_UNCONFIGURED` →
//!   `HW_CONFIGURED` transition) and the separate `RCP_CFG_INCONSISTENT`
//!   guard (named for the `HW_CONFIGURED` → `RCP_CONFIGURED` transition):
//!   two distinctly-named guards for two distinctly-named configuration
//!   phases reads as strong evidence of two distinct register categories
//!   with different reachability, even though the roadmap text does not
//!   spell that reachability rule out explicitly for this checklist item.
//! - `General` register reachability being unconditional in every state is
//!   this crate's own inference from `ROADMAP.md` Milestone 3 (Discovery)
//!   immediately following this milestone: a discovering client plausibly
//!   needs to read server-identity fields (`svr_vendor_id`,
//!   `svr_device_id`, etc. — named in the Register Map subsection) before
//!   any configuration has happened at all, which only works if those
//!   fields are reachable in `HW_UNCONFIGURED` too.
//!
//! Both points are flagged per Guiding Principle 5 as this crate's own
//! working interpretation, pending reconciliation against the
//! specification's actual behavior (never its prose) before being relied
//! on for interop with a real TC18 RC Server.

use crate::RcpError;

// ── RcServerState ────────────────────────────────────────────────────────────

/// The mandatory three-state RC Server lifecycle, per `ROADMAP.md`
/// Milestone 2.
///
/// See this module's doc comment for the numeric encodings' provenance and
/// for the transition guards, register-locking distinction, and demotion
/// path this type deliberately does *not* yet model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
// fusa:req REQ-LIFE-001
pub enum RcServerState {
    /// No hardware configuration has been applied yet. Only [`RegisterCategory::General`]
    /// registers are reachable (see [`is_register_reachable`]).
    HwUnconfigured = 0x00,
    /// Hardware configuration has been applied. [`RegisterCategory::HwConfig`]
    /// and [`RegisterCategory::RcpConfig`] registers both become reachable
    /// alongside [`RegisterCategory::General`].
    HwConfigured = 0x55,
    /// RCP-level (functional) configuration has additionally been applied.
    /// All three [`RegisterCategory`] variants remain reachable; see this
    /// module's doc comment for why *write*-locking is a separate, later
    /// concern this state does not itself add.
    RcpConfigured = 0xAA,
}

impl RcServerState {
    /// The initial state of a freshly constructed (unconfigured) RC Server.
    ///
    /// Not itself claimed by the roadmap text; this crate's own reasonable
    /// default (per Guiding Principle 5, flagged rather than asserted as
    /// spec fact) given `HW_UNCONFIGURED`'s name and `0x00` encoding both
    /// suggest a power-on/reset default.
    pub const INITIAL: Self = Self::HwUnconfigured;

    /// Encode this state as its wire-level byte value.
    pub fn to_u8(self) -> u8 {
        self as u8
    }

    /// Decode a wire-level byte value into an [`RcServerState`].
    ///
    /// Returns `Err(RcpError::Other(_))` for any byte other than the three
    /// defined encodings, mirroring
    /// [`crate::avtpdu::select_header_variant`]'s handling of an
    /// unrecognized subtype byte. Never panics for any input.
    // fusa:req REQ-LIFE-002
    pub fn from_u8(raw: u8) -> Result<Self, RcpError> {
        match raw {
            0x00 => Ok(Self::HwUnconfigured),
            0x55 => Ok(Self::HwConfigured),
            0xAA => Ok(Self::RcpConfigured),
            other => Err(RcpError::Other(format!(
                "lifecycle: unrecognized RC Server state byte 0x{other:02X} (expected 0x00 HW_UNCONFIGURED, 0x55 HW_CONFIGURED, or 0xAA RCP_CONFIGURED)"
            ))),
        }
    }
}

impl Default for RcServerState {
    fn default() -> Self {
        Self::INITIAL
    }
}

// ── RegisterCategory ─────────────────────────────────────────────────────────

/// An abstract placeholder grouping for the RC Server register map, used
/// only to express per-state reachability ahead of the concrete Register
/// Map (`ROADMAP.md` Milestone 2, "Register Map" subsection) this crate has
/// not built yet.
///
/// See this module's doc comment for how this split — and the reachability
/// rule [`is_register_reachable`] derives from it — was inferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// fusa:req REQ-LIFE-003
pub enum RegisterCategory {
    /// Server-identity/status registers (e.g. the eventual
    /// `svr_oa_tc18_magic_nr`, `svr_version`, `svr_vendor_id`,
    /// `svr_device_id`) not gated by lifecycle state at all.
    General,
    /// Registers configuring hardware/physical-layer setup (e.g. the
    /// eventual HW pin-mapping table).
    HwConfig,
    /// Registers configuring RCP-level/functional behavior (e.g. the
    /// eventual request-stream config, EP-ID/`byte_bus_id` mapping,
    /// response/ack queue config, sequencer-state registers).
    RcpConfig,
}

/// Is `category` reachable at all while the RC Server is in `state`?
///
/// This is a coarse, state-gated visibility rule only — it says nothing
/// about whether a *write* to a reachable register is further locked (see
/// this module's doc comment for why that is separate, later work).
/// Never panics for any input.
// fusa:req REQ-LIFE-003
// fusa:req REQ-LIFE-004
pub fn is_register_reachable(state: RcServerState, category: RegisterCategory) -> bool {
    match (state, category) {
        // General registers are reachable in every state (see this
        // module's doc comment for why).
        (_, RegisterCategory::General) => true,
        // HwConfig registers are reachable from the very first state
        // onward — HW_UNCONFIGURED is exactly the state in which they get
        // configured in the first place.
        (_, RegisterCategory::HwConfig) => true,
        // RcpConfig registers only become reachable once hardware
        // configuration exists to build functional configuration on top
        // of; see the module doc comment's HW_CFG_INCONSISTENT /
        // RCP_CFG_INCONSISTENT reasoning.
        (RcServerState::HwUnconfigured, RegisterCategory::RcpConfig) => false,
        (RcServerState::HwConfigured, RegisterCategory::RcpConfig) => true,
        (RcServerState::RcpConfigured, RegisterCategory::RcpConfig) => true,
    }
}

/// Validating counterpart to [`is_register_reachable`].
///
/// Returns `Ok(())` if `category` is reachable while the RC Server is in
/// `state`, `Err(RcpError::RegisterUnreachable)` otherwise. Never panics
/// for any input.
///
/// The error variant is this crate's own provisional name, matching the
/// pre-Error-Model-item style already used by
/// [`RcpError::TimeSyncUnsupported`]/[`RcpError::EndpointAlreadyRegistered`]/
/// [`RcpError::EchoBackMismatch`]. `ROADMAP.md` Milestone 2's separate
/// "Error Model" checklist item will eventually replace `RcpError`'s whole
/// variant set with the specification's own error codes (`UNAUTHORIZED_ACCESS`,
/// `LOCKED_MEM_ACCESS`, etc.); which of those this variant ultimately maps
/// to is that later item's call to make, not this one's.
// fusa:req REQ-LIFE-004
pub fn check_register_reachable(
    state: RcServerState,
    category: RegisterCategory,
) -> Result<(), RcpError> {
    if is_register_reachable(state, category) {
        Ok(())
    } else {
        Err(RcpError::RegisterUnreachable)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    const ALL_STATES: [RcServerState; 3] = [
        RcServerState::HwUnconfigured,
        RcServerState::HwConfigured,
        RcServerState::RcpConfigured,
    ];

    const ALL_CATEGORIES: [RegisterCategory; 3] = [
        RegisterCategory::General,
        RegisterCategory::HwConfig,
        RegisterCategory::RcpConfig,
    ];

    // ── Numeric encoding / round-trip ────────────────────────────────────

    #[test]
    // fusa:test REQ-LIFE-001
    fn state_encodings_match_roadmap_values() {
        assert_eq!(RcServerState::HwUnconfigured.to_u8(), 0x00);
        assert_eq!(RcServerState::HwConfigured.to_u8(), 0x55);
        assert_eq!(RcServerState::RcpConfigured.to_u8(), 0xAA);
    }

    #[test]
    // fusa:test REQ-LIFE-001
    fn from_u8_round_trips_every_valid_encoding() {
        for state in ALL_STATES {
            let raw = state.to_u8();
            assert_eq!(RcServerState::from_u8(raw), Ok(state));
        }
    }

    #[test]
    // fusa:test REQ-LIFE-001
    fn initial_state_is_hw_unconfigured() {
        assert_eq!(RcServerState::INITIAL, RcServerState::HwUnconfigured);
        assert_eq!(RcServerState::default(), RcServerState::HwUnconfigured);
    }

    // ── Rejection of unrecognized encodings ──────────────────────────────

    #[test]
    // fusa:test REQ-LIFE-002
    fn from_u8_rejects_every_byte_other_than_the_three_valid_ones() {
        for raw in 0u8..=255 {
            let result = RcServerState::from_u8(raw);
            match raw {
                0x00 | 0x55 | 0xAA => assert!(result.is_ok(), "0x{raw:02X} should decode"),
                _ => assert!(result.is_err(), "0x{raw:02X} should be rejected"),
            }
        }
    }

    // ── Per-state register reachability ──────────────────────────────────

    #[test]
    // fusa:test REQ-LIFE-003
    fn general_registers_are_reachable_in_every_state() {
        for state in ALL_STATES {
            assert!(is_register_reachable(state, RegisterCategory::General));
        }
    }

    #[test]
    // fusa:test REQ-LIFE-003
    fn hw_config_registers_are_reachable_in_every_state() {
        for state in ALL_STATES {
            assert!(is_register_reachable(state, RegisterCategory::HwConfig));
        }
    }

    #[test]
    // fusa:test REQ-LIFE-003
    fn rcp_config_registers_are_unreachable_only_while_hw_unconfigured() {
        assert!(!is_register_reachable(
            RcServerState::HwUnconfigured,
            RegisterCategory::RcpConfig
        ));
        assert!(is_register_reachable(
            RcServerState::HwConfigured,
            RegisterCategory::RcpConfig
        ));
        assert!(is_register_reachable(
            RcServerState::RcpConfigured,
            RegisterCategory::RcpConfig
        ));
    }

    #[test]
    // fusa:test REQ-LIFE-004
    fn check_register_reachable_agrees_with_is_register_reachable() {
        for state in ALL_STATES {
            for category in ALL_CATEGORIES {
                let reachable = is_register_reachable(state, category);
                let checked = check_register_reachable(state, category);
                assert_eq!(checked.is_ok(), reachable);
                if !reachable {
                    assert_eq!(checked, Err(RcpError::RegisterUnreachable));
                }
            }
        }
    }

    // ── Fuzz-style: arbitrary inputs never panic ─────────────────────────

    #[test]
    // fusa:test REQ-LIFE-005
    fn from_u8_never_panics_across_the_full_byte_range() {
        for raw in 0u8..=255 {
            let _ = RcServerState::from_u8(raw);
        }
    }

    #[test]
    // fusa:test REQ-LIFE-005
    fn reachability_checks_never_panic_for_any_state_category_pair() {
        for state in ALL_STATES {
            for category in ALL_CATEGORIES {
                let _ = is_register_reachable(state, category);
                let _ = check_register_reachable(state, category);
            }
        }
    }
}
