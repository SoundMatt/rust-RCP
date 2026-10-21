---- MODULE LifecycleStateMachine ----
(*
 * Formal specification of the TC18 RC Server lifecycle state machine
 * (ROADMAP.md Phase 14, "RC Server lifecycle state machine", milestone
 * 61; ISO 26262 ASIL-B). Replaces WatchdogProtocol.tla and
 * HealthStateMachine.tla, whose Healthy/Degraded/Faulted zone-controller
 * model was retired along with rcp_zone_t/rcp_controller_t (see
 * ROADMAP.md's Protocol Replacement Notice and Satellite Package
 * Disposition table).
 *
 * An RC Server is a single node with exactly one lifecycle state (no
 * per-zone fan-out the way the retired watchdog model had one state per
 * zone -- "redundancy.h/redundancy.c" | DEPRECATE | Built entirely on
 * Controller/Zone; an RC Server is a single node with one lifecycle
 * state"). The three states and the two upward-transition admission
 * gates below are this project's own symbolic names for concepts
 * `include/rcp/lifecycle.h` already implements as
 * `rcp_lifecycle_state_t`/`rcp_lifecycle_transition()`; no spec prose or
 * on-wire numeric constant is reproduced here.
 *
 * Safety property (SP1): the server can never reach RcpConfigured
 * directly from HwUnconfigured -- every upward transition passes through
 * HwConfigured.
 * Safety property (SP2): once a field class is locked while the server
 * is RcpConfigured, it never becomes unlocked again while the server
 * remains RcpConfigured (a lock can only be cleared by demoting all the
 * way back to HwUnconfigured -- a full reset).
 *
 * Liveness property (LP1, c-RCP-23d / issue #602): given HW/RCP
 * configuration inputs that eventually settle and stay consistent, the
 * server eventually reaches RcpConfigured under fair scheduling -- i.e.
 * this is not merely a state the server *can* reach (SP1/SP2 already
 * establish what happens if it does), it is a state fair scheduling
 * *guarantees* it reaches. TLC's default no-successor-state deadlock
 * check has already passed on every CI run since this spec was added
 * (predating this issue -- every state in the model has at least one
 * enabled successor). LP1 is a related but strictly stronger claim:
 * progress/livelock-freedom under fairness, not mere deadlock-freedom.
 * See FORMAL_VERIFICATION.md's "Liveness Properties" section for the
 * fairness-minimality experiment (WF vs. SF, both re-run against real
 * TLC) that determined the fairness conditions below are the correct
 * minimum, not merely a sufficient one chosen out of caution.
 *)

(*
 * rust-RCP note (issue #221): this spec is shared with c-RCP -- copied
 * from c-RCP tla/LifecycleStateMachine.tla (c-RCP a4f3a1f) and checked by
 * the same TLC invocation in rust-RCP's .github/workflows/ci.yml
 * `formal-verification` job. The c-RCP names in the comments map to
 * rust-RCP as follows:
 *   rcp_lifecycle_state_t / rcp_lifecycle_transition()
 *       -> crate::lifecycle::RcServerState / RcServerState::try_transition
 *   FUNCTIONAL_W_STAR field lock
 *       -> crate::lifecycle::LockPolicy::WStar
 *          (field_lock = Locked <=> ~is_register_writable(state, HwConfig))
 * src/formal.rs's `lifecycle_refines_tla_spec` test exhaustively explores
 * the real rust transition function and asserts every step it takes is a
 * step of Next below and satisfies SP1/SP2, so this spec cannot silently
 * drift from the code.
 *
 * One addition relative to c-RCP's spec: DemoteRcpToHwConfigured.
 * c-RCP parity: c-RCP src/lifecycle.c:204 implements TC18 Figure 17's
 * RCP_CONFIGURED -> HW_CONFIGURED arrow (c-RCP issue #455), but c-RCP's
 * own LifecycleStateMachine.tla predates it and does not model it. It is
 * modeled here so rust-RCP's port of that arrow (issue #189) stays inside
 * the checked spec. Authorization/idle gates on every demotion are
 * abstracted as nondeterministic enabling, which over-approximates the
 * implementation (sound for the safety properties).
 *)

EXTENDS TLC

HwUnconfigured  == "HwUnconfigured"
HwConfigured    == "HwConfigured"
RcpConfigured   == "RcpConfigured"

LifecycleStates == {HwUnconfigured, HwConfigured, RcpConfigured}

Unlocked == "Unlocked"
Locked   == "Locked"
LockStates == {Unlocked, Locked}

VARIABLES state,              \* current lifecycle state
          hw_cfg_consistent,  \* HW_CFG_INCONSISTENT check's current verdict
          rcp_cfg_consistent, \* RCP_CFG_INCONSISTENT check's current verdict
          field_lock          \* FUNCTIONAL_W_STAR-class field lock state

vars == <<state, hw_cfg_consistent, rcp_cfg_consistent, field_lock>>

TypeOK ==
    /\ state              \in LifecycleStates
    /\ hw_cfg_consistent   \in BOOLEAN
    /\ rcp_cfg_consistent  \in BOOLEAN
    /\ field_lock          \in LockStates

Init ==
    /\ state              = HwUnconfigured
    /\ hw_cfg_consistent   \in BOOLEAN
    /\ rcp_cfg_consistent  \in BOOLEAN
    /\ field_lock          = Unlocked

(* Manifest/HW config may change between transition attempts -- the
 * plausibility checks' verdicts are re-evaluated each time
 * rcp_lifecycle_transition() runs, not cached once and for all. *)
ReviseHwConsistency ==
    /\ hw_cfg_consistent' \in BOOLEAN
    /\ UNCHANGED <<state, rcp_cfg_consistent, field_lock>>

ReviseRcpConsistency ==
    /\ rcp_cfg_consistent' \in BOOLEAN
    /\ UNCHANGED <<state, hw_cfg_consistent, field_lock>>

(* HW_UNCONFIGURED -> HW_CONFIGURED, gated by the HW_CFG_INCONSISTENT
 * check (REQ-LIFECYCLE-008). *)
PromoteToHwConfigured ==
    /\ state = HwUnconfigured
    /\ hw_cfg_consistent
    /\ state' = HwConfigured
    /\ UNCHANGED <<hw_cfg_consistent, rcp_cfg_consistent, field_lock>>

(* HW_CONFIGURED -> RCP_CONFIGURED, gated by the RCP_CFG_INCONSISTENT
 * check (REQ-LIFECYCLE-009). HW_GENERIC fields become read-only and
 * FUNCTIONAL_W_STAR fields become permanently locked for the remainder
 * of this configured session on this same transition
 * (REQ-LIFECYCLE-018, REQ-LIFECYCLE-020). *)
PromoteToRcpConfigured ==
    /\ state = HwConfigured
    /\ rcp_cfg_consistent
    /\ state' = RcpConfigured
    /\ field_lock' = Locked
    /\ UNCHANGED <<hw_cfg_consistent, rcp_cfg_consistent>>

(* HW_CONFIGURED -> HW_UNCONFIGURED demotion is unconditional
 * (REQ-LIFECYCLE-010). *)
DemoteToHwUnconfigured ==
    /\ state = HwConfigured
    /\ state' = HwUnconfigured
    /\ UNCHANGED <<hw_cfg_consistent, rcp_cfg_consistent, field_lock>>

(* RCP_CONFIGURED -> HW_CONFIGURED demotion (rust-RCP addition, see the
 * rust-RCP note above; c-RCP src/lifecycle.c:204). FUNCTIONAL_W_STAR
 * fields are writable again in HW_CONFIGURED (c-RCP
 * src/lifecycle.c:424-427, rust is_register_writable()), so the lock is
 * released. *)
DemoteRcpToHwConfigured ==
    /\ state = RcpConfigured
    /\ state' = HwConfigured
    /\ field_lock' = Unlocked
    /\ UNCHANGED <<hw_cfg_consistent, rcp_cfg_consistent>>

(* RCP_CONFIGURED -> HW_UNCONFIGURED full-reset demotion is unconditional
 * (REQ-LIFECYCLE-011) and is the only way to clear a field lock -- a
 * full reset re-opens hardware configuration from scratch, so whatever
 * FUNCTIONAL_W_STAR fields were locked for the just-ended configured
 * session no longer apply to the next one. This is a modeling
 * assumption, not a literal reading of any single requirement above;
 * see FORMAL_VERIFICATION.md's "Assumptions and Abstractions". *)
FullReset ==
    /\ state = RcpConfigured
    /\ state' = HwUnconfigured
    /\ field_lock' = Unlocked
    /\ UNCHANGED <<hw_cfg_consistent, rcp_cfg_consistent>>

Next ==
    \/ ReviseHwConsistency
    \/ ReviseRcpConsistency
    \/ PromoteToHwConfigured
    \/ PromoteToRcpConfigured
    \/ DemoteToHwUnconfigured
    \/ DemoteRcpToHwConfigured
    \/ FullReset

Spec == Init /\ [][Next]_<<state, hw_cfg_consistent, rcp_cfg_consistent, field_lock>>

(* FairSpec adds the minimum fairness each promote action genuinely
 * needs to guarantee LP1, re-derived and confirmed against real TLC
 * runs rather than assumed (see FORMAL_VERIFICATION.md):
 *
 * - WF_vars(PromoteToHwConfigured): weak fairness suffices. Once
 *   hw_cfg_consistent holds continuously and state = HwUnconfigured
 *   holds continuously, nothing else can disable
 *   PromoteToHwConfigured before it fires -- no other action changes
 *   state away from HwUnconfigured. TLC confirms a WF-only variant
 *   already gets the server past HwUnconfigured on its own.
 *
 * - SF_vars(PromoteToRcpConfigured): strong fairness is required, and
 *   WF is provably insufficient here. DemoteToHwUnconfigured is
 *   unconditionally enabled at state = HwConfigured and can race
 *   PromoteToRcpConfigured back to HwUnconfigured every time before it
 *   fires, so PromoteToRcpConfigured is never *continuously* enabled --
 *   only *infinitely often* enabled -- which WF does not act on but SF
 *   does. TLC confirms a WF-only variant on this action finds a
 *   concrete Promote/Demote lasso counterexample that never leaves
 *   {HwUnconfigured, HwConfigured}.
 *)
FairSpec == Spec
    /\ WF_vars(PromoteToHwConfigured)
    /\ SF_vars(PromoteToRcpConfigured)

(* LP1's antecedent: the HW/RCP config plausibility inputs eventually
 * settle and stay consistent (become permanently TRUE) -- without this,
 * ReviseHwConsistency/ReviseRcpConsistency (deliberately left unfair,
 * matching their role as an unconstrained environment input) could
 * keep an input flapping forever and no fairness on the promote actions
 * could compensate, since neither promote action is ever enabled while
 * its gating input is FALSE. *)
InputsEventuallyConsistent == <>[](hw_cfg_consistent /\ rcp_cfg_consistent)

(* LP1: given eventually-consistent inputs and fair scheduling of the
 * two promote actions (at the minimum fairness level each genuinely
 * needs, above), the server eventually reaches RcpConfigured. *)
EventuallyRcpConfigured == InputsEventuallyConsistent => <>(state = RcpConfigured)

(* SP1: No skip-configuration transition -- RcpConfigured is only ever
 * reached from HwConfigured, never directly from HwUnconfigured. *)
NoSkipConfiguration ==
    [][state = HwUnconfigured => state' # RcpConfigured]_<<state>>

(* SP2: A field lock, once set while RcpConfigured, is never cleared
 * except by the full-reset transition back to HwUnconfigured -- i.e. it
 * never silently reverts to Unlocked while the server remains
 * RcpConfigured. *)
FieldLockMonotonicWhileConfigured ==
    [][ (state = RcpConfigured /\ field_lock = Locked /\ state' = RcpConfigured)
            => field_lock' = Locked ]_<<state, field_lock>>

THEOREM Spec => TypeOK /\ NoSkipConfiguration /\ FieldLockMonotonicWhileConfigured
THEOREM FairSpec => EventuallyRcpConfigured

====
