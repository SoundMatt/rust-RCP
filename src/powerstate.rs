// fusa:req REQ-PWR-001
// fusa:req REQ-PWR-002
// fusa:req REQ-PWR-003
// fusa:req REQ-PWR-004
// fusa:req REQ-PWR-005
// fusa:req REQ-PWR-006
// fusa:req REQ-PWR-007
// fusa:req REQ-PWR-008
// fusa:req REQ-PWRSTART-001
// fusa:req REQ-PWRSTART-002
// fusa:req REQ-PWRSTART-003

//! Power-mode model (`ROADMAP.md` Milestone 6, "Real power-mode model
//! backing the safe-state work" bullet).
//!
//! REPLACE of this crate's old ad-hoc power model: the previous
//! [`PowerState`]/[`PowerStateController`] pair (removed by this item, per
//! `ROADMAP.md`'s own Satellite Package Disposition table entry for this
//! file) was a three-state `Active`/`Sleep`/`Standby` decorator keyed on a
//! `Command{cmd_type: CommandType::SLEEP/WAKE}` sent through a `Zone`-keyed
//! `Controller` — the same legacy private-protocol shape this whole
//! rewrite is retiring. Nothing in the real OPEN Alliance TC18 Remote
//! Control Protocol Specification v0.5.1_RC works that way. This module
//! models the spec's real four-mode power model instead:
//!
//! - [`PowerMode`] — the four power modes themselves: [`PowerMode::Normal`],
//!   [`PowerMode::StandBy`], [`PowerMode::Sleep`], and
//!   [`PowerMode::Unpowered`]. See "Provenance note: mode ordering and
//!   `Unpowered`'s software-model semantics" below for how this crate reads
//!   the four names' own relative depth and for what `Unpowered` can and
//!   cannot mean for a running software model.
//! - [`is_power_mode_transition_defined`] — the coarse, state-shape check
//!   (mirroring [`crate::lifecycle::is_transition_defined`]'s own role)
//!   naming exactly which powered-mode pairs this module implements a
//!   transition between: `Normal` <-> `StandBy` and `StandBy` <-> `Sleep`.
//!   `Unpowered` is deliberately excluded from this check — see
//!   [`shutdown_to_unpowered`] and the cold/hot-start functions below for
//!   why entering or leaving `Unpowered` is never a member of this
//!   ordinary, gated transition set.
//! - [`PowerModeGateInput`] / [`is_power_mode_gate_satisfied`] — the
//!   shared entry/exit precondition `ROADMAP.md`'s own checklist wording
//!   names as the reason this item is sequenced into Milestone 6 at all:
//!   every reachable endpoint must be idle and no request may still have a
//!   response outstanding. See "Provenance note: the idle/no-pending-
//!   response gate as caller-supplied facts, composed from
//!   `crate::request::RequestLifecycleState`" below for how this composes
//!   with, rather than duplicates, `crate::request`'s own safe-state entry
//!   machinery.
//! - [`try_enter_power_mode`] — the full ordinary-transition rule:
//!   [`is_power_mode_transition_defined`] gates the transition shape,
//!   [`is_power_mode_gate_satisfied`] gates its timing. Both must hold for
//!   the move to succeed.
//! - [`shutdown_to_unpowered`] — the involuntary, ungated move to
//!   [`PowerMode::Unpowered`] from any of the three powered modes. See
//!   "Provenance note: `shutdown_to_unpowered` as an unconditional
//!   demotion" below for why no gate applies here, mirroring
//!   [`crate::lifecycle::RcServerState::try_transition`]'s own
//!   unconditional `HW_CONFIGURED` -> `HW_UNCONFIGURED` demotion path.
//! - [`StartupPath`] / [`try_cold_start`] / [`try_hot_start`] — the two
//!   distinct startup paths this checklist bullet names by name:
//!   [`try_cold_start`] (`StartupPath::Cold`) is the plain, no-handshake
//!   `Unpowered` -> `Normal` path; [`try_hot_start`] (`StartupPath::Hot`)
//!   is the `Sleep` -> `Normal` path, additionally gated by the
//!   hot-start-from-Sleep WakeUp handshake below reaching completion.
//!   Both remain further gated by [`is_power_mode_gate_satisfied`], the
//!   same shared precondition [`try_enter_power_mode`] uses.
//! - [`WakeUpHandshakeState`] / [`send_wakeup_request`] /
//!   [`acknowledge_wakeup_request`] / [`is_wakeup_handshake_complete`] —
//!   the hot-start-from-Sleep WakeUp handshake itself, modeled as a real
//!   two-step message exchange (`Idle` -> `RequestSent` ->
//!   `Acknowledged`) rather than a single flag flip, per this checklist
//!   bullet's own explicit wording. See "Provenance note: the WakeUp
//!   handshake's own wire encoding" below for why this stays an abstract
//!   state machine rather than a concrete wire message.
//!
//! Same "additive standalone plumbing only" discipline as every Milestone
//! 1-6 entry in `src/request.rs`/`src/e2e.rs`/`src/watchdog.rs`: every item
//! above is a pure function or a plain data type over caller-supplied
//! state. Nothing here owns a real power domain, spawns a thread, sends or
//! receives a real WakeUp message over any transport, or is called from a
//! decoder, CLI, or dispatch loop.
//!
//! ## Provenance note: mode ordering and `Unpowered`'s software-model
//! semantics
//!
//! `ROADMAP.md`'s checklist bullet names the four modes in the order
//! "Normal / StandBy / Sleep / Unpowered" but does not itself state their
//! relative depth or which pairs are directly reachable from which. Per
//! Guiding Principle 5, this module's working interpretation — flagged
//! here rather than asserted as spec fact — reads that ordering as
//! increasing power-down depth: `Normal` (fully operational) shallower than
//! `StandBy`, `StandBy` shallower than `Sleep`, `Sleep` shallower than
//! `Unpowered`. [`is_power_mode_transition_defined`] only names the two
//! adjacent, powered-mode pairs (`Normal`<->`StandBy`, `StandBy`<->`Sleep`)
//! as ordinary transitions, and reserves the deepest hop, `Sleep` all the
//! way back to `Normal`, for the dedicated hot-start path below — this
//! module's own reading of why the checklist bullet calls out the
//! "hot-start-**from-Sleep**" WakeUp handshake specifically, rather than a
//! handshake generic to any powered mode.
//!
//! `Unpowered` itself cannot be a state a live RC Server process is
//! actually running in — by definition, unpowered hardware runs no
//! software at all. This module still models it as an ordinary
//! [`PowerMode`] variant, for the same reason
//! [`crate::lifecycle::RcServerState`] models `HW_UNCONFIGURED` as a real
//! variant despite being a "nothing is configured yet" state: it is the
//! value an external supervisor (or this same process, immediately before
//! the rail actually drops) records to describe the RC Server's condition
//! from the outside, and the value [`try_cold_start`] consumes as its
//! starting point. No code in this module claims to keep running *while*
//! [`PowerMode::Unpowered`] — see [`shutdown_to_unpowered`]'s own doc
//! comment for the precise boundary this module draws around that fact.
//!
//! ## Provenance note: the idle/no-pending-response gate as caller-supplied
//! facts, composed from `crate::request::RequestLifecycleState`
//!
//! `ROADMAP.md`'s checklist bullet states this item is sequenced into
//! Milestone 6, rather than deferred alongside the rest of the
//! endpoint-type work in Milestone 7, specifically because power-mode
//! entry/exit gating "shares the same 'all endpoints idle, no pending
//! response' conditions as safe-state entry." `crate::request` has no
//! single named function already computing that fact — its own
//! `resolve_safe_state_action` takes "should this stream enter safe state
//! right now" as an already-resolved `bool` from its callers, the same
//! "take the fact, not the machinery that would produce it" shape used
//! throughout this crate (see e.g. [`crate::watchdog`]'s own watchdog-tick
//! provenance note). [`PowerModeGateInput`] follows that same shape rather
//! than inventing a live endpoint-tracking type of its own: two plain
//! caller-supplied `bool`s.
//!
//! What this module *does* compose, rather than re-derive, is the shared
//! type a caller would naturally read those two facts from:
//! [`power_mode_gate_from_request_states`] takes a slice of
//! `crate::request::RequestLifecycleState` — the exact type
//! `crate::request`'s own safe-state entry machinery advances every
//! request through — and reads "no endpoint is busy and no response is
//! outstanding" as "every request has reached
//! `RequestLifecycleState::Finalized`" (an empty slice, meaning no
//! endpoints exist to be busy at all, is vacuously idle). `Pending` denotes
//! a request not yet begun; `Started`/`UnderExecution` denote one actively
//! progressing toward a still-owed response; only `Finalized` denotes one
//! that owes nothing further. This is this module's own working
//! interpretation of "idle" and "no pending response" as the same
//! underlying fact about request progress, flagged per Guiding Principle 5
//! rather than asserted as a distinction the checklist wording itself
//! draws.
//!
//! ## Provenance note: `shutdown_to_unpowered` as an unconditional demotion
//!
//! No `..._enable`-style gate or idle precondition is named anywhere for
//! the move *into* `Unpowered` — and there is a structural reason none
//! could apply: a real power-rail loss is an external hardware event this
//! software model cannot refuse by claiming an endpoint is still busy.
//! [`shutdown_to_unpowered`] is accordingly unconditional, exactly
//! mirroring [`crate::lifecycle::RcServerState::try_transition`]'s own
//! `HW_CONFIGURED` -> `HW_UNCONFIGURED` demotion path and that method's own
//! documented reasoning for why no guard applies to a state move that
//! discards rather than admits new configuration.
//!
//! ## Provenance note: the WakeUp handshake's own wire encoding
//!
//! `ROADMAP.md`'s checklist bullet names a "hot-start-from-Sleep WakeUp
//! message handshake" but the confidential OPEN Alliance TC18 Remote
//! Control Protocol Specification v0.5.1_RC's exact wire encoding for that
//! message (its ACF/AVTPDU framing, field layout, or byte values) is out of
//! reach of this item per this crate's own licensing constraint against
//! reproducing spec text. Per Guiding Principle 5, this module does not
//! guess one: [`WakeUpHandshakeState`] models the handshake's *progress* —
//! a request sent, then acknowledged — as an abstract state machine a
//! future transport-level item can drive from real decoded messages,
//! mirroring [`crate::watchdog`]'s own "opaque tick, no live clock" stance
//! on the parts of its model this crate cannot yet source from a decoder.
//!
//! ## Deliberately out of scope
//!
//! - Wiring [`try_enter_power_mode`], [`try_cold_start`], or
//!   [`try_hot_start`] into any decoder, CLI, or dispatch loop, or giving
//!   any of them an owned, mutable "current power mode" field. Every
//!   function here takes its starting [`PowerMode`] (and, where relevant,
//!   [`WakeUpHandshakeState`]) as a plain argument and returns the new
//!   value on success, the same by-value shape
//!   [`crate::lifecycle::RcServerState::try_transition`] already uses.
//! - A real WakeUp message encoder/decoder, or any composition of
//!   [`WakeUpHandshakeState`] with a live transport. See "Provenance note:
//!   the WakeUp handshake's own wire encoding" above.
//! - Composing [`power_mode_gate_from_request_states`]'s output into
//!   [`crate::request::resolve_safe_state_action`] or vice versa — both
//!   now read the same underlying `RequestLifecycleState` progress, but
//!   nothing in this crate unifies "enter a safe state" and "enter a
//!   power-saving mode" into one caller-facing decision yet.
//! - `ROADMAP.md` Milestone 7's own "Wakeup control" endpoint-type bullet
//!   (`ep_type 0x01`), which this item exists to unblock but does not
//!   itself implement — that bullet's fixed `SleepCMD` byte value and
//!   endpoint-level framing are out of this item's scope.

use crate::request::RequestLifecycleState;
use crate::RcpError;

// ── PowerMode ─────────────────────────────────────────────────────────────────

/// The RC Server's power mode, per the real four-mode model this item
/// replaces the legacy `Active`/`Sleep`/`Standby` model with.
///
/// See this module's doc comment "Provenance note: mode ordering and
/// `Unpowered`'s software-model semantics" for this crate's own working
/// interpretation of the four variants' relative depth, and for what
/// [`PowerMode::Unpowered`] can and cannot mean for a running process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// fusa:req REQ-PWR-001
pub enum PowerMode {
    /// Fully operational — every endpoint may be driven normally.
    Normal,
    /// A shallow power-saving mode, one step down from `Normal`.
    StandBy,
    /// A deep power-saving mode, one step down from `StandBy`. Resuming
    /// from this mode to `Normal` is the hot-start path this module
    /// specifically gates behind the WakeUp handshake — see
    /// [`try_hot_start`].
    Sleep,
    /// No power is applied to the RC Server hardware. See this module's
    /// doc comment for the boundary this crate draws around what this
    /// variant can mean for a live process.
    Unpowered,
}

impl PowerMode {
    /// A stable lowercase name for this mode, for logging/diagnostics.
    /// Never panics for any input.
    pub fn as_str(self) -> &'static str {
        match self {
            PowerMode::Normal => "normal",
            PowerMode::StandBy => "standby",
            PowerMode::Sleep => "sleep",
            PowerMode::Unpowered => "unpowered",
        }
    }
}

/// Whether `(from, to)` is one of the two adjacent, powered-mode pairs this
/// module implements an ordinary transition between: `Normal` <->
/// `StandBy` and `StandBy` <-> `Sleep`.
///
/// Every other pair is `false`, including staying in the same mode, the
/// direct `Normal` <-> `Sleep` hop (reserved for the dedicated hot-start
/// path — see [`try_hot_start`]), and any pair naming
/// [`PowerMode::Unpowered`] (reserved for [`shutdown_to_unpowered`] and
/// [`try_cold_start`]). Never panics for any input.
// fusa:req REQ-PWR-002
pub fn is_power_mode_transition_defined(from: PowerMode, to: PowerMode) -> bool {
    matches!(
        (from, to),
        (PowerMode::Normal, PowerMode::StandBy)
            | (PowerMode::StandBy, PowerMode::Normal)
            | (PowerMode::StandBy, PowerMode::Sleep)
            | (PowerMode::Sleep, PowerMode::StandBy)
    )
}

// ── PowerModeGateInput ───────────────────────────────────────────────────────

/// The shared entry/exit precondition every ordinary power-mode transition
/// and startup path in this module gates on: every reachable endpoint must
/// be idle, and no request may still have a response outstanding.
///
/// See this module's doc comment "Provenance note: the idle/no-pending-
/// response gate as caller-supplied facts" for why this stays two plain
/// `bool` fields rather than a newly invented endpoint-tracking type, and
/// [`power_mode_gate_from_request_states`] for how a caller can derive both
/// from `crate::request::RequestLifecycleState` directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
// fusa:req REQ-PWR-003
pub struct PowerModeGateInput {
    /// No endpoint reachable from this power domain is currently busy.
    pub all_endpoints_idle: bool,
    /// No request still has a response outstanding.
    pub no_pending_response: bool,
}

/// The full gate check: both [`PowerModeGateInput`] fields must be `true`.
/// Never panics for any input.
// fusa:req REQ-PWR-003
pub fn is_power_mode_gate_satisfied(input: PowerModeGateInput) -> bool {
    input.all_endpoints_idle && input.no_pending_response
}

/// Derive a [`PowerModeGateInput`] directly from a slice of
/// `crate::request::RequestLifecycleState`, one entry per outstanding
/// request: both `all_endpoints_idle` and `no_pending_response` are `true`
/// iff every entry has reached [`RequestLifecycleState::Finalized`] (an
/// empty slice is vacuously idle — no requests exist to be busy or
/// pending). See this module's doc comment "Provenance note: the
/// idle/no-pending-response gate as caller-supplied facts" for this
/// module's own working interpretation of "idle" and "no pending response"
/// as the same underlying fact about request progress. Never panics for
/// any input.
// fusa:req REQ-PWRSTART-003
pub fn power_mode_gate_from_request_states(states: &[RequestLifecycleState]) -> PowerModeGateInput {
    let idle = states
        .iter()
        .all(|s| matches!(s, RequestLifecycleState::Finalized));
    PowerModeGateInput {
        all_endpoints_idle: idle,
        no_pending_response: idle,
    }
}

// ── Ordinary transitions / shutdown ──────────────────────────────────────────

/// Attempt the full ordinary power-mode transition rule: `to` must be one
/// of the pairs [`is_power_mode_transition_defined`] names for `from`, and
/// `gate` must satisfy [`is_power_mode_gate_satisfied`].
///
/// Returns `Ok(to)` when both hold. Returns `Err(RcpError::RequestRejected)`
/// when the transition shape itself is undefined, or when it is defined but
/// `gate` is not yet satisfied — mirroring
/// [`crate::request::check_compound_gate`]'s own use of
/// `RequestRejected` for "known but not currently satisfied." Never panics
/// for any input.
// fusa:req REQ-PWR-004
pub fn try_enter_power_mode(
    from: PowerMode,
    to: PowerMode,
    gate: PowerModeGateInput,
) -> Result<PowerMode, RcpError> {
    if !is_power_mode_transition_defined(from, to) {
        return Err(RcpError::RequestRejected);
    }
    if !is_power_mode_gate_satisfied(gate) {
        return Err(RcpError::RequestRejected);
    }
    Ok(to)
}

/// The involuntary, unconditional move to [`PowerMode::Unpowered`] from any
/// of the three powered modes.
///
/// See this module's doc comment "Provenance note: `shutdown_to_unpowered`
/// as an unconditional demotion" for why no [`PowerModeGateInput`] gate
/// applies here. Always returns [`PowerMode::Unpowered`]. Never panics for
/// any input.
// fusa:req REQ-PWR-005
pub fn shutdown_to_unpowered(_from: PowerMode) -> PowerMode {
    PowerMode::Unpowered
}

// ── StartupPath / cold and hot start ─────────────────────────────────────────

/// The two distinct startup paths this checklist bullet names by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupPath {
    /// Powering up from [`PowerMode::Unpowered`] with no prior session to
    /// resume. See [`try_cold_start`].
    Cold,
    /// Resuming from [`PowerMode::Sleep`]. See [`try_hot_start`] for the
    /// additional WakeUp-handshake gate this path alone requires.
    Hot,
}

/// Attempt cold start: [`PowerMode::Unpowered`] -> [`PowerMode::Normal`],
/// with no WakeUp handshake involved.
///
/// Returns `Ok(PowerMode::Normal)` when `from` is
/// [`PowerMode::Unpowered`] and `gate` satisfies
/// [`is_power_mode_gate_satisfied`]. Returns
/// `Err(RcpError::RequestRejected)` when `from` is any other mode, or when
/// it is [`PowerMode::Unpowered`] but `gate` is not yet satisfied. Never
/// panics for any input.
// fusa:req REQ-PWRSTART-001
pub fn try_cold_start(from: PowerMode, gate: PowerModeGateInput) -> Result<PowerMode, RcpError> {
    if from != PowerMode::Unpowered {
        return Err(RcpError::RequestRejected);
    }
    if !is_power_mode_gate_satisfied(gate) {
        return Err(RcpError::RequestRejected);
    }
    Ok(PowerMode::Normal)
}

/// Attempt hot start: [`PowerMode::Sleep`] -> [`PowerMode::Normal`], gated
/// by both a completed WakeUp handshake and the shared idle/no-pending-
/// response precondition.
///
/// Returns `Ok(PowerMode::Normal)` when `from` is [`PowerMode::Sleep`],
/// `wakeup` has reached [`WakeUpHandshakeState::Acknowledged`] (see
/// [`is_wakeup_handshake_complete`]), and `gate` satisfies
/// [`is_power_mode_gate_satisfied`]. Returns
/// `Err(RcpError::RequestRejected)` if any of the three does not hold.
/// Never panics for any input.
// fusa:req REQ-PWRSTART-002
pub fn try_hot_start(
    from: PowerMode,
    wakeup: WakeUpHandshakeState,
    gate: PowerModeGateInput,
) -> Result<PowerMode, RcpError> {
    if from != PowerMode::Sleep {
        return Err(RcpError::RequestRejected);
    }
    if !is_wakeup_handshake_complete(wakeup) {
        return Err(RcpError::RequestRejected);
    }
    if !is_power_mode_gate_satisfied(gate) {
        return Err(RcpError::RequestRejected);
    }
    Ok(PowerMode::Normal)
}

// ── WakeUp handshake ──────────────────────────────────────────────────────────

/// The hot-start-from-Sleep WakeUp handshake's own progress, modeled as a
/// real two-step message exchange rather than a single flag flip: a
/// WakeUp request must be sent and then acknowledged before
/// [`try_hot_start`] will admit `Normal`.
///
/// See this module's doc comment "Provenance note: the WakeUp handshake's
/// own wire encoding" for why this stays an abstract progress marker
/// rather than a concrete wire message type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
// fusa:req REQ-PWR-006
pub enum WakeUpHandshakeState {
    /// No WakeUp exchange in progress.
    #[default]
    Idle,
    /// A WakeUp request has been sent and is awaiting acknowledgment.
    RequestSent,
    /// The WakeUp request has been acknowledged; hot-start may now proceed.
    Acknowledged,
}

/// Advance the handshake by sending a WakeUp request: `Idle` ->
/// `RequestSent`.
///
/// Returns `Err(RcpError::RequestRejected)` for any state other than
/// `Idle` — a request cannot be (re-)sent while one is already outstanding
/// or already acknowledged. Never panics for any input.
// fusa:req REQ-PWR-006
pub fn send_wakeup_request(state: WakeUpHandshakeState) -> Result<WakeUpHandshakeState, RcpError> {
    match state {
        WakeUpHandshakeState::Idle => Ok(WakeUpHandshakeState::RequestSent),
        _ => Err(RcpError::RequestRejected),
    }
}

/// Advance the handshake by acknowledging an outstanding WakeUp request:
/// `RequestSent` -> `Acknowledged`.
///
/// Returns `Err(RcpError::RequestRejected)` for any state other than
/// `RequestSent` — an acknowledgment is only meaningful for a request that
/// was actually sent and not yet acknowledged. Never panics for any input.
// fusa:req REQ-PWR-006
pub fn acknowledge_wakeup_request(
    state: WakeUpHandshakeState,
) -> Result<WakeUpHandshakeState, RcpError> {
    match state {
        WakeUpHandshakeState::RequestSent => Ok(WakeUpHandshakeState::Acknowledged),
        _ => Err(RcpError::RequestRejected),
    }
}

/// Whether the WakeUp handshake has reached completion:
/// [`WakeUpHandshakeState::Acknowledged`], and only that variant. Never
/// panics for any input.
// fusa:req REQ-PWR-007
pub fn is_wakeup_handshake_complete(state: WakeUpHandshakeState) -> bool {
    matches!(state, WakeUpHandshakeState::Acknowledged)
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    const ALL_MODES: [PowerMode; 4] = [
        PowerMode::Normal,
        PowerMode::StandBy,
        PowerMode::Sleep,
        PowerMode::Unpowered,
    ];

    const ALL_WAKEUP_STATES: [WakeUpHandshakeState; 3] = [
        WakeUpHandshakeState::Idle,
        WakeUpHandshakeState::RequestSent,
        WakeUpHandshakeState::Acknowledged,
    ];

    const GATE_COMBOS: [PowerModeGateInput; 4] = [
        PowerModeGateInput {
            all_endpoints_idle: false,
            no_pending_response: false,
        },
        PowerModeGateInput {
            all_endpoints_idle: false,
            no_pending_response: true,
        },
        PowerModeGateInput {
            all_endpoints_idle: true,
            no_pending_response: false,
        },
        PowerModeGateInput {
            all_endpoints_idle: true,
            no_pending_response: true,
        },
    ];

    // ── PowerMode ─────────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-PWR-001
    fn power_mode_variants_are_pairwise_distinct() {
        for (i, a) in ALL_MODES.iter().enumerate() {
            for (j, b) in ALL_MODES.iter().enumerate() {
                assert_eq!(i == j, a == b);
            }
        }
    }

    #[test]
    // fusa:test REQ-PWR-001
    fn as_str_gives_a_distinct_name_per_mode() {
        let names: Vec<&str> = ALL_MODES.iter().map(|m| m.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len());
    }

    // ── is_power_mode_transition_defined ─────────────────────────────────

    #[test]
    // fusa:test REQ-PWR-002
    fn adjacent_powered_pairs_are_defined_both_directions() {
        assert!(is_power_mode_transition_defined(
            PowerMode::Normal,
            PowerMode::StandBy
        ));
        assert!(is_power_mode_transition_defined(
            PowerMode::StandBy,
            PowerMode::Normal
        ));
        assert!(is_power_mode_transition_defined(
            PowerMode::StandBy,
            PowerMode::Sleep
        ));
        assert!(is_power_mode_transition_defined(
            PowerMode::Sleep,
            PowerMode::StandBy
        ));
    }

    #[test]
    // fusa:test REQ-PWR-002
    fn direct_normal_sleep_hop_is_not_an_ordinary_transition() {
        assert!(!is_power_mode_transition_defined(
            PowerMode::Normal,
            PowerMode::Sleep
        ));
        assert!(!is_power_mode_transition_defined(
            PowerMode::Sleep,
            PowerMode::Normal
        ));
    }

    #[test]
    // fusa:test REQ-PWR-002
    fn unpowered_is_never_an_ordinary_transition_member() {
        for &m in &ALL_MODES {
            assert!(!is_power_mode_transition_defined(m, PowerMode::Unpowered));
            assert!(!is_power_mode_transition_defined(PowerMode::Unpowered, m));
        }
    }

    #[test]
    // fusa:test REQ-PWR-002
    fn staying_in_the_same_mode_is_not_defined() {
        for &m in &ALL_MODES {
            assert!(!is_power_mode_transition_defined(m, m));
        }
    }

    // ── PowerModeGateInput / is_power_mode_gate_satisfied ────────────────

    #[test]
    // fusa:test REQ-PWR-003
    fn gate_requires_both_flags_true() {
        assert!(is_power_mode_gate_satisfied(PowerModeGateInput {
            all_endpoints_idle: true,
            no_pending_response: true,
        }));
        assert!(!is_power_mode_gate_satisfied(PowerModeGateInput {
            all_endpoints_idle: true,
            no_pending_response: false,
        }));
        assert!(!is_power_mode_gate_satisfied(PowerModeGateInput {
            all_endpoints_idle: false,
            no_pending_response: true,
        }));
        assert!(!is_power_mode_gate_satisfied(PowerModeGateInput::default()));
    }

    // ── power_mode_gate_from_request_states ──────────────────────────────

    #[test]
    // fusa:test REQ-PWRSTART-003
    fn empty_request_state_slice_is_vacuously_idle() {
        let gate = power_mode_gate_from_request_states(&[]);
        assert!(is_power_mode_gate_satisfied(gate));
    }

    #[test]
    // fusa:test REQ-PWRSTART-003
    fn all_finalized_states_are_idle() {
        let states = [
            RequestLifecycleState::Finalized,
            RequestLifecycleState::Finalized,
        ];
        let gate = power_mode_gate_from_request_states(&states);
        assert!(is_power_mode_gate_satisfied(gate));
    }

    #[test]
    // fusa:test REQ-PWRSTART-003
    fn any_non_finalized_state_is_not_idle() {
        for state in [
            RequestLifecycleState::Pending,
            RequestLifecycleState::Started,
            RequestLifecycleState::UnderExecution,
        ] {
            let states = [RequestLifecycleState::Finalized, state];
            let gate = power_mode_gate_from_request_states(&states);
            assert!(!is_power_mode_gate_satisfied(gate));
            assert!(!gate.all_endpoints_idle);
            assert!(!gate.no_pending_response);
        }
    }

    // ── try_enter_power_mode ──────────────────────────────────────────────

    #[test]
    // fusa:test REQ-PWR-004
    fn succeeds_when_defined_and_gated() {
        let gate = PowerModeGateInput {
            all_endpoints_idle: true,
            no_pending_response: true,
        };
        assert_eq!(
            try_enter_power_mode(PowerMode::Normal, PowerMode::StandBy, gate),
            Ok(PowerMode::StandBy)
        );
        assert_eq!(
            try_enter_power_mode(PowerMode::StandBy, PowerMode::Sleep, gate),
            Ok(PowerMode::Sleep)
        );
    }

    #[test]
    // fusa:test REQ-PWR-004
    fn rejected_when_transition_undefined_even_if_gated() {
        let gate = PowerModeGateInput {
            all_endpoints_idle: true,
            no_pending_response: true,
        };
        assert_eq!(
            try_enter_power_mode(PowerMode::Normal, PowerMode::Sleep, gate),
            Err(RcpError::RequestRejected)
        );
        assert_eq!(
            try_enter_power_mode(PowerMode::Normal, PowerMode::Unpowered, gate),
            Err(RcpError::RequestRejected)
        );
    }

    #[test]
    // fusa:test REQ-PWR-004
    fn rejected_when_defined_but_not_gated() {
        let gate = PowerModeGateInput {
            all_endpoints_idle: true,
            no_pending_response: false,
        };
        assert_eq!(
            try_enter_power_mode(PowerMode::Normal, PowerMode::StandBy, gate),
            Err(RcpError::RequestRejected)
        );
    }

    // ── shutdown_to_unpowered ─────────────────────────────────────────────

    #[test]
    // fusa:test REQ-PWR-005
    fn shutdown_is_unconditional_from_every_powered_mode() {
        assert_eq!(
            shutdown_to_unpowered(PowerMode::Normal),
            PowerMode::Unpowered
        );
        assert_eq!(
            shutdown_to_unpowered(PowerMode::StandBy),
            PowerMode::Unpowered
        );
        assert_eq!(
            shutdown_to_unpowered(PowerMode::Sleep),
            PowerMode::Unpowered
        );
    }

    // ── try_cold_start ────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-PWRSTART-001
    fn cold_start_succeeds_from_unpowered_when_gated() {
        let gate = PowerModeGateInput {
            all_endpoints_idle: true,
            no_pending_response: true,
        };
        assert_eq!(
            try_cold_start(PowerMode::Unpowered, gate),
            Ok(PowerMode::Normal)
        );
    }

    #[test]
    // fusa:test REQ-PWRSTART-001
    fn cold_start_rejected_from_a_powered_mode() {
        let gate = PowerModeGateInput {
            all_endpoints_idle: true,
            no_pending_response: true,
        };
        for &m in &[PowerMode::Normal, PowerMode::StandBy, PowerMode::Sleep] {
            assert_eq!(try_cold_start(m, gate), Err(RcpError::RequestRejected));
        }
    }

    #[test]
    // fusa:test REQ-PWRSTART-001
    fn cold_start_rejected_when_not_gated() {
        let gate = PowerModeGateInput::default();
        assert_eq!(
            try_cold_start(PowerMode::Unpowered, gate),
            Err(RcpError::RequestRejected)
        );
    }

    // ── try_hot_start ─────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-PWRSTART-002
    fn hot_start_succeeds_from_sleep_when_acknowledged_and_gated() {
        let gate = PowerModeGateInput {
            all_endpoints_idle: true,
            no_pending_response: true,
        };
        assert_eq!(
            try_hot_start(PowerMode::Sleep, WakeUpHandshakeState::Acknowledged, gate),
            Ok(PowerMode::Normal)
        );
    }

    #[test]
    // fusa:test REQ-PWRSTART-002
    fn hot_start_rejected_from_a_non_sleep_mode() {
        let gate = PowerModeGateInput {
            all_endpoints_idle: true,
            no_pending_response: true,
        };
        for &m in &[PowerMode::Normal, PowerMode::StandBy, PowerMode::Unpowered] {
            assert_eq!(
                try_hot_start(m, WakeUpHandshakeState::Acknowledged, gate),
                Err(RcpError::RequestRejected)
            );
        }
    }

    #[test]
    // fusa:test REQ-PWRSTART-002
    fn hot_start_rejected_without_a_completed_handshake() {
        let gate = PowerModeGateInput {
            all_endpoints_idle: true,
            no_pending_response: true,
        };
        for state in [
            WakeUpHandshakeState::Idle,
            WakeUpHandshakeState::RequestSent,
        ] {
            assert_eq!(
                try_hot_start(PowerMode::Sleep, state, gate),
                Err(RcpError::RequestRejected)
            );
        }
    }

    #[test]
    // fusa:test REQ-PWRSTART-002
    fn hot_start_rejected_when_not_gated() {
        let gate = PowerModeGateInput::default();
        assert_eq!(
            try_hot_start(PowerMode::Sleep, WakeUpHandshakeState::Acknowledged, gate),
            Err(RcpError::RequestRejected)
        );
    }

    // ── WakeUpHandshakeState progression ──────────────────────────────────

    #[test]
    // fusa:test REQ-PWR-006
    fn default_wakeup_state_is_idle() {
        assert_eq!(WakeUpHandshakeState::default(), WakeUpHandshakeState::Idle);
    }

    #[test]
    // fusa:test REQ-PWR-006
    fn handshake_advances_in_order() {
        let sent = send_wakeup_request(WakeUpHandshakeState::Idle).unwrap();
        assert_eq!(sent, WakeUpHandshakeState::RequestSent);
        let acked = acknowledge_wakeup_request(sent).unwrap();
        assert_eq!(acked, WakeUpHandshakeState::Acknowledged);
    }

    #[test]
    // fusa:test REQ-PWR-006
    fn cannot_send_a_request_out_of_idle() {
        assert_eq!(
            send_wakeup_request(WakeUpHandshakeState::RequestSent),
            Err(RcpError::RequestRejected)
        );
        assert_eq!(
            send_wakeup_request(WakeUpHandshakeState::Acknowledged),
            Err(RcpError::RequestRejected)
        );
    }

    #[test]
    // fusa:test REQ-PWR-006
    fn cannot_acknowledge_out_of_request_sent() {
        assert_eq!(
            acknowledge_wakeup_request(WakeUpHandshakeState::Idle),
            Err(RcpError::RequestRejected)
        );
        assert_eq!(
            acknowledge_wakeup_request(WakeUpHandshakeState::Acknowledged),
            Err(RcpError::RequestRejected)
        );
    }

    // ── is_wakeup_handshake_complete ──────────────────────────────────────

    #[test]
    // fusa:test REQ-PWR-007
    fn only_acknowledged_is_complete() {
        assert!(!is_wakeup_handshake_complete(WakeUpHandshakeState::Idle));
        assert!(!is_wakeup_handshake_complete(
            WakeUpHandshakeState::RequestSent
        ));
        assert!(is_wakeup_handshake_complete(
            WakeUpHandshakeState::Acknowledged
        ));
    }

    // ── Never-panics sweep ──────────────────────────────────────────────

    #[test]
    // fusa:test REQ-PWR-008
    fn never_panics_for_any_sampled_input() {
        for &from in &ALL_MODES {
            for &to in &ALL_MODES {
                for &gate in &GATE_COMBOS {
                    let _ = try_enter_power_mode(from, to, gate);
                }
            }
            let _ = shutdown_to_unpowered(from);
            for &gate in &GATE_COMBOS {
                let _ = try_cold_start(from, gate);
                for &wakeup in &ALL_WAKEUP_STATES {
                    let _ = try_hot_start(from, wakeup, gate);
                }
            }
        }
        for &wakeup in &ALL_WAKEUP_STATES {
            let _ = send_wakeup_request(wakeup);
            let _ = acknowledge_wakeup_request(wakeup);
            let _ = is_wakeup_handshake_complete(wakeup);
        }
        let _ = power_mode_gate_from_request_states(&[]);
        let _ = power_mode_gate_from_request_states(&[
            RequestLifecycleState::Pending,
            RequestLifecycleState::Started,
            RequestLifecycleState::UnderExecution,
            RequestLifecycleState::Finalized,
        ]);
    }
}
