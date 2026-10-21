---- MODULE E2ESafePoint ----
(*
 * Formal specification of the TC18 per-request-stream watchdog and
 * safety-tagged-request execution gate (ROADMAP.md Phase 18, "E2E
 * Protection (Safe Points)", milestone 70; ISO 26262 ASIL-B). Replaces
 * AntiReplayGuard.tla: the retired CRC-16 sequence-counter/replay-window
 * mechanism it modeled has no TC18 counterpart in this codebase --
 * `include/rcp/e2e.h`'s own file header records this explicitly ("this
 * module does not reimplement the pre-replacement content's
 * sequence-counter/replay-window mechanism... the same gap cpp-RCP's and
 * rust-RCP's own merges left open"). What e2e.h implements instead, and
 * what this spec formalizes, is a different pair of safety guarantees:
 * a watchdog-overflow purge that never discards a pending safety-tagged
 * request, and an execution gate that only ever lets a safety-tagged
 * request run once its endpoint has reached its configured safe state.
 *
 * This models one request stream's `rx_wd_enable`/`rx_wd_timeout_ms`/
 * `rx_wd_safestate_enable` watchdog (`rcp_e2e_wd_evaluate()`), its
 * pending-request queue split into a safety-tagged and a normal
 * component (`rcp_e2e_watchdog_purge_should_keep()` /
 * `_classify()`), and the safety-tagged execution admission rule
 * (`rcp_e2e_request_may_execute()`) against a polled
 * `endpoint_in_safe_state` measurement
 * (`rcp_e2e_endpoint_in_safe_state()`). CRC32 well-formedness itself
 * (`rcp_e2e_wrap()`/`_unwrap()`) is a pure per-frame computation with no
 * interesting state-transition behavior to model checking, and is
 * covered by `tests/test_e2e.c` instead.
 *
 * Safety property (SP1): a watchdog-overflow purge with
 * rx_wd_safestate_enable set never discards a pending safety-tagged
 * request -- only normal-tagged requests are purged.
 * Safety property (SP2): a safety-tagged request only ever transitions
 * from pending to executed while its endpoint reports it has reached
 * the configured safe state.
 *
 * Liveness property (LP1, c-RCP-23d / issue #602): a safety-tagged
 * request that becomes pending eventually executes, given per-stream
 * fair scheduling of ExecuteSafety and an endpoint safe-state signal
 * that eventually settles and stays true. issue #602's own suggested
 * wording -- "a pending safety-tagged request is eventually either
 * executed or purged, never stuck pending forever" -- is false by
 * construction against this spec's own SP1 above: Miss(s) (the only
 * purge event) never touches safety_pending by design, so "purged" is
 * never a live alternative for a safety-tagged request, and a property
 * requiring "executed or purged" is unprovable as stated (TLC finds a
 * trivial submit-and-never-purge counterexample). LP1 below is the
 * corrected, narrower claim this spec can actually make and TLC
 * confirms holds. TLC's default no-successor-state deadlock check has
 * already passed on every CI run since this spec was added (predating
 * this issue -- every state in the model has at least one enabled
 * successor). LP1 is a related but strictly stronger claim: per-stream
 * progress/livelock-freedom under fairness, not mere deadlock-freedom.
 * See FORMAL_VERIFICATION.md's "Liveness Properties" section for the
 * fairness-minimality experiment (WF vs. SF, re-run against real TLC)
 * that determined weak fairness is the correct minimum here, not SF.
 *)

(*
 * rust-RCP note (issue #221): shared with c-RCP, copied verbatim from
 * c-RCP tla/E2ESafePoint.tla (c-RCP a4f3a1f) and checked by the same TLC
 * invocation in rust-RCP's `formal-verification` CI job. Rust mapping:
 *   rcp_e2e_watchdog_purge_should_keep() / _classify()
 *       -> crate::request::check_watchdog_overflow_purge /
 *          purge_normal_priority_on_watchdog_overflow
 *   MSB-set (safety-tagged) request -> RequestKind::is_safety_tagged
 * SP1 (SafetyRequestsSurvivePurge) is tied to the rust code by
 * src/formal.rs's `request_lifecycle_refines_tla_spec` and
 * `watchdog_purge_refines_e2e_safe_point_sp1` tests. SP2/LP1
 * (rcp_e2e_request_may_execute()'s endpoint-safe-state execution gate)
 * have no rust counterpart yet -- rust-RCP has no port of
 * rcp_e2e_request_may_execute()/rcp_e2e_endpoint_in_safe_state() (tracked
 * by issues #187/#203); until that port lands this spec checks c-RCP's
 * design for them, not rust code. See docs/FORMAL_VERIFICATION.md.
 *)

EXTENDS TLC

CONSTANTS Streams,             \* set of request-stream identifiers
          SafestateEnabled,    \* streams with rx_wd_safestate_enable set
          WatchdogEnabled      \* streams with rx_wd_enable set

ASSUME SafestateEnabled \subseteq Streams
ASSUME WatchdogEnabled  \subseteq Streams

VARIABLES overflowed,           \* Stream -> BOOLEAN: rx_wd_evaluate() overflow verdict
          endpoint_in_safe_state, \* Stream -> BOOLEAN: polled safe-state measurement
          safety_pending,       \* Stream -> BOOLEAN: a safety-tagged request is queued
          normal_pending        \* Stream -> BOOLEAN: a normal-tagged request is queued

vars == <<overflowed, endpoint_in_safe_state, safety_pending, normal_pending>>

TypeOK ==
    /\ overflowed             \in [Streams -> BOOLEAN]
    /\ endpoint_in_safe_state \in [Streams -> BOOLEAN]
    /\ safety_pending         \in [Streams -> BOOLEAN]
    /\ normal_pending         \in [Streams -> BOOLEAN]

Init ==
    /\ overflowed             = [s \in Streams |-> FALSE]
    /\ endpoint_in_safe_state \in [Streams -> BOOLEAN]
    /\ safety_pending         = [s \in Streams |-> FALSE]
    /\ normal_pending         = [s \in Streams |-> FALSE]

(* rcp_watchdog_keeper_kick()-equivalent: resets a stream's elapsed-since-
 * last-kick clock, clearing any overflow verdict (REQ-E2E-024/-WDG-003). *)
Kick(s) ==
    /\ overflowed' = [overflowed EXCEPT ![s] = FALSE]
    /\ UNCHANGED <<endpoint_in_safe_state, safety_pending, normal_pending>>

(* rcp_e2e_wd_evaluate() reporting overflow once elapsed reaches
 * rx_wd_timeout_ms (REQ-E2E-025) -- only possible while the watchdog is
 * enabled for this stream (REQ-E2E-024: a disabled watchdog never
 * overflows). When rx_wd_safestate_enable is also set, this is exactly
 * the watchdog-purge event: every pending normal-tagged request is
 * discarded, but a pending safety-tagged request survives untouched
 * (REQ-E2E-014, REQ-E2E-015, REQ-E2E-026). *)
Miss(s) ==
    /\ s \in WatchdogEnabled
    /\ overflowed[s] = FALSE
    /\ overflowed' = [overflowed EXCEPT ![s] = TRUE]
    /\ IF s \in SafestateEnabled
       THEN normal_pending' = [normal_pending EXCEPT ![s] = FALSE]
       ELSE UNCHANGED normal_pending
    /\ UNCHANGED <<endpoint_in_safe_state, safety_pending>>

(* A caller submits a safety-tagged (MSB-set) request; e2e.h's own gate
 * only ever governs *execution*, not admission into the queue, so
 * submission itself is unconditional (REQ-E2E-011). *)
SubmitSafety(s) ==
    /\ safety_pending' = [safety_pending EXCEPT ![s] = TRUE]
    /\ UNCHANGED <<overflowed, endpoint_in_safe_state, normal_pending>>

SubmitNormal(s) ==
    /\ normal_pending' = [normal_pending EXCEPT ![s] = TRUE]
    /\ UNCHANGED <<overflowed, endpoint_in_safe_state, safety_pending>>

(* rcp_e2e_request_may_execute(): a safety-tagged request executes only
 * once the endpoint reports it has reached its configured safe state
 * (REQ-E2E-012). *)
ExecuteSafety(s) ==
    /\ safety_pending[s]
    /\ endpoint_in_safe_state[s]
    /\ safety_pending' = [safety_pending EXCEPT ![s] = FALSE]
    /\ UNCHANGED <<overflowed, endpoint_in_safe_state, normal_pending>>

(* A non-safety-tagged request is never gated by endpoint_in_safe_state
 * (REQ-E2E-013). *)
ExecuteNormal(s) ==
    /\ normal_pending[s]
    /\ normal_pending' = [normal_pending EXCEPT ![s] = FALSE]
    /\ UNCHANGED <<overflowed, endpoint_in_safe_state, safety_pending>>

(* rcp_e2e_endpoint_in_safe_state()'s own polled measurement changing --
 * e.g. RCP_E2E_MEASURE_SEQUENCER's target-state comparison flipping as
 * the physical endpoint moves. *)
ObserveSafeState(s) ==
    /\ endpoint_in_safe_state' \in [Streams -> BOOLEAN]
    /\ UNCHANGED <<overflowed, safety_pending, normal_pending>>

Next ==
    \E s \in Streams :
        \/ Kick(s)
        \/ Miss(s)
        \/ SubmitSafety(s)
        \/ SubmitNormal(s)
        \/ ExecuteSafety(s)
        \/ ExecuteNormal(s)
        \/ ObserveSafeState(s)

Spec == Init /\ [][Next]_<<overflowed, endpoint_in_safe_state, safety_pending, normal_pending>>

(* FairSpec adds weak fairness, per stream, on ExecuteSafety(s) -- and
 * only weak fairness. Re-derived and confirmed against real TLC runs
 * rather than assumed (see FORMAL_VERIFICATION.md): nothing in this
 * spec can clear safety_pending[s] except ExecuteSafety(s) itself
 * (Miss(s) leaves it untouched by SP1), so once safety_pending[s] holds
 * and endpoint_in_safe_state[s] holds continuously, ExecuteSafety(s)
 * stays continuously enabled until it is taken -- exactly the condition
 * WF acts on. Strong fairness buys nothing extra here; TLC confirms a
 * WF-only variant already suffices once LP1's antecedent below holds. *)
FairSpec == Spec /\ (\A s \in Streams : WF_vars(ExecuteSafety(s)))

(* LP1's antecedent, per stream: the endpoint's polled safe-state signal
 * eventually settles and stays TRUE. Without this, ObserveSafeState
 * (deliberately left unfair, matching its role as an unconstrained
 * polled environment measurement) could keep flipping
 * endpoint_in_safe_state[s] forever, which would repeatedly disable
 * ExecuteSafety(s) right as it becomes enabled -- WF only acts on an
 * action that is *continuously* enabled, and TLC confirms dropping this
 * antecedent produces exactly that flapping-endpoint counterexample. *)
EndpointEventuallyStable(s) == <>[](endpoint_in_safe_state[s])

(* LP1: for every stream, given its endpoint signal eventually settling
 * true and fair (WF) scheduling of that stream's ExecuteSafety, a
 * safety-tagged request that becomes pending on that stream eventually
 * executes. *)
EventuallySafetyExecutes ==
    \A s \in Streams :
        EndpointEventuallyStable(s) => [](safety_pending[s] => <>~safety_pending[s])

(* SP1: a watchdog-overflow purge (safestate-enabled Miss) never clears a
 * pending safety-tagged request -- only Miss can purge, and Miss leaves
 * safety_pending entirely unchanged by construction; this property
 * confirms that guarantee holds for every reachable step, not just by
 * inspection of the action definition. *)
SafetyRequestsSurvivePurge ==
    [][\A s \in Streams :
        (overflowed[s] = FALSE /\ overflowed'[s] = TRUE /\ s \in SafestateEnabled /\ safety_pending[s])
            => safety_pending'[s]]_<<overflowed, safety_pending>>

(* SP2: a safety-tagged request only ever transitions from pending to
 * not-pending while its endpoint was reporting safe state -- i.e. the
 * only way safety_pending[s] goes TRUE -> FALSE is ExecuteSafety(s),
 * whose own guard requires endpoint_in_safe_state[s]. *)
NoUnsafeSafetyExecution ==
    [][\A s \in Streams :
        (safety_pending[s] /\ ~safety_pending'[s]) => endpoint_in_safe_state[s]]_<<safety_pending, endpoint_in_safe_state>>

THEOREM Spec => TypeOK /\ SafetyRequestsSurvivePurge /\ NoUnsafeSafetyExecution
THEOREM FairSpec => EventuallySafetyExecutes

====
