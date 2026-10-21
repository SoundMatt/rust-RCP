//fusa:req REQ-CMP-001
//fusa:req REQ-CMP-002
//fusa:req REQ-CMP-003
//fusa:req REQ-CMP-004
//fusa:req REQ-CMP-005
//fusa:req REQ-CMP-006
//fusa:req REQ-CMP-007
//fusa:req REQ-TRIG-001
//fusa:req REQ-TRIG-002
//fusa:req REQ-TRIG-003
//fusa:req REQ-TRIG-004
//fusa:req REQ-TRIG-005
//fusa:req REQ-CHAIN-001
//fusa:req REQ-CHAIN-002
//fusa:req REQ-CHAIN-003
//fusa:req REQ-TIME-001
//fusa:req REQ-TIME-002
//fusa:req REQ-TIME-003
//fusa:req REQ-CANCEL-001
//fusa:req REQ-CANCEL-002
//fusa:req REQ-CANCEL-003
//fusa:req REQ-CANCEL-004
//fusa:req REQ-SEQ-001
//fusa:req REQ-SEQ-002
//fusa:req REQ-SEQ-003
//fusa:req REQ-SEQ-004
//fusa:req REQ-PRIO-001
//fusa:req REQ-PRIO-002
//fusa:req REQ-PRIO-003
//fusa:req REQ-PRIO-004
//fusa:req REQ-RLC-001
//fusa:req REQ-RLC-002
//fusa:req REQ-RLC-003
//fusa:req REQ-RLC-004
//fusa:req REQ-RLC-005
//fusa:req REQ-RLC-006
//fusa:req REQ-BUNDLE-001
//fusa:req REQ-BUNDLE-002
//fusa:req REQ-SAFETY-001
//fusa:req REQ-SAFETY-002
//fusa:req REQ-SAFETY-003
//fusa:req REQ-SAFETY-004
//fusa:req REQ-SAFETY-005

//! Conditional-request taxonomy: compound / compound-wait (`0x0F`/`0x0B`),
//! triggered (`0x0E`), chained (`0x01`), timed (`0x0A`), the
//! cancellation trio clear-all / clear-non-safestate / clear-single
//! (`0x05`/`0x06`/`0x07`), the persistent sequencer-state register bank, the
//! execution priority ordering that selects which pending request runs
//! next, the request lifecycle state machine, and the feature-bundle
//! gating rule for honestly claiming "compound request support" —
//! `ROADMAP.md` Milestone 5 ("Conditional Requests & Sequencers"), all nine
//! checklist bullets, closing out the milestone. The first bullet
//! covers sequencer-gated
//! execution and wait, with `cmp_exec_delay`/`cmpw_exec_delay` timers and
//! the "advance sequencer only if still in start state" rule. The second
//! covers trigger-occurrence counting that runs independent of the target
//! endpoint's busy/idle state, the `trigger_exec_delay` timer, and the
//! infinite-repeat sentinel (`0xFFFF`). The third covers the `cs`-bit
//! abort-on-predecessor-error semantics that gate whether a chained
//! request's remaining links continue after an earlier link errors, plus
//! the two new `CHAIN_ABORTED`/`CHAIN_ERROR` error codes that checklist
//! bullet names. The fourth covers presentation-time execution as this
//! checklist bullet's own named alternative to a TSCF header: a request
//! that did not arrive framed with [`crate::avtp::TscfHeader::
//! avtp_timestamp`] (e.g. one carried by NTSCF/ACF_ABB instead, which
//! Milestone 1 modeled as having no timestamp field at all) can still
//! carry its own presentation-time execution gate. The fifth covers the
//! cancellation trio: clear-all (mandatory, cancels every pending/
//! in-flight request), clear-non-safestate (optional, cancels every such
//! request except one actively driving an endpoint toward its configured
//! safe state), and clear-single (optional, cancels exactly one pending
//! request identified by a `clear_transaction_num` matched against the
//! already-decoded [`crate::acf::ByteMessageInfo::transaction_num`]). The
//! sixth covers the persistent 8-bit sequencer-state register bank itself:
//! one [`SequencerState`] per sequencer number, initialized at construction
//! to the power-on default state `1`, live-bounded by a `svr_sequencers_max`
//! value mirroring [`crate::regmap::GeneralRegisters::svr_sequencers_max`],
//! finally giving [`check_compound_gate`] and
//! [`advance_sequencer_if_still_in_start_state`] a genuine backing store to
//! compose against instead of requiring every caller to supply a
//! [`SequencerState`] by hand. The seventh covers execution priority
//! ordering: [`RequestKind::Standard`], the eighth and final variant this
//! item adds so the checklist's own priority list ("cancellation >
//! triggered > timed > compound > compound-wait > chained > standard") has
//! a [`RequestKind`] for every tier it names; [`ExecutionPriorityTier`] and
//! [`execution_priority_tier`], collapsing all eight [`RequestKind`] values
//! down to that same seven-tier ordering; and [`PendingRequestKey`] /
//! [`select_next_pending_request`], a pure selection function choosing
//! which of a caller-supplied set of pending requests should run next —
//! highest tier first, FIFO (earliest arrival) among same-tier entries.
//! The eighth covers the request lifecycle state machine itself:
//! [`RequestLifecycleState`], the four linear states this checklist
//! bullet's own wording names (pending -> started -> under-execution ->
//! finalized), [`is_request_lifecycle_transition_defined`] and
//! [`RequestLifecycleState::try_transition`] (mirroring
//! [`crate::lifecycle::RcServerState`]'s own coarse-shape-check-then-guard
//! shape), and the "type-specific sub-behavior at each transition" this
//! checklist bullet names: [`RequestLifecycleGuardInput`] dispatches each
//! of [`RequestKind`]'s nine values onto the already-built per-kind check
//! that hop composes ([`check_compound_gate`] for Compound/CompoundWait,
//! [`is_timed_request_ready`] for Timed, [`check_chain_continuation`] for
//! Chained, [`TriggeredRuntime::fire_ready`] for Triggered), plus
//! [`try_force_cancel_all`]/[`try_force_cancel_non_safestate`]/
//! [`try_force_cancel_single`], the cancellation trio's own type-specific
//! behavior: forcing a *target* request straight to
//! [`RequestLifecycleState::Finalized`] rather than progressing it through
//! the normal linear hops. See "Provenance note: which existing check
//! applies at which lifecycle hop" below for this item's own working
//! mapping from checklist wording to source, since §3.14 is cited by
//! section number only per this crate's spec-citation policy.
//!
//! The ninth and final bullet, closing out the milestone, is
//! feature-bundle gating: [`check_compound_bundle_claim`], the rule that
//! honestly claiming the "compound request support" optional-feature
//! bundle — the bit [`crate::regmap::GeneralRegisters::
//! claims_compound_wait_bundle`] reads — requires *all three* of this
//! milestone's own prior deliverables together (compound-wait support,
//! a sequencer bank sized for at least four sequencers, and
//! clear-non-safestate cancellation support), not any one of them alone
//! and not compound-message parsing by itself. This is deliberately a
//! composing, not a discovering, item: every fact it consults
//! ([`RequestKind::CompoundWait`], [`SequencerBank::svr_sequencers_max`],
//! [`check_clear_non_safestate_cancellation`]) already exists from the
//! milestone's first eight bullets; this item's own job is only the
//! gating rule that ties the three together into one honesty check. See
//! "Provenance note: the compound-bundle gate as three caller-supplied
//! facts, not a read `GeneralRegisters`" below.
//!
//! Compound/compound-wait was the opening item of Milestone 5, and the
//! first thing to land in `src/request.rs` — the module name the
//! naming-reconciliation pass (issue #35, PR #37, "refactor: reconcile
//! module naming with RELAY spec v1.14 §13.7.2") reserved for this
//! milestone's request-kind/taxonomy work — "v1.14" names the RELAY spec
//! revision current when PR #37 landed, not the revision this crate
//! declares today (see [`crate::SPEC_VERSION`] for the current RELAY spec
//! version this crate implements) — mirroring `fragment.rs`'s own
//! reservation for Milestone 8. Triggered is the second, added there.
//! Chained is the third, added there. Timed is the fourth, added there.
//! Cancellation is the fifth, added there. [`SequencerBank`] is the sixth,
//! added in the prior entry this one extends. Execution priority ordering
//! is the seventh, added in the prior entry this one extends. The request
//! lifecycle state machine is the eighth, added in the prior entry this
//! one extends. Feature-bundle gating is the ninth and last, added here.
//! Same "additive standalone plumbing only" discipline as every prior
//! Milestone 1-5 entry, and as the compound/compound-wait, triggered,
//! chained, timed, cancellation, sequencer-bank, execution-priority, and
//! lifecycle-state-machine work above: [`check_compound_bundle_claim`] is
//! a pure function over caller-supplied inputs — nothing here is wired
//! into a decoder, dispatch loop, or any not-yet-built "read a live
//! `GeneralRegisters` and validate its own claimed
//! `svr_implemented_options`" caller. [`RequestLifecycleState::
//! try_transition`] is likewise still a pure, self-consuming function
//! over caller-supplied inputs, and [`select_next_pending_request`] is
//! still not called from anywhere in this crate, including from
//! [`RequestLifecycleState::try_transition`] (see this module's doc
//! comment "Deliberately out of scope" section below for why picking
//! *which* pending request goes next and advancing *that* request's own
//! lifecycle state stay two separate, uncomposed pieces for now). The old
//! `src/prioqueue.rs` `Zone`/`Command`/`Controller`/`Priority` decorator
//! this milestone's own Goal text names as the eventual absorption target
//! for "picking which pending request runs next" was, at the time this
//! item landed, read only as background for this change — `prioqueue.rs`
//! itself was left untouched here (see this module's doc comment
//! "Deliberately out of scope" section below) and was later deleted
//! outright by Milestone 9's DEPRECATE disposition; it no longer exists
//! in `src/`.
//!
//! Twenty-five named pieces are in scope, all implemented here or in the
//! seven prior entries this one extends:
//!
//! - [`RequestKind`] — the request-type discriminant, now covering nine
//!   values ([`RequestKind::ClearAll`] = `0x05`,
//!   [`RequestKind::ClearNonSafestate`] = `0x06`,
//!   [`RequestKind::ClearSingle`] = `0x07`, [`RequestKind::Chained`] =
//!   `0x01`, [`RequestKind::Timed`] = `0x0A`, [`RequestKind::CompoundWait`]
//!   = `0x0B`, [`RequestKind::Triggered`] = `0x0E`,
//!   [`RequestKind::Compound`] = `0x0F`, and — new in this item —
//!   [`RequestKind::Standard`]). See "Provenance note:
//!   `RequestKind`'s wire placement" below for [`RequestKind::
//!   from_gbb_message_timestamp`]/[`RequestKind::to_gbb_message_timestamp`],
//!   the pair of functions binding this type to the leading byte of an
//!   already-decoded [`crate::acf::AcfGbbMessage::message_timestamp`] for
//!   GBB conditional requests — [`RequestKind`]'s first tie to an actual
//!   decoded wire shape — and "Provenance note: `RequestKind::Standard`'s
//!   discriminant" below for why [`RequestKind::Standard`]'s own numeric
//!   value carries even less confidence than the other eight, and is never
//!   produced by that decode-side helper.
//! - [`CompoundGateConfig`] / [`SequencerState`] /
//!   [`check_sequencer_num_in_bounds`] / [`is_gate_satisfied`] /
//!   [`check_compound_gate`] — the sequencer-gating rule: a compound(-wait)
//!   request executes only if the sequencer it names currently holds the
//!   request's configured start state. See "Provenance note: `start_state`
//!   and the sequencer-state machine" below for how this relates to
//!   [`crate::regmap::SequencerStateEntry`] and [`SequencerBank`].
//! - [`SequencerBank`] — the persistent 8-bit sequencer-state register bank
//!   itself: [`SequencerBank::new`] sizes and initializes one
//!   [`SequencerState`] per sequencer number, bounded by a caller-supplied
//!   `svr_sequencers_max`, to the power-on default state `1`
//!   ([`crate::regmap::SequencerStateEntry::power_on_default`]'s own
//!   already-confirmed value); [`SequencerBank::read`] reads a sequencer's
//!   current state; [`SequencerBank::advance_if_still_in_start_state`]
//!   composes [`advance_sequencer_if_still_in_start_state`]'s existing pure
//!   race-guard against this bank's live, mutable store; and
//!   [`SequencerBank::check_compound_gate`] composes [`SequencerBank::read`]
//!   with the free-function [`check_compound_gate`], giving it a genuine
//!   backing store to read `current_state` from instead of requiring the
//!   caller to supply one. See "Provenance note: `SequencerBank`'s
//!   reset-trigger scope" below for what "power-on" is read to mean here.
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
//! - [`should_count_trigger_occurrence`] — c-RCP's occurrence-matching
//!   rule (`c-RCP/include/rcp/request.h:291-303`): an occurrence counts
//!   only when its `(source_ep, signal_nr)` pair matches the request's own
//!   [`TriggeredStep::trigger_source_ep`]/[`TriggeredStep::trigger_signal_nr`],
//!   independent of the target endpoint's busy/idle state. See
//!   "Provenance note: busy/idle independence" below.
//! - [`check_chain_continuation`] — the `cs`-bit abort-on-predecessor-error
//!   rule: whether a chained request's next link runs, given the already
//!   decoded [`crate::acf::ByteMessageInfo::cs`] flag for that link and
//!   whether the chain's preceding link errored. See "Provenance note: the
//!   `cs` bit's chained-request meaning" below.
//! - [`crate::RcpError::ChainAborted`] / [`crate::RcpError::ChainError`] —
//!   the two new `CHAIN_ABORTED`/`CHAIN_ERROR` error codes this checklist
//!   bullet names, added to [`crate::RcpError`] in `src/lib.rs`. See
//!   "Provenance note: `CHAIN_ABORTED`/`CHAIN_ERROR` as new variants, and
//!   the distinction between them" below.
//! - [`TimedExecutionTime`] — the presentation-time execution gate a Timed
//!   request carries in place of a TSCF header's own
//!   [`crate::avtp::TscfHeader::avtp_timestamp`], modeled by composing
//!   [`crate::timestamp::AvtpTimestamp`] rather than duplicating its
//!   shape. See "Provenance note: `TimedExecutionTime`'s wire placement,
//!   width, and the choice to compose `AvtpTimestamp`" below.
//! - [`is_timed_request_ready`] — the presentation-time-execution
//!   readiness rule this checklist bullet's own wording implies: a Timed
//!   request is ready to execute once a caller-supplied current
//!   presentation time has reached or passed the request's own carried
//!   [`TimedExecutionTime`], reusing
//!   [`crate::timestamp::AvtpTimestamp::is_after`]'s existing
//!   wraparound-aware ordering and
//!   [`crate::timestamp::AvtpTimestamp::is_untimed`]'s existing
//!   all-zero-means-untimed fallback rather than inventing new ordering or
//!   fallback logic of its own.
//! - [`check_clear_all_cancellation`] — the clear-all (`0x05`, mandatory)
//!   cancellation rule: every pending/in-flight request is canceled,
//!   unconditionally. See "Provenance note: cancellation scope and the
//!   addressed-endpoint/stream ambiguity" below for what "every" is scoped
//!   to.
//! - [`check_clear_non_safestate_cancellation`] — the clear-non-safestate
//!   (`0x06`, optional) cancellation rule: a request is canceled unless it
//!   is actively driving an endpoint toward its configured safe state. See
//!   "Provenance note: the safe-state-driving predicate as a
//!   caller-supplied parameter" below for why that determination is a
//!   caller-supplied `bool` rather than something this module computes
//!   itself.
//! - [`ClearTransactionNum`] / [`check_clear_single_cancellation`] — the
//!   clear-single (`0x07`, optional) cancellation rule: exactly one pending
//!   request, identified by a `clear_transaction_num` matched against a
//!   caller-supplied candidate transaction number, is canceled. See
//!   "Provenance note: `clear_transaction_num`'s width and matching field"
//!   below for why this matches against
//!   [`crate::acf::ByteMessageInfo::transaction_num`].
//! - [`crate::RcpError::RequestCanceled`] — the outcome signal all three
//!   `check_clear_*_cancellation` functions construct for a request they
//!   select for cancellation; the first construction site for this
//!   Milestone-2-reserved sentinel (see "Provenance note:
//!   `RequestCanceled` as this item's outcome signal" below).
//! - [`ExecutionPriorityTier`] / [`execution_priority_tier`] — the seven
//!   named execution-priority tiers this checklist bullet's own ordering
//!   names ("cancellation > triggered > timed > compound > compound-wait >
//!   chained > standard"), and the mapping from each of [`RequestKind`]'s
//!   nine values down to one of them, with the three cancellation variants
//!   collapsing onto a single [`ExecutionPriorityTier::Cancellation`] tier.
//!   See "Provenance note: `RequestKind::Standard`'s discriminant" below for
//!   [`RequestKind::Standard`] itself, which this tier mapping is the first
//!   thing to actually use.
//! - [`PendingRequestKey`] / [`select_next_pending_request`] — the pure
//!   selection function this checklist bullet's "FIFO within a tier" rule
//!   implies: given a caller-supplied set of pending requests, each tagged
//!   with a [`RequestKind`] and a monotonic arrival marker, which one
//!   should run next. See "Provenance note: arrival order as a
//!   caller-supplied sequence number" below for why arrival order is
//!   modeled this way rather than as an owned queue, and "Provenance note:
//!   what execution priority ordering does not decide" below for the two
//!   points this checklist bullet leaves unstated that this item does not
//!   silently resolve.
//! - [`RequestLifecycleState`] / [`is_request_lifecycle_transition_defined`]
//!   — the four-state lifecycle this checklist bullet names (pending ->
//!   started -> under-execution -> finalized) and the coarse, state-shape
//!   check for which of the twelve possible `(from, to)` pairs among those
//!   four states are actually defined: the three linear forward hops, and
//!   no others — no skip, backward, or identity transition, mirroring
//!   [`crate::lifecycle::is_transition_defined`]'s own discipline.
//! - [`RequestLifecycleGuardInput`] / [`RequestLifecycleState::try_transition`]
//!   — the "type-specific sub-behavior at each transition" this checklist
//!   bullet names: one [`RequestLifecycleGuardInput`] variant per
//!   [`RequestKind`], each carrying exactly the fields the already-built
//!   per-kind check that hop composes needs, and
//!   [`RequestLifecycleState::try_transition`], the self-consuming,
//!   never-panicking `Result`-returning method that rejects any
//!   `(self, target)` pair [`is_request_lifecycle_transition_defined`]
//!   does not name, and otherwise applies that hop's composed guard. See
//!   "Provenance note: which existing check applies at which lifecycle
//!   hop" below for this item's own working mapping.
//! - [`try_force_cancel_all`] / [`try_force_cancel_non_safestate`] /
//!   [`try_force_cancel_single`] — the cancellation trio's own
//!   type-specific lifecycle behavior: each composes the matching
//!   `check_clear_*_cancellation` function against a *target* request's
//!   current [`RequestLifecycleState`], forcing it straight to
//!   [`RequestLifecycleState::Finalized`] when that check selects it for
//!   cancellation rather than progressing it through the normal linear
//!   hops — the "short-circuit" behavior distinguishing the cancellation
//!   trio's own lifecycle role (acting on another pending request) from
//!   every other [`RequestKind`]'s role (gating its own progression). All
//!   three are idempotent against an already-[`RequestLifecycleState::Finalized`]
//!   target.
//! - [`check_compound_bundle_claim`] — the feature-bundle gating rule this
//!   checklist bullet names: claiming the "compound request support"
//!   bundle is honest only when compound-wait support, a sequencer bank
//!   sized for at least [`MIN_SEQUENCERS_FOR_COMPOUND_BUNDLE`] sequencers,
//!   and clear-non-safestate cancellation support are all true together.
//!   See "Provenance note: the compound-bundle gate as three
//!   caller-supplied facts, not a read `GeneralRegisters`" below for why
//!   this takes its three prerequisite facts as plain caller-supplied
//!   values instead of reading them off a live
//!   [`crate::regmap::GeneralRegisters`], and "Provenance note:
//!   `InvalidParameter` as the compound-bundle gate's rejection code"
//!   below for the error-code choice.
//!
//! Deliberately out of scope:
//!
//! - The persistent 8-bit sequencer-state register machine itself
//!   (`ROADMAP.md` Milestone 5's own "Sequencers" checklist bullet) is now
//!   built as [`SequencerBank`] — but every free function that needs a
//!   sequencer's current state ([`is_gate_satisfied`],
//!   [`check_compound_gate`], [`advance_sequencer_if_still_in_start_state`])
//!   keeps taking it as a caller-supplied [`SequencerState`] value rather
//!   than being retrofitted to require a [`SequencerBank`], mirroring
//!   [`crate::lifecycle::RcServerState::try_transition`]'s `is_consistent`
//!   closure and [`crate::ep0::check_ep0_access_for_stream`]'s
//!   `root_client` parameter — neither of those blocked on a sibling item
//!   building the thing they read, and neither did these when they were
//!   added. [`SequencerBank::check_compound_gate`] is the new bank-backed
//!   alternative that composes the free functions instead of replacing
//!   them. Triggered execution's own busy/idle independence needs no such
//!   state at all; see [`should_count_trigger_occurrence`] above (an
//!   occurrence's own `(source_ep, signal_nr)` match, c-RCP parity).
//! - Wiring [`SequencerBank`], [`select_next_pending_request`],
//!   [`RequestLifecycleState::try_transition`], or any of the below into an
//!   actual decoder or dispatch loop. [`select_next_pending_request`]
//!   decides *which* pending request goes next given a caller-supplied
//!   set; [`RequestLifecycleState::try_transition`] advances *one already
//!   identified* request's own lifecycle state; nothing here owns a real
//!   pending-request store, calls [`select_next_pending_request`] and then
//!   feeds its result into [`RequestLifecycleState::try_transition`], or
//!   calls either from anywhere else in this crate.
//! - A unified pending-request record type that owns both a
//!   [`RequestKind`] and a live [`RequestLifecycleState`] together (so that,
//!   for instance, [`select_next_pending_request`] could filter to only
//!   `Pending`-state entries before choosing among them). Each of this
//!   item's functions takes its [`RequestLifecycleState`] and
//!   [`RequestLifecycleGuardInput`]/kind arguments as separate
//!   caller-supplied values instead, mirroring every prior entry's
//!   "caller-supplied state, no owned store" precedent
//!   ([`SequencerBank`] being the one deliberate exception, and even it
//!   stores only [`SequencerState`], not a full pending-request record).
//! - The old `src/prioqueue.rs` `Zone`/`Command`/`Controller`/`Priority`
//!   model this milestone's own Goal text names as the eventual absorption
//!   target for "picking which pending request runs next". This item is
//!   the "Execution priority ordering" checklist bullet the Goal text
//!   points at, and [`select_next_pending_request`] is its own
//!   from-scratch, spec-native implementation of that job — but, at the
//!   time this item landed, `src/prioqueue.rs` itself was only read as
//!   background, not extended, modified, or migrated onto
//!   [`select_next_pending_request`]; that KEEP/DEPRECATE-style migration
//!   was `ROADMAP.md`'s own Milestone 9 satellite-package-migration job
//!   (`prioqueue` was DEPRECATE-dispositioned in that milestone's
//!   satellite table and has since been deleted outright — it no longer
//!   exists in `src/`), not this item's.
//! - Any error/rejection behavior for a pending request that never gets a
//!   turn (queue overflow, starvation) and any statement of what scope
//!   execution priority is evaluated across (per-endpoint, per-stream, or
//!   server-wide). See "Provenance note: what execution priority ordering
//!   does not decide" below.
//! - Reading a live [`crate::regmap::GeneralRegisters`] value and deciding,
//!   on this crate's own initiative, whether its own
//!   `svr_implemented_options` claim is honest. This crate has no
//!   not-yet-built "RC Server instance holding its own live register
//!   state" concept yet — the same gap every prior Milestone 5 entry
//!   already works around by taking its own facts as caller-supplied
//!   parameters rather than reading them from one. See "Provenance note:
//!   the compound-bundle gate as three caller-supplied facts, not a read
//!   `GeneralRegisters`" below.
//! - Gating any of the other four `svr_implemented_options` bundles
//!   (triggered / chained / time-sync&timed / enhanced-cancel) this
//!   checklist bullet does not name. `ROADMAP.md`'s "Feature-bundle
//!   gating" bullet states a rule only for the "compound request support"
//!   bundle; [`check_compound_bundle_claim`] is scoped to that one bundle
//!   only, and does not generalize to (or take a position on) what an
//!   honest claim for any of the other four would require.
//!
//! ## Provenance note: `RequestLifecycleState` carries no numeric encoding
//!
//! Unlike [`crate::lifecycle::RcServerState`] (`0x00`/`0x55`/`0xAA`) or
//! [`RequestKind`] (`0x00`..=`0x0F`), `ROADMAP.md`'s checklist bullet names
//! no wire byte, register value, or other numeric encoding for any of the
//! four [`RequestLifecycleState`] values — "pending", "started",
//! "under-execution", and "finalized" read as this crate's own internal
//! bookkeeping states for a request already admitted to this RC Server,
//! not as a value transmitted on the wire or exposed through a register
//! this crate has modeled so far. Per Guiding Principle 5, rather than
//! mint an unconfirmed placeholder discriminant the way
//! [`RequestKind::Standard`]'s own `0x00` was minted (see "Provenance
//! note: `RequestKind::Standard`'s discriminant" below), this item leaves
//! [`RequestLifecycleState`] as a plain enum with no `#[repr(u8)]` and no
//! `to_u8`/`from_u8` pair at all, flagging the absence rather than
//! guessing at a number nothing in this crate's roadmap text supports.
//!
//! ## Provenance note: which existing check applies at which lifecycle hop
//!
//! §3.14 is cited by section number only, per this crate's policy of never
//! transcribing the confidential OPEN Alliance TC18 specification's own
//! text (see `ROADMAP.md`'s Guiding Principle 4) — so exactly which
//! per-kind rule gates which of the three linear hops
//! ([`RequestLifecycleState::Pending`] -> `Started`, `Started` ->
//! `UnderExecution`, `UnderExecution` -> `Finalized`) is this item's own
//! working interpretation, not a transcription of confirmed spec
//! structure, and is flagged here per Guiding Principle 5 rather than
//! silently asserted as spec fact:
//!
//! - `Pending` -> `Started` (this item reads as: "is this request now
//!   eligible to be the one that runs") is where [`check_compound_gate`]
//!   (Compound/CompoundWait) and [`is_timed_request_ready`] (Timed) are
//!   composed — both are pre-execution eligibility gates in their own
//!   existing doc comments, which is why this item places them at the
//!   *first* hop rather than the second.
//! - `Started` -> `UnderExecution` (read as: "does this request's own
//!   execution actually proceed now that it has been selected") is where
//!   [`check_chain_continuation`] (Chained — whether *this* link runs,
//!   given the preceding link's own outcome) and
//!   [`TriggeredRuntime::fire_ready`] (Triggered — c-RCP's
//!   `rcp_triggered_tick()` fire predicate: started, threshold reached,
//!   `trigger_exec_delay` elapsed, endpoint idle) is composed.
//! - `UnderExecution` -> `Finalized` is unconditional for every
//!   [`RequestKind`] — no checklist wording anywhere in this crate's
//!   roadmap text names a rule for whether a request that has already
//!   begun executing is allowed to *finish*, distinct from whether it was
//!   allowed to *start*. [`RequestKind::Standard`] and the cancellation
//!   trio ([`RequestKind::ClearAll`]/[`RequestKind::ClearNonSafestate`]/
//!   [`RequestKind::ClearSingle`]) are unconditional at every hop — the
//!   cancellation trio's actual type-specific lifecycle behavior is
//!   [`try_force_cancel_all`]/[`try_force_cancel_non_safestate`]/
//!   [`try_force_cancel_single`] instead (see this module's doc comment's
//!   "in scope" list above), not a gate on their own linear progression.
//!
//! ## Provenance note: `RequestKind`'s wire placement
//!
//! `ROADMAP.md`'s checklist bullets name `0x0F`/`0x0B`/`0x0E`/`0x01`/`0x0A`
//! and, for this item, `0x05`/`0x06`/`0x07` as the compound, compound-wait,
//! triggered, chained, timed, clear-all, clear-non-safestate, and
//! clear-single discriminant values, but — unlike `acf_msg_type`
//! ([`crate::acf::ACF_ABB_MSG_TYPE`]/[`crate::acf::ACF_GBB_MSG_TYPE`]),
//! whose byte offset within an ACF message header this crate already
//! pinned down in Milestone 1 — no checklist text in `ROADMAP.md` itself
//! ever stated which byte or field of a request actually carries this
//! discriminant. Per Guiding Principle 5, [`RequestKind`] was therefore
//! modeled, from its introduction through the eight base variants and the
//! three MSB-tagged safety variants added later, as a standalone value type
//! with its own `to_u8`/`from_u8` pair, exactly as confident about its named
//! numeric values as `ROADMAP.md`'s checklist text was, and no more: not
//! attached to any offset within [`crate::acf::ByteMessageInfo`] or any
//! other already-built wire shape, with no such offset guessed.
//!
//! This crate's 2026-07-29 ecosystem audit (issue #101) located that missing
//! offset: for a GBB conditional request, the discriminant this module
//! already names as [`RequestKind`] is carried in the leading (most
//! significant) byte of the already-decoded
//! [`crate::acf::AcfGbbMessage::message_timestamp`] field, not in a
//! standalone `request_type` register or field of its own — citing the
//! specification by section number only (§11.2.2), per Guiding Principle 4.
//! Unlike the `0x0F`/`0x0B`/etc. discriminant values themselves, which
//! `ROADMAP.md` names directly, this byte-offset finding is not yet
//! reflected in `ROADMAP.md`'s own checklist text; it is recorded here, in
//! this module's own provenance note, as this crate's working
//! interpretation of that audit finding, flagged per Guiding Principle 5 for
//! reconciliation against real TC18 behavior before being relied on for
//! interop. [`RequestKind::from_gbb_message_timestamp`]/[`RequestKind::
//! to_gbb_message_timestamp`] compose this offset against
//! [`crate::acf::encode_acf_gbb`]'s already-established big-endian
//! `to_be_bytes()` layout of `message_timestamp`
//! (`(message_timestamp >> 56) as u8` is the leading byte on the wire), so
//! the two stay internally consistent with how this crate already encodes
//! that field, whatever the real TC18 byte order for `message_timestamp`
//! itself eventually turns out to be. No offset is claimed for ACF_ABB (which
//! [`crate::acf::AcfAbbMessage`] models as having no `message_timestamp`
//! region at all — see `crate::acf`'s own module doc comment) or for any
//! other already-built wire shape; this finding is scoped to GBB conditional
//! requests specifically, the one case issue #101 confirmed. See
//! "Provenance note: `RequestKind::Standard`'s discriminant" below for why
//! [`RequestKind::Standard`] is deliberately excluded from this binding
//! rather than assumed to occupy the same leading byte when it reads as
//! `0x00`.
//!
//! ## Provenance note: `RequestKind::Standard`'s discriminant
//!
//! `ROADMAP.md`'s "Execution priority ordering" checklist bullet names
//! "standard" as the lowest-priority tier in its ordering, alongside the
//! other six tiers this module already models — but, unlike every other
//! [`RequestKind`] variant, no `ROADMAP.md` checklist text anywhere in this
//! crate's roadmap gives "standard" a numeric discriminant byte at all. The
//! other eight base variants at least inherit a roadmap-named hex value each
//! (`0x01`/`0x05`/`0x06`/`0x07`/`0x0A`/`0x0B`/`0x0E`/`0x0F`); [`RequestKind::
//! Standard`] has neither. Per Guiding Principle 5, this crate does not
//! invent a plausible-looking spec value for it. The discriminant assigned
//! here, `0x00`, is a crate-local placeholder chosen only for two structural
//! reasons — it is the one byte value none of the other named discriminants
//! occupies, and `#[repr(u8)]` requires every variant to have some concrete
//! value for [`RequestKind::to_u8`]'s `self as u8` cast to compile — not a
//! transcription of, or a guess at, any confirmed TC18 wire encoding.
//!
//! Issue #101's finding (see the provenance note above) sharpens this rather
//! than resolving it: it confirms that GBB conditional requests carry their
//! [`RequestKind`] in `message_timestamp`'s leading byte, but says nothing
//! about what a *standard* (unconditional) GBB request carries there instead
//! — plausibly nothing at all, if standard requests do not reuse this byte
//! position for any discriminant purpose, the same way [`crate::acf::
//! AcfAbbMessage`] carries no `message_timestamp` region whatsoever. Per
//! Guiding Principle 5, this crate does not resolve that either way:
//! [`RequestKind::from_gbb_message_timestamp`] never returns
//! [`RequestKind::Standard`], for any input including a leading byte of
//! `0x00` — a genuine standard request's `message_timestamp` reading `0x00`
//! in its leading byte is indistinguishable, from this byte alone, from a
//! conditional request whose timestamp coincidentally has a zero leading
//! byte, so neither is asserted; the function returns `None` for both
//! rather than picking one. [`RequestKind::to_gbb_message_timestamp`]
//! likewise refuses to encode [`RequestKind::Standard`], returning
//! `Err(RcpError::InvalidParameter)` rather than writing `0x00` into a real
//! `message_timestamp` value as though that meant something confirmed on
//! the wire. Should a future item learn the real "standard" discriminant (or
//! learn that "standard"/unconditional requests carry no discriminant byte
//! at all in this or any position), this placeholder value — and these two
//! functions' treatment of it — is expected to change; nothing in this
//! crate depends on `0x00` specifically
//! beyond this enum's own internal round-trip.
//!
//! ## Provenance note: `start_state` and the sequencer-state machine
//!
//! The gating rule this checklist bullet names — "sequencer-gated
//! execution" — requires comparing a compound(-wait) request's configured
//! start state against a sequencer's actual current state. When this note
//! was first written, the persistent state register that would hold that
//! "current state" was `ROADMAP.md` Milestone 5's own next checklist bullet
//! ("Sequencers"), not yet built in this crate: only
//! [`crate::regmap::SequencerStateEntry`]'s row *shape* (power-on default
//! `1`, single-byte encoding) existed, from Milestone 2's config-table work.
//! [`SequencerBank`] is that checklist bullet, now built — see "Provenance
//! note: `SequencerBank`'s reset-trigger scope" below — but the reasoning
//! that follows, about [`SequencerState`] itself and about the free
//! functions ([`is_gate_satisfied`], [`check_compound_gate`],
//! [`advance_sequencer_if_still_in_start_state`]) taking a current-state
//! value as a caller-supplied parameter rather than an implicit global, is
//! unchanged: this module assumes a sequencer's current state is
//! representable as the same single unstructured byte
//! [`crate::regmap::SequencerStateEntry::seq_state`] already models, wrapped
//! here as [`SequencerState`]. Which specific sequencer a request names is
//! likewise modeled as a plain `u8` sequencer number
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
//! purpose). So [`advance_sequencer_if_still_in_start_state`] — and, by
//! composition, [`SequencerBank::advance_if_still_in_start_state`] — takes
//! the target state as an explicit caller-supplied parameter too, rather
//! than this crate inventing an increment-by-one or other advancement
//! convention.
//!
//! ## Provenance note: `SequencerBank`'s reset-trigger scope
//!
//! `ROADMAP.md`'s checklist bullet names one reset trigger for the
//! power-on default state `1`: "power-on", full stop. It does not state
//! whether that default also applies on some other reset condition this
//! crate has not yet modeled (e.g. whatever `RcServerState` transition
//! [`crate::lifecycle`] eventually names as a warm reset, if any turns out
//! to be distinct from power-on) or whether a live [`SequencerBank`] is
//! ever expected to be reset back to all-defaults after having already
//! advanced some of its sequencers. Per Guiding Principle 5, [`SequencerBank`]
//! is deliberately narrow about this: [`SequencerBank::new`] is the *only*
//! way to obtain an all-defaults bank, modeling "power-on" as "the moment a
//! [`SequencerBank`] is constructed" and nothing broader — there is no
//! separate `reset`/`power_on_reset` method that reinitializes an
//! already-live bank in place, since this crate has no confirmed trigger
//! condition to name such a method after. A caller that needs to model a
//! reset distinct from initial construction can simply construct a fresh
//! [`SequencerBank`] and discard the old one; nothing here prevents that,
//! and nothing here assumes it either.
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
//! ## Provenance note: busy/idle independence
//!
//! Earlier revisions of this module modelled the "counts independent of
//! the target endpoint's busy/idle state" rule as an always-`true`
//! predicate over a deliberately-ignored `endpoint_busy` flag. It now
//! mirrors c-RCP (`c-RCP/src/request.c:515-525`,
//! `rcp_triggered_runtime_record_occurrence()`): occurrence counting takes
//! the observed `(source_ep, signal_nr)` pair and counts it only when it
//! matches the request's own [`TriggeredStep`] selection, with no busy/idle
//! input at all — the independence rule is now structural (the counter has
//! no such parameter). Only the fire transition
//! ([`TriggeredRuntime::tick`]/[`TriggeredRuntime::fire_ready`]) is gated
//! on `endpoint_idle`, exactly as c-RCP's `rcp_triggered_tick()` is.
//!
//! [`crate::gpio::GpioTriggerConfig`]/[`crate::gpio::GpioTriggerSignals`]/
//! [`crate::gpio::evaluate_gpio_triggers`] are a separate, narrower,
//! already-built concept — per-pin change/rising/falling edge-detection
//! arming for the GPIO endpoint type specifically — and are unrelated to
//! this request-kind-level Triggered (`0x0E`) mechanism; nothing here
//! reuses or extends them.
//!
//! ## Provenance note: the `cs` bit's chained-request meaning
//!
//! [`crate::acf::ByteMessageInfo::cs`] was decoded in Milestone 1 as a
//! standalone header flag; that module's own doc comment names it only as
//! one of the shared ACF header's flag bits, with no consumer anywhere in
//! this crate reading it for any semantic purpose. This checklist bullet is
//! the first to give `cs` a stated meaning — gating whether a chained
//! request's remaining links keep running after an earlier link errors —
//! but names that meaning only for a request carrying
//! [`RequestKind::Chained`]; nothing in `ROADMAP.md` states whether `cs`
//! means anything at all for the other three [`RequestKind`] values, or for
//! requests generally. [`check_chain_continuation`] therefore takes `cs` as
//! a plain caller-supplied `bool` — this module does not assume the caller
//! has already confirmed the request it came from is
//! [`RequestKind::Chained`], and does not itself branch on
//! [`RequestKind`] at all, mirroring [`is_gate_satisfied`]'s own
//! kind-agnostic, caller-supplied-context style. Whether `cs` retains its
//! Milestone-1 "checksum-present" reading simultaneously, or whether that
//! reading was itself only ever this crate's own placeholder guess pending
//! this exact item, is left unresolved here per Guiding Principle 5 — no
//! `crate::acf` code is changed by this item.
//!
//! ## Provenance note: `CHAIN_ABORTED`/`CHAIN_ERROR` as new variants, and the
//! distinction between them
//!
//! `ROADMAP.md` Milestone 2's own "Error Model" item closed out
//! [`crate::RcpError`]'s eleven spec-named codes
//! ([`crate::RcpError::UnsupportedCmd`] through
//! [`crate::RcpError::InvalidParameter`]) and explicitly deferred "the
//! timing- and CRC-specific codes wired in by later milestones" — see that
//! enum's own doc comment. `CHAIN_ABORTED`/`CHAIN_ERROR` are the first such
//! later-milestone codes to actually land. Per Guiding Principle 5, this
//! item checked whether either name plausibly collapses onto one of the
//! eleven already-defined variants the way UART's `UNKNOWN_CMD` collapsed
//! onto [`crate::RcpError::UnsupportedCmd`] (Milestone 4) or the way
//! Milestone 2's own mapping table folded several provisional sentinels
//! onto spec-named codes: [`crate::RcpError::RequestRejected`] is the
//! closest existing candidate (both describe a request that does not
//! execute), but the eleven-name list's own `REQUEST_REJECTED` reading —
//! rejected outright, before any execution begins — does not capture
//! `CHAIN_ABORTED`'s more specific "this link was skipped mid-chain because
//! an earlier link in the same chain errored" shape, nor `CHAIN_ERROR`'s
//! "this link ran and failed" shape. Neither collapses cleanly, so both are
//! added as two new [`crate::RcpError`] variants rather than folded onto an
//! existing one.
//!
//! `ROADMAP.md`'s checklist text names both codes side by side but does not
//! spell out what distinguishes them. This crate's working interpretation,
//! flagged here rather than silently assumed: [`crate::RcpError::ChainError`]
//! is read as "this chain link's own execution failed", the per-link
//! failure outcome [`check_chain_continuation`]'s `predecessor_errored`
//! parameter is fed for the *next* link's check; [`crate::RcpError::ChainAborted`]
//! is read as "this chain link did not run at all, because
//! [`check_chain_continuation`]'s `cs`-bit rule aborted it on account of an
//! earlier link's [`crate::RcpError::ChainError`]". Under that reading,
//! [`check_chain_continuation`] — the only chain-related function this item
//! builds — can only ever construct [`crate::RcpError::ChainAborted`];
//! [`crate::RcpError::ChainError`] is added for naming completeness per the
//! checklist's own pairing of the two codes, but is not constructed
//! anywhere in this crate yet, mirroring Milestone 2's own precedent of
//! reserving [`crate::RcpError::SequencerNotKnown`],
//! [`crate::RcpError::RequestCanceled`], [`crate::RcpError::RequestNotFound`],
//! [`crate::RcpError::EpNotFound`], and [`crate::RcpError::ReqStorageOvfl`]
//! ahead of the concrete per-link execution path (a later milestone item)
//! that would actually return it.
//!
//! ## Provenance note: `TimedExecutionTime`'s wire placement, width, and the
//! choice to compose `AvtpTimestamp`
//!
//! `ROADMAP.md`'s checklist bullet names Timed's mechanism only in prose —
//! "presentation-time execution as an alternative to a TSCF header" — and,
//! like every other conditional-request kind in this module, states
//! neither the byte offset a Timed request's own execution-time field
//! occupies nor its field name. What the wording does pin down is the
//! *shape* of the alternative being offered: the one presentation-time
//! field this crate already carries is
//! [`crate::avtp::TscfHeader::avtp_timestamp`], modeled in Milestone 1 as
//! [`crate::timestamp::AvtpTimestamp`] — a 32-bit value with its own
//! rollover-aware comparison and all-zero-means-untimed fallback rule.
//! Reading "alternative to a TSCF header" as "carries the same kind of
//! presentation-time value a TSCF header would have supplied, just sourced
//! from the request itself instead", [`TimedExecutionTime`] composes
//! [`crate::timestamp::AvtpTimestamp`] directly rather than introducing a
//! second, differently-shaped timestamp type or an unconfirmed-width `u32`
//! placeholder of its own — following this recommendation's own guidance
//! to compose the module's existing timestamp primitives as a
//! caller-supplied value rather than duplicate them. This is still a
//! Guiding-Principle-5 judgment call, not a confirmed wire fact: nothing
//! in this crate's roadmap states that a Timed request's execution-time
//! field is actually 32 bits wide, or that it is bit-for-bit identical to
//! [`crate::avtp::TscfHeader::avtp_timestamp`]'s own encoding, only that it
//! serves the same presentation-time-execution purpose. [`TimedExecutionTime`]
//! is, like [`RequestKind`] itself, a standalone value type not yet tied to
//! any offset within [`crate::acf::ByteMessageInfo`] or any other decoded
//! wire shape.
//!
//! ## Provenance note: cancellation scope and the addressed-endpoint/stream
//! ambiguity
//!
//! `ROADMAP.md`'s checklist bullet names clear-all as canceling "every
//! pending/in-flight request" but does not state what that "every" is
//! scoped to — the single addressed endpoint the clear-all request itself
//! targets, every endpoint on the addressed stream, or every request known
//! to this RC Server regardless of stream. This crate has no unified
//! "which requests are currently pending, across which scope" registry yet
//! (that is `ROADMAP.md`'s own not-yet-built "Request lifecycle state
//! machine" checklist bullet, later in this milestone), so
//! [`check_clear_all_cancellation`] does not attempt to enumerate or scope
//! anything itself: it is the uniform per-request outcome rule — a request
//! considered for clear-all is always canceled — leaving *which* requests
//! get considered, and under what scope, to whatever later item builds the
//! request registry this rule would be applied across.
//!
//! ## Provenance note: the safe-state-driving predicate as a
//! caller-supplied parameter
//!
//! `ROADMAP.md`'s checklist bullet states that clear-non-safestate must not
//! cancel requests that are part of a safe-state-driving sequence, but this
//! crate has no `rx_safety_measure`/safe-state machinery yet — that is
//! `ROADMAP.md` Milestone 6's own "Per-stream safety config" checklist
//! bullet, not yet built. [`check_clear_non_safestate_cancellation`]
//! therefore takes "is this request safe-state-related" as a plain
//! caller-supplied `bool` parameter, mirroring [`SequencerState`]'s own
//! caller-supplied-rather-than-read precedent above, rather than this module reading or inventing the not-yet-built
//! safe-state machinery's own state.
//!
//! ## Provenance note: `clear_transaction_num`'s width and matching field
//!
//! `ROADMAP.md`'s checklist bullet names `clear_transaction_num` as
//! clear-single's target-selection field but states neither its wire width
//! nor which already-decoded field it is matched against. Per Guiding
//! Principle 5, this crate reads "clear ... a single [request], identified
//! by a transaction number" as referring to the same per-transaction
//! correlation id Milestone 1 already decoded as
//! [`crate::acf::ByteMessageInfo::transaction_num`] (a plain `u8`), rather
//! than inventing a second, differently-shaped transaction-id concept.
//! [`ClearTransactionNum`] therefore wraps a `u8` to match
//! `transaction_num`'s own width, and [`check_clear_single_cancellation`]
//! takes its candidate transaction number as a plain `u8` — presumed to be
//! a request's own already-decoded `transaction_num` — rather than
//! accepting a [`crate::acf::ByteMessageInfo`] value directly, since this
//! module does not otherwise depend on `crate::acf` types.
//!
//! ## Provenance note: `RequestCanceled` as this item's outcome signal
//!
//! [`crate::RcpError::RequestCanceled`] was added in Milestone 2's own
//! "Error Model" item and reserved, unconstructed, ever since — see that
//! enum's own doc comment listing it among five codes "not yet constructed
//! anywhere in this crate" pending the later milestone that introduces a
//! cancelable-request concept. This item is that later milestone: per
//! Guiding Principle 5, this crate checked whether
//! [`crate::RcpError::RequestRejected`] (the collapse candidate every prior
//! Milestone 5 entry has checked and, so far, rejected for its own new
//! codes) plausibly covers a canceled request instead, and rejected it for
//! the same reason those prior entries did — `RequestRejected` names a
//! request that never executes at all, which does not capture a request
//! that *was* pending/in-flight and was deliberately stopped by another
//! request's own cancellation action. [`check_clear_all_cancellation`],
//! [`check_clear_non_safestate_cancellation`], and
//! [`check_clear_single_cancellation`] all construct
//! [`crate::RcpError::RequestCanceled`] for a request they select for
//! cancellation, retiring it as a reserved-but-unconstructed placeholder.
//!
//! ## Provenance note: arrival order as a caller-supplied sequence number
//!
//! `ROADMAP.md`'s checklist bullet's own tie-break rule, "FIFO within a
//! tier", requires comparing when two same-tier pending requests each
//! arrived — but this crate has no unified pending-request queue type yet
//! (`ROADMAP.md`'s own "Request lifecycle state machine" checklist bullet,
//! the next item in this milestone, is what would introduce one; see this
//! module's doc comment "Deliberately out of scope" section). Per this
//! milestone's own established discipline — [`SequencerState`] being taken
//! as caller-supplied ahead of [`SequencerBank`] existing,
//! `endpoint_busy` being taken as caller-supplied ahead of any unified
//! endpoint-state type existing — [`PendingRequestKey::arrival_seq`] is a
//! plain caller-supplied `u64`, presumed monotonically increasing in
//! arrival order (e.g. a per-server request counter or an arrival
//! timestamp), rather than this module inventing an owned queue/list data
//! structure of its own to track arrival order internally.
//! [`select_next_pending_request`] does not validate monotonicity — a
//! caller that supplies non-monotonic `arrival_seq` values gets
//! whatever FIFO ordering those values imply, silently, the same way a
//! caller that supplies an already-stale [`SequencerState`] to
//! [`check_compound_gate`] gets whatever gating outcome that stale value
//! implies.
//!
//! ## Provenance note: what execution priority ordering does not decide
//!
//! Per Guiding Principle 5, two points `ROADMAP.md`'s checklist bullet does
//! not state are flagged here rather than silently resolved one way or the
//! other:
//!
//! 1. **Scope.** The checklist bullet does not say whether execution
//!    priority is evaluated per-endpoint, per-stream, or server-wide across
//!    every pending request this RC Server holds regardless of which
//!    endpoint or stream it targets — mirroring the exact same
//!    unresolved-scope question this module's own "cancellation scope and
//!    the addressed-endpoint/stream ambiguity" provenance note already
//!    flags for clear-all. [`select_next_pending_request`] sidesteps the
//!    question rather than guessing at it: it operates purely over whatever
//!    slice of [`PendingRequestKey`] values a caller passes in, and it is
//!    the caller's own responsibility to have already narrowed that slice
//!    to whatever scope (one endpoint's pending requests, one stream's, or
//!    the whole server's) applies.
//! 2. **Starvation/overflow.** The checklist bullet names no
//!    error/rejection behavior for a pending request that never gets a
//!    turn because higher-tier requests keep arriving — nor for a pending
//!    set that grows without bound. [`select_next_pending_request`] takes
//!    no position on either: it always returns the single
//!    highest-priority, earliest-arrived entry in whatever non-empty slice
//!    it is given (or `None` for an empty slice), and has no notion of "too
//!    long waiting" or "too many pending" to reject on. Any such policy —
//!    were `ROADMAP.md` ever to name one — belongs to whatever later item
//!    owns the actual pending-request store this function's caller would
//!    draw its slice from, not to this pure selection rule.
//!
//! ## Provenance note: the compound-bundle gate as three caller-supplied
//! facts, not a read `GeneralRegisters`
//!
//! `ROADMAP.md`'s "Feature-bundle gating" checklist bullet reads as a rule
//! about what an implementation is allowed to *claim* — i.e., about
//! whatever sets [`crate::regmap::GeneralRegisters::svr_implemented_options`]
//! before it is exposed to a client, not about a client-side check run
//! against an already-received register block. This crate has no
//! not-yet-built "RC Server instance that owns its own live
//! `GeneralRegisters` and decides what to set `svr_implemented_options`
//! to" concept yet — the same gap every prior Milestone 5 entry already
//! works around by taking its own inputs as caller-supplied parameters
//! rather than reading them from a live server object (mirroring
//! [`SequencerState`], `root_client`, and `is_safestate_related` all being
//! taken the same way in this milestone's earlier entries; see this
//! module's doc comment "Deliberately out of scope" section above).
//! [`check_compound_bundle_claim`] therefore takes its three prerequisite
//! facts — `has_compound_wait`, `svr_sequencers_max`, and
//! `has_clear_non_safestate` — as plain caller-supplied values instead of
//! a [`crate::regmap::GeneralRegisters`] reference, leaving it to whichever
//! later item builds that owning RC Server concept to call this function
//! with the right facts before deciding whether to set the bit
//! [`crate::regmap::GeneralRegisters::claims_compound_wait_bundle`] reads.
//! `svr_sequencers_max` specifically is modeled as a plain `u8` rather than
//! a [`SequencerBank`] reference for the same reason
//! [`check_sequencer_num_in_bounds`] already takes it as a plain `u8`
//! elsewhere in this module: the count bound, not the bank's live mutable
//! state, is all this gate needs.
//!
//! ## Provenance note: `InvalidParameter` as the compound-bundle gate's
//! rejection code
//!
//! `ROADMAP.md`'s checklist bullet names the *rule* a false or partial
//! "compound request support" claim violates but no error code for it.
//! Per Guiding Principle 5, this crate checked [`RcpError::RequestRejected`]
//! — the collapse candidate every prior Milestone 5 entry checked first for
//! its own new outcome codes — and rejected it for the same reason
//! [`check_clear_all_cancellation`] and siblings did: `RequestRejected`
//! names a *request* (a message a client sent) that is refused, not a
//! *capability claim* (a server's own `svr_implemented_options` bit) that
//! fails a self-consistency check with no client request involved at all.
//! [`RcpError::InvalidParameter`] — already one of the eleven confirmed
//! TC18 spec error codes this crate's Milestone 2 "Error Model" item added
//! — reads as the closer fit: its own doc comment already describes it as
//! covering "one or more supplied parameter values is invalid", which a
//! bundle claim contradicted by its own three prerequisite facts is a
//! specific instance of. [`check_compound_bundle_claim`] therefore reuses
//! [`RcpError::InvalidParameter`] rather than inventing a new
//! bundle-gating-specific sentinel of its own.
//!
//! ## Safety-request MSB-tagging & watchdog-overflow purge (`ROADMAP.md`
//! Milestone 6, "Safety-request MSB-tagging" bullet)
//!
//! Added on top of the Milestone 5 taxonomy above, still in this same
//! module per this crate's settled `RequestKind`/taxonomy naming (see the
//! "Compound/compound-wait was the opening item of Milestone 5..."
//! paragraph above). Four pieces:
//!
//! - [`RequestKind::SafetyCompound`] (`0x8F`), [`RequestKind::
//!   SafetyCompoundWait`] (`0x8B`), and [`RequestKind::SafetyTriggered`]
//!   (`0x8E`) — three new variants, each exactly `0x80 | base` over
//!   [`RequestKind::Compound`], [`RequestKind::CompoundWait`], and
//!   [`RequestKind::Triggered`] respectively, the only three of this
//!   checklist bullet's own named hex values that decompose that way
//!   against this module's already-established nine discriminants. See
//!   "Provenance note: safety-tagging modeled as three new `RequestKind`
//!   variants, not a layered-on flag" below for why this extends
//!   [`RequestKind`] itself instead of introducing a second, orthogonal
//!   type the way [`crate::lifecycle::LockPolicy`] layers onto
//!   [`crate::lifecycle::RegisterCategory`].
//! - [`RequestKind::is_safety_tagged`] — the predicate distinguishing
//!   these three variants from the other nine.
//! - [`check_watchdog_overflow_purge`] — the single-request half of the
//!   watchdog-overflow purge rule this checklist bullet names, mirroring
//!   [`check_clear_non_safestate_cancellation`]'s own
//!   caller-supplied-`bool`-gated shape: `Ok(())` (do not purge) unless
//!   `watchdog_overflowed` is `true` *and* the request's own
//!   [`RequestKind::is_safety_tagged`] is `false`, in which case it
//!   returns `Err(RcpError::RequestCanceled)` — reusing, not
//!   reinventing, this crate's already-established cancellation outcome
//!   signal (see "Provenance note: `RequestCanceled` as this item's
//!   outcome signal" above).
//! - [`purge_normal_priority_on_watchdog_overflow`] — the slice-level
//!   composition of the above over [`PendingRequestKey`], mirroring
//!   [`select_next_pending_request`]'s own "pure function over a
//!   caller-supplied slice, no owned queue" shape: partitions a
//!   caller-supplied `&[PendingRequestKey]` into the indices kept queued
//!   and the indices purged. A safety-tagged entry is always kept
//!   (`watchdog_overflowed` or not); a normal-priority entry is kept only
//!   when `watchdog_overflowed` is `false`. Because
//!   [`select_next_pending_request`] and
//!   [`RequestLifecycleState::try_transition`] already treat every
//!   [`PendingRequestKey`]/[`RequestKind`] uniformly regardless of its
//!   safety-tagged status, a safety-tagged request kept queued past a
//!   purge remains eligible for both — this is how this item reads the
//!   checklist's "become the mechanism that drives the endpoint through
//!   its safe state" half of the same sentence, rather than building any
//!   new, safety-tag-specific execution path. See "Provenance note:
//!   watchdog overflow as a caller-supplied boolean, not real timer
//!   state" below for why `watchdog_overflowed` is a plain `bool` input,
//!   not a value read from any not-yet-built watchdog timer.
//!
//! Same "additive standalone plumbing only" discipline as every entry
//! above and every prior Milestone 1-5 entry: neither
//! [`check_watchdog_overflow_purge`] nor
//! [`purge_normal_priority_on_watchdog_overflow`] is called from any
//! decoder, dispatch loop, or the legacy `src/watchdog.rs` — the latter
//! is `ROADMAP.md`'s own REPLACE-dispositioned satellite package with no
//! pending-request-queue or safety-tag concept of its own, read only as
//! background here, not extended or reused. Deliberately out of scope for
//! this item, left for the next two still-unchecked Milestone 6 checklist
//! bullets: real per-stream watchdog configuration/timeout tracking
//! (`rx_wd_enable`/`rx_wd_timeout_interval`/`rx_wd_safestate_enable`, the
//! "Per-stream safety config" bullet) that would ever produce a real
//! `watchdog_overflowed` value, and the real safe-state machinery
//! (`rx_safety_measure`, `rx_safestate_sequencer`) a kept-queued
//! safety-tagged request would ultimately drive.
//!
//! ## Provenance note: safety-tagging modeled as three new `RequestKind`
//! variants, not a layered-on flag
//!
//! Two shapes were considered for `0x8F`/`0x8B`/`0x8E`: three new
//! [`RequestKind`] variants (chosen here), or a `RequestKind` plus a
//! second, orthogonal safety-tag type layered on top — the shape
//! [`crate::lifecycle::LockPolicy`]/[`crate::lifecycle::lock_policy`] and
//! [`crate::discovery::DiscoveryAccessKind`]/[`crate::discovery::
//! check_discovery_access`] both use elsewhere in this crate. Those two
//! precedents layer a *new* type over a category they do not themselves
//! own the discriminant space of ([`crate::lifecycle::RegisterCategory`],
//! a decoded discovery message's own access kind). `RequestKind` is
//! different: this module already owns `RequestKind`'s own `to_u8`/
//! `from_u8` round-trip over the wire discriminant byte itself, and
//! `ROADMAP.md`'s own checklist wording calls `0x8F`/`0x8B`/`0x8E`
//! "variants" — i.e., three more values for that same discriminant byte,
//! not a second field alongside it. Every prior Milestone 5 entry that
//! added a `RequestKind` value ([`RequestKind::Triggered`], [`RequestKind::
//! Chained`], [`RequestKind::Timed`], the cancellation trio,
//! [`RequestKind::Standard`]) took the same "extend the one enum, widen
//! every exhaustive match over it" path rather than introducing a second
//! type, and this item continues that precedent rather than switching
//! shapes now. Extending [`RequestKind`] does cost exhaustiveness in
//! [`execution_priority_tier`], [`resolve_compound_exec_delay`], and
//! [`resolve_trigger_exec_delay`] — each already widens its own match arm
//! set for every new variant added since Milestone 5's first entry (see
//! [`resolve_compound_exec_delay`]'s own doc comment for that precedent
//! stated explicitly), so this is the same recurring cost those three
//! functions already pay, not a new one.
//!
//! ## Provenance note: the MSB-tagged variants' wire placement
//!
//! `ROADMAP.md`'s checklist bullet states only the three hex values
//! `0x8F`/`0x8B`/`0x8E` themselves, the same way it stated
//! `0x0F`/`0x0B`/`0x0E`/etc. for the nine base [`RequestKind`] values —
//! see "Provenance note: `RequestKind`'s wire placement" above, which
//! this note extends rather than restates. No checklist text says which
//! byte or field of a request carries this discriminant, or gives
//! [`crate::acf::ByteMessageInfo`] a bit of its own for the MSB tag; that
//! struct's fields were all pinned down in Milestone 1 for reasons
//! unrelated to this milestone's safety-tagging bullet. Per Guiding
//! Principle 5, [`RequestKind::SafetyCompound`]/[`RequestKind::
//! SafetyCompoundWait`]/[`RequestKind::SafetyTriggered`] are therefore
//! modeled with exactly the same standalone-value-type treatment as the
//! nine base variants: no offset within [`crate::acf::ByteMessageInfo`]
//! or any other already-built wire shape is guessed here.
//!
//! ## Provenance note: execution-priority tier and exec-delay-timer
//! treatment for the three new variants
//!
//! `ROADMAP.md`'s Milestone 6 checklist bullet says nothing about whether
//! a safety-tagged compound/compound-wait/triggered request's own
//! execution-priority tier (Milestone 5's "cancellation > triggered >
//! timed > compound > compound-wait > chained > standard" ordering) or
//! `*_exec_delay` timer selection differs from its untagged counterpart —
//! only the watchdog-overflow purge exemption is named. Per Guiding
//! Principle 5, this item does not invent a difference: [`execution_priority_tier`]
//! maps each of the three new variants to the exact same
//! [`ExecutionPriorityTier`] its own untagged base kind already maps to
//! ([`RequestKind::SafetyCompound`] -> [`ExecutionPriorityTier::Compound`],
//! [`RequestKind::SafetyCompoundWait`] ->
//! [`ExecutionPriorityTier::CompoundWait`], [`RequestKind::
//! SafetyTriggered`] -> [`ExecutionPriorityTier::Triggered`]), and
//! [`resolve_compound_exec_delay`]/[`resolve_trigger_exec_delay`] resolve
//! each new variant's timer identically to its own base kind's. Should a
//! future item learn that safety-tagged requests actually preempt their
//! untagged counterparts' own tier, this is the value expected to change.
//!
//! ## Provenance note: watchdog overflow as a caller-supplied boolean, not
//! real timer state
//!
//! Mirrors [`TriggeredRuntime::tick`]'s own caller-supplied
//! `endpoint_idle` parameter and [`check_compound_bundle_claim`]'s own three
//! caller-supplied facts: this crate has no not-yet-built real watchdog
//! timer/timeout-tracking machinery yet (`ROADMAP.md`'s own next,
//! still-unchecked "Per-stream safety config" bullet's job — see this
//! module's "Deliberately out of scope" note in the section above), so
//! [`check_watchdog_overflow_purge`] and
//! [`purge_normal_priority_on_watchdog_overflow`] both take "a watchdog
//! overflow has occurred" as a plain caller-supplied `watchdog_overflowed:
//! bool`, exactly the same "take the fact, not the machinery that would
//! produce it" shape used throughout Milestones 1-5.
//!
//! ## Per-stream safety config (`ROADMAP.md` Milestone 6, "Per-stream
//! safety config" bullet)
//!
//! The second of the "next two still-unchecked Milestone 6 checklist
//! bullets" the section above names — the first,
//! [`crate::watchdog`]'s new per-stream liveness model, is a sibling
//! module rather than this one, since `ROADMAP.md`'s own checklist wording
//! names `watchdog.rs` itself as the file `rx_wd_enable`/
//! `rx_wd_timeout_interval`/`rx_wd_safestate_enable` replace it in. This
//! section covers the checklist bullet's other four field groups, all of
//! which stay in this module: they are per-request/per-stream dispatch
//! rules that read naturally as extensions of the "Safety-request
//! MSB-tagging" section above, and two of them ([`resolve_safe_state_mechanism`],
//! [`SequencerBank::force_state`]) compose directly against
//! [`SequencerBank`], already defined in this module.
//!
//! - [`E2eFailureScope`] / [`e2e_failure_scope`] / [`check_rx_enforce_e2e`]
//!   — `rx_enforce_e2e`: whether an E2E-CRC failure at one request only
//!   drops that request ([`E2eFailureScope::DropRequest`]) or latches the
//!   whole stream into a fault/safe state
//!   ([`E2eFailureScope::LatchStream`]). [`check_rx_enforce_e2e`] composes,
//!   rather than re-derives, [`crate::e2e::crc32_tc18`] — it runs the CRC
//!   over a caller-supplied coverage buffer (presumed built by
//!   [`crate::e2e::build_crc32_coverage_buffer`] or
//!   [`crate::e2e::build_crc32_coverage_buffer_for_fragment_train`]) and
//!   compares against the wire-carried `expected_crc`, returning
//!   `Err((RcpError::CrcError, scope))` on mismatch with `scope` telling
//!   the caller how far the consequence reaches.
//! - [`SafeStateMechanism`] / [`resolve_safe_state_mechanism`] /
//!   [`safe_state_sequencer_gate`] — `rx_safety_measure`,
//!   `rx_safestate_sequencer`, `rx_safe_sequencer_state`: which safe-state
//!   mechanism a stream uses, hi-Z-all-pins
//!   ([`SafeStateMechanism::HiZAllPins`]) or a sequencer-driven safety
//!   sequence ([`SafeStateMechanism::SequencerDriven`]). The
//!   sequencer-driven branch reads `rx_safestate_sequencer`/
//!   `rx_safe_sequencer_state` as a [`CompoundGateConfig`]
//!   ([`safe_state_sequencer_gate`]) — the same gate shape
//!   [`check_compound_gate`] already reads for ordinary compound-request
//!   gating — under this item's own working interpretation that entering
//!   this safe state means writing the named sequencer to the named target
//!   state, which then satisfies that same gate for any compound/
//!   compound-wait "safety sequence" requests pre-configured against it;
//!   see "Provenance note: the sequencer-driven safe state as a gate
//!   write, not a new mechanism" below for why no second, safe-state-only
//!   sequencer concept is introduced. [`SequencerBank::force_state`] is
//!   this branch's own unconditional write, added alongside
//!   [`enter_sequencer_driven_safe_state`] specifically because entering a
//!   safe state must not be blocked by [`SequencerBank::
//!   advance_if_still_in_start_state`]'s existing start-state race guard —
//!   see that method's own doc comment for why the guard exists for
//!   ordinary execution and why this item deliberately bypasses it here.
//! - [`OverflowOutcome`] / [`evaluate_request_storage_overflow`] —
//!   `rx_ovrflw_safestate_enable`: whether a request-storage overflow
//!   (this crate's existing [`RcpError::ReqStorageOvfl`] sentinel) also
//!   drives the stream's endpoints to safe state
//!   ([`OverflowOutcome::OverflowSafestate`]) or is left as a plain
//!   overflow with no safe-state consequence
//!   ([`OverflowOutcome::OverflowNoSafestate`]), mirroring
//!   [`crate::watchdog::StreamWatchdogOutcome`]'s own three-variant
//!   "no event / event, no consequence / event, with consequence" shape.
//! - [`SequenceTracker`] / [`SequenceEnforcementOutcome`] /
//!   [`evaluate_rx_enforce_seq`] — `rx_enforce_seq`/
//!   `rx_seq_safestate_enable`, ported from c-RCP's
//!   `rcp_e2e_seq_evaluate()` (`c-RCP/src/e2e.c:516`): a stateful
//!   per-stream tracker over the AVTPDU's 8-bit `sequence_num`, accepting a
//!   forward distance of 1..=127 mod 256 and flagging safe state on *any*
//!   discontinuity (even an accepted one). See "Provenance note: the
//!   enforced sequence number's own wire field and width" below.
//! - [`SafeStateAction`] / [`resolve_safe_state_action`] — the unifying
//!   composition every "...`_enable`" outcome type above funnels into:
//!   given "should this stream enter safe state right now" (any of
//!   [`crate::watchdog::StreamWatchdogOutcome::drives_safestate`],
//!   [`OverflowOutcome::drives_safestate`],
//!   [`SequenceEnforcementOutcome::drives_safestate`], or
//!   `matches!(scope, E2eFailureScope::LatchStream)`) and a resolved
//!   [`SafeStateMechanism`], produces the concrete
//!   [`SafeStateAction`] a caller should take.
//!
//! Same "additive standalone plumbing only" discipline as every entry
//! above: none of this is called from a decoder, dispatch loop, or
//! [`crate::watchdog`]. [`check_rx_enforce_e2e`] is the only function here
//! that reaches into another module's behavior
//! ([`crate::e2e::crc32_tc18`]), and even it takes the coverage buffer and
//! expected CRC as plain caller-supplied values rather than reading a live
//! frame itself.
//!
//! ## Provenance note: the sequencer-driven safe state as a gate write, not
//! a new mechanism
//!
//! `ROADMAP.md`'s checklist bullet states `rx_safestate_sequencer`/
//! `rx_safe_sequencer_state` name "which sequencer number and target state
//! kicks off the safety sequence," but not the mechanics of how a
//! sequencer number and target state "kick off" anything — that relationship
//! is defined nowhere this item can read except by analogy to the one
//! sequencer mechanism this crate already has: [`CompoundGateConfig`] /
//! [`is_gate_satisfied`] / [`check_compound_gate`], where a request
//! executes once its named sequencer holds its named target state. This
//! item's working interpretation reuses that same relationship rather than
//! inventing a second one: [`safe_state_sequencer_gate`] builds exactly the
//! [`CompoundGateConfig`] shape [`check_compound_gate`] already consumes,
//! and entering the sequencer-driven safe state
//! ([`enter_sequencer_driven_safe_state`]) is modeled as writing
//! [`SequencerBank`]'s named sequencer to that same target state — after
//! which any pre-configured "safety sequence" compound/compound-wait
//! requests gated on that sequencer/state pair become eligible to run via
//! the ordinary, already-built gate check, with no new sequencer-only
//! safe-state code path required. Flagged per Guiding Principle 5 for
//! reconciliation against real TC18 behavior, never against spec prose.
//!
//! ## Provenance note: the enforced sequence number's own wire field and
//! width
//!
//! The enforced sequence number is the AVTPDU header's own 8-bit
//! `sequence_num` ([`crate::avtp`]), matching c-RCP's
//! `rcp_e2e_seq_tracker_t` (`c-RCP/include/rcp/e2e.h:802-811`), which is
//! fed once per received frame by `frame_seq_gate_admits()`
//! (`c-RCP/src/mock.c:3038`). An earlier revision of this module took a
//! caller-supplied `u32` and compared strictly-greater with no wraparound,
//! so the 255 -> 0 wrap was rejected; [`SequenceTracker::evaluate`] now uses
//! c-RCP's RFC 1982 serial-number window instead.
//!
//! ## `CRC_ERROR` error path (`ROADMAP.md` Milestone 6, "`CRC_ERROR` error
//! path" bullet)
//!
//! The next checklist bullet after "Per-stream safety config" above, and
//! this crate's own closing of the "timing- and CRC-specific codes"
//! [`crate::RcpError`]'s own doc comment named as Milestone 2's deferred
//! work — see that enum's doc comment "TC18 spec error codes" section, and
//! its "Chained-request error codes" section for the precedent that already
//! landed the first two such deferred codes. [`check_rx_enforce_e2e`] —
//! already built by "Per-stream safety config" above — is this item's only
//! touch point: it now constructs [`crate::RcpError::CrcError`] on a CRC32
//! safe-point mismatch instead of its earlier, explicitly-provisional reuse
//! of `RcpError::CrcMismatch`. See "Provenance note: `CrcError` as
//! a new variant, distinct from the legacy `CrcMismatch` sentinel" directly
//! below for the full reasoning, and [`crate::e2e::check_fragment_crc_placement`]'s
//! own doc comment for why that function's unrelated `InvalidParameter`
//! return stays explicitly out of this item's scope. Same "additive
//! standalone plumbing only" discipline as every entry above: this item
//! changes which sentinel an existing, already-standalone function returns,
//! and wires nothing new into a decoder or dispatch loop.
//!
//! ## Provenance note: `CrcError` as a new variant, distinct from the
//! legacy `CrcMismatch` sentinel
//!
//! [`check_rx_enforce_e2e`], landed ahead of this item as part of
//! "Per-stream safety config" above, already needed some [`crate::RcpError`]
//! sentinel for a CRC32 safe-point mismatch. Absent an item-specific code at
//! the time, it reused `RcpError::CrcMismatch` — this crate's
//! existing wire/E2E sentinel — and that function's own doc comment flagged
//! the reuse as provisional, naming this exact checklist bullet as the item
//! that would revisit it. Per Guiding Principle 5, this item checked whether
//! a dedicated `CrcError` code should instead collapse onto `CrcMismatch`,
//! the way several Milestone 2 provisional sentinels collapsed onto
//! spec-named codes — but `CrcMismatch` was not that kind of placeholder at
//! the time: it was the real, then-still-live sentinel for a structurally
//! different mechanism (a CRC-16 computed over a fixed 16-byte legacy frame
//! by `crate::e2e::wrap`/`crate::e2e::unwrap`, independent of TC18's
//! safe-point CRC-32), and folding `CrcError` onto it would have made
//! `crate::e2e::unwrap`'s CRC-16 failures and [`check_rx_enforce_e2e`]'s
//! CRC-32 safe-point failures indistinguishable to any caller matching on
//! the returned [`crate::RcpError`]. This crate reads that as the same
//! shape of problem Milestone 5 solved by adding
//! [`crate::RcpError::ChainAborted`]/[`crate::RcpError::ChainError`] as new
//! variants rather than folding them onto
//! [`crate::RcpError::RequestRejected`] — see this module's own "Provenance
//! note: `CHAIN_ABORTED`/`CHAIN_ERROR` as new variants..." above for that
//! precedent — so [`crate::RcpError::CrcError`] was added as a new variant,
//! and [`check_rx_enforce_e2e`] was updated to construct it instead of
//! `RcpError::CrcMismatch`, with `RcpError::CrcMismatch` itself
//! left untouched at the time, still returned unchanged by the CRC-16
//! `wrap`/`unwrap` path. Per the same Guiding Principle 5 discipline this
//! crate's other provenance notes use: whether TC18's real wire-level
//! `CRC_ERROR` code also covers failure modes beyond the one
//! `rx_enforce_e2e`-driven CRC32 mismatch [`check_rx_enforce_e2e`] already
//! checks — for example,
//! [`crate::e2e::check_fragment_crc_placement`]'s own
//! CRC-presence-by-fragment-position rule — is left open rather than
//! silently assumed either way; this item's own default, absent roadmap
//! text saying otherwise, is "leave [`crate::e2e::check_fragment_crc_placement`]
//! returning [`crate::RcpError::InvalidParameter`] as-is," matching that
//! function's own doc comment note on the same question.
//!
//! **Update (`ROADMAP.md` Milestone 9, `e2e` REPLACE cutover):** the CRC-16
//! `wrap`/`unwrap` frame, `ReplayGuard`, and `E2eController` this note
//! describes have since been deleted outright from `crate::e2e`, and
//! `RcpError::CrcMismatch`/`RcpError::Replay` — the sentinels that pair of
//! mechanisms constructed — were retired along with them (see
//! [`crate::RcpError`]'s own "Wire / E2E errors" section). None of that
//! changes this note's reasoning for why `CrcError` was kept a distinct
//! variant rather than collapsed onto `CrcMismatch`: it records why the two
//! were distinct sentinels for two structurally different mechanisms at the
//! time both existed, which remains the accurate history even though only
//! `CrcError` is still constructible today.

use crate::acf::{
    decode_acf_gbb, encode_acf_gbb, AcfGbbMessage, ByteMessageInfo, Evt, ACF_ABB_HEADER_LEN,
    ACF_GBB_HEADER_LEN, ACF_GBB_MSG_TYPE,
};
use crate::regmap::SequencerStateEntry;
use crate::timestamp::AvtpTimestamp;
use crate::RcpError;

// ── RequestKind ──────────────────────────────────────────────────────────────

/// The request-type discriminant naming a conditional request's kind.
///
/// Nine values are modeled: the eight prior checklist bullets' own named
/// discriminants, plus [`RequestKind::Standard`] — the unconditional kind
/// this milestone's "Execution priority ordering" checklist bullet implies
/// by naming it as the lowest tier in its own ordering. See this module's
/// doc comment "Provenance note: `RequestKind::Standard`'s discriminant"
/// for why [`RequestKind::Standard`]'s numeric value carries materially
/// less confidence than the other eight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
//fusa:req REQ-CMP-001
//fusa:req REQ-TRIG-001
//fusa:req REQ-CHAIN-001
//fusa:req REQ-TIME-001
//fusa:req REQ-CANCEL-001
//fusa:req REQ-PRIO-001
//fusa:req REQ-CMP-009
pub enum RequestKind {
    /// The crate-local placeholder discriminant byte assigned to
    /// [`RequestKind::Standard`] — see this module's doc comment
    /// "Provenance note: `RequestKind::Standard`'s discriminant" for why
    /// `0x00`, unlike every other variant's discriminant below, is not a
    /// transcription of any roadmap-named wire value.
    ///
    /// Standard: the unconditional request kind implied by this milestone's
    /// own execution-priority ordering as its lowest-priority tier — see
    /// [`ExecutionPriorityTier::Standard`].
    Standard = 0x00,
    /// Chained (`0x01`): a sequence of requests whose remaining links are
    /// gated on the `cs`-bit abort-on-predecessor-error rule — see
    /// [`check_chain_continuation`].
    Chained = 0x01,
    /// Clear-all (`0x05`, mandatory): cancels every pending/in-flight
    /// request — see [`check_clear_all_cancellation`].
    ClearAll = 0x05,
    /// Clear-non-safestate (`0x06`, optional): cancels every
    /// pending/in-flight request except one actively driving an endpoint
    /// toward its configured safe state — see
    /// [`check_clear_non_safestate_cancellation`].
    ClearNonSafestate = 0x06,
    /// Clear-single (`0x07`, optional): cancels exactly one pending request
    /// identified by a `clear_transaction_num` — see [`ClearTransactionNum`]
    /// and [`check_clear_single_cancellation`].
    ClearSingle = 0x07,
    /// Timed (`0x0A`): presentation-time execution as this checklist
    /// bullet's own named alternative to a TSCF header — see
    /// [`TimedExecutionTime`] and [`is_timed_request_ready`].
    Timed = 0x0A,
    /// Compound-wait (`0x0B`): sequencer-gated execution that waits for its
    /// gate to be satisfied.
    CompoundWait = 0x0B,
    /// Triggered (`0x0E`): trigger-occurrence counting that runs
    /// independent of the target endpoint's busy/idle state.
    Triggered = 0x0E,
    /// Compound (`0x0F`): sequencer-gated execution.
    Compound = 0x0F,
    /// Safety-tagged compound-wait (`0x8B`, `ROADMAP.md` Milestone 6): the
    /// exact `0x80 | 0x0B` MSB-tagged sibling of [`Self::CompoundWait`] —
    /// exempt from [`purge_normal_priority_on_watchdog_overflow`]'s purge.
    /// See this module's doc comment "Safety-request MSB-tagging &
    /// watchdog-overflow purge" section for the full picture.
    SafetyCompoundWait = 0x8B,
    /// Safety-tagged triggered (`0x8E`, `ROADMAP.md` Milestone 6): the
    /// exact `0x80 | 0x0E` MSB-tagged sibling of [`Self::Triggered`].
    SafetyTriggered = 0x8E,
    /// Safety-tagged compound (`0x8F`, `ROADMAP.md` Milestone 6): the
    /// exact `0x80 | 0x0F` MSB-tagged sibling of [`Self::Compound`].
    SafetyCompound = 0x8F,
}

impl RequestKind {
    /// Encode this request kind as its discriminant byte.
    //fusa:req REQ-CMP-001
    //fusa:req REQ-TRIG-001
    //fusa:req REQ-CHAIN-001
    //fusa:req REQ-TIME-001
    //fusa:req REQ-CANCEL-001
    //fusa:req REQ-PRIO-001
    //fusa:req REQ-SAFETY-001
    //fusa:req REQ-CMP-009
    pub fn to_u8(self) -> u8 {
        self as u8
    }

    /// Decode a discriminant byte into a [`RequestKind`].
    ///
    /// Returns `Err(RcpError::InvalidParameter)` for any value other than
    /// the named discriminants. Never panics for any input.
    //fusa:req REQ-CMP-002
    //fusa:req REQ-TRIG-001
    //fusa:req REQ-CHAIN-001
    //fusa:req REQ-TIME-001
    //fusa:req REQ-CANCEL-001
    //fusa:req REQ-PRIO-001
    //fusa:req REQ-SAFETY-001
    //fusa:req REQ-CMP-009
    //fusa:req REQ-ERRH-001
    pub fn from_u8(raw: u8) -> Result<Self, RcpError> {
        match raw {
            0x00 => Ok(Self::Standard),
            0x01 => Ok(Self::Chained),
            0x05 => Ok(Self::ClearAll),
            0x06 => Ok(Self::ClearNonSafestate),
            0x07 => Ok(Self::ClearSingle),
            0x0A => Ok(Self::Timed),
            0x0B => Ok(Self::CompoundWait),
            0x0E => Ok(Self::Triggered),
            0x0F => Ok(Self::Compound),
            0x8B => Ok(Self::SafetyCompoundWait),
            0x8E => Ok(Self::SafetyTriggered),
            0x8F => Ok(Self::SafetyCompound),
            _ => Err(RcpError::InvalidParameter),
        }
    }

    /// Whether this request kind is one of the three MSB-tagged safety
    /// variants ([`Self::SafetyCompound`], [`Self::SafetyCompoundWait`],
    /// [`Self::SafetyTriggered`]) — the checklist's own "safety-tagged
    /// requests" predicate, consulted by
    /// [`check_watchdog_overflow_purge`]/
    /// [`purge_normal_priority_on_watchdog_overflow`] to exempt these three
    /// from the watchdog-overflow purge. Never panics for any input.
    //fusa:req REQ-SAFETY-002
    pub fn is_safety_tagged(self) -> bool {
        matches!(
            self,
            Self::SafetyCompound | Self::SafetyCompoundWait | Self::SafetyTriggered
        )
    }

    /// Decode a [`RequestKind`] from the leading byte of a GBB conditional
    /// request's already-decoded [`crate::acf::AcfGbbMessage::message_timestamp`].
    ///
    /// This is [`RequestKind`]'s first real binding to an already-built wire
    /// shape — see this module's doc comment "Provenance note: `RequestKind`'s
    /// wire placement" for the working interpretation this composes with and
    /// what it does not resolve. The leading byte is `(message_timestamp >>
    /// 56) as u8`, matching [`crate::acf::encode_acf_gbb`]'s existing
    /// big-endian `to_be_bytes()` encoding of that field — this function does
    /// not itself encode or decode an [`crate::acf::AcfGbbMessage`]; it only
    /// inspects a `message_timestamp` value already produced by
    /// [`crate::acf::decode_acf_gbb`].
    ///
    /// Returns `None`, never [`RequestKind::Standard`], for a leading byte of
    /// `0x00` or any byte [`RequestKind::from_u8`] does not otherwise
    /// recognize — see "Provenance note: `RequestKind::Standard`'s
    /// discriminant" for why `0x00` at this wire position is not treated as
    /// confirmation of an unconditional/standard request: this crate cannot
    /// distinguish a genuine standard GBB request (whose `message_timestamp`
    /// carries no conditional-request-kind byte at all) from a conditional
    /// GBB request whose leading timestamp byte simply happens to be zero.
    /// Returns `Some(kind)` only for the eleven other [`RequestKind`]
    /// discriminants, which — unlike `0x00` — have no such ambiguity: a
    /// standard request's `message_timestamp` is not expected to collide with
    /// one of them. Never panics for any input.
    //fusa:req REQ-CMP-008
    pub fn from_gbb_message_timestamp(message_timestamp: u64) -> Option<Self> {
        let raw = (message_timestamp >> 56) as u8;
        if raw == 0x00 {
            return None;
        }
        Self::from_u8(raw).ok()
    }

    /// Encode this [`RequestKind`] into the leading byte of a GBB conditional
    /// request's `message_timestamp`, preserving the low 56 bits of
    /// `message_timestamp` unchanged.
    ///
    /// The companion encode-side helper to [`RequestKind::
    /// from_gbb_message_timestamp`] — see that method's doc comment for the
    /// byte-offset reasoning this shares. Returns
    /// `Err(RcpError::InvalidParameter)` for [`RequestKind::Standard`]:
    /// unlike the other eleven variants, `Standard` has no confirmed wire
    /// encoding at all (see "Provenance note: `RequestKind::Standard`'s
    /// discriminant"), so this function refuses to inject its `0x00`
    /// placeholder discriminant into a real `message_timestamp` value as
    /// though it meant something on the wire. Never panics for any input.
    //fusa:req REQ-CMP-008
    pub fn to_gbb_message_timestamp(self, message_timestamp: u64) -> Result<u64, RcpError> {
        if self == Self::Standard {
            return Err(RcpError::InvalidParameter);
        }
        let low_56 = message_timestamp & 0x00FF_FFFF_FFFF_FFFF;
        Ok(low_56 | ((self.to_u8() as u64) << 56))
    }
}

// ── Conditional-request wire codecs (ACF_GBB, mtv = 0) ──────────────────────
//
// Ported from c-RCP `src/request.c:85-1058`. Every conditional request is an
// ACF_GBB message with `mtv = 0` whose 8-byte `message_timestamp` region is
// repurposed: octet 0 is the `request_type` opcode, octets 1..7 the kind's
// own sub-fields (big endian). Encoders build the frame through
// [`crate::acf::encode_acf_gbb`] (which, unlike c-RCP's `rcp_acf_encode_gbb`,
// never zeroes the timestamp region for `mtv = 0`, so no separate raw-header
// builder is needed); the header carries `op = 1`, `rsp = err = ms = hs = 0`
// and `read_size_segment = 0`, exactly as c-RCP's zero-initialised
// `rcp_acf_byte_message_info_t` + `RCP_ACF_OP_NONE` packs it.

/// The infinite-repetition sentinel for a compound/compound-wait request's
/// `cmp_repetitions`/`cmpw_repetitions` sub-field (all-ones). Mirrors c-RCP's
/// `RCP_COMPOUND_REPEAT_INFINITE` (`c-RCP/include/rcp/request.h:590`).
//fusa:req REQ-CMP-010
pub const COMPOUND_REPEAT_INFINITE: u16 = 0xFFFF;

/// The largest encodable Timed `presentation_time` (48 bits). Mirrors
/// `RCP_TIMED_PRESENTATION_TIME_MAX` (`c-RCP/include/rcp/request.h:1020`).
//fusa:req REQ-TIMED-003
pub const TIMED_PRESENTATION_TIME_MAX: u64 = 0x0000_FFFF_FFFF_FFFF;

/// Why a conditional-request frame failed to decode. One shared enum for
/// every request kind; the variants are the union of c-RCP's per-kind
/// `rcp_compound_errc_t`/`rcp_triggered_errc_t`/`rcp_chained_errc_t`/
/// `rcp_timed_errc_t`/`rcp_cancel_errc_t` failure modes
/// (`c-RCP/include/rcp/request.h`), with c-RCP's own messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
//fusa:req REQ-CMP-033
pub enum RequestCodecError {
    /// Shorter than the ACF_GBB fixed header, or than its own declared
    /// `acf_msg_length`/`pad`.
    #[error("rcp/request: frame too short")]
    ShortFrame,
    /// `acf_msg_type` is not ACF_GBB.
    #[error("rcp/request: unexpected ACF message type")]
    BadMsgType,
    /// `mtv` is set, so `message_timestamp` is a real timestamp, not a
    /// repurposed sub-field region.
    #[error("rcp/request: message_timestamp not repurposed")]
    NotRepurposed,
    /// The `request_type` opcode is not one the called decoder recognizes.
    #[error("rcp/request: unrecognized request_type")]
    UnknownType,
    /// A reserved sub-field octet carries a set bit.
    #[error("rcp/request: reserved sub-field octet is not zero")]
    ReservedNonzero,
    /// `evt[2:0]`, `hs` or `cs` is set on a cancellation request.
    #[error("rcp/request: evt[2:0], hs, or cs is not zero")]
    EvtHsCsNonzero,
    /// `hs` or `cs` is set on a Timed request.
    #[error("rcp/request: hs/cs must be clear on a timed request")]
    UnsupportedCmd,
}

impl RequestCodecError {
    /// The wire error an RC Server sends back for a conditional request
    /// that failed to decode: always `None` (no response at all).
    ///
    /// c-RCP parity: `rcp_server_endpoint_admit()` returns
    /// `RCP_SERVER_ADMIT_REJECTED` without setting `*out_error` when a
    /// compound/compound-wait/triggered/timed/chained body fails to decode
    /// (`c-RCP/src/server.c:458-512`), and `apply_cancellation()` silently
    /// ignores a clear-single that fails to decode (`c-RCP/src/mock.c:1198`),
    /// so nothing is sent to the client. TC18 RC5 expects an error response
    /// (e.g. Table 10/14's "rejected with error code = UNSUPPORTED_CMD", or
    /// INVALID_PARAMETER for a malformed field, as issue #193 notes). This
    /// crate copies c-RCP's no-response behaviour for interop.
    //fusa:req REQ-CMP-033
    pub fn response_error(self) -> Option<RcpError> {
        None
    }
}

/// The opcode-class predicates c-RCP's decoders test against.
fn is_compound_or_wait(kind: RequestKind) -> bool {
    matches!(
        kind,
        RequestKind::Compound
            | RequestKind::SafetyCompound
            | RequestKind::CompoundWait
            | RequestKind::SafetyCompoundWait
    )
}

fn is_triggered(kind: RequestKind) -> bool {
    matches!(kind, RequestKind::Triggered | RequestKind::SafetyTriggered)
}

/// `request_type` byte of a repurposed `message_timestamp`.
fn ts_request_type(ts: u64) -> u8 {
    (ts >> 56) as u8
}

/// Decode `b` as an ACF_GBB message and require `mtv == 0`, mapping every
/// failure onto c-RCP's `rcp_acf_decode_gbb()` error set
/// (`c-RCP/src/acf.c:484-512`): a too-short frame, an inconsistent
/// `acf_msg_length`/`pad` or a truncated payload are all
/// [`RequestCodecError::ShortFrame`].
fn decode_repurposed_gbb(b: &[u8]) -> Result<AcfGbbMessage, RequestCodecError> {
    if b.len() < ACF_GBB_HEADER_LEN {
        return Err(RequestCodecError::ShortFrame);
    }
    if b[0] >> 1 != ACF_GBB_MSG_TYPE {
        return Err(RequestCodecError::BadMsgType);
    }
    let msg = decode_acf_gbb(b).map_err(|_| RequestCodecError::ShortFrame)?;
    if msg.info.mtv {
        return Err(RequestCodecError::NotRepurposed);
    }
    Ok(msg)
}

/// Build a repurposed ACF_GBB frame (`mtv = 0`, `op = 1`).
fn encode_repurposed_gbb(
    byte_bus_id: u16,
    evt: Evt,
    cs: bool,
    transaction_num: u8,
    ts: u64,
    payload: &[u8],
) -> Result<Vec<u8>, RcpError> {
    encode_acf_gbb(&AcfGbbMessage {
        info: ByteMessageInfo {
            byte_bus_id,
            evt,
            cs,
            transaction_num,
            op: true,
            ..ByteMessageInfo::default()
        },
        message_timestamp: ts,
        payload: payload.to_vec(),
    })
}

/// True iff `evt[2:0]`, `hs` or `cs` is nonzero — the cancellation decoders'
/// shared header rule (TC18 Tables 13-15).
fn evt_hs_cs_nonzero(info: &ByteMessageInfo) -> bool {
    info.evt.sub_opcode & 0x07 != 0 || info.hs || info.cs
}

/// Read just the repurposed `request_type` opcode byte from a received
/// ACF_GBB frame, so a caller can dispatch to the right decoder without a
/// full decode first.
///
/// Returns [`RequestCodecError::ShortFrame`] if `b` is shorter than the
/// ACF_GBB header, [`RequestCodecError::BadMsgType`] if `b[0] >> 1` is not
/// ACF_GBB, and [`RequestCodecError::NotRepurposed`] if `mtv` is set; it
/// does not validate `acf_msg_length`. Mirrors
/// `rcp_compound_peek_request_type()` (`c-RCP/src/request.c:85-101`).
/// Never panics for any input.
//fusa:req REQ-CMP-030
pub fn peek_request_type(b: &[u8]) -> Result<u8, RequestCodecError> {
    if b.len() < ACF_GBB_HEADER_LEN {
        return Err(RequestCodecError::ShortFrame);
    }
    if b[0] >> 1 != ACF_GBB_MSG_TYPE {
        return Err(RequestCodecError::BadMsgType);
    }
    if (b[2] >> 5) & 0x1 != 0 {
        return Err(RequestCodecError::NotRepurposed);
    }
    Ok(b[ACF_ABB_HEADER_LEN])
}

// ── Compound / compound-wait (0x0F/0x8F, 0x0B/0x8B) ─────────────────────────

/// A compound/compound-wait request's five wire sub-fields, in wire order
/// after the leading opcode octet: `cmp_start_state` (offset 1),
/// `cmp_next_state` (offset 2), `cmp_sequencer` (offset 3),
/// `cmp_exec_delay` (offsets 4..5, BE) and `cmp_repetitions` (offsets 6..7,
/// BE) — the `cmpw_*` fields of a compound-wait request share the identical
/// layout. Mirrors c-RCP's `rcp_compound_step_t`
/// (`c-RCP/include/rcp/request.h:597-609`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-CMP-010
pub struct CompoundStep {
    /// The sequencer state this step requires (`0` = any state).
    pub start_state: u8,
    /// The state this step advances the sequencer to (`0` = stay).
    pub next_state: u8,
    /// Which sequencer this step targets.
    pub sequencer_index: u8,
    /// `cmp_exec_delay`/`cmpw_exec_delay`, in multiples of the endpoint's
    /// configured `ep_delay_time` — not milliseconds.
    pub exec_delay: u16,
    /// `cmp_repetitions`/`cmpw_repetitions`: remaining repetitions;
    /// [`COMPOUND_REPEAT_INFINITE`] means never decrement — see
    /// [`complete_repetition`].
    pub repeat_count: u16,
}

impl CompoundStep {
    fn pack(&self, request_type: u8) -> u64 {
        (u64::from(request_type) << 56)
            | (u64::from(self.start_state) << 48)
            | (u64::from(self.next_state) << 40)
            | (u64::from(self.sequencer_index) << 32)
            | (u64::from(self.exec_delay) << 16)
            | u64::from(self.repeat_count)
    }

    fn unpack(ts: u64) -> Self {
        Self {
            start_state: (ts >> 48) as u8,
            next_state: (ts >> 40) as u8,
            sequencer_index: (ts >> 32) as u8,
            exec_delay: (ts >> 16) as u16,
            repeat_count: ts as u16,
        }
    }
}

/// A decoded (or to-be-encoded) compound / compound-wait request.
#[derive(Debug, Clone, PartialEq, Eq)]
//fusa:req REQ-CMP-010
pub struct CompoundRequest {
    /// One of [`RequestKind::Compound`], [`RequestKind::SafetyCompound`],
    /// [`RequestKind::CompoundWait`], [`RequestKind::SafetyCompoundWait`].
    pub request_type: RequestKind,
    /// The addressed endpoint's 11-bit `byte_bus_id`.
    pub byte_bus_id: u16,
    /// The repurposed sub-fields.
    pub step: CompoundStep,
    /// The ACF header's own `evt` (a compound-wait request's §13.5.1
    /// comparison mode; `0` for a plain compound request).
    pub evt: Evt,
    /// The request's `transaction_num`.
    pub transaction_num: u8,
    /// The request's opaque payload (a compound-wait request's comparison
    /// target).
    pub payload: Vec<u8>,
}

/// Encode a compound / compound-wait request.
///
/// Returns `Err(RcpError::InvalidParameter)` if `req.request_type` is not a
/// compound or compound-wait kind, and `Err(RcpError::InvalidSize)` if the
/// payload exceeds the ACF 9-bit quadlet range or `byte_bus_id` exceeds 11
/// bits. Mirrors `rcp_compound_encode_request()`
/// (`c-RCP/src/request.c:186-200`), which returns a NULL frame in those
/// cases. Never panics for any input.
//fusa:req REQ-CMP-010
//fusa:req REQ-CMP-026
//fusa:req REQ-CMP-031
//fusa:req REQ-CMP-032
pub fn encode_compound_request(req: &CompoundRequest) -> Result<Vec<u8>, RcpError> {
    if !is_compound_or_wait(req.request_type) {
        return Err(RcpError::InvalidParameter);
    }
    encode_repurposed_gbb(
        req.byte_bus_id,
        req.evt,
        false,
        req.transaction_num,
        req.step.pack(req.request_type.to_u8()),
        &req.payload,
    )
}

/// Decode a compound / compound-wait request.
///
/// Fails with [`RequestCodecError::ShortFrame`], [`RequestCodecError::BadMsgType`],
/// [`RequestCodecError::NotRepurposed`], or [`RequestCodecError::UnknownType`]
/// (opcode is not `0x0F`/`0x8F`/`0x0B`/`0x8B`). Never panics for any input.
///
/// c-RCP parity: `rcp_compound_decode_request()` (`c-RCP/src/request.c:208-238`)
/// checks only the GBB framing, `mtv` and the opcode; it never inspects
/// `hs`, `cs`, `rsp` or `err`, and accepts every `cmp_*`/`cmpw_*` sub-field
/// value. TC18 RC5 Table 7 (TC18-11.2.2.2-007) and Table 8
/// (TC18-11.2.2.3-009) assign meaning to `cs` only for compound-wait
/// requests (TC18-11.2.2.3-008) and describe `rsp`/`err` as response-only
/// bits, so a strict reading would reject a request that sets them. This
/// decoder copies c-RCP and accepts them unvalidated.
//fusa:req REQ-CMP-011
//fusa:req REQ-CMP-012
//fusa:req REQ-CMP-013
//fusa:req REQ-CMP-014
//fusa:req REQ-CMP-015
//fusa:req REQ-CMP-027
pub fn decode_compound_request(b: &[u8]) -> Result<CompoundRequest, RequestCodecError> {
    let msg = decode_repurposed_gbb(b)?;
    let request_type = RequestKind::from_u8(ts_request_type(msg.message_timestamp))
        .ok()
        .filter(|k| is_compound_or_wait(*k))
        .ok_or(RequestCodecError::UnknownType)?;
    Ok(CompoundRequest {
        request_type,
        byte_bus_id: msg.info.byte_bus_id,
        step: CompoundStep::unpack(msg.message_timestamp),
        evt: msg.info.evt,
        transaction_num: msg.info.transaction_num,
        payload: msg.payload,
    })
}

// ── Triggered (0x0E/0x8E) codec ──────────────────────────────────────────────

impl TriggeredStep {
    fn pack(&self, request_type: u8) -> u64 {
        (u64::from(request_type) << 56)
            | (u64::from(self.trigger_source_ep) << 48)
            | (u64::from(self.trigger_signal_nr) << 40)
            | (u64::from(self.trigger_threshold) << 32)
            | (u64::from(self.exec_delay) << 16)
            | u64::from(self.repeat_count)
    }

    fn unpack(ts: u64) -> Self {
        Self {
            trigger_source_ep: (ts >> 48) as u8,
            trigger_signal_nr: (ts >> 40) as u8,
            trigger_threshold: (ts >> 32) as u8,
            exec_delay: (ts >> 16) as u16,
            repeat_count: ts as u16,
        }
    }
}

/// A decoded (or to-be-encoded) triggered request. c-RCP's triggered codec
/// carries no `evt` of its own: it encodes `evt = 0` and does not surface
/// the decoded value (`c-RCP/src/request.c:431-503`).
#[derive(Debug, Clone, PartialEq, Eq)]
//fusa:req REQ-TRIG-014
pub struct TriggeredRequest {
    /// [`RequestKind::Triggered`] or [`RequestKind::SafetyTriggered`].
    pub request_type: RequestKind,
    /// The addressed endpoint's 11-bit `byte_bus_id`.
    pub byte_bus_id: u16,
    /// The repurposed trigger-selection sub-fields.
    pub step: TriggeredStep,
    /// The request's `transaction_num`.
    pub transaction_num: u8,
    /// The request's opaque payload.
    pub payload: Vec<u8>,
}

/// Encode a triggered request. Returns `Err(RcpError::InvalidParameter)` if
/// `req.request_type` is not a triggered kind and `Err(RcpError::InvalidSize)`
/// for an oversized payload / out-of-range `byte_bus_id`. Mirrors
/// `rcp_triggered_encode_request()` (`c-RCP/src/request.c:431-472`). Never
/// panics for any input.
//fusa:req REQ-TRIG-014
//fusa:req REQ-TRIG-015
pub fn encode_triggered_request(req: &TriggeredRequest) -> Result<Vec<u8>, RcpError> {
    if !is_triggered(req.request_type) {
        return Err(RcpError::InvalidParameter);
    }
    encode_repurposed_gbb(
        req.byte_bus_id,
        Evt::default(),
        false,
        req.transaction_num,
        req.step.pack(req.request_type.to_u8()),
        &req.payload,
    )
}

/// Decode a triggered request: same failure modes as
/// [`decode_compound_request`], with [`RequestCodecError::UnknownType`] for
/// any opcode other than `0x0E`/`0x8E`. Mirrors
/// `rcp_triggered_decode_request()` (`c-RCP/src/request.c:477-503`). Never
/// panics for any input.
///
/// c-RCP parity: TC18 RC5 Table 10 (TC18-11.2.2.5-006) requires `hs=0b`,
/// `cs=0b`, `rsp=0b` and `err=0b` in a triggered request, but
/// `rcp_triggered_decode_request()` (`c-RCP/src/request.c:477-503`) checks
/// none of them. This decoder copies c-RCP: a triggered request with any of
/// those bits set decodes successfully and the bits are ignored.
//fusa:req REQ-TRIG-006
//fusa:req REQ-TRIG-007
//fusa:req REQ-TRIG-016
pub fn decode_triggered_request(b: &[u8]) -> Result<TriggeredRequest, RequestCodecError> {
    let msg = decode_repurposed_gbb(b)?;
    let request_type = RequestKind::from_u8(ts_request_type(msg.message_timestamp))
        .ok()
        .filter(|k| is_triggered(*k))
        .ok_or(RequestCodecError::UnknownType)?;
    Ok(TriggeredRequest {
        request_type,
        byte_bus_id: msg.info.byte_bus_id,
        step: TriggeredStep::unpack(msg.message_timestamp),
        transaction_num: msg.info.transaction_num,
        payload: msg.payload,
    })
}

// ── Chained (0x01) codec ─────────────────────────────────────────────────────

/// A decoded (or to-be-encoded) chained-request member: `request_type
/// 0x01`, octets 1..3 reserved, `chain_exec_delay` at octets 4..5 (BE),
/// octets 6..7 reserved; the ACF header's `cs` bit is this member's
/// abort-on-predecessor-error selector.
#[derive(Debug, Clone, PartialEq, Eq)]
//fusa:req REQ-CHAIN-004
pub struct ChainedMember {
    /// The addressed endpoint's 11-bit `byte_bus_id`.
    pub byte_bus_id: u16,
    /// `chain_exec_delay`, in multiples of the endpoint's `ep_delay_time`,
    /// measured from the predecessor's finalization.
    pub chain_exec_delay: u16,
    /// The `cs` bit: `true` = abort this member if its predecessor errored.
    pub cs: bool,
    /// The member's `transaction_num`.
    pub transaction_num: u8,
    /// The member's opaque payload.
    pub payload: Vec<u8>,
}

/// Encode a chained-request member, leaving every reserved octet zero.
/// Returns `Err(RcpError::InvalidSize)` for an oversized payload /
/// out-of-range `byte_bus_id`. Mirrors `rcp_chained_encode_member()`
/// (`c-RCP/src/request.c:580-626`). Never panics for any input.
//fusa:req REQ-CHAIN-004
//fusa:req REQ-CHAIN-013
pub fn encode_chained_member(member: &ChainedMember) -> Result<Vec<u8>, RcpError> {
    let ts = (u64::from(RequestKind::Chained.to_u8()) << 56)
        | (u64::from(member.chain_exec_delay) << 16);
    encode_repurposed_gbb(
        member.byte_bus_id,
        Evt::default(),
        member.cs,
        member.transaction_num,
        ts,
        &member.payload,
    )
}

/// Decode a chained-request member: [`RequestCodecError::UnknownType`] for an
/// opcode other than `0x01`, [`RequestCodecError::ReservedNonzero`] if any
/// of octets 1..3 or 6..7 is nonzero, otherwise the same failure modes as
/// [`decode_compound_request`]. Never panics for any input.
///
/// c-RCP parity: TC18 RC5 Table 11 (TC18-11.2.2.6-010) requires `hs=0b`,
/// `rsp=0b` and `err=0b` in a chained-request member, but
/// `rcp_chained_decode_member()` (`c-RCP/src/request.c:631-661`) checks only
/// the reserved octets and never inspects `hs`, `rsp`, `err` or `evt`. This
/// decoder copies c-RCP and accepts those bits unvalidated.
//fusa:req REQ-CHAIN-005
//fusa:req REQ-CHAIN-006
//fusa:req REQ-CHAIN-007
//fusa:req REQ-CHAIN-012
pub fn decode_chained_member(b: &[u8]) -> Result<ChainedMember, RequestCodecError> {
    let msg = decode_repurposed_gbb(b)?;
    let ts = msg.message_timestamp;
    if ts_request_type(ts) != RequestKind::Chained.to_u8() {
        return Err(RequestCodecError::UnknownType);
    }
    if ts & 0x00FF_FFFF_0000_FFFF != 0 {
        return Err(RequestCodecError::ReservedNonzero);
    }
    Ok(ChainedMember {
        byte_bus_id: msg.info.byte_bus_id,
        chain_exec_delay: (ts >> 16) as u16,
        cs: msg.info.cs,
        transaction_num: msg.info.transaction_num,
        payload: msg.payload,
    })
}

// ── Timed (0x0A) codec ───────────────────────────────────────────────────────

/// A decoded (or to-be-encoded) NTSCF timed request: `request_type 0x0A`,
/// octet 1 reserved, a 48-bit big-endian `presentation_time` (gPTP
/// nanoseconds mod 2^48) at octets 2..7.
#[derive(Debug, Clone, PartialEq, Eq)]
//fusa:req REQ-TIMED-003
pub struct TimedRequest {
    /// The addressed endpoint's 11-bit `byte_bus_id`.
    pub byte_bus_id: u16,
    /// The 48-bit presentation time (`0..=TIMED_PRESENTATION_TIME_MAX`).
    pub presentation_time: u64,
    /// The request's `transaction_num`.
    pub transaction_num: u8,
    /// The request's opaque payload.
    pub payload: Vec<u8>,
}

/// Encode a timed request with `hs`/`cs` clear and the reserved octet zero.
/// Returns `Err(RcpError::InvalidParameter)` when `presentation_time`
/// exceeds [`TIMED_PRESENTATION_TIME_MAX`] (never silently truncated) and
/// `Err(RcpError::InvalidSize)` for an oversized payload / out-of-range
/// `byte_bus_id`. Mirrors `rcp_timed_encode_request()`
/// (`c-RCP/src/request.c:731-776`). Never panics for any input.
//fusa:req REQ-TIMED-003
pub fn encode_timed_request(req: &TimedRequest) -> Result<Vec<u8>, RcpError> {
    if req.presentation_time > TIMED_PRESENTATION_TIME_MAX {
        return Err(RcpError::InvalidParameter);
    }
    let ts = (u64::from(RequestKind::Timed.to_u8()) << 56) | req.presentation_time;
    encode_repurposed_gbb(
        req.byte_bus_id,
        Evt::default(),
        false,
        req.transaction_num,
        ts,
        &req.payload,
    )
}

/// Decode a timed request: [`RequestCodecError::UnknownType`] for an opcode
/// other than `0x0A`, [`RequestCodecError::ReservedNonzero`] if octet 1 is
/// nonzero, [`RequestCodecError::UnsupportedCmd`] if `hs` or `cs` is set
/// (checked in that order), otherwise the same failure modes as
/// [`decode_compound_request`]. Mirrors
/// `rcp_timed_decode_request()` (`c-RCP/src/request.c:806-837`). Never
/// panics for any input.
///
/// c-RCP parity: TC18 RC5 Table 12 (TC18-11.2.2.7-004) lists `evt`, `rsp`
/// and `err` as the standard request fields, with `rsp`/`err` meaningful only
/// in responses, while TC18-11.2.2.7-005 rejects only `hs`/`cs`.
/// `rcp_timed_decode_request()` (`c-RCP/src/request.c:806-837`) enforces the
/// reserved octet and `hs`/`cs` but never inspects `evt`, `rsp` or `err`.
/// This decoder copies c-RCP and accepts those bits unvalidated.
//fusa:req REQ-TIMED-004
//fusa:req REQ-TIMED-005
//fusa:req REQ-TIMED-009
//fusa:req REQ-TIMED-010
pub fn decode_timed_request(b: &[u8]) -> Result<TimedRequest, RequestCodecError> {
    let msg = decode_repurposed_gbb(b)?;
    let ts = msg.message_timestamp;
    if ts_request_type(ts) != RequestKind::Timed.to_u8() {
        return Err(RequestCodecError::UnknownType);
    }
    if (ts >> 48) as u8 != 0 {
        return Err(RequestCodecError::ReservedNonzero);
    }
    if msg.info.hs || msg.info.cs {
        return Err(RequestCodecError::UnsupportedCmd);
    }
    Ok(TimedRequest {
        byte_bus_id: msg.info.byte_bus_id,
        presentation_time: ts & TIMED_PRESENTATION_TIME_MAX,
        transaction_num: msg.info.transaction_num,
        payload: msg.payload,
    })
}

// ── Cancellation (0x05/0x06/0x07) codecs ─────────────────────────────────────

/// A decoded (or to-be-encoded) clear-all (`0x05`) or clear-non-safestate
/// (`0x06`) request: no sub-fields of its own, a fixed 16-byte frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-CMP-016
//fusa:req REQ-CANCEL-016
pub struct ClearRequest {
    /// The addressed endpoint's 11-bit `byte_bus_id`.
    pub byte_bus_id: u16,
    /// The cancellation request's own `transaction_num`.
    pub transaction_num: u8,
}

/// A decoded (or to-be-encoded) clear-single (`0x07`) request:
/// `clear_transaction_num` at octet 3, every other sub-field octet reserved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-CANCEL-005
pub struct ClearSingleRequest {
    /// The addressed endpoint's 11-bit `byte_bus_id`.
    pub byte_bus_id: u16,
    /// The `transaction_num` of the queued request to cancel.
    pub clear_transaction_num: u8,
    /// The cancellation request's own `transaction_num`.
    pub transaction_num: u8,
}

/// Encode a clear-non-safestate request (opcode `0x06`, 7 zero sub-field
/// octets). Returns `Err(RcpError::InvalidSize)` only for an out-of-range
/// `byte_bus_id`. Mirrors `rcp_compound_encode_clear_non_safestate()`
/// (`c-RCP/src/request.c:243-249`). Never panics for any input.
//fusa:req REQ-CMP-016
pub fn encode_clear_non_safestate(req: &ClearRequest) -> Result<Vec<u8>, RcpError> {
    let ts = u64::from(RequestKind::ClearNonSafestate.to_u8()) << 56;
    encode_repurposed_gbb(
        req.byte_bus_id,
        Evt::default(),
        false,
        req.transaction_num,
        ts,
        &[],
    )
}

/// Decode a clear-non-safestate request: [`RequestCodecError::UnknownType`]
/// unless the opcode is `0x06`, [`RequestCodecError::ReservedNonzero`] if any
/// trailing octet is set, [`RequestCodecError::EvtHsCsNonzero`] if
/// `evt[2:0]`/`hs`/`cs` is set (in that order). Mirrors
/// `rcp_compound_decode_clear_non_safestate()` (`c-RCP/src/request.c:255-291`).
/// Never panics for any input.
//fusa:req REQ-CMP-017
//fusa:req REQ-CMP-018
//fusa:req REQ-CMP-028
//fusa:req REQ-CMP-029
pub fn decode_clear_non_safestate(b: &[u8]) -> Result<ClearRequest, RequestCodecError> {
    decode_clear_no_subfields(b, RequestKind::ClearNonSafestate)
}

/// Encode a clear-all request (opcode `0x05`, 7 zero sub-field octets).
/// Returns `Err(RcpError::InvalidSize)` only for an out-of-range
/// `byte_bus_id`. Mirrors `rcp_cancel_encode_clear_all()`
/// (`c-RCP/src/request.c:955-960`). Never panics for any input.
//fusa:req REQ-CANCEL-016
pub fn encode_clear_all(req: &ClearRequest) -> Result<Vec<u8>, RcpError> {
    let ts = u64::from(RequestKind::ClearAll.to_u8()) << 56;
    encode_repurposed_gbb(
        req.byte_bus_id,
        Evt::default(),
        false,
        req.transaction_num,
        ts,
        &[],
    )
}

/// Decode a clear-all request: same rules as [`decode_clear_non_safestate`]
/// for opcode `0x05`. Mirrors `rcp_cancel_decode_clear_all()`
/// (`c-RCP/src/request.c:966-1001`). Never panics for any input.
//fusa:req REQ-CANCEL-013
//fusa:req REQ-CANCEL-014
//fusa:req REQ-CANCEL-017
pub fn decode_clear_all(b: &[u8]) -> Result<ClearRequest, RequestCodecError> {
    decode_clear_no_subfields(b, RequestKind::ClearAll)
}

fn decode_clear_no_subfields(
    b: &[u8],
    kind: RequestKind,
) -> Result<ClearRequest, RequestCodecError> {
    let msg = decode_repurposed_gbb(b)?;
    let ts = msg.message_timestamp;
    if ts_request_type(ts) != kind.to_u8() {
        return Err(RequestCodecError::UnknownType);
    }
    if ts & 0x00FF_FFFF_FFFF_FFFF != 0 {
        return Err(RequestCodecError::ReservedNonzero);
    }
    if evt_hs_cs_nonzero(&msg.info) {
        return Err(RequestCodecError::EvtHsCsNonzero);
    }
    Ok(ClearRequest {
        byte_bus_id: msg.info.byte_bus_id,
        transaction_num: msg.info.transaction_num,
    })
}

/// Encode a clear-single request. Returns `Err(RcpError::InvalidSize)` only
/// for an out-of-range `byte_bus_id`. Mirrors
/// `rcp_cancel_encode_clear_single()` (`c-RCP/src/request.c:1006-1017`).
/// Never panics for any input.
//fusa:req REQ-CANCEL-005
pub fn encode_clear_single(req: &ClearSingleRequest) -> Result<Vec<u8>, RcpError> {
    let ts = (u64::from(RequestKind::ClearSingle.to_u8()) << 56)
        | (u64::from(req.clear_transaction_num) << 32);
    encode_repurposed_gbb(
        req.byte_bus_id,
        Evt::default(),
        false,
        req.transaction_num,
        ts,
        &[],
    )
}

/// Decode a clear-single request: [`RequestCodecError::UnknownType`] unless
/// the opcode is `0x07`, [`RequestCodecError::ReservedNonzero`] if any of
/// octets 1..2 or 4..7 is set, [`RequestCodecError::EvtHsCsNonzero`] if
/// `evt[2:0]`/`hs`/`cs` is set. Mirrors `rcp_cancel_decode_clear_single()`
/// (`c-RCP/src/request.c:1022-1058`). Never panics for any input.
//fusa:req REQ-CANCEL-006
//fusa:req REQ-CANCEL-007
//fusa:req REQ-CANCEL-015
pub fn decode_clear_single(b: &[u8]) -> Result<ClearSingleRequest, RequestCodecError> {
    let msg = decode_repurposed_gbb(b)?;
    let ts = msg.message_timestamp;
    if ts_request_type(ts) != RequestKind::ClearSingle.to_u8() {
        return Err(RequestCodecError::UnknownType);
    }
    if ts & 0x00FF_FF00_FFFF_FFFF != 0 {
        return Err(RequestCodecError::ReservedNonzero);
    }
    if evt_hs_cs_nonzero(&msg.info) {
        return Err(RequestCodecError::EvtHsCsNonzero);
    }
    Ok(ClearSingleRequest {
        byte_bus_id: msg.info.byte_bus_id,
        clear_transaction_num: (ts >> 32) as u8,
        transaction_num: msg.info.transaction_num,
    })
}

// ── Sequencer gating ─────────────────────────────────────────────────────────

/// A sequencer's persistent state value.
///
/// Mirrors [`crate::regmap::SequencerStateEntry::seq_state`]'s single-byte
/// shape — see this module's doc comment "Provenance note: `start_state`
/// and the not-yet-built sequencer-state machine" for why this crate does
/// not yet read this value from an actual register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-CMP-003
//fusa:req REQ-SEQ-005
pub struct SequencerState(pub u8);

/// A compound/compound-wait request's sequencer gate: which sequencer it
/// names, and the persistent state that sequencer must hold for this
/// request to execute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-CMP-003
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
//fusa:req REQ-CMP-004
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
//fusa:req REQ-CMP-005
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
//fusa:req REQ-CMP-004
//fusa:req REQ-CMP-005
//fusa:req REQ-ERRH-001
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
//fusa:req REQ-CMP-006
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
/// select. `RequestKind::Chained` (a fourth variant, added alongside
/// [`check_chain_continuation`]) is likewise `None` here — `ROADMAP.md`'s
/// Chained checklist bullet names no `*_exec_delay` timer of its own —
/// and so is `RequestKind::Timed` (a fifth variant, added alongside
/// [`TimedExecutionTime`]): Timed's own gate is
/// [`is_timed_request_ready`], not an elapsed-tick delay timer like
/// [`CompoundExecDelays`]/[`TriggerExecDelay`]. The three cancellation
/// variants ([`RequestKind::ClearAll`]/[`RequestKind::ClearNonSafestate`]/
/// [`RequestKind::ClearSingle`], added alongside
/// [`check_clear_all_cancellation`]) are likewise `None` here —
/// `ROADMAP.md`'s Cancellation checklist bullet names no `*_exec_delay`
/// timer for any of the three, and so is [`RequestKind::Standard`] (a
/// ninth and final variant, added alongside [`ExecutionPriorityTier`]) —
/// the unconditional kind names no execution-delay timer of its own
/// either. Not yet called from anywhere in this crate
/// (see this module's doc comment for why), so this is a safe
/// additive-plumbing-stage widening, not a breaking change to any
/// consumer.
//fusa:req REQ-CMP-006
//fusa:req REQ-PRIO-002
//fusa:req REQ-SAFETY-003
pub fn resolve_compound_exec_delay(kind: RequestKind, delays: &CompoundExecDelays) -> Option<u32> {
    match kind {
        RequestKind::Compound | RequestKind::SafetyCompound => Some(delays.cmp_exec_delay),
        RequestKind::CompoundWait | RequestKind::SafetyCompoundWait => Some(delays.cmpw_exec_delay),
        RequestKind::Triggered
        | RequestKind::SafetyTriggered
        | RequestKind::Chained
        | RequestKind::Timed
        | RequestKind::ClearAll
        | RequestKind::ClearNonSafestate
        | RequestKind::ClearSingle
        | RequestKind::Standard => None,
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
//fusa:req REQ-CMP-007
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

// ── SequencerBank: the persistent 8-bit sequencer-state register bank ───────

/// The persistent 8-bit sequencer-state register bank this checklist bullet
/// names: one [`SequencerState`] per sequencer number, live-bounded by a
/// `svr_sequencers_max` value mirroring
/// [`crate::regmap::GeneralRegisters::svr_sequencers_max`].
///
/// See this module's doc comment "Provenance note: `SequencerBank`'s
/// reset-trigger scope" for why [`Self::new`] is the only way to obtain an
/// all-defaults bank, and "Provenance note: `start_state` and the
/// sequencer-state machine" for how this relates to
/// [`crate::regmap::SequencerStateEntry`] and to the free functions
/// ([`is_gate_satisfied`], [`check_compound_gate`],
/// [`advance_sequencer_if_still_in_start_state`]) this type composes rather
/// than replaces.
#[derive(Debug, Clone, PartialEq, Eq)]
//fusa:req REQ-SEQ-001
pub struct SequencerBank {
    states: Vec<SequencerState>,
    /// Per-sequencer owner (TC18 §12.7.10 Table 28 `Request_stream_index`),
    /// index-parallel with `states`; [`SEQUENCER_OWNER_UNCLAIMED`] by
    /// default. Mirrors c-RCP's `rcp_sequencer_table_t.owner[]`
    /// (`c-RCP/include/rcp/request_sequencer.h`).
    owners: Vec<u8>,
}

/// The reserved "no client has claimed this sequencer" owner value — the
/// 1-based `request_stream_index` convention's `0` sentinel. An unclaimed
/// sequencer permits no client at all (fail-closed), mirroring c-RCP's
/// `RCP_SEQUENCER_OWNER_UNCLAIMED` (`c-RCP/include/rcp/request_sequencer.h`).
//fusa:req REQ-SEQ-013
pub const SEQUENCER_OWNER_UNCLAIMED: u8 = 0;

/// The three-way outcome of a sequencer access check — c-RCP's
/// `rcp_sequencer_access_errc_t` (`c-RCP/include/rcp/request_sequencer.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
//fusa:req REQ-WIREERR-005
pub enum SequencerAccess {
    /// The index is valid and its owner equals the requester.
    Permitted,
    /// The index does not address a configured sequencer.
    Unknown,
    /// The index is valid, but the sequencer is unclaimed or owned by a
    /// different request stream.
    Denied,
}

impl SequencerAccess {
    /// The TC18 wire error a Response frame carries for this outcome:
    /// `None` for [`Self::Permitted`] (nothing to report),
    /// [`RcpError::SequencerNotKnown`] (code 2) for [`Self::Unknown`], and
    /// [`RcpError::UnauthorizedAccess`] (code 3) for [`Self::Denied`].
    /// Mirrors `rcp_sequencer_wire_error()`
    /// (`c-RCP/src/request_sequencer.c:143-157`).
    //fusa:req REQ-WIREERR-005
    pub fn wire_error(self) -> Option<RcpError> {
        match self {
            Self::Permitted => None,
            Self::Unknown => Some(RcpError::SequencerNotKnown),
            Self::Denied => Some(RcpError::UnauthorizedAccess),
        }
    }
}

impl SequencerBank {
    /// Build a fresh bank sized for `svr_sequencers_max` sequencers, each
    /// initialized to the power-on default state
    /// ([`crate::regmap::SequencerStateEntry::power_on_default`]'s own
    /// already-confirmed `1`, reused here rather than re-derived).
    ///
    /// A `svr_sequencers_max` of `0` yields an empty bank — mirroring
    /// [`check_sequencer_num_in_bounds`]'s own "`0` means no sequencers
    /// exist" reading, so every [`Self::read`]/advance call against it
    /// returns `Err(RcpError::SequencerNotKnown)`. Never panics for any
    /// input.
    //fusa:req REQ-SEQ-001
    //fusa:req REQ-SEQ-005
    pub fn new(svr_sequencers_max: u8) -> Self {
        let power_on_state = SequencerState(SequencerStateEntry::power_on_default().seq_state);
        Self {
            states: vec![power_on_state; svr_sequencers_max as usize],
            owners: vec![SEQUENCER_OWNER_UNCLAIMED; svr_sequencers_max as usize],
        }
    }

    /// Read `sequencer_num`'s owner (`Request_stream_index`). Returns
    /// `Err(RcpError::SequencerNotKnown)` for an out-of-bounds index.
    /// Mirrors `rcp_sequencer_get_owner()`
    /// (`c-RCP/src/request_sequencer.c:94-100`). Never panics for any input.
    //fusa:req REQ-SEQ-013
    pub fn owner(&self, sequencer_num: u8) -> Result<u8, RcpError> {
        check_sequencer_num_in_bounds(sequencer_num, self.svr_sequencers_max())?;
        Ok(self.owners[sequencer_num as usize])
    }

    /// Overwrite `sequencer_num`'s owner ([`SEQUENCER_OWNER_UNCLAIMED`]
    /// releases it). A pure mechanism with no authorization check of its
    /// own, exactly like `rcp_sequencer_set_owner()`
    /// (`c-RCP/src/request_sequencer.c:102-108`). Returns
    /// `Err(RcpError::SequencerNotKnown)` for an out-of-bounds index, leaving
    /// the bank unchanged. Never panics for any input.
    //fusa:req REQ-SEQ-013
    pub fn set_owner(&mut self, sequencer_num: u8, owner: u8) -> Result<(), RcpError> {
        check_sequencer_num_in_bounds(sequencer_num, self.svr_sequencers_max())?;
        self.owners[sequencer_num as usize] = owner;
        Ok(())
    }

    /// The three-way access decision for `requester_stream_index` touching
    /// `sequencer_num`: [`SequencerAccess::Unknown`] for an out-of-bounds
    /// index (checked first — there is no owner to consult),
    /// [`SequencerAccess::Denied`] for an unclaimed sequencer (fail-closed)
    /// or one owned by a different stream, [`SequencerAccess::Permitted`]
    /// otherwise. Mirrors `rcp_sequencer_access_check()`
    /// (`c-RCP/src/request_sequencer.c:128-140`). Never panics for any input.
    //fusa:req REQ-SEQ-013
    //fusa:req REQ-WIREERR-005
    pub fn access_check(&self, sequencer_num: u8, requester_stream_index: u8) -> SequencerAccess {
        let Ok(owner) = self.owner(sequencer_num) else {
            return SequencerAccess::Unknown;
        };
        if owner == SEQUENCER_OWNER_UNCLAIMED || owner != requester_stream_index {
            SequencerAccess::Denied
        } else {
            SequencerAccess::Permitted
        }
    }

    /// True iff [`Self::access_check`] is [`SequencerAccess::Permitted`]
    /// (`rcp_sequencer_access_permitted()`,
    /// `c-RCP/src/request_sequencer.c:110-116`).
    //fusa:req REQ-SEQ-013
    pub fn access_permitted(&self, sequencer_num: u8, requester_stream_index: u8) -> bool {
        self.access_check(sequencer_num, requester_stream_index) == SequencerAccess::Permitted
    }

    /// The admission-time ownership gate for a compound/compound-wait
    /// request naming `step.sequencer_index`, submitted on the request
    /// stream `requester_stream_index` (1-based; `0` if unresolved).
    ///
    /// Mirrors c-RCP's dispatch-side check (`c-RCP/src/mock.c:1649-1676`):
    /// skipped entirely (`Ok(())`) when this bank has no sequencers at all
    /// (the distinct "compound operations unsupported" case); otherwise
    /// returns [`SequencerAccess::wire_error`] of [`Self::access_check`] as
    /// the error — `Err(RcpError::SequencerNotKnown)` for an unknown index,
    /// `Err(RcpError::UnauthorizedAccess)` for a sequencer this stream does
    /// not own. Never panics for any input.
    //fusa:req REQ-SEQ-013
    //fusa:req REQ-WIREERR-005
    pub fn check_compound_admission(
        &self,
        step: &CompoundStep,
        requester_stream_index: u8,
    ) -> Result<(), RcpError> {
        if self.states.is_empty() {
            return Ok(());
        }
        match self
            .access_check(step.sequencer_index, requester_stream_index)
            .wire_error()
        {
            None => Ok(()),
            Some(e) => Err(e),
        }
    }

    /// This bank's live sequencer-count bound — the `svr_sequencers_max`
    /// value it was constructed with via [`Self::new`].
    //fusa:req REQ-SEQ-001
    pub fn svr_sequencers_max(&self) -> u8 {
        // `Self::new` takes `svr_sequencers_max` as a `u8`, so `states.len()`
        // never exceeds `u8::MAX` and this cast never truncates.
        self.states.len() as u8
    }

    /// Read a sequencer's current persistent state.
    ///
    /// Returns `Err(RcpError::SequencerNotKnown)` for a `sequencer_num` at
    /// or beyond this bank's bound, reusing
    /// [`check_sequencer_num_in_bounds`]'s existing bound check rather than
    /// re-deriving it. Never panics for any input.
    //fusa:req REQ-SEQ-002
    pub fn read(&self, sequencer_num: u8) -> Result<SequencerState, RcpError> {
        check_sequencer_num_in_bounds(sequencer_num, self.svr_sequencers_max())?;
        Ok(self.states[sequencer_num as usize])
    }

    /// Attempt to advance `gate.sequencer_num`'s persistent state to
    /// `next_state`, composing [`advance_sequencer_if_still_in_start_state`]'s
    /// existing pure "advance only if still in start state" race guard
    /// against this bank's live, mutable store instead of duplicating that
    /// rule.
    ///
    /// Returns `Err(RcpError::SequencerNotKnown)` for an out-of-bounds
    /// `gate.sequencer_num`. Otherwise returns `Ok(true)` and updates this
    /// bank's stored state to `next_state` if the sequencer was still in
    /// `gate.start_state` at the moment of the attempt; returns `Ok(false)`
    /// (this bank is left unchanged) if some other request raced ahead and
    /// moved the sequencer out of that state first. Never panics for any
    /// input.
    //fusa:req REQ-SEQ-003
    pub fn advance_if_still_in_start_state(
        &mut self,
        gate: &CompoundGateConfig,
        next_state: SequencerState,
    ) -> Result<bool, RcpError> {
        check_sequencer_num_in_bounds(gate.sequencer_num, self.svr_sequencers_max())?;
        let observed_state = self.states[gate.sequencer_num as usize];
        match advance_sequencer_if_still_in_start_state(observed_state, gate, next_state) {
            Some(advanced) => {
                self.states[gate.sequencer_num as usize] = advanced;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// The full sequencer-gating check against this bank's own live state:
    /// composes [`Self::read`] with the free-function [`check_compound_gate`],
    /// finally giving it a genuine backing store to read `current_state`
    /// from instead of requiring the caller to supply it.
    ///
    /// Returns `Err(RcpError::SequencerNotKnown)` for an out-of-bounds
    /// `gate.sequencer_num`, or `Err(RcpError::RequestRejected)` if the
    /// sequencer is known but not currently in `gate.start_state`. Never
    /// panics for any input.
    //fusa:req REQ-SEQ-004
    pub fn check_compound_gate(&self, gate: &CompoundGateConfig) -> Result<(), RcpError> {
        let current_state = self.read(gate.sequencer_num)?;
        check_compound_gate(current_state, gate, self.svr_sequencers_max())
    }

    /// Unconditionally write `sequencer_num`'s persistent state to
    /// `new_state`, bypassing [`Self::advance_if_still_in_start_state`]'s
    /// start-state race guard.
    ///
    /// Added for the "Per-stream safety config" checklist bullet's
    /// sequencer-driven safe-state entry ([`enter_sequencer_driven_safe_state`]):
    /// unlike an ordinary compound-request advance, entering a safe state
    /// is not conditional on any prior observed state — it must happen
    /// regardless of what the sequencer currently holds. Returns
    /// `Err(RcpError::SequencerNotKnown)` for an out-of-bounds
    /// `sequencer_num`. Never panics for any input.
    //fusa:req REQ-SAFEMEAS-004
    pub fn force_state(
        &mut self,
        sequencer_num: u8,
        new_state: SequencerState,
    ) -> Result<(), RcpError> {
        check_sequencer_num_in_bounds(sequencer_num, self.svr_sequencers_max())?;
        self.states[sequencer_num as usize] = new_state;
        Ok(())
    }
}

// ── Triggered (0x0E): trigger_exec_delay ─────────────────────────────────────

/// The `trigger_exec_delay` execution-delay timer this checklist bullet
/// names for [`RequestKind::Triggered`].
///
/// See this module's doc comment "Provenance note: exec-delay timer width
/// and units" for why this is a plain `u32` placeholder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-TRIG-002
pub struct TriggerExecDelay(pub u32);

/// Select the execution-delay timer that applies to `kind`, if any.
///
/// Returns `Some(delay.0)` when `kind` is [`RequestKind::Triggered`] —
/// the only kind [`TriggerExecDelay`] applies to — and `None` for every
/// other [`RequestKind`], mirroring [`resolve_compound_exec_delay`]'s own
/// per-kind-timer-selection shape, including [`RequestKind::Standard`] (a
/// ninth and final variant, added alongside [`ExecutionPriorityTier`]).
/// Never panics for any input.
//fusa:req REQ-TRIG-002
//fusa:req REQ-PRIO-002
//fusa:req REQ-SAFETY-003
pub fn resolve_trigger_exec_delay(kind: RequestKind, delay: TriggerExecDelay) -> Option<u32> {
    match kind {
        RequestKind::Triggered | RequestKind::SafetyTriggered => Some(delay.0),
        RequestKind::Compound
        | RequestKind::SafetyCompound
        | RequestKind::CompoundWait
        | RequestKind::SafetyCompoundWait
        | RequestKind::Chained
        | RequestKind::Timed
        | RequestKind::ClearAll
        | RequestKind::ClearNonSafestate
        | RequestKind::ClearSingle
        | RequestKind::Standard => None,
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
//fusa:req REQ-TRIG-003
pub enum TriggerRepeatCount {
    /// A finite number of trigger occurrences this request repeats for.
    Finite(u16),
    /// The infinite-repeat sentinel (`0xFFFF`): this request's
    /// trigger-occurrence count never exhausts on its own.
    Infinite,
}

/// The raw wire value `ROADMAP.md`'s checklist bullet names as the
/// infinite-repeat sentinel for a Triggered request's occurrence count.
//fusa:req REQ-TRIG-003
pub const TRIGGER_REPEAT_COUNT_INFINITE_SENTINEL: u16 = 0xFFFF;

impl TriggerRepeatCount {
    /// Decode a raw 16-bit occurrence-count value into a
    /// [`TriggerRepeatCount`]: [`Self::Infinite`] for
    /// [`TRIGGER_REPEAT_COUNT_INFINITE_SENTINEL`], [`Self::Finite`]
    /// otherwise. Never panics for any input.
    //fusa:req REQ-TRIG-003
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
    //fusa:req REQ-TRIG-003
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
//fusa:req REQ-TRIG-004
pub fn is_trigger_repeat_exhausted(
    occurrences_so_far: u16,
    repeat_count: TriggerRepeatCount,
) -> bool {
    match repeat_count {
        TriggerRepeatCount::Infinite => false,
        TriggerRepeatCount::Finite(target) => occurrences_so_far >= target,
    }
}

// ── Triggered (0x0E/0x8E): trigger selection, occurrence counting, fire ─────

/// The infinite-repetition sentinel for a triggered request's
/// `trigger_repetitions` sub-field — the all-ones value of the 2-octet field,
/// identical in meaning to [`COMPOUND_REPEAT_INFINITE`]. Mirrors c-RCP's
/// `RCP_TRIGGERED_REPEAT_INFINITE` (`c-RCP/include/rcp/request.h:782`).
//fusa:req REQ-TRIG-014
pub const TRIGGERED_REPEAT_INFINITE: u16 = 0xFFFF;

/// A triggered request's five wire sub-fields, in wire order after the
/// leading `request_type` opcode octet of the repurposed `message_timestamp`
/// region: `trigger_source_ep` (offset 1), `trigger_signal_nr` (offset 2),
/// `trigger_threshold` (offset 3), `trigger_exec_delay` (offsets 4..5, big
/// endian) and `trigger_repetitions` (offsets 6..7, big endian).
///
/// Mirrors c-RCP's `rcp_triggered_step_t`
/// (`c-RCP/include/rcp/request.h:784-798`) field for field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-TRIG-014
pub struct TriggeredStep {
    /// The endpoint whose trigger signal this request waits on.
    pub trigger_source_ep: u8,
    /// Which of that endpoint's trigger signals (endpoint-defined numbering).
    pub trigger_signal_nr: u8,
    /// Occurrences that must *precede* execution: `0` fires on the first
    /// occurrence, `N` on the `N+1`th — see [`TriggeredRuntime::threshold_reached`].
    pub trigger_threshold: u8,
    /// `trigger_exec_delay`, counted in multiples of the addressed
    /// endpoint's configured `ep_delay_time` — not milliseconds.
    pub exec_delay: u16,
    /// `trigger_repetitions`: remaining repetitions;
    /// [`TRIGGERED_REPEAT_INFINITE`] means never decrement — see
    /// [`complete_repetition`].
    pub repeat_count: u16,
}

impl TriggeredStep {
    /// True iff `elapsed >= self.exec_delay`, both in multiples of the
    /// endpoint's configured `ep_delay_time`. Mirrors
    /// `rcp_triggered_exec_delay_elapsed()` (`c-RCP/src/request.c:535-538`).
    /// Never panics for any input.
    //fusa:req REQ-TRIG-011
    pub fn exec_delay_elapsed(&self, elapsed: u32) -> bool {
        elapsed >= u32::from(self.exec_delay)
    }
}

/// Whether one observed trigger occurrence — emitted by endpoint
/// `source_ep` as its own trigger signal number `signal_nr` — counts toward
/// `step`'s request: true iff `source_ep == step.trigger_source_ep` *and*
/// `signal_nr == step.trigger_signal_nr`.
///
/// A request waiting on one endpoint's trigger signal is never advanced by
/// a different endpoint's, nor by a different signal number of the same
/// endpoint (`c-RCP/include/rcp/request.h:291-303`). Takes no busy/idle
/// input at all: occurrence counting is independent of the target
/// endpoint's state by construction (TC18 §12.9.3); only the fire
/// transition ([`TriggeredRuntime::tick`]) is gated on `endpoint_idle`.
/// Never panics for any input.
//fusa:req REQ-TRIG-005
pub fn should_count_trigger_occurrence(step: &TriggeredStep, source_ep: u8, signal_nr: u8) -> bool {
    source_ep == step.trigger_source_ep && signal_nr == step.trigger_signal_nr
}

/// One triggered request's own runtime (not wire-carried) state: how many
/// matching trigger occurrences have been observed since entering
/// "started". Mirrors c-RCP's `rcp_triggered_runtime_t`
/// (`c-RCP/include/rcp/request.h:835-838`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-TRIG-008
pub struct TriggeredRuntime {
    /// Matching occurrences recorded since the last [`Self::enter_started`].
    pub occurrence_count: u32,
    /// Whether this request is in its "started" (counting) state.
    pub started: bool,
}

impl TriggeredRuntime {
    /// Reset to `occurrence_count == 0`, `started == true` — the "entering
    /// started" transition (`rcp_triggered_runtime_enter_started()`,
    /// `c-RCP/src/request.c:508-512`).
    //fusa:req REQ-TRIG-008
    pub fn enter_started(&mut self) {
        self.occurrence_count = 0;
        self.started = true;
    }

    /// Record one observed trigger occurrence. Increments
    /// [`Self::occurrence_count`] and returns `true` iff [`Self::started`]
    /// and [`should_count_trigger_occurrence`] holds for `(source_ep,
    /// signal_nr)`; otherwise leaves `self` unchanged and returns `false`.
    /// Mirrors `rcp_triggered_runtime_record_occurrence()`
    /// (`c-RCP/src/request.c:515-525`); the counter wraps like c-RCP's
    /// `uint32_t` increment rather than panicking. Never panics for any
    /// input.
    //fusa:req REQ-TRIG-009
    pub fn record_occurrence(
        &mut self,
        step: &TriggeredStep,
        source_ep: u8,
        signal_nr: u8,
    ) -> bool {
        if !self.started || !should_count_trigger_occurrence(step, source_ep, signal_nr) {
            return false;
        }
        self.occurrence_count = self.occurrence_count.wrapping_add(1);
        true
    }

    /// True iff `occurrence_count > step.trigger_threshold` — a threshold of
    /// `0` is satisfied by one occurrence, `N` by `N+1`
    /// (`rcp_triggered_threshold_reached()`, `c-RCP/src/request.c:528-532`).
    //fusa:req REQ-TRIG-010
    pub fn threshold_reached(&self, step: &TriggeredStep) -> bool {
        self.occurrence_count > u32::from(step.trigger_threshold)
    }

    /// The non-mutating half of [`Self::tick`]: true iff [`Self::started`],
    /// [`Self::threshold_reached`], [`TriggeredStep::exec_delay_elapsed`]
    /// and `endpoint_idle` all hold. Never panics for any input.
    //fusa:req REQ-TRIG-012
    pub fn fire_ready(&self, step: &TriggeredStep, elapsed: u32, endpoint_idle: bool) -> bool {
        self.started
            && self.threshold_reached(step)
            && step.exec_delay_elapsed(elapsed)
            && endpoint_idle
    }

    /// The fire transition: when [`Self::fire_ready`] holds, resets `self`
    /// (`occurrence_count = 0`, `started = false`) and returns `true`;
    /// otherwise leaves `self` unchanged and returns `false`. Advances no
    /// sequencer — a triggered request has none. Mirrors
    /// `rcp_triggered_tick()` (`c-RCP/src/request.c:542-553`).
    //fusa:req REQ-TRIG-012
    //fusa:req REQ-TRIG-013
    pub fn tick(&mut self, step: &TriggeredStep, elapsed: u32, endpoint_idle: bool) -> bool {
        if !self.fire_ready(step, elapsed, endpoint_idle) {
            return false;
        }
        self.occurrence_count = 0;
        self.started = false;
        true
    }
}

/// Apply the end-of-execution repetition rule to a compound, compound-wait
/// or triggered request's `repeat_count` (`cmp_repetitions`/
/// `cmpw_repetitions`/`trigger_repetitions`): the infinite sentinel
/// (`0xFFFF`) is never decremented and the request stays stored; `0` means
/// this execution was the last and the request is removed; anything else
/// is decremented by one and the request stays stored for re-arming.
///
/// Returns `true` iff the request stays in the endpoint's request storage
/// (re-arm), `false` iff it must be removed. Mirrors
/// `rcp_server_endpoint_complete()`'s repetition step
/// (`c-RCP/src/server.c:875-883`). Never panics for any input.
//fusa:req REQ-CMP-034
pub fn complete_repetition(repeat_count: &mut u16) -> bool {
    match *repeat_count {
        COMPOUND_REPEAT_INFINITE => true,
        0 => false,
        _ => {
            *repeat_count -= 1;
            true
        }
    }
}

// ── Chained (0x01): cs-bit abort-on-predecessor-error semantics ─────────────

/// The `cs`-bit abort-on-predecessor-error rule this checklist bullet names
/// for [`RequestKind::Chained`]: whether the next link in a chained request
/// runs, given that link's own decoded
/// [`crate::acf::ByteMessageInfo::cs`] flag and whether the chain's
/// preceding link errored.
///
/// Returns `Ok(())` when the chain should continue running its next link —
/// either `cs` is not set (this link's `cs`-bit abort-on-predecessor-error
/// behavior is not requested) or the preceding link did not error, so there
/// is nothing to abort on. Returns `Err(RcpError::ChainAborted)` when `cs`
/// is set and `predecessor_errored` is `true`: the predecessor errored and
/// this link's `cs`-bit requests aborting the remainder of the chain on
/// that account. Never panics for any input.
///
/// See this module's doc comment "Provenance note: the `cs` bit's
/// chained-request meaning" for why `cs` and `predecessor_errored` are both
/// caller-supplied rather than read from any live chain-execution state,
/// and "Provenance note: `CHAIN_ABORTED`/`CHAIN_ERROR` as new variants, and
/// the distinction between them" for why this function only ever
/// constructs [`RcpError::ChainAborted`], never [`RcpError::ChainError`].
//fusa:req REQ-CHAIN-002
//fusa:req REQ-ERRH-001
pub fn check_chain_continuation(cs: bool, predecessor_errored: bool) -> Result<(), RcpError> {
    if cs && predecessor_errored {
        Err(RcpError::ChainAborted)
    } else {
        Ok(())
    }
}

// ── Timed (0x0A): presentation-time execution as an alternative to a TSCF
//    header ──────────────────────────────────────────────────────────────────

/// A Timed request's own carried presentation-time execution gate — the
/// mechanism this checklist bullet names as an alternative to a TSCF
/// header's own [`crate::avtp::TscfHeader::avtp_timestamp`], for a request
/// that did not arrive framed with one.
///
/// See this module's doc comment "Provenance note: `TimedExecutionTime`'s
/// wire placement, width, and the choice to compose `AvtpTimestamp`" for
/// why this composes [`crate::timestamp::AvtpTimestamp`] rather than
/// duplicating its shape or introducing a new unconfirmed-width
/// placeholder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-TIME-002
pub struct TimedExecutionTime(pub AvtpTimestamp);

/// Whether a Timed request is ready to execute: `true` once `current` — a
/// caller-supplied reference presentation time, in the same units as
/// `exec_time` — has reached or passed `exec_time`, the request's own
/// carried [`TimedExecutionTime`].
///
/// Ordering reuses [`crate::timestamp::AvtpTimestamp::is_after`]'s
/// existing wraparound-aware comparison rather than a plain numeric `>=`,
/// so a `current` that has just rolled over past `exec_time` is still
/// correctly read as ready. An `exec_time` whose wrapped
/// [`crate::timestamp::AvtpTimestamp`] is
/// [`crate::timestamp::AvtpTimestamp::is_untimed`] (the all-zero fallback
/// [`crate::timestamp`] already established for TSCF's own
/// `avtp_timestamp`) carries no timing constraint at all, so this function
/// reads that case as always ready — mirroring the same fallback rule
/// rather than treating an untimed `exec_time` as an unreachable instant
/// far in the past or future. Never panics for any input.
//fusa:req REQ-TIME-003
pub fn is_timed_request_ready(current: AvtpTimestamp, exec_time: TimedExecutionTime) -> bool {
    if exec_time.0.is_untimed() {
        return true;
    }
    current == exec_time.0 || current.is_after(exec_time.0)
}

// ── Cancellation (0x05/0x06/0x07): clear-all / clear-non-safestate /
//    clear-single ──────────────────────────────────────────────────────────

/// The clear-all (`0x05`, mandatory) cancellation rule this checklist
/// bullet names: every pending/in-flight request considered under a
/// clear-all is canceled, unconditionally.
///
/// Always returns `Err(RcpError::RequestCanceled)`. Never panics — this
/// function takes no input to panic on. See this module's doc comment
/// "Provenance note: cancellation scope and the addressed-endpoint/stream
/// ambiguity" for what "every" is scoped to, and "Provenance note:
/// `RequestCanceled` as this item's outcome signal" for why this
/// constructs [`RcpError::RequestCanceled`] rather than
/// [`RcpError::RequestRejected`] or a new variant of its own.
//fusa:req REQ-CANCEL-002
//fusa:req REQ-ERRH-001
pub fn check_clear_all_cancellation() -> Result<(), RcpError> {
    Err(RcpError::RequestCanceled)
}

/// The clear-non-safestate (`0x06`, optional) cancellation rule this
/// checklist bullet names: every pending/in-flight request is canceled
/// *except* one that is actively driving an endpoint toward its configured
/// safe state.
///
/// Returns `Ok(())` (do not cancel) when `is_safestate_related` is `true`,
/// and `Err(RcpError::RequestCanceled)` otherwise. Never panics for any
/// input.
///
/// See this module's doc comment "Provenance note: the safe-state-driving
/// predicate as a caller-supplied parameter" for why `is_safestate_related`
/// is taken as a plain caller-supplied `bool` rather than read from this
/// crate's not-yet-built `rx_safety_measure`/safe-state machinery.
//fusa:req REQ-CANCEL-003
pub fn check_clear_non_safestate_cancellation(is_safestate_related: bool) -> Result<(), RcpError> {
    if is_safestate_related {
        Ok(())
    } else {
        Err(RcpError::RequestCanceled)
    }
}

/// A clear-single (`0x07`) cancellation's `clear_transaction_num` target: the
/// single pending request's transaction number to cancel.
///
/// See this module's doc comment "Provenance note:
/// `clear_transaction_num`'s width and matching field" for why this wraps a
/// `u8` matching [`crate::acf::ByteMessageInfo::transaction_num`]'s own
/// width.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-CANCEL-004
pub struct ClearTransactionNum(pub u8);

/// The clear-single (`0x07`, optional) cancellation rule this checklist
/// bullet names: exactly one pending request — the one whose own
/// transaction number matches `target` — is canceled.
///
/// `candidate_transaction_num` is the transaction number of the request
/// under consideration, presumed to be a request's own already-decoded
/// [`crate::acf::ByteMessageInfo::transaction_num`]. Returns
/// `Err(RcpError::RequestCanceled)` when `candidate_transaction_num` equals
/// `target.0`, and `Ok(())` (do not cancel) otherwise. Never panics for any
/// input.
//fusa:req REQ-CANCEL-004
pub fn check_clear_single_cancellation(
    candidate_transaction_num: u8,
    target: ClearTransactionNum,
) -> Result<(), RcpError> {
    if candidate_transaction_num == target.0 {
        Err(RcpError::RequestCanceled)
    } else {
        Ok(())
    }
}

// ── Execution priority ordering ──────────────────────────────────────────────

/// The seven execution-priority tiers `ROADMAP.md`'s "Execution priority
/// ordering" checklist bullet names, in the exact order it states them —
/// cancellation first (highest priority), standard last (lowest):
/// "cancellation > triggered > timed > compound > compound-wait > chained >
/// standard".
///
/// [`RequestKind`]'s three cancellation variants ([`RequestKind::ClearAll`],
/// [`RequestKind::ClearNonSafestate`], [`RequestKind::ClearSingle`]) all
/// collapse onto the single [`Self::Cancellation`] tier here — the
/// checklist names one "cancellation" priority tier, not three, even though
/// three distinct [`RequestKind`] discriminants carry it.
///
/// `derive(PartialOrd, Ord)` orders variants by declaration position, so
/// [`Self::Cancellation`] (declared first) compares as "less than" every
/// later-declared, lower-priority tier; [`select_next_pending_request`]
/// relies on that ordering directly rather than re-deriving a rank number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
//fusa:req REQ-PRIO-003
pub enum ExecutionPriorityTier {
    /// Highest priority: the cancellation trio (clear-all,
    /// clear-non-safestate, clear-single).
    Cancellation,
    /// [`RequestKind::Triggered`]'s own tier.
    Triggered,
    /// [`RequestKind::Timed`]'s own tier.
    Timed,
    /// [`RequestKind::Compound`]'s own tier.
    Compound,
    /// [`RequestKind::CompoundWait`]'s own tier.
    CompoundWait,
    /// [`RequestKind::Chained`]'s own tier.
    Chained,
    /// Lowest priority: [`RequestKind::Standard`]'s own tier.
    Standard,
}

/// Map a [`RequestKind`] to the [`ExecutionPriorityTier`] it executes
/// under, collapsing [`RequestKind`]'s twelve values down to the
/// checklist's seven named tiers. Each of the three MSB-tagged safety
/// variants ([`RequestKind::SafetyCompound`], [`RequestKind::
/// SafetyCompoundWait`], [`RequestKind::SafetyTriggered`]) maps to the
/// same tier as its own untagged base kind — see this module's doc
/// comment "Provenance note: execution-priority tier and exec-delay-timer
/// treatment for the three new variants" for why. Never panics for any
/// input.
//fusa:req REQ-PRIO-003
//fusa:req REQ-SAFETY-003
pub fn execution_priority_tier(kind: RequestKind) -> ExecutionPriorityTier {
    match kind {
        RequestKind::ClearAll | RequestKind::ClearNonSafestate | RequestKind::ClearSingle => {
            ExecutionPriorityTier::Cancellation
        }
        RequestKind::Triggered | RequestKind::SafetyTriggered => ExecutionPriorityTier::Triggered,
        RequestKind::Timed => ExecutionPriorityTier::Timed,
        RequestKind::Compound | RequestKind::SafetyCompound => ExecutionPriorityTier::Compound,
        RequestKind::CompoundWait | RequestKind::SafetyCompoundWait => {
            ExecutionPriorityTier::CompoundWait
        }
        RequestKind::Chained => ExecutionPriorityTier::Chained,
        RequestKind::Standard => ExecutionPriorityTier::Standard,
    }
}

/// One pending request's priority-ordering inputs, as
/// [`select_next_pending_request`] consumes them: its [`RequestKind`] and a
/// caller-supplied, presumed-monotonically-increasing arrival marker.
///
/// See this module's doc comment "Provenance note: arrival order as a
/// caller-supplied sequence number" for why `arrival_seq` is a plain `u64`
/// rather than this module owning a queue data structure of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
//fusa:req REQ-PRIO-004
pub struct PendingRequestKey {
    /// This pending request's kind, which [`execution_priority_tier`] maps
    /// to the tier it competes within.
    pub kind: RequestKind,
    /// This pending request's arrival marker: presumed to increase
    /// monotonically in true arrival order across the caller-supplied set
    /// [`select_next_pending_request`] is evaluated over. Not validated for
    /// monotonicity — see this module's doc comment "Provenance note:
    /// arrival order as a caller-supplied sequence number".
    pub arrival_seq: u64,
}

/// Select which of `pending`'s entries should execute next: the entry
/// whose [`execution_priority_tier`] is highest priority (lowest
/// [`ExecutionPriorityTier`] ordinal), breaking ties between same-tier
/// entries by earliest `arrival_seq` — the checklist's own "FIFO within a
/// tier" rule.
///
/// Returns the index into `pending` of the selected entry, or `None` if
/// `pending` is empty. Never panics for any input. See this module's doc
/// comment "Provenance note: what execution priority ordering does not
/// decide" for the scope and starvation/overflow questions this function
/// deliberately does not answer.
//fusa:req REQ-PRIO-004
pub fn select_next_pending_request(pending: &[PendingRequestKey]) -> Option<usize> {
    pending
        .iter()
        .enumerate()
        .min_by_key(|(_, key)| (execution_priority_tier(key.kind), key.arrival_seq))
        .map(|(index, _)| index)
}

// ── Request lifecycle state machine (§3.14) ──────────────────────────────────

/// The four-state request lifecycle `ROADMAP.md`'s "Request lifecycle state
/// machine" checklist bullet names: pending -> started -> under-execution
/// -> finalized.
///
/// Mirrors [`crate::lifecycle::RcServerState`]'s own linear-transition
/// shape, but see this module's doc comment "Provenance note:
/// `RequestLifecycleState` carries no numeric encoding" for why this type,
/// unlike [`crate::lifecycle::RcServerState`], has no `#[repr(u8)]` and no
/// `to_u8`/`from_u8` pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
//fusa:req REQ-RLC-001
pub enum RequestLifecycleState {
    /// A request has been decoded and admitted to this RC Server's pending
    /// set, but has not yet been evaluated for execution eligibility.
    Pending,
    /// A request has passed its kind-specific eligibility gate (if any —
    /// see [`RequestLifecycleGuardInput`]) but has not yet begun actively
    /// driving its target endpoint.
    Started,
    /// A request is actively driving its target endpoint.
    UnderExecution,
    /// A request has completed — either by finishing normal execution or
    /// by being force-canceled (see [`try_force_cancel_all`],
    /// [`try_force_cancel_non_safestate`], [`try_force_cancel_single`]).
    /// Terminal: no transition out of this state is defined.
    Finalized,
}

/// Whether `(from, to)` is one of the three linear forward hops this
/// checklist bullet names.
///
/// Mirrors [`crate::lifecycle::is_transition_defined`]'s own role as the
/// coarse, state-shape check performed before any type-specific guard:
/// every pair other than the three named here is `false`, including
/// staying in the same state, any backward move, and skipping a state on
/// the way up (e.g. `Pending` straight to `UnderExecution`). Never panics
/// for any input.
//fusa:req REQ-RLC-001
pub fn is_request_lifecycle_transition_defined(
    from: RequestLifecycleState,
    to: RequestLifecycleState,
) -> bool {
    matches!(
        (from, to),
        (
            RequestLifecycleState::Pending,
            RequestLifecycleState::Started
        ) | (
            RequestLifecycleState::Started,
            RequestLifecycleState::UnderExecution
        ) | (
            RequestLifecycleState::UnderExecution,
            RequestLifecycleState::Finalized
        )
    )
}

/// The type-specific inputs [`RequestLifecycleState::try_transition`]
/// consults when advancing a request along the linear pending -> started
/// -> under-execution -> finalized path — the "type-specific sub-behavior
/// at each transition" this checklist bullet names.
///
/// One variant per [`RequestKind`], each carrying exactly the fields the
/// already-built per-kind check that variant composes needs. See this
/// module's doc comment "Provenance note: which existing check applies at
/// which lifecycle hop" for this item's own working mapping from checklist
/// wording to the two guarded hops below, and for why the third hop
/// (`UnderExecution` -> `Finalized`) is unconditional for every kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
//fusa:req REQ-RLC-003
//fusa:req REQ-RLC-004
pub enum RequestLifecycleGuardInput {
    /// [`RequestKind::Standard`]: no gate at either hop.
    Standard,
    /// [`RequestKind::Chained`]: [`check_chain_continuation`] gates the
    /// `Started` -> `UnderExecution` hop.
    Chained {
        /// This link's own decoded `cs` bit.
        cs: bool,
        /// Whether the chain's preceding link errored.
        predecessor_errored: bool,
    },
    /// [`RequestKind::ClearAll`]: no gate at either hop — see
    /// [`try_force_cancel_all`] for this kind's actual type-specific
    /// behavior, which applies to a *target* request rather than to a
    /// clear-all request's own linear progression.
    ClearAll,
    /// [`RequestKind::ClearNonSafestate`]: no gate at either hop — see
    /// [`try_force_cancel_non_safestate`].
    ClearNonSafestate,
    /// [`RequestKind::ClearSingle`]: no gate at either hop — see
    /// [`try_force_cancel_single`].
    ClearSingle,
    /// [`RequestKind::Timed`]: [`is_timed_request_ready`] gates the
    /// `Pending` -> `Started` hop.
    Timed {
        /// The current presentation time.
        current: AvtpTimestamp,
        /// This request's own carried execution-time gate.
        exec_time: TimedExecutionTime,
    },
    /// [`RequestKind::CompoundWait`]: [`check_compound_gate`] gates the
    /// `Pending` -> `Started` hop.
    CompoundWait {
        /// The gated sequencer's current persistent state.
        current_sequencer_state: SequencerState,
        /// This request's own sequencer gate configuration.
        gate: CompoundGateConfig,
        /// The live sequencer-count bound `gate.sequencer_num` is checked
        /// against.
        svr_sequencers_max: u8,
    },
    /// [`RequestKind::Triggered`]: [`TriggeredRuntime::fire_ready`] (c-RCP's
    /// `rcp_triggered_tick()` predicate, `c-RCP/src/request.c:542-553`)
    /// gates the `Started` -> `UnderExecution` hop.
    Triggered {
        /// This request's own decoded trigger selection.
        step: TriggeredStep,
        /// This request's own occurrence-counting runtime state.
        runtime: TriggeredRuntime,
        /// Elapsed time since the request was armed, in multiples of the
        /// endpoint's `ep_delay_time`.
        elapsed: u32,
        /// Whether the target endpoint is idle — gates only the fire
        /// transition, never occurrence counting.
        endpoint_idle: bool,
    },
    /// [`RequestKind::Compound`]: [`check_compound_gate`] gates the
    /// `Pending` -> `Started` hop.
    Compound {
        /// The gated sequencer's current persistent state.
        current_sequencer_state: SequencerState,
        /// This request's own sequencer gate configuration.
        gate: CompoundGateConfig,
        /// The live sequencer-count bound `gate.sequencer_num` is checked
        /// against.
        svr_sequencers_max: u8,
    },
}

/// Evaluate the type-specific guard for advancing into `to`, given `input`.
///
/// Returns `Ok(())` when the hop's kind-specific gate (if any) is
/// satisfied, and the same `Err` the composed existing check itself
/// constructs otherwise — [`RcpError::SequencerNotKnown`] or
/// [`RcpError::RequestRejected`] from [`check_compound_gate`], or
/// [`RcpError::ChainAborted`] from [`check_chain_continuation`] — or a
/// fresh [`RcpError::RequestRejected`] for the two hop guards this item
/// composes from a plain `bool`-returning check ([`is_timed_request_ready`],
/// [`TriggeredRuntime::fire_ready`]) rather than one that already
/// constructs its own [`RcpError`]. Every `(to, input)` pair not named
/// below — including `to == `[`RequestLifecycleState::Finalized`] and
/// every [`RequestLifecycleGuardInput::Standard`]/`ClearAll`/
/// `ClearNonSafestate`/`ClearSingle` input at either guarded hop — is an
/// unconditional pass. Never panics for any input.
//fusa:req REQ-RLC-003
//fusa:req REQ-RLC-004
//fusa:req REQ-RLC-005
fn request_lifecycle_transition_guard(
    to: RequestLifecycleState,
    input: &RequestLifecycleGuardInput,
) -> Result<(), RcpError> {
    match (to, input) {
        (
            RequestLifecycleState::Started,
            RequestLifecycleGuardInput::Compound {
                current_sequencer_state,
                gate,
                svr_sequencers_max,
            },
        )
        | (
            RequestLifecycleState::Started,
            RequestLifecycleGuardInput::CompoundWait {
                current_sequencer_state,
                gate,
                svr_sequencers_max,
            },
        ) => check_compound_gate(*current_sequencer_state, gate, *svr_sequencers_max),
        (
            RequestLifecycleState::Started,
            RequestLifecycleGuardInput::Timed { current, exec_time },
        ) => {
            if is_timed_request_ready(*current, *exec_time) {
                Ok(())
            } else {
                Err(RcpError::RequestRejected)
            }
        }
        (
            RequestLifecycleState::UnderExecution,
            RequestLifecycleGuardInput::Chained {
                cs,
                predecessor_errored,
            },
        ) => check_chain_continuation(*cs, *predecessor_errored),
        (
            RequestLifecycleState::UnderExecution,
            RequestLifecycleGuardInput::Triggered {
                step,
                runtime,
                elapsed,
                endpoint_idle,
            },
        ) => {
            if runtime.fire_ready(step, *elapsed, *endpoint_idle) {
                Ok(())
            } else {
                Err(RcpError::RequestRejected)
            }
        }
        _ => Ok(()),
    }
}

impl RequestLifecycleState {
    /// Attempt to move a request from its current lifecycle state (`self`)
    /// to `target`, applying `input`'s type-specific guard for hops that
    /// have one.
    ///
    /// Returns `Err(RcpError::RequestRejected)` immediately for any
    /// `(self, target)` pair [`is_request_lifecycle_transition_defined`]
    /// does not name — mirroring
    /// [`crate::lifecycle::RcServerState::try_transition`]'s own coarse
    /// shape check ahead of any guard. For a defined hop, delegates to
    /// [`request_lifecycle_transition_guard`]; on success returns
    /// `Ok(target)`, on failure returns whatever `Err` that guard
    /// constructed. This method takes `self` by value and returns the
    /// *new* state on success — it does not mutate anything in place,
    /// matching `RcServerState::try_transition`'s own non-mutating
    /// convention. Never panics for any input.
    ///
    /// This diverges from `RcServerState::try_transition`'s plain
    /// `impl FnOnce() -> bool` guard closure: this checklist bullet's own
    /// "type-specific sub-behavior at each transition" wording means a
    /// request's guard is type-specific, so `input` carries kind-aware
    /// data rather than a single opaque predicate — see
    /// [`RequestLifecycleGuardInput`].
    //fusa:req REQ-RLC-002
    pub fn try_transition(
        self,
        target: Self,
        input: &RequestLifecycleGuardInput,
    ) -> Result<Self, RcpError> {
        if !is_request_lifecycle_transition_defined(self, target) {
            return Err(RcpError::RequestRejected);
        }
        request_lifecycle_transition_guard(target, input)?;
        Ok(target)
    }
}

// ── Cancellation trio: force-canceling a target request out of its normal
//    linear progression ───────────────────────────────────────────────────

/// Force a target request out of its normal linear lifecycle progression
/// and straight to [`RequestLifecycleState::Finalized`], per a clear-all
/// (`0x05`, mandatory) cancellation — the cancellation trio's own
/// type-specific lifecycle behavior, applied to a request other than
/// itself (see this module's doc comment "in scope" list).
///
/// A `current` already at [`RequestLifecycleState::Finalized`] is left
/// unchanged and this returns `Ok(())` — finalized is terminal, and a
/// second cancellation of an already-finished request has nothing left to
/// cancel. Otherwise, delegates to [`check_clear_all_cancellation`]: since
/// that check is mandatory and unconditional, `*current` always becomes
/// `Finalized` and `Err(RcpError::RequestCanceled)` is always returned.
/// Never panics for any input.
//fusa:req REQ-RLC-006
pub fn try_force_cancel_all(current: &mut RequestLifecycleState) -> Result<(), RcpError> {
    if *current == RequestLifecycleState::Finalized {
        return Ok(());
    }
    match check_clear_all_cancellation() {
        Ok(()) => Ok(()),
        Err(err) => {
            *current = RequestLifecycleState::Finalized;
            Err(err)
        }
    }
}

/// The clear-non-safestate (`0x06`, optional) analog of
/// [`try_force_cancel_all`]: force-cancels `current` unless
/// `is_safestate_related` is `true`, delegating to
/// [`check_clear_non_safestate_cancellation`]. A `current` already at
/// [`RequestLifecycleState::Finalized`] is left unchanged. Never panics
/// for any input.
//fusa:req REQ-RLC-006
pub fn try_force_cancel_non_safestate(
    current: &mut RequestLifecycleState,
    is_safestate_related: bool,
) -> Result<(), RcpError> {
    if *current == RequestLifecycleState::Finalized {
        return Ok(());
    }
    match check_clear_non_safestate_cancellation(is_safestate_related) {
        Ok(()) => Ok(()),
        Err(err) => {
            *current = RequestLifecycleState::Finalized;
            Err(err)
        }
    }
}

/// The clear-single (`0x07`, optional) analog of [`try_force_cancel_all`]:
/// force-cancels `current` only if `candidate_transaction_num` matches
/// `target`, delegating to [`check_clear_single_cancellation`]. A
/// `current` already at [`RequestLifecycleState::Finalized`] is left
/// unchanged. Never panics for any input.
//fusa:req REQ-RLC-006
pub fn try_force_cancel_single(
    current: &mut RequestLifecycleState,
    candidate_transaction_num: u8,
    target: ClearTransactionNum,
) -> Result<(), RcpError> {
    if *current == RequestLifecycleState::Finalized {
        return Ok(());
    }
    match check_clear_single_cancellation(candidate_transaction_num, target) {
        Ok(()) => Ok(()),
        Err(err) => {
            *current = RequestLifecycleState::Finalized;
            Err(err)
        }
    }
}

// ── Feature-bundle gating ────────────────────────────────────────────────────

/// The minimum sequencer count [`check_compound_bundle_claim`] requires
/// before a "compound request support" claim is honest — `ROADMAP.md`'s
/// "Feature-bundle gating" checklist bullet's own stated "≥4 sequencers"
/// threshold.
pub const MIN_SEQUENCERS_FOR_COMPOUND_BUNDLE: u8 = 4;

/// The feature-bundle gating rule `ROADMAP.md`'s "Feature-bundle gating"
/// checklist bullet names: honestly claiming the "compound request
/// support" optional-feature bundle — the bit
/// [`crate::regmap::GeneralRegisters::claims_compound_wait_bundle`] reads —
/// requires shipping compound-wait support, a sequencer bank sized for at
/// least [`MIN_SEQUENCERS_FOR_COMPOUND_BUNDLE`] sequencers, *and*
/// clear-non-safestate cancellation support, all three together, not
/// compound-message parsing (i.e. [`RequestKind::Compound`]/
/// [`RequestKind::CompoundWait`] decoding alone) by itself.
///
/// `has_compound_wait` and `has_clear_non_safestate` are presumed to
/// already reflect whether this implementation actually executes
/// [`RequestKind::CompoundWait`] (via [`check_compound_gate`]/
/// [`resolve_compound_exec_delay`]) and honors clear-non-safestate (via
/// [`check_clear_non_safestate_cancellation`]) respectively, not merely
/// whether it can decode the corresponding wire bytes.
/// `svr_sequencers_max` is presumed to already be the same bound
/// [`SequencerBank::new`]/[`check_sequencer_num_in_bounds`] enforce
/// elsewhere in this module (mirroring
/// [`crate::regmap::GeneralRegisters::svr_sequencers_max`]).
///
/// Returns `Ok(())` only when all three hold, and
/// `Err(RcpError::InvalidParameter)` if any one is missing — including
/// when all three are false, and when `svr_sequencers_max` is nonzero but
/// still below [`MIN_SEQUENCERS_FOR_COMPOUND_BUNDLE`]. Never panics for
/// any input.
///
/// See this module's doc comment "Provenance note: the compound-bundle
/// gate as three caller-supplied facts, not a read `GeneralRegisters`" for
/// why these three facts are taken as plain caller-supplied values rather
/// than read from a live [`crate::regmap::GeneralRegisters`], and
/// "Provenance note: `InvalidParameter` as the compound-bundle gate's
/// rejection code" for the error-code choice.
//fusa:req REQ-BUNDLE-001
//fusa:req REQ-BUNDLE-002
pub fn check_compound_bundle_claim(
    has_compound_wait: bool,
    svr_sequencers_max: u8,
    has_clear_non_safestate: bool,
) -> Result<(), RcpError> {
    if has_compound_wait
        && svr_sequencers_max >= MIN_SEQUENCERS_FOR_COMPOUND_BUNDLE
        && has_clear_non_safestate
    {
        Ok(())
    } else {
        Err(RcpError::InvalidParameter)
    }
}

// ── Safety-request MSB-tagging (0x8F/0x8B/0x8E) & watchdog-overflow purge ────

/// The watchdog-overflow purge rule this checklist bullet names, evaluated
/// for a single request: on watchdog overflow, a normal-priority
/// (non-safety-tagged) request is purged, while a safety-tagged request
/// ([`RequestKind::is_safety_tagged`]) is exempt and remains queued.
///
/// Returns `Ok(())` (do not purge) when `watchdog_overflowed` is `false`
/// (no overflow, nothing to purge) or when `kind.is_safety_tagged()` is
/// `true` (exempt regardless of overflow state), and
/// `Err(RcpError::RequestCanceled)` — the same outcome signal the
/// cancellation trio already constructs, see this module's doc comment
/// "Provenance note: `RequestCanceled` as this item's outcome signal" —
/// only when `watchdog_overflowed` is `true` and `kind` is not
/// safety-tagged. Never panics for any input.
//fusa:req REQ-SAFETY-004
pub fn check_watchdog_overflow_purge(
    kind: RequestKind,
    watchdog_overflowed: bool,
) -> Result<(), RcpError> {
    if watchdog_overflowed && !kind.is_safety_tagged() {
        Err(RcpError::RequestCanceled)
    } else {
        Ok(())
    }
}

/// Partition `pending`'s entries by [`check_watchdog_overflow_purge`]:
/// which stay queued, and which are purged, on a watchdog overflow.
///
/// Returns `(kept, purged)`, each a `Vec` of indices into `pending`, in
/// `pending`'s own original relative order. When `watchdog_overflowed` is
/// `false`, every index lands in `kept` and `purged` is empty. When
/// `watchdog_overflowed` is `true`, an entry lands in `kept` iff its own
/// [`PendingRequestKey::kind`] is safety-tagged
/// ([`RequestKind::is_safety_tagged`]), and in `purged` otherwise. Returns
/// `(vec![], vec![])` for an empty `pending`. Never panics for any input.
///
/// Composes, rather than re-deriving, [`PendingRequestKey`] (Milestone 5's
/// "Execution priority ordering" pending-request record) and
/// [`check_watchdog_overflow_purge`] above — mirrors
/// [`select_next_pending_request`]'s own "pure function over a
/// caller-supplied slice" shape rather than owning a queue of its own. See
/// this module's doc comment "Safety-request MSB-tagging &
/// watchdog-overflow purge" section for how a kept-queued safety-tagged
/// request composes with the rest of this crate's already-built
/// pending-request machinery.
//fusa:req REQ-SAFETY-005
pub fn purge_normal_priority_on_watchdog_overflow(
    pending: &[PendingRequestKey],
    watchdog_overflowed: bool,
) -> (Vec<usize>, Vec<usize>) {
    let mut kept = Vec::new();
    let mut purged = Vec::new();
    for (index, key) in pending.iter().enumerate() {
        match check_watchdog_overflow_purge(key.kind, watchdog_overflowed) {
            Ok(()) => kept.push(index),
            Err(_) => purged.push(index),
        }
    }
    (kept, purged)
}

// ── Per-stream safety config: rx_enforce_e2e ─────────────────────────────────

/// The consequence scope of an E2E-CRC failure on one request within a
/// stream, selected by `rx_enforce_e2e`: see [`e2e_failure_scope`]/
/// [`check_rx_enforce_e2e`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
//fusa:req REQ-E2EENF-001
pub enum E2eFailureScope {
    /// Drop the one bad request; the rest of the stream is unaffected.
    DropRequest,
    /// Latch the whole stream into a fault/safe state until explicitly
    /// released.
    LatchStream,
}

/// Select the [`E2eFailureScope`] `rx_enforce_e2e` names: [`Self::LatchStream`]
/// when `rx_enforce_e2e` is `true`, [`Self::DropRequest`] otherwise. Never
/// panics for any input.
//fusa:req REQ-E2EENF-001
pub fn e2e_failure_scope(rx_enforce_e2e: bool) -> E2eFailureScope {
    if rx_enforce_e2e {
        E2eFailureScope::LatchStream
    } else {
        E2eFailureScope::DropRequest
    }
}

/// The full `rx_enforce_e2e` rule: compare a computed CRC-32
/// ([`crate::e2e::crc32_tc18`], run over `coverage_buffer`) against
/// `expected_crc`, and — on mismatch — report [`e2e_failure_scope`]'s
/// scope alongside the failure.
///
/// Returns `Ok(())` when the computed and expected CRCs match. Returns
/// `Err((RcpError::CrcError, scope))` on mismatch, where `scope` is
/// [`e2e_failure_scope(rx_enforce_e2e)`](e2e_failure_scope). Never panics
/// for any input; `coverage_buffer` is presumed built by
/// [`crate::e2e::build_crc32_coverage_buffer`] or
/// [`crate::e2e::build_crc32_coverage_buffer_for_fragment_train`], but
/// this function does not itself validate that provenance.
///
/// Constructs [`RcpError::CrcError`] — the dedicated TC18 safe-point
/// sentinel — rather than the legacy `RcpError::CrcMismatch` an earlier
/// revision of this function provisionally reused; `CrcMismatch` and the
/// CRC-16 `wrap`/`unwrap` path it once paired with have both since been
/// removed by Milestone 9's `e2e` REPLACE cutover. See this module's doc
/// comment "Provenance note: `CrcError` as a new variant, distinct from the
/// legacy `CrcMismatch` sentinel" for the full history (`ROADMAP.md`
/// Milestone 6, "`CRC_ERROR` error path").
//fusa:req REQ-E2EENF-002
//fusa:req REQ-CRC-011
pub fn check_rx_enforce_e2e(
    coverage_buffer: &[u8],
    expected_crc: u32,
    rx_enforce_e2e: bool,
) -> Result<(), (RcpError, E2eFailureScope)> {
    let computed_crc = crate::e2e::crc32_tc18(coverage_buffer);
    if computed_crc == expected_crc {
        Ok(())
    } else {
        Err((RcpError::CrcError, e2e_failure_scope(rx_enforce_e2e)))
    }
}

// ── Per-stream safety config: rx_safety_measure / rx_safestate_sequencer /
//    rx_safe_sequencer_state ───────────────────────────────────────────────

/// The safe-state mechanism a stream uses when driven to safe state,
/// selected by `rx_safety_measure`: see [`resolve_safe_state_mechanism`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
//fusa:req REQ-SAFEMEAS-001
pub enum SafeStateMechanism {
    /// Force every I/O pin on the stream's endpoints to high-impedance.
    HiZAllPins,
    /// Run a configured sequencer-based safety request sequence, gated by
    /// the wrapped [`CompoundGateConfig`] — see [`safe_state_sequencer_gate`].
    SequencerDriven(CompoundGateConfig),
}

/// Build the [`CompoundGateConfig`] identifying a stream's sequencer-driven
/// safe-state trigger from `rx_safestate_sequencer`/`rx_safe_sequencer_state`.
///
/// See this module's doc comment "Provenance note: the sequencer-driven
/// safe state as a gate write, not a new mechanism" for why this reuses
/// [`CompoundGateConfig`] rather than a safe-state-only type. Never panics
/// for any input.
//fusa:req REQ-SAFEMEAS-002
pub fn safe_state_sequencer_gate(
    rx_safestate_sequencer: u8,
    rx_safe_sequencer_state: u8,
) -> CompoundGateConfig {
    CompoundGateConfig {
        sequencer_num: rx_safestate_sequencer,
        start_state: SequencerState(rx_safe_sequencer_state),
    }
}

/// Select the [`SafeStateMechanism`] `rx_safety_measure` names:
/// [`SafeStateMechanism::SequencerDriven`] (wrapping
/// [`safe_state_sequencer_gate`]'s result) when `rx_safety_measure` is
/// `true`, [`SafeStateMechanism::HiZAllPins`] otherwise. Never panics for
/// any input.
//fusa:req REQ-SAFEMEAS-001
pub fn resolve_safe_state_mechanism(
    rx_safety_measure: bool,
    rx_safestate_sequencer: u8,
    rx_safe_sequencer_state: u8,
) -> SafeStateMechanism {
    if rx_safety_measure {
        SafeStateMechanism::SequencerDriven(safe_state_sequencer_gate(
            rx_safestate_sequencer,
            rx_safe_sequencer_state,
        ))
    } else {
        SafeStateMechanism::HiZAllPins
    }
}

/// Enter a sequencer-driven safe state: unconditionally write `gate`'s
/// sequencer to `gate`'s target state, via [`SequencerBank::force_state`]
/// rather than the race-guarded [`SequencerBank::
/// advance_if_still_in_start_state`].
///
/// Returns `Err(RcpError::SequencerNotKnown)` for an out-of-bounds
/// `gate.sequencer_num`. Never panics for any input.
//fusa:req REQ-SAFEMEAS-003
pub fn enter_sequencer_driven_safe_state(
    bank: &mut SequencerBank,
    gate: &CompoundGateConfig,
) -> Result<(), RcpError> {
    bank.force_state(gate.sequencer_num, gate.start_state)
}

// ── Per-stream safety config: rx_ovrflw_safestate_enable ────────────────────

/// The result of evaluating a request-storage overflow against
/// `rx_ovrflw_safestate_enable`: see [`evaluate_request_storage_overflow`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
//fusa:req REQ-OVRFLW-001
pub enum OverflowOutcome {
    /// No overflow occurred.
    NoOverflow,
    /// Storage overflowed, and `rx_ovrflw_safestate_enable` is `false` — no
    /// safe-state consequence configured.
    OverflowNoSafestate,
    /// Storage overflowed, and `rx_ovrflw_safestate_enable` is `true` —
    /// drive every endpoint on this stream to safe state.
    OverflowSafestate,
}

impl OverflowOutcome {
    /// True for either overflow variant. Never panics for any input.
    //fusa:req REQ-OVRFLW-002
    pub fn is_overflow(&self) -> bool {
        !matches!(self, Self::NoOverflow)
    }

    /// True only for [`Self::OverflowSafestate`]. Never panics for any
    /// input.
    //fusa:req REQ-OVRFLW-002
    pub fn drives_safestate(&self) -> bool {
        matches!(self, Self::OverflowSafestate)
    }
}

/// The full `rx_ovrflw_safestate_enable` rule: whether a request-storage
/// overflow ([`RcpError::ReqStorageOvfl`]'s own already-established
/// condition) also drives the stream's endpoints to safe state.
///
/// Returns [`OverflowOutcome::NoOverflow`] when `storage_overflowed` is
/// `false`, regardless of `rx_ovrflw_safestate_enable`. Otherwise returns
/// [`OverflowOutcome::OverflowSafestate`] or
/// [`OverflowOutcome::OverflowNoSafestate`], selected by
/// `rx_ovrflw_safestate_enable`. Never panics for any input.
//fusa:req REQ-OVRFLW-003
pub fn evaluate_request_storage_overflow(
    storage_overflowed: bool,
    rx_ovrflw_safestate_enable: bool,
) -> OverflowOutcome {
    if !storage_overflowed {
        OverflowOutcome::NoOverflow
    } else if rx_ovrflw_safestate_enable {
        OverflowOutcome::OverflowSafestate
    } else {
        OverflowOutcome::OverflowNoSafestate
    }
}

// ── Per-stream safety config: rx_enforce_seq / rx_seq_safestate_enable ──────

/// The result of evaluating one received frame's AVTPDU `sequence_num`
/// against a stream's [`SequenceTracker`] and its
/// `rx_enforce_seq`/`rx_seq_safestate_enable` configuration: see
/// [`SequenceTracker::evaluate`]. Mirrors c-RCP's `rcp_e2e_seq_result_t`
/// (`c-RCP/include/rcp/e2e.h:816-822`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
//fusa:req REQ-SEQENF-001
pub struct SequenceEnforcementOutcome {
    /// The frame's requests may be filed for execution.
    pub accept: bool,
    /// `sequence_num` did not advance by exactly one from the previously
    /// accepted value (never `true` on a stream's first frame).
    pub discontinuity: bool,
    /// `discontinuity && rx_seq_safestate_enable`: drive every endpoint on
    /// this stream to its safe state.
    pub enter_safe_state: bool,
}

impl SequenceEnforcementOutcome {
    /// True iff the frame is rejected (`!accept`). Never panics.
    //fusa:req REQ-SEQENF-002
    pub fn is_rejected(&self) -> bool {
        !self.accept
    }

    /// True iff this outcome drives the stream to safe state
    /// (`enter_safe_state`) — independent of [`Self::is_rejected`]: an
    /// accepted frame with a gap still drives safe state. Never panics.
    //fusa:req REQ-SEQENF-002
    pub fn drives_safestate(&self) -> bool {
        self.enter_safe_state
    }
}

/// Caller-owned per-request-stream sequence tracker: the "previously
/// accepted request on this stream" state TC18 §12.7.7 Table 24's
/// `rx_enforce_seq`/`rx_seq_safestate_enable` are defined against. One
/// instance per configured request stream; call [`Self::evaluate`] once per
/// received frame (not once per ACF message inside it), as c-RCP's
/// `frame_seq_gate_admits()` does (`c-RCP/src/mock.c:3038`).
///
/// Mirrors c-RCP's `rcp_e2e_seq_tracker_t`
/// (`c-RCP/include/rcp/e2e.h:802-811`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-E2E-028
pub struct SequenceTracker {
    has_prev: bool,
    prev_seq: u8,
}

impl SequenceTracker {
    /// A fresh tracker with no previously accepted frame
    /// (`rcp_e2e_seq_tracker_init()`).
    //fusa:req REQ-E2E-028
    pub fn new() -> Self {
        Self::default()
    }

    /// The last accepted `sequence_num`, or `None` before the first frame.
    //fusa:req REQ-E2E-028
    pub fn prev_seq(&self) -> Option<u8> {
        self.has_prev.then_some(self.prev_seq)
    }

    /// Evaluate one received frame's 8-bit AVTPDU `sequence_num`.
    ///
    /// The first frame on a stream always accepts and is never a
    /// discontinuity. Afterwards, with `fwd = seq.wrapping_sub(prev_seq)`
    /// (forward distance mod 256): `accept = !rx_enforce_seq || (1..=127)
    /// .contains(&fwd)` (RFC 1982 serial-number window, so 255 -> 0 is a
    /// genuine advance); `discontinuity = fwd != 1`; `enter_safe_state =
    /// discontinuity && rx_seq_safestate_enable`. The tracked value advances
    /// only on accept, so a rejected (stale/replayed) frame never drags the
    /// reference point backward. Mirrors `rcp_e2e_seq_evaluate()`
    /// (`c-RCP/src/e2e.c:516-555`). Never panics for any input.
    ///
    /// c-RCP parity: `c-RCP/src/e2e.c:543-544` raises `enter_safe_state` on
    /// *any* discontinuity — a forward gap (an accepted frame), a duplicate
    /// or a reorder alike, and regardless of `rx_enforce_seq` — treating
    /// `rx_seq_safestate_enable` as an independent bit. TC18 RC5 §12.7.7
    /// Table 24 describes a single `rx_enforce_sequence` bit ("1b: stream
    /// is blocked upon out of order reception and Safe state will be
    /// entered"), i.e. safe state only on an out-of-order *rejection*. This
    /// crate copies c-RCP's behaviour for byte-for-byte interop.
    //fusa:req REQ-E2E-028
    //fusa:req REQ-E2E-029
    pub fn evaluate(
        &mut self,
        rx_enforce_seq: bool,
        rx_seq_safestate_enable: bool,
        seq: u8,
    ) -> SequenceEnforcementOutcome {
        if !self.has_prev {
            self.has_prev = true;
            self.prev_seq = seq;
            return SequenceEnforcementOutcome {
                accept: true,
                discontinuity: false,
                enter_safe_state: false,
            };
        }
        let fwd_distance = seq.wrapping_sub(self.prev_seq);
        let accept = !rx_enforce_seq || (1..=127).contains(&fwd_distance);
        let discontinuity = fwd_distance != 1;
        if accept {
            self.prev_seq = seq;
        }
        SequenceEnforcementOutcome {
            accept,
            discontinuity,
            enter_safe_state: discontinuity && rx_seq_safestate_enable,
        }
    }
}

/// The full `rx_enforce_seq`/`rx_seq_safestate_enable` rule as a free
/// function over a stream's [`SequenceTracker`]: identical to
/// [`SequenceTracker::evaluate`] (`rcp_e2e_seq_evaluate()`,
/// `c-RCP/src/e2e.c:516`). Never panics for any input.
//fusa:req REQ-SEQENF-003
pub fn evaluate_rx_enforce_seq(
    tracker: &mut SequenceTracker,
    rx_enforce_seq: bool,
    rx_seq_safestate_enable: bool,
    seq: u8,
) -> SequenceEnforcementOutcome {
    tracker.evaluate(rx_enforce_seq, rx_seq_safestate_enable, seq)
}

// ── Per-stream safety config: the unifying safe-state action ────────────────

/// The concrete action a caller should take once some rule above has
/// decided a stream should enter safe state: see
/// [`resolve_safe_state_action`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
//fusa:req REQ-SAFEACT-001
pub enum SafeStateAction {
    /// No safe-state entry is called for.
    None,
    /// Force every I/O pin on the stream's endpoints to high-impedance.
    ForceHiZAllPins,
    /// Drive the wrapped sequencer/target-state gate — see
    /// [`enter_sequencer_driven_safe_state`].
    ForceSequencerState(CompoundGateConfig),
}

/// Combine "should this stream enter safe state right now"
/// (`should_enter_safe_state` — the caller-computed OR of, e.g.,
/// [`crate::watchdog::StreamWatchdogOutcome::drives_safestate`],
/// [`OverflowOutcome::drives_safestate`],
/// [`SequenceEnforcementOutcome::drives_safestate`], or an
/// [`E2eFailureScope::LatchStream`] scope) with a resolved
/// [`SafeStateMechanism`] into one concrete [`SafeStateAction`].
///
/// Returns [`SafeStateAction::None`] when `should_enter_safe_state` is
/// `false`, regardless of `mechanism`. Otherwise returns
/// [`SafeStateAction::ForceHiZAllPins`] or
/// [`SafeStateAction::ForceSequencerState`], mirroring `mechanism`. Never
/// panics for any input.
//fusa:req REQ-SAFEACT-002
pub fn resolve_safe_state_action(
    should_enter_safe_state: bool,
    mechanism: SafeStateMechanism,
) -> SafeStateAction {
    if !should_enter_safe_state {
        return SafeStateAction::None;
    }
    match mechanism {
        SafeStateMechanism::HiZAllPins => SafeStateAction::ForceHiZAllPins,
        SafeStateMechanism::SequencerDriven(gate) => SafeStateAction::ForceSequencerState(gate),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── RequestKind: discriminant round-trip / rejection ────────────────────

    const ALL_REQUEST_KINDS: [RequestKind; 12] = [
        RequestKind::Standard,
        RequestKind::Chained,
        RequestKind::ClearAll,
        RequestKind::ClearNonSafestate,
        RequestKind::ClearSingle,
        RequestKind::Timed,
        RequestKind::CompoundWait,
        RequestKind::Triggered,
        RequestKind::Compound,
        RequestKind::SafetyCompoundWait,
        RequestKind::SafetyTriggered,
        RequestKind::SafetyCompound,
    ];

    #[test]
    //fusa:test REQ-CMP-001
    //fusa:test REQ-TRIG-001
    //fusa:test REQ-CHAIN-001
    //fusa:test REQ-TIME-001
    //fusa:test REQ-CANCEL-001
    //fusa:test REQ-PRIO-001
    //fusa:test REQ-SAFETY-001
    fn request_kind_round_trips_through_to_u8_from_u8() {
        for kind in ALL_REQUEST_KINDS {
            assert_eq!(RequestKind::from_u8(kind.to_u8()), Ok(kind));
        }
    }

    #[test]
    //fusa:test REQ-CMP-001
    //fusa:test REQ-TRIG-001
    //fusa:test REQ-CHAIN-001
    //fusa:test REQ-TIME-001
    //fusa:test REQ-CANCEL-001
    //fusa:test REQ-PRIO-001
    //fusa:test REQ-SAFETY-001
    fn request_kind_discriminants_match_roadmap_named_values() {
        assert_eq!(RequestKind::Compound.to_u8(), 0x0F);
        assert_eq!(RequestKind::CompoundWait.to_u8(), 0x0B);
        assert_eq!(RequestKind::Triggered.to_u8(), 0x0E);
        assert_eq!(RequestKind::Chained.to_u8(), 0x01);
        assert_eq!(RequestKind::Timed.to_u8(), 0x0A);
        assert_eq!(RequestKind::ClearAll.to_u8(), 0x05);
        assert_eq!(RequestKind::ClearNonSafestate.to_u8(), 0x06);
        assert_eq!(RequestKind::ClearSingle.to_u8(), 0x07);
        // Standard's 0x00 is this crate's own crate-local placeholder, not a
        // roadmap-named value — see this module's doc comment "Provenance
        // note: `RequestKind::Standard`'s discriminant". Asserted here
        // anyway so an accidental future renumbering is caught by this
        // test suite.
        assert_eq!(RequestKind::Standard.to_u8(), 0x00);
        assert_eq!(RequestKind::SafetyCompound.to_u8(), 0x8F);
        assert_eq!(RequestKind::SafetyCompoundWait.to_u8(), 0x8B);
        assert_eq!(RequestKind::SafetyTriggered.to_u8(), 0x8E);
    }

    #[test]
    //fusa:test REQ-SAFETY-001
    fn request_kind_safety_variants_are_exactly_0x80_or_their_base_kind() {
        assert_eq!(
            RequestKind::SafetyCompound.to_u8(),
            0x80 | RequestKind::Compound.to_u8()
        );
        assert_eq!(
            RequestKind::SafetyCompoundWait.to_u8(),
            0x80 | RequestKind::CompoundWait.to_u8()
        );
        assert_eq!(
            RequestKind::SafetyTriggered.to_u8(),
            0x80 | RequestKind::Triggered.to_u8()
        );
    }

    #[test]
    //fusa:test REQ-CMP-002
    //fusa:test REQ-SAFETY-001
    fn request_kind_from_u8_rejects_every_other_value() {
        for raw in [
            0x02u8, 0x03, 0x04, 0x08, 0x0C, 0x10, 0x7F, 0x80, 0x8A, 0x8C, 0x8D, 0xFF,
        ] {
            assert_eq!(RequestKind::from_u8(raw), Err(RcpError::InvalidParameter));
        }
    }

    #[test]
    //fusa:test REQ-TRIG-001
    fn request_kind_from_u8_accepts_triggered_discriminant() {
        assert_eq!(RequestKind::from_u8(0x0E), Ok(RequestKind::Triggered));
    }

    #[test]
    //fusa:test REQ-PRIO-001
    fn request_kind_from_u8_accepts_standard_discriminant() {
        assert_eq!(RequestKind::from_u8(0x00), Ok(RequestKind::Standard));
    }

    #[test]
    //fusa:test REQ-CHAIN-001
    fn request_kind_from_u8_accepts_chained_discriminant() {
        assert_eq!(RequestKind::from_u8(0x01), Ok(RequestKind::Chained));
    }

    #[test]
    //fusa:test REQ-TIME-001
    fn request_kind_from_u8_accepts_timed_discriminant() {
        assert_eq!(RequestKind::from_u8(0x0A), Ok(RequestKind::Timed));
    }

    #[test]
    //fusa:test REQ-CANCEL-001
    fn request_kind_from_u8_accepts_all_three_cancellation_discriminants() {
        assert_eq!(RequestKind::from_u8(0x05), Ok(RequestKind::ClearAll));
        assert_eq!(
            RequestKind::from_u8(0x06),
            Ok(RequestKind::ClearNonSafestate)
        );
        assert_eq!(RequestKind::from_u8(0x07), Ok(RequestKind::ClearSingle));
    }

    #[test]
    //fusa:test REQ-CMP-002
    fn request_kind_from_u8_never_panics_across_the_full_byte_range() {
        for raw in 0u8..=255 {
            let _ = RequestKind::from_u8(raw);
        }
    }

    // ── RequestKind: GBB message_timestamp wire binding ──────────────────────

    const ALL_NON_STANDARD_REQUEST_KINDS: [RequestKind; 11] = [
        RequestKind::Chained,
        RequestKind::ClearAll,
        RequestKind::ClearNonSafestate,
        RequestKind::ClearSingle,
        RequestKind::Timed,
        RequestKind::CompoundWait,
        RequestKind::Triggered,
        RequestKind::Compound,
        RequestKind::SafetyCompoundWait,
        RequestKind::SafetyTriggered,
        RequestKind::SafetyCompound,
    ];

    #[test]
    //fusa:test REQ-CMP-008
    fn request_kind_gbb_message_timestamp_round_trips_for_every_non_standard_kind() {
        for kind in ALL_NON_STANDARD_REQUEST_KINDS {
            for message_timestamp in [0u64, u64::MAX, 0x0011_2233_4455_6677] {
                let encoded = kind.to_gbb_message_timestamp(message_timestamp).unwrap();
                assert_eq!(RequestKind::from_gbb_message_timestamp(encoded), Some(kind));
            }
        }
    }

    #[test]
    //fusa:test REQ-CMP-008
    fn request_kind_gbb_message_timestamp_encode_preserves_low_56_bits() {
        let message_timestamp = 0x00AA_BBCC_DDEE_FF11;
        let encoded = RequestKind::Compound
            .to_gbb_message_timestamp(message_timestamp)
            .unwrap();
        assert_eq!(encoded & 0x00FF_FFFF_FFFF_FFFF, 0x00AA_BBCC_DDEE_FF11);
        assert_eq!(encoded >> 56, RequestKind::Compound.to_u8() as u64);
    }

    #[test]
    //fusa:test REQ-CMP-008
    fn request_kind_gbb_message_timestamp_encode_rejects_standard() {
        assert_eq!(
            RequestKind::Standard.to_gbb_message_timestamp(0),
            Err(RcpError::InvalidParameter)
        );
        assert_eq!(
            RequestKind::Standard.to_gbb_message_timestamp(u64::MAX),
            Err(RcpError::InvalidParameter)
        );
    }

    #[test]
    //fusa:test REQ-CMP-008
    fn request_kind_gbb_message_timestamp_decode_never_returns_standard() {
        // A leading byte of 0x00 is ambiguous between "genuinely a standard
        // request" and "a conditional request whose timestamp coincidentally
        // has a zero leading byte" -- see this module's doc comment
        // "Provenance note: `RequestKind::Standard`'s discriminant". Neither
        // is asserted; this must decode to `None`, never
        // `Some(RequestKind::Standard)`.
        assert_eq!(RequestKind::from_gbb_message_timestamp(0), None);
        assert_eq!(
            RequestKind::from_gbb_message_timestamp(0x00FF_FFFF_FFFF_FFFF),
            None
        );
        for raw in 0u8..=255 {
            let ts = (raw as u64) << 56;
            assert_ne!(
                RequestKind::from_gbb_message_timestamp(ts),
                Some(RequestKind::Standard)
            );
        }
    }

    #[test]
    //fusa:test REQ-CMP-008
    fn request_kind_gbb_message_timestamp_decode_rejects_unrecognized_leading_byte() {
        for raw in [0x02u8, 0x03, 0x04, 0x08, 0x0C, 0x10, 0x7F, 0x80, 0xFF] {
            let ts = (raw as u64) << 56;
            assert_eq!(RequestKind::from_gbb_message_timestamp(ts), None);
        }
    }

    #[test]
    //fusa:test REQ-CMP-008
    fn request_kind_gbb_message_timestamp_decode_ignores_low_56_bits() {
        for kind in ALL_NON_STANDARD_REQUEST_KINDS {
            let leading = (kind.to_u8() as u64) << 56;
            assert_eq!(RequestKind::from_gbb_message_timestamp(leading), Some(kind));
            assert_eq!(
                RequestKind::from_gbb_message_timestamp(leading | 0x00FF_FFFF_FFFF_FFFF),
                Some(kind)
            );
        }
    }

    #[test]
    //fusa:test REQ-CMP-008
    fn request_kind_gbb_message_timestamp_decode_never_panics_across_the_full_byte_range() {
        for raw in 0u8..=255 {
            let ts = (raw as u64) << 56;
            let _ = RequestKind::from_gbb_message_timestamp(ts);
            let _ = RequestKind::from_gbb_message_timestamp(ts | 0x1234_5678);
        }
    }

    #[test]
    //fusa:test REQ-CMP-008
    fn request_kind_gbb_message_timestamp_encode_never_panics_for_any_kind_and_timestamp() {
        for kind in ALL_REQUEST_KINDS {
            for message_timestamp in [0u64, u64::MAX, 0x8000_0000_0000_0000] {
                let _ = kind.to_gbb_message_timestamp(message_timestamp);
            }
        }
    }

    // ── SequencerState / CompoundGateConfig ──────────────────────────────────

    #[test]
    //fusa:test REQ-CMP-003
    fn sequencer_state_default_is_zero() {
        assert_eq!(SequencerState::default(), SequencerState(0));
    }

    #[test]
    //fusa:test REQ-CMP-003
    fn compound_gate_config_default_is_sequencer_zero_state_zero() {
        let gate = CompoundGateConfig::default();
        assert_eq!(gate.sequencer_num, 0);
        assert_eq!(gate.start_state, SequencerState(0));
    }

    // ── check_sequencer_num_in_bounds ────────────────────────────────────────

    #[test]
    //fusa:test REQ-CMP-004
    fn check_sequencer_num_in_bounds_accepts_every_num_below_max() {
        for max in [1u8, 4, 255] {
            for num in 0..max {
                assert_eq!(check_sequencer_num_in_bounds(num, max), Ok(()));
            }
        }
    }

    #[test]
    //fusa:test REQ-CMP-004
    fn check_sequencer_num_in_bounds_rejects_num_at_or_above_max() {
        for (num, max) in [(0u8, 0u8), (4, 4), (5, 4), (255, 4)] {
            assert_eq!(
                check_sequencer_num_in_bounds(num, max),
                Err(RcpError::SequencerNotKnown)
            );
        }
    }

    #[test]
    //fusa:test REQ-CMP-004
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
    //fusa:test REQ-CMP-005
    fn is_gate_satisfied_true_only_when_current_state_matches_start_state() {
        let gate = sample_gate();
        assert!(is_gate_satisfied(SequencerState(1), &gate));
        assert!(!is_gate_satisfied(SequencerState(0), &gate));
        assert!(!is_gate_satisfied(SequencerState(2), &gate));
    }

    #[test]
    //fusa:test REQ-CMP-005
    fn check_compound_gate_ok_when_sequencer_known_and_state_matches() {
        let gate = sample_gate();
        assert_eq!(check_compound_gate(SequencerState(1), &gate, 4), Ok(()));
    }

    #[test]
    //fusa:test REQ-CMP-004
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
    //fusa:test REQ-CMP-005
    fn check_compound_gate_rejects_mismatched_state_for_a_known_sequencer() {
        let gate = sample_gate();
        assert_eq!(
            check_compound_gate(SequencerState(0), &gate, 4),
            Err(RcpError::RequestRejected)
        );
    }

    #[test]
    //fusa:test REQ-CMP-005
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
    //fusa:test REQ-CMP-006
    fn compound_exec_delays_default_is_zero_for_both_timers() {
        let delays = CompoundExecDelays::default();
        assert_eq!(delays.cmp_exec_delay, 0);
        assert_eq!(delays.cmpw_exec_delay, 0);
    }

    #[test]
    //fusa:test REQ-CMP-006
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
    //fusa:test REQ-SAFETY-003
    fn resolve_compound_exec_delay_matches_the_safety_tagged_variants_base_kind() {
        let delays = CompoundExecDelays {
            cmp_exec_delay: 100,
            cmpw_exec_delay: 200,
        };
        assert_eq!(
            resolve_compound_exec_delay(RequestKind::SafetyCompound, &delays),
            resolve_compound_exec_delay(RequestKind::Compound, &delays)
        );
        assert_eq!(
            resolve_compound_exec_delay(RequestKind::SafetyCompoundWait, &delays),
            resolve_compound_exec_delay(RequestKind::CompoundWait, &delays)
        );
        assert_eq!(
            resolve_compound_exec_delay(RequestKind::SafetyTriggered, &delays),
            None
        );
    }

    #[test]
    //fusa:test REQ-CMP-006
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

    #[test]
    //fusa:test REQ-CMP-006
    //fusa:test REQ-CHAIN-001
    fn resolve_compound_exec_delay_is_none_for_chained() {
        let delays = CompoundExecDelays {
            cmp_exec_delay: 100,
            cmpw_exec_delay: 200,
        };
        assert_eq!(
            resolve_compound_exec_delay(RequestKind::Chained, &delays),
            None
        );
    }

    #[test]
    //fusa:test REQ-CMP-006
    //fusa:test REQ-TIME-001
    fn resolve_compound_exec_delay_is_none_for_timed() {
        let delays = CompoundExecDelays {
            cmp_exec_delay: 100,
            cmpw_exec_delay: 200,
        };
        assert_eq!(
            resolve_compound_exec_delay(RequestKind::Timed, &delays),
            None
        );
    }

    #[test]
    //fusa:test REQ-CMP-006
    //fusa:test REQ-CANCEL-001
    fn resolve_compound_exec_delay_is_none_for_all_three_cancellation_kinds() {
        let delays = CompoundExecDelays {
            cmp_exec_delay: 100,
            cmpw_exec_delay: 200,
        };
        for kind in [
            RequestKind::ClearAll,
            RequestKind::ClearNonSafestate,
            RequestKind::ClearSingle,
        ] {
            assert_eq!(resolve_compound_exec_delay(kind, &delays), None);
        }
    }

    #[test]
    //fusa:test REQ-CMP-006
    //fusa:test REQ-PRIO-002
    fn resolve_compound_exec_delay_is_none_for_standard() {
        let delays = CompoundExecDelays {
            cmp_exec_delay: 100,
            cmpw_exec_delay: 200,
        };
        assert_eq!(
            resolve_compound_exec_delay(RequestKind::Standard, &delays),
            None
        );
    }

    // ── advance_sequencer_if_still_in_start_state ────────────────────────────

    #[test]
    //fusa:test REQ-CMP-007
    fn advance_sequencer_advances_when_still_in_start_state() {
        let gate = sample_gate();
        assert_eq!(
            advance_sequencer_if_still_in_start_state(SequencerState(1), &gate, SequencerState(3)),
            Some(SequencerState(3))
        );
    }

    #[test]
    //fusa:test REQ-CMP-007
    fn advance_sequencer_refuses_when_race_moved_it_out_of_start_state() {
        let gate = sample_gate();
        assert_eq!(
            advance_sequencer_if_still_in_start_state(SequencerState(9), &gate, SequencerState(3)),
            None
        );
    }

    #[test]
    //fusa:test REQ-CMP-007
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

    // ── SequencerBank ─────────────────────────────────────────────────────────

    #[test]
    //fusa:test REQ-SEQ-001
    fn sequencer_bank_new_sizes_the_bank_to_svr_sequencers_max() {
        for max in [0u8, 1, 4, 255] {
            let bank = SequencerBank::new(max);
            assert_eq!(bank.svr_sequencers_max(), max);
        }
    }

    #[test]
    //fusa:test REQ-SEQ-001
    fn sequencer_bank_new_initializes_every_sequencer_to_the_power_on_default_state() {
        let bank = SequencerBank::new(4);
        for sequencer_num in 0..4u8 {
            assert_eq!(bank.read(sequencer_num), Ok(SequencerState(1)));
        }
        assert_eq!(
            SequencerStateEntry::power_on_default().seq_state,
            1,
            "SequencerBank::new is documented as reusing this exact value"
        );
    }

    #[test]
    //fusa:test REQ-SEQ-001
    fn sequencer_bank_new_with_zero_max_yields_an_empty_bank() {
        let bank = SequencerBank::new(0);
        assert_eq!(bank.svr_sequencers_max(), 0);
        assert_eq!(bank.read(0), Err(RcpError::SequencerNotKnown));
    }

    // ── SequencerBank::read ──────────────────────────────────────────────────

    #[test]
    //fusa:test REQ-SEQ-002
    fn sequencer_bank_read_rejects_sequencer_num_at_or_above_the_bound() {
        let bank = SequencerBank::new(4);
        for sequencer_num in [4u8, 5, 255] {
            assert_eq!(bank.read(sequencer_num), Err(RcpError::SequencerNotKnown));
        }
    }

    #[test]
    //fusa:test REQ-SEQ-002
    fn sequencer_bank_read_never_panics_for_any_sampled_input() {
        let bank = SequencerBank::new(4);
        for sequencer_num in [0u8, 3, 4, 255] {
            let _ = bank.read(sequencer_num);
        }
        let empty_bank = SequencerBank::new(0);
        for sequencer_num in [0u8, 255] {
            let _ = empty_bank.read(sequencer_num);
        }
    }

    // ── SequencerBank::advance_if_still_in_start_state ───────────────────────

    #[test]
    //fusa:test REQ-SEQ-003
    fn sequencer_bank_advance_mutates_the_store_when_still_in_start_state() {
        let mut bank = SequencerBank::new(4);
        let gate = CompoundGateConfig {
            sequencer_num: 2,
            start_state: SequencerState(1),
        };
        assert_eq!(
            bank.advance_if_still_in_start_state(&gate, SequencerState(9)),
            Ok(true)
        );
        assert_eq!(bank.read(2), Ok(SequencerState(9)));
        // Every other sequencer in the bank is untouched.
        assert_eq!(bank.read(0), Ok(SequencerState(1)));
        assert_eq!(bank.read(1), Ok(SequencerState(1)));
        assert_eq!(bank.read(3), Ok(SequencerState(1)));
    }

    #[test]
    //fusa:test REQ-SEQ-003
    fn sequencer_bank_advance_leaves_the_store_unchanged_when_race_lost() {
        let mut bank = SequencerBank::new(4);
        // First advance moves sequencer 2 out of start_state 1.
        let gate = CompoundGateConfig {
            sequencer_num: 2,
            start_state: SequencerState(1),
        };
        assert_eq!(
            bank.advance_if_still_in_start_state(&gate, SequencerState(9)),
            Ok(true)
        );
        // A second attempt against the same stale start_state now loses the
        // race and must not mutate the store further.
        assert_eq!(
            bank.advance_if_still_in_start_state(&gate, SequencerState(42)),
            Ok(false)
        );
        assert_eq!(bank.read(2), Ok(SequencerState(9)));
    }

    #[test]
    //fusa:test REQ-SEQ-003
    fn sequencer_bank_advance_rejects_out_of_bounds_sequencer_num() {
        let mut bank = SequencerBank::new(2);
        let gate = CompoundGateConfig {
            sequencer_num: 2,
            start_state: SequencerState(1),
        };
        assert_eq!(
            bank.advance_if_still_in_start_state(&gate, SequencerState(9)),
            Err(RcpError::SequencerNotKnown)
        );
    }

    #[test]
    //fusa:test REQ-SEQ-003
    fn sequencer_bank_advance_never_panics_for_any_sampled_input() {
        let mut bank = SequencerBank::new(4);
        for sequencer_num in [0u8, 3, 4, 255] {
            let gate = CompoundGateConfig {
                sequencer_num,
                start_state: SequencerState(1),
            };
            for next in [0u8, 3, 255] {
                let _ = bank.advance_if_still_in_start_state(&gate, SequencerState(next));
            }
        }
    }

    // ── SequencerBank::check_compound_gate ───────────────────────────────────

    #[test]
    //fusa:test REQ-SEQ-004
    fn sequencer_bank_check_compound_gate_ok_when_default_state_matches_gate() {
        let bank = SequencerBank::new(4);
        let gate = CompoundGateConfig {
            sequencer_num: 1,
            start_state: SequencerState(1),
        };
        assert_eq!(bank.check_compound_gate(&gate), Ok(()));
    }

    #[test]
    //fusa:test REQ-SEQ-004
    fn sequencer_bank_check_compound_gate_rejects_mismatched_state() {
        let bank = SequencerBank::new(4);
        let gate = CompoundGateConfig {
            sequencer_num: 1,
            start_state: SequencerState(2),
        };
        assert_eq!(
            bank.check_compound_gate(&gate),
            Err(RcpError::RequestRejected)
        );
    }

    #[test]
    //fusa:test REQ-SEQ-004
    fn sequencer_bank_check_compound_gate_rejects_out_of_bounds_sequencer() {
        let bank = SequencerBank::new(2);
        let gate = CompoundGateConfig {
            sequencer_num: 2,
            start_state: SequencerState(1),
        };
        assert_eq!(
            bank.check_compound_gate(&gate),
            Err(RcpError::SequencerNotKnown)
        );
    }

    #[test]
    //fusa:test REQ-SEQ-004
    fn sequencer_bank_check_compound_gate_reflects_a_prior_advance() {
        let mut bank = SequencerBank::new(4);
        let gate = CompoundGateConfig {
            sequencer_num: 1,
            start_state: SequencerState(1),
        };
        assert_eq!(
            bank.advance_if_still_in_start_state(&gate, SequencerState(5)),
            Ok(true)
        );
        // The gate no longer matches this bank's live state.
        assert_eq!(
            bank.check_compound_gate(&gate),
            Err(RcpError::RequestRejected)
        );
        let advanced_gate = CompoundGateConfig {
            sequencer_num: 1,
            start_state: SequencerState(5),
        };
        assert_eq!(bank.check_compound_gate(&advanced_gate), Ok(()));
    }

    #[test]
    //fusa:test REQ-SEQ-004
    fn sequencer_bank_check_compound_gate_never_panics_for_any_sampled_input() {
        let bank = SequencerBank::new(4);
        for sequencer_num in [0u8, 3, 4, 255] {
            for start_state in [0u8, 1, 255] {
                let gate = CompoundGateConfig {
                    sequencer_num,
                    start_state: SequencerState(start_state),
                };
                let _ = bank.check_compound_gate(&gate);
            }
        }
    }

    // ── TriggerExecDelay / resolve_trigger_exec_delay ────────────────────────

    #[test]
    //fusa:test REQ-TRIG-002
    fn trigger_exec_delay_default_is_zero() {
        assert_eq!(TriggerExecDelay::default(), TriggerExecDelay(0));
    }

    #[test]
    //fusa:test REQ-TRIG-002
    fn resolve_trigger_exec_delay_selects_the_timer_only_for_triggered() {
        let delay = TriggerExecDelay(42);
        assert_eq!(
            resolve_trigger_exec_delay(RequestKind::Triggered, delay),
            Some(42)
        );
    }

    #[test]
    //fusa:test REQ-SAFETY-003
    fn resolve_trigger_exec_delay_matches_the_safety_tagged_variants_base_kind() {
        let delay = TriggerExecDelay(42);
        assert_eq!(
            resolve_trigger_exec_delay(RequestKind::SafetyTriggered, delay),
            resolve_trigger_exec_delay(RequestKind::Triggered, delay)
        );
        assert_eq!(
            resolve_trigger_exec_delay(RequestKind::SafetyCompound, delay),
            None
        );
        assert_eq!(
            resolve_trigger_exec_delay(RequestKind::SafetyCompoundWait, delay),
            None
        );
    }

    #[test]
    //fusa:test REQ-TRIG-002
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
        assert_eq!(
            resolve_trigger_exec_delay(RequestKind::Chained, delay),
            None
        );
        assert_eq!(resolve_trigger_exec_delay(RequestKind::Timed, delay), None);
    }

    #[test]
    //fusa:test REQ-TRIG-002
    //fusa:test REQ-CANCEL-001
    fn resolve_trigger_exec_delay_is_none_for_all_three_cancellation_kinds() {
        let delay = TriggerExecDelay(42);
        for kind in [
            RequestKind::ClearAll,
            RequestKind::ClearNonSafestate,
            RequestKind::ClearSingle,
        ] {
            assert_eq!(resolve_trigger_exec_delay(kind, delay), None);
        }
    }

    #[test]
    //fusa:test REQ-TRIG-002
    //fusa:test REQ-PRIO-002
    fn resolve_trigger_exec_delay_is_none_for_standard() {
        let delay = TriggerExecDelay(42);
        assert_eq!(
            resolve_trigger_exec_delay(RequestKind::Standard, delay),
            None
        );
    }

    // ── TriggerRepeatCount ────────────────────────────────────────────────────

    #[test]
    //fusa:test REQ-TRIG-003
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
    //fusa:test REQ-TRIG-003
    fn trigger_repeat_count_from_u16_maps_every_other_value_to_finite() {
        for raw in [0u16, 1, 42, 0xFFFE] {
            assert_eq!(
                TriggerRepeatCount::from_u16(raw),
                TriggerRepeatCount::Finite(raw)
            );
        }
    }

    #[test]
    //fusa:test REQ-TRIG-003
    fn trigger_repeat_count_finite_round_trips_through_to_u16_from_u16() {
        for raw in [0u16, 1, 42, 0xFFFE] {
            let count = TriggerRepeatCount::from_u16(raw);
            assert_eq!(TriggerRepeatCount::from_u16(count.to_u16()), count);
        }
    }

    #[test]
    //fusa:test REQ-TRIG-003
    fn trigger_repeat_count_infinite_round_trips_through_to_u16_from_u16() {
        let count = TriggerRepeatCount::Infinite;
        assert_eq!(count.to_u16(), TRIGGER_REPEAT_COUNT_INFINITE_SENTINEL);
        assert_eq!(TriggerRepeatCount::from_u16(count.to_u16()), count);
    }

    #[test]
    //fusa:test REQ-TRIG-003
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
    //fusa:test REQ-TRIG-004
    fn is_trigger_repeat_exhausted_is_always_false_for_infinite() {
        for occurrences in [0u16, 1, 100, u16::MAX] {
            assert!(!is_trigger_repeat_exhausted(
                occurrences,
                TriggerRepeatCount::Infinite
            ));
        }
    }

    #[test]
    //fusa:test REQ-TRIG-004
    fn is_trigger_repeat_exhausted_true_once_occurrences_reach_finite_target() {
        let target = TriggerRepeatCount::Finite(3);
        assert!(!is_trigger_repeat_exhausted(0, target));
        assert!(!is_trigger_repeat_exhausted(2, target));
        assert!(is_trigger_repeat_exhausted(3, target));
        assert!(is_trigger_repeat_exhausted(4, target));
    }

    #[test]
    //fusa:test REQ-TRIG-004
    fn is_trigger_repeat_exhausted_never_panics_for_any_sampled_input() {
        for occurrences in [0u16, 1, 3, u16::MAX] {
            for target in [0u16, 3, u16::MAX] {
                let _ =
                    is_trigger_repeat_exhausted(occurrences, TriggerRepeatCount::Finite(target));
            }
            let _ = is_trigger_repeat_exhausted(occurrences, TriggerRepeatCount::Infinite);
        }
    }

    // ── should_count_trigger_occurrence / TriggeredRuntime ────────────────────

    fn trig_step() -> TriggeredStep {
        TriggeredStep {
            trigger_source_ep: 3,
            trigger_signal_nr: 2,
            trigger_threshold: 1,
            exec_delay: 5,
            repeat_count: 0,
        }
    }

    #[test]
    //fusa:test REQ-TRIG-005
    fn should_count_trigger_occurrence_requires_matching_source_ep_and_signal_nr() {
        let step = trig_step();
        assert!(should_count_trigger_occurrence(&step, 3, 2));
        assert!(!should_count_trigger_occurrence(&step, 4, 2));
        assert!(!should_count_trigger_occurrence(&step, 3, 1));
        assert!(!should_count_trigger_occurrence(&step, 2, 3));
    }

    /// Mirrors the c-RCP harness's `rcp_triggered_*` sequence (golden output
    /// recorded from `librcp.a`: rec_unstarted 0, rec_wrong_ep 0,
    /// rec_wrong_sig 0, rec_match 1 count=1, thr_after_1 0, tick_after_1 0,
    /// thr_after_2 1, tick_delay4 0, tick_busy 0, tick_ok 1 count=0
    /// started=0).
    #[test]
    //fusa:test REQ-TRIG-008
    //fusa:test REQ-TRIG-009
    //fusa:test REQ-TRIG-010
    //fusa:test REQ-TRIG-011
    //fusa:test REQ-TRIG-012
    //fusa:test REQ-TRIG-013
    fn triggered_runtime_matches_c_rcp_occurrence_threshold_and_fire_sequence() {
        let s = trig_step();
        let mut rt = TriggeredRuntime::default();
        assert!(!rt.record_occurrence(&s, 3, 2), "not started");
        rt.enter_started();
        assert_eq!(
            rt,
            TriggeredRuntime {
                occurrence_count: 0,
                started: true
            }
        );
        assert!(!rt.record_occurrence(&s, 4, 2), "wrong ep");
        assert!(!rt.record_occurrence(&s, 3, 1), "wrong signal");
        assert!(rt.record_occurrence(&s, 3, 2));
        assert_eq!(rt.occurrence_count, 1);
        assert!(!rt.threshold_reached(&s), "threshold 1 needs 2 occurrences");
        assert!(!rt.tick(&s, 100, true));
        assert!(rt.record_occurrence(&s, 3, 2));
        assert!(rt.threshold_reached(&s));
        assert!(!s.exec_delay_elapsed(4));
        assert!(s.exec_delay_elapsed(5));
        assert!(!rt.tick(&s, 4, true), "delay not elapsed");
        assert!(!rt.tick(&s, 5, false), "endpoint busy");
        assert_eq!(rt.occurrence_count, 2, "held ticks leave runtime unchanged");
        assert!(rt.tick(&s, 5, true));
        assert_eq!(
            rt,
            TriggeredRuntime {
                occurrence_count: 0,
                started: false
            }
        );
        assert!(
            !rt.record_occurrence(&s, 3, 2),
            "fired request stops counting"
        );
    }

    #[test]
    //fusa:test REQ-TRIG-010
    fn triggered_threshold_zero_fires_on_first_occurrence() {
        let s = TriggeredStep {
            trigger_threshold: 0,
            ..trig_step()
        };
        let mut rt = TriggeredRuntime::default();
        rt.enter_started();
        assert!(!rt.threshold_reached(&s));
        rt.record_occurrence(&s, 3, 2);
        assert!(rt.threshold_reached(&s));
    }

    // ── complete_repetition ───────────────────────────────────────────────────

    #[test]
    //fusa:test REQ-CMP-034
    fn complete_repetition_mirrors_c_rcp_infinite_zero_and_decrement_rules() {
        let mut inf = COMPOUND_REPEAT_INFINITE;
        assert!(complete_repetition(&mut inf));
        assert_eq!(inf, 0xFFFF);
        assert_eq!(TRIGGERED_REPEAT_INFINITE, COMPOUND_REPEAT_INFINITE);
        let mut last = 0u16;
        assert!(!complete_repetition(&mut last));
        assert_eq!(last, 0);
        let mut two = 2u16;
        assert!(complete_repetition(&mut two));
        assert_eq!(two, 1);
        assert!(complete_repetition(&mut two));
        assert_eq!(two, 0);
        assert!(!complete_repetition(&mut two));
    }

    // ── Conditional-request wire codecs: c-RCP golden vectors ─────────────────
    //
    // Every hex vector below was produced by a C harness linked against
    // c-RCP's real `librcp.a` (`rcp_*_encode_*`), and every expected decode
    // error by running c-RCP's own `rcp_*_decode_*` over the same bytes.

    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    const V_COMPOUND_1: &str = "1a054123005a80000f0203011234ffffdeadbe00";
    const V_COMPOUND_WAIT_SAFETY_2: &str = "1a0407ff900080008b0100ff00000007";
    const V_TRIGGERED_1: &str = "1a06c005000980000e030204000a00010102030405000000";
    const V_TRIGGERED_SAFETY_2: &str = "1a04000000ff80008effffffffffffff";
    const V_CHAINED_1: &str = "1a05c0420107800001000000beef0000aa000000";
    const V_CHAINED_2: &str = "1a040001000080000100000000000000";
    const V_TIMED_1: &str = "1a058010000380000a00123456789abc09080000";
    const V_TIMED_2: &str = "1a040000008080000a00ffffffffffff";
    const V_CLEAR_NON_SAFESTATE: &str = "1a040055001180000600000000000000";
    const V_CLEAR_ALL: &str = "1a040066002280000500000000000000";
    const V_CLEAR_SINGLE: &str = "1a04007700338000070000ab00000000";

    fn compound_1() -> CompoundRequest {
        CompoundRequest {
            request_type: RequestKind::Compound,
            byte_bus_id: 0x123,
            step: CompoundStep {
                start_state: 2,
                next_state: 3,
                sequencer_index: 1,
                exec_delay: 0x1234,
                repeat_count: 0xFFFF,
            },
            evt: Evt::default(),
            transaction_num: 0x5A,
            payload: vec![0xDE, 0xAD, 0xBE],
        }
    }

    fn compound_wait_safety_2() -> CompoundRequest {
        CompoundRequest {
            request_type: RequestKind::SafetyCompoundWait,
            byte_bus_id: 0x7FF,
            step: CompoundStep {
                start_state: 1,
                next_state: 0,
                sequencer_index: 255,
                exec_delay: 0,
                repeat_count: 7,
            },
            evt: Evt {
                ack: true,
                sub_opcode: 1,
            },
            transaction_num: 0,
            payload: vec![],
        }
    }

    #[test]
    //fusa:test REQ-CMP-010
    //fusa:test REQ-CMP-015
    //fusa:test REQ-CMP-026
    //fusa:test REQ-CMP-027
    fn compound_codec_matches_c_rcp_golden_vectors_both_ways() {
        for (req, hex) in [
            (compound_1(), V_COMPOUND_1),
            (compound_wait_safety_2(), V_COMPOUND_WAIT_SAFETY_2),
        ] {
            assert_eq!(encode_compound_request(&req).unwrap(), unhex(hex));
            assert_eq!(decode_compound_request(&unhex(hex)).unwrap(), req);
        }
        for kind in [
            RequestKind::Compound,
            RequestKind::CompoundWait,
            RequestKind::SafetyCompound,
        ] {
            let req = CompoundRequest {
                request_type: kind,
                ..compound_1()
            };
            let frame = encode_compound_request(&req).unwrap();
            assert_eq!(peek_request_type(&frame), Ok(kind.to_u8()));
            assert_eq!(decode_compound_request(&frame).unwrap(), req);
        }
    }

    #[test]
    //fusa:test REQ-CMP-031
    //fusa:test REQ-CMP-032
    fn compound_encode_rejects_non_compound_kinds_and_oversized_payload() {
        for kind in [
            RequestKind::Triggered,
            RequestKind::Chained,
            RequestKind::Timed,
            RequestKind::ClearAll,
            RequestKind::Standard,
        ] {
            let req = CompoundRequest {
                request_type: kind,
                ..compound_1()
            };
            assert_eq!(
                encode_compound_request(&req),
                Err(RcpError::InvalidParameter)
            );
        }
        // RCP_ACF_GBB_MAX_PAYLOAD = 511 * 4 - 16 = 2028: accepted; one more is not.
        let max = CompoundRequest {
            payload: vec![0; 2028],
            ..compound_1()
        };
        assert!(encode_compound_request(&max).is_ok());
        let over = CompoundRequest {
            payload: vec![0; 2029],
            ..compound_1()
        };
        assert_eq!(encode_compound_request(&over), Err(RcpError::InvalidSize));
    }

    #[test]
    //fusa:test REQ-CMP-011
    //fusa:test REQ-CMP-012
    //fusa:test REQ-CMP-013
    //fusa:test REQ-CMP-014
    fn compound_decode_errors_match_c_rcp() {
        use RequestCodecError::*;
        let cases = [
            ("1a054123005a80000f0203011234ff", ShortFrame),
            ("1a054123005a80000f0203011234ffffdeadbe", ShortFrame),
            ("1c054123005a80000f0203011234ffffdeadbe00", BadMsgType),
            ("1a056123005a80000f0203011234ffffdeadbe00", NotRepurposed),
            (V_TRIGGERED_1, UnknownType),
        ];
        for (hex, err) in cases {
            assert_eq!(decode_compound_request(&unhex(hex)), Err(err), "{hex}");
        }
        // hs is not validated by c-RCP's compound decoder.
        assert!(
            decode_compound_request(&unhex("1a054123025a80000f0203011234ffffdeadbe00")).is_ok()
        );
        // acf_msg_length shorter than the GBB header is a short frame too.
        let mut bad_len = unhex(V_COMPOUND_1);
        bad_len[1] = 0x03;
        assert_eq!(decode_compound_request(&bad_len), Err(ShortFrame));
    }

    #[test]
    //fusa:test REQ-CMP-030
    fn peek_request_type_matches_c_rcp() {
        assert_eq!(peek_request_type(&unhex(V_CLEAR_SINGLE)), Ok(0x07));
        assert_eq!(
            peek_request_type(&unhex("1a04207700338000070000ab00000000")),
            Err(RequestCodecError::NotRepurposed)
        );
        assert_eq!(
            peek_request_type(&unhex("1c04007700338000070000ab00000000")),
            Err(RequestCodecError::BadMsgType)
        );
        assert_eq!(
            peek_request_type(&unhex(V_CLEAR_SINGLE)[..15]),
            Err(RequestCodecError::ShortFrame)
        );
    }

    fn triggered_1() -> TriggeredRequest {
        TriggeredRequest {
            request_type: RequestKind::Triggered,
            byte_bus_id: 5,
            step: TriggeredStep {
                trigger_source_ep: 3,
                trigger_signal_nr: 2,
                trigger_threshold: 4,
                exec_delay: 10,
                repeat_count: 1,
            },
            transaction_num: 9,
            payload: vec![1, 2, 3, 4, 5],
        }
    }

    #[test]
    //fusa:test REQ-TRIG-007
    //fusa:test REQ-TRIG-014
    //fusa:test REQ-TRIG-015
    fn triggered_codec_matches_c_rcp_golden_vectors_both_ways() {
        let t2 = TriggeredRequest {
            request_type: RequestKind::SafetyTriggered,
            byte_bus_id: 0,
            step: TriggeredStep {
                trigger_source_ep: 0xFF,
                trigger_signal_nr: 0xFF,
                trigger_threshold: 0xFF,
                exec_delay: 0xFFFF,
                repeat_count: 0xFFFF,
            },
            transaction_num: 0xFF,
            payload: vec![],
        };
        for (req, hex) in [(triggered_1(), V_TRIGGERED_1), (t2, V_TRIGGERED_SAFETY_2)] {
            assert_eq!(encode_triggered_request(&req).unwrap(), unhex(hex));
            assert_eq!(decode_triggered_request(&unhex(hex)).unwrap(), req);
        }
        let bad = TriggeredRequest {
            request_type: RequestKind::Compound,
            ..triggered_1()
        };
        assert_eq!(
            encode_triggered_request(&bad),
            Err(RcpError::InvalidParameter)
        );
    }

    #[test]
    //fusa:test REQ-TRIG-006
    //fusa:test REQ-TRIG-007
    //fusa:test REQ-TRIG-016
    fn triggered_decode_errors_match_c_rcp() {
        use RequestCodecError::*;
        for (hex, err) in [
            (
                "1a06e005000980000e030204000a00010102030405000000",
                NotRepurposed,
            ),
            (V_COMPOUND_1, UnknownType),
            ("1a06c005000980000e03", ShortFrame),
            (
                "1c06c005000980000e030204000a00010102030405000000",
                BadMsgType,
            ),
        ] {
            assert_eq!(decode_triggered_request(&unhex(hex)), Err(err), "{hex}");
        }
    }

    #[test]
    //fusa:test REQ-CHAIN-004
    //fusa:test REQ-CHAIN-007
    //fusa:test REQ-CHAIN-013
    fn chained_codec_matches_c_rcp_golden_vectors_both_ways() {
        let m1 = ChainedMember {
            byte_bus_id: 0x42,
            chain_exec_delay: 0xBEEF,
            cs: true,
            transaction_num: 7,
            payload: vec![0xAA],
        };
        let m2 = ChainedMember {
            byte_bus_id: 1,
            chain_exec_delay: 0,
            cs: false,
            transaction_num: 0,
            payload: vec![],
        };
        for (m, hex) in [(m1.clone(), V_CHAINED_1), (m2, V_CHAINED_2)] {
            assert_eq!(encode_chained_member(&m).unwrap(), unhex(hex));
            assert_eq!(decode_chained_member(&unhex(hex)).unwrap(), m);
        }
        let over = ChainedMember {
            payload: vec![0; 2029],
            ..m1
        };
        assert_eq!(encode_chained_member(&over), Err(RcpError::InvalidSize));
    }

    #[test]
    //fusa:test REQ-CHAIN-005
    //fusa:test REQ-CHAIN-006
    //fusa:test REQ-CHAIN-012
    fn chained_decode_errors_match_c_rcp() {
        use RequestCodecError::*;
        for (hex, err) in [
            ("1a05e0420107800001000000beef0000aa000000", NotRepurposed),
            (V_TIMED_1, UnknownType),
            ("1a05c0420107800001010000beef0000aa000000", ReservedNonzero),
            ("1a05c0420107800001000000beef0080aa000000", ReservedNonzero),
            ("1a05c04201", ShortFrame),
        ] {
            assert_eq!(decode_chained_member(&unhex(hex)), Err(err), "{hex}");
        }
        assert!(decode_chained_member(&unhex("1a05c0420307800001000000beef0000aa000000")).is_ok());
    }

    #[test]
    //fusa:test REQ-TIMED-003
    //fusa:test REQ-TIMED-005
    fn timed_codec_matches_c_rcp_golden_vectors_both_ways() {
        let t1 = TimedRequest {
            byte_bus_id: 0x10,
            presentation_time: 0x0000_1234_5678_9ABC,
            transaction_num: 3,
            payload: vec![9, 8],
        };
        let t2 = TimedRequest {
            byte_bus_id: 0,
            presentation_time: TIMED_PRESENTATION_TIME_MAX,
            transaction_num: 0x80,
            payload: vec![],
        };
        for (req, hex) in [(t1.clone(), V_TIMED_1), (t2, V_TIMED_2)] {
            assert_eq!(encode_timed_request(&req).unwrap(), unhex(hex));
            assert_eq!(decode_timed_request(&unhex(hex)).unwrap(), req);
        }
        let too_big = TimedRequest {
            presentation_time: TIMED_PRESENTATION_TIME_MAX + 1,
            ..t1
        };
        assert_eq!(
            encode_timed_request(&too_big),
            Err(RcpError::InvalidParameter)
        );
    }

    #[test]
    //fusa:test REQ-TIMED-004
    //fusa:test REQ-TIMED-005
    //fusa:test REQ-TIMED-009
    //fusa:test REQ-TIMED-010
    fn timed_decode_errors_match_c_rcp() {
        use RequestCodecError::*;
        for (hex, err) in [
            ("1a05a010000380000a00123456789abc09080000", NotRepurposed),
            (V_CHAINED_1, UnknownType),
            ("1a058010000380000a10123456789abc09080000", ReservedNonzero),
            ("1a058010020380000a00123456789abc09080000", UnsupportedCmd),
            ("1a058010010380000a00123456789abc09080000", UnsupportedCmd),
            ("1a0580", ShortFrame),
            ("1c058010000380000a00123456789abc09080000", BadMsgType),
        ] {
            assert_eq!(decode_timed_request(&unhex(hex)), Err(err), "{hex}");
        }
        // evt is not validated by c-RCP's timed decoder.
        assert!(decode_timed_request(&unhex("1a058010100380000a00123456789abc09080000")).is_ok());
    }

    #[test]
    //fusa:test REQ-CMP-016
    //fusa:test REQ-CMP-017
    //fusa:test REQ-CANCEL-005
    //fusa:test REQ-CANCEL-007
    //fusa:test REQ-CANCEL-016
    fn cancellation_codecs_match_c_rcp_golden_vectors_both_ways() {
        let cns = ClearRequest {
            byte_bus_id: 0x55,
            transaction_num: 0x11,
        };
        assert_eq!(
            encode_clear_non_safestate(&cns).unwrap(),
            unhex(V_CLEAR_NON_SAFESTATE)
        );
        assert_eq!(
            decode_clear_non_safestate(&unhex(V_CLEAR_NON_SAFESTATE)),
            Ok(cns)
        );
        let ca = ClearRequest {
            byte_bus_id: 0x66,
            transaction_num: 0x22,
        };
        assert_eq!(encode_clear_all(&ca).unwrap(), unhex(V_CLEAR_ALL));
        assert_eq!(decode_clear_all(&unhex(V_CLEAR_ALL)), Ok(ca));
        let cs = ClearSingleRequest {
            byte_bus_id: 0x77,
            clear_transaction_num: 0xAB,
            transaction_num: 0x33,
        };
        assert_eq!(encode_clear_single(&cs).unwrap(), unhex(V_CLEAR_SINGLE));
        assert_eq!(decode_clear_single(&unhex(V_CLEAR_SINGLE)), Ok(cs));
        let bad_bus = ClearRequest {
            byte_bus_id: 0x800,
            transaction_num: 0,
        };
        assert_eq!(encode_clear_all(&bad_bus), Err(RcpError::InvalidSize));
    }

    #[test]
    //fusa:test REQ-CMP-018
    //fusa:test REQ-CMP-028
    //fusa:test REQ-CMP-029
    //fusa:test REQ-CANCEL-006
    //fusa:test REQ-CANCEL-013
    //fusa:test REQ-CANCEL-014
    //fusa:test REQ-CANCEL-015
    //fusa:test REQ-CANCEL-017
    fn cancellation_decode_errors_match_c_rcp() {
        use RequestCodecError::*;
        type Dec = fn(&[u8]) -> Result<(), RequestCodecError>;
        let cns: Dec = |b| decode_clear_non_safestate(b).map(|_| ());
        let ca: Dec = |b| decode_clear_all(b).map(|_| ());
        let csg: Dec = |b| decode_clear_single(b).map(|_| ());
        let cases: [(Dec, &str, Result<(), RequestCodecError>); 21] = [
            (cns, "1a042055001180000600000000000000", Err(NotRepurposed)),
            (cns, V_CLEAR_ALL, Err(UnknownType)),
            (
                cns,
                "1a040055001180000600000000000001",
                Err(ReservedNonzero),
            ),
            (cns, "1a040055101180000600000000000000", Err(EvtHsCsNonzero)),
            (cns, "1a040055801180000600000000000000", Ok(())),
            (cns, "1a040055021180000600000000000000", Err(EvtHsCsNonzero)),
            (cns, "1a040055011180000600000000000000", Err(EvtHsCsNonzero)),
            (cns, "1a0400", Err(ShortFrame)),
            (ca, V_CLEAR_NON_SAFESTATE, Err(UnknownType)),
            (ca, "1a040066002280000501000000000000", Err(ReservedNonzero)),
            (ca, "1a040066102280000500000000000000", Err(EvtHsCsNonzero)),
            (ca, "1a040066802280000500000000000000", Ok(())),
            (ca, "1a040066012280000500000000000000", Err(EvtHsCsNonzero)),
            (ca, "1a042066002280000500000000000000", Err(NotRepurposed)),
            (ca, "1c040066002280000500000000000000", Err(BadMsgType)),
            (csg, V_CLEAR_ALL, Err(UnknownType)),
            (
                csg,
                "1a04007700338000070001ab00000000",
                Err(ReservedNonzero),
            ),
            (
                csg,
                "1a04007700338000070000ab01000000",
                Err(ReservedNonzero),
            ),
            (csg, "1a04007702338000070000ab00000000", Err(EvtHsCsNonzero)),
            (csg, "1a04007780338000070000ab00000000", Ok(())),
            (csg, "1a04207700338000070000ab00000000", Err(NotRepurposed)),
        ];
        for (dec, hex, expected) in cases {
            assert_eq!(dec(&unhex(hex)), expected, "{hex}");
        }
    }

    #[test]
    //fusa:test REQ-CMP-033
    fn request_codec_errors_have_distinct_messages_and_send_no_response() {
        use RequestCodecError::*;
        let all = [
            ShortFrame,
            BadMsgType,
            NotRepurposed,
            UnknownType,
            ReservedNonzero,
            EvtHsCsNonzero,
            UnsupportedCmd,
        ];
        let msgs: std::collections::HashSet<String> = all.iter().map(|e| e.to_string()).collect();
        assert_eq!(msgs.len(), all.len());
        for e in all {
            assert!(e.to_string().starts_with("rcp/request: "));
            // c-RCP parity: a conditional request that fails to decode gets
            // no error response at all.
            assert_eq!(e.response_error(), None);
        }
    }

    #[test]
    //fusa:test REQ-CMP-011
    //fusa:test REQ-TRIG-016
    //fusa:test REQ-CHAIN-005
    //fusa:test REQ-TIMED-004
    //fusa:test REQ-CANCEL-017
    fn conditional_decoders_never_panic_on_truncations_of_every_vector() {
        for hex in [
            V_COMPOUND_1,
            V_TRIGGERED_1,
            V_CHAINED_1,
            V_TIMED_1,
            V_CLEAR_NON_SAFESTATE,
            V_CLEAR_ALL,
            V_CLEAR_SINGLE,
        ] {
            let b = unhex(hex);
            for n in 0..=b.len() {
                let p = &b[..n];
                let _ = peek_request_type(p);
                let _ = decode_compound_request(p);
                let _ = decode_triggered_request(p);
                let _ = decode_chained_member(p);
                let _ = decode_timed_request(p);
                let _ = decode_clear_non_safestate(p);
                let _ = decode_clear_all(p);
                let _ = decode_clear_single(p);
            }
        }
    }

    // ── SequencerBank ownership (REQ-SEQ-013 / REQ-WIREERR-005) ──────────────

    /// Golden table from c-RCP's real `rcp_sequencer_access_check()` /
    /// `rcp_sequencer_wire_error()` over a 3-sequencer table with owner[1]=2,
    /// owner[2]=0xFF, owner[0] unclaimed, for idx 0..4 x requester 0..3.
    #[test]
    //fusa:test REQ-SEQ-013
    //fusa:test REQ-WIREERR-005
    fn sequencer_access_check_matches_c_rcp_golden_table() {
        let mut bank = SequencerBank::new(3);
        assert_eq!(bank.owner(0), Ok(SEQUENCER_OWNER_UNCLAIMED));
        bank.set_owner(1, 2).unwrap();
        bank.set_owner(2, 0xFF).unwrap();
        assert_eq!(bank.set_owner(3, 1), Err(RcpError::SequencerNotKnown));
        assert_eq!(bank.owner(3), Err(RcpError::SequencerNotKnown));
        for idx in 0u8..4 {
            for req in 0u8..3 {
                let (expected, wire) = match (idx, req) {
                    (3, _) => (SequencerAccess::Unknown, Some(2u8)),
                    (1, 2) => (SequencerAccess::Permitted, None),
                    _ => (SequencerAccess::Denied, Some(3u8)),
                };
                let got = bank.access_check(idx, req);
                assert_eq!(got, expected, "idx={idx} req={req}");
                assert_eq!(got.wire_error().and_then(|e| e.tc18_wire_code()), wire);
                assert_eq!(
                    bank.access_permitted(idx, req),
                    expected == SequencerAccess::Permitted
                );
            }
        }
    }

    #[test]
    //fusa:test REQ-SEQ-013
    //fusa:test REQ-WIREERR-005
    fn check_compound_admission_enforces_ownership_and_skips_an_empty_bank() {
        let step = CompoundStep {
            sequencer_index: 1,
            ..CompoundStep::default()
        };
        // No sequencers at all: the ownership gate is skipped (mock.c:1649).
        assert_eq!(
            SequencerBank::new(0).check_compound_admission(&step, 0),
            Ok(())
        );
        let mut bank = SequencerBank::new(2);
        // Unclaimed: fail-closed.
        assert_eq!(
            bank.check_compound_admission(&step, 1),
            Err(RcpError::UnauthorizedAccess)
        );
        bank.set_owner(1, 1).unwrap();
        assert_eq!(bank.check_compound_admission(&step, 1), Ok(()));
        assert_eq!(
            bank.check_compound_admission(&step, 2),
            Err(RcpError::UnauthorizedAccess)
        );
        let unknown = CompoundStep {
            sequencer_index: 2,
            ..step
        };
        assert_eq!(
            bank.check_compound_admission(&unknown, 1),
            Err(RcpError::SequencerNotKnown)
        );
        // Ownership is independent of the state registers.
        bank.force_state(1, SequencerState(9)).unwrap();
        assert_eq!(bank.owner(1), Ok(1));
    }

    // ── check_chain_continuation ──────────────────────────────────────────────

    #[test]
    //fusa:test REQ-CHAIN-002
    fn check_chain_continuation_aborts_only_when_cs_set_and_predecessor_errored() {
        assert_eq!(
            check_chain_continuation(true, true),
            Err(RcpError::ChainAborted)
        );
    }

    #[test]
    //fusa:test REQ-CHAIN-002
    fn check_chain_continuation_continues_when_cs_not_set_even_if_predecessor_errored() {
        assert_eq!(check_chain_continuation(false, true), Ok(()));
    }

    #[test]
    //fusa:test REQ-CHAIN-002
    fn check_chain_continuation_continues_when_predecessor_did_not_error_regardless_of_cs() {
        assert_eq!(check_chain_continuation(true, false), Ok(()));
        assert_eq!(check_chain_continuation(false, false), Ok(()));
    }

    #[test]
    //fusa:test REQ-CHAIN-002
    fn check_chain_continuation_never_panics_for_any_sampled_input() {
        for cs in [true, false] {
            for predecessor_errored in [true, false] {
                let _ = check_chain_continuation(cs, predecessor_errored);
            }
        }
    }

    // ── RcpError::ChainAborted / RcpError::ChainError ────────────────────────

    #[test]
    //fusa:test REQ-CHAIN-003
    fn chain_aborted_and_chain_error_are_distinct_rcp_error_variants() {
        assert_ne!(RcpError::ChainAborted, RcpError::ChainError);
        assert_eq!(RcpError::ChainAborted, RcpError::ChainAborted);
        assert_eq!(RcpError::ChainError, RcpError::ChainError);
    }

    #[test]
    //fusa:test REQ-CHAIN-003
    fn chain_aborted_and_chain_error_carry_the_roadmap_named_codes_in_their_display_text() {
        assert!(RcpError::ChainAborted.to_string().contains("CHAIN_ABORTED"));
        assert!(RcpError::ChainError.to_string().contains("CHAIN_ERROR"));
    }

    // ── TimedExecutionTime / is_timed_request_ready ──────────────────────────

    #[test]
    //fusa:test REQ-TIME-002
    fn timed_execution_time_default_is_untimed() {
        let exec_time = TimedExecutionTime::default();
        assert_eq!(exec_time.0, AvtpTimestamp::default());
        assert!(exec_time.0.is_untimed());
    }

    #[test]
    //fusa:test REQ-TIME-002
    fn timed_execution_time_wraps_avtp_timestamp_by_value() {
        let exec_time = TimedExecutionTime(AvtpTimestamp::new(1_000));
        assert_eq!(exec_time.0.to_u32(), 1_000);
    }

    #[test]
    //fusa:test REQ-TIME-003
    fn is_timed_request_ready_false_before_exec_time_is_reached() {
        let exec_time = TimedExecutionTime(AvtpTimestamp::new(1_000));
        assert!(!is_timed_request_ready(AvtpTimestamp::new(999), exec_time));
    }

    #[test]
    //fusa:test REQ-TIME-003
    fn is_timed_request_ready_true_exactly_at_exec_time() {
        let exec_time = TimedExecutionTime(AvtpTimestamp::new(1_000));
        assert!(is_timed_request_ready(AvtpTimestamp::new(1_000), exec_time));
    }

    #[test]
    //fusa:test REQ-TIME-003
    fn is_timed_request_ready_true_after_exec_time_has_passed() {
        let exec_time = TimedExecutionTime(AvtpTimestamp::new(1_000));
        assert!(is_timed_request_ready(AvtpTimestamp::new(1_001), exec_time));
    }

    #[test]
    //fusa:test REQ-TIME-003
    fn is_timed_request_ready_true_across_a_rollover() {
        // AvtpTimestamp::is_after is wraparound-aware; a current time that
        // just wrapped past u32::MAX back to a small value must still read
        // as ready for an exec_time set just before the rollover.
        let exec_time = TimedExecutionTime(AvtpTimestamp::new(u32::MAX - 1));
        assert!(is_timed_request_ready(AvtpTimestamp::new(2), exec_time));
    }

    #[test]
    //fusa:test REQ-TIME-003
    fn is_timed_request_ready_always_true_for_an_untimed_exec_time() {
        let exec_time = TimedExecutionTime(AvtpTimestamp::default());
        for current in [0u32, 1, 1_000, u32::MAX] {
            assert!(is_timed_request_ready(
                AvtpTimestamp::new(current),
                exec_time
            ));
        }
    }

    #[test]
    //fusa:test REQ-TIME-003
    fn is_timed_request_ready_never_panics_for_any_sampled_input() {
        for current in [0u32, 1, 1_000, u32::MAX] {
            for target in [0u32, 1, 1_000, u32::MAX] {
                let _ = is_timed_request_ready(
                    AvtpTimestamp::new(current),
                    TimedExecutionTime(AvtpTimestamp::new(target)),
                );
            }
        }
    }

    // ── check_clear_all_cancellation ──────────────────────────────────────────

    #[test]
    //fusa:test REQ-CANCEL-002
    fn check_clear_all_cancellation_always_cancels() {
        assert_eq!(
            check_clear_all_cancellation(),
            Err(RcpError::RequestCanceled)
        );
    }

    // ── check_clear_non_safestate_cancellation ────────────────────────────────

    #[test]
    //fusa:test REQ-CANCEL-003
    fn check_clear_non_safestate_cancellation_spares_safestate_related_requests() {
        assert_eq!(check_clear_non_safestate_cancellation(true), Ok(()));
    }

    #[test]
    //fusa:test REQ-CANCEL-003
    fn check_clear_non_safestate_cancellation_cancels_non_safestate_related_requests() {
        assert_eq!(
            check_clear_non_safestate_cancellation(false),
            Err(RcpError::RequestCanceled)
        );
    }

    #[test]
    //fusa:test REQ-CANCEL-003
    fn check_clear_non_safestate_cancellation_never_panics_for_any_input() {
        for is_safestate_related in [true, false] {
            let _ = check_clear_non_safestate_cancellation(is_safestate_related);
        }
    }

    // ── ClearTransactionNum / check_clear_single_cancellation ────────────────

    #[test]
    //fusa:test REQ-CANCEL-004
    fn clear_transaction_num_default_is_zero() {
        assert_eq!(ClearTransactionNum::default(), ClearTransactionNum(0));
    }

    #[test]
    //fusa:test REQ-CANCEL-004
    fn check_clear_single_cancellation_cancels_only_the_matching_transaction_num() {
        let target = ClearTransactionNum(0x42);
        assert_eq!(
            check_clear_single_cancellation(0x42, target),
            Err(RcpError::RequestCanceled)
        );
    }

    #[test]
    //fusa:test REQ-CANCEL-004
    fn check_clear_single_cancellation_spares_every_non_matching_transaction_num() {
        let target = ClearTransactionNum(0x42);
        for candidate in [0x00u8, 0x01, 0x41, 0x43, 0xFF] {
            assert_eq!(check_clear_single_cancellation(candidate, target), Ok(()));
        }
    }

    #[test]
    //fusa:test REQ-CANCEL-004
    fn check_clear_single_cancellation_never_panics_for_any_sampled_input() {
        for candidate in [0x00u8, 0x42, 0xFF] {
            for target in [0x00u8, 0x42, 0xFF] {
                let _ = check_clear_single_cancellation(candidate, ClearTransactionNum(target));
            }
        }
    }

    // ── ExecutionPriorityTier / execution_priority_tier ──────────────────────

    #[test]
    //fusa:test REQ-PRIO-003
    fn execution_priority_tier_orders_tiers_cancellation_highest_standard_lowest() {
        // `ROADMAP.md`'s own stated order: cancellation > triggered > timed >
        // compound > compound-wait > chained > standard. Ord's derive makes
        // the earlier-declared variant compare as "less than" — i.e. this
        // module's convention is: lower ordinal == higher priority.
        assert!(ExecutionPriorityTier::Cancellation < ExecutionPriorityTier::Triggered);
        assert!(ExecutionPriorityTier::Triggered < ExecutionPriorityTier::Timed);
        assert!(ExecutionPriorityTier::Timed < ExecutionPriorityTier::Compound);
        assert!(ExecutionPriorityTier::Compound < ExecutionPriorityTier::CompoundWait);
        assert!(ExecutionPriorityTier::CompoundWait < ExecutionPriorityTier::Chained);
        assert!(ExecutionPriorityTier::Chained < ExecutionPriorityTier::Standard);
    }

    #[test]
    //fusa:test REQ-PRIO-003
    fn execution_priority_tier_collapses_all_three_cancellation_kinds_onto_one_tier() {
        for kind in [
            RequestKind::ClearAll,
            RequestKind::ClearNonSafestate,
            RequestKind::ClearSingle,
        ] {
            assert_eq!(
                execution_priority_tier(kind),
                ExecutionPriorityTier::Cancellation
            );
        }
    }

    #[test]
    //fusa:test REQ-PRIO-003
    fn execution_priority_tier_maps_every_remaining_kind_to_its_own_named_tier() {
        assert_eq!(
            execution_priority_tier(RequestKind::Triggered),
            ExecutionPriorityTier::Triggered
        );
        assert_eq!(
            execution_priority_tier(RequestKind::Timed),
            ExecutionPriorityTier::Timed
        );
        assert_eq!(
            execution_priority_tier(RequestKind::Compound),
            ExecutionPriorityTier::Compound
        );
        assert_eq!(
            execution_priority_tier(RequestKind::CompoundWait),
            ExecutionPriorityTier::CompoundWait
        );
        assert_eq!(
            execution_priority_tier(RequestKind::Chained),
            ExecutionPriorityTier::Chained
        );
        assert_eq!(
            execution_priority_tier(RequestKind::Standard),
            ExecutionPriorityTier::Standard
        );
    }

    #[test]
    //fusa:test REQ-SAFETY-003
    fn execution_priority_tier_maps_each_safety_tagged_variant_to_its_base_kinds_tier() {
        assert_eq!(
            execution_priority_tier(RequestKind::SafetyCompound),
            execution_priority_tier(RequestKind::Compound)
        );
        assert_eq!(
            execution_priority_tier(RequestKind::SafetyCompoundWait),
            execution_priority_tier(RequestKind::CompoundWait)
        );
        assert_eq!(
            execution_priority_tier(RequestKind::SafetyTriggered),
            execution_priority_tier(RequestKind::Triggered)
        );
    }

    #[test]
    //fusa:test REQ-PRIO-003
    fn execution_priority_tier_never_panics_for_any_request_kind() {
        for kind in ALL_REQUEST_KINDS {
            let _ = execution_priority_tier(kind);
        }
    }

    // ── PendingRequestKey / select_next_pending_request ──────────────────────

    #[test]
    //fusa:test REQ-PRIO-004
    fn select_next_pending_request_is_none_for_an_empty_slice() {
        assert_eq!(select_next_pending_request(&[]), None);
    }

    #[test]
    //fusa:test REQ-PRIO-004
    fn select_next_pending_request_picks_the_single_entry() {
        let pending = [PendingRequestKey {
            kind: RequestKind::Standard,
            arrival_seq: 0,
        }];
        assert_eq!(select_next_pending_request(&pending), Some(0));
    }

    #[test]
    //fusa:test REQ-PRIO-004
    fn select_next_pending_request_picks_the_highest_priority_tier_regardless_of_arrival_order() {
        // A later-arriving cancellation request must still win over an
        // earlier-arriving standard request — priority tier dominates FIFO,
        // not the other way around.
        let pending = [
            PendingRequestKey {
                kind: RequestKind::Standard,
                arrival_seq: 0,
            },
            PendingRequestKey {
                kind: RequestKind::Chained,
                arrival_seq: 1,
            },
            PendingRequestKey {
                kind: RequestKind::ClearSingle,
                arrival_seq: 2,
            },
        ];
        assert_eq!(select_next_pending_request(&pending), Some(2));
    }

    #[test]
    //fusa:test REQ-PRIO-004
    fn select_next_pending_request_respects_the_full_roadmap_tier_order() {
        // One entry per tier, deliberately listed out of priority order and
        // out of arrival order, so this test cannot pass by accident of
        // slice order alone.
        let pending = [
            PendingRequestKey {
                kind: RequestKind::Chained,
                arrival_seq: 5,
            },
            PendingRequestKey {
                kind: RequestKind::Standard,
                arrival_seq: 0,
            },
            PendingRequestKey {
                kind: RequestKind::CompoundWait,
                arrival_seq: 4,
            },
            PendingRequestKey {
                kind: RequestKind::Compound,
                arrival_seq: 3,
            },
            PendingRequestKey {
                kind: RequestKind::Timed,
                arrival_seq: 2,
            },
            PendingRequestKey {
                kind: RequestKind::Triggered,
                arrival_seq: 1,
            },
            PendingRequestKey {
                kind: RequestKind::ClearAll,
                arrival_seq: 6,
            },
        ];
        // Index 6 is RequestKind::ClearAll — the cancellation tier, which
        // outranks every other tier present regardless of its arrival_seq.
        assert_eq!(select_next_pending_request(&pending), Some(6));
    }

    #[test]
    //fusa:test REQ-PRIO-004
    fn select_next_pending_request_breaks_same_tier_ties_fifo_by_earliest_arrival() {
        let pending = [
            PendingRequestKey {
                kind: RequestKind::Triggered,
                arrival_seq: 10,
            },
            PendingRequestKey {
                kind: RequestKind::Triggered,
                arrival_seq: 3,
            },
            PendingRequestKey {
                kind: RequestKind::Triggered,
                arrival_seq: 7,
            },
        ];
        // Index 1 carries the earliest arrival_seq (3) among the three
        // same-tier Triggered entries.
        assert_eq!(select_next_pending_request(&pending), Some(1));
    }

    #[test]
    //fusa:test REQ-PRIO-004
    fn select_next_pending_request_prefers_cancellation_regardless_of_which_of_the_three_kinds() {
        for cancellation_kind in [
            RequestKind::ClearAll,
            RequestKind::ClearNonSafestate,
            RequestKind::ClearSingle,
        ] {
            let pending = [
                PendingRequestKey {
                    kind: RequestKind::Triggered,
                    arrival_seq: 0,
                },
                PendingRequestKey {
                    kind: cancellation_kind,
                    arrival_seq: 1,
                },
            ];
            assert_eq!(select_next_pending_request(&pending), Some(1));
        }
    }

    #[test]
    //fusa:test REQ-PRIO-004
    fn select_next_pending_request_never_panics_for_any_sampled_input() {
        for kind in ALL_REQUEST_KINDS {
            for arrival_seq in [0u64, 1, u64::MAX] {
                let pending = [PendingRequestKey { kind, arrival_seq }];
                let _ = select_next_pending_request(&pending);
            }
        }
    }

    // ── RequestLifecycleState: transition-shape definition ──────────────────

    const ALL_LIFECYCLE_STATES: [RequestLifecycleState; 4] = [
        RequestLifecycleState::Pending,
        RequestLifecycleState::Started,
        RequestLifecycleState::UnderExecution,
        RequestLifecycleState::Finalized,
    ];

    #[test]
    //fusa:test REQ-RLC-001
    fn is_request_lifecycle_transition_defined_allows_only_the_three_linear_forward_hops() {
        let defined_pairs = [
            (
                RequestLifecycleState::Pending,
                RequestLifecycleState::Started,
            ),
            (
                RequestLifecycleState::Started,
                RequestLifecycleState::UnderExecution,
            ),
            (
                RequestLifecycleState::UnderExecution,
                RequestLifecycleState::Finalized,
            ),
        ];
        for from in ALL_LIFECYCLE_STATES {
            for to in ALL_LIFECYCLE_STATES {
                let expected = defined_pairs.contains(&(from, to));
                assert_eq!(
                    is_request_lifecycle_transition_defined(from, to),
                    expected,
                    "from={from:?} to={to:?}"
                );
            }
        }
    }

    #[test]
    //fusa:test REQ-RLC-001
    fn is_request_lifecycle_transition_defined_rejects_every_backward_or_identity_pair() {
        for state in ALL_LIFECYCLE_STATES {
            // Identity: staying put is never a defined transition.
            assert!(!is_request_lifecycle_transition_defined(state, state));
        }
        // Backward moves, sampled explicitly.
        assert!(!is_request_lifecycle_transition_defined(
            RequestLifecycleState::Started,
            RequestLifecycleState::Pending
        ));
        assert!(!is_request_lifecycle_transition_defined(
            RequestLifecycleState::Finalized,
            RequestLifecycleState::UnderExecution
        ));
        // Skip: Pending straight to UnderExecution or Finalized.
        assert!(!is_request_lifecycle_transition_defined(
            RequestLifecycleState::Pending,
            RequestLifecycleState::UnderExecution
        ));
        assert!(!is_request_lifecycle_transition_defined(
            RequestLifecycleState::Pending,
            RequestLifecycleState::Finalized
        ));
        assert!(!is_request_lifecycle_transition_defined(
            RequestLifecycleState::Started,
            RequestLifecycleState::Finalized
        ));
    }

    // ── RequestLifecycleState::try_transition: undefined-shape rejection ────

    #[test]
    //fusa:test REQ-RLC-002
    fn try_transition_rejects_every_undefined_shape_regardless_of_input() {
        for from in ALL_LIFECYCLE_STATES {
            for to in ALL_LIFECYCLE_STATES {
                if is_request_lifecycle_transition_defined(from, to) {
                    continue;
                }
                assert_eq!(
                    from.try_transition(to, &RequestLifecycleGuardInput::Standard),
                    Err(RcpError::RequestRejected),
                    "from={from:?} to={to:?}"
                );
            }
        }
    }

    // ── RequestLifecycleState::try_transition: Pending -> Started guards ────

    #[test]
    //fusa:test REQ-RLC-002
    //fusa:test REQ-RLC-003
    fn try_transition_pending_to_started_passes_unconditionally_for_ungated_kinds() {
        for input in [
            RequestLifecycleGuardInput::Standard,
            RequestLifecycleGuardInput::Chained {
                cs: true,
                predecessor_errored: true,
            },
            RequestLifecycleGuardInput::ClearAll,
            RequestLifecycleGuardInput::ClearNonSafestate,
            RequestLifecycleGuardInput::ClearSingle,
            RequestLifecycleGuardInput::Triggered {
                step: TriggeredStep::default(),
                runtime: TriggeredRuntime::default(),
                elapsed: 0,
                endpoint_idle: true,
            },
        ] {
            assert_eq!(
                RequestLifecycleState::Pending
                    .try_transition(RequestLifecycleState::Started, &input),
                Ok(RequestLifecycleState::Started)
            );
        }
    }

    #[test]
    //fusa:test REQ-RLC-002
    //fusa:test REQ-RLC-003
    fn try_transition_pending_to_started_gates_timed_on_is_timed_request_ready() {
        let exec_time = TimedExecutionTime(AvtpTimestamp::new(1_000));

        let not_ready = RequestLifecycleGuardInput::Timed {
            current: AvtpTimestamp::new(500),
            exec_time,
        };
        assert_eq!(
            RequestLifecycleState::Pending
                .try_transition(RequestLifecycleState::Started, &not_ready),
            Err(RcpError::RequestRejected)
        );

        let ready = RequestLifecycleGuardInput::Timed {
            current: AvtpTimestamp::new(1_000),
            exec_time,
        };
        assert_eq!(
            RequestLifecycleState::Pending.try_transition(RequestLifecycleState::Started, &ready),
            Ok(RequestLifecycleState::Started)
        );
    }

    #[test]
    //fusa:test REQ-RLC-002
    //fusa:test REQ-RLC-003
    fn try_transition_pending_to_started_gates_compound_and_compound_wait_on_check_compound_gate() {
        let gate = CompoundGateConfig {
            sequencer_num: 0,
            start_state: SequencerState(1),
        };

        for make_input in [
            (|current_sequencer_state, gate, svr_sequencers_max| {
                RequestLifecycleGuardInput::Compound {
                    current_sequencer_state,
                    gate,
                    svr_sequencers_max,
                }
            })
                as fn(SequencerState, CompoundGateConfig, u8) -> RequestLifecycleGuardInput,
            (|current_sequencer_state, gate, svr_sequencers_max| {
                RequestLifecycleGuardInput::CompoundWait {
                    current_sequencer_state,
                    gate,
                    svr_sequencers_max,
                }
            })
                as fn(SequencerState, CompoundGateConfig, u8) -> RequestLifecycleGuardInput,
        ] {
            // Gate satisfied.
            let satisfied = make_input(SequencerState(1), gate, 4);
            assert_eq!(
                RequestLifecycleState::Pending
                    .try_transition(RequestLifecycleState::Started, &satisfied),
                Ok(RequestLifecycleState::Started)
            );

            // Sequencer known, but not in start state.
            let not_satisfied = make_input(SequencerState(2), gate, 4);
            assert_eq!(
                RequestLifecycleState::Pending
                    .try_transition(RequestLifecycleState::Started, &not_satisfied),
                Err(RcpError::RequestRejected)
            );

            // Sequencer out of bounds.
            let unknown_sequencer = make_input(SequencerState(1), gate, 0);
            assert_eq!(
                RequestLifecycleState::Pending
                    .try_transition(RequestLifecycleState::Started, &unknown_sequencer),
                Err(RcpError::SequencerNotKnown)
            );
        }
    }

    // ── RequestLifecycleState::try_transition: Started -> UnderExecution
    //    guards ──────────────────────────────────────────────────────────────

    #[test]
    //fusa:test REQ-RLC-002
    //fusa:test REQ-RLC-004
    fn try_transition_started_to_under_execution_passes_unconditionally_for_ungated_kinds() {
        let gate = CompoundGateConfig {
            sequencer_num: 0,
            start_state: SequencerState(1),
        };
        for input in [
            RequestLifecycleGuardInput::Standard,
            RequestLifecycleGuardInput::ClearAll,
            RequestLifecycleGuardInput::ClearNonSafestate,
            RequestLifecycleGuardInput::ClearSingle,
            RequestLifecycleGuardInput::Timed {
                current: AvtpTimestamp::new(0),
                exec_time: TimedExecutionTime(AvtpTimestamp::new(0)),
            },
            RequestLifecycleGuardInput::Compound {
                current_sequencer_state: SequencerState(9),
                gate,
                svr_sequencers_max: 0,
            },
            RequestLifecycleGuardInput::CompoundWait {
                current_sequencer_state: SequencerState(9),
                gate,
                svr_sequencers_max: 0,
            },
        ] {
            assert_eq!(
                RequestLifecycleState::Started
                    .try_transition(RequestLifecycleState::UnderExecution, &input),
                Ok(RequestLifecycleState::UnderExecution)
            );
        }
    }

    #[test]
    //fusa:test REQ-RLC-002
    //fusa:test REQ-RLC-004
    fn try_transition_started_to_under_execution_gates_chained_on_check_chain_continuation() {
        let continues = RequestLifecycleGuardInput::Chained {
            cs: true,
            predecessor_errored: false,
        };
        assert_eq!(
            RequestLifecycleState::Started
                .try_transition(RequestLifecycleState::UnderExecution, &continues),
            Ok(RequestLifecycleState::UnderExecution)
        );

        let aborts = RequestLifecycleGuardInput::Chained {
            cs: true,
            predecessor_errored: true,
        };
        assert_eq!(
            RequestLifecycleState::Started
                .try_transition(RequestLifecycleState::UnderExecution, &aborts),
            Err(RcpError::ChainAborted)
        );
    }

    #[test]
    //fusa:test REQ-RLC-002
    //fusa:test REQ-RLC-004
    fn try_transition_started_to_under_execution_gates_triggered_on_fire_readiness() {
        let step = TriggeredStep {
            trigger_source_ep: 3,
            trigger_signal_nr: 2,
            trigger_threshold: 1,
            exec_delay: 5,
            repeat_count: 0,
        };
        let mut runtime = TriggeredRuntime::default();
        runtime.enter_started();
        runtime.record_occurrence(&step, 3, 2);
        runtime.record_occurrence(&step, 3, 2);
        let input = |runtime, elapsed, endpoint_idle| RequestLifecycleGuardInput::Triggered {
            step,
            runtime,
            elapsed,
            endpoint_idle,
        };
        let hop = |i: &RequestLifecycleGuardInput| {
            RequestLifecycleState::Started.try_transition(RequestLifecycleState::UnderExecution, i)
        };
        assert_eq!(
            hop(&input(runtime, 5, true)),
            Ok(RequestLifecycleState::UnderExecution)
        );
        // Delay not yet elapsed, endpoint busy, or threshold not reached: held.
        assert_eq!(
            hop(&input(runtime, 4, true)),
            Err(RcpError::RequestRejected)
        );
        assert_eq!(
            hop(&input(runtime, 5, false)),
            Err(RcpError::RequestRejected)
        );
        let mut one = TriggeredRuntime::default();
        one.enter_started();
        one.record_occurrence(&step, 3, 2);
        assert_eq!(hop(&input(one, 5, true)), Err(RcpError::RequestRejected));
        // Not started: never fires.
        assert_eq!(
            hop(&input(TriggeredRuntime::default(), 5, true)),
            Err(RcpError::RequestRejected)
        );
    }

    // ── RequestLifecycleState::try_transition: UnderExecution -> Finalized
    //    is unconditional ───────────────────────────────────────────────────

    #[test]
    //fusa:test REQ-RLC-002
    //fusa:test REQ-RLC-005
    fn try_transition_under_execution_to_finalized_is_unconditional_for_every_kind() {
        let gate = CompoundGateConfig {
            sequencer_num: 0,
            start_state: SequencerState(1),
        };
        let inputs = [
            RequestLifecycleGuardInput::Standard,
            RequestLifecycleGuardInput::Chained {
                cs: true,
                predecessor_errored: true,
            },
            RequestLifecycleGuardInput::ClearAll,
            RequestLifecycleGuardInput::ClearNonSafestate,
            RequestLifecycleGuardInput::ClearSingle,
            RequestLifecycleGuardInput::Timed {
                current: AvtpTimestamp::new(0),
                exec_time: TimedExecutionTime(AvtpTimestamp::new(u32::MAX)),
            },
            RequestLifecycleGuardInput::CompoundWait {
                current_sequencer_state: SequencerState(9),
                gate,
                svr_sequencers_max: 0,
            },
            RequestLifecycleGuardInput::Triggered {
                step: TriggeredStep::default(),
                runtime: TriggeredRuntime::default(),
                elapsed: 0,
                endpoint_idle: true,
            },
            RequestLifecycleGuardInput::Compound {
                current_sequencer_state: SequencerState(9),
                gate,
                svr_sequencers_max: 0,
            },
        ];
        for input in inputs {
            assert_eq!(
                RequestLifecycleState::UnderExecution
                    .try_transition(RequestLifecycleState::Finalized, &input),
                Ok(RequestLifecycleState::Finalized)
            );
        }
    }

    #[test]
    //fusa:test REQ-RLC-002
    fn try_transition_never_panics_for_any_sampled_state_pair_or_input() {
        let gate = CompoundGateConfig {
            sequencer_num: 0,
            start_state: SequencerState(1),
        };
        let inputs = [
            RequestLifecycleGuardInput::Standard,
            RequestLifecycleGuardInput::Chained {
                cs: false,
                predecessor_errored: false,
            },
            RequestLifecycleGuardInput::ClearAll,
            RequestLifecycleGuardInput::ClearNonSafestate,
            RequestLifecycleGuardInput::ClearSingle,
            RequestLifecycleGuardInput::Timed {
                current: AvtpTimestamp::new(0),
                exec_time: TimedExecutionTime(AvtpTimestamp::new(0)),
            },
            RequestLifecycleGuardInput::CompoundWait {
                current_sequencer_state: SequencerState(0),
                gate,
                svr_sequencers_max: 1,
            },
            RequestLifecycleGuardInput::Triggered {
                step: TriggeredStep::default(),
                runtime: TriggeredRuntime::default(),
                elapsed: 0,
                endpoint_idle: true,
            },
            RequestLifecycleGuardInput::Compound {
                current_sequencer_state: SequencerState(0),
                gate,
                svr_sequencers_max: 1,
            },
        ];
        for from in ALL_LIFECYCLE_STATES {
            for to in ALL_LIFECYCLE_STATES {
                for input in &inputs {
                    let _ = from.try_transition(to, input);
                }
            }
        }
    }

    // ── Cancellation trio: force-canceling a target request ─────────────────

    #[test]
    //fusa:test REQ-RLC-006
    fn try_force_cancel_all_always_finalizes_and_returns_request_canceled() {
        for mut state in [
            RequestLifecycleState::Pending,
            RequestLifecycleState::Started,
            RequestLifecycleState::UnderExecution,
        ] {
            let result = try_force_cancel_all(&mut state);
            assert_eq!(result, Err(RcpError::RequestCanceled));
            assert_eq!(state, RequestLifecycleState::Finalized);
        }
    }

    #[test]
    //fusa:test REQ-RLC-006
    fn try_force_cancel_all_is_idempotent_once_already_finalized() {
        let mut state = RequestLifecycleState::Finalized;
        assert_eq!(try_force_cancel_all(&mut state), Ok(()));
        assert_eq!(state, RequestLifecycleState::Finalized);
    }

    #[test]
    //fusa:test REQ-RLC-006
    fn try_force_cancel_non_safestate_leaves_safestate_related_requests_untouched() {
        let mut state = RequestLifecycleState::UnderExecution;
        assert_eq!(try_force_cancel_non_safestate(&mut state, true), Ok(()));
        assert_eq!(state, RequestLifecycleState::UnderExecution);
    }

    #[test]
    //fusa:test REQ-RLC-006
    fn try_force_cancel_non_safestate_finalizes_non_safestate_related_requests() {
        let mut state = RequestLifecycleState::UnderExecution;
        assert_eq!(
            try_force_cancel_non_safestate(&mut state, false),
            Err(RcpError::RequestCanceled)
        );
        assert_eq!(state, RequestLifecycleState::Finalized);
    }

    #[test]
    //fusa:test REQ-RLC-006
    fn try_force_cancel_single_finalizes_only_the_matching_transaction() {
        let target = ClearTransactionNum(7);

        let mut matching = RequestLifecycleState::Started;
        assert_eq!(
            try_force_cancel_single(&mut matching, 7, target),
            Err(RcpError::RequestCanceled)
        );
        assert_eq!(matching, RequestLifecycleState::Finalized);

        let mut non_matching = RequestLifecycleState::Started;
        assert_eq!(
            try_force_cancel_single(&mut non_matching, 8, target),
            Ok(())
        );
        assert_eq!(non_matching, RequestLifecycleState::Started);
    }

    #[test]
    //fusa:test REQ-RLC-006
    fn force_cancel_functions_never_panic_for_any_sampled_input() {
        for state in ALL_LIFECYCLE_STATES {
            let mut s = state;
            let _ = try_force_cancel_all(&mut s);

            let mut s = state;
            let _ = try_force_cancel_non_safestate(&mut s, true);
            let mut s = state;
            let _ = try_force_cancel_non_safestate(&mut s, false);

            let mut s = state;
            let _ = try_force_cancel_single(&mut s, 0, ClearTransactionNum(0));
            let mut s = state;
            let _ = try_force_cancel_single(&mut s, 1, ClearTransactionNum(0));
        }
    }

    // ── check_compound_bundle_claim ─────────────────────────────────────────

    #[test]
    //fusa:test REQ-BUNDLE-001
    fn check_compound_bundle_claim_accepts_all_three_prerequisites_together() {
        assert_eq!(check_compound_bundle_claim(true, 4, true), Ok(()));
        // More than the minimum sequencer count is also honest.
        assert_eq!(check_compound_bundle_claim(true, 8, true), Ok(()));
        assert_eq!(check_compound_bundle_claim(true, u8::MAX, true), Ok(()));
    }

    #[test]
    //fusa:test REQ-BUNDLE-001
    fn check_compound_bundle_claim_rejects_missing_compound_wait() {
        assert_eq!(
            check_compound_bundle_claim(false, 4, true),
            Err(RcpError::InvalidParameter)
        );
    }

    #[test]
    //fusa:test REQ-BUNDLE-002
    fn check_compound_bundle_claim_rejects_too_few_sequencers() {
        assert_eq!(
            check_compound_bundle_claim(true, 3, true),
            Err(RcpError::InvalidParameter)
        );
        assert_eq!(
            check_compound_bundle_claim(true, 0, true),
            Err(RcpError::InvalidParameter)
        );
    }

    #[test]
    //fusa:test REQ-BUNDLE-001
    fn check_compound_bundle_claim_rejects_missing_clear_non_safestate() {
        assert_eq!(
            check_compound_bundle_claim(true, 4, false),
            Err(RcpError::InvalidParameter)
        );
    }

    #[test]
    //fusa:test REQ-BUNDLE-001
    fn check_compound_bundle_claim_rejects_compound_message_parsing_alone() {
        // The checklist's own named failure case: none of the three real
        // prerequisites are met, only (implicitly) the ability to decode a
        // compound-request message, which this function does not accept as
        // input at all.
        assert_eq!(
            check_compound_bundle_claim(false, 0, false),
            Err(RcpError::InvalidParameter)
        );
    }

    #[test]
    //fusa:test REQ-BUNDLE-002
    fn check_compound_bundle_claim_never_panics_for_any_sampled_input() {
        for has_compound_wait in [false, true] {
            for svr_sequencers_max in [0, 1, 3, 4, 5, u8::MAX] {
                for has_clear_non_safestate in [false, true] {
                    let _ = check_compound_bundle_claim(
                        has_compound_wait,
                        svr_sequencers_max,
                        has_clear_non_safestate,
                    );
                }
            }
        }
    }

    // ── RequestKind::is_safety_tagged ─────────────────────────────────────────

    #[test]
    //fusa:test REQ-SAFETY-002
    fn is_safety_tagged_is_true_only_for_the_three_safety_variants() {
        for kind in [
            RequestKind::SafetyCompound,
            RequestKind::SafetyCompoundWait,
            RequestKind::SafetyTriggered,
        ] {
            assert!(kind.is_safety_tagged());
        }
        for kind in [
            RequestKind::Standard,
            RequestKind::Chained,
            RequestKind::ClearAll,
            RequestKind::ClearNonSafestate,
            RequestKind::ClearSingle,
            RequestKind::Timed,
            RequestKind::CompoundWait,
            RequestKind::Triggered,
            RequestKind::Compound,
        ] {
            assert!(!kind.is_safety_tagged());
        }
    }

    // ── check_watchdog_overflow_purge ──────────────────────────────────────────

    #[test]
    //fusa:test REQ-SAFETY-004
    fn check_watchdog_overflow_purge_keeps_everything_when_not_overflowed() {
        for kind in ALL_REQUEST_KINDS {
            assert_eq!(check_watchdog_overflow_purge(kind, false), Ok(()));
        }
    }

    #[test]
    //fusa:test REQ-SAFETY-004
    fn check_watchdog_overflow_purge_purges_normal_priority_kinds_on_overflow() {
        for kind in [
            RequestKind::Standard,
            RequestKind::Chained,
            RequestKind::ClearAll,
            RequestKind::ClearNonSafestate,
            RequestKind::ClearSingle,
            RequestKind::Timed,
            RequestKind::CompoundWait,
            RequestKind::Triggered,
            RequestKind::Compound,
        ] {
            assert_eq!(
                check_watchdog_overflow_purge(kind, true),
                Err(RcpError::RequestCanceled)
            );
        }
    }

    #[test]
    //fusa:test REQ-SAFETY-004
    fn check_watchdog_overflow_purge_exempts_safety_tagged_kinds_on_overflow() {
        for kind in [
            RequestKind::SafetyCompound,
            RequestKind::SafetyCompoundWait,
            RequestKind::SafetyTriggered,
        ] {
            assert_eq!(check_watchdog_overflow_purge(kind, true), Ok(()));
        }
    }

    #[test]
    //fusa:test REQ-SAFETY-004
    fn check_watchdog_overflow_purge_never_panics_for_any_sampled_input() {
        for kind in ALL_REQUEST_KINDS {
            for watchdog_overflowed in [false, true] {
                let _ = check_watchdog_overflow_purge(kind, watchdog_overflowed);
            }
        }
    }

    // ── purge_normal_priority_on_watchdog_overflow ─────────────────────────────

    #[test]
    //fusa:test REQ-SAFETY-005
    fn purge_normal_priority_on_watchdog_overflow_is_a_no_op_for_an_empty_slice() {
        assert_eq!(
            purge_normal_priority_on_watchdog_overflow(&[], true),
            (vec![], vec![])
        );
        assert_eq!(
            purge_normal_priority_on_watchdog_overflow(&[], false),
            (vec![], vec![])
        );
    }

    #[test]
    //fusa:test REQ-SAFETY-005
    fn purge_normal_priority_on_watchdog_overflow_keeps_everything_without_overflow() {
        let pending = [
            PendingRequestKey {
                kind: RequestKind::Standard,
                arrival_seq: 0,
            },
            PendingRequestKey {
                kind: RequestKind::SafetyCompound,
                arrival_seq: 1,
            },
            PendingRequestKey {
                kind: RequestKind::Triggered,
                arrival_seq: 2,
            },
        ];
        assert_eq!(
            purge_normal_priority_on_watchdog_overflow(&pending, false),
            (vec![0, 1, 2], vec![])
        );
    }

    #[test]
    //fusa:test REQ-SAFETY-005
    fn purge_normal_priority_on_watchdog_overflow_purges_normal_keeps_safety_tagged() {
        let pending = [
            PendingRequestKey {
                kind: RequestKind::Standard,
                arrival_seq: 0,
            },
            PendingRequestKey {
                kind: RequestKind::SafetyCompound,
                arrival_seq: 1,
            },
            PendingRequestKey {
                kind: RequestKind::Triggered,
                arrival_seq: 2,
            },
            PendingRequestKey {
                kind: RequestKind::SafetyTriggered,
                arrival_seq: 3,
            },
            PendingRequestKey {
                kind: RequestKind::ClearAll,
                arrival_seq: 4,
            },
            PendingRequestKey {
                kind: RequestKind::SafetyCompoundWait,
                arrival_seq: 5,
            },
        ];
        assert_eq!(
            purge_normal_priority_on_watchdog_overflow(&pending, true),
            (vec![1, 3, 5], vec![0, 2, 4])
        );
    }

    #[test]
    //fusa:test REQ-SAFETY-005
    fn purge_normal_priority_on_watchdog_overflow_all_safety_tagged_keeps_all() {
        let pending = [
            PendingRequestKey {
                kind: RequestKind::SafetyCompound,
                arrival_seq: 0,
            },
            PendingRequestKey {
                kind: RequestKind::SafetyCompoundWait,
                arrival_seq: 1,
            },
            PendingRequestKey {
                kind: RequestKind::SafetyTriggered,
                arrival_seq: 2,
            },
        ];
        assert_eq!(
            purge_normal_priority_on_watchdog_overflow(&pending, true),
            (vec![0, 1, 2], vec![])
        );
    }

    #[test]
    //fusa:test REQ-SAFETY-005
    fn purge_normal_priority_on_watchdog_overflow_all_normal_purges_all() {
        let pending = [
            PendingRequestKey {
                kind: RequestKind::Standard,
                arrival_seq: 0,
            },
            PendingRequestKey {
                kind: RequestKind::Compound,
                arrival_seq: 1,
            },
        ];
        assert_eq!(
            purge_normal_priority_on_watchdog_overflow(&pending, true),
            (vec![], vec![0, 1])
        );
    }

    #[test]
    //fusa:test REQ-SAFETY-005
    fn purge_normal_priority_on_watchdog_overflow_never_panics_for_any_sampled_input() {
        for kind in ALL_REQUEST_KINDS {
            for watchdog_overflowed in [false, true] {
                let pending = [PendingRequestKey {
                    kind,
                    arrival_seq: 0,
                }];
                let _ = purge_normal_priority_on_watchdog_overflow(&pending, watchdog_overflowed);
            }
        }
    }

    // ── Per-stream safety config: rx_enforce_e2e ─────────────────────────────

    #[test]
    //fusa:test REQ-E2EENF-001
    fn e2e_failure_scope_selects_by_rx_enforce_e2e() {
        assert_eq!(e2e_failure_scope(false), E2eFailureScope::DropRequest);
        assert_eq!(e2e_failure_scope(true), E2eFailureScope::LatchStream);
    }

    #[test]
    //fusa:test REQ-E2EENF-002
    fn check_rx_enforce_e2e_accepts_matching_crc() {
        let buffer = b"safe-point coverage bytes";
        let expected = crate::e2e::crc32_tc18(buffer);
        assert_eq!(check_rx_enforce_e2e(buffer, expected, false), Ok(()));
        assert_eq!(check_rx_enforce_e2e(buffer, expected, true), Ok(()));
    }

    #[test]
    //fusa:test REQ-E2EENF-002
    //fusa:test REQ-CRC-011
    fn check_rx_enforce_e2e_reports_scope_on_mismatch() {
        let buffer = b"safe-point coverage bytes";
        let wrong = crate::e2e::crc32_tc18(buffer).wrapping_add(1);
        assert_eq!(
            check_rx_enforce_e2e(buffer, wrong, false),
            Err((RcpError::CrcError, E2eFailureScope::DropRequest))
        );
        assert_eq!(
            check_rx_enforce_e2e(buffer, wrong, true),
            Err((RcpError::CrcError, E2eFailureScope::LatchStream))
        );
    }

    #[test]
    //fusa:test REQ-E2EENF-002
    fn check_rx_enforce_e2e_never_panics_for_any_sampled_input() {
        for buffer in [&b""[..], &b"x"[..], &[0u8; 64][..]] {
            for expected in [0u32, 1, u32::MAX] {
                for rx_enforce_e2e in [false, true] {
                    let _ = check_rx_enforce_e2e(buffer, expected, rx_enforce_e2e);
                }
            }
        }
    }

    // ── RcpError::CrcError ────────────────────────────────────────────────

    #[test]
    //fusa:test REQ-CRC-011
    fn crc_error_is_distinct_from_other_rcperror_variants() {
        // The legacy `RcpError::CrcMismatch` sentinel this variant was
        // originally kept distinct from (see this module's "Provenance
        // note: CrcError as a new variant..." doc comment) was itself
        // retired by Milestone 9's `e2e` REPLACE cutover, so this test now
        // checks CrcError's distinctness against a still-live variant
        // instead.
        assert_ne!(RcpError::CrcError, RcpError::ChainError);
        assert_eq!(RcpError::CrcError, RcpError::CrcError);
    }

    #[test]
    //fusa:test REQ-CRC-011
    fn crc_error_carries_the_roadmap_named_code_in_its_display_text() {
        assert!(RcpError::CrcError.to_string().contains("CRC_ERROR"));
    }

    // ── Per-stream safety config: rx_safety_measure / rx_safestate_sequencer /
    //    rx_safe_sequencer_state ────────────────────────────────────────────

    #[test]
    //fusa:test REQ-SAFEMEAS-002
    fn safe_state_sequencer_gate_carries_both_fields_through() {
        let gate = safe_state_sequencer_gate(3, 7);
        assert_eq!(gate.sequencer_num, 3);
        assert_eq!(gate.start_state, SequencerState(7));
    }

    #[test]
    //fusa:test REQ-SAFEMEAS-001
    fn resolve_safe_state_mechanism_selects_by_rx_safety_measure() {
        assert_eq!(
            resolve_safe_state_mechanism(false, 3, 7),
            SafeStateMechanism::HiZAllPins
        );
        assert_eq!(
            resolve_safe_state_mechanism(true, 3, 7),
            SafeStateMechanism::SequencerDriven(safe_state_sequencer_gate(3, 7))
        );
    }

    #[test]
    //fusa:test REQ-SAFEMEAS-004
    fn force_state_writes_unconditionally_even_outside_start_state() {
        let mut bank = SequencerBank::new(4);
        let gate = CompoundGateConfig {
            sequencer_num: 1,
            start_state: SequencerState(9),
        };
        // The sequencer is at its power-on default, not `gate.start_state` —
        // an ordinary advance would refuse; `force_state` must not.
        assert_ne!(bank.read(1).unwrap(), gate.start_state);
        assert_eq!(bank.force_state(1, SequencerState(9)), Ok(()));
        assert_eq!(bank.read(1).unwrap(), SequencerState(9));
    }

    #[test]
    //fusa:test REQ-SAFEMEAS-004
    fn force_state_rejects_out_of_bounds_sequencer() {
        let mut bank = SequencerBank::new(2);
        assert_eq!(
            bank.force_state(2, SequencerState(1)),
            Err(RcpError::SequencerNotKnown)
        );
    }

    #[test]
    //fusa:test REQ-SAFEMEAS-003
    fn enter_sequencer_driven_safe_state_composes_force_state() {
        let mut bank = SequencerBank::new(4);
        let gate = safe_state_sequencer_gate(2, 5);
        assert_eq!(enter_sequencer_driven_safe_state(&mut bank, &gate), Ok(()));
        assert_eq!(bank.read(2).unwrap(), SequencerState(5));
    }

    #[test]
    //fusa:test REQ-SAFEMEAS-003
    fn enter_sequencer_driven_safe_state_never_panics_for_any_sampled_input() {
        for svr_sequencers_max in [0u8, 1, 4] {
            for sequencer_num in [0u8, 1, 4, u8::MAX] {
                let mut bank = SequencerBank::new(svr_sequencers_max);
                let gate = safe_state_sequencer_gate(sequencer_num, 5);
                let _ = enter_sequencer_driven_safe_state(&mut bank, &gate);
            }
        }
    }

    // ── Per-stream safety config: rx_ovrflw_safestate_enable ─────────────────

    #[test]
    //fusa:test REQ-OVRFLW-001
    //fusa:test REQ-OVRFLW-003
    fn evaluate_request_storage_overflow_no_overflow_ignores_safestate_flag() {
        assert_eq!(
            evaluate_request_storage_overflow(false, false),
            OverflowOutcome::NoOverflow
        );
        assert_eq!(
            evaluate_request_storage_overflow(false, true),
            OverflowOutcome::NoOverflow
        );
    }

    #[test]
    //fusa:test REQ-OVRFLW-003
    fn evaluate_request_storage_overflow_selects_by_safestate_flag() {
        assert_eq!(
            evaluate_request_storage_overflow(true, false),
            OverflowOutcome::OverflowNoSafestate
        );
        assert_eq!(
            evaluate_request_storage_overflow(true, true),
            OverflowOutcome::OverflowSafestate
        );
    }

    #[test]
    //fusa:test REQ-OVRFLW-002
    fn overflow_outcome_predicates_agree_with_variant_identity() {
        assert!(!OverflowOutcome::NoOverflow.is_overflow());
        assert!(!OverflowOutcome::NoOverflow.drives_safestate());
        assert!(OverflowOutcome::OverflowNoSafestate.is_overflow());
        assert!(!OverflowOutcome::OverflowNoSafestate.drives_safestate());
        assert!(OverflowOutcome::OverflowSafestate.is_overflow());
        assert!(OverflowOutcome::OverflowSafestate.drives_safestate());
    }

    // ── Per-stream safety config: rx_enforce_seq / rx_seq_safestate_enable ───

    fn seq_outcome(
        accept: bool,
        discontinuity: bool,
        enter_safe_state: bool,
    ) -> SequenceEnforcementOutcome {
        SequenceEnforcementOutcome {
            accept,
            discontinuity,
            enter_safe_state,
        }
    }

    #[test]
    //fusa:test REQ-E2E-028
    //fusa:test REQ-SEQENF-001
    fn sequence_tracker_first_frame_always_accepts_without_discontinuity() {
        for en in [false, true] {
            for ss in [false, true] {
                let mut t = SequenceTracker::new();
                assert_eq!(t.prev_seq(), None);
                assert_eq!(t.evaluate(en, ss, 77), seq_outcome(true, false, false));
                assert_eq!(t.prev_seq(), Some(77));
            }
        }
    }

    #[test]
    //fusa:test REQ-E2E-028
    //fusa:test REQ-SEQENF-003
    fn sequence_tracker_accepts_the_255_to_0_wrap_as_a_single_increment() {
        let mut t = SequenceTracker::new();
        t.evaluate(true, true, 254);
        assert_eq!(t.evaluate(true, true, 255), seq_outcome(true, false, false));
        assert_eq!(
            evaluate_rx_enforce_seq(&mut t, true, true, 0),
            seq_outcome(true, false, false)
        );
        assert_eq!(t.evaluate(true, true, 1), seq_outcome(true, false, false));
        assert_eq!(t.prev_seq(), Some(1));
    }

    #[test]
    //fusa:test REQ-E2E-028
    //fusa:test REQ-E2E-029
    fn sequence_tracker_window_edge_plus_127_accepts_plus_128_rejects() {
        // +127 is the last forward distance inside the RFC 1982 window: accepted,
        // but a discontinuity (gap), so safe state when enabled.
        let mut t = SequenceTracker::new();
        t.evaluate(true, true, 200);
        assert_eq!(
            t.evaluate(true, true, 200u8.wrapping_add(127)),
            seq_outcome(true, true, true)
        );
        assert_eq!(t.prev_seq(), Some(200u8.wrapping_add(127)));

        // +128 is outside the window: rejected, prev_seq unchanged.
        let mut t = SequenceTracker::new();
        t.evaluate(true, false, 200);
        assert_eq!(
            t.evaluate(true, false, 200u8.wrapping_add(128)),
            seq_outcome(false, true, false)
        );
        assert_eq!(t.prev_seq(), Some(200));
    }

    #[test]
    //fusa:test REQ-E2E-028
    //fusa:test REQ-E2E-029
    fn sequence_tracker_rejects_duplicates_and_flags_safe_state_on_them() {
        let mut t = SequenceTracker::new();
        t.evaluate(true, true, 9);
        let dup = t.evaluate(true, true, 9);
        assert_eq!(dup, seq_outcome(false, true, true));
        assert!(dup.is_rejected());
        assert!(dup.drives_safestate());
        // With rx_enforce_seq off a duplicate is still accepted (and still a
        // discontinuity) — c-RCP parity: safe state is independent of accept.
        let mut t = SequenceTracker::new();
        t.evaluate(false, true, 9);
        let dup = t.evaluate(false, true, 9);
        assert_eq!(dup, seq_outcome(true, true, true));
        assert!(!dup.is_rejected());
        assert!(dup.drives_safestate());
    }

    #[test]
    //fusa:test REQ-SEQENF-002
    fn sequence_enforcement_outcome_predicates_read_their_fields() {
        assert!(!seq_outcome(true, false, false).is_rejected());
        assert!(!seq_outcome(true, false, false).drives_safestate());
        assert!(seq_outcome(false, true, false).is_rejected());
        assert!(!seq_outcome(false, true, false).drives_safestate());
        assert!(!seq_outcome(true, true, true).is_rejected());
        assert!(seq_outcome(true, true, true).drives_safestate());
    }

    /// Golden vectors generated from c-RCP's real `rcp_e2e_seq_evaluate()`
    /// (`c-RCP/src/e2e.c:516`) by a C harness linked against `librcp.a`:
    /// per `(rx_enforce_seq, rx_seq_safestate_enable)`, the `(accept,
    /// discontinuity, enter_safe_state)` triple for each seq in `SEQS`, and
    /// the tracker's final `prev_seq`.
    #[test]
    //fusa:test REQ-E2E-028
    //fusa:test REQ-E2E-029
    //fusa:test REQ-SEQENF-001
    //fusa:test REQ-SEQENF-003
    fn sequence_tracker_matches_c_rcp_golden_vectors() {
        const SEQS: [u8; 13] = [250, 251, 255, 0, 1, 1, 0, 128, 129, 2, 3, 131, 4];
        let golden: [(bool, bool, &str, u8); 4] = [
            (
                false,
                false,
                "100 100 110 100 100 110 110 110 100 110 100 110 110",
                4,
            ),
            (
                false,
                true,
                "100 100 111 100 100 111 111 111 100 111 100 111 111",
                4,
            ),
            (
                true,
                false,
                "100 100 110 100 100 010 010 110 100 010 010 110 010",
                131,
            ),
            (
                true,
                true,
                "100 100 111 100 100 011 011 111 100 011 011 111 011",
                131,
            ),
        ];
        for (en, ss, expected, prev) in golden {
            let mut t = SequenceTracker::new();
            let got: Vec<String> = SEQS
                .iter()
                .map(|&seq| {
                    let r = t.evaluate(en, ss, seq);
                    format!(
                        "{}{}{}",
                        r.accept as u8, r.discontinuity as u8, r.enter_safe_state as u8
                    )
                })
                .collect();
            assert_eq!(got.join(" "), expected, "en={en} ss={ss}");
            assert_eq!(t.prev_seq(), Some(prev), "en={en} ss={ss}");
        }
    }

    #[test]
    //fusa:test REQ-SEQENF-003
    fn evaluate_rx_enforce_seq_never_panics_for_any_input() {
        for prev in 0..=u8::MAX {
            for seq in [0u8, 1, 127, 128, 255] {
                for en in [false, true] {
                    for ss in [false, true] {
                        let mut t = SequenceTracker::new();
                        t.evaluate(en, ss, prev);
                        let _ = evaluate_rx_enforce_seq(&mut t, en, ss, seq);
                    }
                }
            }
        }
    }

    // ── Per-stream safety config: the unifying safe-state action ─────────────

    #[test]
    //fusa:test REQ-SAFEACT-001
    //fusa:test REQ-SAFEACT-002
    fn resolve_safe_state_action_is_none_when_not_entering_safe_state() {
        assert_eq!(
            resolve_safe_state_action(false, SafeStateMechanism::HiZAllPins),
            SafeStateAction::None
        );
        assert_eq!(
            resolve_safe_state_action(
                false,
                SafeStateMechanism::SequencerDriven(safe_state_sequencer_gate(1, 2))
            ),
            SafeStateAction::None
        );
    }

    #[test]
    //fusa:test REQ-SAFEACT-002
    fn resolve_safe_state_action_mirrors_the_mechanism_when_entering() {
        assert_eq!(
            resolve_safe_state_action(true, SafeStateMechanism::HiZAllPins),
            SafeStateAction::ForceHiZAllPins
        );
        let gate = safe_state_sequencer_gate(1, 2);
        assert_eq!(
            resolve_safe_state_action(true, SafeStateMechanism::SequencerDriven(gate)),
            SafeStateAction::ForceSequencerState(gate)
        );
    }

    #[test]
    //fusa:test REQ-SAFEACT-002
    fn resolve_safe_state_action_never_panics_for_any_sampled_input() {
        let mechanisms = [
            SafeStateMechanism::HiZAllPins,
            SafeStateMechanism::SequencerDriven(safe_state_sequencer_gate(0, 0)),
            SafeStateMechanism::SequencerDriven(safe_state_sequencer_gate(u8::MAX, u8::MAX)),
        ];
        for should_enter_safe_state in [false, true] {
            for mechanism in mechanisms {
                let _ = resolve_safe_state_action(should_enter_safe_state, mechanism);
            }
        }
    }

    // ── TC18-literal conformance checks ──────────────────────────────────────

    #[test]
    //fusa:test REQ-CMP-009
    fn request_kind_discriminants_match_tc18_table_5_condition_type_bytes() {
        // TC18 §11.2.2, Table 5 "Different types of conditional requests"
        // (TC18.txt lines 1256-1257): "The first byte in the
        // message_timestamp field is used to indicate the type of
        // condition." Table 5's own rows, transcribed as literals:
        //   0x0F, 0x8F -> Compound
        //   0x0B, 0x8B -> Compound wait
        //   0x0E, 0x8E -> Triggered
        //   0x01       -> Chained
        //   0x0A       -> Timed
        assert_eq!(RequestKind::Compound.to_u8(), 0x0F);
        assert_eq!(RequestKind::SafetyCompound.to_u8(), 0x8F);
        assert_eq!(RequestKind::CompoundWait.to_u8(), 0x0B);
        assert_eq!(RequestKind::SafetyCompoundWait.to_u8(), 0x8B);
        assert_eq!(RequestKind::Triggered.to_u8(), 0x0E);
        assert_eq!(RequestKind::SafetyTriggered.to_u8(), 0x8E);
        assert_eq!(RequestKind::Chained.to_u8(), 0x01);
        assert_eq!(RequestKind::Timed.to_u8(), 0x0A);

        // TC18 §11.2.3.1 Table 13 (line 1985) "request_type 0x05",
        // §11.2.3.2 Table 14 (line 2056) "request_type 0x06", and
        // §11.2.3.3 Table 15 (line 2142) "request_type 0x07". (Tables
        // renumbered from 11/12/13 since this note was written — see
        // issue #164.)
        assert_eq!(RequestKind::ClearAll.to_u8(), 0x05);
        assert_eq!(RequestKind::ClearNonSafestate.to_u8(), 0x06);
        assert_eq!(RequestKind::ClearSingle.to_u8(), 0x07);

        // TC18 §11.2.2 (line 1258): "If the MSB of the identifier (0x8x) is
        // set the request is treated as a safety request".
        for (safety, base) in [
            (RequestKind::SafetyCompound, RequestKind::Compound),
            (RequestKind::SafetyCompoundWait, RequestKind::CompoundWait),
            (RequestKind::SafetyTriggered, RequestKind::Triggered),
        ] {
            assert_eq!(safety.to_u8(), 0x80 | base.to_u8());
            assert!(safety.is_safety_tagged());
            assert!(!base.is_safety_tagged());
        }

        // Decode side: those same literal bytes, positioned as Table 5's
        // "first byte in message_timestamp field" — i.e. bits 63:56 of the
        // big-endian 8-octet ACF_GBB message_timestamp.
        for (byte, expected) in [
            (0x0Fu8, RequestKind::Compound),
            (0x8F, RequestKind::SafetyCompound),
            (0x0B, RequestKind::CompoundWait),
            (0x8B, RequestKind::SafetyCompoundWait),
            (0x0E, RequestKind::Triggered),
            (0x8E, RequestKind::SafetyTriggered),
            (0x01, RequestKind::Chained),
            (0x0A, RequestKind::Timed),
            (0x05, RequestKind::ClearAll),
            (0x06, RequestKind::ClearNonSafestate),
            (0x07, RequestKind::ClearSingle),
        ] {
            let message_timestamp = (byte as u64) << 56;
            assert_eq!(
                RequestKind::from_gbb_message_timestamp(message_timestamp),
                Some(expected),
                "Table 5 condition-type byte {byte:#04X}"
            );
        }
    }

    #[test]
    //fusa:test REQ-SEQ-005
    fn sequencer_bank_and_state_stay_within_tc18_256_sequencer_and_state_ceiling() {
        // TC18 §12.10 "Sequencers" (TC18.txt lines 3870-3871): "The number of
        // sequencers and states per sequencer are limited to 256 by this
        // definition. An RC Server implementation may support only a lower
        // number of sequencers."
        const TC18_SEQUENCER_LIMIT: usize = 256;
        const TC18_STATES_PER_SEQUENCER_LIMIT: usize = 256;

        // `SequencerState` is one octet wide, so it spans exactly TC18's
        // 256-state ceiling and every one of those states is distinct.
        let all_states: Vec<SequencerState> = (0..=u8::MAX).map(SequencerState).collect();
        assert_eq!(all_states.len(), TC18_STATES_PER_SEQUENCER_LIMIT);
        for (i, a) in all_states.iter().enumerate() {
            for b in &all_states[i + 1..] {
                assert_ne!(a, b);
            }
        }

        // The widest bank this crate can build is bounded by `u8`-valued
        // `svr_sequencers_max`, so it never exceeds TC18's 256-sequencer
        // ceiling. TC18 §12.10 also fixes the power-on state at 1:
        // "After power-on/reset all sequencers are in state 1."
        let bank = SequencerBank::new(u8::MAX);
        assert!(bank.svr_sequencers_max() as usize <= TC18_SEQUENCER_LIMIT);
        assert_eq!(bank.svr_sequencers_max(), 255);
        assert_eq!(bank.read(0), Ok(SequencerState(1)));
        assert_eq!(bank.read(254), Ok(SequencerState(1)));
        // Sequencer number 255 is beyond this crate's own `u8` bound, and so
        // is unreachable even though TC18's ceiling would admit it.
        assert_eq!(bank.read(255), Err(RcpError::SequencerNotKnown));
    }

    #[test]
    //fusa:test REQ-ERRH-001
    fn request_module_error_outcomes_carry_their_tc18_table_27_wire_codes() {
        // TC18 §12.9.6 Table 30 "Error codes in responses" (TC18.txt lines
        // 3812-3844; table renumbered from Table 27 since this note was
        // written — see issue #164), transcribed as literals:
        // SEQUENCER_NOT_KNOWN = 2, REQUEST_CANCELED = 5,
        // REQUEST_REJECTED = 11, INVALID_PARAMETER = 15, CHAIN_ABORTED = 16.

        // TC18 §11.2.2.2 (TC18.txt lines 1384-1385): a compound request
        // naming a sequencer the RC Server does not have.
        let unknown_sequencer = CompoundGateConfig {
            sequencer_num: 4,
            start_state: SequencerState(1),
        };
        assert_eq!(
            check_compound_gate(SequencerState(1), &unknown_sequencer, 4)
                .unwrap_err()
                .tc18_wire_code(),
            Some(2)
        );

        // TC18 §11.2.2.2 (TC18.txt lines 1384-1385): the sequencer is known
        // but is not in the request's cmp_start_state, so the request is
        // not due.
        let unmet_gate = CompoundGateConfig {
            sequencer_num: 0,
            start_state: SequencerState(3),
        };
        assert_eq!(
            check_compound_gate(SequencerState(1), &unmet_gate, 4)
                .unwrap_err()
                .tc18_wire_code(),
            Some(11)
        );

        // TC18 §11.2.3 (TC18.txt lines 1979-1980): "Each request that is
        // cancelled will send an error response with the error code =
        // REQUEST_CANCELED."
        assert_eq!(
            check_clear_all_cancellation().unwrap_err().tc18_wire_code(),
            Some(5)
        );
        assert_eq!(
            check_clear_non_safestate_cancellation(false)
                .unwrap_err()
                .tc18_wire_code(),
            Some(5)
        );
        assert_eq!(
            check_clear_single_cancellation(7, ClearTransactionNum(7))
                .unwrap_err()
                .tc18_wire_code(),
            Some(5)
        );
        // TC18 §11.2.2.2 (TC18.txt lines 1384-1385) / §11.2.2.6 Table 11
        // (TC18.txt line 1847): the watchdog-overflow purge of
        // non-safety-tagged requests is likewise a cancellation.
        assert_eq!(
            check_watchdog_overflow_purge(RequestKind::Compound, true)
                .unwrap_err()
                .tc18_wire_code(),
            Some(5)
        );

        // TC18 §12.9.6 Table 30 (TC18.txt lines 3839-3840): cs = 1 and
        // "error occurred in one of the preceding requests" -> CHAIN_ABORTED.
        // (Not Table 9/§11.2.2.4 as previously cited — that table's own
        // `cs` field is unrelated; this exact phrase is the Table 30
        // CHAIN_ABORTED row's own description text. See §11.2.2.6 Table 11,
        // TC18.txt lines 1833-1838, for the `cs`-bit definition itself.)
        assert_eq!(
            check_chain_continuation(true, true)
                .unwrap_err()
                .tc18_wire_code(),
            Some(16)
        );

        // TC18 §11.2.2 Table 5 (TC18.txt lines 1262-1266) names no 0x02
        // condition type, so decoding one is a parameter out of range ->
        // INVALID_PARAMETER.
        assert_eq!(
            RequestKind::from_u8(0x02).unwrap_err().tc18_wire_code(),
            Some(15)
        );
    }
}
