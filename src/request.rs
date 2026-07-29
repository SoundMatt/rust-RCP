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
// fusa:req REQ-CHAIN-001
// fusa:req REQ-CHAIN-002
// fusa:req REQ-CHAIN-003
// fusa:req REQ-TIME-001
// fusa:req REQ-TIME-002
// fusa:req REQ-TIME-003
// fusa:req REQ-CANCEL-001
// fusa:req REQ-CANCEL-002
// fusa:req REQ-CANCEL-003
// fusa:req REQ-CANCEL-004
// fusa:req REQ-SEQ-001
// fusa:req REQ-SEQ-002
// fusa:req REQ-SEQ-003
// fusa:req REQ-SEQ-004
// fusa:req REQ-PRIO-001
// fusa:req REQ-PRIO-002
// fusa:req REQ-PRIO-003
// fusa:req REQ-PRIO-004

//! Conditional-request taxonomy: compound / compound-wait (`0x0F`/`0x0B`),
//! triggered (`0x0E`), chained (`0x01`), timed (`0x0A`), the
//! cancellation trio clear-all / clear-non-safestate / clear-single
//! (`0x05`/`0x06`/`0x07`), the persistent sequencer-state register bank, and
//! the execution priority ordering that selects which pending request runs
//! next — `ROADMAP.md` Milestone 5 ("Conditional Requests & Sequencers"),
//! first through seventh checklist bullets. The first bullet
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
//!
//! Compound/compound-wait was the opening item of Milestone 5, and the
//! first thing to land in `src/request.rs` — the module name the
//! naming-reconciliation pass (issue #35, PR #37, "refactor: reconcile
//! module naming with RELAY spec v1.14 §13.7.2") reserved for this
//! milestone's request-kind/taxonomy work, mirroring `fragment.rs`'s own
//! reservation for Milestone 8. Triggered is the second, added there.
//! Chained is the third, added there. Timed is the fourth, added there.
//! Cancellation is the fifth, added there. [`SequencerBank`] is the sixth,
//! added in the prior entry this one extends. Execution priority ordering
//! is the seventh, added here. Same "additive standalone
//! plumbing only" discipline as every prior Milestone 1-5 entry, and as the
//! compound/compound-wait, triggered, chained, timed, cancellation, and
//! sequencer-bank work above: [`select_next_pending_request`] is a pure
//! function over caller-supplied inputs — nothing here is wired into a
//! decoder, dispatch loop, or the request-lifecycle state machine (the next
//! checklist bullet in this milestone, still not built). The old
//! `src/prioqueue.rs` `Zone`/`Command`/`Controller`/`Priority` decorator
//! this milestone's own Goal text names as the eventual absorption target
//! for "picking which pending request runs next" is read only as
//! background for this change — see this module's doc comment
//! "Deliberately out of scope" section below for why `prioqueue.rs` itself
//! is still not touched.
//!
//! Twenty-four named pieces are in scope, all implemented here or in the six
//! prior entries this one extends:
//!
//! - [`RequestKind`] — the request-type discriminant, now covering nine
//!   values ([`RequestKind::ClearAll`] = `0x05`,
//!   [`RequestKind::ClearNonSafestate`] = `0x06`,
//!   [`RequestKind::ClearSingle`] = `0x07`, [`RequestKind::Chained`] =
//!   `0x01`, [`RequestKind::Timed`] = `0x0A`, [`RequestKind::CompoundWait`]
//!   = `0x0B`, [`RequestKind::Triggered`] = `0x0E`,
//!   [`RequestKind::Compound`] = `0x0F`, and — new in this item —
//!   [`RequestKind::Standard`]). See "Provenance note:
//!   `RequestKind`'s wire placement" below for why this is modeled as a
//!   standalone value type, not yet tied to a decoded byte offset, and
//!   "Provenance note: `RequestKind::Standard`'s discriminant" below for why
//!   its own numeric value carries even less confidence than the other
//!   eight.
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
//! - Wiring [`SequencerBank`], [`select_next_pending_request`], or any of
//!   the below into an actual decoder, dispatch loop, or the
//!   request-lifecycle state machine (`ROADMAP.md`'s own "Request lifecycle
//!   state machine" checklist bullet, the next one in this milestone, still
//!   not built). [`select_next_pending_request`] decides *which* pending
//!   request goes next given a caller-supplied set; nothing here owns a
//!   real pending-request set, tracks lifecycle transitions, or calls this
//!   function from anywhere.
//! - The old `src/prioqueue.rs` `Zone`/`Command`/`Controller`/`Priority`
//!   model this milestone's own Goal text names as the eventual absorption
//!   target for "picking which pending request runs next". This item is
//!   the "Execution priority ordering" checklist bullet the Goal text
//!   points at, and [`select_next_pending_request`] is its own
//!   from-scratch, spec-native implementation of that job — but
//!   `src/prioqueue.rs` itself is still read only as background, not
//!   extended, modified, or migrated onto [`select_next_pending_request`];
//!   that KEEP/DEPRECATE-style migration is `ROADMAP.md`'s own Milestone 9
//!   satellite-package-migration job (`prioqueue` is DEPRECATE-dispositioned
//!   in that milestone's satellite table), not this item's.
//! - Any error/rejection behavior for a pending request that never gets a
//!   turn (queue overflow, starvation) and any statement of what scope
//!   execution priority is evaluated across (per-endpoint, per-stream, or
//!   server-wide). See "Provenance note: what execution priority ordering
//!   does not decide" below.
//!
//! ## Provenance note: `RequestKind`'s wire placement
//!
//! `ROADMAP.md`'s checklist bullets name `0x0F`/`0x0B`/`0x0E`/`0x01`/`0x0A`
//! and, for this item, `0x05`/`0x06`/`0x07` as the compound, compound-wait,
//! triggered, chained, timed, clear-all, clear-non-safestate, and
//! clear-single discriminant values, but — unlike `acf_msg_type`
//! ([`crate::acf::ACF_ABB_MSG_TYPE`]/[`crate::acf::ACF_GBB_MSG_TYPE`]),
//! whose byte offset within an ACF message header this crate already
//! pinned down in Milestone 1 — no checklist text anywhere in this crate's
//! roadmap states which byte or field of a request actually carries this
//! discriminant. Per Guiding Principle 5, [`RequestKind`] is therefore
//! modeled as a standalone value type with its own `to_u8`/`from_u8` pair,
//! exactly as confident about its named numeric values as the checklist
//! text is, and no more: it is not attached to any offset within
//! [`crate::acf::ByteMessageInfo`] or any other already-built wire shape,
//! and no such offset is guessed here. This reasoning is unchanged by
//! adding [`RequestKind::Triggered`], [`RequestKind::Chained`],
//! [`RequestKind::Timed`], or the [`RequestKind::ClearAll`]/
//! [`RequestKind::ClearNonSafestate`]/[`RequestKind::ClearSingle`] trio;
//! each is simply one more value under the same still-open question. See
//! "Provenance note: `RequestKind::Standard`'s discriminant" below for why
//! [`RequestKind::Standard`] — added by this item — is a step further out
//! on that same open question, not just one more instance of it.
//!
//! ## Provenance note: `RequestKind::Standard`'s discriminant
//!
//! `ROADMAP.md`'s "Execution priority ordering" checklist bullet names
//! "standard" as the lowest-priority tier in its ordering, alongside the
//! other six tiers this module already models — but, unlike every other
//! [`RequestKind`] variant, no `ROADMAP.md` checklist text anywhere in this
//! crate's roadmap gives "standard" a numeric discriminant byte at all. The
//! other eight variants at least inherit a roadmap-named hex value each
//! (`0x01`/`0x05`/`0x06`/`0x07`/`0x0A`/`0x0B`/`0x0E`/`0x0F`) even though
//! their byte *offset* is unconfirmed (see the provenance note above);
//! [`RequestKind::Standard`] has neither. Per Guiding Principle 5, this
//! crate does not invent a plausible-looking spec value for it. The
//! discriminant assigned here, `0x00`, is a crate-local placeholder chosen
//! only for two structural reasons — it is the one byte value none of the
//! other eight named discriminants occupies, and `#[repr(u8)]` requires
//! every variant to have some concrete value for [`RequestKind::to_u8`]'s
//! `self as u8` cast to compile — not a transcription of, or a guess at,
//! any confirmed TC18 wire encoding. Should a future item learn the real
//! "standard" discriminant (or learn that "standard"/unconditional requests
//! carry no discriminant byte at all, being whatever a request is when none
//! of the other eight kinds' bytes match), this placeholder value is
//! expected to change; nothing in this crate depends on `0x00` specifically
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
// fusa:req REQ-CMP-001
// fusa:req REQ-TRIG-001
// fusa:req REQ-CHAIN-001
// fusa:req REQ-TIME-001
// fusa:req REQ-CANCEL-001
// fusa:req REQ-PRIO-001
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
}

impl RequestKind {
    /// Encode this request kind as its discriminant byte.
    // fusa:req REQ-CMP-001
    // fusa:req REQ-TRIG-001
    // fusa:req REQ-CHAIN-001
    // fusa:req REQ-TIME-001
    // fusa:req REQ-CANCEL-001
    // fusa:req REQ-PRIO-001
    pub fn to_u8(self) -> u8 {
        self as u8
    }

    /// Decode a discriminant byte into a [`RequestKind`].
    ///
    /// Returns `Err(RcpError::InvalidParameter)` for any value other than
    /// the named discriminants. Never panics for any input.
    // fusa:req REQ-CMP-002
    // fusa:req REQ-TRIG-001
    // fusa:req REQ-CHAIN-001
    // fusa:req REQ-TIME-001
    // fusa:req REQ-CANCEL-001
    // fusa:req REQ-PRIO-001
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
// fusa:req REQ-CMP-006
// fusa:req REQ-PRIO-002
pub fn resolve_compound_exec_delay(kind: RequestKind, delays: &CompoundExecDelays) -> Option<u32> {
    match kind {
        RequestKind::Compound => Some(delays.cmp_exec_delay),
        RequestKind::CompoundWait => Some(delays.cmpw_exec_delay),
        RequestKind::Triggered
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
// fusa:req REQ-SEQ-001
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
    // fusa:req REQ-SEQ-001
    pub fn new(svr_sequencers_max: u8) -> Self {
        let power_on_state = SequencerState(SequencerStateEntry::power_on_default().seq_state);
        Self {
            states: vec![power_on_state; svr_sequencers_max as usize],
        }
    }

    /// This bank's live sequencer-count bound — the `svr_sequencers_max`
    /// value it was constructed with via [`Self::new`].
    // fusa:req REQ-SEQ-001
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
    // fusa:req REQ-SEQ-002
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
    // fusa:req REQ-SEQ-003
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
    // fusa:req REQ-SEQ-004
    pub fn check_compound_gate(&self, gate: &CompoundGateConfig) -> Result<(), RcpError> {
        let current_state = self.read(gate.sequencer_num)?;
        check_compound_gate(current_state, gate, self.svr_sequencers_max())
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
/// per-kind-timer-selection shape, including [`RequestKind::Standard`] (a
/// ninth and final variant, added alongside [`ExecutionPriorityTier`]).
/// Never panics for any input.
// fusa:req REQ-TRIG-002
// fusa:req REQ-PRIO-002
pub fn resolve_trigger_exec_delay(kind: RequestKind, delay: TriggerExecDelay) -> Option<u32> {
    match kind {
        RequestKind::Triggered => Some(delay.0),
        RequestKind::Compound
        | RequestKind::CompoundWait
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
// fusa:req REQ-CHAIN-002
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
// fusa:req REQ-TIME-002
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
// fusa:req REQ-TIME-003
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
// fusa:req REQ-CANCEL-002
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
// fusa:req REQ-CANCEL-003
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
// fusa:req REQ-CANCEL-004
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
// fusa:req REQ-CANCEL-004
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
// fusa:req REQ-PRIO-003
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
/// under, collapsing [`RequestKind`]'s nine values down to the checklist's
/// seven named tiers. Never panics for any input.
// fusa:req REQ-PRIO-003
pub fn execution_priority_tier(kind: RequestKind) -> ExecutionPriorityTier {
    match kind {
        RequestKind::ClearAll | RequestKind::ClearNonSafestate | RequestKind::ClearSingle => {
            ExecutionPriorityTier::Cancellation
        }
        RequestKind::Triggered => ExecutionPriorityTier::Triggered,
        RequestKind::Timed => ExecutionPriorityTier::Timed,
        RequestKind::Compound => ExecutionPriorityTier::Compound,
        RequestKind::CompoundWait => ExecutionPriorityTier::CompoundWait,
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
// fusa:req REQ-PRIO-004
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
// fusa:req REQ-PRIO-004
pub fn select_next_pending_request(pending: &[PendingRequestKey]) -> Option<usize> {
    pending
        .iter()
        .enumerate()
        .min_by_key(|(_, key)| (execution_priority_tier(key.kind), key.arrival_seq))
        .map(|(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── RequestKind: discriminant round-trip / rejection ────────────────────

    const ALL_REQUEST_KINDS: [RequestKind; 9] = [
        RequestKind::Standard,
        RequestKind::Chained,
        RequestKind::ClearAll,
        RequestKind::ClearNonSafestate,
        RequestKind::ClearSingle,
        RequestKind::Timed,
        RequestKind::CompoundWait,
        RequestKind::Triggered,
        RequestKind::Compound,
    ];

    #[test]
    // fusa:test REQ-CMP-001
    // fusa:test REQ-TRIG-001
    // fusa:test REQ-CHAIN-001
    // fusa:test REQ-TIME-001
    // fusa:test REQ-CANCEL-001
    // fusa:test REQ-PRIO-001
    fn request_kind_round_trips_through_to_u8_from_u8() {
        for kind in ALL_REQUEST_KINDS {
            assert_eq!(RequestKind::from_u8(kind.to_u8()), Ok(kind));
        }
    }

    #[test]
    // fusa:test REQ-CMP-001
    // fusa:test REQ-TRIG-001
    // fusa:test REQ-CHAIN-001
    // fusa:test REQ-TIME-001
    // fusa:test REQ-CANCEL-001
    // fusa:test REQ-PRIO-001
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
    }

    #[test]
    // fusa:test REQ-CMP-002
    fn request_kind_from_u8_rejects_every_other_value() {
        for raw in [0x02u8, 0x03, 0x04, 0x08, 0x0C, 0x10, 0x7F, 0xFF] {
            assert_eq!(RequestKind::from_u8(raw), Err(RcpError::InvalidParameter));
        }
    }

    #[test]
    // fusa:test REQ-TRIG-001
    fn request_kind_from_u8_accepts_triggered_discriminant() {
        assert_eq!(RequestKind::from_u8(0x0E), Ok(RequestKind::Triggered));
    }

    #[test]
    // fusa:test REQ-PRIO-001
    fn request_kind_from_u8_accepts_standard_discriminant() {
        assert_eq!(RequestKind::from_u8(0x00), Ok(RequestKind::Standard));
    }

    #[test]
    // fusa:test REQ-CHAIN-001
    fn request_kind_from_u8_accepts_chained_discriminant() {
        assert_eq!(RequestKind::from_u8(0x01), Ok(RequestKind::Chained));
    }

    #[test]
    // fusa:test REQ-TIME-001
    fn request_kind_from_u8_accepts_timed_discriminant() {
        assert_eq!(RequestKind::from_u8(0x0A), Ok(RequestKind::Timed));
    }

    #[test]
    // fusa:test REQ-CANCEL-001
    fn request_kind_from_u8_accepts_all_three_cancellation_discriminants() {
        assert_eq!(RequestKind::from_u8(0x05), Ok(RequestKind::ClearAll));
        assert_eq!(
            RequestKind::from_u8(0x06),
            Ok(RequestKind::ClearNonSafestate)
        );
        assert_eq!(RequestKind::from_u8(0x07), Ok(RequestKind::ClearSingle));
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

    #[test]
    // fusa:test REQ-CMP-006
    // fusa:test REQ-CHAIN-001
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
    // fusa:test REQ-CMP-006
    // fusa:test REQ-TIME-001
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
    // fusa:test REQ-CMP-006
    // fusa:test REQ-CANCEL-001
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
    // fusa:test REQ-CMP-006
    // fusa:test REQ-PRIO-002
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

    // ── SequencerBank ─────────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-SEQ-001
    fn sequencer_bank_new_sizes_the_bank_to_svr_sequencers_max() {
        for max in [0u8, 1, 4, 255] {
            let bank = SequencerBank::new(max);
            assert_eq!(bank.svr_sequencers_max(), max);
        }
    }

    #[test]
    // fusa:test REQ-SEQ-001
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
    // fusa:test REQ-SEQ-001
    fn sequencer_bank_new_with_zero_max_yields_an_empty_bank() {
        let bank = SequencerBank::new(0);
        assert_eq!(bank.svr_sequencers_max(), 0);
        assert_eq!(bank.read(0), Err(RcpError::SequencerNotKnown));
    }

    // ── SequencerBank::read ──────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-SEQ-002
    fn sequencer_bank_read_rejects_sequencer_num_at_or_above_the_bound() {
        let bank = SequencerBank::new(4);
        for sequencer_num in [4u8, 5, 255] {
            assert_eq!(bank.read(sequencer_num), Err(RcpError::SequencerNotKnown));
        }
    }

    #[test]
    // fusa:test REQ-SEQ-002
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
    // fusa:test REQ-SEQ-003
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
    // fusa:test REQ-SEQ-003
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
    // fusa:test REQ-SEQ-003
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
    // fusa:test REQ-SEQ-003
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
    // fusa:test REQ-SEQ-004
    fn sequencer_bank_check_compound_gate_ok_when_default_state_matches_gate() {
        let bank = SequencerBank::new(4);
        let gate = CompoundGateConfig {
            sequencer_num: 1,
            start_state: SequencerState(1),
        };
        assert_eq!(bank.check_compound_gate(&gate), Ok(()));
    }

    #[test]
    // fusa:test REQ-SEQ-004
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
    // fusa:test REQ-SEQ-004
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
    // fusa:test REQ-SEQ-004
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
    // fusa:test REQ-SEQ-004
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
        assert_eq!(
            resolve_trigger_exec_delay(RequestKind::Chained, delay),
            None
        );
        assert_eq!(resolve_trigger_exec_delay(RequestKind::Timed, delay), None);
    }

    #[test]
    // fusa:test REQ-TRIG-002
    // fusa:test REQ-CANCEL-001
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
    // fusa:test REQ-TRIG-002
    // fusa:test REQ-PRIO-002
    fn resolve_trigger_exec_delay_is_none_for_standard() {
        let delay = TriggerExecDelay(42);
        assert_eq!(
            resolve_trigger_exec_delay(RequestKind::Standard, delay),
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

    // ── check_chain_continuation ──────────────────────────────────────────────

    #[test]
    // fusa:test REQ-CHAIN-002
    fn check_chain_continuation_aborts_only_when_cs_set_and_predecessor_errored() {
        assert_eq!(
            check_chain_continuation(true, true),
            Err(RcpError::ChainAborted)
        );
    }

    #[test]
    // fusa:test REQ-CHAIN-002
    fn check_chain_continuation_continues_when_cs_not_set_even_if_predecessor_errored() {
        assert_eq!(check_chain_continuation(false, true), Ok(()));
    }

    #[test]
    // fusa:test REQ-CHAIN-002
    fn check_chain_continuation_continues_when_predecessor_did_not_error_regardless_of_cs() {
        assert_eq!(check_chain_continuation(true, false), Ok(()));
        assert_eq!(check_chain_continuation(false, false), Ok(()));
    }

    #[test]
    // fusa:test REQ-CHAIN-002
    fn check_chain_continuation_never_panics_for_any_sampled_input() {
        for cs in [true, false] {
            for predecessor_errored in [true, false] {
                let _ = check_chain_continuation(cs, predecessor_errored);
            }
        }
    }

    // ── RcpError::ChainAborted / RcpError::ChainError ────────────────────────

    #[test]
    // fusa:test REQ-CHAIN-003
    fn chain_aborted_and_chain_error_are_distinct_rcp_error_variants() {
        assert_ne!(RcpError::ChainAborted, RcpError::ChainError);
        assert_eq!(RcpError::ChainAborted, RcpError::ChainAborted);
        assert_eq!(RcpError::ChainError, RcpError::ChainError);
    }

    #[test]
    // fusa:test REQ-CHAIN-003
    fn chain_aborted_and_chain_error_carry_the_roadmap_named_codes_in_their_display_text() {
        assert!(RcpError::ChainAborted.to_string().contains("CHAIN_ABORTED"));
        assert!(RcpError::ChainError.to_string().contains("CHAIN_ERROR"));
    }

    // ── TimedExecutionTime / is_timed_request_ready ──────────────────────────

    #[test]
    // fusa:test REQ-TIME-002
    fn timed_execution_time_default_is_untimed() {
        let exec_time = TimedExecutionTime::default();
        assert_eq!(exec_time.0, AvtpTimestamp::default());
        assert!(exec_time.0.is_untimed());
    }

    #[test]
    // fusa:test REQ-TIME-002
    fn timed_execution_time_wraps_avtp_timestamp_by_value() {
        let exec_time = TimedExecutionTime(AvtpTimestamp::new(1_000));
        assert_eq!(exec_time.0.to_u32(), 1_000);
    }

    #[test]
    // fusa:test REQ-TIME-003
    fn is_timed_request_ready_false_before_exec_time_is_reached() {
        let exec_time = TimedExecutionTime(AvtpTimestamp::new(1_000));
        assert!(!is_timed_request_ready(AvtpTimestamp::new(999), exec_time));
    }

    #[test]
    // fusa:test REQ-TIME-003
    fn is_timed_request_ready_true_exactly_at_exec_time() {
        let exec_time = TimedExecutionTime(AvtpTimestamp::new(1_000));
        assert!(is_timed_request_ready(AvtpTimestamp::new(1_000), exec_time));
    }

    #[test]
    // fusa:test REQ-TIME-003
    fn is_timed_request_ready_true_after_exec_time_has_passed() {
        let exec_time = TimedExecutionTime(AvtpTimestamp::new(1_000));
        assert!(is_timed_request_ready(AvtpTimestamp::new(1_001), exec_time));
    }

    #[test]
    // fusa:test REQ-TIME-003
    fn is_timed_request_ready_true_across_a_rollover() {
        // AvtpTimestamp::is_after is wraparound-aware; a current time that
        // just wrapped past u32::MAX back to a small value must still read
        // as ready for an exec_time set just before the rollover.
        let exec_time = TimedExecutionTime(AvtpTimestamp::new(u32::MAX - 1));
        assert!(is_timed_request_ready(AvtpTimestamp::new(2), exec_time));
    }

    #[test]
    // fusa:test REQ-TIME-003
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
    // fusa:test REQ-TIME-003
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
    // fusa:test REQ-CANCEL-002
    fn check_clear_all_cancellation_always_cancels() {
        assert_eq!(
            check_clear_all_cancellation(),
            Err(RcpError::RequestCanceled)
        );
    }

    // ── check_clear_non_safestate_cancellation ────────────────────────────────

    #[test]
    // fusa:test REQ-CANCEL-003
    fn check_clear_non_safestate_cancellation_spares_safestate_related_requests() {
        assert_eq!(check_clear_non_safestate_cancellation(true), Ok(()));
    }

    #[test]
    // fusa:test REQ-CANCEL-003
    fn check_clear_non_safestate_cancellation_cancels_non_safestate_related_requests() {
        assert_eq!(
            check_clear_non_safestate_cancellation(false),
            Err(RcpError::RequestCanceled)
        );
    }

    #[test]
    // fusa:test REQ-CANCEL-003
    fn check_clear_non_safestate_cancellation_never_panics_for_any_input() {
        for is_safestate_related in [true, false] {
            let _ = check_clear_non_safestate_cancellation(is_safestate_related);
        }
    }

    // ── ClearTransactionNum / check_clear_single_cancellation ────────────────

    #[test]
    // fusa:test REQ-CANCEL-004
    fn clear_transaction_num_default_is_zero() {
        assert_eq!(ClearTransactionNum::default(), ClearTransactionNum(0));
    }

    #[test]
    // fusa:test REQ-CANCEL-004
    fn check_clear_single_cancellation_cancels_only_the_matching_transaction_num() {
        let target = ClearTransactionNum(0x42);
        assert_eq!(
            check_clear_single_cancellation(0x42, target),
            Err(RcpError::RequestCanceled)
        );
    }

    #[test]
    // fusa:test REQ-CANCEL-004
    fn check_clear_single_cancellation_spares_every_non_matching_transaction_num() {
        let target = ClearTransactionNum(0x42);
        for candidate in [0x00u8, 0x01, 0x41, 0x43, 0xFF] {
            assert_eq!(check_clear_single_cancellation(candidate, target), Ok(()));
        }
    }

    #[test]
    // fusa:test REQ-CANCEL-004
    fn check_clear_single_cancellation_never_panics_for_any_sampled_input() {
        for candidate in [0x00u8, 0x42, 0xFF] {
            for target in [0x00u8, 0x42, 0xFF] {
                let _ = check_clear_single_cancellation(candidate, ClearTransactionNum(target));
            }
        }
    }

    // ── ExecutionPriorityTier / execution_priority_tier ──────────────────────

    #[test]
    // fusa:test REQ-PRIO-003
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
    // fusa:test REQ-PRIO-003
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
    // fusa:test REQ-PRIO-003
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
    // fusa:test REQ-PRIO-003
    fn execution_priority_tier_never_panics_for_any_request_kind() {
        for kind in ALL_REQUEST_KINDS {
            let _ = execution_priority_tier(kind);
        }
    }

    // ── PendingRequestKey / select_next_pending_request ──────────────────────

    #[test]
    // fusa:test REQ-PRIO-004
    fn select_next_pending_request_is_none_for_an_empty_slice() {
        assert_eq!(select_next_pending_request(&[]), None);
    }

    #[test]
    // fusa:test REQ-PRIO-004
    fn select_next_pending_request_picks_the_single_entry() {
        let pending = [PendingRequestKey {
            kind: RequestKind::Standard,
            arrival_seq: 0,
        }];
        assert_eq!(select_next_pending_request(&pending), Some(0));
    }

    #[test]
    // fusa:test REQ-PRIO-004
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
    // fusa:test REQ-PRIO-004
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
    // fusa:test REQ-PRIO-004
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
    // fusa:test REQ-PRIO-004
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
    // fusa:test REQ-PRIO-004
    fn select_next_pending_request_never_panics_for_any_sampled_input() {
        for kind in ALL_REQUEST_KINDS {
            for arrival_seq in [0u64, 1, u64::MAX] {
                let pending = [PendingRequestKey { kind, arrival_seq }];
                let _ = select_next_pending_request(&pending);
            }
        }
    }
}
