// fusa:req REQ-DISC-001
// fusa:req REQ-DISC-002
// fusa:req REQ-DISC-003
// fusa:req REQ-DISC-004
// fusa:req REQ-DISC-005
// fusa:req REQ-DISC-006
// fusa:req REQ-DISC-007
// fusa:req REQ-DISC-008
// fusa:req REQ-DISC-009
// fusa:req REQ-DISC-010
// fusa:req REQ-DISC-011
// fusa:req REQ-DISC-012
// fusa:req REQ-DISC-013
// fusa:req REQ-DISC-014
// fusa:req REQ-DISC-015
// fusa:req REQ-DISC-016
// fusa:req REQ-DISC-017
// fusa:req REQ-DISC-018
// fusa:req REQ-DISC-019
// fusa:req REQ-DISC-020
// fusa:req REQ-DISC-021

//! Discovery request/response, discovery-stream claiming, multi-client
//! coexistence, and the client-side discovery cache — TC18 register-map
//! model (`ROADMAP.md` Milestone 3 "Discovery" subsection, all four
//! checklist bullets: "Discovery request/response", "Discovery-stream
//! claiming: first-claimant rule, `Discovery_TimeOut` (~20 ms default)
//! lapse-and-reopen behavior", "Multi-client coexistence: other clients may
//! still read via discovery while a stream is claimed; only the claimant
//! may configure", and "Client-side discovery cache so re-discovery isn't
//! mandatory on every power cycle for already-known topology").
//!
//! This module begins Milestone 3, which per the subsection's own Goal text
//! replaces [`crate::mdns`] as the *mandatory* discovery path (mDNS may
//! continue to exist as a complementary network-rendezvous helper, per the
//! satellite disposition table, but is not a substitute for this). Nothing
//! here reuses or extends [`crate::mdns`]'s `Zone`/host/port/txt-record
//! model — that is a different, private-protocol concept with nothing in
//! common with the TC18 broadcast-`ACF_ABB` mechanism modeled below.
//!
//! All four of this subsection's checklist bullets are now in scope, which
//! closes the "Discovery" subsection out entirely. Deliberately still out of
//! scope, per every prior Milestone 1/2/3 entry's own discipline:
//!
//! - Wiring any of the below into an actual decoder, dispatch loop, or
//!   [`crate::avtp`]/[`crate::acf`] caller — this module remains additive
//!   standalone plumbing only, matching the discipline every prior
//!   Milestone 1/2 entry already established. In particular,
//!   [`DiscoveryClaim`]/[`try_claim_discovery_stream`]/
//!   [`check_discovery_access`]/[`DiscoveryCache`] model claim/cache state
//!   as plain data a caller owns and threads through explicitly (mirroring
//!   how [`build_discovery_response`] takes `state`/`general` as
//!   caller-supplied values rather than owning them) — nothing here spawns a
//!   timer thread, holds a lock, or reads the real clock itself.
//!
//!   `ROADMAP.md` Milestone 9's `udp` REPLACE row (its still-open "deeper
//!   rebuild": register-map-driven dispatch, discovery integration) is the
//!   first item to actually wire this module into a live path:
//!   [`crate::udp::UdpRcServer`] composes [`is_discovery_request`]/
//!   [`build_discovery_response`]/[`check_discovery_access`]/
//!   [`try_claim_discovery_stream`] (and the new
//!   [`is_discovery_configure_request`] below, added by that same item) into
//!   its own inbound-datagram dispatch. This module's own functions are
//!   otherwise unchanged by that wiring — see [`crate::udp`]'s module doc
//!   comment for the wire-level "configure" encoding [`crate::udp`] had to
//!   choose, and this function's own doc comment for why it lives here
//!   rather than in `udp.rs` itself.
//!
//! This module composes with, rather than duplicates, three already-landed
//! Milestone 1/2 pieces:
//!
//! - [`crate::acf::AcfAbbMessage`]/[`crate::acf::encode_acf_abb`]/
//!   [`crate::acf::decode_acf_abb`] — the timestamp-free ACF framing a
//!   discovery request/response is carried in.
//! - [`crate::ep0::EP0_BYTE_BUS_ID`]/[`crate::ep0::route_byte_bus_id`]/
//!   [`crate::ep0::access_kind`] — the already-reserved EP0 pseudo-endpoint
//!   address and the read/write direction convention, reused here rather
//!   than re-derived.
//! - [`crate::lifecycle::check_register_reachable`] with
//!   [`crate::lifecycle::RegisterCategory::General`] — already modeled as
//!   reachable in every [`crate::lifecycle::RcServerState`], which is
//!   exactly this item's "answerable in **any** lifecycle state"
//!   requirement. [`build_discovery_response`] calls this gate explicitly
//!   (rather than assuming its always-true outcome) so that composition is
//!   demonstrated and tested, not merely asserted.
//! - [`crate::regmap::GeneralRegisters`] — register address 0's
//!   field-level content (`svr_oa_tc18_magic_nr` and the rest of `§3.6`'s
//!   table), reused as the discovery response payload's content rather than
//!   inventing a new addressing scheme, and as the source
//!   [`DiscoveryCache::remember`] snapshots its cache-worthy subset from.
//!
//! ## Multi-client coexistence
//!
//! [`DiscoveryAccessKind`]/[`check_discovery_access`] add this subsection's
//! third checklist bullet: a distinct read/configure access-kind
//! distinction over the discovery stream, layered on top of (not replacing)
//! [`try_claim_discovery_stream`]'s existing claim-tracking. This mirrors,
//! at the discovery-claim level, the same shape [`crate::ep0::is_root_client`]/
//! [`crate::ep0::check_ep0_access_for_stream`] already established at the
//! EP0 root-client level — see that module's own "Root client" section.
//!
//! - Reading discovery info ([`DiscoveryAccessKind::Read`]) —
//!   [`build_discovery_request`]/[`build_discovery_response`]'s existing
//!   broadcast mechanism — always succeeds, for any requester, regardless of
//!   whether the discovery stream is currently claimed, by whom, or whether
//!   an existing claim has lapsed. [`check_discovery_access`] never even
//!   inspects its `current` claim parameter for this kind, mirroring
//!   [`build_discovery_response`]'s own "answerable in **any** lifecycle
//!   state" unconditional behavior.
//! - Configuring the discovery stream ([`DiscoveryAccessKind::Configure`])
//!   is restricted to whichever [`crate::avtp::StreamId`] currently holds
//!   the live (not [`DiscoveryClaim::has_lapsed`]) claim: an unclaimed
//!   stream, the live claimant itself, or any requester once the existing
//!   claim has lapsed, may configure — exactly the same "who may act" rule
//!   [`try_claim_discovery_stream`]'s own first-claimant logic already
//!   applies when granting a claim (both now share the same internal
//!   `claim_permits` helper, rather than duplicating that rule twice). A
//!   different, still-live claimant's configure attempt is rejected with
//!   `Err(RcpError::UnauthorizedAccess)`.
//! - [`DISCOVERY_BROADCAST_STREAM_ID`] is always rejected as a
//!   `Configure` requester, with `Err(RcpError::InvalidParameter)`,
//!   regardless of claim state — mirroring
//!   [`try_claim_discovery_stream`]'s own rejection of the broadcast
//!   sentinel as a claimant, for the same reason: a broadcast address names
//!   no single real client, so it cannot meaningfully be "the claimant" a
//!   configure action is attributed to.
//!
//! This module still performs no register I/O, does not itself decide what
//! wire-level operation counts as "configuring" the discovery stream (that
//! remains a later, decoder/dispatch-level concern — see the out-of-scope
//! list above), and takes [`DiscoveryAccessKind`] as a caller-supplied value
//! rather than deriving it from a decoded message — see this module's
//! Provenance note for why.
//!
//! ## Client-side discovery cache
//!
//! [`DiscoveryCache`]/[`DiscoveryCacheEntry`] add this subsection's fourth
//! and final checklist bullet: a client-side cache of already-discovered
//! servers, so a client reconnecting to previously-known topology is not
//! forced to re-run [`build_discovery_request`]/[`build_discovery_response`]'s
//! broadcast exchange on every power cycle.
//!
//! - [`DiscoveryCache`] is a plain [`StreamId`]-keyed map a caller owns and
//!   threads through explicitly, matching the same "no timer thread, no
//!   lock, no real-clock read of its own" discipline
//!   [`DiscoveryClaim`]/[`try_claim_discovery_stream`] already established
//!   for claim state — see the out-of-scope list above. Claim state and
//!   cache state are deliberately independent: this module does not use a
//!   [`DiscoveryClaim`]'s lapse to auto-evict a [`DiscoveryCache`] entry, or
//!   vice versa — see this module's Provenance note for why.
//! - [`DiscoveryCache::remember`] records the cache-worthy subset of a
//!   [`GeneralRegisters`] value the caller already obtained (e.g. by
//!   decoding a prior [`build_discovery_response`] payload) under the
//!   [`StreamId`] the caller reached that server through — [`DiscoveryCache`]
//!   performs no discovery I/O of its own, exactly as [`build_discovery_response`]
//!   performs none either.
//! - [`DiscoveryCache::is_known`]/[`DiscoveryCacheEntry::is_stale`] let a
//!   caller decide, given its own choice of staleness window, whether a
//!   cached entry is still fresh enough to skip re-discovery for — this
//!   module deliberately imposes no default staleness window of its own
//!   (unlike [`DISCOVERY_TIME_OUT`], which the roadmap explicitly names a
//!   default for) — see this module's Provenance note.
//! - [`DiscoveryCacheEntry::matches`] lets a caller confirm a freshly
//!   observed [`GeneralRegisters`] still agrees with a cached entry's
//!   identity, distinguishing "the same previously discovered server,
//!   cache still valid" from "a different server now answers at this
//!   address, cache entry is stale in the identity sense and real
//!   re-discovery is warranted" — a currently-fresh (per
//!   [`DiscoveryCacheEntry::is_stale`]) entry can still fail this check if
//!   the underlying topology changed without the client observing it age
//!   out.
//! - [`DiscoveryCache::invalidate`] lets a caller drop a single entry
//!   explicitly (e.g. after [`DiscoveryCacheEntry::matches`] returns
//!   `false`, or on any other caller-decided trigger) without needing to
//!   discard the whole cache.
//!
//! ## Provenance note
//!
//! Nine working interpretations this item introduces, per Guiding
//! Principle 5, are flagged here for reconciliation against the OPEN
//! Alliance TC18 Remote Control Protocol Specification v0.5.1_RC's actual
//! behavior (never its prose) before being relied on for interop with a
//! real TC18 RC Server:
//!
//! - **Broadcast addressing.** `ROADMAP.md`'s own checklist wording states
//!   that the discovery request must be "broadcastable" but does not name a
//!   wire-level broadcast address convention, and
//!   [`crate::avtp::StreamId`] has no broadcast/multicast concept of its
//!   own (it is built as `sender_mac || unique_id`, always identifying one
//!   specific sender). Rather than invent an out-of-band "this AVTPDU is a
//!   broadcast" flag with no roadmap-named field to carry it, this module
//!   reuses the reserved IEEE 802.3 all-ones Ethernet broadcast MAC address
//!   (`FF:FF:FF:FF:FF:FF`) as `sender_mac`, paired with `unique_id == 0`,
//!   as a single well-known sentinel [`crate::avtp::StreamId`] value: see
//!   [`DISCOVERY_BROADCAST_STREAM_ID`]. This is a crate-local sentinel
//!   convention only, not a claim that a real TC18 RC Server recognizes
//!   this exact `stream_id` value as "broadcast" on the wire.
//! - **Register address on the wire.** Milestone 1's `byte_message_info`
//!   header ([`crate::acf::ByteMessageInfo`]) has no dedicated
//!   register-address field — only `byte_bus_id` (the EP0/device-endpoint
//!   selector [`crate::ep0`] already models) and the opaque
//!   `read_size`/`segment_num` byte, neither of which this crate's Milestone
//!   1 provenance note claims is a register address. Rather than guess an
//!   unconfirmed bit position inside `byte_message_info` for a field the
//!   roadmap has not named there, this module carries the register address
//!   as a big-endian `u16` prefix of [`crate::acf::AcfAbbMessage::payload`]
//!   ([`DISCOVERY_REGISTER_ADDRESS_LEN`] bytes wide, matching the crate's
//!   existing big-endian convention — see [`crate::regmap`]'s own
//!   `encode`/`decode` methods). This mirrors how a discovery *response*
//!   already has to carry [`crate::regmap::GeneralRegisters::encode`]'s
//!   fixed-length block somewhere, and payload is the only carrier this
//!   crate's Milestone 1 framing offers either direction.
//! - **Claim identity.** The roadmap names a "first-claimant rule" but does
//!   not say what identifies a claimant. Rather than invent a new identity
//!   type, this module reuses [`crate::avtp::StreamId`] — the same type
//!   [`DISCOVERY_BROADCAST_STREAM_ID`] above already models a sender
//!   identity with — as the claimant identity
//!   [`try_claim_discovery_stream`] compares against. The sentinel broadcast
//!   value itself is rejected as a claimant (see
//!   [`RcpError::InvalidParameter`] in that function's own doc comment):
//!   a broadcast address names no single real client, so it cannot
//!   meaningfully hold an exclusive claim.
//! - **Re-claim by the current claimant.** Nothing in the roadmap's wording
//!   says whether the claimant that already holds a live claim may issue
//!   another claim request against itself. This module treats that case as
//!   a no-op refresh (it succeeds and re-timestamps the claim at `now`,
//!   rather than being rejected as "a second claimant") since rejecting a
//!   claimant's own repeat request would make an idle-but-still-interested
//!   claimant indistinguishable from one that never claimed at all —
//!   defeating the purpose of a lapse timer. This is a working
//!   interpretation, not a transcription of confirmed spec behavior.
//! - **Read/configure access-kind distinction.** The roadmap's checklist
//!   wording distinguishes "read via discovery" from "configure the
//!   discovery stream" but does not say how a caller tells the two apart on
//!   the wire: [`is_discovery_request`]'s own read-direction (`op = false`)
//!   check, reused from [`crate::ep0::access_kind`], already identifies the
//!   *discovery request/response* mechanism specifically, but nothing in
//!   this crate's Milestone 1 framing names a distinct operation, register,
//!   or flag for "configuring the discovery stream" the way, say,
//!   [`crate::ep0::Ep0AccessKind::Write`] is derived structurally from
//!   `ByteMessageInfo::op`. Rather than guess an unconfirmed encoding for a
//!   concept the roadmap names but this crate's wire model does not yet
//!   carry, [`DiscoveryAccessKind`] is a plain caller-supplied value
//!   [`check_discovery_access`] takes as a parameter, not a value derived
//!   from a decoded message — deferring "which wire operation(s) count as a
//!   configure attempt" to whichever later item wires this into an actual
//!   decoder/dispatch loop (see the out-of-scope list above).
//! - **`RcpError` choice for a non-claimant's rejected configure attempt.**
//!   Per this crate's Milestone 2 "Error Model" precedent
//!   ([`RcpError`]'s own doc comment), this module reuses
//!   [`RcpError::UnauthorizedAccess`] rather than inventing a new
//!   provisional sentinel: a non-claimant's configure attempt is the same
//!   shape of failure as [`crate::ep0::check_ep0_access_for_stream`]'s
//!   non-root-client write rejection — the requesting stream's identity
//!   does not authorize the attempted access — just gated on the discovery
//!   claim axis instead of the EP0 root-client axis. The broadcast
//!   sentinel's rejection as a `Configure` requester reuses
//!   [`RcpError::InvalidParameter`] for the same reason
//!   [`try_claim_discovery_stream`] already does: the supplied requester
//!   value itself is not a meaningful single-client identity, independent
//!   of any claim state.
//! - **Which `GeneralRegisters` fields are cache-worthy.** The roadmap names
//!   `svr_oa_tc18_magic_nr`, `svr_vendor_id`, `svr_device_id`, and
//!   `svr_ep_count` as example identifying/topology fields but does not give
//!   an exhaustive list. [`DiscoveryCacheEntry`] snapshots exactly those four
//!   plus `svr_version` (the protocol version a client would also want to
//!   recognize a server by across a reconnect) and deliberately excludes the
//!   rest of [`GeneralRegisters`] — in particular
//!   `svr_configuration_lock` and the `§3.6` table-descriptor fields
//!   (`svr_hw_cfg` and siblings), which describe current, potentially
//!   reconfigurable server state rather than a server's stable identity, and
//!   so are treated as must-always-be-read-fresh rather than cache-worthy.
//!   This five-field subset is this crate's own working judgment call, not a
//!   spec-cited list.
//! - **Cache staleness policy.** The roadmap's checklist wording does not
//!   state a cache lifetime, unlike [`DISCOVERY_TIME_OUT`]'s explicitly
//!   roadmap-stated `~20 ms` default. Rather than invent an unstated
//!   default staleness window, [`DiscoveryCache::is_known`]/
//!   [`DiscoveryCacheEntry::is_stale`] take `now`/`max_age` as caller-supplied
//!   parameters (mirroring [`DiscoveryClaim::has_lapsed`]'s own
//!   `now`/`timeout` shape) rather than this module picking a duration of
//!   its own.
//! - **Cache/claim independence.** Nothing in the roadmap's wording says
//!   whether a lapsed [`DiscoveryClaim`] should evict a [`DiscoveryCache`]
//!   entry for the same [`StreamId`], or whether an invalidated cache entry
//!   should affect claim state. This module treats the two as orthogonal:
//!   a discovery-stream claim is about who may currently *configure* the
//!   discovery stream, while a cache entry is about what a client
//!   previously *learned* about a server's identity — a claim lapsing (e.g.
//!   because the claimant went briefly idle) says nothing about whether the
//!   server's identity changed, and an invalidated cache entry says nothing
//!   about who currently holds the discovery-stream claim. A caller that
//!   wants the two coupled composes them explicitly rather than this module
//!   assuming that coupling on the caller's behalf.
//! - **Wire encoding for a "configure" attempt.** Added by the `udp`
//!   REPLACE item that first wires this module into a dispatch loop (see the
//!   out-of-scope list above): [`is_discovery_configure_request`] reuses
//!   [`is_discovery_request`]'s exact addressing/register shape, flipping
//!   only the read/write direction bit — this crate's own chosen encoding,
//!   not a confirmed spec field, since neither the roadmap nor this crate's
//!   Milestone 1 framing names one. An alternative this module rejected: a
//!   dedicated non-zero register address for "configure," which would have
//!   split discovery's on-wire footprint across two register addresses for
//!   no roadmap-stated reason, where the read/write bit `ByteMessageInfo`
//!   already carries needs none.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::acf::{AcfAbbMessage, ByteMessageInfo};
use crate::avtp::StreamId;
use crate::ep0::{access_kind, route_byte_bus_id, Ep0AccessKind, RequestRoute, EP0_BYTE_BUS_ID};
use crate::lifecycle::{check_register_reachable, RcServerState, RegisterCategory};
use crate::regmap::GeneralRegisters;
use crate::RcpError;

// ── Broadcast addressing ─────────────────────────────────────────────────────

/// This module's sentinel `sender_mac` for a broadcast-addressed discovery
/// request: the reserved IEEE 802.3 all-ones Ethernet broadcast address.
/// See this module's Provenance note for why this crate-local convention
/// was chosen over an unconfirmed alternative.
pub const DISCOVERY_BROADCAST_SENDER_MAC: [u8; 6] = [0xFF; 6];

/// This module's sentinel `unique_id` for a broadcast-addressed discovery
/// request, paired with [`DISCOVERY_BROADCAST_SENDER_MAC`]. See this
/// module's Provenance note.
pub const DISCOVERY_BROADCAST_UNIQUE_ID: u16 = 0;

/// The sentinel broadcast [`StreamId`] a discovery request is addressed to,
/// composed from [`DISCOVERY_BROADCAST_SENDER_MAC`] and
/// [`DISCOVERY_BROADCAST_UNIQUE_ID`]. See this module's Provenance note.
pub const DISCOVERY_BROADCAST_STREAM_ID: StreamId = StreamId {
    sender_mac: DISCOVERY_BROADCAST_SENDER_MAC,
    unique_id: DISCOVERY_BROADCAST_UNIQUE_ID,
};

/// Is `stream_id` this module's sentinel broadcast discovery address?
///
/// Never panics for any input.
// fusa:req REQ-DISC-001
pub fn is_discovery_broadcast_stream_id(stream_id: StreamId) -> bool {
    stream_id == DISCOVERY_BROADCAST_STREAM_ID
}

// ── Register address on the wire ─────────────────────────────────────────────

/// Byte width of the big-endian register-address prefix this module carries
/// at the start of an [`AcfAbbMessage::payload`]. See this module's
/// Provenance note.
pub const DISCOVERY_REGISTER_ADDRESS_LEN: usize = 2;

/// The register address a discovery request targets: register address 0,
/// the start of [`crate::regmap::GeneralRegisters`]'s block (which
/// itself begins with `svr_oa_tc18_magic_nr`).
pub const DISCOVERY_REGISTER_ADDRESS: u16 = 0;

/// Decode a [`DISCOVERY_REGISTER_ADDRESS_LEN`]-byte big-endian register
/// address from the start of `payload`.
///
/// Returns `None` if `payload` is shorter than
/// [`DISCOVERY_REGISTER_ADDRESS_LEN`]. Never panics for any input.
fn decode_register_address(payload: &[u8]) -> Option<u16> {
    if payload.len() < DISCOVERY_REGISTER_ADDRESS_LEN {
        return None;
    }
    Some(u16::from_be_bytes([payload[0], payload[1]]))
}

// ── Discovery request ─────────────────────────────────────────────────────────

/// Build a discovery request: a read-direction [`AcfAbbMessage`] addressed
/// to [`EP0_BYTE_BUS_ID`], targeting [`DISCOVERY_REGISTER_ADDRESS`].
///
/// `transaction_num` is passed through to the built message's
/// `byte_message_info` unchanged — this function has no opinion on
/// transaction-id allocation, which remains a caller/later-milestone
/// concern. The result always satisfies [`is_discovery_request`]. Never
/// panics for any input.
// fusa:req REQ-DISC-002
pub fn build_discovery_request(transaction_num: u8) -> AcfAbbMessage {
    AcfAbbMessage {
        info: ByteMessageInfo {
            byte_bus_id: EP0_BYTE_BUS_ID,
            op: false, // read, per crate::ep0::Ep0AccessKind's op convention
            transaction_num,
            ..ByteMessageInfo::default()
        },
        payload: DISCOVERY_REGISTER_ADDRESS.to_be_bytes().to_vec(),
    }
}

/// Is `msg` a discovery request: a read-direction `ACF_ABB` addressed to
/// [`EP0_BYTE_BUS_ID`], whose payload's leading
/// [`DISCOVERY_REGISTER_ADDRESS_LEN`] bytes decode to
/// [`DISCOVERY_REGISTER_ADDRESS`]?
///
/// Reuses [`crate::ep0::route_byte_bus_id`]/[`crate::ep0::access_kind`]
/// rather than re-deriving the EP0/read-direction checks. Trailing payload
/// bytes beyond the register-address prefix, if any, are ignored — this
/// function does not itself decide whether extra payload content is
/// otherwise meaningful or malformed. Never panics for any input.
// fusa:req REQ-DISC-002
// fusa:req REQ-DISC-003
pub fn is_discovery_request(msg: &AcfAbbMessage) -> bool {
    route_byte_bus_id(msg.info.byte_bus_id) == RequestRoute::Ep0
        && access_kind(&msg.info) == Ep0AccessKind::Read
        && decode_register_address(&msg.payload) == Some(DISCOVERY_REGISTER_ADDRESS)
}

/// Is `msg` this module's chosen wire encoding for a "configure the
/// discovery stream" access attempt: a write-direction `ACF_ABB` addressed
/// to [`EP0_BYTE_BUS_ID`], whose payload's leading
/// [`DISCOVERY_REGISTER_ADDRESS_LEN`] bytes decode to
/// [`DISCOVERY_REGISTER_ADDRESS`] — the same shape [`is_discovery_request`]
/// recognizes, but write- rather than read-direction.
///
/// Added by `ROADMAP.md` Milestone 9's `udp` REPLACE item (the "deeper
/// rebuild" the roadmap's own Progress note names: register-map-driven
/// dispatch, discovery integration) — the first item to wire this module
/// into an actual decoder/dispatch loop, per this module's own out-of-scope
/// list above. Per Guiding Principle 5, this is flagged as a working
/// interpretation, not a transcription of confirmed spec behavior: neither
/// `ROADMAP.md`'s checklist wording nor this crate's Milestone 1 framing
/// names a distinct field, register, or opcode for "configuring the
/// discovery stream" (see [`DiscoveryAccessKind`]'s own doc comment, which
/// already flagged deferring this exact choice). Reusing
/// [`is_discovery_request`]'s own read/write-direction symmetry —
/// [`crate::ep0::Ep0AccessKind`]'s existing convention for every other EP0
/// register — needs no new wire field and keeps the discovery request/
/// response/configure trio addressed at the one register
/// ([`DISCOVERY_REGISTER_ADDRESS`]) the roadmap already names, rather than
/// inventing a second one. Lives here, next to [`is_discovery_request`],
/// rather than in `crate::udp` (its only caller today): recognizing this
/// wire shape is a discovery-module-level concept — should some other
/// transport ever also need to recognize it, duplicating this function
/// there would violate the same "reuse, don't duplicate" discipline this
/// module's own doc comment already applies to [`crate::acf`]/[`crate::ep0`]
/// composition.
///
/// Trailing payload bytes beyond the register-address prefix, if any, are
/// ignored, mirroring [`is_discovery_request`]. Never panics for any input.
// fusa:req REQ-DISC-021
pub fn is_discovery_configure_request(msg: &AcfAbbMessage) -> bool {
    route_byte_bus_id(msg.info.byte_bus_id) == RequestRoute::Ep0
        && access_kind(&msg.info) == Ep0AccessKind::Write
        && decode_register_address(&msg.payload) == Some(DISCOVERY_REGISTER_ADDRESS)
}

// ── Discovery response ───────────────────────────────────────────────────────

/// Build a discovery response to `request`, given the RC Server's current
/// `state` and its `general` register content.
///
/// Composes with [`check_register_reachable`] against
/// [`RegisterCategory::General`] explicitly — rather than assuming its
/// always-reachable-in-every-state outcome — so that this item's
/// "answerable in **any** lifecycle state" requirement is demonstrated and
/// tested against the real gate, not merely asserted in a doc comment. The
/// response payload is `general.encode()` verbatim (register address 0's
/// full content, per [`crate::regmap::GeneralRegisters`]), and its
/// `byte_message_info` header echoes `request`'s `byte_bus_id` per the
/// existing echo-back rule ([`crate::acf::build_response_info`]).
///
/// This function performs no register I/O of its own: `general` is a
/// caller-supplied value, exactly as [`crate::ep0::check_ep0_access`]
/// performs no register I/O either — see that module's own doc comment for
/// why. Never panics for any input.
// fusa:req REQ-DISC-004
pub fn build_discovery_response(
    request: &ByteMessageInfo,
    state: RcServerState,
    general: &GeneralRegisters,
) -> Result<AcfAbbMessage, RcpError> {
    check_register_reachable(state, RegisterCategory::General)?;

    let response_info = crate::acf::build_response_info(request, ByteMessageInfo::default());
    Ok(AcfAbbMessage {
        info: response_info,
        payload: general.encode().to_vec(),
    })
}

// ── Discovery-stream claiming ────────────────────────────────────────────────

/// Default `Discovery_TimeOut` lapse window: the interval a claim on the
/// discovery stream survives without being refreshed before it is
/// considered lapsed and reopens to any claimant. Matches the roadmap's
/// stated `~20 ms` default.
pub const DISCOVERY_TIME_OUT: Duration = Duration::from_millis(20);

/// A live claim on the discovery stream, held by exactly one
/// [`StreamId`]-identified claimant as of a point in time.
///
/// Deliberately plain data: nothing here reads the real clock, spawns a
/// timer, or holds a lock — see this module's doc comment for why. A caller
/// owns an `Option<DiscoveryClaim>` and threads it through
/// [`try_claim_discovery_stream`] explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiscoveryClaim {
    claimant: StreamId,
    claimed_at: Instant,
}

impl DiscoveryClaim {
    /// The [`StreamId`] that currently holds this claim.
    pub fn claimant(&self) -> StreamId {
        self.claimant
    }

    /// The instant this claim was most recently established or refreshed.
    pub fn claimed_at(&self) -> Instant {
        self.claimed_at
    }

    /// Has this claim lapsed as of `now`, given `timeout`?
    ///
    /// A claim lapses once at least `timeout` has elapsed since
    /// [`Self::claimed_at`]. Uses [`Instant::saturating_duration_since`], so
    /// an `now` that is (incorrectly) earlier than [`Self::claimed_at`]
    /// reads as zero elapsed time rather than panicking or wrapping. Never
    /// panics for any input.
    // fusa:req REQ-DISC-006
    pub fn has_lapsed(&self, now: Instant, timeout: Duration) -> bool {
        now.saturating_duration_since(self.claimed_at) >= timeout
    }
}

/// Attempt to claim the discovery stream as `claimant` at `now`, applying
/// the first-claimant rule against `current`'s existing claim (if any).
///
/// - If `current` is `None`, or holds a claim that has
///   [`DiscoveryClaim::has_lapsed`] as of `now` under `timeout`, the claim
///   succeeds: a new [`DiscoveryClaim`] timestamped `now` is returned. This
///   is the `Discovery_TimeOut` lapse-and-reopen behavior.
/// - If `current` already holds a live claim for the same `claimant`, the
///   claim also succeeds and is re-timestamped at `now` (a refresh, not a
///   second claimant — see this module's Provenance note).
/// - If `current` holds a live claim for a *different* claimant, the claim
///   is rejected with `Err(RcpError::RequestRejected)` — the first-claimant
///   rule. `current`'s existing claim is unaffected by a rejected attempt
///   (this function returns a new claim on success or an error on failure;
///   it never mutates state itself, since it owns none).
/// - `claimant == `[`DISCOVERY_BROADCAST_STREAM_ID`] is always rejected
///   with `Err(RcpError::InvalidParameter)`, regardless of `current` — see
///   this module's Provenance note for why a broadcast address cannot hold
///   an exclusive claim.
///
/// Never panics for any input.
// fusa:req REQ-DISC-007
// fusa:req REQ-DISC-008
// fusa:req REQ-DISC-009
pub fn try_claim_discovery_stream(
    current: Option<DiscoveryClaim>,
    claimant: StreamId,
    now: Instant,
    timeout: Duration,
) -> Result<DiscoveryClaim, RcpError> {
    // fusa:req REQ-DISC-009
    if is_discovery_broadcast_stream_id(claimant) {
        return Err(RcpError::InvalidParameter);
    }

    // fusa:req REQ-DISC-007
    if claim_permits(current, claimant, now, timeout) {
        Ok(DiscoveryClaim {
            claimant,
            claimed_at: now,
        })
    } else {
        // fusa:req REQ-DISC-008
        Err(RcpError::RequestRejected)
    }
}

/// Shared "who may act" rule underlying both [`try_claim_discovery_stream`]'s
/// first-claimant grant decision and [`check_discovery_access`]'s
/// `Configure`-kind gate: is `who` permitted against `current`'s claim state
/// as of `now` under `timeout`?
///
/// `true` iff `current` is `None` (unclaimed), `current`'s claim already
/// belongs to `who`, or `current`'s claim has [`DiscoveryClaim::has_lapsed`]
/// as of `now` under `timeout`. Does not itself special-case
/// [`DISCOVERY_BROADCAST_STREAM_ID`] as `who` — both callers apply that
/// rejection themselves, since it carries a different `RcpError` variant at
/// each call site. Never panics for any input.
fn claim_permits(
    current: Option<DiscoveryClaim>,
    who: StreamId,
    now: Instant,
    timeout: Duration,
) -> bool {
    match current {
        None => true,
        Some(existing) => existing.claimant == who || existing.has_lapsed(now, timeout),
    }
}

// ── Multi-client coexistence: read/configure access-kind distinction ───────────

/// The kind of access being attempted against the discovery stream: reading
/// discovery info (always open, claim state notwithstanding) vs. configuring
/// the discovery stream (restricted to the live claimant). See this module's
/// "Multi-client coexistence" section and Provenance note for the full
/// rationale, including why this is a caller-supplied value rather than one
/// [`check_discovery_access`] derives from a decoded message itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// fusa:req REQ-DISC-011
pub enum DiscoveryAccessKind {
    /// Reading discovery info — the broadcast discovery request/response
    /// mechanism ([`build_discovery_request`]/[`build_discovery_response`]).
    Read,
    /// Configuring the discovery stream itself.
    Configure,
}

/// Is a `kind`-typed access against the discovery stream, by `requester`,
/// permitted given `current`'s claim state as of `now` under `timeout`?
///
/// - [`DiscoveryAccessKind::Read`] always succeeds: `current`, `now`, and
///   `timeout` are not even inspected for this kind. Any requester,
///   claimant or not, may read discovery info regardless of claim state —
///   see this module's "Multi-client coexistence" section.
/// - [`DiscoveryAccessKind::Configure`] succeeds iff [`claim_permits`]
///   answers `true` for `requester` against `current` (unclaimed, already
///   `requester`'s own live claim, or a lapsed claim) — the same rule
///   [`try_claim_discovery_stream`] itself applies when granting a claim.
///   Otherwise (a live claim held by a different claimant), it is rejected
///   with `Err(RcpError::UnauthorizedAccess)`, mirroring
///   [`crate::ep0::check_ep0_access_for_stream`]'s non-root-writer
///   rejection.
/// - `requester == `[`DISCOVERY_BROADCAST_STREAM_ID`] is always rejected for
///   [`DiscoveryAccessKind::Configure`] with `Err(RcpError::InvalidParameter)`,
///   regardless of `current` — mirroring [`try_claim_discovery_stream`]'s own
///   rejection of the broadcast sentinel as a claimant.
///
/// Performs no register I/O and does not mutate `current` — like
/// [`try_claim_discovery_stream`], this function only ever answers a
/// question about caller-supplied state. Never panics for any input.
// fusa:req REQ-DISC-012
// fusa:req REQ-DISC-013
// fusa:req REQ-DISC-014
pub fn check_discovery_access(
    current: Option<DiscoveryClaim>,
    requester: StreamId,
    kind: DiscoveryAccessKind,
    now: Instant,
    timeout: Duration,
) -> Result<(), RcpError> {
    match kind {
        DiscoveryAccessKind::Read => Ok(()),
        DiscoveryAccessKind::Configure => {
            // fusa:req REQ-DISC-014
            if is_discovery_broadcast_stream_id(requester) {
                return Err(RcpError::InvalidParameter);
            }

            // fusa:req REQ-DISC-012
            if claim_permits(current, requester, now, timeout) {
                Ok(())
            } else {
                // fusa:req REQ-DISC-013
                Err(RcpError::UnauthorizedAccess)
            }
        }
    }
}

// ── Client-side discovery cache ─────────────────────────────────────────────

/// The cache-worthy subset of a discovered server's [`GeneralRegisters`],
/// snapshotted as of the [`Instant`] it was cached, keyed elsewhere (in
/// [`DiscoveryCache`]) by the [`StreamId`] the caller reached that server
/// through.
///
/// See this module's "Client-side discovery cache" section and Provenance
/// note for which fields were chosen as cache-worthy and why. Deliberately
/// plain data, mirroring [`DiscoveryClaim`]'s own discipline: nothing here
/// reads the real clock, spawns a timer, or holds a lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiscoveryCacheEntry {
    svr_oa_tc18_magic_nr: u32,
    svr_version: u32,
    svr_vendor_id: u16,
    svr_device_id: u16,
    svr_ep_count: u16,
    cached_at: Instant,
}

impl DiscoveryCacheEntry {
    /// Snapshot the cache-worthy subset of `general` as of `cached_at`.
    fn from_general_registers(general: &GeneralRegisters, cached_at: Instant) -> Self {
        Self {
            svr_oa_tc18_magic_nr: general.svr_oa_tc18_magic_nr,
            svr_version: general.svr_version,
            svr_vendor_id: general.svr_vendor_id,
            svr_device_id: general.svr_device_id,
            svr_ep_count: general.svr_ep_count,
            cached_at,
        }
    }

    /// This entry's cached `svr_oa_tc18_magic_nr`.
    pub fn svr_oa_tc18_magic_nr(&self) -> u32 {
        self.svr_oa_tc18_magic_nr
    }

    /// This entry's cached `svr_version`.
    pub fn svr_version(&self) -> u32 {
        self.svr_version
    }

    /// This entry's cached `svr_vendor_id`.
    pub fn svr_vendor_id(&self) -> u16 {
        self.svr_vendor_id
    }

    /// This entry's cached `svr_device_id`.
    pub fn svr_device_id(&self) -> u16 {
        self.svr_device_id
    }

    /// This entry's cached `svr_ep_count`.
    pub fn svr_ep_count(&self) -> u16 {
        self.svr_ep_count
    }

    /// The instant this entry was most recently inserted or refreshed via
    /// [`DiscoveryCache::remember`].
    pub fn cached_at(&self) -> Instant {
        self.cached_at
    }

    /// Has this entry gone stale as of `now`, given `max_age`?
    ///
    /// Mirrors [`DiscoveryClaim::has_lapsed`]'s own boundary/never-panic
    /// discipline exactly: an entry is stale once at least `max_age` has
    /// elapsed since [`Self::cached_at`] (inclusive at the boundary), using
    /// [`Instant::saturating_duration_since`] so an out-of-order `now` reads
    /// as zero elapsed time rather than panicking or wrapping. `max_age` is
    /// entirely caller-supplied — see this module's Provenance note for why
    /// no default is provided here. Never panics for any input.
    // fusa:req REQ-DISC-018
    pub fn is_stale(&self, now: Instant, max_age: Duration) -> bool {
        now.saturating_duration_since(self.cached_at) >= max_age
    }

    /// Does this entry's cached identity still agree with a freshly observed
    /// `general`?
    ///
    /// Compares only the same cache-worthy subset [`Self::from_general_registers`]
    /// snapshots, not [`Self::cached_at`]. `false` means the server this
    /// entry was cached from is no longer the one answering at the cached
    /// [`StreamId`] — a real re-discovery is warranted, not merely a cache
    /// refresh — see this module's "Client-side discovery cache" section.
    /// Never panics for any input.
    // fusa:req REQ-DISC-019
    pub fn matches(&self, general: &GeneralRegisters) -> bool {
        self.svr_oa_tc18_magic_nr == general.svr_oa_tc18_magic_nr
            && self.svr_version == general.svr_version
            && self.svr_vendor_id == general.svr_vendor_id
            && self.svr_device_id == general.svr_device_id
            && self.svr_ep_count == general.svr_ep_count
    }
}

/// A client-side cache of previously discovered servers, keyed by the
/// [`StreamId`] a caller reached each one through.
///
/// See this module's "Client-side discovery cache" section for the full
/// rationale. Deliberately plain, caller-owned state: no timer thread, no
/// lock, no real-clock read of its own — a caller supplies `now` to every
/// staleness-aware method, exactly as [`try_claim_discovery_stream`] and
/// [`check_discovery_access`] already require for claim state.
#[derive(Debug, Clone, Default)]
pub struct DiscoveryCache {
    entries: HashMap<StreamId, DiscoveryCacheEntry>,
}

impl DiscoveryCache {
    /// Construct an empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record (or refresh) `stream_id`'s discovered identity from `general`
    /// as of `now` — e.g. learned from a prior [`build_discovery_response`]
    /// payload this client already decoded.
    ///
    /// Returns `Err(RcpError::InvalidParameter)` — without modifying the
    /// cache — if `stream_id` is [`DISCOVERY_BROADCAST_STREAM_ID`]: the
    /// broadcast sentinel names no single real server, so it cannot
    /// meaningfully be a cache key, mirroring
    /// [`try_claim_discovery_stream`]'s own rejection of it as a claimant.
    /// An existing entry for `stream_id`, if any, is overwritten rather than
    /// preserved. Never panics for any input.
    // fusa:req REQ-DISC-016
    // fusa:req REQ-DISC-017
    pub fn remember(
        &mut self,
        stream_id: StreamId,
        general: &GeneralRegisters,
        now: Instant,
    ) -> Result<(), RcpError> {
        if is_discovery_broadcast_stream_id(stream_id) {
            return Err(RcpError::InvalidParameter);
        }
        self.entries.insert(
            stream_id,
            DiscoveryCacheEntry::from_general_registers(general, now),
        );
        Ok(())
    }

    /// Look up `stream_id`'s cached entry, if any, regardless of staleness.
    ///
    /// Callers that care about staleness should check
    /// [`DiscoveryCacheEntry::is_stale`] themselves, or use [`Self::is_known`]
    /// — this module holds no opinion of its own on what staleness window is
    /// appropriate, see this module's Provenance note. Never panics for any
    /// input.
    pub fn lookup(&self, stream_id: StreamId) -> Option<&DiscoveryCacheEntry> {
        self.entries.get(&stream_id)
    }

    /// Is `stream_id` cached and not [`DiscoveryCacheEntry::is_stale`] as of
    /// `now` under `max_age`?
    ///
    /// The intended use: a client may skip re-running
    /// [`build_discovery_request`]'s broadcast exchange for `stream_id` when
    /// this returns `true`, and falls back to real discovery otherwise
    /// (unknown, or known but stale). Never panics for any input.
    // fusa:req REQ-DISC-016
    // fusa:req REQ-DISC-018
    pub fn is_known(&self, stream_id: StreamId, now: Instant, max_age: Duration) -> bool {
        self.entries
            .get(&stream_id)
            .is_some_and(|entry| !entry.is_stale(now, max_age))
    }

    /// Remove `stream_id`'s cached entry, if any.
    ///
    /// Returns `true` iff an entry was present and removed, `false`
    /// otherwise. This module never invalidates a cache entry on its own
    /// (e.g. because a [`DiscoveryClaim`] lapsed) — see this module's
    /// Provenance note for why claim state and cache state are deliberately
    /// kept independent; a caller that wants that coupling calls this
    /// explicitly. Never panics for any input.
    // fusa:req REQ-DISC-019
    pub fn invalidate(&mut self, stream_id: StreamId) -> bool {
        self.entries.remove(&stream_id).is_some()
    }

    /// Number of entries currently cached, stale or not.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Is the cache empty?
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    const ALL_STATES: [RcServerState; 3] = [
        RcServerState::HwUnconfigured,
        RcServerState::HwConfigured,
        RcServerState::RcpConfigured,
    ];

    fn sample_general_registers() -> GeneralRegisters {
        GeneralRegisters {
            svr_oa_tc18_magic_nr: 0x4F41_5443,
            svr_version: 0x0006_0000,
            svr_vendor_id: 0x0102,
            svr_device_id: 0x0304,
            svr_ep_count: 4,
            ..GeneralRegisters::default()
        }
    }

    // ── Broadcast addressing ─────────────────────────────────────────────

    #[test]
    // fusa:test REQ-DISC-001
    fn is_discovery_broadcast_stream_id_true_only_for_the_sentinel() {
        assert!(is_discovery_broadcast_stream_id(
            DISCOVERY_BROADCAST_STREAM_ID
        ));
        assert!(!is_discovery_broadcast_stream_id(StreamId::new(
            [0x02, 0x11, 0x22, 0x33, 0x44, 0x55],
            1
        )));
        // Same sender_mac as the sentinel but a different unique_id is not
        // the sentinel.
        assert!(!is_discovery_broadcast_stream_id(StreamId::new(
            DISCOVERY_BROADCAST_SENDER_MAC,
            1
        )));
        // All-ones sender_mac, but not the reserved sentinel unique_id.
        assert!(!is_discovery_broadcast_stream_id(StreamId::new(
            DISCOVERY_BROADCAST_SENDER_MAC,
            0xFFFF
        )));
    }

    // ── Discovery request ─────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-DISC-002
    fn build_discovery_request_is_recognized_by_is_discovery_request() {
        for transaction_num in [0u8, 1, 0x42, 0xFF] {
            let request = build_discovery_request(transaction_num);
            assert!(is_discovery_request(&request));
            assert_eq!(request.info.byte_bus_id, EP0_BYTE_BUS_ID);
            assert!(!request.info.op);
            assert_eq!(request.info.transaction_num, transaction_num);
        }
    }

    #[test]
    // fusa:test REQ-DISC-002
    fn build_discovery_request_round_trips_through_acf_abb_encode_decode() {
        let request = build_discovery_request(0x11);
        let frame = crate::acf::encode_acf_abb(&request).unwrap();
        let decoded = crate::acf::decode_acf_abb(&frame).unwrap();
        assert_eq!(decoded, request);
        assert!(is_discovery_request(&decoded));
    }

    #[test]
    // fusa:test REQ-DISC-003
    fn is_discovery_request_rejects_non_ep0_byte_bus_id() {
        let mut request = build_discovery_request(0);
        request.info.byte_bus_id = 1;
        assert!(!is_discovery_request(&request));
    }

    #[test]
    // fusa:test REQ-DISC-003
    fn is_discovery_request_rejects_write_direction() {
        let mut request = build_discovery_request(0);
        request.info.op = true;
        assert!(!is_discovery_request(&request));
    }

    #[test]
    // fusa:test REQ-DISC-003
    fn is_discovery_request_rejects_mismatched_register_address() {
        let mut request = build_discovery_request(0);
        request.payload = 7u16.to_be_bytes().to_vec();
        assert!(!is_discovery_request(&request));
    }

    #[test]
    // fusa:test REQ-DISC-003
    fn is_discovery_request_rejects_short_payload() {
        let mut request = build_discovery_request(0);
        request.payload = vec![0x00];
        assert!(!is_discovery_request(&request));

        request.payload = vec![];
        assert!(!is_discovery_request(&request));
    }

    #[test]
    // fusa:test REQ-DISC-003
    fn is_discovery_request_ignores_trailing_payload_bytes() {
        let mut request = build_discovery_request(0);
        request.payload.extend_from_slice(&[0xAA, 0xBB, 0xCC]);
        assert!(is_discovery_request(&request));
    }

    // ── is_discovery_configure_request ──────────────────────────────────────

    #[test]
    // fusa:test REQ-DISC-021
    fn is_discovery_configure_request_recognizes_the_write_direction_shape() {
        let mut request = build_discovery_request(0);
        request.info.op = true;
        assert!(is_discovery_configure_request(&request));
        // The read-direction shape it was built from is not itself a
        // configure attempt.
        assert!(!is_discovery_configure_request(&build_discovery_request(0)));
    }

    #[test]
    // fusa:test REQ-DISC-021
    fn is_discovery_configure_request_and_is_discovery_request_are_mutually_exclusive() {
        for op in [false, true] {
            let mut request = build_discovery_request(0);
            request.info.op = op;
            assert_ne!(
                is_discovery_request(&request),
                is_discovery_configure_request(&request)
            );
        }
    }

    #[test]
    // fusa:test REQ-DISC-021
    fn is_discovery_configure_request_rejects_non_matching_shapes() {
        let mut request = build_discovery_request(0);
        request.info.op = true;

        // Wrong byte_bus_id.
        let mut wrong_bus = request.clone();
        wrong_bus.info.byte_bus_id = 1;
        assert!(!is_discovery_configure_request(&wrong_bus));

        // Wrong register address.
        let mut wrong_addr = request.clone();
        wrong_addr.payload = 7u16.to_be_bytes().to_vec();
        assert!(!is_discovery_configure_request(&wrong_addr));

        // Short payload.
        request.payload = vec![0x00];
        assert!(!is_discovery_configure_request(&request));
        request.payload = vec![];
        assert!(!is_discovery_configure_request(&request));
    }

    #[test]
    // fusa:test REQ-DISC-021
    fn is_discovery_configure_request_ignores_trailing_payload_bytes() {
        let mut request = build_discovery_request(0);
        request.info.op = true;
        request.payload.extend_from_slice(&[0xAA, 0xBB, 0xCC]);
        assert!(is_discovery_configure_request(&request));
    }

    // ── Discovery response: answerable in any lifecycle state ─────────────

    #[test]
    // fusa:test REQ-DISC-004
    fn discovery_response_succeeds_in_every_lifecycle_state() {
        let general = sample_general_registers();
        let request = build_discovery_request(0x55).info;
        for state in ALL_STATES {
            let response = build_discovery_response(&request, state, &general)
                .unwrap_or_else(|e| panic!("discovery must be answerable in {state:?}: {e:?}"));
            assert_eq!(response.payload, general.encode().to_vec());
            assert_eq!(response.info.byte_bus_id, EP0_BYTE_BUS_ID);
            assert!(response.info.rsp);
        }
    }

    #[test]
    // fusa:test REQ-DISC-004
    fn discovery_response_echoes_request_byte_bus_id_per_echo_back_rule() {
        let general = sample_general_registers();
        let request = ByteMessageInfo {
            byte_bus_id: EP0_BYTE_BUS_ID,
            op: false,
            transaction_num: 0x09,
            ..ByteMessageInfo::default()
        };
        let response =
            build_discovery_response(&request, RcServerState::HwUnconfigured, &general).unwrap();
        assert_eq!(
            crate::acf::verify_echo_back(&request, &response.info),
            Ok(())
        );
    }

    #[test]
    // fusa:test REQ-DISC-004
    fn discovery_response_round_trips_through_acf_abb_encode_decode() {
        let general = sample_general_registers();
        let request = build_discovery_request(0x22).info;
        let response =
            build_discovery_response(&request, RcServerState::RcpConfigured, &general).unwrap();
        let frame = crate::acf::encode_acf_abb(&response).unwrap();
        let decoded = crate::acf::decode_acf_abb(&frame).unwrap();
        assert_eq!(decoded, response);
        let decoded_general = GeneralRegisters::decode(&decoded.payload).unwrap();
        assert_eq!(decoded_general, general);
    }

    // ── Discovery-stream claiming ───────────────────────────────────────────

    fn client_a() -> StreamId {
        StreamId::new([0x02, 0x00, 0x00, 0x00, 0x00, 0x01], 1)
    }

    fn client_b() -> StreamId {
        StreamId::new([0x02, 0x00, 0x00, 0x00, 0x00, 0x02], 7)
    }

    #[test]
    // fusa:test REQ-DISC-007
    fn first_claimant_wins_an_unclaimed_stream() {
        let now = Instant::now();
        let claim = try_claim_discovery_stream(None, client_a(), now, DISCOVERY_TIME_OUT)
            .expect("first claim must succeed");
        assert_eq!(claim.claimant(), client_a());
        assert_eq!(claim.claimed_at(), now);
    }

    #[test]
    // fusa:test REQ-DISC-008
    fn second_different_claimant_is_rejected_while_claim_is_live() {
        let claimed_at = Instant::now();
        let claim =
            try_claim_discovery_stream(None, client_a(), claimed_at, DISCOVERY_TIME_OUT).unwrap();
        let still_live = claimed_at + (DISCOVERY_TIME_OUT / 2);
        let err =
            try_claim_discovery_stream(Some(claim), client_b(), still_live, DISCOVERY_TIME_OUT)
                .unwrap_err();
        assert_eq!(err, RcpError::RequestRejected);
    }

    #[test]
    // fusa:test REQ-DISC-007
    fn same_claimant_may_refresh_its_own_live_claim() {
        let claimed_at = Instant::now();
        let claim =
            try_claim_discovery_stream(None, client_a(), claimed_at, DISCOVERY_TIME_OUT).unwrap();
        let still_live = claimed_at + (DISCOVERY_TIME_OUT / 2);
        let refreshed =
            try_claim_discovery_stream(Some(claim), client_a(), still_live, DISCOVERY_TIME_OUT)
                .expect("same claimant refreshing its own claim must succeed");
        assert_eq!(refreshed.claimant(), client_a());
        assert_eq!(refreshed.claimed_at(), still_live);
    }

    #[test]
    // fusa:test REQ-DISC-006
    // fusa:test REQ-DISC-007
    fn lapsed_claim_reopens_to_a_new_claimant() {
        let claimed_at = Instant::now();
        let claim =
            try_claim_discovery_stream(None, client_a(), claimed_at, DISCOVERY_TIME_OUT).unwrap();
        assert!(!claim.has_lapsed(claimed_at, DISCOVERY_TIME_OUT));

        let after_timeout = claimed_at + DISCOVERY_TIME_OUT;
        assert!(claim.has_lapsed(after_timeout, DISCOVERY_TIME_OUT));

        let reclaimed =
            try_claim_discovery_stream(Some(claim), client_b(), after_timeout, DISCOVERY_TIME_OUT)
                .expect("a lapsed claim must reopen to any claimant");
        assert_eq!(reclaimed.claimant(), client_b());
        assert_eq!(reclaimed.claimed_at(), after_timeout);
    }

    #[test]
    // fusa:test REQ-DISC-006
    fn has_lapsed_boundary_is_inclusive_of_the_exact_timeout() {
        let claimed_at = Instant::now();
        let claim = DiscoveryClaim {
            claimant: client_a(),
            claimed_at,
        };
        let just_before = claimed_at + (DISCOVERY_TIME_OUT - Duration::from_millis(1));
        let exactly_at = claimed_at + DISCOVERY_TIME_OUT;
        assert!(!claim.has_lapsed(just_before, DISCOVERY_TIME_OUT));
        assert!(claim.has_lapsed(exactly_at, DISCOVERY_TIME_OUT));
    }

    #[test]
    // fusa:test REQ-DISC-006
    fn has_lapsed_never_panics_when_now_precedes_claimed_at() {
        let claimed_at = Instant::now();
        let claim = DiscoveryClaim {
            claimant: client_a(),
            claimed_at,
        };
        let earlier = claimed_at.checked_sub(Duration::from_millis(5));
        if let Some(earlier) = earlier {
            assert!(!claim.has_lapsed(earlier, DISCOVERY_TIME_OUT));
        }
    }

    #[test]
    // fusa:test REQ-DISC-009
    fn broadcast_sentinel_is_never_an_eligible_claimant() {
        let now = Instant::now();
        let err = try_claim_discovery_stream(
            None,
            DISCOVERY_BROADCAST_STREAM_ID,
            now,
            DISCOVERY_TIME_OUT,
        )
        .unwrap_err();
        assert_eq!(err, RcpError::InvalidParameter);

        // Also rejected as an attempted claimant against an already-live
        // claim, and against a lapsed one.
        let claim = try_claim_discovery_stream(None, client_a(), now, DISCOVERY_TIME_OUT).unwrap();
        let after_timeout = now + DISCOVERY_TIME_OUT;
        assert_eq!(
            try_claim_discovery_stream(
                Some(claim),
                DISCOVERY_BROADCAST_STREAM_ID,
                after_timeout,
                DISCOVERY_TIME_OUT,
            ),
            Err(RcpError::InvalidParameter)
        );
    }

    // ── Multi-client coexistence: read/configure access-kind distinction ───

    #[test]
    // fusa:test REQ-DISC-011
    // fusa:test REQ-DISC-012
    fn read_access_always_succeeds_and_never_consults_claim_state() {
        let now = Instant::now();
        // Unclaimed.
        assert_eq!(
            check_discovery_access(
                None,
                client_a(),
                DiscoveryAccessKind::Read,
                now,
                DISCOVERY_TIME_OUT
            ),
            Ok(())
        );
        // A live claim held by someone else -- still succeeds for the
        // non-claimant reader.
        let claim = try_claim_discovery_stream(None, client_a(), now, DISCOVERY_TIME_OUT).unwrap();
        assert_eq!(
            check_discovery_access(
                Some(claim),
                client_b(),
                DiscoveryAccessKind::Read,
                now,
                DISCOVERY_TIME_OUT
            ),
            Ok(())
        );
        // Even the broadcast sentinel may read.
        assert_eq!(
            check_discovery_access(
                Some(claim),
                DISCOVERY_BROADCAST_STREAM_ID,
                DiscoveryAccessKind::Read,
                now,
                DISCOVERY_TIME_OUT
            ),
            Ok(())
        );
    }

    #[test]
    // fusa:test REQ-DISC-012
    fn configure_succeeds_on_an_unclaimed_stream() {
        let now = Instant::now();
        assert_eq!(
            check_discovery_access(
                None,
                client_a(),
                DiscoveryAccessKind::Configure,
                now,
                DISCOVERY_TIME_OUT
            ),
            Ok(())
        );
    }

    #[test]
    // fusa:test REQ-DISC-012
    fn configure_succeeds_for_the_live_claimant() {
        let claimed_at = Instant::now();
        let claim =
            try_claim_discovery_stream(None, client_a(), claimed_at, DISCOVERY_TIME_OUT).unwrap();
        let still_live = claimed_at + (DISCOVERY_TIME_OUT / 2);
        assert_eq!(
            check_discovery_access(
                Some(claim),
                client_a(),
                DiscoveryAccessKind::Configure,
                still_live,
                DISCOVERY_TIME_OUT
            ),
            Ok(())
        );
    }

    #[test]
    // fusa:test REQ-DISC-013
    fn configure_rejects_a_different_live_claimant() {
        let claimed_at = Instant::now();
        let claim =
            try_claim_discovery_stream(None, client_a(), claimed_at, DISCOVERY_TIME_OUT).unwrap();
        let still_live = claimed_at + (DISCOVERY_TIME_OUT / 2);
        assert_eq!(
            check_discovery_access(
                Some(claim),
                client_b(),
                DiscoveryAccessKind::Configure,
                still_live,
                DISCOVERY_TIME_OUT
            ),
            Err(RcpError::UnauthorizedAccess)
        );
    }

    #[test]
    // fusa:test REQ-DISC-012
    fn configure_succeeds_for_any_requester_once_the_claim_has_lapsed() {
        let claimed_at = Instant::now();
        let claim =
            try_claim_discovery_stream(None, client_a(), claimed_at, DISCOVERY_TIME_OUT).unwrap();
        let after_timeout = claimed_at + DISCOVERY_TIME_OUT;
        assert_eq!(
            check_discovery_access(
                Some(claim),
                client_b(),
                DiscoveryAccessKind::Configure,
                after_timeout,
                DISCOVERY_TIME_OUT
            ),
            Ok(())
        );
    }

    #[test]
    // fusa:test REQ-DISC-014
    fn configure_rejects_the_broadcast_sentinel_regardless_of_claim_state() {
        let now = Instant::now();
        // Unclaimed.
        assert_eq!(
            check_discovery_access(
                None,
                DISCOVERY_BROADCAST_STREAM_ID,
                DiscoveryAccessKind::Configure,
                now,
                DISCOVERY_TIME_OUT
            ),
            Err(RcpError::InvalidParameter)
        );
        // Live claim held by a real client.
        let claim = try_claim_discovery_stream(None, client_a(), now, DISCOVERY_TIME_OUT).unwrap();
        assert_eq!(
            check_discovery_access(
                Some(claim),
                DISCOVERY_BROADCAST_STREAM_ID,
                DiscoveryAccessKind::Configure,
                now,
                DISCOVERY_TIME_OUT
            ),
            Err(RcpError::InvalidParameter)
        );
        // Lapsed claim.
        let after_timeout = now + DISCOVERY_TIME_OUT;
        assert_eq!(
            check_discovery_access(
                Some(claim),
                DISCOVERY_BROADCAST_STREAM_ID,
                DiscoveryAccessKind::Configure,
                after_timeout,
                DISCOVERY_TIME_OUT
            ),
            Err(RcpError::InvalidParameter)
        );
    }

    #[test]
    // fusa:test REQ-DISC-012
    // fusa:test REQ-DISC-013
    fn check_discovery_access_configure_agrees_with_try_claim_discovery_stream_grant_decision() {
        // check_discovery_access's Configure gate and
        // try_claim_discovery_stream's grant decision share the same
        // claim_permits rule -- demonstrate the two agree across a spread of
        // claim states rather than merely asserting it in a doc comment.
        let base = Instant::now();
        let claimants = [client_a(), client_b()];
        for &existing_claimant in &claimants {
            let claim =
                try_claim_discovery_stream(None, existing_claimant, base, DISCOVERY_TIME_OUT)
                    .unwrap();
            for &requester in &claimants {
                for offset_ms in [
                    0u64,
                    DISCOVERY_TIME_OUT.as_millis() as u64 / 2,
                    DISCOVERY_TIME_OUT.as_millis() as u64,
                ] {
                    let now = base + Duration::from_millis(offset_ms);
                    let access = check_discovery_access(
                        Some(claim),
                        requester,
                        DiscoveryAccessKind::Configure,
                        now,
                        DISCOVERY_TIME_OUT,
                    );
                    let claim_grant =
                        try_claim_discovery_stream(Some(claim), requester, now, DISCOVERY_TIME_OUT);
                    assert_eq!(
                        access.is_ok(),
                        claim_grant.is_ok(),
                        "{existing_claimant:?} {requester:?} {offset_ms}ms"
                    );
                }
            }
        }
    }

    // ── Client-side discovery cache ─────────────────────────────────────────

    fn sample_general_registers_b() -> GeneralRegisters {
        GeneralRegisters {
            svr_oa_tc18_magic_nr: 0x4F41_5443,
            svr_version: 0x0006_0001,
            svr_vendor_id: 0x0506,
            svr_device_id: 0x0708,
            svr_ep_count: 8,
            ..GeneralRegisters::default()
        }
    }

    #[test]
    // fusa:test REQ-DISC-016
    fn discovery_cache_remember_then_lookup_round_trips_the_cache_worthy_subset() {
        let mut cache = DiscoveryCache::new();
        assert!(cache.is_empty());
        let general = sample_general_registers();
        let now = Instant::now();
        cache.remember(client_a(), &general, now).unwrap();
        assert_eq!(cache.len(), 1);
        assert!(!cache.is_empty());

        let entry = cache.lookup(client_a()).expect("entry must be present");
        assert_eq!(entry.svr_oa_tc18_magic_nr(), general.svr_oa_tc18_magic_nr);
        assert_eq!(entry.svr_version(), general.svr_version);
        assert_eq!(entry.svr_vendor_id(), general.svr_vendor_id);
        assert_eq!(entry.svr_device_id(), general.svr_device_id);
        assert_eq!(entry.svr_ep_count(), general.svr_ep_count);
        assert_eq!(entry.cached_at(), now);
        assert!(entry.matches(&general));

        // An unremembered stream_id has no entry.
        assert!(cache.lookup(client_b()).is_none());
    }

    #[test]
    // fusa:test REQ-DISC-016
    fn discovery_cache_remember_overwrites_an_existing_entry_for_the_same_stream_id() {
        let mut cache = DiscoveryCache::new();
        let first = Instant::now();
        cache
            .remember(client_a(), &sample_general_registers(), first)
            .unwrap();
        assert_eq!(cache.len(), 1);

        let second = first + Duration::from_millis(5);
        let updated = sample_general_registers_b();
        cache.remember(client_a(), &updated, second).unwrap();

        // Still exactly one entry -- overwritten, not appended.
        assert_eq!(cache.len(), 1);
        let entry = cache.lookup(client_a()).unwrap();
        assert_eq!(entry.svr_device_id(), updated.svr_device_id);
        assert_eq!(entry.cached_at(), second);
        assert!(entry.matches(&updated));
        assert!(!entry.matches(&sample_general_registers()));
    }

    #[test]
    // fusa:test REQ-DISC-017
    fn discovery_cache_remember_rejects_the_broadcast_sentinel_as_stream_id() {
        let mut cache = DiscoveryCache::new();
        let err = cache
            .remember(
                DISCOVERY_BROADCAST_STREAM_ID,
                &sample_general_registers(),
                Instant::now(),
            )
            .unwrap_err();
        assert_eq!(err, RcpError::InvalidParameter);
        assert!(cache.is_empty());
    }

    #[test]
    // fusa:test REQ-DISC-018
    fn discovery_cache_is_known_reflects_staleness_under_max_age() {
        let mut cache = DiscoveryCache::new();
        let cached_at = Instant::now();
        cache
            .remember(client_a(), &sample_general_registers(), cached_at)
            .unwrap();
        let max_age = Duration::from_millis(30);

        // Not yet stale.
        assert!(cache.is_known(client_a(), cached_at, max_age));
        assert!(cache.is_known(client_a(), cached_at + (max_age / 2), max_age));

        // Stale once max_age has fully elapsed (inclusive boundary).
        assert!(!cache.is_known(client_a(), cached_at + max_age, max_age));

        // An unknown stream_id is never "known".
        assert!(!cache.is_known(client_b(), cached_at, max_age));
    }

    #[test]
    // fusa:test REQ-DISC-018
    fn discovery_cache_entry_is_stale_boundary_is_inclusive_and_never_panics_on_out_of_order_now() {
        let cached_at = Instant::now();
        let general = sample_general_registers();
        let entry = DiscoveryCacheEntry::from_general_registers(&general, cached_at);
        let max_age = Duration::from_millis(20);

        let just_before = cached_at + (max_age - Duration::from_millis(1));
        let exactly_at = cached_at + max_age;
        assert!(!entry.is_stale(just_before, max_age));
        assert!(entry.is_stale(exactly_at, max_age));

        if let Some(earlier) = cached_at.checked_sub(Duration::from_millis(5)) {
            assert!(!entry.is_stale(earlier, max_age));
        }
    }

    #[test]
    // fusa:test REQ-DISC-019
    fn discovery_cache_entry_matches_detects_an_identity_change() {
        let general = sample_general_registers();
        let entry = DiscoveryCacheEntry::from_general_registers(&general, Instant::now());
        assert!(entry.matches(&general));

        let mut changed = general;
        changed.svr_device_id = general.svr_device_id.wrapping_add(1);
        assert!(!entry.matches(&changed));
    }

    #[test]
    // fusa:test REQ-DISC-019
    fn discovery_cache_invalidate_removes_the_entry_and_is_idempotent() {
        let mut cache = DiscoveryCache::new();
        cache
            .remember(client_a(), &sample_general_registers(), Instant::now())
            .unwrap();
        assert!(cache.invalidate(client_a()));
        assert!(cache.lookup(client_a()).is_none());
        assert!(cache.is_empty());

        // Invalidating an already-absent entry reports false, not an error.
        assert!(!cache.invalidate(client_a()));
    }

    #[test]
    // fusa:test REQ-DISC-016
    fn discovery_cache_holds_independent_entries_across_multiple_stream_ids() {
        let mut cache = DiscoveryCache::new();
        let now = Instant::now();
        cache
            .remember(client_a(), &sample_general_registers(), now)
            .unwrap();
        cache
            .remember(client_b(), &sample_general_registers_b(), now)
            .unwrap();
        assert_eq!(cache.len(), 2);
        assert_eq!(
            cache.lookup(client_a()).unwrap().svr_device_id(),
            sample_general_registers().svr_device_id
        );
        assert_eq!(
            cache.lookup(client_b()).unwrap().svr_device_id(),
            sample_general_registers_b().svr_device_id
        );

        cache.invalidate(client_a());
        assert_eq!(cache.len(), 1);
        assert!(cache.lookup(client_a()).is_none());
        assert!(cache.lookup(client_b()).is_some());
    }

    // ── Never panics ────────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-DISC-010
    fn try_claim_discovery_stream_never_panics_across_sampled_inputs() {
        let mut state: u32 = 0xC1A1_0DEC;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        let base = Instant::now();
        let claimants = [client_a(), client_b(), DISCOVERY_BROADCAST_STREAM_ID];
        for &claimant in &claimants {
            for existing in [None, Some(client_a()), Some(client_b())] {
                let current = existing.map(|c| DiscoveryClaim {
                    claimant: c,
                    claimed_at: base,
                });
                let offset_ms = (next() % 50) as u64;
                let now = base + Duration::from_millis(offset_ms);
                let timeout_ms = 1 + (next() % 40) as u64;
                let _ = try_claim_discovery_stream(
                    current,
                    claimant,
                    now,
                    Duration::from_millis(timeout_ms),
                );
            }
        }
    }

    #[test]
    // fusa:test REQ-DISC-015
    fn check_discovery_access_never_panics_across_sampled_inputs() {
        let mut state: u32 = 0xACCE_5501;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        let base = Instant::now();
        let requesters = [client_a(), client_b(), DISCOVERY_BROADCAST_STREAM_ID];
        let kinds = [DiscoveryAccessKind::Read, DiscoveryAccessKind::Configure];
        for &requester in &requesters {
            for existing in [None, Some(client_a()), Some(client_b())] {
                let current = existing.map(|c| DiscoveryClaim {
                    claimant: c,
                    claimed_at: base,
                });
                for &kind in &kinds {
                    let offset_ms = (next() % 50) as u64;
                    let now = base + Duration::from_millis(offset_ms);
                    let timeout_ms = 1 + (next() % 40) as u64;
                    let _ = check_discovery_access(
                        current,
                        requester,
                        kind,
                        now,
                        Duration::from_millis(timeout_ms),
                    );
                }
            }
        }
    }

    #[test]
    // fusa:test REQ-DISC-005
    fn is_discovery_request_never_panics_on_arbitrary_payloads() {
        let mut state: u32 = 0xD15C_0BE1;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for byte_bus_id in [0u16, 1, EP0_BYTE_BUS_ID, 0x07FF] {
            for op in [false, true] {
                for payload_len in 0..8usize {
                    let payload: Vec<u8> =
                        (0..payload_len).map(|_| (next() & 0xFF) as u8).collect();
                    let msg = AcfAbbMessage {
                        info: ByteMessageInfo {
                            byte_bus_id,
                            op,
                            ..ByteMessageInfo::default()
                        },
                        payload,
                    };
                    let _ = is_discovery_request(&msg);
                }
            }
        }
    }

    #[test]
    // fusa:test REQ-DISC-005
    fn build_discovery_response_never_panics_for_any_state() {
        let general = sample_general_registers();
        for state in ALL_STATES {
            let request = build_discovery_request(0).info;
            let _ = build_discovery_response(&request, state, &general);
        }
    }

    #[test]
    // fusa:test REQ-DISC-005
    fn is_discovery_broadcast_stream_id_never_panics_across_sampled_values() {
        let mut state: u32 = 0xB0AD_CA57;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for _ in 0..32 {
            let mac = [
                (next() & 0xFF) as u8,
                (next() & 0xFF) as u8,
                (next() & 0xFF) as u8,
                (next() & 0xFF) as u8,
                (next() & 0xFF) as u8,
                (next() & 0xFF) as u8,
            ];
            let unique_id = (next() & 0xFFFF) as u16;
            let _ = is_discovery_broadcast_stream_id(StreamId::new(mac, unique_id));
        }
    }

    #[test]
    // fusa:test REQ-DISC-020
    fn discovery_cache_operations_never_panic_across_sampled_inputs() {
        let mut state: u32 = 0xCAC4_E5EE;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        let base = Instant::now();
        let stream_ids = [client_a(), client_b(), DISCOVERY_BROADCAST_STREAM_ID];
        let generals = [sample_general_registers(), sample_general_registers_b()];
        let mut cache = DiscoveryCache::new();
        for &stream_id in &stream_ids {
            for general in &generals {
                let offset_ms = (next() % 50) as u64;
                let now = base + Duration::from_millis(offset_ms);
                let _ = cache.remember(stream_id, general, now);

                let query_offset_ms = (next() % 50) as u64;
                let query_now = base + Duration::from_millis(query_offset_ms);
                let max_age_ms = 1 + (next() % 40) as u64;
                let _ = cache.is_known(stream_id, query_now, Duration::from_millis(max_age_ms));
                let _ = cache.lookup(stream_id);
                if let Some(entry) = cache.lookup(stream_id) {
                    let _ = entry.is_stale(query_now, Duration::from_millis(max_age_ms));
                    let _ = entry.matches(general);
                }
                let _ = cache.invalidate(stream_id);
            }
        }
    }
}
