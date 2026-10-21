---- MODULE RequestLifecycle ----
(*
 * Formal specification of rust-RCP's per-request lifecycle state machine
 * ("request store" side of issue #221): crate::request::RequestLifecycleState
 * (Pending -> Started -> UnderExecution -> Finalized), its type-specific
 * hop guards (RequestLifecycleState::try_transition), the cancellation
 * trio (try_force_cancel_all / try_force_cancel_non_safestate /
 * try_force_cancel_single), and the watchdog-overflow purge
 * (check_watchdog_overflow_purge).
 *
 * c-RCP has no counterpart spec: c-RCP tracks requests in
 * rcp_server_t's request store without a first-class four-state lifecycle
 * type, so this is a rust-RCP-original model in the style of c-RCP's
 * LifecycleStateMachine.tla / E2ESafePoint.tla (same conventions: Spec /
 * FairSpec, guard verdicts as free environment inputs, safety properties
 * as action properties, one fairness-conditioned liveness property).
 *
 * One request is modeled; requests do not interact through this state
 * machine (ordering between requests is select_next_pending_request's
 * concern, a stateless pure function), so one instance is a faithful
 * abstraction. The request's two static attributes are chosen
 * nondeterministically at Init so every combination is checked:
 *   safety_tagged      -- RequestKind::is_safety_tagged (0x8F/0x8B/0x8E)
 *   safestate_related  -- the clear-non-safestate exemption input
 *
 * Safety properties:
 *   SP1 NoSkipHop: the only state changes are the three linear forward
 *       hops or a jump to Finalized (completion / cancellation / purge).
 *   SP2 FinalizedTerminal: Finalized is never left.
 *   SP3 SafetyRequestsSurvivePurge: a watchdog-overflow purge never
 *       finalizes a safety-tagged request (same guarantee as
 *       E2ESafePoint.tla's SP1, at the request level).
 *   SP4 SafestateRequestsSurviveNonSafestateClear: a clear-non-safestate
 *       cancellation never finalizes a safestate-related request.
 * Liveness property:
 *   LP1 EventuallyFinalized: if the hop guards eventually settle and stay
 *       satisfied, the request eventually reaches Finalized under weak
 *       fairness of the three forward hops.
 *
 * src/formal.rs's `request_lifecycle_refines_tla_spec` test exhaustively
 * drives the real rust functions from every state with every guard input
 * shape and asserts each step taken is a step of Next and satisfies
 * SP1-SP4.
 *)

EXTENDS TLC

Pending        == "Pending"
Started        == "Started"
UnderExecution == "UnderExecution"
Finalized      == "Finalized"

States == {Pending, Started, UnderExecution, Finalized}

\* How the request reached Finalized (history variable for SP3/SP4).
Causes == {"none", "completed", "clear_all", "clear_non_safestate",
           "clear_single", "watchdog_purge"}

VARIABLES state,             \* RequestLifecycleState
          guard_ok,          \* current verdict of the kind-specific hop guard
          safety_tagged,     \* RequestKind::is_safety_tagged (static)
          safestate_related, \* clear-non-safestate exemption (static)
          cause              \* history: what finalized the request

vars == <<state, guard_ok, safety_tagged, safestate_related, cause>>

TypeOK ==
    /\ state             \in States
    /\ guard_ok          \in BOOLEAN
    /\ safety_tagged     \in BOOLEAN
    /\ safestate_related \in BOOLEAN
    /\ cause             \in Causes

Init ==
    /\ state             = Pending
    /\ guard_ok          \in BOOLEAN
    /\ safety_tagged     \in BOOLEAN
    /\ safestate_related \in BOOLEAN
    /\ cause             = "none"

(* The guard inputs (timed exec time vs. presentation time, sequencer
 * state, chain predecessor outcome, trigger repeat count) change between
 * evaluation attempts; their verdict is an unconstrained environment
 * input. *)
ReviseGuard ==
    /\ guard_ok' \in BOOLEAN
    /\ UNCHANGED <<state, safety_tagged, safestate_related, cause>>

(* Pending -> Started: gated by is_timed_request_ready / check_compound_gate
 * for Timed / Compound / CompoundWait kinds (REQ-RLC-003). *)
Start ==
    /\ state = Pending
    /\ guard_ok
    /\ state' = Started
    /\ UNCHANGED <<guard_ok, safety_tagged, safestate_related, cause>>

(* Started -> UnderExecution: gated by check_chain_continuation /
 * trigger-repeat exhaustion for Chained / Triggered kinds (REQ-RLC-004). *)
Execute ==
    /\ state = Started
    /\ guard_ok
    /\ state' = UnderExecution
    /\ UNCHANGED <<guard_ok, safety_tagged, safestate_related, cause>>

(* UnderExecution -> Finalized: unconditional for every kind (REQ-RLC-005). *)
Complete ==
    /\ state = UnderExecution
    /\ state' = Finalized
    /\ cause' = "completed"
    /\ UNCHANGED <<guard_ok, safety_tagged, safestate_related>>

(* try_force_cancel_all: mandatory, unconditional (REQ-RLC-006). *)
ClearAll ==
    /\ state # Finalized
    /\ state' = Finalized
    /\ cause' = "clear_all"
    /\ UNCHANGED <<guard_ok, safety_tagged, safestate_related>>

(* try_force_cancel_non_safestate: exempts safestate-related requests. *)
ClearNonSafestate ==
    /\ state # Finalized
    /\ ~safestate_related
    /\ state' = Finalized
    /\ cause' = "clear_non_safestate"
    /\ UNCHANGED <<guard_ok, safety_tagged, safestate_related>>

(* try_force_cancel_single: the transaction-number match is an environment
 * input, so it is modeled as nondeterministically enabled. *)
ClearSingle ==
    /\ state # Finalized
    /\ state' = Finalized
    /\ cause' = "clear_single"
    /\ UNCHANGED <<guard_ok, safety_tagged, safestate_related>>

(* check_watchdog_overflow_purge on an overflow: a normal-priority request
 * is purged (REQ-SAFETY-004); a safety-tagged one is exempt. *)
WatchdogPurge ==
    /\ state # Finalized
    /\ ~safety_tagged
    /\ state' = Finalized
    /\ cause' = "watchdog_purge"
    /\ UNCHANGED <<guard_ok, safety_tagged, safestate_related>>

Next ==
    \/ ReviseGuard
    \/ Start
    \/ Execute
    \/ Complete
    \/ ClearAll
    \/ ClearNonSafestate
    \/ ClearSingle
    \/ WatchdogPurge

Spec == Init /\ [][Next]_vars

(* Weak fairness on the three forward hops suffices: once guard_ok stays
 * TRUE, nothing but the hop itself (or a jump to Finalized, which only
 * helps) disables the hop at the current state. *)
FairSpec == Spec /\ WF_vars(Start) /\ WF_vars(Execute) /\ WF_vars(Complete)

ForwardHops == {<<Pending, Started>>, <<Started, UnderExecution>>,
                <<UnderExecution, Finalized>>}

(* SP1 *)
NoSkipHop ==
    [][state' # state =>
         (<<state, state'>> \in ForwardHops \/ state' = Finalized)]_<<state>>

(* SP2 *)
FinalizedTerminal ==
    [][state = Finalized => state' = Finalized]_<<state>>

(* SP3 *)
SafetyRequestsSurvivePurge ==
    [](cause = "watchdog_purge" => ~safety_tagged)

(* SP4 *)
SafestateRequestsSurviveNonSafestateClear ==
    [](cause = "clear_non_safestate" => ~safestate_related)

GuardEventuallyStable == <>[](guard_ok)

(* LP1 *)
EventuallyFinalized == GuardEventuallyStable => <>(state = Finalized)

THEOREM Spec => TypeOK /\ NoSkipHop /\ FinalizedTerminal
                /\ SafetyRequestsSurvivePurge
                /\ SafestateRequestsSurviveNonSafestateClear
THEOREM FairSpec => EventuallyFinalized

====
