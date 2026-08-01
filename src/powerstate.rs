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
//!   [`PowerMode::Unpowered`], each as TC18 v0.5.1_RC §12.4 defines it. See
//!   "Mode reachability, per TC18 §12.4 Figure 17" below for which modes
//!   are reachable from which, and "Provenance note: `Unpowered`'s
//!   software-model semantics" for what `Unpowered` can and cannot mean for
//!   a running software model.
//! - [`is_power_mode_transition_defined`] — the coarse, state-shape check
//!   (mirroring [`crate::lifecycle::is_transition_defined`]'s own role)
//!   naming exactly which powered-mode pairs this module implements an
//!   *ordinary* (non-start-up) transition between: the two "Go to" edges of
//!   the specification's own state diagram, `Normal` -> `StandBy` and
//!   `Normal` -> `Sleep`. The two reverse edges are start-up paths with
//!   their own extra gating and their own functions ([`try_hot_start`] for
//!   `StandBy` -> `Normal`, [`try_cold_start`] for `Sleep` -> `Normal`), so
//!   they are deliberately not members of this ordinary set. `Unpowered` is
//!   likewise excluded — see [`shutdown_to_unpowered`] and the
//!   cold/hot-start functions below.
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
//!   distinct start-up types the specification defines by name (TC18
//!   v0.5.1_RC §12.4.1 "Power-On / Wake-Up / Start-Up behavior", p.46:
//!   "There are two types of start-up: a cold start (after power-on or
//!   wake-up from sleep) and a hot start (=wake-up from StandBy)").
//!   [`try_cold_start`] (`StartupPath::Cold`) is the plain, no-handshake
//!   path admitting **both** documented cold-start origins, `Unpowered` ->
//!   `Normal` and `Sleep` -> `Normal`; [`try_hot_start`]
//!   (`StartupPath::Hot`) is the `StandBy` -> `Normal` path, additionally
//!   gated by the hot-start WakeUp handshake below reaching completion.
//!   Both remain further gated by [`is_power_mode_gate_satisfied`], the
//!   same shared precondition [`try_enter_power_mode`] uses.
//! - [`WakeUpHandshakeState`] / [`send_wakeup_request`] /
//!   [`acknowledge_wakeup_request`] / [`is_wakeup_handshake_complete`] —
//!   the hot-start WakeUp handshake itself, modeled as a real two-step
//!   message exchange (`Idle` -> `RequestSent` -> `Acknowledged`) rather
//!   than a single flag flip. §12.4.1's "Hot-start-up procedure" is what
//!   attaches this handshake to the hot-start path specifically: the RC
//!   Server "will send ... a repetitive response ... with a WakeUp message
//!   and the WakeUp source. The message will be repeated until a valid
//!   AVTPDU from the sleep request Client is received." See "Provenance
//!   note: the WakeUp handshake's own wire encoding" below for why this
//!   stays an abstract state machine rather than a concrete wire message.
//!
//! Same "additive standalone plumbing only" discipline as every Milestone
//! 1-6 entry in `src/request.rs`/`src/e2e.rs`/`src/watchdog.rs`: every item
//! above is a pure function or a plain data type over caller-supplied
//! state. Nothing here owns a real power domain, spawns a thread, sends or
//! receives a real WakeUp message over any transport, or is called from a
//! decoder, CLI, or dispatch loop.
//!
//! ## Mode reachability, per TC18 §12.4 Figure 17
//!
//! The set of transitions below is **not** an inference from the four mode
//! names' relative "depth" — earlier revisions of this module read it that
//! way, and got the cold/hot-start mapping exactly backwards as a result.
//! TC18 v0.5.1_RC §12.4 "Power- and operation modes", Figure 17 "power and
//! operation modes" (p.46) is a labelled state diagram that names every
//! edge explicitly. It has exactly five, and this module implements exactly
//! those five:
//!
//! | Edge | Figure 17 label | Implemented by |
//! |------|-----------------|----------------|
//! | `Unpowered` -> `Normal` | "Cold start" | [`try_cold_start`] |
//! | `Sleep` -> `Normal`     | "Cold start" | [`try_cold_start`] |
//! | `StandBy` -> `Normal`   | "Hot start"  | [`try_hot_start`] |
//! | `Normal` -> `StandBy`   | "Go to StandBy" | [`try_enter_power_mode`] |
//! | `Normal` -> `Sleep`     | "Go to Sleep"   | [`try_enter_power_mode`] |
//!
//! Two consequences worth stating, because both contradict what this file
//! previously asserted:
//!
//! - **There is no `StandBy` <-> `Sleep` edge at all.** Figure 17 places
//!   `Normal` and `StandBy` inside a "Powered" box and `Sleep` inside a
//!   separate "Only part of PHY powered" box, with no arrow of any kind
//!   between `StandBy` and `Sleep`. Both low-power modes are entered from,
//!   and returned to, `Normal` only. `Sleep` is not "one step down from
//!   `StandBy`"; the two are siblings.
//! - **`Sleep` -> `Normal` is a cold start, not a hot start**, and it is
//!   therefore *not* behind the WakeUp handshake. §12.4.1 states the
//!   mapping in one sentence — "a cold start (after power-on or wake-up
//!   from sleep) and a hot start (=wake-up from StandBy)" — and Figure 17's
//!   arrow labels agree: the `Sleep` -> `Normal` arrow reads "Cold start"
//!   and the `StandBy` -> `Normal` arrow reads "Hot start". This is
//!   consistent with each mode's own §12.4 definition: `StandBy` "maintains
//!   ... configuration data ... alive", so resuming from it needs no
//!   reconfiguration (a hot start, §12.4.1: "In hot-start the configuration
//!   does not need to be redone"), whereas `Sleep` is only defined as "the
//!   mode with the lowest possible power c[on]sumptio[n] still being able
//!   to be woken by a dedicated WakePin or the network interface" and
//!   carries no such retention guarantee.
//!
//! ## Provenance note: `Unpowered`'s software-model semantics
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
//! TC18 §12.4.1's "Hot-start-up procedure" describes this handshake's
//! *behavior* — the RC Server sends a repetitive WakeUp response carrying
//! the WakeUp source over the responder stream configured for the original
//! standby request, repeating "until a valid AVTPDU from the sleep request
//! Client is received" — but the specification gives no field diagram for
//! the WakeUp message itself, so its exact wire encoding (ACF/AVTPDU
//! framing, field layout, byte values) is not recoverable from that text.
//! §12.4.1 also states two distinct wake-up sources ("an internal EP signal
//! of the RC Server or the dedicated wakepin", versus "a TC14/TC10 wake-up
//! request on the network"), which differ only in whether the network
//! interface must be enabled first — a distinction with no bearing on
//! handshake *progress*. Per Guiding Principle 5, this module does not
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
/// See this module's doc comment "Mode reachability, per TC18 §12.4
/// Figure 17" for which variants are reachable from which, and
/// "Provenance note: `Unpowered`'s software-model semantics" for what
/// [`PowerMode::Unpowered`] can and cannot mean for a running process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// fusa:req REQ-PWR-001
pub enum PowerMode {
    /// Fully operational — every endpoint may be driven normally. TC18
    /// §12.4's "Powered" mode, in the lifecycle state the RC Server is
    /// configured for. Figure 17 makes this the hub: it is the only mode
    /// either low-power mode is entered from or returned to.
    Normal,
    /// TC18 §12.4: "the mode in which the system maintains the lowest
    /// possible power while keeping configuration data and functional wake
    /// up sources alive." Because configuration survives, resuming from
    /// here is the **hot start** — see [`try_hot_start`], which gates it
    /// behind the WakeUp handshake.
    StandBy,
    /// TC18 §12.4: "the mode with the lowest possible power c[on]sumptio[n]
    /// still being able to be woken by a dedicated WakePin or the network
    /// interface." Figure 17 places this outside the "Powered" box, in
    /// "Only part of PHY powered". No configuration-retention guarantee is
    /// stated, and §12.4.1 correspondingly classes wake-up from here as a
    /// **cold start** — see [`try_cold_start`]. It is *not* one step below
    /// [`PowerMode::StandBy`]: no edge joins the two.
    Sleep,
    /// TC18 §12.4: "the mode in which no sufficient power supply is
    /// available." See this module's doc comment for the boundary this
    /// crate draws around what this variant can mean for a live process.
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

/// Whether `(from, to)` is one of the two *ordinary* (non-start-up)
/// transitions TC18 §12.4 Figure 17 defines: its "Go to StandBy" edge
/// (`Normal` -> `StandBy`) and its "Go to Sleep" edge
/// (`Normal` -> `Sleep`).
///
/// Every other pair is `false`. In particular:
///
/// - **`StandBy` <-> `Sleep` is `false` in both directions.** Figure 17 has
///   no edge joining the two low-power modes; each is reached only from,
///   and returns only to, `Normal`. (Earlier releases of this crate wrongly
///   accepted this pair.)
/// - The two reverse, wake-up directions — `StandBy` -> `Normal` and
///   `Sleep` -> `Normal` — are `false` here because they are start-ups
///   carrying extra preconditions, handled by [`try_hot_start`] and
///   [`try_cold_start`] respectively rather than by
///   [`try_enter_power_mode`].
/// - Any pair naming [`PowerMode::Unpowered`] is `false` (reserved for
///   [`shutdown_to_unpowered`] and [`try_cold_start`]), as is staying in
///   the same mode.
///
/// Never panics for any input.
// fusa:req REQ-PWR-002
pub fn is_power_mode_transition_defined(from: PowerMode, to: PowerMode) -> bool {
    matches!(
        (from, to),
        (PowerMode::Normal, PowerMode::StandBy) | (PowerMode::Normal, PowerMode::Sleep)
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

/// The two start-up types TC18 §12.4.1 names: "There are two types of
/// start-up: a cold start (after power-on or wake-up from sleep) and a hot
/// start (=wake-up from StandBy)."
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupPath {
    /// Powering up from [`PowerMode::Unpowered`], or waking from
    /// [`PowerMode::Sleep`] — §12.4.1's "after power-on **or wake-up from
    /// sleep**". Neither origin guarantees retained configuration, so after
    /// either the RC Server comes up "in its configured lifecycle state",
    /// possibly needing further configuration. See [`try_cold_start`].
    Cold,
    /// Waking from [`PowerMode::StandBy`] — §12.4.1's "=wake-up from
    /// StandBy", where "the configuration does not need to be redone, as it
    /// shall be maintained during low-power mode". See [`try_hot_start`]
    /// for the additional WakeUp-handshake gate this path alone requires.
    Hot,
}

/// Attempt cold start, with no WakeUp handshake involved.
///
/// TC18 §12.4.1 gives the cold start **two** origins — "a cold start (after
/// power-on or wake-up from sleep)" — and Figure 17 draws both as arrows
/// labelled "Cold start". Both are accepted here:
///
/// - [`PowerMode::Unpowered`] -> [`PowerMode::Normal`] (power-on)
/// - [`PowerMode::Sleep`] -> [`PowerMode::Normal`] (wake-up from sleep)
///
/// Returns `Ok(PowerMode::Normal)` when `from` is either of those and
/// `gate` satisfies [`is_power_mode_gate_satisfied`]. Returns
/// `Err(RcpError::RequestRejected)` when `from` is [`PowerMode::Normal`]
/// (already started) or [`PowerMode::StandBy`] (whose resume is the hot
/// start — see [`try_hot_start`]), or when the origin is valid but `gate`
/// is not yet satisfied. Never panics for any input.
// fusa:req REQ-PWRSTART-001
pub fn try_cold_start(from: PowerMode, gate: PowerModeGateInput) -> Result<PowerMode, RcpError> {
    if !matches!(from, PowerMode::Unpowered | PowerMode::Sleep) {
        return Err(RcpError::RequestRejected);
    }
    if !is_power_mode_gate_satisfied(gate) {
        return Err(RcpError::RequestRejected);
    }
    Ok(PowerMode::Normal)
}

/// Attempt hot start: [`PowerMode::StandBy`] -> [`PowerMode::Normal`],
/// gated by both a completed WakeUp handshake and the shared idle/no-
/// pending-response precondition.
///
/// TC18 §12.4.1 defines the hot start as exactly one origin — "a hot start
/// (=wake-up from StandBy)", drawn in Figure 17 as the single arrow
/// labelled "Hot start" — and it is that section's "Hot-start-up procedure"
/// that specifies the repeated WakeUp message awaiting "a valid AVTPDU from
/// the sleep request Client", which [`WakeUpHandshakeState`] models. The
/// handshake therefore gates *this* path, not the `Sleep` -> `Normal` cold
/// start. (Earlier releases of this crate had the two origins swapped.)
///
/// Returns `Ok(PowerMode::Normal)` when `from` is [`PowerMode::StandBy`],
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
    if from != PowerMode::StandBy {
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

/// The hot-start-from-StandBy WakeUp handshake's own progress, modeled as a
/// real two-step message exchange rather than a single flag flip: a
/// WakeUp request must be sent and then acknowledged before
/// [`try_hot_start`] will admit `Normal`. Per TC18 §12.4.1's "Hot-start-up
/// procedure", `Acknowledged` corresponds to that section's terminating
/// condition, "a valid AVTPDU from the sleep request Client is received".
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

    /// TC18 §12.4 Figure 17's two "Go to ..." edges, the only ordinary
    /// (non-start-up) transitions the diagram draws. Both leave `Normal`.
    #[test]
    // fusa:test REQ-PWR-002
    fn figure_17_go_to_edges_are_the_ordinary_transitions() {
        // "Go to StandBy"
        assert!(is_power_mode_transition_defined(
            PowerMode::Normal,
            PowerMode::StandBy
        ));
        // "Go to Sleep"
        assert!(is_power_mode_transition_defined(
            PowerMode::Normal,
            PowerMode::Sleep
        ));
    }

    /// Figure 17 draws no arrow of any kind between `StandBy` and `Sleep`:
    /// they sit in different boxes ("Powered" vs "Only part of PHY
    /// powered") and are reached only via `Normal`. Releases before
    /// v5.0.0 wrongly accepted this pair in both directions.
    #[test]
    // fusa:test REQ-PWR-002
    fn standby_sleep_pair_is_not_a_transition_in_either_direction() {
        assert!(!is_power_mode_transition_defined(
            PowerMode::StandBy,
            PowerMode::Sleep
        ));
        assert!(!is_power_mode_transition_defined(
            PowerMode::Sleep,
            PowerMode::StandBy
        ));
    }

    /// Figure 17's two wake-up edges back to `Normal` are start-ups with
    /// their own extra preconditions, so they are not members of the
    /// ordinary set — [`try_hot_start`] and [`try_cold_start`] own them.
    #[test]
    // fusa:test REQ-PWR-002
    fn wakeup_edges_back_to_normal_are_not_ordinary_transitions() {
        assert!(!is_power_mode_transition_defined(
            PowerMode::StandBy,
            PowerMode::Normal
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

    /// Exhaustive cross-product: exactly the two Figure 17 "Go to" edges,
    /// and nothing else, out of all 16 ordered pairs.
    #[test]
    // fusa:test REQ-PWR-002
    fn exactly_two_ordered_pairs_are_defined() {
        let defined: Vec<(PowerMode, PowerMode)> = ALL_MODES
            .iter()
            .flat_map(|&a| ALL_MODES.iter().map(move |&b| (a, b)))
            .filter(|&(a, b)| is_power_mode_transition_defined(a, b))
            .collect();
        assert_eq!(
            defined,
            vec![
                (PowerMode::Normal, PowerMode::StandBy),
                (PowerMode::Normal, PowerMode::Sleep),
            ]
        );
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
            try_enter_power_mode(PowerMode::Normal, PowerMode::Sleep, gate),
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
        // No Figure 17 edge joins the two low-power modes.
        assert_eq!(
            try_enter_power_mode(PowerMode::StandBy, PowerMode::Sleep, gate),
            Err(RcpError::RequestRejected)
        );
        // Wake-up edges belong to the start-up functions, not here.
        assert_eq!(
            try_enter_power_mode(PowerMode::StandBy, PowerMode::Normal, gate),
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

    /// TC18 §12.4.1: "a cold start (after power-on **or wake-up from
    /// sleep**)". Both origins, and Figure 17 labels both arrows
    /// "Cold start". Releases before v5.0.0 accepted only `Unpowered`.
    #[test]
    // fusa:test REQ-PWRSTART-001
    fn cold_start_succeeds_from_both_documented_origins_when_gated() {
        let gate = PowerModeGateInput {
            all_endpoints_idle: true,
            no_pending_response: true,
        };
        // "after power-on"
        assert_eq!(
            try_cold_start(PowerMode::Unpowered, gate),
            Ok(PowerMode::Normal)
        );
        // "or wake-up from sleep"
        assert_eq!(
            try_cold_start(PowerMode::Sleep, gate),
            Ok(PowerMode::Normal)
        );
    }

    /// `Sleep` -> `Normal` is a cold start, so it takes no WakeUp
    /// handshake: §12.4.1 attaches the handshake to the "Hot-start-up
    /// procedure" only. Releases before v5.0.0 gated this path behind the
    /// handshake, blocking every wake-from-sleep that had not run one.
    #[test]
    // fusa:test REQ-PWRSTART-001
    fn cold_start_from_sleep_needs_no_wakeup_handshake() {
        let gate = PowerModeGateInput {
            all_endpoints_idle: true,
            no_pending_response: true,
        };
        // The handshake state is not even an argument to try_cold_start;
        // an origin in Sleep with the handshake still Idle must succeed.
        assert!(!is_wakeup_handshake_complete(WakeUpHandshakeState::Idle));
        assert_eq!(
            try_cold_start(PowerMode::Sleep, gate),
            Ok(PowerMode::Normal)
        );
    }

    /// `StandBy` is the hot-start origin, not a cold-start one, and
    /// `Normal` is already started.
    #[test]
    // fusa:test REQ-PWRSTART-001
    fn cold_start_rejected_from_normal_and_standby() {
        let gate = PowerModeGateInput {
            all_endpoints_idle: true,
            no_pending_response: true,
        };
        for &m in &[PowerMode::Normal, PowerMode::StandBy] {
            assert_eq!(try_cold_start(m, gate), Err(RcpError::RequestRejected));
        }
    }

    #[test]
    // fusa:test REQ-PWRSTART-001
    fn cold_start_rejected_when_not_gated() {
        let gate = PowerModeGateInput::default();
        for &m in &[PowerMode::Unpowered, PowerMode::Sleep] {
            assert_eq!(try_cold_start(m, gate), Err(RcpError::RequestRejected));
        }
    }

    // ── try_hot_start ─────────────────────────────────────────────────────

    /// TC18 §12.4.1: "a hot start (=wake-up from StandBy)", Figure 17's
    /// single "Hot start" arrow. Releases before v5.0.0 had this origin
    /// as `Sleep`.
    #[test]
    // fusa:test REQ-PWRSTART-002
    fn hot_start_succeeds_from_standby_when_acknowledged_and_gated() {
        let gate = PowerModeGateInput {
            all_endpoints_idle: true,
            no_pending_response: true,
        };
        assert_eq!(
            try_hot_start(PowerMode::StandBy, WakeUpHandshakeState::Acknowledged, gate),
            Ok(PowerMode::Normal)
        );
    }

    #[test]
    // fusa:test REQ-PWRSTART-002
    fn hot_start_rejected_from_every_non_standby_mode() {
        let gate = PowerModeGateInput {
            all_endpoints_idle: true,
            no_pending_response: true,
        };
        // Sleep in particular: that origin is a cold start (§12.4.1), so
        // routing it through the hot-start path must be rejected even with
        // a fully acknowledged handshake.
        for &m in &[PowerMode::Normal, PowerMode::Sleep, PowerMode::Unpowered] {
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
                try_hot_start(PowerMode::StandBy, state, gate),
                Err(RcpError::RequestRejected)
            );
        }
    }

    #[test]
    // fusa:test REQ-PWRSTART-002
    fn hot_start_rejected_when_not_gated() {
        let gate = PowerModeGateInput::default();
        assert_eq!(
            try_hot_start(PowerMode::StandBy, WakeUpHandshakeState::Acknowledged, gate),
            Err(RcpError::RequestRejected)
        );
    }

    /// Cross-cutting: the cold- and hot-start origin sets are disjoint and
    /// together cover exactly the three non-`Normal` modes, matching
    /// Figure 17's three inbound arrows to `Normal`.
    #[test]
    // fusa:test REQ-PWRSTART-002
    fn cold_and_hot_start_origins_partition_the_three_inbound_edges() {
        let gate = PowerModeGateInput {
            all_endpoints_idle: true,
            no_pending_response: true,
        };
        let cold: Vec<PowerMode> = ALL_MODES
            .iter()
            .copied()
            .filter(|&m| try_cold_start(m, gate).is_ok())
            .collect();
        let hot: Vec<PowerMode> = ALL_MODES
            .iter()
            .copied()
            .filter(|&m| try_hot_start(m, WakeUpHandshakeState::Acknowledged, gate).is_ok())
            .collect();
        assert_eq!(cold, vec![PowerMode::Sleep, PowerMode::Unpowered]);
        assert_eq!(hot, vec![PowerMode::StandBy]);
        assert!(cold.iter().all(|m| !hot.contains(m)));
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
