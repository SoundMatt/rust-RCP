//fusa:req REQ-WDG-001
//fusa:req REQ-WDG-002
//fusa:req REQ-WDG-003
//fusa:req REQ-WDG-004
//fusa:req REQ-WDG-005
//fusa:req REQ-WDG-006
//fusa:req REQ-WDG-007
//fusa:req REQ-WDG-008

//! Per-stream watchdog liveness model (`ROADMAP.md` Milestone 6, "Per-stream
//! safety config" bullet).
//!
//! REPLACE of this crate's old ad-hoc watchdog: the previous
//! [`WatchdogMonitor`]/[`WatchdogConfig`] pair (removed by this item, per
//! `ROADMAP.md`'s own Satellite Package Disposition table entry for this
//! file) spawned a background thread that periodically sent a
//! `Command{cmd_type: CommandType::WATCHDOG}` to a `Zone`-keyed
//! `Controller` and counted consecutive miss/response cycles. Nothing in
//! the real OPEN Alliance TC18 Remote Control Protocol Specification
//! v0.5.1_RC works that way: liveness is a **per-stream** property, reset
//! by *every* request the RC Server receives on that stream — there is no
//! separate periodic poll message at all. This module models that design
//! instead:
//!
//! - [`StreamWatchdogTimeout`] — a stream's configured watchdog timeout,
//!   mirroring [`crate::regmap::RequestStreamConfigEntry::
//!   rx_wd_timeout_interval`]'s own width. See "Provenance note: the
//!   timeout's clock-tick unit" below for why this stays an opaque tick
//!   count rather than a [`std::time::Duration`].
//! - [`StreamWatchdogState`] — one stream's liveness record: the tick at
//!   which its watchdog was last reset. [`StreamWatchdogState::new`]
//!   starts a fresh record at construction time; [`StreamWatchdogState::
//!   reset_on_request`] is the "every request resets liveness" rule
//!   itself, called once per request the stream receives — never on a
//!   timer, never independent of real request traffic.
//! - [`is_stream_watchdog_expired`] — the pure elapsed-ticks-vs-timeout
//!   check [`StreamWatchdogState`] and [`StreamWatchdogTimeout`] compose
//!   into.
//! - [`StreamWatchdogOutcome`] / [`evaluate_stream_watchdog`] — the full
//!   per-stream rule: gated by `rx_wd_enable` (a disabled watchdog is
//!   always [`StreamWatchdogOutcome::Alive`], regardless of elapsed
//!   ticks), and, on expiry, split by `rx_wd_safestate_enable` into
//!   [`StreamWatchdogOutcome::ExpiredSafestate`] (drive every endpoint on
//!   this stream to safe state) versus [`StreamWatchdogOutcome::
//!   ExpiredNoSafestate`] (overflow occurred, but no safe-state
//!   consequence configured). [`StreamWatchdogOutcome::watchdog_overflowed`]
//!   collapses either expired variant down to the plain `bool` that
//!   [`crate::request::check_watchdog_overflow_purge`]/
//!   [`crate::request::purge_normal_priority_on_watchdog_overflow`]
//!   already take as a caller-supplied fact — this module is the "next two
//!   still-unchecked Milestone 6 checklist bullets" `src/request.rs`'s own
//!   doc comment named as the eventual real source of that value. See this
//!   module's doc comment "Deliberately out of scope" section below for why
//!   that composition itself is still not wired up here.
//!
//! Same "additive standalone plumbing only" discipline as every Milestone
//! 1-6 entry in `src/request.rs`/`src/e2e.rs`: every item above is a pure
//! function or a plain data type over caller-supplied state. Nothing here
//! spawns a thread, owns a real clock, is called from a decoder or dispatch
//! loop, or is wired into [`crate::request::check_watchdog_overflow_purge`].
//!
//! ## Provenance note: the timeout's clock-tick unit
//!
//! `ROADMAP.md`'s checklist bullet names `rx_wd_timeout_interval` but does
//! not state its unit, and [`crate::regmap::RequestStreamConfigEntry::
//! rx_wd_timeout_interval`]'s own doc comment (settled in Milestone 2,
//! unrelated to this item) says only "in clock ticks" — no tick rate. Per
//! Guiding Principle 5, this module does not guess a rate: [`StreamWatchdogState`]
//! and [`StreamWatchdogTimeout`] both work in an opaque `u64`/`u16`
//! "tick" unit a caller supplies (e.g. from whatever real clock source a
//! future dispatch loop reads), the same "take the fact, not the machinery
//! that would produce it" shape [`crate::request`]'s own watchdog-overflow
//! provenance note already uses for `watchdog_overflowed` itself.
//!
//! ## Deliberately out of scope
//!
//! - Composing [`StreamWatchdogOutcome::watchdog_overflowed`] into
//!   [`crate::request::check_watchdog_overflow_purge`]/
//!   [`crate::request::purge_normal_priority_on_watchdog_overflow`], or
//!   [`StreamWatchdogOutcome::drives_safestate`] into
//!   [`crate::request::resolve_safe_state_action`] — both now exist as
//!   pure functions a future dispatch-loop item can call together, but
//!   nothing in this crate does so yet.
//! - A live clock source, a background thread, or any owned map from a
//!   real `rx_stream_id` to its [`StreamWatchdogState`]. A caller
//!   maintaining one stream's [`StreamWatchdogState`] per configured
//!   [`crate::regmap::RequestStreamConfigEntry`] row is left to that
//!   future dispatch-loop item, mirroring [`crate::request::SequencerBank`]'s
//!   own "the bank exists, nothing owns one yet" precedent.

// ── StreamWatchdogTimeout / StreamWatchdogState ──────────────────────────────

/// A stream's configured watchdog timeout, in clock ticks — mirrors
/// [`crate::regmap::RequestStreamConfigEntry::rx_wd_timeout_interval`]'s
/// own width. See this module's doc comment "Provenance note: the
/// timeout's clock-tick unit" for why the unit itself is left opaque.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-WDG-001
pub struct StreamWatchdogTimeout(pub u16);

/// One stream's watchdog liveness record: the tick at which it was last
/// reset by an incoming request.
///
/// Never panics for any input. See this module's doc comment for the
/// "reset on every request, no periodic poll" design this type backs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-WDG-002
pub struct StreamWatchdogState {
    last_reset_tick: u64,
}

impl StreamWatchdogState {
    /// Start a fresh liveness record, as if just reset at `now_tick`.
    //fusa:req REQ-WDG-002
    pub fn new(now_tick: u64) -> Self {
        StreamWatchdogState {
            last_reset_tick: now_tick,
        }
    }

    /// The "every request resets liveness" rule itself: record `now_tick`
    /// as this stream's most recent liveness reset, replacing whatever was
    /// recorded before. Intended to be called once per request the stream
    /// receives, never on a periodic timer independent of real traffic.
    //fusa:req REQ-WDG-002
    pub fn reset_on_request(self, now_tick: u64) -> Self {
        StreamWatchdogState {
            last_reset_tick: now_tick,
        }
    }

    /// The tick this record was last reset at.
    //fusa:req REQ-WDG-002
    pub fn last_reset_tick(&self) -> u64 {
        self.last_reset_tick
    }
}

/// Whether `state`'s watchdog has expired: `now_tick` is at least
/// `timeout.0` ticks past `state`'s last reset.
///
/// Uses saturating arithmetic, so a `now_tick` that precedes
/// `state.last_reset_tick()` (a caller-supplied clock going backwards) is
/// read as "no time has elapsed" rather than panicking or wrapping. Never
/// panics for any input.
//fusa:req REQ-WDG-003
pub fn is_stream_watchdog_expired(
    state: StreamWatchdogState,
    now_tick: u64,
    timeout: StreamWatchdogTimeout,
) -> bool {
    now_tick.saturating_sub(state.last_reset_tick) >= timeout.0 as u64
}

// ── StreamWatchdogOutcome / evaluate_stream_watchdog ─────────────────────────

/// The result of evaluating one stream's watchdog: alive, expired with no
/// configured safe-state consequence, or expired with one.
///
/// See [`evaluate_stream_watchdog`] for the full `rx_wd_enable`/
/// `rx_wd_safestate_enable` gating rule this type's three variants encode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
//fusa:req REQ-WDG-004
//fusa:req REQ-WDG-005
pub enum StreamWatchdogOutcome {
    /// The watchdog is disabled, or is enabled but has not yet expired.
    Alive,
    /// The watchdog expired, and `rx_wd_safestate_enable` is `false` — an
    /// overflow occurred, but this stream has no configured safe-state
    /// reaction to it.
    ExpiredNoSafestate,
    /// The watchdog expired, and `rx_wd_safestate_enable` is `true` — every
    /// endpoint on this stream should be driven to its configured safe
    /// state.
    ExpiredSafestate,
}

impl StreamWatchdogOutcome {
    /// True for either expired variant — the plain `bool` fact
    /// [`crate::request::check_watchdog_overflow_purge`]/
    /// [`crate::request::purge_normal_priority_on_watchdog_overflow`]
    /// already take as `watchdog_overflowed`. Never panics for any input.
    //fusa:req REQ-WDG-006
    pub fn watchdog_overflowed(&self) -> bool {
        !matches!(self, Self::Alive)
    }

    /// True only for [`Self::ExpiredSafestate`] — whether this outcome
    /// should drive the stream's endpoints to their configured safe state.
    /// Never panics for any input.
    //fusa:req REQ-WDG-007
    pub fn drives_safestate(&self) -> bool {
        matches!(self, Self::ExpiredSafestate)
    }
}

/// The full per-stream watchdog rule: [`is_stream_watchdog_expired`],
/// gated by `rx_wd_enable`, with the safe-state consequence selected by
/// `rx_wd_safestate_enable`.
///
/// Returns [`StreamWatchdogOutcome::Alive`] when `rx_wd_enable` is `false`
/// (an unconditional exemption — an expired-but-disabled watchdog never
/// overflows) or when the watchdog has not expired.
/// Returns [`StreamWatchdogOutcome::ExpiredSafestate`] or
/// [`StreamWatchdogOutcome::ExpiredNoSafestate`] on expiry, selected by
/// `rx_wd_safestate_enable`. Never panics for any input.
//fusa:req REQ-WDG-004
//fusa:req REQ-WDG-005
pub fn evaluate_stream_watchdog(
    state: StreamWatchdogState,
    now_tick: u64,
    timeout: StreamWatchdogTimeout,
    rx_wd_enable: bool,
    rx_wd_safestate_enable: bool,
) -> StreamWatchdogOutcome {
    if !rx_wd_enable || !is_stream_watchdog_expired(state, now_tick, timeout) {
        return StreamWatchdogOutcome::Alive;
    }
    if rx_wd_safestate_enable {
        StreamWatchdogOutcome::ExpiredSafestate
    } else {
        StreamWatchdogOutcome::ExpiredNoSafestate
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    // ── StreamWatchdogState ──────────────────────────────────────────────────

    #[test]
    //fusa:test REQ-WDG-002
    fn new_records_the_construction_tick() {
        let state = StreamWatchdogState::new(42);
        assert_eq!(state.last_reset_tick(), 42);
    }

    #[test]
    //fusa:test REQ-WDG-002
    fn reset_on_request_overwrites_the_prior_tick() {
        let state = StreamWatchdogState::new(10).reset_on_request(99);
        assert_eq!(state.last_reset_tick(), 99);
    }

    // ── is_stream_watchdog_expired ───────────────────────────────────────────

    #[test]
    //fusa:test REQ-WDG-001
    //fusa:test REQ-WDG-003
    fn not_expired_before_the_timeout_elapses() {
        let state = StreamWatchdogState::new(100);
        assert!(!is_stream_watchdog_expired(
            state,
            150,
            StreamWatchdogTimeout(100)
        ));
    }

    #[test]
    //fusa:test REQ-WDG-003
    fn expired_exactly_at_the_timeout_boundary() {
        let state = StreamWatchdogState::new(100);
        assert!(is_stream_watchdog_expired(
            state,
            200,
            StreamWatchdogTimeout(100)
        ));
    }

    #[test]
    //fusa:test REQ-WDG-003
    fn expired_well_past_the_timeout() {
        let state = StreamWatchdogState::new(0);
        assert!(is_stream_watchdog_expired(
            state,
            u64::MAX,
            StreamWatchdogTimeout(1)
        ));
    }

    #[test]
    //fusa:test REQ-WDG-003
    fn reset_pushes_expiry_back_out() {
        let state = StreamWatchdogState::new(100).reset_on_request(190);
        assert!(!is_stream_watchdog_expired(
            state,
            200,
            StreamWatchdogTimeout(100)
        ));
    }

    #[test]
    //fusa:test REQ-WDG-003
    fn never_panics_when_now_tick_precedes_last_reset() {
        let state = StreamWatchdogState::new(1_000);
        assert!(!is_stream_watchdog_expired(
            state,
            0,
            StreamWatchdogTimeout(u16::MAX)
        ));
    }

    // ── evaluate_stream_watchdog / StreamWatchdogOutcome ─────────────────────

    #[test]
    //fusa:test REQ-WDG-004
    fn disabled_watchdog_is_always_alive_even_when_long_expired() {
        let state = StreamWatchdogState::new(0);
        let outcome =
            evaluate_stream_watchdog(state, u64::MAX, StreamWatchdogTimeout(1), false, true);
        assert_eq!(outcome, StreamWatchdogOutcome::Alive);
        assert!(!outcome.watchdog_overflowed());
        assert!(!outcome.drives_safestate());
    }

    #[test]
    //fusa:test REQ-WDG-004
    fn enabled_watchdog_is_alive_before_expiry() {
        let state = StreamWatchdogState::new(0);
        let outcome = evaluate_stream_watchdog(state, 5, StreamWatchdogTimeout(100), true, true);
        assert_eq!(outcome, StreamWatchdogOutcome::Alive);
    }

    #[test]
    //fusa:test REQ-WDG-005
    fn expired_with_safestate_enabled_drives_safestate() {
        let state = StreamWatchdogState::new(0);
        let outcome = evaluate_stream_watchdog(state, 100, StreamWatchdogTimeout(100), true, true);
        assert_eq!(outcome, StreamWatchdogOutcome::ExpiredSafestate);
        assert!(outcome.watchdog_overflowed());
        assert!(outcome.drives_safestate());
    }

    #[test]
    //fusa:test REQ-WDG-005
    fn expired_without_safestate_enabled_overflows_but_does_not_drive_safestate() {
        let state = StreamWatchdogState::new(0);
        let outcome = evaluate_stream_watchdog(state, 100, StreamWatchdogTimeout(100), true, false);
        assert_eq!(outcome, StreamWatchdogOutcome::ExpiredNoSafestate);
        assert!(outcome.watchdog_overflowed());
        assert!(!outcome.drives_safestate());
    }

    #[test]
    //fusa:test REQ-WDG-006
    //fusa:test REQ-WDG-007
    fn watchdog_overflowed_and_drives_safestate_agree_with_variant_identity() {
        assert!(!StreamWatchdogOutcome::Alive.watchdog_overflowed());
        assert!(!StreamWatchdogOutcome::Alive.drives_safestate());
        assert!(StreamWatchdogOutcome::ExpiredNoSafestate.watchdog_overflowed());
        assert!(!StreamWatchdogOutcome::ExpiredNoSafestate.drives_safestate());
        assert!(StreamWatchdogOutcome::ExpiredSafestate.watchdog_overflowed());
        assert!(StreamWatchdogOutcome::ExpiredSafestate.drives_safestate());
    }

    #[test]
    //fusa:test REQ-WDG-008
    fn evaluate_stream_watchdog_never_panics_for_any_sampled_input() {
        let ticks = [0u64, 1, 100, u64::MAX];
        let timeouts = [0u16, 1, 100, u16::MAX];
        for &reset_tick in &ticks {
            for &now_tick in &ticks {
                for &timeout in &timeouts {
                    for rx_wd_enable in [false, true] {
                        for rx_wd_safestate_enable in [false, true] {
                            let state = StreamWatchdogState::new(reset_tick);
                            let _ = evaluate_stream_watchdog(
                                state,
                                now_tick,
                                StreamWatchdogTimeout(timeout),
                                rx_wd_enable,
                                rx_wd_safestate_enable,
                            );
                        }
                    }
                }
            }
        }
    }
}
