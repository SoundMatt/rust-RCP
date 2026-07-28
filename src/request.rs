// fusa:req REQ-CMP-001
// fusa:req REQ-CMP-002
// fusa:req REQ-CMP-003
// fusa:req REQ-CMP-004
// fusa:req REQ-CMP-005
// fusa:req REQ-CMP-006
// fusa:req REQ-CMP-007

//! Conditional-request taxonomy: compound / compound-wait (`0x0F`/`0x0B`) —
//! `ROADMAP.md` Milestone 5 ("Conditional Requests & Sequencers"), first
//! checklist bullet: sequencer-gated execution and wait;
//! `cmp_exec_delay`/`cmpw_exec_delay` timers; "advance sequencer only if
//! still in start state" rule.
//!
//! This is the opening item of Milestone 5, and the first thing to land in
//! `src/request.rs` — the module name the naming-reconciliation pass
//! (issue #35, PR #37, "refactor: reconcile module naming with RELAY spec
//! v1.14 §13.7.2") reserved for this milestone's request-kind/taxonomy
//! work, mirroring `fragment.rs`'s own reservation for Milestone 8. Four of
//! this milestone's later checklist items (Triggered, Chained, Timed, and
//! the cancellation trio, plus the "Standard"/unconditional kind implied by
//! the spec's own execution-priority ordering) are expected to extend
//! [`RequestKind`] and add sibling sections to this module; none of that is
//! attempted here. Same "additive standalone plumbing only" discipline as
//! every prior Milestone 1-4 entry: nothing here is wired into a decoder,
//! dispatch loop, or request-lifecycle state machine. The old
//! `src/prioqueue.rs` `Zone`/`Command`/`Controller`/`Priority` decorator
//! this milestone's own Goal text names as the eventual absorption target
//! for "picking which pending request runs next" is read only as
//! background for this change, not extended or touched.
//!
//! Four named pieces are in scope, all implemented here:
//!
//! - [`RequestKind`] — the request-type discriminant, covering the two
//!   values this checklist bullet names ([`RequestKind::Compound`] =
//!   `0x0F`, [`RequestKind::CompoundWait`] = `0x0B`). See "Provenance note:
//!   `RequestKind`'s wire placement" below for why this is modeled as a
//!   standalone value type, not yet tied to a decoded byte offset.
//! - [`CompoundGateConfig`] / [`SequencerState`] /
//!   [`check_sequencer_num_in_bounds`] / [`is_gate_satisfied`] /
//!   [`check_compound_gate`] — the sequencer-gating rule: a compound(-wait)
//!   request executes only if the sequencer it names currently holds the
//!   request's configured start state. See "Provenance note: `start_state`
//!   and the not-yet-built sequencer-state machine" below for how this
//!   relates to [`crate::regmap::SequencerStateEntry`].
//! - [`CompoundExecDelays`] / [`resolve_compound_exec_delay`] — the
//!   `cmp_exec_delay`/`cmpw_exec_delay` execution-delay timers, one field
//!   per request kind, selected by [`RequestKind`]. See "Provenance note:
//!   `cmp_exec_delay`/`cmpw_exec_delay` width and units" below.
//! - [`advance_sequencer_if_still_in_start_state`] — the "advance sequencer
//!   only if still in start state" rule, turned into a pure, testable
//!   function mirroring [`crate::uart::resolve_uart_read_completion`]'s and
//!   [`crate::spi::truncate_spi_status_for_compound_wait`]'s own
//!   prose-rule-to-function precedent.
//!
//! Deliberately out of scope:
//!
//! - The other four request kinds this milestone's checklist names
//!   (Triggered, Chained, Timed, and the cancellation trio), and the
//!   "Standard" (unconditional) kind implicit in the spec's own priority
//!   ordering. [`RequestKind`] intentionally leaves room for them but does
//!   not add them.
//! - The persistent 8-bit sequencer-state register machine itself
//!   (`ROADMAP.md` Milestone 5's own next checklist bullet, "Sequencers").
//!   Every function here that needs a sequencer's current state takes it
//!   as a caller-supplied [`SequencerState`] value, mirroring
//!   [`crate::lifecycle::RcServerState::try_transition`]'s `is_consistent`
//!   closure and [`crate::ep0::check_ep0_access_for_stream`]'s
//!   `root_client` parameter — neither of those blocked on a sibling item
//!   building the thing they read, and neither does this.
//! - Wiring any of the below into an actual decoder, dispatch loop, or
//!   request-lifecycle state machine (`ROADMAP.md`'s own "Request
//!   lifecycle state machine" checklist bullet, later in this milestone).
//! - The old `src/prioqueue.rs` model this milestone's Goal text names as
//!   the eventual absorption target for "picking which pending request
//!   runs next" — that absorption is the separate "Execution priority
//!   ordering" checklist bullet, not this one.
//!
//! ## Provenance note: `RequestKind`'s wire placement
//!
//! `ROADMAP.md`'s checklist bullet names `0x0F`/`0x0B` as the compound and
//! compound-wait discriminant values, but — unlike `acf_msg_type`
//! ([`crate::acf::ACF_ABB_MSG_TYPE`]/[`crate::acf::ACF_GBB_MSG_TYPE`]),
//! whose byte offset within an ACF message header this crate already
//! pinned down in Milestone 1 — no checklist text anywhere in this crate's
//! roadmap states which byte or field of a request actually carries this
//! discriminant. Per Guiding Principle 5, [`RequestKind`] is therefore
//! modeled as a standalone value type with its own `to_u8`/`from_u8` pair,
//! exactly as confident about its two named numeric values as the
//! checklist text is, and no more: it is not attached to any offset within
//! [`crate::acf::ByteMessageInfo`] or any other already-built wire shape,
//! and no such offset is guessed here.
//!
//! ## Provenance note: `start_state` and the not-yet-built sequencer-state
//! machine
//!
//! The gating rule this checklist bullet names — "sequencer-gated
//! execution" — requires comparing a compound(-wait) request's configured
//! start state against a sequencer's actual current state. The persistent
//! state register that would hold that "current state" is `ROADMAP.md`
//! Milestone 5's own next checklist bullet ("Sequencers"), not yet built in
//! this crate: only [`crate::regmap::SequencerStateEntry`]'s row *shape*
//! (power-on default `1`, single-byte encoding) exists so far, from
//! Milestone 2's config-table work. This module assumes a sequencer's
//! current state is representable as the same single unstructured byte
//! [`crate::regmap::SequencerStateEntry::seq_state`] already models,
//! wrapped here as [`SequencerState`] — but takes every current-state value
//! as a caller-supplied parameter rather than reading it from a register
//! this crate cannot yet provide. Which specific sequencer a request names
//! is likewise modeled as a plain `u8` sequencer number
//! ([`CompoundGateConfig::sequencer_num`]), mirroring
//! [`crate::regmap::RequestStreamConfigEntry::rx_safestate_sequencer`]'s
//! own established "sequencer number is a plain byte" precedent — not a
//! transcription of a confirmed compound-request wire field name.
//!
//! [`CompoundGateConfig`] likewise names no "next state" a successful
//! execution should advance a sequencer to; `ROADMAP.md`'s checklist text
//! says only that the sequencer advances, not to which value (unlike
//! [`crate::regmap::RequestStreamConfigEntry::rx_safe_sequencer_state`]'s
//! own named target-state field for a different, safe-state-entry
//! purpose). So [`advance_sequencer_if_still_in_start_state`] takes the
//! target state as an explicit caller-supplied parameter too, rather than
//! this crate inventing an increment-by-one or other advancement
//! convention.
//!
//! ## Provenance note: `cmp_exec_delay`/`cmpw_exec_delay` width and units
//!
//! `ROADMAP.md`'s checklist bullet names both timers but states neither's
//! wire width nor its unit of measure. Per Guiding Principle 5,
//! [`CompoundExecDelays`] carries both as plain `u32` elapsed-tick counts —
//! this crate's own unconfirmed-width/units placeholder, mirroring
//! [`crate::uart::UartRxQueueConfig::uart_timeout`]'s and
//! [`crate::pwm::PwmInFunctionalConfig::no_signal_timeout`]'s own
//! established precedent for this exact situation — rather than guessing a
//! specific width or resolution. [`resolve_compound_exec_delay`] only
//! selects which of the two fields a given [`RequestKind`] uses; it does
//! not interpret the resulting value against any clock or scheduler, since
//! no such wiring exists in this crate yet.

use crate::RcpError;

// ── RequestKind ──────────────────────────────────────────────────────────────

/// The request-type discriminant naming a conditional request's kind.
///
/// Only the two values this checklist bullet names are modeled; see this
/// module's doc comment "Deliberately out of scope" section for why the
/// remaining conditional-request kinds `ROADMAP.md` names elsewhere in this
/// milestone are not yet added as variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
// fusa:req REQ-CMP-001
pub enum RequestKind {
    /// Compound-wait (`0x0B`): sequencer-gated execution that waits for its
    /// gate to be satisfied.
    CompoundWait = 0x0B,
    /// Compound (`0x0F`): sequencer-gated execution.
    Compound = 0x0F,
}

impl RequestKind {
    /// Encode this request kind as its discriminant byte.
    // fusa:req REQ-CMP-001
    pub fn to_u8(self) -> u8 {
        self as u8
    }

    /// Decode a discriminant byte into a [`RequestKind`].
    ///
    /// Returns `Err(RcpError::InvalidParameter)` for any value other than
    /// the two named discriminants — including the other conditional-
    /// request kinds `ROADMAP.md` names elsewhere in this milestone, which
    /// this module does not yet model. Never panics for any input.
    // fusa:req REQ-CMP-002
    pub fn from_u8(raw: u8) -> Result<Self, RcpError> {
        match raw {
            0x0B => Ok(Self::CompoundWait),
            0x0F => Ok(Self::Compound),
            _ => Err(RcpError::InvalidParameter),
        }
    }
}

// ── Sequencer gating ─────────────────────────────────────────────────────────

/// A sequencer's persistent state value.
///
/// Mirrors [`crate::regmap::SequencerStateEntry::seq_state`]'s single-byte
/// shape — see this module's doc comment "Provenance note: `start_state`
/// and the not-yet-built sequencer-state machine" for why this crate does
/// not yet read this value from an actual register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
// fusa:req REQ-CMP-003
pub struct SequencerState(pub u8);

/// A compound/compound-wait request's sequencer gate: which sequencer it
/// names, and the persistent state that sequencer must hold for this
/// request to execute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
// fusa:req REQ-CMP-003
pub struct CompoundGateConfig {
    /// The sequencer number this request is gated on, mirroring
    /// [`crate::regmap::RequestStreamConfigEntry::rx_safestate_sequencer`]'s
    /// own "plain byte" sequencer-number precedent.
    pub sequencer_num: u8,
    /// The persistent sequencer state this request requires before it may
    /// execute.
    pub start_state: SequencerState,
}

/// Validate `sequencer_num` against the sequencer-count bound
/// [`crate::regmap::GeneralRegisters::svr_sequencers_max`] already models.
///
/// Returns `Err(RcpError::SequencerNotKnown)` if `sequencer_num` is not
/// less than `svr_sequencers_max` (`0` meaning no sequencers exist at all
/// — every `sequencer_num` is then out of bounds). Never panics for any
/// input.
// fusa:req REQ-CMP-004
pub fn check_sequencer_num_in_bounds(
    sequencer_num: u8,
    svr_sequencers_max: u8,
) -> Result<(), RcpError> {
    if sequencer_num < svr_sequencers_max {
        Ok(())
    } else {
        Err(RcpError::SequencerNotKnown)
    }
}

/// Whether a compound/compound-wait request's gate is currently satisfied:
/// `current_state` equals `gate.start_state`.
///
/// Never panics for any input.
// fusa:req REQ-CMP-005
pub fn is_gate_satisfied(current_state: SequencerState, gate: &CompoundGateConfig) -> bool {
    current_state == gate.start_state
}

/// The full sequencer-gating check: `gate.sequencer_num` must be a known
/// sequencer per `svr_sequencers_max` ([`check_sequencer_num_in_bounds`]),
/// and `current_state` must satisfy `gate` ([`is_gate_satisfied`]).
///
/// Returns `Err(RcpError::SequencerNotKnown)` for an out-of-bounds
/// sequencer number, or `Err(RcpError::RequestRejected)` if the sequencer
/// is known but not currently in the request's start state. Never panics
/// for any input.
// fusa:req REQ-CMP-004
// fusa:req REQ-CMP-005
pub fn check_compound_gate(
    current_state: SequencerState,
    gate: &CompoundGateConfig,
    svr_sequencers_max: u8,
) -> Result<(), RcpError> {
    check_sequencer_num_in_bounds(gate.sequencer_num, svr_sequencers_max)?;
    if is_gate_satisfied(current_state, gate) {
        Ok(())
    } else {
        Err(RcpError::RequestRejected)
    }
}

// ── Execution-delay timers ───────────────────────────────────────────────────

/// The `cmp_exec_delay`/`cmpw_exec_delay` execution-delay timers this
/// checklist bullet names, one per request kind.
///
/// See this module's doc comment "Provenance note:
/// `cmp_exec_delay`/`cmpw_exec_delay` width and units" for why both are
/// plain `u32` placeholders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
// fusa:req REQ-CMP-006
pub struct CompoundExecDelays {
    /// The `0x0F` compound request kind's execution-delay timer.
    pub cmp_exec_delay: u32,
    /// The `0x0B` compound-wait request kind's execution-delay timer.
    pub cmpw_exec_delay: u32,
}

/// Select the execution-delay timer that applies to `kind`.
///
/// Never panics for any input.
// fusa:req REQ-CMP-006
pub fn resolve_compound_exec_delay(kind: RequestKind, delays: &CompoundExecDelays) -> u32 {
    match kind {
        RequestKind::Compound => delays.cmp_exec_delay,
        RequestKind::CompoundWait => delays.cmpw_exec_delay,
    }
}

// ── "Advance only if still in start state" rule ──────────────────────────────

/// Attempt to advance a sequencer's persistent state following a
/// successful compound/compound-wait execution.
///
/// `observed_state` is the sequencer's persistent state read at the moment
/// of the advance attempt — which, per this checklist bullet's own
/// wording, may differ from the state observed at gate-check time
/// ([`check_compound_gate`]) if some other request raced ahead and moved
/// the sequencer in between. Returns `Some(next_state)` only if
/// `observed_state` still equals `gate.start_state`; returns `None`
/// (meaning: do not advance — the race was lost) if it does not. Never
/// panics for any input.
///
/// `next_state` is caller-supplied — see this module's doc comment for why
/// no advancement convention (increment-by-one or otherwise) is guessed
/// here.
// fusa:req REQ-CMP-007
pub fn advance_sequencer_if_still_in_start_state(
    observed_state: SequencerState,
    gate: &CompoundGateConfig,
    next_state: SequencerState,
) -> Option<SequencerState> {
    if observed_state == gate.start_state {
        Some(next_state)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── RequestKind: discriminant round-trip / rejection ────────────────────

    const ALL_REQUEST_KINDS: [RequestKind; 2] = [RequestKind::CompoundWait, RequestKind::Compound];

    #[test]
    // fusa:test REQ-CMP-001
    fn request_kind_round_trips_through_to_u8_from_u8() {
        for kind in ALL_REQUEST_KINDS {
            assert_eq!(RequestKind::from_u8(kind.to_u8()), Ok(kind));
        }
    }

    #[test]
    // fusa:test REQ-CMP-001
    fn request_kind_discriminants_match_roadmap_named_values() {
        assert_eq!(RequestKind::Compound.to_u8(), 0x0F);
        assert_eq!(RequestKind::CompoundWait.to_u8(), 0x0B);
    }

    #[test]
    // fusa:test REQ-CMP-002
    fn request_kind_from_u8_rejects_every_other_value() {
        for raw in [0x00u8, 0x01, 0x0A, 0x0C, 0x0E, 0x10, 0x7F, 0xFF] {
            assert_eq!(RequestKind::from_u8(raw), Err(RcpError::InvalidParameter));
        }
    }

    #[test]
    // fusa:test REQ-CMP-002
    fn request_kind_from_u8_never_panics_across_the_full_byte_range() {
        for raw in 0u8..=255 {
            let _ = RequestKind::from_u8(raw);
        }
    }

    // ── SequencerState / CompoundGateConfig ──────────────────────────────────

    #[test]
    // fusa:test REQ-CMP-003
    fn sequencer_state_default_is_zero() {
        assert_eq!(SequencerState::default(), SequencerState(0));
    }

    #[test]
    // fusa:test REQ-CMP-003
    fn compound_gate_config_default_is_sequencer_zero_state_zero() {
        let gate = CompoundGateConfig::default();
        assert_eq!(gate.sequencer_num, 0);
        assert_eq!(gate.start_state, SequencerState(0));
    }

    // ── check_sequencer_num_in_bounds ────────────────────────────────────────

    #[test]
    // fusa:test REQ-CMP-004
    fn check_sequencer_num_in_bounds_accepts_every_num_below_max() {
        for max in [1u8, 4, 255] {
            for num in 0..max {
                assert_eq!(check_sequencer_num_in_bounds(num, max), Ok(()));
            }
        }
    }

    #[test]
    // fusa:test REQ-CMP-004
    fn check_sequencer_num_in_bounds_rejects_num_at_or_above_max() {
        for (num, max) in [(0u8, 0u8), (4, 4), (5, 4), (255, 4)] {
            assert_eq!(
                check_sequencer_num_in_bounds(num, max),
                Err(RcpError::SequencerNotKnown)
            );
        }
    }

    #[test]
    // fusa:test REQ-CMP-004
    fn check_sequencer_num_in_bounds_never_panics_for_any_sampled_pair() {
        for num in [0u8, 1, 127, 255] {
            for max in [0u8, 1, 127, 255] {
                let _ = check_sequencer_num_in_bounds(num, max);
            }
        }
    }

    // ── is_gate_satisfied / check_compound_gate ──────────────────────────────

    fn sample_gate() -> CompoundGateConfig {
        CompoundGateConfig {
            sequencer_num: 2,
            start_state: SequencerState(1),
        }
    }

    #[test]
    // fusa:test REQ-CMP-005
    fn is_gate_satisfied_true_only_when_current_state_matches_start_state() {
        let gate = sample_gate();
        assert!(is_gate_satisfied(SequencerState(1), &gate));
        assert!(!is_gate_satisfied(SequencerState(0), &gate));
        assert!(!is_gate_satisfied(SequencerState(2), &gate));
    }

    #[test]
    // fusa:test REQ-CMP-005
    fn check_compound_gate_ok_when_sequencer_known_and_state_matches() {
        let gate = sample_gate();
        assert_eq!(check_compound_gate(SequencerState(1), &gate, 4), Ok(()));
    }

    #[test]
    // fusa:test REQ-CMP-004
    fn check_compound_gate_rejects_out_of_bounds_sequencer_before_checking_state() {
        let gate = sample_gate();
        // svr_sequencers_max of 2 puts sequencer_num 2 out of bounds, even
        // though the state would otherwise satisfy the gate.
        assert_eq!(
            check_compound_gate(SequencerState(1), &gate, 2),
            Err(RcpError::SequencerNotKnown)
        );
    }

    #[test]
    // fusa:test REQ-CMP-005
    fn check_compound_gate_rejects_mismatched_state_for_a_known_sequencer() {
        let gate = sample_gate();
        assert_eq!(
            check_compound_gate(SequencerState(0), &gate, 4),
            Err(RcpError::RequestRejected)
        );
    }

    #[test]
    // fusa:test REQ-CMP-005
    fn check_compound_gate_never_panics_for_any_sampled_input() {
        let gate = sample_gate();
        for state in [0u8, 1, 2, 255] {
            for max in [0u8, 1, 2, 255] {
                let _ = check_compound_gate(SequencerState(state), &gate, max);
            }
        }
    }

    // ── CompoundExecDelays / resolve_compound_exec_delay ─────────────────────

    #[test]
    // fusa:test REQ-CMP-006
    fn compound_exec_delays_default_is_zero_for_both_timers() {
        let delays = CompoundExecDelays::default();
        assert_eq!(delays.cmp_exec_delay, 0);
        assert_eq!(delays.cmpw_exec_delay, 0);
    }

    #[test]
    // fusa:test REQ-CMP-006
    fn resolve_compound_exec_delay_selects_the_matching_timer() {
        let delays = CompoundExecDelays {
            cmp_exec_delay: 100,
            cmpw_exec_delay: 200,
        };
        assert_eq!(
            resolve_compound_exec_delay(RequestKind::Compound, &delays),
            100
        );
        assert_eq!(
            resolve_compound_exec_delay(RequestKind::CompoundWait, &delays),
            200
        );
    }

    // ── advance_sequencer_if_still_in_start_state ────────────────────────────

    #[test]
    // fusa:test REQ-CMP-007
    fn advance_sequencer_advances_when_still_in_start_state() {
        let gate = sample_gate();
        assert_eq!(
            advance_sequencer_if_still_in_start_state(SequencerState(1), &gate, SequencerState(3)),
            Some(SequencerState(3))
        );
    }

    #[test]
    // fusa:test REQ-CMP-007
    fn advance_sequencer_refuses_when_race_moved_it_out_of_start_state() {
        let gate = sample_gate();
        assert_eq!(
            advance_sequencer_if_still_in_start_state(SequencerState(9), &gate, SequencerState(3)),
            None
        );
    }

    #[test]
    // fusa:test REQ-CMP-007
    fn advance_sequencer_never_panics_for_any_sampled_input() {
        let gate = sample_gate();
        for observed in [0u8, 1, 9, 255] {
            for next in [0u8, 3, 255] {
                let _ = advance_sequencer_if_still_in_start_state(
                    SequencerState(observed),
                    &gate,
                    SequencerState(next),
                );
            }
        }
    }
}
