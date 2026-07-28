// fusa:req REQ-CMP-001
// fusa:req REQ-CMP-002
// fusa:req REQ-CMP-003
// fusa:req REQ-CMP-004
// fusa:req REQ-CMP-005
// fusa:req REQ-CMP-006
// fusa:req REQ-CMP-007
// fusa:req REQ-TRIG-001
// fusa:req REQ-TRIG-002
// fusa:req REQ-TRIG-003
// fusa:req REQ-TRIG-004
// fusa:req REQ-TRIG-005

//! Conditional-request taxonomy: compound / compound-wait (`0x0F`/`0x0B`)
//! and triggered (`0x0E`) — `ROADMAP.md` Milestone 5 ("Conditional
//! Requests & Sequencers"), first and second checklist bullets. The first
//! bullet covers sequencer-gated execution and wait, with
//! `cmp_exec_delay`/`cmpw_exec_delay` timers and the "advance sequencer
//! only if still in start state" rule. The second covers trigger-occurrence
//! counting that runs independent of the target endpoint's busy/idle
//! state, the `trigger_exec_delay` timer, and the infinite-repeat sentinel
//! (`0xFFFF`).
//!
//! Compound/compound-wait was the opening item of Milestone 5, and the
//! first thing to land in `src/request.rs` — the module name the
//! naming-reconciliation pass (issue #35, PR #37, "refactor: reconcile
//! module naming with RELAY spec v1.14 §13.7.2") reserved for this
//! milestone's request-kind/taxonomy work, mirroring `fragment.rs`'s own
//! reservation for Milestone 8. Triggered is the second, added here. Three
//! of this milestone's remaining checklist items (Chained, Timed, and the
//! cancellation trio, plus the "Standard"/unconditional kind implied by the
//! spec's own execution-priority ordering) are still expected to extend
//! [`RequestKind`] and add sibling sections to this module; none of that is
//! attempted here. Same "additive standalone plumbing only" discipline as
//! every prior Milestone 1-4 entry, and as the compound/compound-wait work
//! immediately above: nothing here is wired into a decoder, dispatch loop,
//! or request-lifecycle state machine. The old `src/prioqueue.rs`
//! `Zone`/`Command`/`Controller`/`Priority` decorator this milestone's own
//! Goal text names as the eventual absorption target for "picking which
//! pending request runs next" is read only as background for this change,
//! not extended or touched.
//!
//! Seven named pieces are in scope, all implemented here:
//!
//! - [`RequestKind`] — the request-type discriminant, now covering three
//!   values ([`RequestKind::Compound`] = `0x0F`, [`RequestKind::CompoundWait`]
//!   = `0x0B`, [`RequestKind::Triggered`] = `0x0E`). See "Provenance note:
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
//!   exec-delay timer width and units" below.
//! - [`advance_sequencer_if_still_in_start_state`] — the "advance sequencer
//!   only if still in start state" rule, turned into a pure, testable
//!   function mirroring [`crate::uart::resolve_uart_read_completion`]'s and
//!   [`crate::spi::truncate_spi_status_for_compound_wait`]'s own
//!   prose-rule-to-function precedent.
//! - [`TriggerExecDelay`] / [`resolve_trigger_exec_delay`] — the
//!   `trigger_exec_delay` execution-delay timer, mirroring
//!   [`CompoundExecDelays`]/[`resolve_compound_exec_delay`]'s own
//!   per-kind-timer shape, but for the single [`RequestKind::Triggered`]
//!   kind. See "Provenance note: exec-delay timer width and units" below.
//! - [`TriggerRepeatCount`] / [`is_trigger_repeat_exhausted`] — the
//!   trigger-occurrence repeat count a Triggered request is configured
//!   with, modeled as an explicit `Finite(u16)`/`Infinite` enum rather than
//!   a bare `u16` that would let the checklist's own infinite-repeat
//!   sentinel (`0xFFFF`) silently mean "65535 repeats" instead of "never
//!   exhausts" — mirroring [`crate::gpio::GpioWriteSemantics::Unnamed8th`]'s
//!   and [`crate::i2c::I2cSpeedMode::HighSpeedRowA`]/`HighSpeedRowB`'s own
//!   established "special value gets a named variant, not a silently-folded
//!   number" precedent. See "Provenance note: the infinite-repeat sentinel"
//!   below.
//! - [`should_count_trigger_occurrence`] — the "runs independent of
//!   endpoint busy/idle state" rule, turned into a pure, testable predicate
//!   following this module's own `advance_sequencer_if_still_in_start_state`
//!   precedent. See "Provenance note: busy/idle independence as a
//!   caller-supplied parameter" below.
//!
//! Deliberately out of scope:
//!
//! - The other three request kinds this milestone's checklist still names
//!   (Chained, Timed, and the cancellation trio), and the "Standard"
//!   (unconditional) kind implicit in the spec's own priority ordering.
//!   [`RequestKind`] intentionally leaves room for them but does not add
//!   them.
//! - The persistent 8-bit sequencer-state register machine itself
//!   (`ROADMAP.md` Milestone 5's own "Sequencers" checklist bullet, not yet
//!   built). Every function here that needs a sequencer's current state
//!   takes it as a caller-supplied [`SequencerState`] value, mirroring
//!   [`crate::lifecycle::RcServerState::try_transition`]'s `is_consistent`
//!   closure and [`crate::ep0::check_ep0_access_for_stream`]'s
//!   `root_client` parameter — neither of those blocked on a sibling item
//!   building the thing they read, and neither does this. Triggered
//!   execution's own busy/idle independence needs no such state at all;
//!   see [`should_count_trigger_occurrence`] above.
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
//! `ROADMAP.md`'s checklist bullets name `0x0F`/`0x0B`/`0x0E` as the
//! compound, compound-wait, and triggered discriminant values, but —
//! unlike `acf_msg_type` ([`crate::acf::ACF_ABB_MSG_TYPE`]/
//! [`crate::acf::ACF_GBB_MSG_TYPE`]), whose byte offset within an ACF
//! message header this crate already pinned down in Milestone 1 — no
//! checklist text anywhere in this crate's roadmap states which byte or
//! field of a request actually carries this discriminant. Per Guiding
//! Principle 5, [`RequestKind`] is therefore modeled as a standalone value
//! type with its own `to_u8`/`from_u8` pair, exactly as confident about its
//! named numeric values as the checklist text is, and no more: it is not
//! attached to any offset within [`crate::acf::ByteMessageInfo`] or any
//! other already-built wire shape, and no such offset is guessed here. This
//! reasoning is unchanged by adding [`RequestKind::Triggered`]; it is
//! simply a third value under the same still-open question.
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
//! ## Provenance note: exec-delay timer width and units
//!
//! `ROADMAP.md`'s checklist bullets name `cmp_exec_delay`, `cmpw_exec_delay`,
//! and `trigger_exec_delay` but state none of the three's wire width nor
//! unit of measure. Per Guiding Principle 5, [`CompoundExecDelays`] and
//! [`TriggerExecDelay`] all carry their timer as a plain `u32` elapsed-tick
//! count — this crate's own unconfirmed-width/units placeholder, mirroring
//! [`crate::uart::UartRxQueueConfig::uart_timeout`]'s and
//! [`crate::pwm::PwmInFunctionalConfig::no_signal_timeout`]'s own
//! established precedent for this exact situation — rather than guessing a
//! specific width or resolution. [`resolve_compound_exec_delay`] and
//! [`resolve_trigger_exec_delay`] only select/gate which timer value a
//! given [`RequestKind`] uses; neither interprets the resulting value
//! against any clock or scheduler, since no such wiring exists in this
//! crate yet.
//!
//! ## Provenance note: the infinite-repeat sentinel
//!
//! `ROADMAP.md`'s checklist bullet names `0xFFFF` as an "infinite-repeat
//! sentinel" for a Triggered request's trigger-occurrence count, which this
//! crate reads as confirmation the underlying field is 16 bits wide (the
//! sentinel itself does not fit any narrower width named elsewhere in this
//! crate's roadmap). [`TriggerRepeatCount::from_u16`] maps the raw value
//! `0xFFFF` to [`TriggerRepeatCount::Infinite`] and every other raw value to
//! [`TriggerRepeatCount::Finite`]; consequently a directly-constructed
//! `TriggerRepeatCount::Finite(0xFFFF)` does not round-trip through
//! `to_u16`/`from_u16` back to itself (it decodes as `Infinite`, since the
//! wire has only the one `0xFFFF` value to represent both). This mirrors
//! [`RequestKind`]'s own closed discriminant space: `0xFFFF` on the wire
//! means "infinite", full stop, the same way `0x0B`/`0x0E`/`0x0F` mean their
//! one named [`RequestKind`] each. Callers are not expected to construct
//! `Finite(0xFFFF)` directly; [`TriggerRepeatCount::from_u16`] is the
//! intended construction path from any raw wire value.
//!
//! ## Provenance note: busy/idle independence as a caller-supplied parameter
//!
//! `ROADMAP.md`'s checklist bullet states that Triggered's occurrence
//! counting runs independent of the target endpoint's busy/idle state, but
//! this crate has no unified endpoint busy/idle state type yet — none of
//! the six endpoint-type modules Milestone 4 built (`gpio`, `spi`, `i2c`,
//! `uart`, `pwm`, `adc`) expose one, and building one is out of scope for
//! this item. [`should_count_trigger_occurrence`] therefore takes the
//! endpoint's busy/idle state as a plain caller-supplied `bool` parameter
//! it deliberately ignores — mirroring [`SequencerState`]'s own
//! caller-supplied-rather-than-read precedent above — so the independence
//! rule is expressed as a real, testable function signature rather than a
//! comment, without inventing the endpoint-state type this crate does not
//! yet have.
//!
//! [`crate::gpio::GpioTriggerConfig`]/[`crate::gpio::GpioTriggerSignals`]/
//! [`crate::gpio::evaluate_gpio_triggers`] are a separate, narrower,
//! already-built concept — per-pin change/rising/falling edge-detection
//! arming for the GPIO endpoint type specifically — and are unrelated to
//! this request-kind-level Triggered (`0x0E`) mechanism; nothing here
//! reuses or extends them.

use crate::RcpError;

// ── RequestKind ──────────────────────────────────────────────────────────────

/// The request-type discriminant naming a conditional request's kind.
///
/// Only the three values the checklist bullets built so far name are
/// modeled; see this module's doc comment "Deliberately out of scope"
/// section for why the remaining conditional-request kinds `ROADMAP.md`
/// names elsewhere in this milestone are not yet added as variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
// fusa:req REQ-CMP-001
// fusa:req REQ-TRIG-001
pub enum RequestKind {
    /// Compound-wait (`0x0B`): sequencer-gated execution that waits for its
    /// gate to be satisfied.
    CompoundWait = 0x0B,
    /// Triggered (`0x0E`): trigger-occurrence counting that runs
    /// independent of the target endpoint's busy/idle state.
    Triggered = 0x0E,
    /// Compound (`0x0F`): sequencer-gated execution.
    Compound = 0x0F,
}

impl RequestKind {
    /// Encode this request kind as its discriminant byte.
    // fusa:req REQ-CMP-001
    // fusa:req REQ-TRIG-001
    pub fn to_u8(self) -> u8 {
        self as u8
    }

    /// Decode a discriminant byte into a [`RequestKind`].
    ///
    /// Returns `Err(RcpError::InvalidParameter)` for any value other than
    /// the named discriminants — including the other conditional-request
    /// kinds `ROADMAP.md` names elsewhere in this milestone, which this
    /// module does not yet model. Never panics for any input.
    // fusa:req REQ-CMP-002
    // fusa:req REQ-TRIG-001
    pub fn from_u8(raw: u8) -> Result<Self, RcpError> {
        match raw {
            0x0B => Ok(Self::CompoundWait),
            0x0E => Ok(Self::Triggered),
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

/// Select the execution-delay timer that applies to `kind`, if any.
///
/// Returns `Some` for [`RequestKind::Compound`]/[`RequestKind::CompoundWait`]
/// — the two kinds [`CompoundExecDelays`] applies to — and `None` for
/// every other [`RequestKind`] (e.g. [`RequestKind::Triggered`], which has
/// its own [`TriggerExecDelay`]/[`resolve_trigger_exec_delay`] pair
/// instead), mirroring `resolve_trigger_exec_delay`'s own shape. Never
/// panics for any input.
///
/// This signature widened from a bare `u32` to `Option<u32>` when
/// [`RequestKind::Triggered`] was added as a third [`RequestKind`] variant,
/// since `CompoundExecDelays` has no field a Triggered request could
/// select. Not yet called from anywhere in this crate (see this module's
/// doc comment for why), so this is a safe additive-plumbing-stage
/// widening, not a breaking change to any consumer.
// fusa:req REQ-CMP-006
pub fn resolve_compound_exec_delay(kind: RequestKind, delays: &CompoundExecDelays) -> Option<u32> {
    match kind {
        RequestKind::Compound => Some(delays.cmp_exec_delay),
        RequestKind::CompoundWait => Some(delays.cmpw_exec_delay),
        RequestKind::Triggered => None,
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

// ── Triggered (0x0E): trigger_exec_delay ─────────────────────────────────────

/// The `trigger_exec_delay` execution-delay timer this checklist bullet
/// names for [`RequestKind::Triggered`].
///
/// See this module's doc comment "Provenance note: exec-delay timer width
/// and units" for why this is a plain `u32` placeholder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
// fusa:req REQ-TRIG-002
pub struct TriggerExecDelay(pub u32);

/// Select the execution-delay timer that applies to `kind`, if any.
///
/// Returns `Some(delay.0)` when `kind` is [`RequestKind::Triggered`] —
/// the only kind [`TriggerExecDelay`] applies to — and `None` for every
/// other [`RequestKind`], mirroring [`resolve_compound_exec_delay`]'s own
/// per-kind-timer-selection shape. Never panics for any input.
// fusa:req REQ-TRIG-002
pub fn resolve_trigger_exec_delay(kind: RequestKind, delay: TriggerExecDelay) -> Option<u32> {
    match kind {
        RequestKind::Triggered => Some(delay.0),
        RequestKind::Compound | RequestKind::CompoundWait => None,
    }
}

// ── Triggered (0x0E): trigger-occurrence repeat count ────────────────────────

/// A Triggered request's configured trigger-occurrence repeat count: either
/// a finite target occurrence count, or the infinite-repeat sentinel
/// `ROADMAP.md`'s checklist bullet names.
///
/// See this module's doc comment "Provenance note: the infinite-repeat
/// sentinel" for the reasoning behind modeling this as an explicit enum,
/// and for why a directly-constructed `Finite(0xFFFF)` does not round-trip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// fusa:req REQ-TRIG-003
pub enum TriggerRepeatCount {
    /// A finite number of trigger occurrences this request repeats for.
    Finite(u16),
    /// The infinite-repeat sentinel (`0xFFFF`): this request's
    /// trigger-occurrence count never exhausts on its own.
    Infinite,
}

/// The raw wire value `ROADMAP.md`'s checklist bullet names as the
/// infinite-repeat sentinel for a Triggered request's occurrence count.
// fusa:req REQ-TRIG-003
pub const TRIGGER_REPEAT_COUNT_INFINITE_SENTINEL: u16 = 0xFFFF;

impl TriggerRepeatCount {
    /// Decode a raw 16-bit occurrence-count value into a
    /// [`TriggerRepeatCount`]: [`Self::Infinite`] for
    /// [`TRIGGER_REPEAT_COUNT_INFINITE_SENTINEL`], [`Self::Finite`]
    /// otherwise. Never panics for any input.
    // fusa:req REQ-TRIG-003
    pub fn from_u16(raw: u16) -> Self {
        if raw == TRIGGER_REPEAT_COUNT_INFINITE_SENTINEL {
            Self::Infinite
        } else {
            Self::Finite(raw)
        }
    }

    /// Encode this repeat count back to its raw 16-bit wire value.
    ///
    /// See this module's doc comment "Provenance note: the infinite-repeat
    /// sentinel" for why `Self::Finite(0xFFFF)` — not reachable via
    /// [`Self::from_u16`] — encodes to the same sentinel value as
    /// [`Self::Infinite`] rather than round-tripping to itself.
    // fusa:req REQ-TRIG-003
    pub fn to_u16(self) -> u16 {
        match self {
            Self::Finite(n) => n,
            Self::Infinite => TRIGGER_REPEAT_COUNT_INFINITE_SENTINEL,
        }
    }
}

/// Whether a Triggered request's trigger-occurrence count is exhausted:
/// always `false` for [`TriggerRepeatCount::Infinite`] (it never exhausts on
/// its own), and `true` once `occurrences_so_far` has reached or passed the
/// finite configured target. Never panics for any input.
// fusa:req REQ-TRIG-004
pub fn is_trigger_repeat_exhausted(
    occurrences_so_far: u16,
    repeat_count: TriggerRepeatCount,
) -> bool {
    match repeat_count {
        TriggerRepeatCount::Infinite => false,
        TriggerRepeatCount::Finite(target) => occurrences_so_far >= target,
    }
}

// ── Triggered (0x0E): busy/idle-independent occurrence counting ─────────────

/// Whether a trigger event should count as a trigger occurrence for a
/// Triggered request: always `true`, regardless of `endpoint_busy` — the
/// property this checklist bullet names as running "independent of endpoint
/// busy/idle state", distinguishing Triggered from the sequencer-gated
/// compound/compound-wait kinds this module also models. Never panics for
/// any input.
///
/// See this module's doc comment "Provenance note: busy/idle independence
/// as a caller-supplied parameter" for why `endpoint_busy` is taken (and
/// deliberately ignored) rather than omitted outright.
// fusa:req REQ-TRIG-005
pub fn should_count_trigger_occurrence(endpoint_busy: bool) -> bool {
    let _ = endpoint_busy;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── RequestKind: discriminant round-trip / rejection ────────────────────

    const ALL_REQUEST_KINDS: [RequestKind; 3] = [
        RequestKind::CompoundWait,
        RequestKind::Triggered,
        RequestKind::Compound,
    ];

    #[test]
    // fusa:test REQ-CMP-001
    // fusa:test REQ-TRIG-001
    fn request_kind_round_trips_through_to_u8_from_u8() {
        for kind in ALL_REQUEST_KINDS {
            assert_eq!(RequestKind::from_u8(kind.to_u8()), Ok(kind));
        }
    }

    #[test]
    // fusa:test REQ-CMP-001
    // fusa:test REQ-TRIG-001
    fn request_kind_discriminants_match_roadmap_named_values() {
        assert_eq!(RequestKind::Compound.to_u8(), 0x0F);
        assert_eq!(RequestKind::CompoundWait.to_u8(), 0x0B);
        assert_eq!(RequestKind::Triggered.to_u8(), 0x0E);
    }

    #[test]
    // fusa:test REQ-CMP-002
    fn request_kind_from_u8_rejects_every_other_value() {
        for raw in [0x00u8, 0x01, 0x0A, 0x0C, 0x10, 0x7F, 0xFF] {
            assert_eq!(RequestKind::from_u8(raw), Err(RcpError::InvalidParameter));
        }
    }

    #[test]
    // fusa:test REQ-TRIG-001
    fn request_kind_from_u8_accepts_triggered_discriminant() {
        assert_eq!(RequestKind::from_u8(0x0E), Ok(RequestKind::Triggered));
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
            Some(100)
        );
        assert_eq!(
            resolve_compound_exec_delay(RequestKind::CompoundWait, &delays),
            Some(200)
        );
    }

    #[test]
    // fusa:test REQ-CMP-006
    fn resolve_compound_exec_delay_is_none_for_triggered() {
        let delays = CompoundExecDelays {
            cmp_exec_delay: 100,
            cmpw_exec_delay: 200,
        };
        assert_eq!(
            resolve_compound_exec_delay(RequestKind::Triggered, &delays),
            None
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

    // ── TriggerExecDelay / resolve_trigger_exec_delay ────────────────────────

    #[test]
    // fusa:test REQ-TRIG-002
    fn trigger_exec_delay_default_is_zero() {
        assert_eq!(TriggerExecDelay::default(), TriggerExecDelay(0));
    }

    #[test]
    // fusa:test REQ-TRIG-002
    fn resolve_trigger_exec_delay_selects_the_timer_only_for_triggered() {
        let delay = TriggerExecDelay(42);
        assert_eq!(
            resolve_trigger_exec_delay(RequestKind::Triggered, delay),
            Some(42)
        );
    }

    #[test]
    // fusa:test REQ-TRIG-002
    fn resolve_trigger_exec_delay_is_none_for_every_other_kind() {
        let delay = TriggerExecDelay(42);
        assert_eq!(
            resolve_trigger_exec_delay(RequestKind::Compound, delay),
            None
        );
        assert_eq!(
            resolve_trigger_exec_delay(RequestKind::CompoundWait, delay),
            None
        );
    }

    // ── TriggerRepeatCount ────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-TRIG-003
    fn trigger_repeat_count_from_u16_maps_sentinel_to_infinite() {
        assert_eq!(
            TriggerRepeatCount::from_u16(TRIGGER_REPEAT_COUNT_INFINITE_SENTINEL),
            TriggerRepeatCount::Infinite
        );
        assert_eq!(
            TriggerRepeatCount::from_u16(0xFFFF),
            TriggerRepeatCount::Infinite
        );
    }

    #[test]
    // fusa:test REQ-TRIG-003
    fn trigger_repeat_count_from_u16_maps_every_other_value_to_finite() {
        for raw in [0u16, 1, 42, 0xFFFE] {
            assert_eq!(
                TriggerRepeatCount::from_u16(raw),
                TriggerRepeatCount::Finite(raw)
            );
        }
    }

    #[test]
    // fusa:test REQ-TRIG-003
    fn trigger_repeat_count_finite_round_trips_through_to_u16_from_u16() {
        for raw in [0u16, 1, 42, 0xFFFE] {
            let count = TriggerRepeatCount::from_u16(raw);
            assert_eq!(TriggerRepeatCount::from_u16(count.to_u16()), count);
        }
    }

    #[test]
    // fusa:test REQ-TRIG-003
    fn trigger_repeat_count_infinite_round_trips_through_to_u16_from_u16() {
        let count = TriggerRepeatCount::Infinite;
        assert_eq!(count.to_u16(), TRIGGER_REPEAT_COUNT_INFINITE_SENTINEL);
        assert_eq!(TriggerRepeatCount::from_u16(count.to_u16()), count);
    }

    #[test]
    // fusa:test REQ-TRIG-003
    fn trigger_repeat_count_directly_constructed_finite_sentinel_collapses_to_infinite() {
        // See this module's doc comment "Provenance note: the
        // infinite-repeat sentinel" — this is the one deliberate,
        // documented non-round-trip in this type.
        let count = TriggerRepeatCount::Finite(0xFFFF);
        assert_eq!(count.to_u16(), TRIGGER_REPEAT_COUNT_INFINITE_SENTINEL);
        assert_eq!(
            TriggerRepeatCount::from_u16(count.to_u16()),
            TriggerRepeatCount::Infinite
        );
    }

    // ── is_trigger_repeat_exhausted ───────────────────────────────────────────

    #[test]
    // fusa:test REQ-TRIG-004
    fn is_trigger_repeat_exhausted_is_always_false_for_infinite() {
        for occurrences in [0u16, 1, 100, u16::MAX] {
            assert!(!is_trigger_repeat_exhausted(
                occurrences,
                TriggerRepeatCount::Infinite
            ));
        }
    }

    #[test]
    // fusa:test REQ-TRIG-004
    fn is_trigger_repeat_exhausted_true_once_occurrences_reach_finite_target() {
        let target = TriggerRepeatCount::Finite(3);
        assert!(!is_trigger_repeat_exhausted(0, target));
        assert!(!is_trigger_repeat_exhausted(2, target));
        assert!(is_trigger_repeat_exhausted(3, target));
        assert!(is_trigger_repeat_exhausted(4, target));
    }

    #[test]
    // fusa:test REQ-TRIG-004
    fn is_trigger_repeat_exhausted_never_panics_for_any_sampled_input() {
        for occurrences in [0u16, 1, 3, u16::MAX] {
            for target in [0u16, 3, u16::MAX] {
                let _ =
                    is_trigger_repeat_exhausted(occurrences, TriggerRepeatCount::Finite(target));
            }
            let _ = is_trigger_repeat_exhausted(occurrences, TriggerRepeatCount::Infinite);
        }
    }

    // ── should_count_trigger_occurrence ───────────────────────────────────────

    #[test]
    // fusa:test REQ-TRIG-005
    fn should_count_trigger_occurrence_is_always_true_regardless_of_busy_state() {
        assert!(should_count_trigger_occurrence(true));
        assert!(should_count_trigger_occurrence(false));
    }
}
