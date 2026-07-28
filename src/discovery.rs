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

//! Discovery request/response and discovery-stream claiming — TC18
//! register-map model (`ROADMAP.md` Milestone 3 "Discovery" subsection,
//! first two checklist bullets: "Discovery request/response" and
//! "Discovery-stream claiming: first-claimant rule, `Discovery_TimeOut`
//! (~20 ms default) lapse-and-reopen behavior").
//!
//! This module begins Milestone 3, which per the subsection's own Goal text
//! replaces [`crate::mdns`] as the *mandatory* discovery path (mDNS may
//! continue to exist as a complementary network-rendezvous helper, per the
//! satellite disposition table, but is not a substitute for this). Nothing
//! here reuses or extends [`crate::mdns`]'s `Zone`/host/port/txt-record
//! model — that is a different, private-protocol concept with nothing in
//! common with the TC18 broadcast-`ACF_ABB` mechanism modeled below.
//!
//! Only this subsection's first two checklist bullets are in scope.
//! Deliberately out of scope, as separate later checklist bullets in the
//! same subsection:
//!
//! - Multi-client coexistence once a stream is claimed (other clients may
//!   still read via discovery; only the claimant may configure) — needs a
//!   read/configure access-kind distinction this item does not introduce.
//! - The client-side discovery cache.
//! - Wiring any of the below into an actual decoder, dispatch loop, or
//!   [`crate::avtpdu`]/[`crate::acf`] caller — this module remains additive
//!   standalone plumbing only, matching the discipline every prior
//!   Milestone 1/2 entry already established. In particular,
//!   [`DiscoveryClaim`]/[`try_claim_discovery_stream`] model claim state as
//!   plain data a caller owns and threads through explicitly (mirroring how
//!   [`build_discovery_response`] takes `state`/`general` as caller-supplied
//!   values rather than owning them) — nothing here spawns a timer thread,
//!   holds a lock, or reads the real clock itself.
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
//! - [`crate::register_map::GeneralRegisters`] — register address 0's
//!   field-level content (`svr_oa_tc18_magic_nr` and the rest of `§3.6`'s
//!   table), reused as the discovery response payload's content rather than
//!   inventing a new addressing scheme.
//!
//! ## Provenance note
//!
//! Four working interpretations this item introduces, per Guiding Principle
//! 5, are flagged here for reconciliation against the OPEN Alliance TC18
//! Remote Control Protocol Specification v0.5.1_RC's actual behavior
//! (never its prose) before being relied on for interop with a real TC18 RC
//! Server:
//!
//! - **Broadcast addressing.** `ROADMAP.md`'s own checklist wording states
//!   that the discovery request must be "broadcastable" but does not name a
//!   wire-level broadcast address convention, and
//!   [`crate::avtpdu::StreamId`] has no broadcast/multicast concept of its
//!   own (it is built as `sender_mac || unique_id`, always identifying one
//!   specific sender). Rather than invent an out-of-band "this AVTPDU is a
//!   broadcast" flag with no roadmap-named field to carry it, this module
//!   reuses the reserved IEEE 802.3 all-ones Ethernet broadcast MAC address
//!   (`FF:FF:FF:FF:FF:FF`) as `sender_mac`, paired with `unique_id == 0`,
//!   as a single well-known sentinel [`crate::avtpdu::StreamId`] value: see
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
//!   existing big-endian convention — see [`crate::register_map`]'s own
//!   `encode`/`decode` methods). This mirrors how a discovery *response*
//!   already has to carry [`crate::register_map::GeneralRegisters::encode`]'s
//!   fixed-length block somewhere, and payload is the only carrier this
//!   crate's Milestone 1 framing offers either direction.
//! - **Claim identity.** The roadmap names a "first-claimant rule" but does
//!   not say what identifies a claimant. Rather than invent a new identity
//!   type, this module reuses [`crate::avtpdu::StreamId`] — the same type
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

use std::time::{Duration, Instant};

use crate::acf::{AcfAbbMessage, ByteMessageInfo};
use crate::avtpdu::StreamId;
use crate::ep0::{access_kind, route_byte_bus_id, Ep0AccessKind, RequestRoute, EP0_BYTE_BUS_ID};
use crate::lifecycle::{check_register_reachable, RcServerState, RegisterCategory};
use crate::register_map::GeneralRegisters;
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
/// the start of [`crate::register_map::GeneralRegisters`]'s block (which
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
/// full content, per [`crate::register_map::GeneralRegisters`]), and its
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
    let granted = match current {
        None => true,
        Some(existing) => existing.claimant == claimant || existing.has_lapsed(now, timeout),
    };

    if granted {
        Ok(DiscoveryClaim {
            claimant,
            claimed_at: now,
        })
    } else {
        // fusa:req REQ-DISC-008
        Err(RcpError::RequestRejected)
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
}
