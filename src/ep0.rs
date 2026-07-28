// fusa:req REQ-EP0-001
// fusa:req REQ-EP0-002
// fusa:req REQ-EP0-003
// fusa:req REQ-EP0-004
// fusa:req REQ-EP0-005
// fusa:req REQ-EP0-006
// fusa:req REQ-EP0-007
// fusa:req REQ-EP0-008
// fusa:req REQ-EP0-009
// fusa:req REQ-EP0-010
// fusa:req REQ-EP0-011

//! EP0 (RC-Server-as-endpoint) whole-register-map read/write addressing,
//! plus the root-client access-control axis layered on top of it —
//! TC18 register-map model (`ROADMAP.md` Milestone 2, "EP0
//! (RC-Server-as-Endpoint)" subsection, now in full).
//!
//! This module covers both items of Milestone 2's "EP0" subsection, the
//! next subsection in document order after "Lifecycle State Machine" (see
//! [`crate::lifecycle`], which this module composes with rather than
//! duplicates). Per that module's own doc comment, EP0 was explicitly
//! named as out of scope there and left for this item.
//!
//! - The first bullet, whole-register-map read/write addressing, is
//!   unchanged from its original shape: [`EP0_BYTE_BUS_ID`]/
//!   [`is_ep0_address`], [`RequestRoute`]/[`route_byte_bus_id`],
//!   [`Ep0AccessKind`]/[`access_kind`], and [`check_ep0_access`].
//! - The second bullet, the **root-client** concept
//!   (`svr_root_client_index`), is new: [`is_root_client`] and
//!   [`check_ep0_access_for_stream`] add a distinct, orthogonal
//!   access-control axis — *which stream is asking* — layered on top of
//!   (not replacing) [`check_ep0_access`]'s lifecycle-state gating. See the
//!   "Root client" section below and this module's Provenance note for how
//!   that axis was scoped.
//!
//! `byte_bus_id 0` is reserved: it addresses the RC Server acting as a
//! pseudo-endpoint over its own whole register map, not any device-facing
//! endpoint an application has registered. This module formalizes that
//! reservation two ways:
//!
//! - [`EP0_BYTE_BUS_ID`] / [`is_ep0_address`] name the reserved value
//!   explicitly, rather than leaving `0` an unremarked magic number
//!   wherever it appears.
//! - [`route_byte_bus_id`] makes the *routing* consequence structural: a
//!   `byte_bus_id` of `0` decides [`RequestRoute::Ep0`] from the
//!   `byte_bus_id` value alone, before [`crate::addressing::EndpointTable`]
//!   is ever consulted. A request/response addressed to EP0 must never be
//!   resolved through `EndpointTable`'s per-stream device-endpoint
//!   keyspace — the RC Server itself is not an entry in that table.
//!
//! [`check_ep0_access`] is the whole-register-map read/write path itself,
//! but only at the granularity this crate can currently support:
//! [`crate::lifecycle::RegisterCategory`], not concrete register fields
//! (the sibling "Register Map" subsection's still-unbuilt job — see that
//! subsection's own checklist bullets). A read consults
//! [`crate::lifecycle::check_register_reachable`]; a write additionally
//! consults [`crate::lifecycle::check_register_writable`] — the same two
//! fully-implemented gates [`crate::lifecycle`] already built for exactly
//! this purpose. [`access_kind`] derives the read/write direction from
//! [`crate::acf::ByteMessageInfo::op`]; see "Provenance note" below for why
//! that derivation, not either gate itself, is this item's own working
//! interpretation.
//!
//! This module performs no register I/O of any kind — there is no concrete
//! register content anywhere in this crate yet to read or write.
//! [`check_ep0_access`] only ever answers "is this category-granularity
//! access permitted", never "here is the value". Wiring a real read/write
//! data path through EP0 is necessarily downstream of the "Register Map"
//! subsection, not this item's job.
//!
//! The echo-back rule ([`crate::acf::build_response_info`] /
//! [`crate::acf::verify_echo_back`]) needs no EP0-specific counterpart:
//! both functions already operate purely on `byte_bus_id`, and `0` passes
//! through them like any other value — see this module's tests for a
//! demonstration that an EP0-addressed request/response pair round-trips
//! through the existing echo-back rule unchanged.
//!
//! ## Root client
//!
//! [`is_root_client`]/[`check_ep0_access_for_stream`] add the second "EP0"
//! checklist bullet: a distinct, orthogonal access-control axis over *which
//! stream is asking*, layered on top of (not replacing)
//! [`check_ep0_access`]'s lifecycle-state gating.
//!
//! - The RC Server designates at most one [`crate::avtp::StreamId`] as its
//!   root client at any time, represented here as a plain `Option<StreamId>`
//!   — see the Provenance note below for why no dedicated wrapper type was
//!   introduced for it.
//! - [`check_ep0_access_for_stream`] leaves EP0 *reads* exactly as
//!   [`check_ep0_access`] already decided them, for root and non-root
//!   streams alike — root-client status only ever gates *writes*, matching
//!   this checklist bullet's own wording ("full-server **write** access for
//!   exactly one stream").
//! - For an EP0 *write*, [`check_ep0_access_for_stream`] first checks
//!   [`is_root_client`] for the requesting stream: if it is the designated
//!   root client, the write proceeds exactly as [`check_ep0_access`] would
//!   decide it (i.e. still subject to lifecycle-state reachability/locking);
//!   if it is not, the write is rejected with
//!   [`crate::RcpError::UnauthorizedAccess`] without even consulting
//!   [`check_ep0_access`] — a non-root stream's EP0 write is refused
//!   regardless of what lifecycle state would otherwise have permitted.
//! - No root client at all (`root_client == None`) behaves like every
//!   stream being non-root: every EP0 write is rejected with
//!   `UnauthorizedAccess` until some stream is designated.
//!
//! This module still performs no register I/O and does not decide what a
//! non-root stream's *per-endpoint* (device-endpoint, non-EP0) write access
//! looks like — routing a non-EP0-addressed request to a device endpoint at
//! all is [`route_byte_bus_id`]'s job, unaffected by this section, and
//! validating what a device endpoint permits once routed there is
//! necessarily downstream of Milestone 4's endpoint work, not this item's.
//!
//! Deliberately out of scope for this item (separate, later checklist
//! bullets):
//!
//! - The concrete Register Map subsection's field layout
//!   (`svr_oa_tc18_magic_nr`, HW pin-mapping tables, register addresses,
//!   etc.) — [`crate::lifecycle::RegisterCategory`] remains the same
//!   abstract placeholder it already was for the "Lifecycle State Machine"
//!   subsection; this item does not narrow or extend it.
//! - Milestone 3's Discovery item, which also names `byte_bus_id 0` (a
//!   broadcastable ACF_ABB read at "register address 0", answerable in
//!   *any* lifecycle state). That is a distinct, later concern: discovery's
//!   own answerable-in-any-state rule is stronger than, and not derived
//!   from, [`check_ep0_access`]'s general category-reachability composition
//!   — reconciling the two (if they need reconciling at all) is left to
//!   whichever item builds discovery, not guessed at here.
//! - Wiring [`route_byte_bus_id`]/[`check_ep0_access`]/
//!   [`check_ep0_access_for_stream`] into an actual decoder, dispatch loop,
//!   or [`crate::addressing::EndpointTable`] caller — this module remains
//!   additive standalone plumbing only, matching the discipline every prior
//!   Milestone 1/2 entry already established. Nothing here designates a
//!   root client against a real RC Server instance either — this module
//!   defines only the *check*, taking `root_client` as a caller-supplied
//!   value, mirroring how [`crate::lifecycle::RcServerState::try_transition`]
//!   takes its consistency guard as a caller-supplied closure rather than
//!   owning any server state itself.
//!   [`crate::addressing::EndpointTable::register`] is deliberately left
//!   unchanged: it still structurally permits registering a device endpoint
//!   at `byte_bus_id 0` (nothing in `crate::addressing` special-cases it),
//!   since guarding that registration path is a change to an existing
//!   caller/module this item's scope does not include — see this module's
//!   own tests for why that structural possibility does not, on its own,
//!   compromise [`route_byte_bus_id`]'s routing decision.
//!
//! ## Provenance note
//!
//! `byte_bus_id 0` being the reserved EP0 address is taken directly from
//! `ROADMAP.md`'s own Milestone 2 "EP0 (RC-Server-as-Endpoint)" heading and
//! Milestone 3's Discovery bullet (which names the same reserved value by
//! number), which in turn cite the OPEN Alliance TC18 Remote Control
//! Protocol Specification v0.5.1_RC by name only for this subsection. This
//! module's own doc comments therefore do not cite a `§3.x` section number
//! for EP0 itself, matching [`crate::lifecycle`]'s own precedent for the
//! "Lifecycle State Machine" subsection.
//!
//! [`access_kind`]'s `op = false` → read / `op = true` → write convention
//! is this crate's own working interpretation, not a transcription of
//! specified behavior: [`crate::acf`]'s own provenance note documents `op`
//! only as "Operation flag" with no direction assigned either way. This
//! item assigns the more conventional reading — `false` as the "no
//! operation beyond the default" (read) case, `true` as the state-changing
//! (write) case — for the same reason [`crate::lifecycle`] picked
//! `HW_UNCONFIGURED` as `RcServerState::INITIAL`: it is this crate's own
//! reasonable default, not a guessed fact, and is flagged here per Guiding
//! Principle 5 for reconciliation against the specification's actual
//! behavior (never its prose) before being relied on for interop with a
//! real TC18 RC Server.
//!
//! [`check_ep0_access`]'s composition rule itself — read against
//! [`crate::lifecycle::check_register_reachable`] alone, write additionally
//! against [`crate::lifecycle::check_register_writable`] — is not a new
//! working interpretation this item introduces; it restates the layering
//! [`crate::lifecycle`]'s own doc comment already established (reachability
//! as the coarser gate, writability as the finer one layered on top), only
//! now selected by [`access_kind`] instead of being the caller's job to
//! choose between explicitly.
//!
//! The root-client section goes a step further than `ROADMAP.md`'s own
//! wording and is this crate's own working interpretation, flagged per
//! Guiding Principle 5, pending reconciliation against the specification's
//! actual behavior (never its prose) before being relied on for interop
//! with a real TC18 RC Server:
//!
//! - `svr_root_client_index` is the roadmap checklist's name for the
//!   backing register, but the concrete Register Map subsection that would
//!   define such a field is still unbuilt (see the out-of-scope list
//!   above). Rather than invent a placeholder register type the way
//!   [`crate::lifecycle::RegisterCategory`] stands in for the whole
//!   register map, this item represents "which stream (if any) currently
//!   holds the index" directly as `Option<`[`crate::avtp::StreamId`]`>` —
//!   the natural in-memory shape of "at most one designated stream" — and
//!   leaves the eventual `svr_root_client_index` register's own encoding
//!   (whatever wire representation of a stream identity it turns out to be)
//!   to whichever later item builds the concrete Register Map.
//! - That root-client status gates EP0 *writes* only, leaving EP0 *reads*
//!   identical to [`check_ep0_access`] for every stream regardless of
//!   root-client status, is inferred from the checklist bullet's own
//!   wording naming "full-server **write** access" for the root client and
//!   only "per-endpoint-restricted access" — not "read-restricted access"
//!   — for everyone else. Nothing in the checklist bullet's text suggests
//!   non-root streams lose EP0 *read* visibility, only that they cannot
//!   exercise the root client's whole-server *write* privilege.
//! - `RcpError::UnauthorizedAccess` is Milestone 2's "Error Model" item's
//!   TC18 spec error code for a non-root-client stream's rejected EP0
//!   write — this function originally returned a crate-invented
//!   `RootClientRequired` sentinel, since remapped onto the same spec code
//!   [`crate::lifecycle::check_register_reachable`] now uses for its own
//!   lifecycle-state-gated rejection; see [`crate::RcpError`]'s own doc
//!   comment for the full provenance/mapping note, including why the two
//!   collapse onto one code rather than staying distinct.
//! - An absent root client (`root_client == None`) rejecting every EP0
//!   write with `UnauthorizedAccess` — rather than, say, treating "no root
//!   client designated" as "anyone may write" — is this crate's own
//!   conservative default: the checklist bullet describes root-client
//!   write access as a privilege belonging to "exactly one stream", which
//!   this crate reads as implying no stream holds that privilege before one
//!   has been designated, not that the privilege is open to all until then.

use crate::acf::ByteMessageInfo;
use crate::avtp::StreamId;
use crate::lifecycle::{
    check_register_reachable, check_register_writable, RcServerState, RegisterCategory,
};
use crate::RcpError;

// ── Reserved EP0 address ─────────────────────────────────────────────────────

/// The reserved `byte_bus_id` value addressing the RC Server itself as EP0
/// (the whole-register-map pseudo-endpoint), per this module's doc comment.
///
/// Distinct from [`crate::addressing::EndpointTable`]'s per-stream
/// device-endpoint keyspace — a request/response addressed to this value is
/// never looked up there (see [`route_byte_bus_id`]).
pub const EP0_BYTE_BUS_ID: u16 = 0;

/// Is `byte_bus_id` the reserved EP0 address?
///
/// Never panics for any input.
// fusa:req REQ-EP0-001
pub fn is_ep0_address(byte_bus_id: u16) -> bool {
    byte_bus_id == EP0_BYTE_BUS_ID
}

// ── Routing: EP0 vs. device endpoint ─────────────────────────────────────────

/// Where a `(stream_id, byte_bus_id)`-addressed request should be routed.
///
/// See [`route_byte_bus_id`] for how this decision is made, and this
/// module's doc comment for why it must be made *before*
/// [`crate::addressing::EndpointTable`] is ever consulted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// fusa:req REQ-EP0-002
pub enum RequestRoute {
    /// `byte_bus_id` is the reserved EP0 address. Route to this module's
    /// whole-register-map read/write path ([`check_ep0_access`]), not to
    /// `EndpointTable`.
    Ep0,
    /// `byte_bus_id` is not the reserved EP0 address. Route to
    /// `EndpointTable::lookup` instead; resolving that lookup (including
    /// the case where nothing is registered under the pair) is the
    /// caller's concern, not this module's.
    DeviceEndpoint,
}

/// Decide whether `byte_bus_id` routes to EP0 or to a device endpoint.
///
/// This is a pure decision over `byte_bus_id` alone — it takes no
/// [`crate::addressing::EndpointTable`] parameter and performs no lookup,
/// which is itself the point: the EP0/device-endpoint routing decision must
/// not depend on, or be overridable by, whatever a table happens to have
/// registered. Never panics for any input.
// fusa:req REQ-EP0-002
pub fn route_byte_bus_id(byte_bus_id: u16) -> RequestRoute {
    if is_ep0_address(byte_bus_id) {
        RequestRoute::Ep0
    } else {
        RequestRoute::DeviceEndpoint
    }
}

// ── Access direction ──────────────────────────────────────────────────────────

/// The direction of an EP0-addressed access, derived from
/// [`crate::acf::ByteMessageInfo::op`].
///
/// See this module's provenance note for why `op`'s direction convention is
/// this crate's own working interpretation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// fusa:req REQ-EP0-003
pub enum Ep0AccessKind {
    /// `op = false`.
    Read,
    /// `op = true`.
    Write,
}

/// Derive an [`Ep0AccessKind`] from `info.op`.
///
/// Never panics: `info` is an already-decoded value, not raw bytes, so
/// there is no truncated-input shape to reject.
// fusa:req REQ-EP0-003
pub fn access_kind(info: &ByteMessageInfo) -> Ep0AccessKind {
    if info.op {
        Ep0AccessKind::Write
    } else {
        Ep0AccessKind::Read
    }
}

// ── Whole-register-map access check ──────────────────────────────────────────

/// Is an EP0-addressed access to `category`, in `state`, permitted by
/// `info`'s direction?
///
/// Composes with [`crate::lifecycle`]'s already-implemented gates rather
/// than duplicating them: a read ([`Ep0AccessKind::Read`]) is permitted iff
/// [`check_register_reachable`] succeeds; a write ([`Ep0AccessKind::Write`])
/// is additionally checked against [`check_register_writable`]'s write-lock
/// rule, which itself re-checks reachability before considering the lock —
/// so a write to an unreachable category still reports
/// `Err(RcpError::UnauthorizedAccess)`, not `LockedMemAccess`, exactly as
/// [`check_register_writable`]'s own doc comment specifies.
///
/// This function decides only whether the access may proceed at
/// [`RegisterCategory`] granularity — it performs no register I/O and does
/// not itself verify that `info`'s `byte_bus_id` is actually
/// [`EP0_BYTE_BUS_ID`] (that is [`route_byte_bus_id`]'s job, expected to
/// have already run before this function is called). Never panics for any
/// input.
// fusa:req REQ-EP0-004
// fusa:req REQ-EP0-005
// fusa:req REQ-EP0-006
pub fn check_ep0_access(
    state: RcServerState,
    category: RegisterCategory,
    info: &ByteMessageInfo,
) -> Result<(), RcpError> {
    match access_kind(info) {
        Ep0AccessKind::Read => check_register_reachable(state, category),
        Ep0AccessKind::Write => check_register_writable(state, category),
    }
}

// ── Root client ───────────────────────────────────────────────────────────────

/// Is `stream` the RC Server's currently designated root client?
///
/// `root_client` is this crate's own in-memory working representation of
/// the roadmap's `svr_root_client_index` register field — see this
/// module's Provenance note. `None` means no stream currently holds
/// root-client status, in which case this always answers `false`. Never
/// panics for any input.
// fusa:req REQ-EP0-007
pub fn is_root_client(root_client: Option<StreamId>, stream: StreamId) -> bool {
    root_client == Some(stream)
}

/// Root-client-aware counterpart to [`check_ep0_access`].
///
/// Reads ([`Ep0AccessKind::Read`]) are decided identically to
/// [`check_ep0_access`], for `requesting_stream` regardless of whether it
/// is the root client — see this module's "Root client" section for why
/// root-client status gates writes only.
///
/// Writes ([`Ep0AccessKind::Write`]) additionally require
/// `requesting_stream` to be the designated `root_client`
/// ([`is_root_client`]): if it is, the write is decided exactly as
/// [`check_ep0_access`] would decide it (still subject to lifecycle-state
/// reachability/locking); if it is not — including when `root_client` is
/// `None` — the write is rejected with `Err(RcpError::UnauthorizedAccess)`
/// without consulting [`check_ep0_access`] at all, so a non-root stream's
/// EP0 write is refused regardless of what lifecycle state would otherwise
/// have permitted.
///
/// Like [`check_ep0_access`], this function performs no register I/O and
/// does not itself verify that `info`'s `byte_bus_id` is actually
/// [`EP0_BYTE_BUS_ID`]. Never panics for any input.
// fusa:req REQ-EP0-008
// fusa:req REQ-EP0-009
// fusa:req REQ-EP0-010
// fusa:req REQ-EP0-011
pub fn check_ep0_access_for_stream(
    state: RcServerState,
    category: RegisterCategory,
    info: &ByteMessageInfo,
    requesting_stream: StreamId,
    root_client: Option<StreamId>,
) -> Result<(), RcpError> {
    match access_kind(info) {
        Ep0AccessKind::Read => check_ep0_access(state, category, info),
        Ep0AccessKind::Write => {
            if is_root_client(root_client, requesting_stream) {
                check_ep0_access(state, category, info)
            } else {
                Err(RcpError::UnauthorizedAccess)
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::acf::ByteMessageInfo;

    const ALL_STATES: [RcServerState; 3] = [
        RcServerState::HwUnconfigured,
        RcServerState::HwConfigured,
        RcServerState::RcpConfigured,
    ];

    const ALL_CATEGORIES: [RegisterCategory; 3] = [
        RegisterCategory::General,
        RegisterCategory::HwConfig,
        RegisterCategory::RcpConfig,
    ];

    fn info_with_op(op: bool) -> ByteMessageInfo {
        ByteMessageInfo {
            byte_bus_id: EP0_BYTE_BUS_ID,
            op,
            ..ByteMessageInfo::default()
        }
    }

    // ── Reserved address ──────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-EP0-001
    fn is_ep0_address_true_only_for_zero() {
        assert!(is_ep0_address(0));
        for byte_bus_id in [1u16, 2, 7, 0x0123, crate::acf::BYTE_MESSAGE_INFO_11BIT_MAX] {
            assert!(!is_ep0_address(byte_bus_id));
        }
    }

    #[test]
    // fusa:test REQ-EP0-001
    fn is_ep0_address_never_panics_across_the_full_range() {
        for byte_bus_id in 0u16..=crate::acf::BYTE_MESSAGE_INFO_11BIT_MAX {
            let _ = is_ep0_address(byte_bus_id);
        }
    }

    // ── Routing ───────────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-EP0-002
    fn route_byte_bus_id_is_ep0_only_for_zero() {
        assert_eq!(route_byte_bus_id(0), RequestRoute::Ep0);
        for byte_bus_id in [1u16, 2, 7, 0x0123, crate::acf::BYTE_MESSAGE_INFO_11BIT_MAX] {
            assert_eq!(route_byte_bus_id(byte_bus_id), RequestRoute::DeviceEndpoint);
        }
    }

    #[test]
    // fusa:test REQ-EP0-002
    fn ep0_route_is_decided_before_and_independent_of_endpoint_table_contents() {
        use crate::addressing::{EndpointId, EndpointTable};
        use crate::avtp::StreamId;

        let mut table = EndpointTable::new();
        let sid = StreamId::new([0x02, 0x11, 0x22, 0x33, 0x44, 0x55], 1);

        // route_byte_bus_id's decision for byte_bus_id 0 is structural,
        // decided from byte_bus_id alone, before any table is consulted --
        // true whether or not the table happens to have something
        // registered at that address (EndpointTable::register itself is
        // deliberately left unchanged by this module; see the module doc
        // comment).
        assert_eq!(route_byte_bus_id(EP0_BYTE_BUS_ID), RequestRoute::Ep0);

        table
            .register(sid, EP0_BYTE_BUS_ID, EndpointId(99))
            .unwrap();
        assert!(table.lookup(sid, EP0_BYTE_BUS_ID).is_some());
        // The routing decision is unchanged: still Ep0, never consulting
        // (and certainly never returning) whatever the table resolved.
        assert_eq!(route_byte_bus_id(EP0_BYTE_BUS_ID), RequestRoute::Ep0);
    }

    #[test]
    // fusa:test REQ-EP0-002
    fn route_byte_bus_id_never_panics_across_the_full_range() {
        for byte_bus_id in 0u16..=crate::acf::BYTE_MESSAGE_INFO_11BIT_MAX {
            let _ = route_byte_bus_id(byte_bus_id);
        }
    }

    // ── Access direction ─────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-EP0-003
    fn access_kind_matches_op_flag() {
        assert_eq!(access_kind(&info_with_op(false)), Ep0AccessKind::Read);
        assert_eq!(access_kind(&info_with_op(true)), Ep0AccessKind::Write);
    }

    // ── Whole-register-map access check ─────────────────────────────────

    #[test]
    // fusa:test REQ-EP0-004
    fn ep0_read_agrees_with_check_register_reachable() {
        for state in ALL_STATES {
            for category in ALL_CATEGORIES {
                let info = ByteMessageInfo {
                    byte_bus_id: EP0_BYTE_BUS_ID,
                    op: false,
                    ..ByteMessageInfo::default()
                };
                assert_eq!(
                    check_ep0_access(state, category, &info),
                    check_register_reachable(state, category),
                    "{state:?} {category:?}"
                );
            }
        }
    }

    #[test]
    // fusa:test REQ-EP0-005
    fn ep0_write_agrees_with_check_register_writable() {
        for state in ALL_STATES {
            for category in ALL_CATEGORIES {
                let info = ByteMessageInfo {
                    byte_bus_id: EP0_BYTE_BUS_ID,
                    op: true,
                    ..ByteMessageInfo::default()
                };
                assert_eq!(
                    check_ep0_access(state, category, &info),
                    check_register_writable(state, category),
                    "{state:?} {category:?}"
                );
            }
        }
    }

    #[test]
    // fusa:test REQ-EP0-005
    fn ep0_write_to_unreachable_category_reports_unreachable_not_locked() {
        // RcpConfig is unreachable (not merely locked) while
        // HwUnconfigured -- check_ep0_access must surface that specific
        // reason, matching check_register_writable's own documented
        // distinction.
        let info = info_with_op(true);
        assert_eq!(
            check_ep0_access(
                RcServerState::HwUnconfigured,
                RegisterCategory::RcpConfig,
                &info
            ),
            Err(RcpError::UnauthorizedAccess)
        );
    }

    #[test]
    // fusa:test REQ-EP0-005
    fn ep0_write_to_permanently_locked_category_reports_locked() {
        // HwConfig is reachable but W*-locked once RcpConfigured.
        let info = info_with_op(true);
        assert_eq!(
            check_ep0_access(
                RcServerState::RcpConfigured,
                RegisterCategory::HwConfig,
                &info
            ),
            Err(RcpError::LockedMemAccess)
        );
    }

    #[test]
    // fusa:test REQ-EP0-006
    fn check_ep0_access_never_panics_for_any_state_category_op_combination() {
        for state in ALL_STATES {
            for category in ALL_CATEGORIES {
                for op in [false, true] {
                    let info = info_with_op(op);
                    let _ = check_ep0_access(state, category, &info);
                }
            }
        }
    }

    // ── Interop with the existing echo-back rule ─────────────────────────

    #[test]
    // fusa:test REQ-EP0-002
    fn ep0_addressed_request_round_trips_through_the_existing_echo_back_rule() {
        // No EP0-specific echo-back logic is needed: build_response_info/
        // verify_echo_back already operate purely on byte_bus_id, and 0
        // passes through unchanged like any other value.
        let request = ByteMessageInfo {
            byte_bus_id: EP0_BYTE_BUS_ID,
            op: false,
            ..ByteMessageInfo::default()
        };
        let response = crate::acf::build_response_info(&request, ByteMessageInfo::default());
        assert_eq!(response.byte_bus_id, EP0_BYTE_BUS_ID);
        assert_eq!(crate::acf::verify_echo_back(&request, &response), Ok(()));
    }

    // ── Root client ───────────────────────────────────────────────────────

    fn stream(unique_id: u16) -> StreamId {
        StreamId::new([0x02, 0x11, 0x22, 0x33, 0x44, 0x55], unique_id)
    }

    #[test]
    // fusa:test REQ-EP0-007
    fn is_root_client_true_only_for_the_designated_stream() {
        let root = stream(1);
        let other = stream(2);
        assert!(is_root_client(Some(root), root));
        assert!(!is_root_client(Some(root), other));
        assert!(!is_root_client(None, root));
        assert!(!is_root_client(None, other));
    }

    #[test]
    // fusa:test REQ-EP0-008
    fn root_client_read_agrees_with_check_ep0_access_regardless_of_root_status() {
        let root = stream(1);
        let non_root = stream(2);
        for state in ALL_STATES {
            for category in ALL_CATEGORIES {
                let info = info_with_op(false);
                let plain = check_ep0_access(state, category, &info);
                assert_eq!(
                    check_ep0_access_for_stream(state, category, &info, root, Some(root)),
                    plain,
                    "root reader, {state:?} {category:?}"
                );
                assert_eq!(
                    check_ep0_access_for_stream(state, category, &info, non_root, Some(root)),
                    plain,
                    "non-root reader, {state:?} {category:?}"
                );
                assert_eq!(
                    check_ep0_access_for_stream(state, category, &info, non_root, None),
                    plain,
                    "reader with no root client designated, {state:?} {category:?}"
                );
            }
        }
    }

    #[test]
    // fusa:test REQ-EP0-009
    fn root_client_write_agrees_with_check_ep0_access_for_the_root_stream() {
        let root = stream(1);
        for state in ALL_STATES {
            for category in ALL_CATEGORIES {
                let info = info_with_op(true);
                assert_eq!(
                    check_ep0_access_for_stream(state, category, &info, root, Some(root)),
                    check_ep0_access(state, category, &info),
                    "{state:?} {category:?}"
                );
            }
        }
    }

    #[test]
    // fusa:test REQ-EP0-010
    fn non_root_write_is_always_rejected_with_root_client_required() {
        let root = stream(1);
        let non_root = stream(2);
        for state in ALL_STATES {
            for category in ALL_CATEGORIES {
                let info = info_with_op(true);
                assert_eq!(
                    check_ep0_access_for_stream(state, category, &info, non_root, Some(root)),
                    Err(RcpError::UnauthorizedAccess),
                    "non-root writer, {state:?} {category:?}"
                );
                assert_eq!(
                    check_ep0_access_for_stream(state, category, &info, non_root, None),
                    Err(RcpError::UnauthorizedAccess),
                    "writer with no root client designated, {state:?} {category:?}"
                );
            }
        }
    }

    #[test]
    // fusa:test REQ-EP0-010
    fn root_client_write_can_still_be_rejected_by_lifecycle_state() {
        // Root-client status only clears the root-client gate -- it does
        // not bypass the lifecycle-state gating check_ep0_access already
        // enforces. RcpConfig is unreachable while HwUnconfigured even for
        // the root client.
        let root = stream(1);
        let info = info_with_op(true);
        assert_eq!(
            check_ep0_access_for_stream(
                RcServerState::HwUnconfigured,
                RegisterCategory::RcpConfig,
                &info,
                root,
                Some(root),
            ),
            Err(RcpError::UnauthorizedAccess)
        );
    }

    #[test]
    // fusa:test REQ-EP0-011
    fn check_ep0_access_for_stream_never_panics_for_any_combination() {
        let root = stream(1);
        let non_root = stream(2);
        for state in ALL_STATES {
            for category in ALL_CATEGORIES {
                for op in [false, true] {
                    let info = info_with_op(op);
                    for (requesting_stream, root_client) in [
                        (root, Some(root)),
                        (non_root, Some(root)),
                        (non_root, None),
                        (root, None),
                    ] {
                        let _ = check_ep0_access_for_stream(
                            state,
                            category,
                            &info,
                            requesting_stream,
                            root_client,
                        );
                    }
                }
            }
        }
    }
}
