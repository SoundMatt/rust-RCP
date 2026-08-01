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
//! Chained, [`should_count_trigger_occurrence`]/
//! [`is_trigger_repeat_exhausted`] for Triggered), plus
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
//! - [`should_count_trigger_occurrence`] — the "runs independent of
//!   endpoint busy/idle state" rule, turned into a pure, testable predicate
//!   following this module's own `advance_sequencer_if_still_in_start_state`
//!   precedent. See "Provenance note: busy/idle independence as a
//!   caller-supplied parameter" below.
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
//!   state at all; see [`should_count_trigger_occurrence`] above.
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
//!   [`should_count_trigger_occurrence`]/[`is_trigger_repeat_exhausted`]
//!   (Triggered — whether this occurrence still counts toward a
//!   not-yet-exhausted repeat budget) are composed.
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
//! caller-supplied-rather-than-read precedent above and
//! [`should_count_trigger_occurrence`]'s own caller-supplied-`bool`
//! precedent, rather than this module reading or inventing the not-yet-built
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
//! Mirrors [`should_count_trigger_occurrence`]'s own busy/idle `bool`
//! parameter and [`check_compound_bundle_claim`]'s own three
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
//! - [`SequenceEnforcementOutcome`] / [`evaluate_rx_enforce_seq`] —
//!   `rx_enforce_seq`/`rx_seq_safestate_enable`: whether a candidate
//!   request's sequence number must strictly exceed the last accepted one
//!   before being queued at all, and whether a violation also drives the
//!   stream to safe state. See "Provenance note: the enforced sequence
//!   number's own wire field and width" below for why the sequence number
//!   itself is a caller-supplied, unsourced `u32`.
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
//! `ROADMAP.md`'s checklist bullet names `rx_enforce_seq` only as
//! "monotonically increasing sequence numbers," without stating which
//! already-decoded field carries that sequence number. At the time this
//! item was written, this crate had a second, not obviously equivalent
//! candidate besides the one named below: `crate::e2e`'s own `seqNum` (a
//! `u32` from the CRC-16 + replay-guard frame `ROADMAP.md`'s own Satellite
//! Package Disposition table separately REPLACE-dispositioned) — since
//! removed outright by Milestone 9's `e2e` REPLACE cutover, leaving
//! [`crate::acf::ByteMessageInfo::transaction_num`] (a `u8`, but named for
//! matching a response to its request, not for detecting gaps/reordering)
//! as this crate's only remaining decoded candidate. Per Guiding Principle
//! 5 this item does not guess which one `rx_enforce_seq` actually means.
//! [`evaluate_rx_enforce_seq`] instead takes `last_accepted_seq`/
//! `candidate_seq` as plain caller-supplied `u32` values, wide enough to
//! hold either existing candidate without truncation, flagged here for
//! reconciliation against real TC18 behavior once that field is settled.
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
//fusa:req REQ-TRIG-005
pub fn should_count_trigger_occurrence(endpoint_busy: bool) -> bool {
    let _ = endpoint_busy;
    true
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
    /// [`RequestKind::Triggered`]: [`should_count_trigger_occurrence`] and
    /// [`is_trigger_repeat_exhausted`] together gate the `Started` ->
    /// `UnderExecution` hop.
    Triggered {
        /// The target endpoint's busy/idle state — deliberately
        /// non-gating (see [`should_count_trigger_occurrence`]), carried
        /// here only so this hop composes that function rather than
        /// bypassing it.
        endpoint_busy: bool,
        /// This request's own configured repeat count.
        repeat: TriggerRepeatCount,
        /// Trigger occurrences already counted for this request.
        occurrences_so_far: u16,
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
/// the Triggered repeat-exhaustion check) rather than one that already
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
                endpoint_busy,
                repeat,
                occurrences_so_far,
            },
        ) => {
            if should_count_trigger_occurrence(*endpoint_busy)
                && is_trigger_repeat_exhausted(*occurrences_so_far, *repeat)
            {
                Err(RcpError::RequestRejected)
            } else {
                Ok(())
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

/// The result of evaluating a candidate request's sequence number against
/// `rx_enforce_seq`/`rx_seq_safestate_enable`: see
/// [`evaluate_rx_enforce_seq`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
//fusa:req REQ-SEQENF-001
pub enum SequenceEnforcementOutcome {
    /// Enforcement is disabled, or the candidate sequence number strictly
    /// exceeds the last accepted one: queue the request.
    Accepted,
    /// Enforcement is enabled and violated, and `rx_seq_safestate_enable`
    /// is `false` — reject the request, no safe-state consequence.
    RejectedNoSafestate,
    /// Enforcement is enabled and violated, and `rx_seq_safestate_enable`
    /// is `true` — reject the request and drive every endpoint on this
    /// stream to safe state.
    RejectedSafestate,
}

impl SequenceEnforcementOutcome {
    /// True for either rejected variant. Never panics for any input.
    //fusa:req REQ-SEQENF-002
    pub fn is_rejected(&self) -> bool {
        !matches!(self, Self::Accepted)
    }

    /// True only for [`Self::RejectedSafestate`]. Never panics for any
    /// input.
    //fusa:req REQ-SEQENF-002
    pub fn drives_safestate(&self) -> bool {
        matches!(self, Self::RejectedSafestate)
    }
}

/// The full `rx_enforce_seq`/`rx_seq_safestate_enable` rule: whether
/// `candidate_seq` may be queued for execution at all, and whether a
/// violation also drives the stream's endpoints to safe state.
///
/// Returns [`SequenceEnforcementOutcome::Accepted`] when `rx_enforce_seq`
/// is `false` (an unconditional exemption), or when `candidate_seq` is
/// strictly greater than `last_accepted_seq`. Otherwise (enforcement
/// enabled and `candidate_seq` does not strictly increase) returns
/// [`SequenceEnforcementOutcome::RejectedSafestate`] or
/// [`SequenceEnforcementOutcome::RejectedNoSafestate`], selected by
/// `rx_seq_safestate_enable`. Never panics for any input. See this
/// module's doc comment "Provenance note: the enforced sequence number's
/// own wire field and width" for why `last_accepted_seq`/`candidate_seq`
/// are plain caller-supplied `u32` values.
//fusa:req REQ-SEQENF-003
pub fn evaluate_rx_enforce_seq(
    last_accepted_seq: u32,
    candidate_seq: u32,
    rx_enforce_seq: bool,
    rx_seq_safestate_enable: bool,
) -> SequenceEnforcementOutcome {
    if !rx_enforce_seq || candidate_seq > last_accepted_seq {
        return SequenceEnforcementOutcome::Accepted;
    }
    if rx_seq_safestate_enable {
        SequenceEnforcementOutcome::RejectedSafestate
    } else {
        SequenceEnforcementOutcome::RejectedNoSafestate
    }
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

    // ── should_count_trigger_occurrence ───────────────────────────────────────

    #[test]
    //fusa:test REQ-TRIG-005
    fn should_count_trigger_occurrence_is_always_true_regardless_of_busy_state() {
        assert!(should_count_trigger_occurrence(true));
        assert!(should_count_trigger_occurrence(false));
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
                endpoint_busy: true,
                repeat: TriggerRepeatCount::Finite(0),
                occurrences_so_far: 0,
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
    fn try_transition_started_to_under_execution_gates_triggered_on_repeat_exhaustion() {
        let not_exhausted = RequestLifecycleGuardInput::Triggered {
            endpoint_busy: true,
            repeat: TriggerRepeatCount::Finite(3),
            occurrences_so_far: 2,
        };
        assert_eq!(
            RequestLifecycleState::Started
                .try_transition(RequestLifecycleState::UnderExecution, &not_exhausted),
            Ok(RequestLifecycleState::UnderExecution)
        );

        let exhausted = RequestLifecycleGuardInput::Triggered {
            endpoint_busy: true,
            repeat: TriggerRepeatCount::Finite(3),
            occurrences_so_far: 3,
        };
        assert_eq!(
            RequestLifecycleState::Started
                .try_transition(RequestLifecycleState::UnderExecution, &exhausted),
            Err(RcpError::RequestRejected)
        );

        let infinite_never_exhausts = RequestLifecycleGuardInput::Triggered {
            endpoint_busy: false,
            repeat: TriggerRepeatCount::Infinite,
            occurrences_so_far: u16::MAX,
        };
        assert_eq!(
            RequestLifecycleState::Started.try_transition(
                RequestLifecycleState::UnderExecution,
                &infinite_never_exhausts
            ),
            Ok(RequestLifecycleState::UnderExecution)
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
                endpoint_busy: true,
                repeat: TriggerRepeatCount::Finite(0),
                occurrences_so_far: 0,
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
                endpoint_busy: false,
                repeat: TriggerRepeatCount::Infinite,
                occurrences_so_far: 0,
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

    #[test]
    //fusa:test REQ-SEQENF-001
    //fusa:test REQ-SEQENF-003
    fn evaluate_rx_enforce_seq_accepts_when_disabled_regardless_of_ordering() {
        assert_eq!(
            evaluate_rx_enforce_seq(10, 5, false, false),
            SequenceEnforcementOutcome::Accepted
        );
        assert_eq!(
            evaluate_rx_enforce_seq(10, 5, false, true),
            SequenceEnforcementOutcome::Accepted
        );
    }

    #[test]
    //fusa:test REQ-SEQENF-003
    fn evaluate_rx_enforce_seq_accepts_strictly_increasing_sequence() {
        assert_eq!(
            evaluate_rx_enforce_seq(5, 6, true, true),
            SequenceEnforcementOutcome::Accepted
        );
    }

    #[test]
    //fusa:test REQ-SEQENF-003
    fn evaluate_rx_enforce_seq_rejects_equal_or_decreasing_sequence() {
        assert_eq!(
            evaluate_rx_enforce_seq(5, 5, true, false),
            SequenceEnforcementOutcome::RejectedNoSafestate
        );
        assert_eq!(
            evaluate_rx_enforce_seq(5, 4, true, false),
            SequenceEnforcementOutcome::RejectedNoSafestate
        );
        assert_eq!(
            evaluate_rx_enforce_seq(5, 5, true, true),
            SequenceEnforcementOutcome::RejectedSafestate
        );
    }

    #[test]
    //fusa:test REQ-SEQENF-002
    fn sequence_enforcement_outcome_predicates_agree_with_variant_identity() {
        assert!(!SequenceEnforcementOutcome::Accepted.is_rejected());
        assert!(!SequenceEnforcementOutcome::Accepted.drives_safestate());
        assert!(SequenceEnforcementOutcome::RejectedNoSafestate.is_rejected());
        assert!(!SequenceEnforcementOutcome::RejectedNoSafestate.drives_safestate());
        assert!(SequenceEnforcementOutcome::RejectedSafestate.is_rejected());
        assert!(SequenceEnforcementOutcome::RejectedSafestate.drives_safestate());
    }

    #[test]
    //fusa:test REQ-SEQENF-003
    fn evaluate_rx_enforce_seq_never_panics_for_any_sampled_input() {
        let seqs = [0u32, 1, 5, u32::MAX];
        for &last in &seqs {
            for &candidate in &seqs {
                for rx_enforce_seq in [false, true] {
                    for rx_seq_safestate_enable in [false, true] {
                        let _ = evaluate_rx_enforce_seq(
                            last,
                            candidate,
                            rx_enforce_seq,
                            rx_seq_safestate_enable,
                        );
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
        // (TC18.txt line 1186): "The first byte in the message_timestamp
        // field is used to indicate the type of condition." Table 5's own
        // rows, transcribed as literals:
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

        // TC18 §11.2.3.1 Table 11 (line 1679) "request_type 0x05",
        // §11.2.3.2 Table 12 (line 1733) "request_type 0x06", and
        // §11.2.3.3 Table 13 (line 1792) "request_type 0x07".
        assert_eq!(RequestKind::ClearAll.to_u8(), 0x05);
        assert_eq!(RequestKind::ClearNonSafestate.to_u8(), 0x06);
        assert_eq!(RequestKind::ClearSingle.to_u8(), 0x07);

        // TC18 §11.2.2 (line 1186): "If the MSB of the identifier (0x8x) is
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
        // TC18 §12.10 "Sequencers" (TC18.txt line 3463): "The number of
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
        // TC18 §12.9.6 Table 27 "Error codes in responses" (TC18.txt line
        // 3413), transcribed as literals: SEQUENCER_NOT_KNOWN = 2,
        // REQUEST_CANCELED = 5, REQUEST_REJECTED = 11,
        // INVALID_PARAMETER = 15, CHAIN_ABORTED = 16.

        // TC18 §11.2.2.1 (line 1203): a compound request naming a sequencer
        // the RC Server does not have.
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

        // TC18 §11.2.2.1 (line 1203): the sequencer is known but is not in
        // the request's cmp_start_state, so the request is not due.
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

        // TC18 §11.2.3 (line 1672): "Each request that is cancelled will send
        // an error response with the error code = REQUEST_CANCELED."
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
        // TC18 §11.2.2.1 (line 1203) / §11.2.2.4 Table 9 (line 1586): the
        // watchdog-overflow purge of non-safety-tagged requests is likewise a
        // cancellation.
        assert_eq!(
            check_watchdog_overflow_purge(RequestKind::Compound, true)
                .unwrap_err()
                .tc18_wire_code(),
            Some(5)
        );

        // TC18 §11.2.2.4 Table 9 (line 1586): cs = 1 and "error occurred in
        // one of the preceding requests" -> CHAIN_ABORTED.
        assert_eq!(
            check_chain_continuation(true, true)
                .unwrap_err()
                .tc18_wire_code(),
            Some(16)
        );

        // TC18 §11.2.2 Table 5 (line 1186) names no 0x02 condition type, so
        // decoding one is a parameter out of range -> INVALID_PARAMETER.
        assert_eq!(
            RequestKind::from_u8(0x02).unwrap_err().tc18_wire_code(),
            Some(15)
        );
    }
}
