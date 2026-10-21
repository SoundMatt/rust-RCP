//fusa:req REQ-FORMAL-001
//fusa:req REQ-FORMAL-002
//fusa:req REQ-FORMAL-003
//fusa:req REQ-FORMAL-004

//! Formal model helpers — state-machine invariant checking and property witnesses.
//!
//! These are lightweight runtime-checkable invariants that mirror the formal
//! model maintained in offline verification tools (e.g., TLA+). They are used
//! in tests to ensure the implementation never violates key safety properties.
//!
//! The TLA+ models themselves live in `tla/` and are model-checked by TLC in
//! CI (`formal-verification` job, issue #221), mirroring c-RCP's own
//! `formal-verification` job (c-RCP `.github/workflows/ci.yml:193`):
//!
//! - `tla/LifecycleStateMachine.tla` -- shared with c-RCP; models
//!   [`crate::lifecycle::RcServerState`].
//! - `tla/E2ESafePoint.tla` -- shared with c-RCP; its SP1 models
//!   [`crate::request::check_watchdog_overflow_purge`].
//! - `tla/RequestLifecycle.tla` -- rust-RCP-original; models
//!   [`crate::request::RequestLifecycleState`], the cancellation trio and
//!   the watchdog-overflow purge.
//!
//! This module's `refinement` tests close the gap between model and code:
//! they exhaustively drive the real transition functions over each model's
//! finite abstract state space and use [`check_all`]/[`witness`] to assert
//! every step the code takes is a step of the corresponding TLA+ `Next`
//! relation and preserves its safety properties. See
//! `docs/FORMAL_VERIFICATION.md`.

// ── Invariant types ───────────────────────────────────────────────────────────

/// A named invariant with a checkable predicate.
//fusa:req REQ-FORMAL-001
pub struct Invariant<S> {
    pub name: &'static str,
    pub predicate: Box<dyn Fn(&S) -> bool + Send + Sync>,
}

impl<S> Invariant<S> {
    pub fn new(name: &'static str, f: impl Fn(&S) -> bool + Send + Sync + 'static) -> Self {
        Invariant {
            name,
            predicate: Box::new(f),
        }
    }

    pub fn check(&self, state: &S) -> bool {
        (self.predicate)(state)
    }
}

/// Result of checking a set of invariants against a state.
//fusa:req REQ-FORMAL-002
#[derive(Debug, Default)]
pub struct CheckResult {
    pub passed: Vec<&'static str>,
    pub failed: Vec<&'static str>,
}

impl CheckResult {
    pub fn all_passed(&self) -> bool {
        self.failed.is_empty()
    }
}

/// Check all invariants against `state`.
//fusa:req REQ-FORMAL-003
pub fn check_all<S>(state: &S, invs: &[Invariant<S>]) -> CheckResult {
    let mut result = CheckResult::default();
    for inv in invs {
        if inv.check(state) {
            result.passed.push(inv.name);
        } else {
            result.failed.push(inv.name);
        }
    }
    result
}

/// Witness: records the first state that violates an invariant.
//fusa:req REQ-FORMAL-004
pub fn witness<S: Clone>(states: &[S], inv: &Invariant<S>) -> Option<S> {
    states.iter().find(|s| !inv.check(s)).cloned()
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    //fusa:test REQ-FORMAL-001
    //fusa:test REQ-FORMAL-003
    fn invariant_passes_when_predicate_true() {
        let inv = Invariant::new("non-negative", |x: &i32| *x >= 0);
        let result = check_all(&42i32, std::slice::from_ref(&inv));
        assert!(result.all_passed());
    }

    #[test]
    //fusa:test REQ-FORMAL-003
    fn invariant_fails_when_predicate_false() {
        let inv = Invariant::new("always-false", |_: &i32| false);
        let result = check_all(&0i32, std::slice::from_ref(&inv));
        assert!(!result.all_passed());
        assert_eq!(result.failed, vec!["always-false"]);
    }

    #[test]
    //fusa:test REQ-FORMAL-002
    fn check_result_tracks_pass_fail() {
        let invs = vec![
            Invariant::new("pass", |x: &i32| *x >= 0),
            Invariant::new("fail", |x: &i32| *x < 0),
        ];
        let r = check_all(&5i32, &invs);
        assert_eq!(r.passed, vec!["pass"]);
        assert_eq!(r.failed, vec!["fail"]);
    }

    #[test]
    //fusa:test REQ-FORMAL-004
    fn witness_finds_first_violation() {
        let inv = Invariant::new("positive", |x: &i32| *x > 0);
        let states = vec![1, 2, -1, 3, -2];
        let w = witness(&states, &inv).unwrap();
        assert_eq!(w, -1);
    }

    #[test]
    //fusa:test REQ-FORMAL-004
    fn witness_returns_none_when_no_violation() {
        let inv = Invariant::new("positive", |x: &i32| *x > 0);
        let states = vec![1, 2, 3];
        assert!(witness(&states, &inv).is_none());
    }

    /// Refinement checks: the real transition functions against the TLA+
    /// models in `tla/` (issue #221). Each test explores the model's whole
    /// finite abstract state space by breadth-first search, driving every
    /// successor through the *real* rust function, and asserts every
    /// observed step is a step of the spec's `Next` and satisfies the
    /// spec's safety properties.
    mod refinement {
        use super::super::*;
        use crate::lifecycle::{
            check_hw_cfg, check_rcp_cfg, is_register_writable, EndpointPlausibility,
            PlausibilitySnapshot, RcServerState, RegisterCategory, WriterCtx,
        };
        use crate::request::{
            check_watchdog_overflow_purge, purge_normal_priority_on_watchdog_overflow,
            try_force_cancel_all, try_force_cancel_non_safestate, try_force_cancel_single,
            ClearTransactionNum, CompoundGateConfig, PendingRequestKey, RequestKind,
            RequestLifecycleGuardInput, RequestLifecycleState, SequencerState, TimedExecutionTime,
            TriggeredRuntime, TriggeredStep,
        };
        use std::collections::{BTreeSet, VecDeque};

        // ── tla/LifecycleStateMachine.tla ────────────────────────────────

        const LIFECYCLE_STATES: [RcServerState; 3] = [
            RcServerState::HwUnconfigured,
            RcServerState::HwConfigured,
            RcServerState::RcpConfigured,
        ];

        /// One TLA+ state: `<<state, hw_cfg_consistent, rcp_cfg_consistent,
        /// field_lock>>`, with `field_lock` derived from the real
        /// `is_register_writable` (W* = `RegisterCategory::HwConfig`).
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        struct LcState {
            state: u8,
            hw: bool,
            rcp: bool,
        }

        impl LcState {
            fn server(self) -> RcServerState {
                RcServerState::from_u8(self.state).unwrap()
            }
            fn locked(self) -> bool {
                !is_register_writable(self.server(), RegisterCategory::HwConfig)
            }
        }

        /// The spec's `Next` (plus stuttering), as a predicate on a step.
        /// Stuttering is the `revise_hw` case with an unchanged input.
        fn lifecycle_spec_next(pre: LcState, post: LcState) -> bool {
            let inputs_same = pre.hw == post.hw && pre.rcp == post.rcp;
            let state_same = pre.state == post.state;
            let lock_same = pre.locked() == post.locked();
            let revise_hw = state_same && pre.rcp == post.rcp && lock_same;
            let revise_rcp = state_same && pre.hw == post.hw && lock_same;
            let action = inputs_same && lifecycle_spec_action(pre, post);
            revise_hw || revise_rcp || action
        }

        /// The spec's five state-changing actions, ignoring `UNCHANGED`
        /// on the two inputs (checked by the caller).
        fn lifecycle_spec_action(pre: LcState, post: LcState) -> bool {
            use RcServerState::*;
            let lock_same = pre.locked() == post.locked();
            match (pre.server(), post.server()) {
                // PromoteToHwConfigured
                (HwUnconfigured, HwConfigured) => pre.hw && lock_same,
                // PromoteToRcpConfigured
                (HwConfigured, RcpConfigured) => pre.rcp && post.locked(),
                // DemoteToHwUnconfigured
                (HwConfigured, HwUnconfigured) => lock_same,
                // DemoteRcpToHwConfigured / FullReset
                (RcpConfigured, HwConfigured) | (RcpConfigured, HwUnconfigured) => !post.locked(),
                _ => false,
            }
        }

        /// A one-endpoint plausibility snapshot whose [`check_hw_cfg`]
        /// verdict is `hw` and whose [`check_rcp_cfg`] verdict is `rcp`:
        /// the HW check reads only `hw_pin_mapped`/`has_request_stream`,
        /// the RCP check only `has_stream_assoc` (no request stream is
        /// configured, so the stream-consistency half holds vacuously).
        fn snapshot_endpoint(hw: bool, rcp: bool) -> [EndpointPlausibility; 1] {
            [EndpointPlausibility {
                ep_used: true,
                hw_pin_mapped: hw,
                has_request_stream: true,
                has_stream_assoc: rcp,
                request_stream_index: 0,
            }]
        }

        /// Every writer context (all 32 flag combinations).
        fn all_writers() -> Vec<WriterCtx> {
            (0u8..32)
                .map(|m| WriterCtx {
                    via_root_client_ep0: m & 1 != 0,
                    via_owning_stream: m & 2 != 0,
                    via_non_unicast_frame: m & 4 != 0,
                    via_discovery_stream: m & 8 != 0,
                    via_valid_stream_association: m & 16 != 0,
                })
                .collect()
        }

        /// Successors of `s` as the real code produces them: the two
        /// environment inputs revised freely, and `try_transition` towards
        /// every target under a plausibility snapshot whose two verdicts
        /// are exactly the spec's two inputs, for every writer context and
        /// both idleness verdicts. Authorization and idleness only ever
        /// remove steps, so enumerating all of them (including the most
        /// permissive) cannot mask a forbidden step.
        fn lifecycle_successors(s: LcState) -> Vec<LcState> {
            let mut out = Vec::new();
            for b in [false, true] {
                out.push(LcState { hw: b, ..s });
                out.push(LcState { rcp: b, ..s });
            }
            let eps = snapshot_endpoint(s.hw, s.rcp);
            let snap = PlausibilitySnapshot {
                endpoints: &eps,
                request_streams: &[],
                response_stream_count: 0,
            };
            assert_eq!(check_hw_cfg(Some(&snap)).is_ok(), s.hw);
            assert_eq!(check_rcp_cfg(Some(&snap)).is_ok(), s.rcp);
            let from = s.server();
            for target in LIFECYCLE_STATES {
                for writer in all_writers() {
                    for idle in [false, true] {
                        if let Ok(next) = from.try_transition(target, Some(&snap), writer, idle) {
                            out.push(LcState {
                                state: next.to_u8(),
                                ..s
                            });
                        }
                    }
                }
            }
            out
        }

        fn lifecycle_explore() -> (BTreeSet<LcState>, Vec<(LcState, LcState)>) {
            let mut seen = BTreeSet::new();
            let mut steps = Vec::new();
            let mut queue = VecDeque::new();
            for hw in [false, true] {
                for rcp in [false, true] {
                    let init = LcState {
                        state: RcServerState::INITIAL.to_u8(),
                        hw,
                        rcp,
                    };
                    assert!(!init.locked(), "Init: field_lock = Unlocked");
                    if seen.insert(init) {
                        queue.push_back(init);
                    }
                }
            }
            while let Some(s) = queue.pop_front() {
                for n in lifecycle_successors(s) {
                    steps.push((s, n));
                    if seen.insert(n) {
                        queue.push_back(n);
                    }
                }
            }
            (seen, steps)
        }

        #[test]
        //fusa:test REQ-FORMAL-003
        //fusa:test REQ-FORMAL-004
        //fusa:test REQ-LIFE-006
        //fusa:test REQ-LIFE-010
        //fusa:test REQ-LIFE-019
        fn lifecycle_refines_tla_spec() {
            let (reachable, steps) = lifecycle_explore();

            // Non-vacuity: every lifecycle state is reached.
            let states: BTreeSet<u8> = reachable.iter().map(|s| s.state).collect();
            assert_eq!(states.len(), 3, "reachable: {reachable:?}");

            let step_invariants = lifecycle_step_invariants();
            for inv in &step_invariants {
                assert_eq!(witness(&steps, inv), None, "{} violated", inv.name);
            }
            for step in &steps {
                assert!(check_all(step, &step_invariants).all_passed());
            }
            assert_eventually_rcp_configured(&reachable);
        }

        /// The spec's `Next` plus its step-level safety properties.
        fn lifecycle_step_invariants() -> Vec<Invariant<(LcState, LcState)>> {
            vec![
                Invariant::new("Next", |(a, b): &(LcState, LcState)| {
                    lifecycle_spec_next(*a, *b)
                }),
                // SP1 NoSkipConfiguration
                Invariant::new("NoSkipConfiguration", |(a, b): &(LcState, LcState)| {
                    !(a.server() == RcServerState::HwUnconfigured
                        && b.server() == RcServerState::RcpConfigured)
                }),
                // SP2 FieldLockMonotonicWhileConfigured
                Invariant::new(
                    "FieldLockMonotonicWhileConfigured",
                    |(a, b): &(LcState, LcState)| {
                        let configured = RcServerState::RcpConfigured;
                        !(a.server() == configured && a.locked() && b.server() == configured)
                            || b.locked()
                    },
                ),
            ]
        }

        /// LP1 (bounded form): with both inputs consistent and held fixed,
        /// RcpConfigured is reachable from every reachable state through
        /// implementation steps alone.
        fn assert_eventually_rcp_configured(reachable: &BTreeSet<LcState>) {
            for &start in reachable.iter().filter(|s| s.hw && s.rcp) {
                let mut seen = BTreeSet::from([start]);
                let mut queue = VecDeque::from([start]);
                let mut reached = false;
                while let Some(s) = queue.pop_front() {
                    if s.server() == RcServerState::RcpConfigured {
                        reached = true;
                        break;
                    }
                    for n in lifecycle_successors(s) {
                        if n.hw && n.rcp && seen.insert(n) {
                            queue.push_back(n);
                        }
                    }
                }
                assert!(reached, "EventuallyRcpConfigured fails from {start:?}");
            }
        }

        // ── tla/RequestLifecycle.tla ─────────────────────────────────────

        const REQUEST_STATES: [RequestLifecycleState; 4] = [
            RequestLifecycleState::Pending,
            RequestLifecycleState::Started,
            RequestLifecycleState::UnderExecution,
            RequestLifecycleState::Finalized,
        ];

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum Cause {
            None,
            Completed,
            ClearAll,
            ClearNonSafestate,
            ClearSingle,
            WatchdogPurge,
        }

        /// One TLA+ step's observable part: `<<state, cause>>` before and
        /// after, plus the request's static attributes.
        #[derive(Debug, Clone, Copy, PartialEq)]
        struct RqStep {
            pre: RequestLifecycleState,
            post: RequestLifecycleState,
            cause: Cause,
            safety_tagged: bool,
            safestate_related: bool,
        }

        fn request_spec_next(s: &RqStep) -> bool {
            use RequestLifecycleState::*;
            let not_final = s.pre != Finalized;
            match s.cause {
                Cause::None => {
                    s.pre == s.post
                        || matches!(
                            (s.pre, s.post),
                            (Pending, Started) | (Started, UnderExecution)
                        )
                }
                Cause::Completed => s.pre == UnderExecution && s.post == Finalized,
                Cause::ClearAll | Cause::ClearSingle => not_final && s.post == Finalized,
                Cause::ClearNonSafestate => {
                    not_final && !s.safestate_related && s.post == Finalized
                }
                Cause::WatchdogPurge => not_final && !s.safety_tagged && s.post == Finalized,
            }
        }

        fn all_request_kinds() -> Vec<RequestKind> {
            (0u8..=255)
                .filter_map(|raw| RequestKind::from_u8(raw).ok())
                .collect()
        }

        /// Guard inputs covering both verdicts of every guarded hop.
        fn guard_inputs() -> Vec<RequestLifecycleGuardInput> {
            let gate = CompoundGateConfig {
                sequencer_num: 0,
                start_state: SequencerState(1),
            };
            let out_of_range_gate = CompoundGateConfig {
                sequencer_num: 9,
                start_state: SequencerState(1),
            };
            let mut v = vec![
                RequestLifecycleGuardInput::Standard,
                RequestLifecycleGuardInput::ClearAll,
                RequestLifecycleGuardInput::ClearNonSafestate,
                RequestLifecycleGuardInput::ClearSingle,
            ];
            for (current, exec) in [(50, 100), (100, 100), (200, 100)] {
                v.push(RequestLifecycleGuardInput::Timed {
                    current,
                    exec_time: TimedExecutionTime::new(exec).unwrap(),
                });
            }
            for cs in [false, true] {
                for predecessor_errored in [false, true] {
                    v.push(RequestLifecycleGuardInput::Chained {
                        cs,
                        predecessor_errored,
                    });
                }
            }
            let step = TriggeredStep {
                trigger_threshold: 1,
                exec_delay: 2,
                ..TriggeredStep::default()
            };
            for started in [false, true] {
                for occurrence_count in [0, 1, 2] {
                    for elapsed in [1, 2] {
                        for endpoint_idle in [false, true] {
                            v.push(RequestLifecycleGuardInput::Triggered {
                                step,
                                runtime: TriggeredRuntime {
                                    occurrence_count,
                                    started,
                                },
                                elapsed,
                                endpoint_idle,
                            });
                        }
                    }
                }
            }
            for state in [0, 1] {
                for g in [gate, out_of_range_gate] {
                    v.push(RequestLifecycleGuardInput::Compound {
                        current_sequencer_state: SequencerState(state),
                        gate: g,
                        svr_sequencers_max: 4,
                    });
                    v.push(RequestLifecycleGuardInput::CompoundWait {
                        current_sequencer_state: SequencerState(state),
                        gate: g,
                        svr_sequencers_max: 4,
                    });
                }
            }
            v
        }

        /// `try_transition` towards every target under every guard input,
        /// as `(post, cause)` pairs.
        fn guarded_hops(
            pre: RequestLifecycleState,
            inputs: &[RequestLifecycleGuardInput],
        ) -> Vec<(RequestLifecycleState, Cause)> {
            let mut out = Vec::new();
            for target in REQUEST_STATES {
                for input in inputs {
                    if let Ok(post) = pre.try_transition(target, input) {
                        let cause = if post == RequestLifecycleState::Finalized {
                            Cause::Completed
                        } else {
                            Cause::None
                        };
                        out.push((post, cause));
                    }
                }
            }
            out
        }

        /// `cause` if `post` moved off `pre`, else a stutter.
        fn caused(
            pre: RequestLifecycleState,
            post: RequestLifecycleState,
            cause: Cause,
        ) -> (RequestLifecycleState, Cause) {
            (post, if post != pre { cause } else { Cause::None })
        }

        /// The three cancellation functions applied to `pre`.
        fn cancel_steps(
            pre: RequestLifecycleState,
            safestate_related: bool,
        ) -> Vec<(RequestLifecycleState, Cause)> {
            let mut out = Vec::new();
            let mut s = pre;
            let _ = try_force_cancel_all(&mut s);
            out.push(caused(pre, s, Cause::ClearAll));
            let mut s = pre;
            let _ = try_force_cancel_non_safestate(&mut s, safestate_related);
            out.push(caused(pre, s, Cause::ClearNonSafestate));
            for candidate in [3u8, 4] {
                let mut s = pre;
                let _ = try_force_cancel_single(&mut s, candidate, ClearTransactionNum(3));
                out.push(caused(pre, s, Cause::ClearSingle));
            }
            out
        }

        /// The watchdog purge applied to `pre`. The single-request rule and
        /// its slice composition must agree; a purge finalizes the request.
        fn purge_steps(
            pre: RequestLifecycleState,
            kind: RequestKind,
        ) -> Vec<(RequestLifecycleState, Cause)> {
            let mut out = Vec::new();
            for overflowed in [false, true] {
                let single = check_watchdog_overflow_purge(kind, overflowed).is_err();
                let key = PendingRequestKey {
                    kind,
                    arrival_seq: 0,
                };
                let (_, purged) = purge_normal_priority_on_watchdog_overflow(&[key], overflowed);
                assert_eq!(
                    single,
                    purged == vec![0],
                    "{kind:?} overflowed={overflowed}"
                );
                if single && pre != RequestLifecycleState::Finalized {
                    out.push((RequestLifecycleState::Finalized, Cause::WatchdogPurge));
                }
            }
            out
        }

        /// Every step the real request functions can take from `pre`.
        fn request_steps(
            pre: RequestLifecycleState,
            kind: RequestKind,
            safestate_related: bool,
            inputs: &[RequestLifecycleGuardInput],
        ) -> Vec<RqStep> {
            let safety_tagged = kind.is_safety_tagged();
            let step = |post, cause| RqStep {
                pre,
                post,
                cause,
                safety_tagged,
                safestate_related,
            };
            guarded_hops(pre, inputs)
                .into_iter()
                .chain(cancel_steps(pre, safestate_related))
                .chain(purge_steps(pre, kind))
                .map(|(post, cause)| step(post, cause))
                .collect()
        }

        #[test]
        //fusa:test REQ-FORMAL-003
        //fusa:test REQ-FORMAL-004
        //fusa:test REQ-RLC-002
        //fusa:test REQ-RLC-006
        //fusa:test REQ-SAFETY-004
        //fusa:test REQ-SAFETY-005
        fn request_lifecycle_refines_tla_spec() {
            let inputs = guard_inputs();
            let kinds = all_request_kinds();
            assert_eq!(kinds.len(), 12, "every RequestKind is explored");

            let (steps, reached) = request_explore(&kinds, &inputs);
            assert_eq!(reached.len(), 4, "non-vacuity: every state reached");
            assert!(steps.iter().any(|s| s.cause == Cause::WatchdogPurge));
            assert!(steps.iter().any(|s| s.cause == Cause::ClearNonSafestate));
            for inv in &request_invariants() {
                assert_eq!(witness(&steps, inv), None, "{} violated", inv.name);
            }
        }

        /// The spec's `Next` plus SP1-SP4.
        fn request_invariants() -> Vec<Invariant<RqStep>> {
            vec![
                Invariant::new("Next", request_spec_next),
                // SP1 NoSkipHop
                Invariant::new("NoSkipHop", |s: &RqStep| {
                    use RequestLifecycleState::*;
                    s.pre == s.post
                        || s.post == Finalized
                        || matches!(
                            (s.pre, s.post),
                            (Pending, Started) | (Started, UnderExecution)
                        )
                }),
                // SP2 FinalizedTerminal
                Invariant::new("FinalizedTerminal", |s: &RqStep| {
                    s.pre != RequestLifecycleState::Finalized
                        || s.post == RequestLifecycleState::Finalized
                }),
                // SP3 SafetyRequestsSurvivePurge
                Invariant::new("SafetyRequestsSurvivePurge", |s: &RqStep| {
                    s.cause != Cause::WatchdogPurge || !s.safety_tagged
                }),
                // SP4 SafestateRequestsSurviveNonSafestateClear
                Invariant::new("SafestateRequestsSurviveNonSafestateClear", |s: &RqStep| {
                    s.cause != Cause::ClearNonSafestate || !s.safestate_related
                }),
            ]
        }

        /// Breadth-first search from `Pending` for every kind and both
        /// `safestate_related` values: every step taken, and every state
        /// reached (by name).
        fn request_explore(
            kinds: &[RequestKind],
            inputs: &[RequestLifecycleGuardInput],
        ) -> (Vec<RqStep>, BTreeSet<String>) {
            let mut steps = Vec::new();
            let mut reached = BTreeSet::new();
            for &kind in kinds {
                for safestate_related in [false, true] {
                    let mut seen = vec![RequestLifecycleState::Pending];
                    let mut queue = VecDeque::from([RequestLifecycleState::Pending]);
                    while let Some(pre) = queue.pop_front() {
                        for s in request_steps(pre, kind, safestate_related, inputs) {
                            if !seen.contains(&s.post) {
                                seen.push(s.post);
                                queue.push_back(s.post);
                            }
                            steps.push(s);
                        }
                    }
                    reached.extend(seen.iter().map(|s| format!("{s:?}")));
                }
            }
            (steps, reached)
        }

        // ── tla/E2ESafePoint.tla SP1 ─────────────────────────────────────

        #[test]
        //fusa:test REQ-SAFETY-004
        //fusa:test REQ-SAFETY-005
        fn watchdog_purge_refines_e2e_safe_point_sp1() {
            // SafetyRequestsSurvivePurge over a mixed queue: on overflow
            // every safety-tagged entry is kept and every normal entry is
            // purged (Miss(s) with s \in SafestateEnabled).
            let kinds = all_request_kinds();
            let pending: Vec<PendingRequestKey> = kinds
                .iter()
                .enumerate()
                .map(|(i, &kind)| PendingRequestKey {
                    kind,
                    arrival_seq: i as u64,
                })
                .collect();
            let (kept, purged) = purge_normal_priority_on_watchdog_overflow(&pending, true);
            for i in kept {
                assert!(pending[i].kind.is_safety_tagged(), "{:?}", pending[i]);
            }
            assert!(!purged.is_empty());
            for i in purged {
                assert!(!pending[i].kind.is_safety_tagged(), "{:?}", pending[i]);
            }
            let (kept, purged) = purge_normal_priority_on_watchdog_overflow(&pending, false);
            assert_eq!(kept.len(), pending.len());
            assert!(purged.is_empty());
        }
    }
}
