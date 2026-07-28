// fusa:req REQ-LIFE-001
// fusa:req REQ-LIFE-002
// fusa:req REQ-LIFE-003
// fusa:req REQ-LIFE-004
// fusa:req REQ-LIFE-005
// fusa:req REQ-LIFE-006
// fusa:req REQ-LIFE-007
// fusa:req REQ-LIFE-008
// fusa:req REQ-LIFE-009
// fusa:req REQ-LIFE-010
// fusa:req REQ-LIFE-011

//! RC Server lifecycle state machine — TC18 register-map model
//! (`ROADMAP.md` Milestone 2, "Lifecycle State Machine" subsection, first
//! three checklist items).
//!
//! This module begins Milestone 2, which replaces the legacy
//! `Zone`/`Controller`/`Registry` abstraction with a first-class RC Server
//! entity model. Per Guiding Principle 2 ("sequence work so nothing is
//! built on a foundation that will itself change later ... lifecycle model
//! and register-map split before endpoints"), the lifecycle model comes
//! first: [`RcServerState`] is the three-state machine every RC Server is
//! in at all times, [`RegisterCategory`]/[`is_register_reachable`] give
//! that state machine its first, coarsest teeth — which broad class of
//! register can be touched at all in each state — and
//! [`RcServerState::try_transition`]/[`is_transition_defined`] give it its
//! second: which moves between states are even structurally allowed, and
//! under what caller-supplied consistency guard.
//!
//! This module now covers the *first three* of four "Lifecycle State
//! Machine" checklist items and stops well short of the remaining one,
//! which is separate, later work:
//!
//! - **Register-locking-by-state**, including the `W`/`W*` distinction
//!   (permanently locked once `RCP_CONFIGURED`). [`is_register_reachable`]
//!   answers "is this category of register touchable at all right now" —
//!   a coarser question than locking's "given that it's reachable, is a
//!   *write* to it still permitted, or has it become permanently locked".
//!   [`LockPolicy`]/[`lock_policy`]/[`is_register_writable`]/
//!   [`check_register_writable`] now answer that finer question, layered
//!   on top of (not replacing) reachability: a category this module
//!   reports reachable in a given state may still be write-locked by this
//!   rule.
//! - The **demotion path** from `HW_CONFIGURED` back to `HW_UNCONFIGURED`.
//!   [`RcServerState::try_transition`] deliberately implements only the two
//!   *forward* transitions (`HW_UNCONFIGURED` -> `HW_CONFIGURED` and
//!   `HW_CONFIGURED` -> `RCP_CONFIGURED`); every other `(from, to)` pair —
//!   including this demotion path, any full rollback to `HW_UNCONFIGURED`,
//!   skipping `HW_CONFIGURED` entirely, or staying in the same state — is
//!   rejected with `RcpError::InvalidLifecycleTransition` because it is
//!   not yet implemented here, not because this module has evaluated it
//!   and found it illegal. The later demotion-path item may relax that.
//!
//! Also out of scope, as later subsections of the same milestone: EP0 (the
//! RC-Server-as-endpoint whole-register-map read/write path) and the
//! concrete Register Map itself (the actual field layout of
//! `svr_oa_tc18_magic_nr`, HW pin-mapping tables, etc.). [`RegisterCategory`]
//! is intentionally an abstract placeholder standing in for "whichever
//! concrete registers the Register Map subsection eventually defines,
//! grouped this way" — it names no concrete field, mirroring how
//! [`crate::addressing::EndpointId`] stood in for a concrete endpoint type
//! ahead of Milestone 4. [`RcServerState::try_transition`]'s
//! `is_consistent` closure parameter plays the same placeholder role for
//! the `HW_CFG_INCONSISTENT`/`RCP_CFG_INCONSISTENT` guards: this crate has
//! no register map yet to actually validate hardware or RCP-level
//! configuration against, so the concrete consistency criteria are left to
//! the caller (mirroring [`crate::formal::Invariant`]'s caller-supplied
//! predicate shape) rather than guessed at here. Whatever eventually
//! supplies a real predicate — the Register Map work, or the endpoints
//! that sit on top of it — is this module's problem to wire up later, not
//! this item's to invent.
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
//!
//! [`RcServerState::try_transition`] and [`is_transition_defined`] go a
//! similar step further than the roadmap text, again as this crate's own
//! working interpretation rather than a transcription of specified
//! behavior:
//!
//! - That exactly the *two forward* transitions named by the two guards
//!   (`HW_UNCONFIGURED` -> `HW_CONFIGURED`, `HW_CONFIGURED` ->
//!   `RCP_CONFIGURED`) are structurally legal, and every other `(from,
//!   to)` pair is rejected outright, is inferred from the roadmap
//!   checklist ordering the two guard names alongside exactly those two
//!   transitions and calling out the demotion path as a distinct, later
//!   item — read together, that is this crate's evidence that no other
//!   transition (including staying put) is meant to succeed via this
//!   function.
//! - Neither guard's actual pass/fail criterion is defined anywhere in
//!   this crate yet — no register-map fields exist to validate hardware or
//!   RCP-level configuration against, since the Register Map subsection is
//!   later work in this same milestone. `try_transition` therefore takes
//!   the consistency check as a caller-supplied `is_consistent: impl
//!   FnOnce() -> bool` closure rather than evaluating anything itself; the
//!   two new `RcpError` sentinels it can return
//!   (`RcpError::HwCfgInconsistent`, `RcpError::RcpCfgInconsistent`) are
//!   named after the guards for traceability but, like
//!   `RcpError::RegisterUnreachable` before them, are this crate's own
//!   provisional names pending the milestone's later "Error Model" item.
//!   `RcpError::InvalidLifecycleTransition` (for any `(from, to)` pair
//!   outside the two defined transitions) is the same kind of provisional
//!   sentinel.
//!
//! Both points are flagged per Guiding Principle 5 as this crate's own
//! working interpretation, pending reconciliation against the
//! specification's actual behavior (never its prose) before being relied
//! on for interop with a real TC18 RC Server.
//!
//! [`LockPolicy`] and [`lock_policy`]'s per-[`RegisterCategory`] assignment
//! go the same step further than the roadmap text, and are — like
//! [`RegisterCategory`] itself — this crate's own working interpretation,
//! not a transcription of the specification's behavior. No concrete
//! Register Map exists yet (that subsection is later work in this same
//! milestone) to derive a real per-field `W`/`W*` assignment from, so this
//! crate reasons at the same [`RegisterCategory`] granularity
//! [`is_register_reachable`] already uses:
//!
//! - [`RegisterCategory::General`] is modeled as never writable through
//!   this module at all (`lock_policy` returns `None`), inferred from this
//!   module's own doc comment already describing that category as
//!   server-identity/status fields (`svr_vendor_id`, `svr_device_id`,
//!   etc.) — read/status data, not configuration a client would plausibly
//!   write.
//! - [`RegisterCategory::HwConfig`] is modeled as `W*`: writable while
//!   reachable, but permanently locked the moment `RCP_CONFIGURED` is
//!   reached. This is inferred from `HwConfig` being the *foundation*
//!   layer — the `RCP_CFG_INCONSISTENT` guard that admits
//!   `HW_CONFIGURED` -> `RCP_CONFIGURED` validates RCP-level configuration
//!   against whatever hardware configuration already exists at that
//!   moment, with no guard defined anywhere in this crate that
//!   re-validates RCP-level configuration if hardware configuration were
//!   changed out from under it afterward — permanently locking `HwConfig`
//!   once `RCP_CONFIGURED` is reached is this crate's reading of what
//!   keeps that invariant from silently breaking.
//! - [`RegisterCategory::RcpConfig`] is modeled as `W`: writable whenever
//!   reachable, including while `RCP_CONFIGURED`, with no permanent lock
//!   this module adds on top of reachability. This is inferred from
//!   `RcpConfig` being the *operating* layer built on top of `HwConfig` —
//!   this crate's own reasonable expectation (per Guiding Principle 5,
//!   flagged rather than asserted as spec fact) that functional/operating
//!   parameters remain adjustable once a server is fully configured, unlike
//!   the foundation they were built on.
//!
//! All three points are flagged per Guiding Principle 5 as this crate's own
//! working interpretation, pending reconciliation against the
//! specification's actual behavior (never its prose) before being relied
//! on for interop with a real TC18 RC Server.

use crate::RcpError;

// ── RcServerState ────────────────────────────────────────────────────────────

/// The mandatory three-state RC Server lifecycle, per `ROADMAP.md`
/// Milestone 2.
///
/// See this module's doc comment for the numeric encodings' provenance and
/// for the register-locking distinction and demotion path this type
/// deliberately does *not* yet model. See [`RcServerState::try_transition`]
/// for the guarded transitions this type *does* model.
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

// ── Register write-locking (W vs W*) ───────────────────────────────────────────

/// The write-lock policy governing whether an already-[`is_register_reachable`]
/// [`RegisterCategory`] may additionally be *written*, per `ROADMAP.md`'s
/// `W`/`W*` distinction.
///
/// See this module's doc comment for how the per-category assignment
/// ([`lock_policy`]) was inferred, and [`is_register_writable`] for how a
/// policy combines with reachability to produce a final writable/not
/// answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// fusa:req REQ-LIFE-009
pub enum LockPolicy {
    /// `W` — write access may still vary with lifecycle state (through
    /// [`is_register_reachable`]'s reachability gate) but this policy never
    /// *permanently* forecloses it: there is no state past which this rule
    /// alone rejects a write.
    W,
    /// `W*` — write access is available whenever the category is otherwise
    /// reachable, up to and including `HW_CONFIGURED`, but becomes
    /// permanently locked the moment the RC Server reaches
    /// `RCP_CONFIGURED`. No transition [`RcServerState::try_transition`]
    /// currently implements moves a server back out of `RCP_CONFIGURED`,
    /// so today this lock is permanent for the remaining lifetime of the
    /// in-memory state; the still-unbuilt demotion path is this rule's
    /// stated escape hatch, not a loophole this module implements.
    WStar,
}

/// This crate's provisional mapping from [`RegisterCategory`] to
/// [`LockPolicy`], or `None` if this module models the category as never
/// writable at all regardless of lifecycle state.
///
/// See this module's doc comment for the per-category reasoning behind
/// each assignment. Never panics for any input.
// fusa:req REQ-LIFE-009
pub fn lock_policy(category: RegisterCategory) -> Option<LockPolicy> {
    match category {
        RegisterCategory::General => None,
        RegisterCategory::HwConfig => Some(LockPolicy::WStar),
        RegisterCategory::RcpConfig => Some(LockPolicy::W),
    }
}

/// Is `category` writable while the RC Server is in `state`?
///
/// Composes [`is_register_reachable`] (a category must be reachable at all
/// before a write to it can be considered) with [`lock_policy`]'s `W`/`W*`
/// rule: unreachable categories are never writable; categories with no
/// [`LockPolicy`] (`None`) are never writable; `W` categories are writable
/// whenever reachable; `W*` categories are writable whenever reachable
/// except while `RcServerState::RcpConfigured`, where they are permanently
/// locked. Never panics for any input.
// fusa:req REQ-LIFE-009
// fusa:req REQ-LIFE-010
pub fn is_register_writable(state: RcServerState, category: RegisterCategory) -> bool {
    if !is_register_reachable(state, category) {
        return false;
    }
    match lock_policy(category) {
        None => false,
        Some(LockPolicy::W) => true,
        Some(LockPolicy::WStar) => state != RcServerState::RcpConfigured,
    }
}

/// Validating counterpart to [`is_register_writable`].
///
/// Returns `Ok(())` if `category` is writable while the RC Server is in
/// `state`. Otherwise distinguishes *why* the write is rejected —
/// `Err(RcpError::RegisterUnreachable)` if `category` is not reachable at
/// all in `state` (mirroring [`check_register_reachable`]), or
/// `Err(RcpError::RegisterLocked)` if it is reachable but write-locked by
/// [`lock_policy`]'s rule. Never panics for any input.
///
/// `RcpError::RegisterLocked` is this crate's own provisional name, matching
/// the pre-Error-Model-item style already used by
/// `RcpError::RegisterUnreachable` and the lifecycle guard sentinels before
/// it; which of the specification's own error codes it ultimately maps to
/// is this milestone's later "Error Model" item's call to make, not this
/// one's.
// fusa:req REQ-LIFE-010
pub fn check_register_writable(
    state: RcServerState,
    category: RegisterCategory,
) -> Result<(), RcpError> {
    if !is_register_reachable(state, category) {
        Err(RcpError::RegisterUnreachable)
    } else if is_register_writable(state, category) {
        Ok(())
    } else {
        Err(RcpError::RegisterLocked)
    }
}

// ── Lifecycle transitions ────────────────────────────────────────────────────

/// Is `(from, to)` one of the two forward transitions this crate currently
/// implements a guard for?
///
/// This is the coarse, state-shape check [`RcServerState::try_transition`]
/// performs before it ever considers a caller-supplied consistency
/// predicate — it says nothing about whether the hardware or RCP-level
/// configuration being applied is actually consistent, only whether
/// `(from, to)` is a transition shape this crate implements at all. Every
/// pair other than the two named here is `false`, including staying in the
/// same state, moving backward, skipping `HW_CONFIGURED` entirely, or the
/// `HW_CONFIGURED` -> `HW_UNCONFIGURED` demotion path (a separate, later
/// roadmap item — see this module's doc comment). Never panics for any
/// input.
// fusa:req REQ-LIFE-006
pub fn is_transition_defined(from: RcServerState, to: RcServerState) -> bool {
    matches!(
        (from, to),
        (RcServerState::HwUnconfigured, RcServerState::HwConfigured)
            | (RcServerState::HwConfigured, RcServerState::RcpConfigured)
    )
}

impl RcServerState {
    /// Attempt to move this RC Server from its current state (`self`) to
    /// `target`.
    ///
    /// Only the two forward transitions named by `ROADMAP.md`'s
    /// `HW_CFG_INCONSISTENT`/`RCP_CFG_INCONSISTENT` guards are implemented:
    ///
    /// - `HW_UNCONFIGURED` -> `HW_CONFIGURED`, guarded by the
    ///   `HW_CFG_INCONSISTENT` check
    /// - `HW_CONFIGURED` -> `RCP_CONFIGURED`, guarded by the
    ///   `RCP_CFG_INCONSISTENT` check
    ///
    /// For either of those two shapes, `is_consistent` is invoked exactly
    /// once: if it returns `true`, the transition succeeds and `Ok(target)`
    /// is returned; if it returns `false`, the matching guard sentinel
    /// (`RcpError::HwCfgInconsistent` or `RcpError::RcpCfgInconsistent`)
    /// is returned and `self` is left unchanged (this method takes `self`
    /// by value and returns the *new* state on success — it does not
    /// mutate anything in place).
    ///
    /// For every other `(self, target)` pair — see [`is_transition_defined`]
    /// for the exact rule — `is_consistent` is never invoked at all, and
    /// `Err(RcpError::InvalidLifecycleTransition)` is returned instead, per
    /// this module's doc comment on why that includes the demotion path
    /// rather than that path being evaluated and rejected on its merits.
    ///
    /// This crate has no register map yet to derive a real consistency
    /// check from (see this module's doc comment's Provenance note), so
    /// `is_consistent` is deliberately a caller-supplied placeholder,
    /// mirroring [`crate::formal::Invariant`]'s predicate shape. Never
    /// panics for any input, including a `target` equal to `self`.
    // fusa:req REQ-LIFE-006
    // fusa:req REQ-LIFE-007
    // fusa:req REQ-LIFE-008
    pub fn try_transition(
        self,
        target: Self,
        is_consistent: impl FnOnce() -> bool,
    ) -> Result<Self, RcpError> {
        match (self, target) {
            (Self::HwUnconfigured, Self::HwConfigured) => {
                if is_consistent() {
                    Ok(target)
                } else {
                    Err(RcpError::HwCfgInconsistent)
                }
            }
            (Self::HwConfigured, Self::RcpConfigured) => {
                if is_consistent() {
                    Ok(target)
                } else {
                    Err(RcpError::RcpCfgInconsistent)
                }
            }
            _ => Err(RcpError::InvalidLifecycleTransition),
        }
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

    // ── Register write-locking (W vs W*) ──────────────────────────────────

    #[test]
    // fusa:test REQ-LIFE-009
    fn lock_policy_matches_the_documented_per_category_assignment() {
        assert_eq!(lock_policy(RegisterCategory::General), None);
        assert_eq!(
            lock_policy(RegisterCategory::HwConfig),
            Some(LockPolicy::WStar)
        );
        assert_eq!(
            lock_policy(RegisterCategory::RcpConfig),
            Some(LockPolicy::W)
        );
    }

    #[test]
    // fusa:test REQ-LIFE-009
    // fusa:test REQ-LIFE-010
    fn general_registers_are_never_writable_in_any_state() {
        for state in ALL_STATES {
            assert!(!is_register_writable(state, RegisterCategory::General));
        }
    }

    #[test]
    // fusa:test REQ-LIFE-009
    // fusa:test REQ-LIFE-010
    fn hw_config_registers_are_writable_until_permanently_locked_at_rcp_configured() {
        assert!(is_register_writable(
            RcServerState::HwUnconfigured,
            RegisterCategory::HwConfig
        ));
        assert!(is_register_writable(
            RcServerState::HwConfigured,
            RegisterCategory::HwConfig
        ));
        assert!(!is_register_writable(
            RcServerState::RcpConfigured,
            RegisterCategory::HwConfig
        ));
    }

    #[test]
    // fusa:test REQ-LIFE-009
    // fusa:test REQ-LIFE-010
    fn rcp_config_registers_are_writable_whenever_reachable_including_at_rcp_configured() {
        // Unreachable in HW_UNCONFIGURED, so not writable either -- but for
        // reachability's reason, not a write-lock.
        assert!(!is_register_writable(
            RcServerState::HwUnconfigured,
            RegisterCategory::RcpConfig
        ));
        assert!(is_register_writable(
            RcServerState::HwConfigured,
            RegisterCategory::RcpConfig
        ));
        // Unlike HwConfig, RcpConfig is never permanently locked by this
        // rule -- it remains writable at RCP_CONFIGURED.
        assert!(is_register_writable(
            RcServerState::RcpConfigured,
            RegisterCategory::RcpConfig
        ));
    }

    #[test]
    // fusa:test REQ-LIFE-010
    fn is_register_writable_never_true_for_an_unreachable_category() {
        for state in ALL_STATES {
            for category in ALL_CATEGORIES {
                if !is_register_reachable(state, category) {
                    assert!(!is_register_writable(state, category));
                }
            }
        }
    }

    #[test]
    // fusa:test REQ-LIFE-010
    fn check_register_writable_agrees_with_is_register_writable_and_distinguishes_the_reason() {
        for state in ALL_STATES {
            for category in ALL_CATEGORIES {
                let writable = is_register_writable(state, category);
                let checked = check_register_writable(state, category);
                assert_eq!(checked.is_ok(), writable, "{state:?} {category:?}");
                if !writable {
                    let expected = if !is_register_reachable(state, category) {
                        RcpError::RegisterUnreachable
                    } else {
                        RcpError::RegisterLocked
                    };
                    assert_eq!(checked, Err(expected), "{state:?} {category:?}");
                }
            }
        }
    }

    #[test]
    // fusa:test REQ-LIFE-011
    fn write_lock_checks_never_panic_for_any_state_category_pair() {
        for state in ALL_STATES {
            for category in ALL_CATEGORIES {
                let _ = lock_policy(category);
                let _ = is_register_writable(state, category);
                let _ = check_register_writable(state, category);
            }
        }
    }

    // ── Transition guard: which shapes are defined ───────────────────────

    #[test]
    // fusa:test REQ-LIFE-006
    fn is_transition_defined_true_only_for_the_two_forward_transitions() {
        for from in ALL_STATES {
            for to in ALL_STATES {
                let defined = is_transition_defined(from, to);
                let expected = matches!(
                    (from, to),
                    (RcServerState::HwUnconfigured, RcServerState::HwConfigured)
                        | (RcServerState::HwConfigured, RcServerState::RcpConfigured)
                );
                assert_eq!(defined, expected, "{from:?} -> {to:?}");
            }
        }
    }

    #[test]
    // fusa:test REQ-LIFE-006
    fn try_transition_success_agrees_with_is_transition_defined_when_guard_passes() {
        for from in ALL_STATES {
            for to in ALL_STATES {
                let defined = is_transition_defined(from, to);
                let result = from.try_transition(to, || true);
                assert_eq!(result.is_ok(), defined, "{from:?} -> {to:?}");
            }
        }
    }

    // ── Transition guard: round-trip on the two defined transitions ──────

    #[test]
    // fusa:test REQ-LIFE-007
    fn hw_unconfigured_to_hw_configured_succeeds_when_guard_passes() {
        let result =
            RcServerState::HwUnconfigured.try_transition(RcServerState::HwConfigured, || true);
        assert_eq!(result, Ok(RcServerState::HwConfigured));
    }

    #[test]
    // fusa:test REQ-LIFE-007
    fn hw_unconfigured_to_hw_configured_rejected_when_guard_fails() {
        let result =
            RcServerState::HwUnconfigured.try_transition(RcServerState::HwConfigured, || false);
        assert_eq!(result, Err(RcpError::HwCfgInconsistent));
    }

    #[test]
    // fusa:test REQ-LIFE-007
    fn hw_configured_to_rcp_configured_succeeds_when_guard_passes() {
        let result =
            RcServerState::HwConfigured.try_transition(RcServerState::RcpConfigured, || true);
        assert_eq!(result, Ok(RcServerState::RcpConfigured));
    }

    #[test]
    // fusa:test REQ-LIFE-007
    fn hw_configured_to_rcp_configured_rejected_when_guard_fails() {
        let result =
            RcServerState::HwConfigured.try_transition(RcServerState::RcpConfigured, || false);
        assert_eq!(result, Err(RcpError::RcpCfgInconsistent));
    }

    // ── Transition guard: rejection of every undefined transition ────────

    #[test]
    // fusa:test REQ-LIFE-008
    fn undefined_transitions_are_rejected_without_consulting_the_guard() {
        let undefined_pairs = [
            // Full rollback in one step.
            (RcServerState::RcpConfigured, RcServerState::HwUnconfigured),
            // Skipping HW_CONFIGURED entirely.
            (RcServerState::HwUnconfigured, RcServerState::RcpConfigured),
            // The demotion path — a separate, later roadmap item, not
            // implemented by this function (see this module's doc comment).
            (RcServerState::HwConfigured, RcServerState::HwUnconfigured),
            // A single-step backward move.
            (RcServerState::RcpConfigured, RcServerState::HwConfigured),
            // Staying in the same state, for all three states.
            (RcServerState::HwUnconfigured, RcServerState::HwUnconfigured),
            (RcServerState::HwConfigured, RcServerState::HwConfigured),
            (RcServerState::RcpConfigured, RcServerState::RcpConfigured),
        ];
        for (from, to) in undefined_pairs {
            let mut guard_called = false;
            let result = from.try_transition(to, || {
                guard_called = true;
                true
            });
            assert_eq!(
                result,
                Err(RcpError::InvalidLifecycleTransition),
                "{from:?} -> {to:?}"
            );
            assert!(
                !guard_called,
                "guard should not be consulted for undefined transition {from:?} -> {to:?}"
            );
        }
    }

    // ── Fuzz-style: arbitrary (state, state, guard-result) never panics ──

    #[test]
    // fusa:test REQ-LIFE-008
    fn try_transition_never_panics_for_any_state_pair_or_guard_result() {
        for from in ALL_STATES {
            for to in ALL_STATES {
                let _ = from.try_transition(to, || true);
                let _ = from.try_transition(to, || false);
            }
        }
    }
}
