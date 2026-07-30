// fusa:req REQ-MOCKSRV-001
// fusa:req REQ-MOCKSRV-002
// fusa:req REQ-MOCKSRV-003
// fusa:req REQ-MOCKSRV-004
// fusa:req REQ-MOCKSRV-005
// fusa:req REQ-MOCKSRV-006
// fusa:req REQ-MOCKSRV-007
// fusa:req REQ-MOCKSRV-008
// fusa:req REQ-MOCKSRV-009
// fusa:req REQ-MOCKSRV-010

//! In-process test double for this crate's OPEN Alliance TC18 Remote
//! Control Protocol Specification v0.5.1_RC RC Server model.
//!
//! `ROADMAP.md` Milestone 9's Satellite Package Disposition table called
//! `mock` a **REPLACE** package: it "must model an RC Server + Endpoints
//! for testing, not a `Zone`-keyed controller." [`RcServer`]/[`Endpoint`]/
//! [`MockEndpoint`], added by that item, are that replacement — an
//! in-memory RC Server, keyed by `(`[`crate::avtp::StreamId`]`,
//! byte_bus_id)` and gated by [`crate::lifecycle::RcServerState`].
//! [`RcServer::handle_ntscf_frame`] answers a whole on-wire request by
//! composing this crate's Milestone 1-3 primitives —
//! [`crate::avtp::decode_ntscf_frame`]/[`crate::avtp::encode_ntscf_frame`],
//! [`crate::acf::decode_acf_abb`]/[`crate::acf::encode_acf_abb`],
//! [`crate::acf::build_response_info`]/[`crate::acf::verify_echo_back`],
//! [`crate::ep0::route_byte_bus_id`]/[`crate::ep0::check_ep0_access_for_stream`],
//! and [`crate::addressing::EndpointTable`] — into one live decode ->
//! route -> dispatch -> encode path.
//!
//! This module previously also carried a `MockController`/`MockRegistry`/
//! `Handler` test double for this crate's pre-Milestone-10 `Zone`-keyed
//! `Controller`/`Registry` API, kept alongside the RC Server model above
//! while other satellite packages still depended on it. `ROADMAP.md`
//! Milestone 10's core-surface cutover has since removed that whole API
//! from [`crate`] with no compatibility shim (per Guiding Principle 5,
//! recorded here rather than silently done), and this module now models
//! only the RC Server: `MockController`/`MockRegistry`/`Handler` are
//! deleted outright, along with their own `.fusa-reqs.json`
//! `REQ-CTRL-*`/`REQ-REG-*`/`REQ-RESP-*`/`REQ-STAT-*` entries, which
//! described only that removed code and traced to nothing else.
//!
//! All operations in this module execute synchronously in memory with no
//! network I/O, and are safe for concurrent use.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use crate::RcpError;

// ─────────────────────────────────────────────────────────────────────────────
//  RC Server + Endpoints test double (ROADMAP.md Milestone 9, `mock` REPLACE)
// ─────────────────────────────────────────────────────────────────────────────
//
// See this module's doc comment for the full scope note. In short: this
// section models an in-memory OPEN Alliance TC18 RC Server good enough to
// drive a request through the whole decode -> route -> dispatch -> encode
// path this crate did not previously assemble anywhere, addressed by
// `(StreamId, byte_bus_id)` and gated by `RcServerState` rather than by
// `Zone`.

use crate::acf::{
    build_response_info, decode_acf_abb, encode_acf_abb, verify_echo_back, AcfAbbMessage,
};
use crate::addressing::{EndpointId, EndpointTable};
use crate::avtp::{decode_ntscf_frame, encode_ntscf_frame, StreamId};
use crate::ep0::{check_ep0_access_for_stream, route_byte_bus_id, RequestRoute};
use crate::lifecycle::{RcServerState, RegisterCategory};
use crate::regmap::{EndpointType, GeneralRegisters};

// ── Endpoint abstraction ──────────────────────────────────────────────────────

/// The minimal per-endpoint behavior [`RcServer`] dispatches a
/// device-endpoint-addressed request to, once [`crate::ep0::route_byte_bus_id`]
/// has decided the request is not EP0-addressed.
///
/// Concrete, device-facing endpoint types (`crate::gpio`, `crate::can`,
/// etc.) are each their own additive, standalone set of pure functions over
/// their own wire shapes today (per every one of Milestone 4/7's own "Done"
/// notes) — none of them implement this trait yet, and wiring any one of
/// them onto it is out of scope for this item; it belongs to whichever
/// later milestone item first needs a live endpoint dispatched through an
/// RC Server. `canbr`'s own REPLACE rebuild (Milestone 9) has since
/// completed without wiring `crate::can` onto this trait either, for the
/// same reason — that rebuild's scope was the legacy `CanBridge`/
/// `CanSocket` cutover itself, not new dispatch plumbing; `linbr`'s own
/// REPLACE rebuild has since completed the same way. `udp`'s own
/// still-open deeper rebuild, anticipated here as "the most likely next
/// caller," has *also* since completed: [`crate::udp::UdpRcServer`] composes
/// [`RcServer`] directly (see that type's own doc comment for the
/// "test double called from non-test code" judgment call this crate made
/// rather than duplicating `handle_abb`'s dispatch logic) — but it still
/// dispatches through this same [`Endpoint`] trait rather than wiring any
/// concrete endpoint type onto it, so the gap this paragraph describes
/// remains open for whichever later item first needs one.
/// [`MockEndpoint`] is
/// this item's only implementation, standing
/// in for a concrete endpoint the same way [`crate::addressing::EndpointId`]
/// itself stands in for one.
///
/// Takes `&self` rather than `&mut self` so an implementation can be shared
/// behind `Arc<dyn Endpoint>` inside [`RcServer`]'s endpoint map — the same
/// shared-behind-`Arc`, interior-mutability shape [`MockEndpoint`] itself
/// uses for its own buffer. Never required to panic for any input;
/// [`MockEndpoint`]'s own impl never does.
pub trait Endpoint: Send + Sync {
    /// This endpoint's register-map type discriminant.
    fn ep_type(&self) -> EndpointType;

    /// Answer a read addressed to this endpoint.
    ///
    /// `read_size` is the request's raw
    /// [`crate::acf::ReadSizeOrSegment::as_read_size`] value. This trait
    /// does not itself prescribe what an implementation does with it —
    /// [`MockEndpoint::read`] treats it as a requested byte count, capped
    /// to however much data is actually held.
    fn read(&self, read_size: u16) -> Result<Vec<u8>, RcpError>;

    /// Apply a write addressed to this endpoint, given the request's raw
    /// payload bytes.
    fn write(&self, payload: &[u8]) -> Result<(), RcpError>;
}

/// A trivial byte-buffer-backed [`Endpoint`] test double.
///
/// [`MockEndpoint::write`] replaces the held buffer wholesale with
/// `payload`; [`MockEndpoint::read`] returns up to `read_size` bytes from
/// the front of whatever is currently held, or the whole buffer if
/// `read_size` (as an unsigned byte count) is not smaller than it — this
/// crate's own simplification, not a transcription of any real per-endpoint
/// read-chunking rule (none of the concrete endpoint-type modules define
/// one uniformly; that is separate, later work, same as
/// [`crate::fragment`]'s own AVTPDU-size chunking already flags for
/// responses).
pub struct MockEndpoint {
    ep_type: EndpointType,
    buf: Mutex<Vec<u8>>,
}

impl MockEndpoint {
    /// Construct a mock endpoint of type `ep_type`, initially holding
    /// `initial` as its buffer.
    pub fn new(ep_type: EndpointType, initial: Vec<u8>) -> Arc<Self> {
        Arc::new(Self {
            ep_type,
            buf: Mutex::new(initial),
        })
    }
}

impl Endpoint for MockEndpoint {
    fn ep_type(&self) -> EndpointType {
        self.ep_type
    }

    // fusa:req REQ-MOCKSRV-010
    fn read(&self, read_size: u16) -> Result<Vec<u8>, RcpError> {
        let buf = self.buf.lock().unwrap();
        let n = (read_size as usize).min(buf.len());
        Ok(buf[..n].to_vec())
    }

    // fusa:req REQ-MOCKSRV-009
    fn write(&self, payload: &[u8]) -> Result<(), RcpError> {
        *self.buf.lock().unwrap() = payload.to_vec();
        Ok(())
    }
}

// ── RcServer ──────────────────────────────────────────────────────────────────

/// An in-memory OPEN Alliance TC18 RC Server test double.
///
/// Holds exactly the state this item's design calls for: a
/// [`RcServerState`] lifecycle position, a [`GeneralRegisters`] snapshot
/// (the only [`RegisterCategory::General`] register block this crate has
/// concretely defined so far — `HwConfig`/`RcpConfig` register I/O through
/// EP0 is not modeled by this item; see [`Self::handle_abb`]'s doc comment),
/// an optional root-client [`StreamId`] gating EP0 writes, and an
/// [`EndpointTable`] of registered device endpoints alongside their
/// [`Endpoint`] implementations.
///
/// Deliberately does not model this crate's pre-Milestone-10
/// `MockController`'s publish/subscribe `Status` broadcast (removed
/// outright by Milestone 10's core-surface cutover): this crate's new core
/// has no live asynchronous-notification mechanism yet (no TC18 analog has
/// been identified for it in this crate to date), so replicating that shape
/// here would invent behavior rather than model something real. Should a
/// real notification mechanism land in a later milestone, extending this
/// type to test-double it is that milestone's job, not this one's.
pub struct RcServer {
    state: Mutex<RcServerState>,
    general: Mutex<GeneralRegisters>,
    root_client: Mutex<Option<StreamId>>,
    endpoints: Mutex<EndpointTable>,
    endpoint_impls: Mutex<HashMap<EndpointId, Arc<dyn Endpoint>>>,
    next_endpoint_id: AtomicU32,
    /// Free-running NTSCF `sequence_num` counter shared across every
    /// stream this server answers. A per-stream counter would be more
    /// faithful, but this crate has not built a per-stream sequencer
    /// registry for responses yet; a single shared counter is this test
    /// double's own simplification, not a spec requirement.
    sequence_num: AtomicU32,
}

impl RcServer {
    /// Construct a fresh RC Server, starting at [`RcServerState::INITIAL`]
    /// with no root client and no registered endpoints, holding `general`
    /// as its initial [`GeneralRegisters`] snapshot.
    // fusa:req REQ-MOCKSRV-001
    pub fn new(general: GeneralRegisters) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(RcServerState::INITIAL),
            general: Mutex::new(general),
            root_client: Mutex::new(None),
            endpoints: Mutex::new(EndpointTable::new()),
            endpoint_impls: Mutex::new(HashMap::new()),
            next_endpoint_id: AtomicU32::new(0),
            sequence_num: AtomicU32::new(0),
        })
    }

    /// This server's current lifecycle state.
    // fusa:req REQ-MOCKSRV-002
    pub fn state(&self) -> RcServerState {
        *self.state.lock().unwrap()
    }

    /// A snapshot of this server's current [`GeneralRegisters`].
    pub fn general_registers(&self) -> GeneralRegisters {
        *self.general.lock().unwrap()
    }

    /// Attempt to move this server from its current state to `target`,
    /// delegating to [`RcServerState::try_transition`]. On success, this
    /// server's stored state is updated to `target`; on failure, it is left
    /// unchanged.
    // fusa:req REQ-MOCKSRV-002
    pub fn try_transition(
        &self,
        target: RcServerState,
        is_consistent: impl FnOnce() -> bool,
    ) -> Result<(), RcpError> {
        let mut state = self.state.lock().unwrap();
        let new_state = state.try_transition(target, is_consistent)?;
        *state = new_state;
        Ok(())
    }

    /// Designate `stream` (or nobody, if `None`) as this server's root
    /// client — the one stream permitted to write EP0, per
    /// [`crate::ep0::check_ep0_access_for_stream`].
    pub fn set_root_client(&self, stream: Option<StreamId>) {
        *self.root_client.lock().unwrap() = stream;
    }

    /// Register `endpoint` under `(stream_id, byte_bus_id)` and return the
    /// fresh [`EndpointId`] handle it was assigned.
    ///
    /// Returns whatever error [`EndpointTable::register`] returns —
    /// `Err(RcpError::InvalidSize)` for an oversized `byte_bus_id`, or
    /// `Err(RcpError::EpError)` for an already-registered pair — without
    /// allocating an endpoint id or storing `endpoint` in either case.
    // fusa:req REQ-MOCKSRV-003
    pub fn register_endpoint(
        &self,
        stream_id: StreamId,
        byte_bus_id: u16,
        endpoint: Arc<dyn Endpoint>,
    ) -> Result<EndpointId, RcpError> {
        let id = EndpointId(self.next_endpoint_id.load(Ordering::SeqCst));
        self.endpoints
            .lock()
            .unwrap()
            .register(stream_id, byte_bus_id, id)?;
        self.next_endpoint_id.fetch_add(1, Ordering::SeqCst);
        self.endpoint_impls.lock().unwrap().insert(id, endpoint);
        Ok(id)
    }

    /// Answer one already-decoded [`AcfAbbMessage`] request from
    /// `stream_id`, returning the response [`AcfAbbMessage`] to send back.
    ///
    /// Routing follows [`crate::ep0::route_byte_bus_id`]:
    ///
    /// - [`RequestRoute::Ep0`]: gated by
    ///   [`check_ep0_access_for_stream`] against
    ///   [`RegisterCategory::General`] only — this test double models no
    ///   other register category's storage, so an EP0 access this crate
    ///   would otherwise route to `HwConfig`/`RcpConfig` is answered as
    ///   `General` too rather than rejected outright, a scope-narrowing
    ///   simplification flagged here rather than silently assumed. A read
    ///   returns the current [`GeneralRegisters::encode`] snapshot
    ///   verbatim, for either the root client or any other stream (reads
    ///   are never root-client-gated, per [`check_ep0_access_for_stream`]'s
    ///   own doc comment). A write requires the payload to decode as a
    ///   complete [`GeneralRegisters`] block (`Err(RcpError::ShortFrame)`
    ///   otherwise) and would replace the snapshot wholesale — but
    ///   [`crate::lifecycle::lock_policy`] assigns
    ///   [`RegisterCategory::General`] no [`crate::lifecycle::LockPolicy`]
    ///   at all, which [`crate::lifecycle::is_register_writable`]'s own doc
    ///   comment states means "never writable regardless of lifecycle
    ///   state." This test double does not special-case that: an EP0 write
    ///   is therefore always rejected with `Err(RcpError::LockedMemAccess)`
    ///   once past the root-client check, for the root client exactly as
    ///   for anyone else, and the snapshot is never actually replaced by
    ///   this path today — an honest consequence of modeling only
    ///   `General`, not a bug, and not this item's to work around by
    ///   inventing a writable category this crate has not built storage
    ///   for.
    /// - [`RequestRoute::DeviceEndpoint`]: resolved through
    ///   [`EndpointTable::lookup`], `Err(RcpError::EpNotFound)` if nothing
    ///   is registered under the pair, otherwise dispatched to the
    ///   registered [`Endpoint::read`]/[`Endpoint::write`].
    ///
    /// Every response this function builds echoes `request.info.byte_bus_id`
    /// via [`build_response_info`], and is checked against
    /// [`verify_echo_back`] before being returned (never observably fails,
    /// since `build_response_info` always sets the field it echoes — this
    /// call exists so a future change to either function is caught by this
    /// module's own tests rather than by a caller).
    // fusa:req REQ-MOCKSRV-004
    // fusa:req REQ-MOCKSRV-005
    // fusa:req REQ-MOCKSRV-006
    // fusa:req REQ-MOCKSRV-007
    pub fn handle_abb(
        &self,
        stream_id: StreamId,
        request: &AcfAbbMessage,
    ) -> Result<AcfAbbMessage, RcpError> {
        let response_payload = match route_byte_bus_id(request.info.byte_bus_id) {
            RequestRoute::Ep0 => {
                let state = self.state();
                let root_client = *self.root_client.lock().unwrap();
                check_ep0_access_for_stream(
                    state,
                    RegisterCategory::General,
                    &request.info,
                    stream_id,
                    root_client,
                )?;
                if request.info.op {
                    let decoded = GeneralRegisters::decode(&request.payload)?;
                    *self.general.lock().unwrap() = decoded;
                    Vec::new()
                } else {
                    self.general.lock().unwrap().encode().to_vec()
                }
            }
            RequestRoute::DeviceEndpoint => {
                let endpoint_id = self
                    .endpoints
                    .lock()
                    .unwrap()
                    .lookup(stream_id, request.info.byte_bus_id)
                    .ok_or(RcpError::EpNotFound)?;
                let endpoint = self
                    .endpoint_impls
                    .lock()
                    .unwrap()
                    .get(&endpoint_id)
                    .cloned()
                    .ok_or(RcpError::EpNotFound)?;
                if request.info.op {
                    endpoint.write(&request.payload)?;
                    Vec::new()
                } else {
                    endpoint.read(request.info.read_size_segment.as_read_size())?
                }
            }
        };

        let response_info = build_response_info(&request.info, request.info);
        verify_echo_back(&request.info, &response_info)?;
        Ok(AcfAbbMessage {
            info: response_info,
            payload: response_payload,
        })
    }

    /// Answer one whole on-wire NTSCF-framed ACF_ABB request, given
    /// `stream_id` and the raw AVTPDU bytes a transport received.
    ///
    /// Composes [`decode_ntscf_frame`] -> [`decode_acf_abb`] ->
    /// [`Self::handle_abb`] -> [`encode_acf_abb`] -> [`encode_ntscf_frame`]
    /// so a caller never has to touch any intermediate decoded type — the
    /// same reuse of Milestone 1's already-built AVTPDU/ACF stack the
    /// `wire` REPLACE cutover established for [`crate::udp::UdpTransport`].
    /// The response frame's `sequence_num` is this server's own
    /// free-running counter (see [`Self`]'s doc comment), unrelated to the
    /// request frame's.
    // fusa:req REQ-MOCKSRV-008
    pub fn handle_ntscf_frame(
        &self,
        stream_id: StreamId,
        frame: &[u8],
    ) -> Result<Vec<u8>, RcpError> {
        let (_hdr, acf_bytes) = decode_ntscf_frame(frame)?;
        let request = decode_acf_abb(acf_bytes)?;
        let response = self.handle_abb(stream_id, &request)?;
        let response_bytes = encode_acf_abb(&response)?;
        let seq = self.sequence_num.fetch_add(1, Ordering::SeqCst) as u8;
        encode_ntscf_frame(stream_id, seq, &response_bytes)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod rc_server_tests {
    use super::*;
    use crate::acf::{ByteMessageInfo, Evt, ReadSizeOrSegment};
    use crate::ep0::EP0_BYTE_BUS_ID;

    fn stream(unique_id: u16) -> StreamId {
        StreamId::new([0x02, 0x11, 0x22, 0x33, 0x44, 0x55], unique_id)
    }

    fn abb_request(byte_bus_id: u16, op: bool, payload: Vec<u8>) -> AcfAbbMessage {
        AcfAbbMessage {
            info: ByteMessageInfo {
                byte_bus_id,
                op,
                evt: Evt::default(),
                read_size_segment: ReadSizeOrSegment(payload.len() as u16),
                ..Default::default()
            },
            payload,
        }
    }

    // ── Lifecycle ─────────────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-MOCKSRV-001
    fn new_server_starts_hw_unconfigured_with_no_root_client() {
        let srv = RcServer::new(GeneralRegisters::default());
        assert_eq!(srv.state(), RcServerState::HwUnconfigured);
    }

    #[test]
    // fusa:test REQ-MOCKSRV-002
    fn try_transition_updates_state_on_success_and_leaves_it_on_failure() {
        let srv = RcServer::new(GeneralRegisters::default());
        srv.try_transition(RcServerState::HwConfigured, || true)
            .unwrap();
        assert_eq!(srv.state(), RcServerState::HwConfigured);

        let err = srv
            .try_transition(RcServerState::RcpConfigured, || false)
            .unwrap_err();
        assert_eq!(err, RcpError::InvalidParameter);
        assert_eq!(srv.state(), RcServerState::HwConfigured);
    }

    // ── Endpoint registration ─────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-MOCKSRV-003
    fn register_endpoint_assigns_unique_ids_and_rejects_duplicate_pair() {
        let srv = RcServer::new(GeneralRegisters::default());
        let sid = stream(1);
        let ep_a = MockEndpoint::new(EndpointType::Gpio, vec![0; 4]);
        let ep_b = MockEndpoint::new(EndpointType::Gpio, vec![0; 4]);

        let id_a = srv.register_endpoint(sid, 7, ep_a).unwrap();
        let id_b = srv.register_endpoint(sid, 8, ep_b.clone()).unwrap();
        assert_ne!(id_a, id_b);

        let err = srv.register_endpoint(sid, 8, ep_b).unwrap_err();
        assert_eq!(err, RcpError::EpError);
    }

    // ── EP0 dispatch ──────────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-MOCKSRV-004
    // fusa:test REQ-MOCKSRV-005
    fn ep0_read_returns_general_registers_snapshot() {
        let regs = GeneralRegisters {
            svr_vendor_id: 0x1234,
            ..Default::default()
        };
        let srv = RcServer::new(regs);
        let sid = stream(1);

        let req = abb_request(EP0_BYTE_BUS_ID, false, Vec::new());
        let resp = srv.handle_abb(sid, &req).unwrap();
        assert_eq!(resp.payload, regs.encode().to_vec());
    }

    #[test]
    // fusa:test REQ-MOCKSRV-004
    // fusa:test REQ-MOCKSRV-005
    fn ep0_write_is_locked_even_for_the_root_client() {
        // RegisterCategory::General has no LockPolicy at all
        // (crate::lifecycle::lock_policy), meaning "never writable
        // regardless of lifecycle state" per is_register_writable's own
        // doc comment — this holds even for the designated root client,
        // who is otherwise the only stream ever permitted to write EP0 at
        // all. See handle_abb's own doc comment for why this test double
        // does not work around that.
        let srv = RcServer::new(GeneralRegisters::default());
        let sid = stream(1);
        srv.set_root_client(Some(sid));

        let new_regs = GeneralRegisters {
            svr_vendor_id: 0xBEEF,
            ..Default::default()
        };
        let req = abb_request(EP0_BYTE_BUS_ID, true, new_regs.encode().to_vec());
        let err = srv.handle_abb(sid, &req).unwrap_err();
        assert_eq!(err, RcpError::LockedMemAccess);

        // The snapshot must be unchanged.
        assert_eq!(
            srv.general_registers().svr_vendor_id,
            GeneralRegisters::default().svr_vendor_id
        );
    }

    #[test]
    // fusa:test REQ-MOCKSRV-004
    fn ep0_write_from_non_root_client_is_rejected() {
        let srv = RcServer::new(GeneralRegisters::default());
        let root = stream(1);
        let other = stream(2);
        srv.set_root_client(Some(root));

        let req = abb_request(
            EP0_BYTE_BUS_ID,
            true,
            GeneralRegisters::default().encode().to_vec(),
        );
        let err = srv.handle_abb(other, &req).unwrap_err();
        assert_eq!(err, RcpError::UnauthorizedAccess);
        // The snapshot must be unchanged.
        assert_eq!(
            srv.general_registers().svr_vendor_id,
            GeneralRegisters::default().svr_vendor_id
        );
    }

    #[test]
    // fusa:test REQ-MOCKSRV-004
    fn ep0_read_is_reachable_in_every_lifecycle_state() {
        let srv = RcServer::new(GeneralRegisters::default());
        let sid = stream(1);
        let req = abb_request(EP0_BYTE_BUS_ID, false, Vec::new());

        // HW_UNCONFIGURED (initial).
        assert!(srv.handle_abb(sid, &req).is_ok());

        srv.try_transition(RcServerState::HwConfigured, || true)
            .unwrap();
        assert!(srv.handle_abb(sid, &req).is_ok());

        srv.try_transition(RcServerState::RcpConfigured, || true)
            .unwrap();
        assert!(srv.handle_abb(sid, &req).is_ok());
    }

    // ── Device endpoint dispatch ──────────────────────────────────────────────

    #[test]
    // fusa:test REQ-MOCKSRV-006
    fn device_endpoint_write_then_read_round_trips_through_dispatch() {
        let srv = RcServer::new(GeneralRegisters::default());
        let sid = stream(1);
        let ep = MockEndpoint::new(EndpointType::Gpio, vec![0; 4]);
        srv.register_endpoint(sid, 5, ep).unwrap();

        let write_req = abb_request(5, true, vec![0xDE, 0xAD, 0xBE, 0xEF]);
        srv.handle_abb(sid, &write_req).unwrap();

        let mut read_req = abb_request(5, false, Vec::new());
        read_req.info.read_size_segment = ReadSizeOrSegment(4);
        let resp = srv.handle_abb(sid, &read_req).unwrap();
        assert_eq!(resp.payload, vec![0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    // fusa:test REQ-MOCKSRV-006
    fn unregistered_device_endpoint_returns_ep_not_found() {
        let srv = RcServer::new(GeneralRegisters::default());
        let sid = stream(1);
        let req = abb_request(9, false, Vec::new());
        let err = srv.handle_abb(sid, &req).unwrap_err();
        assert_eq!(err, RcpError::EpNotFound);
    }

    #[test]
    // fusa:test REQ-MOCKSRV-006
    fn endpoint_registered_under_one_stream_is_not_visible_from_another() {
        let srv = RcServer::new(GeneralRegisters::default());
        let sid_a = stream(1);
        let sid_b = stream(2);
        let ep = MockEndpoint::new(EndpointType::Gpio, vec![0; 4]);
        srv.register_endpoint(sid_a, 5, ep).unwrap();

        let req = abb_request(5, false, Vec::new());
        assert!(srv.handle_abb(sid_a, &req).is_ok());
        let err = srv.handle_abb(sid_b, &req).unwrap_err();
        assert_eq!(err, RcpError::EpNotFound);
    }

    // ── Echo-back ─────────────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-MOCKSRV-007
    fn response_echoes_request_byte_bus_id() {
        let srv = RcServer::new(GeneralRegisters::default());
        let sid = stream(1);
        let ep = MockEndpoint::new(EndpointType::Gpio, vec![0; 4]);
        srv.register_endpoint(sid, 11, ep).unwrap();

        let req = abb_request(11, false, Vec::new());
        let resp = srv.handle_abb(sid, &req).unwrap();
        assert_eq!(resp.info.byte_bus_id, 11);
        assert!(resp.info.rsp);
    }

    // ── Whole on-wire round trip ──────────────────────────────────────────────

    #[test]
    // fusa:test REQ-MOCKSRV-008
    fn handle_ntscf_frame_round_trips_a_whole_on_wire_request() {
        let srv = RcServer::new(GeneralRegisters::default());
        let sid = stream(1);
        let ep = MockEndpoint::new(EndpointType::Gpio, vec![0xAA, 0xBB, 0xCC, 0xDD]);
        srv.register_endpoint(sid, 3, ep).unwrap();

        let mut req = abb_request(3, false, Vec::new());
        req.info.read_size_segment = ReadSizeOrSegment(4);
        let req_bytes = encode_acf_abb(&req).unwrap();
        let frame = encode_ntscf_frame(sid, 0, &req_bytes).unwrap();

        let response_frame = srv.handle_ntscf_frame(sid, &frame).unwrap();
        let (_hdr, resp_acf_bytes) = decode_ntscf_frame(&response_frame).unwrap();
        let resp = decode_acf_abb(resp_acf_bytes).unwrap();
        assert_eq!(resp.payload, vec![0xAA, 0xBB, 0xCC, 0xDD]);
        assert_eq!(resp.info.byte_bus_id, 3);
    }

    #[test]
    // fusa:test REQ-MOCKSRV-008
    fn handle_ntscf_frame_never_panics_on_garbage_input() {
        let srv = RcServer::new(GeneralRegisters::default());
        let sid = stream(1);
        for garbage in [&b""[..], &b"\x00"[..], &[0xFFu8; 4][..], &[0u8; 40][..]] {
            let _ = srv.handle_ntscf_frame(sid, garbage);
        }
    }

    // ── MockEndpoint ──────────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-MOCKSRV-009
    fn mock_endpoint_read_returns_last_written_bytes() {
        let ep = MockEndpoint::new(EndpointType::Gpio, Vec::new());
        ep.write(&[1, 2, 3]).unwrap();
        assert_eq!(ep.read(3).unwrap(), vec![1, 2, 3]);
    }

    #[test]
    // fusa:test REQ-MOCKSRV-010
    fn mock_endpoint_read_size_exceeding_buffer_does_not_panic() {
        let ep = MockEndpoint::new(EndpointType::Gpio, vec![1, 2]);
        let out = ep.read(255).unwrap();
        assert_eq!(out, vec![1, 2]);
    }

    #[test]
    // fusa:test REQ-MOCKSRV-010
    fn mock_endpoint_read_on_empty_buffer_does_not_panic() {
        let ep = MockEndpoint::new(EndpointType::Gpio, Vec::new());
        assert_eq!(ep.read(10).unwrap(), Vec::<u8>::new());
    }
}
