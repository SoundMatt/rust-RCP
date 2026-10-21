//! Tests for [`crate::server`]: the request store (c-RCP `server.c`), the
//! scheduler (c-RCP `scheduler.c`) and the [`RcServer`] dispatch pipeline
//! (the server half of c-RCP `mock.c`).

use super::*;
use crate::acf::{encode_acf_gbb, AcfGbbMessage, ReadSizeOrSegment};
use crate::ep0::EP0_BYTE_BUS_ID;
use crate::mock::MockEndpoint;
use crate::request::{
    encode_chained_member, encode_clear_all, encode_clear_non_safestate, encode_clear_single,
    encode_compound_request, encode_timed_request, encode_triggered_request, ChainedMember,
    ClearRequest, ClearSingleRequest, CompoundRequest, TimedRequest, TriggeredRequest,
    COMPOUND_REPEAT_INFINITE,
};

// ── Fixtures ─────────────────────────────────────────────────────────────────

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

fn abb_bytes(byte_bus_id: u16, op: bool, tn: u8, evt: Evt, payload: Vec<u8>) -> Vec<u8> {
    let mut msg = abb_request(byte_bus_id, op, payload);
    msg.info.transaction_num = tn;
    msg.info.evt = evt;
    encode_acf_abb(&msg).unwrap()
}

fn read_bytes(byte_bus_id: u16, tn: u8, read_size: u16) -> Vec<u8> {
    let mut msg = abb_request(byte_bus_id, false, Vec::new());
    msg.info.transaction_num = tn;
    msg.info.read_size_segment = ReadSizeOrSegment(read_size);
    encode_acf_abb(&msg).unwrap()
}

const ACK_EVT: Evt = Evt {
    ack: true,
    sub_opcode: 0,
};

const CONFIG_EVT: Evt = Evt {
    ack: false,
    sub_opcode: 0x07,
};

fn frame(sid: StreamId, seq: u8, members: &[Vec<u8>]) -> Vec<u8> {
    encode_ntscf_frame(sid, seq, &members.concat()).unwrap()
}

fn compound(
    kind: RequestKind,
    bbid: u16,
    tn: u8,
    step: CompoundStep,
    evt: u8,
    payload: Vec<u8>,
) -> Vec<u8> {
    encode_compound_request(&CompoundRequest {
        request_type: kind,
        byte_bus_id: bbid,
        step,
        evt: Evt {
            ack: evt & 0x8 != 0,
            sub_opcode: evt & 0x7,
        },
        transaction_num: tn,
        payload,
    })
    .unwrap()
}

fn step(start: u8, next: u8, seq: u8, delay: u16, repeat: u16) -> CompoundStep {
    CompoundStep {
        start_state: start,
        next_state: next,
        sequencer_index: seq,
        exec_delay: delay,
        repeat_count: repeat,
    }
}

fn triggered(kind: RequestKind, bbid: u16, tn: u8, step: TriggeredStep) -> Vec<u8> {
    encode_triggered_request(&TriggeredRequest {
        request_type: kind,
        byte_bus_id: bbid,
        step,
        transaction_num: tn,
        payload: Vec::new(),
    })
    .unwrap()
}

fn timed(bbid: u16, tn: u8, presentation_time: u64) -> Vec<u8> {
    encode_timed_request(&TimedRequest {
        byte_bus_id: bbid,
        presentation_time,
        transaction_num: tn,
        payload: Vec::new(),
    })
    .unwrap()
}

fn chained(bbid: u16, tn: u8, delay: u16, cs: bool) -> Vec<u8> {
    encode_chained_member(&ChainedMember {
        byte_bus_id: bbid,
        chain_exec_delay: delay,
        cs,
        transaction_num: tn,
        payload: Vec::new(),
    })
    .unwrap()
}

fn clear_all(bbid: u16, tn: u8) -> Vec<u8> {
    encode_clear_all(&ClearRequest {
        byte_bus_id: bbid,
        transaction_num: tn,
    })
    .unwrap()
}

fn clear_single(bbid: u16, tn: u8, target: u8) -> Vec<u8> {
    encode_clear_single(&ClearSingleRequest {
        byte_bus_id: bbid,
        clear_transaction_num: target,
        transaction_num: tn,
    })
    .unwrap()
}

fn ctx(now: u32) -> TickCtx<'static> {
    TickCtx {
        now,
        endpoint_idle: true,
        ..TickCtx::default()
    }
}

fn decode_one(bytes: &[u8]) -> AcfAbbMessage {
    decode_acf_abb(bytes).unwrap()
}

/// A server with one MockEndpoint at `(stream(1), 5)`.
fn server_with_endpoint(initial: Vec<u8>) -> (Arc<RcServer>, Arc<MockEndpoint>) {
    let srv = RcServer::new(GeneralRegisters::default());
    let ep = MockEndpoint::new(EndpointType::Gpio, initial);
    srv.register_endpoint(stream(1), 5, ep.clone()).unwrap();
    srv.set_time_sync_supported(true);
    (srv, ep)
}

/// A request-stream configuration row for `sid` with every response
/// routed (`rx_ack_stream_index = rx_resp_stream_index = 1`).
fn cfg_for(sid: StreamId) -> RequestStreamConfigEntry {
    RequestStreamConfigEntry {
        rx_stream_id: sid.to_u64(),
        rx_ack_stream_index: 1,
        rx_resp_stream_index: 1,
        ..RequestStreamConfigEntry::default()
    }
}

fn dispatch(srv: &RcServer, sid: StreamId, seq: u8, members: &[Vec<u8>]) -> FrameDispatch {
    srv.dispatch_frame(
        &frame(sid, seq, members),
        &Peer::from_header(Instant::now()),
    )
}

// ── Lifecycle / registration / EP0 (moved from mock.rs) ─────────────────────

#[test]
//fusa:test REQ-MOCKSRV-001
fn new_server_starts_hw_unconfigured_with_no_root_client() {
    let srv = RcServer::new(GeneralRegisters::default());
    assert_eq!(srv.state(), RcServerState::HwUnconfigured);
    assert_eq!(srv.ep_id_map(), default_ep_id_map());
    assert!(srv.request_stream_cfg().is_empty());
}

#[test]
//fusa:test REQ-MOCKSRV-002
fn try_transition_updates_state_on_success_and_leaves_it_on_failure() {
    let srv = RcServer::new(GeneralRegisters::default());
    let snap = PlausibilitySnapshot::default();
    srv.try_transition(
        RcServerState::HwConfigured,
        Some(&snap),
        WriterCtx::default(),
        true,
    )
    .unwrap();
    assert_eq!(srv.state(), RcServerState::HwConfigured);

    let root = WriterCtx {
        via_root_client_ep0: true,
        ..WriterCtx::default()
    };
    let err = srv
        .try_transition(RcServerState::RcpConfigured, None, root, true)
        .unwrap_err();
    assert_eq!(err, LifecycleError::RcpCfgInconsistent);
    assert_eq!(srv.state(), RcServerState::HwConfigured);

    let err = srv
        .try_transition(
            RcServerState::RcpConfigured,
            Some(&snap),
            WriterCtx::default(),
            true,
        )
        .unwrap_err();
    assert_eq!(err, LifecycleError::Unauthorized);
    assert_eq!(srv.state(), RcServerState::HwConfigured);
}

#[test]
//fusa:test REQ-MOCKSRV-003
fn register_endpoint_assigns_unique_ids_and_rejects_duplicate_pair() {
    let srv = RcServer::new(GeneralRegisters::default());
    let sid = stream(1);
    let id_a = srv
        .register_endpoint(sid, 7, MockEndpoint::new(EndpointType::Gpio, vec![0; 4]))
        .unwrap();
    let ep_b = MockEndpoint::new(EndpointType::Gpio, vec![0; 4]);
    let id_b = srv.register_endpoint(sid, 8, ep_b.clone()).unwrap();
    assert_ne!(id_a, id_b);
    assert_eq!(
        srv.register_endpoint(sid, 8, ep_b).unwrap_err(),
        RcpError::EpError
    );
}

#[test]
//fusa:test REQ-MOCKSRV-004
//fusa:test REQ-MOCKSRV-005
fn ep0_read_returns_general_registers_snapshot() {
    let regs = GeneralRegisters {
        svr_vendor_id: 0x1234,
        ..Default::default()
    };
    let srv = RcServer::new(regs);
    let resp = srv
        .handle_abb(stream(1), &abb_request(EP0_BYTE_BUS_ID, false, Vec::new()))
        .unwrap();
    assert_eq!(resp.payload, regs.encode().to_vec());
}

#[test]
//fusa:test REQ-MOCKSRV-004
//fusa:test REQ-MOCKSRV-005
//fusa:test REQ-RMAP-037
fn ep0_general_write_from_the_root_client_is_locked_in_every_state() {
    let srv = RcServer::new(GeneralRegisters::default());
    let sid = stream(1);
    srv.set_root_client(Some(sid));
    let snap = PlausibilitySnapshot::default();
    let root = WriterCtx {
        via_root_client_ep0: true,
        ..WriterCtx::default()
    };
    let new_regs = GeneralRegisters {
        svr_vendor_id: 0xBEEF,
        ..Default::default()
    };
    let req = abb_request(EP0_BYTE_BUS_ID, true, new_regs.encode().to_vec());
    for target in [RcServerState::HwConfigured, RcServerState::RcpConfigured] {
        assert_eq!(
            srv.handle_abb(sid, &req).unwrap_err(),
            RcpError::LockedMemAccess
        );
        assert_eq!(srv.general_registers().svr_vendor_id, 0);
        srv.try_transition(target, Some(&snap), root, true).unwrap();
    }
    assert_eq!(
        srv.handle_abb(sid, &req).unwrap_err(),
        RcpError::LockedMemAccess
    );
}

#[test]
//fusa:test REQ-MOCKSRV-004
fn ep0_write_from_non_root_client_is_rejected() {
    let srv = RcServer::new(GeneralRegisters::default());
    srv.set_root_client(Some(stream(1)));
    let req = abb_request(
        EP0_BYTE_BUS_ID,
        true,
        GeneralRegisters::default().encode().to_vec(),
    );
    // c-RCP parity: writer-independent LOCKED_MEM_ACCESS (c-RCP/src/regmap.c:283).
    assert_eq!(
        srv.handle_abb(stream(2), &req).unwrap_err(),
        RcpError::LockedMemAccess
    );
}

#[test]
//fusa:test REQ-MOCKSRV-004
fn ep0_read_is_reachable_in_every_lifecycle_state() {
    let srv = RcServer::new(GeneralRegisters::default());
    let sid = stream(1);
    let req = abb_request(EP0_BYTE_BUS_ID, false, Vec::new());
    assert!(srv.handle_abb(sid, &req).is_ok());
    let snap = PlausibilitySnapshot::default();
    let root = WriterCtx {
        via_root_client_ep0: true,
        ..WriterCtx::default()
    };
    srv.try_transition(RcServerState::HwConfigured, Some(&snap), root, true)
        .unwrap();
    assert!(srv.handle_abb(sid, &req).is_ok());
    srv.try_transition(RcServerState::RcpConfigured, Some(&snap), root, true)
        .unwrap();
    assert!(srv.handle_abb(sid, &req).is_ok());
}

#[test]
//fusa:test REQ-MOCKSRV-006
fn device_endpoint_write_then_read_round_trips_through_dispatch() {
    let (srv, _ep) = server_with_endpoint(vec![0; 4]);
    srv.handle_abb(
        stream(1),
        &abb_request(5, true, vec![0xDE, 0xAD, 0xBE, 0xEF]),
    )
    .unwrap();
    let mut read_req = abb_request(5, false, Vec::new());
    read_req.info.read_size_segment = ReadSizeOrSegment(4);
    let resp = srv.handle_abb(stream(1), &read_req).unwrap();
    assert_eq!(resp.payload, vec![0xDE, 0xAD, 0xBE, 0xEF]);
}

#[test]
//fusa:test REQ-MOCKSRV-006
fn unregistered_device_endpoint_returns_ep_not_found_and_is_stream_scoped() {
    let (srv, _ep) = server_with_endpoint(vec![0; 4]);
    let req = abb_request(5, false, Vec::new());
    assert!(srv.handle_abb(stream(1), &req).is_ok());
    assert_eq!(
        srv.handle_abb(stream(2), &req).unwrap_err(),
        RcpError::EpNotFound
    );
    assert_eq!(
        srv.handle_abb(stream(1), &abb_request(9, false, Vec::new()))
            .unwrap_err(),
        RcpError::EpNotFound
    );
}

#[test]
//fusa:test REQ-MOCKSRV-007
fn response_echoes_request_byte_bus_id() {
    let (srv, _ep) = server_with_endpoint(vec![0; 4]);
    let resp = srv
        .handle_abb(stream(1), &abb_request(5, false, Vec::new()))
        .unwrap();
    assert_eq!(resp.info.byte_bus_id, 5);
    assert!(resp.info.rsp);
}

#[test]
//fusa:test REQ-MOCKSRV-008
fn handle_ntscf_frame_round_trips_multiple_requests_in_one_frame() {
    let srv = RcServer::new(GeneralRegisters::default());
    let sid = stream(1);
    srv.register_endpoint(
        sid,
        3,
        MockEndpoint::new(EndpointType::Gpio, vec![0xAA, 0xBB]),
    )
    .unwrap();
    srv.register_endpoint(
        sid,
        4,
        MockEndpoint::new(EndpointType::Gpio, vec![0x11, 0x22]),
    )
    .unwrap();
    let f = frame(sid, 0, &[read_bytes(3, 1, 2), read_bytes(4, 2, 2)]);
    let response = srv.handle_ntscf_frame(sid, &f).unwrap();
    let (hdr, acf) = decode_ntscf_frame(&response).unwrap();
    assert_eq!(hdr.stream_id, sid.to_u64());
    let responses = crate::acf::decode_acf_abb_messages(acf).unwrap();
    assert_eq!(responses.len(), 2);
    assert_eq!(responses[0].payload, vec![0xAA, 0xBB]);
    assert_eq!(responses[0].info.transaction_num, 1);
    assert_eq!(responses[1].payload, vec![0x11, 0x22]);
    assert_eq!(responses[1].info.transaction_num, 2);
}

#[test]
//fusa:test REQ-MOCKSRV-008
fn handle_ntscf_frame_errs_only_for_a_frame_level_failure_and_never_panics() {
    let srv = RcServer::new(GeneralRegisters::default());
    let sid = stream(1);
    for garbage in [&b""[..], &b"\x00"[..], &[0xFFu8; 4][..], &[0u8; 40][..]] {
        let _ = srv.handle_ntscf_frame(sid, garbage);
    }
    assert!(srv.handle_ntscf_frame(sid, &[0xFF; 3]).is_err());
    // An unregistered endpoint is dropped: a frame with no members' responses.
    let response = srv
        .handle_ntscf_frame(sid, &frame(sid, 0, &[read_bytes(9, 0, 1)]))
        .unwrap();
    assert!(decode_ntscf_frame(&response).unwrap().1.is_empty());
}

// ── Scheduler (REQ-SCHED-*) ──────────────────────────────────────────────────

#[test]
//fusa:test REQ-SCHED-001
fn sched_classify_maps_every_opcode() {
    assert_eq!(sched_classify(false, 0x0F), SchedKind::Standard);
    let cases = [
        (0x05, SchedKind::Cancellation),
        (0x06, SchedKind::Cancellation),
        (0x07, SchedKind::Cancellation),
        (0x0E, SchedKind::Triggered),
        (0x8E, SchedKind::Triggered),
        (0x0A, SchedKind::Timed),
        (0x0F, SchedKind::Compound),
        (0x8F, SchedKind::Compound),
        (0x0B, SchedKind::CompoundWait),
        (0x8B, SchedKind::CompoundWait),
        (0x01, SchedKind::Chained),
        (0x00, SchedKind::Standard),
        (0x42, SchedKind::Standard),
    ];
    for (rt, kind) in cases {
        assert_eq!(sched_classify(true, rt), kind, "opcode {rt:#04x}");
    }
}

#[test]
//fusa:test REQ-SCHED-002
fn sched_rank_orders_cancellation_down_to_standard() {
    let order = [
        SchedKind::Cancellation,
        SchedKind::Triggered,
        SchedKind::Timed,
        SchedKind::Compound,
        SchedKind::CompoundWait,
        SchedKind::Chained,
        SchedKind::Standard,
    ];
    for pair in order.windows(2) {
        assert!(pair[0].rank() > pair[1].rank());
    }
}

#[test]
//fusa:test REQ-SCHED-003
fn sched_compare_orders_by_rank_then_fifo() {
    use std::cmp::Ordering::*;
    assert_eq!(
        sched_compare((SchedKind::Timed, 9), (SchedKind::Chained, 1)),
        Less
    );
    assert_eq!(
        sched_compare((SchedKind::Chained, 1), (SchedKind::Timed, 9)),
        Greater
    );
    assert_eq!(
        sched_compare((SchedKind::Timed, 1), (SchedKind::Timed, 2)),
        Less
    );
    assert_eq!(
        sched_compare((SchedKind::Timed, 2), (SchedKind::Timed, 1)),
        Greater
    );
    assert_eq!(
        sched_compare((SchedKind::Timed, 2), (SchedKind::Timed, 2)),
        Equal
    );
}

#[test]
//fusa:test REQ-SCHED-004
fn split_frame_members_reports_every_abb_and_gbb_member() {
    let a = read_bytes(3, 1, 0);
    let g = timed(3, 2, 0);
    let stream_bytes = [a.clone(), g.clone(), a.clone()].concat();
    let ranges = split_frame_members(&stream_bytes);
    assert_eq!(
        ranges,
        vec![
            0..a.len(),
            a.len()..a.len() + g.len(),
            a.len() + g.len()..stream_bytes.len()
        ]
    );
}

#[test]
//fusa:test REQ-SCHED-005
fn split_frame_members_rejects_a_malformed_stream_whole() {
    let a = read_bytes(3, 1, 0);
    // Truncated second member.
    let truncated = [a.clone(), a[..5].to_vec()].concat();
    assert!(split_frame_members(&truncated).is_empty());
    // One trailing byte (header truncated).
    assert!(split_frame_members(&[a.clone(), vec![0x1C]].concat()).is_empty());
    // Unknown acf_msg_type.
    assert!(split_frame_members(&[0xFE; 8]).is_empty());
    // Declared length shorter than the header.
    let mut short = a.clone();
    short[1] = 1;
    assert!(split_frame_members(&short).is_empty());
    // Declared length past the end.
    let mut long = a;
    long[1] = 9;
    assert!(split_frame_members(&long).is_empty());
}

#[test]
//fusa:test REQ-SCHED-006
fn split_frame_members_returns_nothing_for_an_empty_stream() {
    assert!(split_frame_members(&[]).is_empty());
}

// ── Compound-wait comparison ────────────────────────────────────────────────

#[test]
//fusa:test REQ-SRV-019
fn compound_wait_evt_valid_rejects_only_the_reserved_mode() {
    for evt in 0u8..16 {
        assert_eq!(compound_wait_evt_valid(evt), evt & 0x7 != 0x3, "evt {evt}");
    }
}

#[test]
//fusa:test REQ-SRV-020
fn compound_wait_match_applies_every_mode() {
    // Length rule first.
    assert!(!compound_wait_match(0, &[1, 2], &[1]));
    // Exact (status capped to payload length).
    assert!(compound_wait_match(0, &[1, 2], &[1, 2, 9]));
    assert!(!compound_wait_match(0, &[1, 2], &[1, 3]));
    assert!(compound_wait_match(0, &[], &[]));
    // AND-ones / AND-zeros.
    assert!(compound_wait_match(1, &[0x0F], &[0xFF]));
    assert!(!compound_wait_match(1, &[0x0F], &[0x0E]));
    assert!(compound_wait_match(2, &[0x0F], &[0xF0]));
    assert!(!compound_wait_match(2, &[0x0F], &[0x01]));
    // Reserved never matches; word modes need four bytes.
    assert!(!compound_wait_match(3, &[0; 4], &[0; 4]));
    assert!(!compound_wait_match(4, &[0; 3], &[0; 3]));
    let payload = [0x00, 0x10, 0x00, 0x20];
    // c-RCP parity: payload >= status / payload <= status.
    assert!(compound_wait_match(4, &payload, &[0x00, 0x0F, 0xFF, 0xFF]));
    assert!(!compound_wait_match(4, &payload, &[0x00, 0x11, 0x00, 0x00]));
    assert!(compound_wait_match(5, &payload, &[0x00, 0x11, 0x00, 0x00]));
    assert!(compound_wait_match(6, &payload, &[0xFF, 0xFF, 0x00, 0x1F]));
    assert!(!compound_wait_match(6, &payload, &[0x00, 0x00, 0x00, 0x21]));
    assert!(compound_wait_match(7, &payload, &[0x00, 0x00, 0x00, 0x21]));
}

// ── ServerEndpoint: ep_enable queue (REQ-SRV-001..003, 015, 016, 023) ──────

#[test]
//fusa:test REQ-SRV-001
//fusa:test REQ-SRV-002
fn submit_queues_while_disabled_and_executes_while_enabled() {
    let mut ep = ServerEndpoint::new(true);
    let f = read_bytes(5, 1, 1);
    assert_eq!(ep.submit(&f), Submission::ExecuteNow);
    assert_eq!(ep.queue_len(), 0);
    ep.set_enable(false);
    assert_eq!(ep.submit(&f), Submission::Queued { ack: None });
    assert_eq!(ep.queue_len(), 1);
}

#[test]
//fusa:test REQ-SRV-003
//fusa:test REQ-SRV-023
fn drain_refuses_while_disabled_then_drains_fifo_once_enabled() {
    let mut ep = ServerEndpoint::new(false);
    let (a, b) = (read_bytes(5, 1, 1), read_bytes(5, 2, 1));
    let _ = ep.submit(&a);
    let _ = ep.submit(&b);
    assert_eq!(ep.drain_one(), None);
    assert_eq!(ep.queue_len(), 2);
    ep.set_enable(true);
    assert!(ep.ep_enable());
    assert_eq!(ep.drain_one(), Some(a));
    assert_eq!(ep.drain_one(), Some(b));
    assert_eq!(ep.drain_one(), None);
}

#[test]
//fusa:test REQ-SRV-015
fn a_disabled_endpoint_still_executes_configuration_requests() {
    let mut ep = ServerEndpoint::new(false);
    // ACF_ABB with evt[2:0] = 111b.
    let cfg = abb_bytes(5, true, 1, CONFIG_EVT, vec![1]);
    assert_eq!(ep.submit(&cfg), Submission::ExecuteNow);
    // A timestamped (mtv=1) ACF_GBB with evt[2:0] = 111b whose octet 8 is
    // not a compound-wait opcode: configuration.
    let gbb = encode_acf_gbb(&AcfGbbMessage {
        info: ByteMessageInfo {
            byte_bus_id: 5,
            evt: CONFIG_EVT,
            mtv: true,
            ..Default::default()
        },
        message_timestamp: 0x0100_0000_0000_0000,
        payload: Vec::new(),
    })
    .unwrap();
    assert_eq!(ep.submit(&gbb), Submission::ExecuteNow);
    // A compound-wait request with evt[2:0] = 111b is operational: queued.
    let cw = compound(
        RequestKind::CompoundWait,
        5,
        2,
        step(1, 2, 0, 0, 0),
        0x7,
        vec![],
    );
    assert!(matches!(ep.submit(&cw), Submission::Queued { .. }));
    // Too short to read a header: queued (fail-safe).
    assert!(matches!(ep.submit(&[0x1C]), Submission::Queued { .. }));
    assert_eq!(ep.queue_len(), 2);
}

#[test]
//fusa:test REQ-SRV-016
fn queuing_a_request_emits_the_requested_acknowledge() {
    let mut ep = ServerEndpoint::new(false);
    let Submission::Queued { ack: Some(ack) } =
        ep.submit(&abb_bytes(5, false, 0x33, ACK_EVT, vec![]))
    else {
        panic!("an acknowledge was requested");
    };
    let ack = decode_one(&ack);
    assert_eq!(ack.info.response_kind(), ResponseKind::Acknowledge);
    assert_eq!(ack.info.transaction_num, 0x33);
    assert_eq!(ack.info.byte_bus_id, 5);
    assert!(!ack.info.err && ack.info.rsp);
    // Without evt[3]: no acknowledge.
    assert_eq!(
        ep.submit(&read_bytes(5, 1, 0)),
        Submission::Queued { ack: None }
    );
}

// ── ServerEndpoint::admit (REQ-SRV-004, 005, 019, 022, 024..026, 042) ──────

#[test]
//fusa:test REQ-PWRMODE-028
fn admit_refuses_everything_while_admission_is_suspended() {
    let mut ep = ServerEndpoint::new(true);
    ep.set_admission_suspended(true);
    for f in [read_bytes(5, 1, 0), clear_all(5, 1), timed(5, 1, 0)] {
        let a = ep.admit(&f, 0, None);
        assert_eq!(a.result, AdmitResult::Suspended);
        assert_eq!(a.request_type, 0);
    }
    assert_eq!(ep.pending_count() + ep.queue_len(), 0);
    ep.set_admission_suspended(false);
    assert_eq!(
        ep.admit(&read_bytes(5, 1, 0), 0, None).result,
        AdmitResult::ExecuteNow
    );
}

#[test]
//fusa:test REQ-ACF-050
fn admit_rejects_a_response_frame_with_invalid_parameter() {
    let mut ep = ServerEndpoint::new(true);
    let mut msg = abb_request(5, false, Vec::new());
    msg.info.rsp = true;
    let a = ep.admit(&encode_acf_abb(&msg).unwrap(), 0, None);
    assert_eq!(a.result, AdmitResult::Rejected);
    assert_eq!(a.error, Some(RcpError::InvalidParameter));
}

#[test]
//fusa:test REQ-SRV-004
//fusa:test REQ-SRV-026
fn admit_takes_the_standard_path_for_abb_and_unrecognized_opcodes() {
    let mut ep = ServerEndpoint::new(true);
    let a = ep.admit(&read_bytes(5, 1, 0), 0, None);
    assert_eq!(
        (a.result, a.request_type, a.index),
        (AdmitResult::ExecuteNow, 0, None)
    );
    // Repurposed ACF_GBB with an opcode nobody defines: standard.
    let odd = encode_acf_gbb(&AcfGbbMessage {
        info: ByteMessageInfo {
            byte_bus_id: 5,
            ..Default::default()
        },
        message_timestamp: 0x4200_0000_0000_0000,
        payload: Vec::new(),
    })
    .unwrap();
    assert_eq!(ep.admit(&odd, 0, None).result, AdmitResult::ExecuteNow);
    ep.set_enable(false);
    let q = ep.admit(&abb_bytes(5, false, 9, ACK_EVT, vec![]), 0, None);
    assert_eq!(q.result, AdmitResult::Queued);
    assert!(q.ack.is_some());
}

#[test]
//fusa:test REQ-SRV-024
fn admit_reports_a_cancellation_without_storing_it() {
    let mut ep = ServerEndpoint::new(true);
    let a = ep.admit(&clear_all(5, 1), 0, None);
    assert_eq!(
        (a.result, a.request_type),
        (AdmitResult::Cancellation, 0x05)
    );
    assert_eq!(ep.pending_count(), 0);
}

#[test]
//fusa:test REQ-SRV-025
//fusa:test REQ-SRV-026
fn admit_decodes_and_stores_every_conditional_kind() {
    let mut ep = ServerEndpoint::new(true);
    let frames = [
        (
            compound(RequestKind::Compound, 5, 1, step(1, 2, 0, 3, 0), 0, vec![]),
            SchedKind::Compound,
            0x0F,
        ),
        (
            triggered(RequestKind::Triggered, 5, 2, TriggeredStep::default()),
            SchedKind::Triggered,
            0x0E,
        ),
        (timed(5, 3, 77), SchedKind::Timed, 0x0A),
        (chained(5, 4, 6, true), SchedKind::Chained, 0x01),
    ];
    for (i, (f, kind, rt)) in frames.iter().enumerate() {
        let a = ep.admit(f, 10, None);
        assert_eq!((a.result, a.request_type), (AdmitResult::Pending, *rt));
        let req = ep.pending(a.index.unwrap()).unwrap();
        assert_eq!(req.kind, *kind);
        assert_eq!(req.transaction_num, i as u8 + 1);
        assert_eq!(req.frame, *f);
        assert_eq!(req.armed_at, 10);
        assert!(!req.has_presentation_gate);
    }
    assert_eq!(ep.pending_count(), 4);
    let t = ep.pending(2).unwrap();
    assert_eq!(t.presentation_time, 77);
    let c = ep.pending(3).unwrap();
    assert_eq!((c.chain_exec_delay, c.cs), (6, true));
    assert!(ep.pending(1).unwrap().triggered_runtime.started);
    assert_eq!(ep.pending(0).unwrap().compound.exec_delay, 3);
    assert!(ep.pending(SERVER_MAX_PENDING).is_none());
}

#[test]
//fusa:test REQ-SRV-005
fn admit_rejects_when_the_store_is_full_with_request_storage_overflow() {
    let mut ep = ServerEndpoint::new(true);
    for i in 0..SERVER_MAX_PENDING {
        assert_eq!(
            ep.admit(&timed(5, i as u8, 0), 0, None).result,
            AdmitResult::Pending
        );
    }
    let a = ep.admit(&timed(5, 99, 0), 0, None);
    assert_eq!(a.result, AdmitResult::Rejected);
    assert_eq!(a.error, Some(RcpError::ReqStorageOvfl));
    assert_eq!(ep.pending_count(), SERVER_MAX_PENDING);
    // Under a TSCF gate a full store rejects with no code (c-RCP parity).
    let gate = PresentationGate {
        avtp_timestamp: 0,
        gptp_reference_now: 0,
    };
    let g = ep.admit(&read_bytes(5, 1, 0), 0, Some(gate));
    assert_eq!((g.result, g.error), (AdmitResult::Rejected, None));
}

#[test]
//fusa:test REQ-SRV-005
fn admit_rejects_an_undecodable_conditional_body_storing_nothing() {
    let mut ep = ServerEndpoint::new(true);
    // A timed request with hs set fails decode_timed_request.
    let mut bad = timed(5, 1, 0);
    bad[4] |= 0x02;
    let a = ep.admit(&bad, 0, None);
    assert_eq!(
        (a.result, a.error.clone(), a.request_type),
        (AdmitResult::Rejected, None, 0x0A)
    );
    for f in [
        {
            // Declares one quadlet more than it carries.
            let mut t = triggered(RequestKind::Triggered, 5, 1, TriggeredStep::default());
            t[1] += 1;
            t
        },
        {
            let mut c = chained(5, 1, 0, false);
            c[14] = 0xFF; // reserved octet
            c
        },
        {
            let mut c = compound(RequestKind::Compound, 5, 1, step(1, 1, 0, 0, 0), 0, vec![]);
            c[1] += 1;
            c
        },
    ] {
        assert_eq!(ep.admit(&f, 0, None).result, AdmitResult::Rejected);
    }
    assert_eq!(ep.pending_count(), 0);
}

#[test]
//fusa:test REQ-SRV-019
//fusa:test REQ-SRV-022
//fusa:test REQ-SRV-042
fn admit_handles_compound_wait_evt_and_target() {
    let mut ep = ServerEndpoint::new(true);
    let reserved = compound(
        RequestKind::CompoundWait,
        5,
        1,
        step(1, 2, 0, 0, 0),
        0x3,
        vec![7],
    );
    let a = ep.admit(&reserved, 0, None);
    assert_eq!(a.result, AdmitResult::Rejected);
    assert_eq!(a.error, Some(RcpError::UnsupportedCmd));
    assert_eq!(ep.pending_count(), 0);

    let one = compound(
        RequestKind::CompoundWait,
        5,
        2,
        step(1, 2, 0, 0, 0),
        0x1,
        vec![0x0F],
    );
    let two = compound(
        RequestKind::SafetyCompoundWait,
        5,
        3,
        step(1, 2, 0, 0, 0),
        0x2,
        vec![0xF0],
    );
    let i1 = ep.admit(&one, 0, None).index.unwrap();
    let i2 = ep.admit(&two, 0, None).index.unwrap();
    let (r1, r2) = (ep.pending(i1).unwrap(), ep.pending(i2).unwrap());
    assert_eq!(
        (r1.compound_wait_evt, r1.compound_wait_target.clone()),
        (0x1, vec![0x0F])
    );
    assert_eq!(
        (r2.compound_wait_evt, r2.compound_wait_target.clone()),
        (0x2, vec![0xF0])
    );
    assert_eq!(r2.request_type, 0x8B);
}

#[test]
//fusa:test REQ-SRV-004
//fusa:test REQ-SRV-016
fn admit_under_a_tscf_gate_stores_standard_and_cancellation_requests() {
    let mut ep = ServerEndpoint::new(true);
    let gate = PresentationGate {
        avtp_timestamp: 1000,
        gptp_reference_now: 0,
    };
    let s = ep.admit(&abb_bytes(5, false, 4, ACK_EVT, vec![]), 7, Some(gate));
    assert_eq!((s.result, s.request_type), (AdmitResult::Pending, 0));
    assert!(s.ack.is_some());
    let req = ep.pending(s.index.unwrap()).unwrap();
    assert_eq!(req.kind, SchedKind::Standard);
    assert!(req.has_presentation_gate);
    assert_eq!(req.presentation_gate_ns, 1000);
    let c = ep.admit(&clear_all(5, 1), 7, Some(gate));
    assert_eq!((c.result, c.request_type), (AdmitResult::Pending, 0x05));
    assert_eq!(
        ep.pending(c.index.unwrap()).unwrap().kind,
        SchedKind::Cancellation
    );
    let t = ep.admit(&timed(5, 2, 5), 7, Some(gate));
    assert!(ep.pending(t.index.unwrap()).unwrap().has_presentation_gate);

    // Not due before the presentation time, nor without a locked time base.
    let seqs = SequencerBank::new(0);
    let mut early = ctx(0);
    early.gptp_now = 999;
    early.gptp_locked = true;
    assert_eq!(ep.select_due(&early, &seqs), None);
    let mut unlocked = early;
    unlocked.gptp_now = 1000;
    unlocked.gptp_locked = false;
    assert_eq!(ep.select_due(&unlocked, &seqs), None);
    let mut due = unlocked;
    due.gptp_locked = true;
    // The cancellation outranks the standard request.
    assert_eq!(ep.select_due(&due, &seqs), c.index);
}

// ── ServerEndpoint::select_due (REQ-SRV-006..008, 027..032) ────────────────

#[test]
//fusa:test REQ-SRV-006
fn a_compound_request_needs_its_sequencer_state_its_delay_and_an_idle_endpoint() {
    let mut ep = ServerEndpoint::new(true);
    let mut seqs = SequencerBank::new(2);
    let i = ep
        .admit(
            &compound(RequestKind::Compound, 5, 1, step(2, 3, 1, 5, 0), 0, vec![]),
            0,
            None,
        )
        .index;
    // Sequencer 1 is in its power-on state 1, not start_state 2.
    assert_eq!(ep.select_due(&ctx(100), &seqs), None);
    seqs.force_state(1, SequencerState(2)).unwrap();
    // Arms at 100; delay 5 not yet elapsed.
    assert_eq!(ep.select_due(&ctx(100), &seqs), None);
    assert_eq!(ep.select_due(&ctx(104), &seqs), None);
    let mut busy = ctx(105);
    busy.endpoint_idle = false;
    assert_eq!(ep.select_due(&busy, &seqs), None);
    assert_eq!(ep.select_due(&ctx(105), &seqs), i);
}

#[test]
//fusa:test REQ-SRV-006
fn a_compound_request_never_starts_on_a_disabled_or_unknown_sequencer() {
    let mut ep = ServerEndpoint::new(true);
    let mut seqs = SequencerBank::new(1);
    let _ = ep.admit(
        &compound(RequestKind::Compound, 5, 1, step(0, 3, 0, 0, 0), 0, vec![]),
        0,
        None,
    );
    let _ = ep.admit(
        &compound(RequestKind::Compound, 5, 2, step(0, 3, 4, 0, 0), 0, vec![]),
        0,
        None,
    );
    seqs.force_state(0, SequencerState(0)).unwrap();
    assert_eq!(ep.select_due(&ctx(1), &seqs), None);
    // start_state 0 = any state, once the sequencer is enabled again.
    seqs.force_state(0, SequencerState(9)).unwrap();
    assert_eq!(ep.select_due(&ctx(1), &seqs), Some(0));
}

#[test]
//fusa:test REQ-SRV-027
//fusa:test REQ-SRV-020
fn a_compound_wait_request_needs_its_comparison_but_not_an_idle_endpoint() {
    let mut ep = ServerEndpoint::new(true);
    let seqs = SequencerBank::new(1);
    let a = ep
        .admit(
            &compound(
                RequestKind::CompoundWait,
                5,
                1,
                step(1, 2, 0, 0, 0),
                0x0,
                vec![0xAB],
            ),
            0,
            None,
        )
        .index;
    let b = ep
        .admit(
            &compound(
                RequestKind::CompoundWait,
                5,
                2,
                step(1, 2, 0, 0, 0),
                0x0,
                vec![0xCD],
            ),
            0,
            None,
        )
        .index;
    let mut c = ctx(1);
    c.endpoint_idle = false;
    assert_eq!(ep.select_due(&c, &seqs), None);
    c.current_status = &[0xCD];
    assert_eq!(ep.select_due(&c, &seqs), b);
    c.current_status = &[0xAB];
    assert_eq!(ep.select_due(&c, &seqs), a);
}

#[test]
//fusa:test REQ-SRV-028
//fusa:test REQ-SRV-011
fn a_triggered_request_needs_its_threshold_its_delay_and_an_idle_endpoint() {
    let mut ep = ServerEndpoint::new(true);
    let seqs = SequencerBank::new(0);
    let st = TriggeredStep {
        trigger_source_ep: 3,
        trigger_signal_nr: 1,
        trigger_threshold: 1,
        exec_delay: 2,
        repeat_count: 0,
    };
    let i = ep
        .admit(&triggered(RequestKind::Triggered, 5, 1, st), 0, None)
        .index;
    assert_eq!(ep.notify_trigger(3, 1), 1);
    assert_eq!(
        ep.select_due(&ctx(10), &seqs),
        None,
        "threshold 1 needs two occurrences"
    );
    assert_eq!(
        ep.notify_trigger(3, 2),
        0,
        "a different signal does not count"
    );
    assert_eq!(
        ep.notify_trigger(4, 1),
        0,
        "a different endpoint does not count"
    );
    assert_eq!(ep.notify_trigger(3, 1), 1);
    // Arms at 10 (threshold reached), even while busy.
    let mut busy = ctx(10);
    busy.endpoint_idle = false;
    assert_eq!(ep.select_due(&busy, &seqs), None);
    assert_eq!(ep.select_due(&ctx(11), &seqs), None);
    let mut busy = ctx(12);
    busy.endpoint_idle = false;
    assert_eq!(ep.select_due(&busy, &seqs), None);
    assert_eq!(ep.select_due(&ctx(12), &seqs), i);
}

#[test]
//fusa:test REQ-SRV-029
fn a_timed_request_needs_a_locked_time_base_and_its_presentation_time() {
    let mut ep = ServerEndpoint::new(true);
    let seqs = SequencerBank::new(0);
    let i = ep.admit(&timed(5, 1, 500), 0, None).index;
    let mut c = ctx(0);
    c.gptp_now = 600;
    assert_eq!(ep.select_due(&c, &seqs), None, "unlocked");
    c.gptp_locked = true;
    c.gptp_now = 499;
    assert_eq!(ep.select_due(&c, &seqs), None);
    c.gptp_now = 500;
    assert_eq!(ep.select_due(&c, &seqs), i);
}

#[test]
//fusa:test REQ-SRV-030
//fusa:test REQ-SRV-012
//fusa:test REQ-SRV-038
fn a_chained_request_runs_its_delay_from_its_predecessor_and_needs_an_idle_endpoint() {
    let mut ep = ServerEndpoint::new(true);
    let seqs = SequencerBank::new(0);
    let i = ep.admit(&chained(5, 1, 4, false), 0, None).index.unwrap();
    assert_eq!(
        ep.select_due(&ctx(100), &seqs),
        None,
        "predecessor not done"
    );
    assert!(ep.chain_predecessor_done(i, 50));
    // Delay measured from 50, not from the first evaluation at 100.
    assert_eq!(ep.select_due(&ctx(53), &seqs), None);
    let mut busy = ctx(54);
    busy.endpoint_idle = false;
    assert_eq!(ep.select_due(&busy, &seqs), None);
    assert_eq!(ep.pending(i).unwrap().armed_at, 50);
    assert_eq!(ep.select_due(&ctx(54), &seqs), Some(i));
}

#[test]
//fusa:test REQ-SRV-039
fn chain_predecessor_done_changes_nothing_for_a_non_chained_index() {
    let mut ep = ServerEndpoint::new(true);
    let t = ep.admit(&timed(5, 1, 0), 9, None).index.unwrap();
    assert!(!ep.chain_predecessor_done(t, 0));
    assert_eq!(ep.pending(t).unwrap().armed_at, 9);
    assert!(!ep.chain_predecessor_done(t + 1, 0));
    assert!(!ep.chain_predecessor_done(SERVER_MAX_PENDING, 0));
}

#[test]
//fusa:test REQ-SRV-031
fn select_due_reports_nothing_when_nothing_qualifies() {
    let mut ep = ServerEndpoint::new(true);
    let seqs = SequencerBank::new(0);
    assert_eq!(ep.select_due(&ctx(0), &seqs), None);
    let _ = ep.admit(&timed(5, 1, 10), 0, None);
    assert_eq!(ep.select_due(&ctx(0), &seqs), None);
}

#[test]
//fusa:test REQ-SRV-007
//fusa:test REQ-SRV-032
fn safety_tagged_requests_wait_for_the_safe_state_others_do_not() {
    let mut ep = ServerEndpoint::new(true);
    let seqs = SequencerBank::new(1);
    let safety = ep
        .admit(
            &compound(
                RequestKind::SafetyCompound,
                5,
                1,
                step(0, 0, 0, 0, 0),
                0,
                vec![],
            ),
            0,
            None,
        )
        .index;
    let normal = ep
        .admit(
            &compound(RequestKind::Compound, 5, 2, step(0, 0, 0, 0, 0), 0, vec![]),
            0,
            None,
        )
        .index;
    assert_eq!(ep.select_due(&ctx(1), &seqs), normal);
    let mut safe = ctx(1);
    safe.in_safe_state = true;
    // Both due now; equal rank, the safety one arrived first.
    assert_eq!(ep.select_due(&safe, &seqs), safety);
    let _ = ep.cancel_single(1);
    assert_eq!(ep.select_due(&safe, &seqs), normal);
}

#[test]
//fusa:test REQ-SRV-008
fn select_due_returns_the_highest_priority_then_the_earliest() {
    let mut ep = ServerEndpoint::new(true);
    let seqs = SequencerBank::new(1);
    let chain = ep.admit(&chained(5, 1, 0, false), 0, None).index.unwrap();
    assert!(ep.chain_predecessor_done(chain, 0));
    let cmp1 = ep
        .admit(
            &compound(RequestKind::Compound, 5, 2, step(0, 0, 0, 0, 0), 0, vec![]),
            0,
            None,
        )
        .index;
    let _cmp2 = ep
        .admit(
            &compound(RequestKind::Compound, 5, 3, step(0, 0, 0, 0, 0), 0, vec![]),
            0,
            None,
        )
        .index;
    assert_eq!(ep.select_due(&ctx(0), &seqs), cmp1);
    let _ = ep.cancel_single(2);
    let _ = ep.cancel_single(3);
    assert_eq!(ep.select_due(&ctx(0), &seqs), Some(chain));
}

#[test]
//fusa:test REQ-SRV-015
//fusa:test REQ-SRV-016
fn a_disabled_endpoint_never_reports_a_conditional_request_due() {
    let mut ep = ServerEndpoint::new(true);
    let seqs = SequencerBank::new(0);
    let gate = PresentationGate {
        avtp_timestamp: 0,
        gptp_reference_now: 0,
    };
    let t = ep.admit(&timed(5, 1, 0), 0, None).index;
    ep.set_enable(false);
    let mut c = ctx(0);
    c.gptp_locked = true;
    assert_eq!(ep.select_due(&c, &seqs), None);
    // A TSCF-gated standard request is not gated by ep_enable (c-RCP parity).
    let s = ep.admit(&read_bytes(5, 2, 0), 0, Some(gate)).index;
    assert_eq!(ep.select_due(&c, &seqs), s);
    ep.set_enable(true);
    assert_eq!(ep.select_due(&c, &seqs), t);
}

// ── ServerEndpoint::complete (REQ-SRV-009, 010, 033..037) ──────────────────

#[test]
//fusa:test REQ-SRV-009
//fusa:test REQ-SRV-010
//fusa:test REQ-SRV-036
fn completing_a_compound_request_advances_its_sequencer_and_counts_down() {
    let mut ep = ServerEndpoint::new(true);
    let mut seqs = SequencerBank::new(1);
    let i = ep
        .admit(
            &compound(RequestKind::Compound, 5, 1, step(1, 4, 0, 0, 1), 0, vec![]),
            0,
            None,
        )
        .index
        .unwrap();
    assert_eq!(ep.select_due(&ctx(3), &seqs), Some(i));
    assert!(
        ep.complete(i, &ctx(3), &mut seqs),
        "repeat_count 1 -> 0, stays"
    );
    assert_eq!(seqs.read(0).unwrap(), SequencerState(4));
    let req = ep.pending(i).unwrap();
    assert_eq!(req.compound.repeat_count, 0);
    assert!(!req.armed);
    assert_eq!(req.armed_at, 3);
    // The advance guard: the sequencer already left start_state 1.
    seqs.force_state(0, SequencerState(1)).unwrap();
    assert!(
        !ep.complete(i, &ctx(4), &mut seqs),
        "repeat_count 0 removes"
    );
    assert_eq!(ep.pending_count(), 0);
    assert!(!ep.complete(i, &ctx(4), &mut seqs));
    assert!(!ep.complete(SERVER_MAX_PENDING, &ctx(4), &mut seqs));
}

#[test]
//fusa:test REQ-SRV-009
fn a_compound_completion_respects_the_advance_guard_and_next_state_zero() {
    let mut seqs = SequencerBank::new(1);
    let stay = step(1, 0, 0, 0, 0);
    assert!(compound_tick(&mut seqs, &stay, 0));
    assert_eq!(seqs.read(0).unwrap(), SequencerState(1));
    let move_on = step(1, 3, 0, 5, 0);
    assert!(!compound_tick(&mut seqs, &move_on, 4), "delay not elapsed");
    assert!(compound_tick(&mut seqs, &move_on, 5));
    assert!(
        !compound_tick(&mut seqs, &move_on, 5),
        "no longer in start_state"
    );
    assert!(
        !compound_tick(&mut seqs, &step(1, 0, 7, 0, 0), 0),
        "unknown sequencer"
    );
    seqs.force_state(0, SequencerState(0)).unwrap();
    assert!(
        !compound_tick(&mut seqs, &step(0, 3, 0, 0, 0), 0),
        "disabled sequencer"
    );
}

#[test]
//fusa:test REQ-SRV-033
//fusa:test REQ-SRV-035
fn completing_a_compound_wait_request_advances_only_when_matched() {
    let mut ep = ServerEndpoint::new(true);
    let mut seqs = SequencerBank::new(1);
    let i = ep
        .admit(
            &compound(
                RequestKind::CompoundWait,
                5,
                1,
                step(1, 2, 0, 0, COMPOUND_REPEAT_INFINITE),
                0x0,
                vec![0x5A],
            ),
            0,
            None,
        )
        .index
        .unwrap();
    let mut no_match = ctx(1);
    no_match.current_status = &[0x00];
    assert!(ep.complete(i, &no_match, &mut seqs));
    assert_eq!(seqs.read(0).unwrap(), SequencerState(1));
    let mut matched = ctx(2);
    matched.current_status = &[0x5A];
    assert!(ep.complete(i, &matched, &mut seqs));
    assert_eq!(seqs.read(0).unwrap(), SequencerState(2));
    assert_eq!(
        ep.pending(i).unwrap().compound.repeat_count,
        COMPOUND_REPEAT_INFINITE,
        "infinite is never decremented"
    );
}

#[test]
//fusa:test REQ-SRV-034
//fusa:test REQ-SRV-037
//fusa:test REQ-SRV-010
fn triggered_timed_and_chained_completions_advance_no_sequencer() {
    let mut ep = ServerEndpoint::new(true);
    let mut seqs = SequencerBank::new(1);
    let st = TriggeredStep {
        repeat_count: 2,
        ..TriggeredStep::default()
    };
    let trig = ep
        .admit(&triggered(RequestKind::Triggered, 5, 1, st), 0, None)
        .index
        .unwrap();
    let tim = ep.admit(&timed(5, 2, 0), 0, None).index.unwrap();
    let chn = ep.admit(&chained(5, 3, 0, false), 0, None).index.unwrap();
    let _ = ep.notify_trigger(0, 0);
    assert!(ep.complete(trig, &ctx(5), &mut seqs));
    let req = ep.pending(trig).unwrap();
    assert_eq!(req.triggered.repeat_count, 1);
    assert!(req.triggered_runtime.started);
    assert_eq!(req.triggered_runtime.occurrence_count, 0);
    assert!(!ep.complete(tim, &ctx(5), &mut seqs));
    assert!(!ep.complete(chn, &ctx(5), &mut seqs));
    assert_eq!(seqs.read(0).unwrap(), SequencerState(1));
    assert_eq!(ep.pending_count(), 1);
}

// ── Cancellation and purge (REQ-SRV-013, 014, 021, 040, 041, CANCEL-012) ───

fn store_with_mixed_requests() -> ServerEndpoint {
    let mut ep = ServerEndpoint::new(true);
    let _ = ep.admit(
        &compound(RequestKind::Compound, 5, 1, step(1, 1, 0, 0, 0), 0, vec![]),
        0,
        None,
    );
    let _ = ep.admit(
        &compound(
            RequestKind::SafetyCompound,
            5,
            2,
            step(1, 1, 0, 0, 0),
            0,
            vec![],
        ),
        0,
        None,
    );
    let _ = ep.admit(
        &triggered(RequestKind::SafetyTriggered, 5, 3, TriggeredStep::default()),
        0,
        None,
    );
    let _ = ep.admit(&timed(5, 4, 0), 0, None);
    ep
}

#[test]
//fusa:test REQ-SRV-013
//fusa:test REQ-SRV-021
fn cancel_all_empties_the_store() {
    let mut ep = store_with_mixed_requests();
    assert_eq!(ep.pending_count(), 4);
    assert_eq!(ep.cancel_all(), 4);
    assert_eq!(ep.pending_count(), 0);
    assert_eq!(ep.cancel_all(), 0);
}

#[test]
//fusa:test REQ-SRV-040
fn cancel_single_removes_only_its_target() {
    let mut ep = store_with_mixed_requests();
    assert_eq!(ep.cancel_single(4), CancelResult::Canceled);
    assert_eq!(ep.cancel_single(4), CancelResult::NotFound);
    assert_eq!(ep.pending_count(), 3);
}

#[test]
//fusa:test REQ-SRV-041
//fusa:test REQ-SRV-014
fn non_safestate_cancel_and_watchdog_purge_keep_only_the_safety_sequence() {
    let mut ep = store_with_mixed_requests();
    assert_eq!(ep.cancel_non_safestate(), 2);
    assert!(ep
        .pending
        .iter()
        .flatten()
        .all(|r| r.request_type & 0x80 != 0));
    let mut ep = store_with_mixed_requests();
    assert_eq!(ep.watchdog_purge(), 2);
    assert_eq!(ep.pending_count(), 2);
}

#[test]
//fusa:test REQ-CANCEL-012
fn cancel_chain_from_removes_a_chain_from_a_position_onward() {
    let mut ep = ServerEndpoint::new(true);
    for (tn, group, pos) in [(1u8, 7u32, 0u8), (2, 7, 1), (3, 7, 2), (4, 8, 1)] {
        let i = ep.admit(&chained(5, tn, 0, false), 0, None).index.unwrap();
        let req = ep.pending[i].as_mut().unwrap();
        req.chain_group = group;
        req.chain_position = pos;
    }
    assert_eq!(
        ep.cancel_chain_from(0, 0),
        0,
        "group 0 is the no-chain sentinel"
    );
    assert_eq!(ep.cancel_chain_from(7, 1), 2);
    assert_eq!(ep.pending_count(), 2);
    assert_eq!(ep.last_pending_index(), Some(3));
}

// ── RcServer dispatch: endpoint store (REQ-MOCK-010..018, 021, 027, 030) ───

#[test]
//fusa:test REQ-MOCK-019
//fusa:test REQ-MOCK-014
//fusa:test REQ-MOCK-021
fn dispatch_frame_dispatches_every_member_in_frame_order() {
    let (srv, ep) = server_with_endpoint(vec![0xAA, 0xBB]);
    let d = dispatch(
        &srv,
        stream(1),
        3,
        &[
            read_bytes(5, 1, 2),
            abb_bytes(5, true, 2, Evt::default(), vec![0x01]),
            read_bytes(5, 3, 2),
        ],
    );
    assert_eq!(d.requester, Some(stream(1)));
    assert_eq!(d.sequence_num, Some(3));
    assert_eq!(d.error, None);
    assert_eq!(d.members.len(), 3);
    assert!(d
        .members
        .iter()
        .all(|m| m.result == DispatchResult::Ok && m.byte_bus_id == 5));
    assert_eq!(
        decode_one(d.members[0].response.as_ref().unwrap()).payload,
        vec![0xAA, 0xBB]
    );
    assert_eq!(
        decode_one(d.members[2].response.as_ref().unwrap()).payload,
        vec![0x01]
    );
    assert_eq!(ep.read(4).unwrap(), vec![0x01]);
    let all = d.response_bytes();
    assert_eq!(crate::acf::decode_acf_abb_messages(&all).unwrap().len(), 3);
}

#[test]
//fusa:test REQ-MOCK-020
fn dispatch_frame_fails_safe_on_malformed_input() {
    let (srv, _ep) = server_with_endpoint(vec![0xAA]);
    let peer = Peer::from_header(Instant::now());
    let d = srv.dispatch_frame(&[0x00, 0x01], &peer);
    assert!(d.error.is_some() && d.members.is_empty() && d.requester.is_none());
    // A well-formed header with a malformed ACF stream dispatches nothing.
    let d = srv.dispatch_frame(&frame(stream(1), 0, &[vec![0xFE; 8]]), &peer);
    assert_eq!(d.error, Some(RcpError::ShortFrame));
    assert!(d.members.is_empty());
    // A GBB-typed member declaring fewer octets than a GBB header makes the
    // whole stream malformed: nothing at all is dispatched.
    let mut short_gbb = read_bytes(5, 1, 0);
    short_gbb[0] = ACF_GBB_MSG_TYPE << 1;
    short_gbb[1] = 2;
    let d = dispatch(&srv, stream(1), 0, &[short_gbb, read_bytes(5, 2, 1)]);
    assert_eq!(d.error, Some(RcpError::ShortFrame));
    assert!(d.members.is_empty());
}

#[test]
//fusa:test REQ-MOCK-020
fn an_undecodable_member_is_reported_without_stopping_the_frame() {
    let (srv, _ep) = server_with_endpoint(vec![0xAA]);
    // An ACF_ABB member whose payload/pad is inconsistent with its length.
    let mut bad = abb_bytes(5, true, 1, Evt::default(), vec![]);
    bad[2] |= 0xC0; // pad = 3 with no payload octets at all
    let d = dispatch(&srv, stream(1), 0, &[bad, read_bytes(5, 2, 1)]);
    assert_eq!(d.members.len(), 2);
    assert_eq!(d.members[0].result, DispatchResult::ErrUnknownBus);
    assert_eq!(d.members[0].byte_bus_id, 0);
    assert_eq!(d.members[0].response, None);
    assert_eq!(d.members[1].result, DispatchResult::Ok);
}

#[test]
//fusa:test REQ-MOCK-019
fn dispatch_frame_caps_the_member_count_as_c_rcp_does() {
    let (srv, _ep) = server_with_endpoint(vec![0xAA]);
    let members: Vec<Vec<u8>> = (0..MAX_FRAME_MEMBERS as u8 + 1)
        .map(|tn| read_bytes(5, tn, 1))
        .collect();
    let d = dispatch(&srv, stream(1), 0, &members);
    assert_eq!(d.members.len(), MAX_FRAME_MEMBERS - 1);
    let exactly: Vec<Vec<u8>> = (0..MAX_FRAME_MEMBERS as u8)
        .map(|tn| read_bytes(5, tn, 1))
        .collect();
    assert_eq!(
        dispatch(&srv, stream(1), 0, &exactly).members.len(),
        MAX_FRAME_MEMBERS
    );
}

#[test]
//fusa:test REQ-MOCK-013
//fusa:test REQ-MOCK-030
//fusa:test REQ-MOCK-031
fn an_unregistered_stream_and_byte_bus_id_pair_is_dropped_silently() {
    let (srv, _ep) = server_with_endpoint(vec![0xAA]);
    for (sid, bbid) in [(stream(1), 9u16), (stream(2), 5)] {
        let d = dispatch(&srv, sid, 0, &[read_bytes(bbid, 1, 1)]);
        assert_eq!(d.members[0].result, DispatchResult::ErrUnknownBus);
        assert_eq!(d.members[0].byte_bus_id, bbid);
        assert_eq!(d.members[0].response, None);
    }
    // An in-process peer overrides the header's stream id.
    let f = frame(stream(2), 0, &[read_bytes(5, 1, 1)]);
    let d = srv.dispatch_frame(&f, &Peer::stream(stream(1)));
    assert_eq!(d.requester, Some(stream(1)));
    assert_eq!(d.members[0].result, DispatchResult::Ok);
}

#[test]
//fusa:test REQ-MOCK-015
//fusa:test REQ-MOCK-010
//fusa:test REQ-MOCK-011
//fusa:test REQ-MOCK-016
//fusa:test REQ-MOCK-017
//fusa:test REQ-MOCK-018
fn a_disabled_endpoint_queues_until_re_enabled_and_drained() {
    let (srv, ep) = server_with_endpoint(vec![0x00]);
    let sid = stream(1);
    assert!(!srv.set_endpoint_enable(sid, 9, false));
    assert!(srv.set_endpoint_enable(sid, 5, false));
    let d = dispatch(
        &srv,
        sid,
        0,
        &[
            abb_bytes(5, true, 1, Evt::default(), vec![0x11]),
            abb_bytes(5, true, 2, ACK_EVT, vec![0x22]),
        ],
    );
    assert_eq!(d.members[0].result, DispatchResult::Queued);
    assert_eq!(d.members[0].response, None);
    let ack = decode_one(d.members[1].response.as_ref().unwrap());
    assert_eq!(ack.info.response_kind(), ResponseKind::Acknowledge);
    assert_eq!(srv.endpoint_queue_len(sid, 5), 2);
    assert_eq!(srv.endpoint_queue_len(sid, 9), 0);
    assert_eq!(ep.read(1).unwrap(), vec![0x00], "nothing executed");
    assert_eq!(srv.drain_endpoint(sid, 5), None, "still disabled");
    assert_eq!(srv.drain_endpoint(sid, 9), None, "unregistered");
    assert!(srv.set_endpoint_enable(sid, 5, true));
    let first = srv.drain_endpoint(sid, 5).unwrap();
    assert_eq!(
        decode_one(first.response.as_ref().unwrap())
            .info
            .transaction_num,
        1
    );
    assert_eq!(ep.read(1).unwrap(), vec![0x11]);
    assert!(srv.drain_endpoint(sid, 5).is_some());
    assert_eq!(ep.read(1).unwrap(), vec![0x22]);
    assert_eq!(srv.drain_endpoint(sid, 5), None, "empty");
}

#[test]
//fusa:test REQ-PWRMODE-028
fn a_suspended_endpoint_rejects_without_a_response() {
    let (srv, ep) = server_with_endpoint(vec![0x00]);
    assert!(srv.set_endpoint_admission_suspended(stream(1), 5, true));
    assert!(!srv.set_endpoint_admission_suspended(stream(1), 9, true));
    let d = dispatch(
        &srv,
        stream(1),
        0,
        &[abb_bytes(5, true, 1, ACK_EVT, vec![0x11])],
    );
    assert_eq!(d.members[0].result, DispatchResult::Rejected);
    assert_eq!(d.members[0].response, None);
    assert_eq!(ep.read(1).unwrap(), vec![0x00]);
}

#[test]
//fusa:test REQ-MOCK-027
//fusa:test REQ-MOCK-022
//fusa:test REQ-MOCK-023
//fusa:test REQ-MOCK-024
fn a_stored_request_executes_on_tick_against_the_servers_own_sequencers() {
    let (srv, ep) = server_with_endpoint(vec![0x00]);
    let sid = stream(1);
    srv.set_sequencer_count(2);
    assert_eq!(srv.general_registers().svr_sequencers_max, 2);
    srv.set_request_stream_cfg(&[cfg_for(sid)]).unwrap();
    srv.with_sequencers(|s| s.set_owner(1, 1)).unwrap();
    let mut req = compound(
        RequestKind::Compound,
        5,
        7,
        step(1, 2, 1, 0, 0),
        0,
        vec![0x42],
    );
    req[6] |= 0x80; // op = write
    let d = dispatch(&srv, sid, 0, &[req]);
    assert_eq!(d.members[0].result, DispatchResult::Pending);
    assert_eq!(srv.pending_count(sid, 5), 1);
    assert_eq!(srv.pending_count(sid, 9), 0);
    assert_eq!(srv.tick(sid, 9, &ctx(0)), None, "unregistered");
    let mut busy = ctx(0);
    busy.endpoint_idle = false;
    assert_eq!(srv.tick(sid, 5, &busy), None, "nothing due");
    let executed = srv.tick(sid, 5, &ctx(0)).unwrap();
    assert_eq!(
        decode_one(executed.response.as_ref().unwrap())
            .info
            .transaction_num,
        7
    );
    assert_eq!(ep.read(1).unwrap(), vec![0x42]);
    assert_eq!(
        srv.with_sequencers(|s| s.read(1).unwrap()),
        SequencerState(2)
    );
    assert_eq!(srv.pending_count(sid, 5), 0);
}

#[test]
//fusa:test REQ-MOCK-025
fn notify_trigger_reaches_every_endpoints_store() {
    let (srv, _ep) = server_with_endpoint(vec![0x00]);
    srv.register_endpoint(stream(2), 6, MockEndpoint::new(EndpointType::Gpio, vec![]))
        .unwrap();
    let st = TriggeredStep {
        trigger_source_ep: 4,
        trigger_signal_nr: 0,
        ..TriggeredStep::default()
    };
    let _ = dispatch(
        &srv,
        stream(1),
        0,
        &[triggered(RequestKind::Triggered, 5, 1, st)],
    );
    let _ = dispatch(
        &srv,
        stream(2),
        0,
        &[triggered(RequestKind::Triggered, 6, 1, st)],
    );
    let _ = dispatch(
        &srv,
        stream(1),
        1,
        &[triggered(RequestKind::Triggered, EP0_BYTE_BUS_ID, 2, st)],
    );
    assert_eq!(srv.notify_trigger(4, 0), 3);
    assert_eq!(srv.notify_trigger(4, 1), 0);
}

#[test]
//fusa:test REQ-MOCK-026
fn watchdog_purge_purges_one_endpoint() {
    let (srv, _ep) = server_with_endpoint(vec![0x00]);
    let _ = dispatch(
        &srv,
        stream(1),
        0,
        &[
            timed(5, 1, 0),
            triggered(RequestKind::SafetyTriggered, 5, 2, TriggeredStep::default()),
        ],
    );
    assert_eq!(srv.watchdog_purge(stream(1), 9), 0);
    assert_eq!(srv.watchdog_purge(stream(1), 5), 1);
    assert_eq!(srv.pending_count(stream(1), 5), 1);
}

#[test]
//fusa:test REQ-MOCK-028
//fusa:test REQ-CANCEL-012
fn cancellations_apply_immediately_and_clear_single_reports_not_found() {
    let (srv, _ep) = server_with_endpoint(vec![0x00]);
    let sid = stream(1);
    let _ = dispatch(
        &srv,
        sid,
        0,
        &[timed(5, 1, 0), timed(5, 2, 0), timed(5, 3, 0)],
    );
    let d = dispatch(&srv, sid, 1, &[clear_single(5, 9, 2)]);
    assert_eq!(d.members[0].result, DispatchResult::Cancelled);
    assert_eq!(d.members[0].response, None);
    assert_eq!(srv.pending_count(sid, 5), 2);
    let d = dispatch(&srv, sid, 2, &[clear_single(5, 9, 2)]);
    let resp = decode_one(d.members[0].response.as_ref().unwrap());
    assert!(resp.info.err && resp.info.rsp);
    assert_eq!(
        resp.info.transaction_num, 9,
        "the cancellation's own transaction_num"
    );
    assert_eq!(resp.info.evt, Evt::default());
    assert_eq!(resp.payload, vec![6]); // REQUEST_NOT_FOUND
    let ns = encode_clear_non_safestate(&ClearRequest {
        byte_bus_id: 5,
        transaction_num: 1,
    })
    .unwrap();
    let _ = dispatch(&srv, sid, 3, &[ns]);
    assert_eq!(srv.pending_count(sid, 5), 0);
    let _ = dispatch(&srv, sid, 4, &[timed(5, 1, 0), clear_all(5, 2)]);
    assert_eq!(srv.pending_count(sid, 5), 0);
    // An undecodable clear-single is ignored.
    let mut bad = clear_single(5, 1, 1);
    bad[4] |= 0x01; // cs set
    assert_eq!(dispatch(&srv, sid, 5, &[bad]).members[0].response, None);
}

#[test]
//fusa:test REQ-MOCK-029
//fusa:test REQ-CANCEL-012
fn chained_members_are_sequenced_in_frame_order() {
    let (srv, _ep) = server_with_endpoint(vec![0x00]);
    let sid = stream(1);
    // A chain with no predecessor: CHAIN_ERROR, and the rest aborted.
    let d = dispatch(
        &srv,
        sid,
        0,
        &[chained(5, 1, 0, false), chained(5, 2, 0, false)],
    );
    let e = decode_one(d.members[0].response.as_ref().unwrap());
    assert_eq!(
        (d.members[0].result, e.payload.clone()),
        (DispatchResult::ChainError, vec![17])
    );
    let a = decode_one(d.members[1].response.as_ref().unwrap());
    assert_eq!(
        (d.members[1].result, a.payload.clone()),
        (DispatchResult::ChainAborted, vec![16])
    );
    // A failed predecessor aborts a cs=1 successor, not a cs=0 one.
    let d = dispatch(
        &srv,
        sid,
        1,
        &[read_bytes(9, 1, 0), chained(5, 2, 0, false)],
    );
    assert_eq!(d.members[1].result, DispatchResult::Pending);
    let d = dispatch(&srv, sid, 2, &[read_bytes(9, 1, 0), chained(5, 3, 0, true)]);
    assert_eq!(d.members[1].result, DispatchResult::ChainAborted);
    // An accepted chain: group/position recorded, predecessor done.
    let d = dispatch(
        &srv,
        sid,
        3,
        &[
            timed(5, 10, 0),
            chained(5, 11, 0, true),
            chained(5, 12, 0, true),
        ],
    );
    assert!(d
        .members
        .iter()
        .all(|m| m.result == DispatchResult::Pending));
    let positions: Vec<(u8, u32, u8, bool)> = srv
        .core
        .lock()
        .unwrap()
        .slots
        .values()
        .next()
        .unwrap()
        .store
        .pending
        .iter()
        .flatten()
        .filter(|r| r.transaction_num >= 10)
        .map(|r| {
            (
                r.transaction_num,
                r.chain_group,
                r.chain_position,
                r.predecessor_done,
            )
        })
        .collect();
    assert!(positions.contains(&(10, 1, 0, false)));
    assert!(positions.contains(&(11, 1, 1, true)));
    assert!(positions.contains(&(12, 1, 2, true)));
    // Cancelling the anchor's successor cascades to the rest of the chain.
    let _ = dispatch(&srv, sid, 4, &[clear_single(5, 99, 11)]);
    let left: Vec<u8> = srv
        .core
        .lock()
        .unwrap()
        .slots
        .values()
        .next()
        .unwrap()
        .store
        .pending
        .iter()
        .flatten()
        .map(|r| r.transaction_num)
        .collect();
    assert!(left.contains(&10) && !left.contains(&11) && !left.contains(&12));
}

#[test]
//fusa:test REQ-SRV-016
//fusa:test REQ-ACF-033
fn storing_acknowledges_when_asked_and_a_rejection_uses_the_c_rcp_shape() {
    let (srv, _ep) = server_with_endpoint(vec![0x00]);
    let sid = stream(1);
    let mut asked = timed(5, 1, 0);
    asked[4] |= 0x80; // evt[3]
    srv.set_time_sync_supported(true);
    let d = dispatch(&srv, sid, 0, &[asked.clone()]);
    let ack = decode_one(d.members[0].response.as_ref().unwrap());
    assert_eq!(ack.info.response_kind(), ResponseKind::Acknowledge);
    assert!(!ack.info.err);
    // Fill the store; the next stored request asking for an ack gets an
    // Acknowledge-rejected REQUEST_STORAGE_OVERFLOW response.
    for tn in 2..=SERVER_MAX_PENDING as u8 {
        let _ = dispatch(&srv, sid, tn, &[timed(5, tn, 0)]);
    }
    let d = dispatch(&srv, sid, 0, &[asked]);
    let rej = decode_one(d.members[0].response.as_ref().unwrap());
    assert_eq!(d.members[0].result, DispatchResult::Rejected);
    assert_eq!(rej.info.response_kind(), ResponseKind::Acknowledge);
    assert!(rej.info.err);
    assert_eq!(rej.payload, vec![10]); // REQUEST_STORAGE_OVERFLOW
                                       // Without evt[3]: no response at all.
    let d = dispatch(&srv, sid, 0, &[timed(5, 77, 0)]);
    assert_eq!(d.members[0].response, None);
}

#[test]
//fusa:test REQ-SRV-022
fn a_reserved_compound_wait_evt_gets_an_unconditional_unsupported_cmd_error_response() {
    let (srv, _ep) = server_with_endpoint(vec![0x00]);
    let d = dispatch(
        &srv,
        stream(1),
        0,
        &[compound(
            RequestKind::CompoundWait,
            5,
            4,
            step(1, 2, 0, 0, 0),
            0x3,
            vec![],
        )],
    );
    let resp = decode_one(d.members[0].response.as_ref().unwrap());
    assert_eq!(resp.info.response_kind(), ResponseKind::Error);
    assert_eq!(
        (resp.info.transaction_num, resp.payload.clone()),
        (4, vec![1])
    );
}

#[test]
//fusa:test REQ-WIREERR-006
fn a_timed_request_without_time_sync_is_rejected_with_gptp_fail() {
    let (srv, _ep) = server_with_endpoint(vec![0x00]);
    srv.set_time_sync_supported(false);
    let d = dispatch(&srv, stream(1), 0, &[timed(5, 3, 0)]);
    assert_eq!(d.members[0].result, DispatchResult::Rejected);
    let resp = decode_one(d.members[0].response.as_ref().unwrap());
    assert_eq!(
        (resp.info.transaction_num, resp.payload.clone()),
        (3, vec![14])
    );
    assert_eq!(srv.pending_count(stream(1), 5), 0);
    srv.set_time_sync_supported(true);
    assert_eq!(
        dispatch(&srv, stream(1), 1, &[timed(5, 3, 0)]).members[0].result,
        DispatchResult::Pending
    );
}

#[test]
//fusa:test REQ-SEQ-013
//fusa:test REQ-WIREERR-005
fn a_compound_step_on_a_sequencer_its_stream_does_not_own_is_rejected() {
    let (srv, _ep) = server_with_endpoint(vec![0x00]);
    let sid = stream(1);
    srv.set_request_stream_cfg(&[cfg_for(sid)]).unwrap();
    srv.set_sequencer_count(2);
    let req = |tn, seq| {
        compound(
            RequestKind::Compound,
            5,
            tn,
            step(1, 2, seq, 0, 0),
            0,
            vec![],
        )
    };
    let d = dispatch(&srv, sid, 0, &[req(1, 0), req(2, 7)]);
    assert_eq!(
        decode_one(d.members[0].response.as_ref().unwrap()).payload,
        vec![3]
    );
    assert_eq!(
        decode_one(d.members[1].response.as_ref().unwrap()).payload,
        vec![2]
    );
    assert_eq!(srv.pending_count(sid, 5), 0);
    srv.with_sequencers(|s| s.set_owner(0, 1)).unwrap();
    assert_eq!(
        dispatch(&srv, sid, 1, &[req(3, 0)]).members[0].result,
        DispatchResult::Pending
    );
    // No sequencers at all: the gate is skipped.
    srv.set_sequencer_count(0);
    assert_eq!(
        dispatch(&srv, sid, 2, &[req(4, 7)]).members[0].result,
        DispatchResult::Pending
    );
}

// ── Per-stream safety state (REQ-E2E-*, WDG-010, RMAP-048/049, SRVCFG) ─────

#[test]
//fusa:test REQ-SRVCFG-001
//fusa:test REQ-SRVCFG-002
fn stream_configuration_tables_are_bounded_and_sync_their_capacity() {
    let srv = RcServer::new(GeneralRegisters::default());
    let rows = vec![cfg_for(stream(1)), cfg_for(stream(2))];
    srv.set_request_stream_cfg(&rows).unwrap();
    assert_eq!(srv.request_stream_cfg(), rows);
    assert_eq!(srv.general_registers().svr_request_stream_cfg.capacity, 2);
    assert_eq!(srv.request_stream_index(stream(2)), 2);
    assert_eq!(srv.request_stream_index(stream(3)), 0);
    let too_many = vec![cfg_for(stream(1)); REQUEST_STREAM_CFG_MAX_ENTRIES + 1];
    assert_eq!(
        srv.set_request_stream_cfg(&too_many).unwrap_err(),
        RcpError::InvalidSize
    );
    assert_eq!(srv.request_stream_cfg(), rows);

    let map = vec![EpByteBusIdMapEntry {
        map_stream_index: 2,
        map_ep_nr: 3,
        map_byte_bus_id: 5,
    }];
    srv.set_ep_id_map(&map).unwrap();
    assert_eq!(srv.ep_id_map(), map);
    assert_eq!(srv.general_registers().svr_ep_bytebus_id_map.capacity, 1);
    let too_many = vec![map[0]; EP_ID_MAP_MAX_ENTRIES + 1];
    assert_eq!(
        srv.set_ep_id_map(&too_many).unwrap_err(),
        RcpError::InvalidSize
    );
}

/// A server whose endpoint at `(stream(1), 5)` is bound to request stream 1
/// by EP_ID_config, with `cfg` as that stream's configuration.
fn configured(cfg: RequestStreamConfigEntry) -> (Arc<RcServer>, Arc<MockEndpoint>) {
    let (srv, ep) = server_with_endpoint(vec![0x00]);
    srv.set_request_stream_cfg(&[cfg]).unwrap();
    srv.set_ep_id_map(&[EpByteBusIdMapEntry {
        map_stream_index: 1,
        map_ep_nr: 1,
        map_byte_bus_id: 5,
    }])
    .unwrap();
    srv.set_time_sync_supported(true);
    (srv, ep)
}

#[test]
//fusa:test REQ-E2E-028
fn the_sequence_gate_rejects_a_replayed_frame_whole() {
    let sid = stream(1);
    let (srv, _ep) = configured(RequestStreamConfigEntry {
        rx_enforce_seq: true,
        ..cfg_for(sid)
    });
    assert_eq!(
        dispatch(&srv, sid, 10, &[read_bytes(5, 1, 1)]).members[0].result,
        DispatchResult::Ok
    );
    let d = dispatch(&srv, sid, 10, &[read_bytes(5, 2, 1), read_bytes(5, 3, 1)]);
    assert_eq!(d.members.len(), 2);
    assert!(d
        .members
        .iter()
        .all(|m| m.result == DispatchResult::SeqError && m.response.is_none()));
    assert_eq!(
        dispatch(&srv, sid, 11, &[read_bytes(5, 4, 1)]).members[0].result,
        DispatchResult::Ok
    );
    // Not enforced on an unconfigured stream.
    assert_eq!(
        dispatch(&srv, stream(9), 0, &[read_bytes(5, 1, 1)])
            .members
            .len(),
        1
    );
    // c-RCP parity: the latch is set only via rx_seq_safestate_enable.
    assert_eq!(srv.stream_status(sid), Some(StreamStatus::default()));
}

#[test]
//fusa:test REQ-E2E-029
//fusa:test REQ-MOCK-033
//fusa:test REQ-E2E-046
fn a_sequence_gap_enters_safe_state_and_latches_but_does_not_block() {
    let sid = stream(1);
    let (srv, _ep) = configured(RequestStreamConfigEntry {
        rx_seq_safestate_enable: true,
        ..cfg_for(sid)
    });
    let _ = dispatch(
        &srv,
        sid,
        1,
        &[
            timed(5, 1, 0),
            triggered(RequestKind::SafetyTriggered, 5, 2, TriggeredStep::default()),
        ],
    );
    assert_eq!(srv.pending_count(sid, 5), 2);
    let d = dispatch(&srv, sid, 5, &[read_bytes(5, 3, 1)]);
    assert_eq!(
        d.members[0].result,
        DispatchResult::Ok,
        "the latch never blocks"
    );
    assert_eq!(
        srv.pending_count(sid, 5),
        1,
        "only the safety sequence survives"
    );
    let status = srv.stream_status(sid).unwrap();
    assert!(status.seq_blocked && status.rx_blocked());
    assert!(srv.stream_status_rx_blocked(sid));
    assert!(!srv.stream_status_rx_blocked(stream(9)));
    assert_eq!(
        dispatch(&srv, sid, 6, &[read_bytes(5, 4, 1)]).members[0].result,
        DispatchResult::Ok
    );
    srv.release_stream(sid);
    assert!(!srv.stream_status_rx_blocked(sid));
}

#[test]
//fusa:test REQ-E2E-030
//fusa:test REQ-E2E-046
fn a_storage_overflow_enters_safe_state_on_the_whole_stream_when_enabled() {
    let sid = stream(1);
    for enable in [false, true] {
        let (srv, _ep) = configured(RequestStreamConfigEntry {
            rx_ovrflw_safestate_enable: enable,
            ..cfg_for(sid)
        });
        srv.register_endpoint(sid, 6, MockEndpoint::new(EndpointType::Gpio, vec![]))
            .unwrap();
        srv.set_ep_id_map(&[
            EpByteBusIdMapEntry {
                map_stream_index: 1,
                map_ep_nr: 1,
                map_byte_bus_id: 5,
            },
            EpByteBusIdMapEntry {
                map_stream_index: 1,
                map_ep_nr: 2,
                map_byte_bus_id: 6,
            },
            EpByteBusIdMapEntry {
                map_stream_index: 1,
                map_ep_nr: 2,
                map_byte_bus_id: 6,
            },
        ])
        .unwrap();
        let _ = dispatch(&srv, sid, 0, &[timed(6, 1, 0)]);
        for tn in 0..SERVER_MAX_PENDING as u8 {
            let _ = dispatch(&srv, sid, tn, &[timed(5, tn, 0)]);
        }
        let d = dispatch(&srv, sid, 0, &[timed(5, 99, 0)]);
        assert_eq!(d.members[0].result, DispatchResult::Rejected);
        let expected = if enable { 0 } else { SERVER_MAX_PENDING };
        assert_eq!(srv.pending_count(sid, 5), expected);
        assert_eq!(
            srv.pending_count(sid, 6),
            usize::from(!enable),
            "the sibling endpoint too"
        );
        assert_eq!(srv.stream_status(sid).unwrap().overflow_blocked, enable);
    }
    // An unconfigured stream overflows without escalation.
    let (srv, _ep) = server_with_endpoint(vec![]);
    srv.set_time_sync_supported(true);
    for tn in 0..=SERVER_MAX_PENDING as u8 {
        let _ = dispatch(&srv, stream(1), tn, &[timed(5, tn, 0)]);
    }
    assert_eq!(srv.pending_count(stream(1), 5), SERVER_MAX_PENDING);
}

#[test]
//fusa:test REQ-RMAP-048
//fusa:test REQ-RMAP-049
fn responses_are_suppressed_when_their_stream_pointer_is_zero() {
    let sid = stream(1);
    let (srv, _ep) = configured(RequestStreamConfigEntry {
        rx_ack_stream_index: 0,
        rx_resp_stream_index: 1,
        ..cfg_for(sid)
    });
    let d = dispatch(&srv, sid, 0, &[read_bytes(5, 1, 1), timed(5, 2, 0)]);
    assert!(d.members[0].response.is_some());
    srv.set_endpoint_enable(sid, 5, false);
    let d = dispatch(&srv, sid, 1, &[abb_bytes(5, false, 3, ACK_EVT, vec![])]);
    assert_eq!(d.members[0].result, DispatchResult::Queued);
    assert_eq!(d.members[0].response, None, "acknowledge suppressed");
    srv.set_request_stream_cfg(&[RequestStreamConfigEntry {
        rx_ack_stream_index: 1,
        rx_resp_stream_index: 0,
        ..cfg_for(sid)
    }])
    .unwrap();
    srv.set_endpoint_enable(sid, 5, true);
    let d = dispatch(&srv, sid, 2, &[read_bytes(5, 4, 1)]);
    assert_eq!(d.members[0].response, None, "read response suppressed");
    srv.set_endpoint_enable(sid, 5, false);
    let d = dispatch(&srv, sid, 3, &[abb_bytes(5, false, 5, ACK_EVT, vec![])]);
    assert!(d.members[0].response.is_some(), "acknowledge routed");
}

#[test]
//fusa:test REQ-E2E-021
//fusa:test REQ-E2E-045
//fusa:test REQ-E2E-046
fn a_crc_failure_under_rx_enforce_e2e_blocks_the_stream_until_released() {
    let sid = stream(1);
    let (srv, ep) = configured(RequestStreamConfigEntry {
        rx_enforce_e2e: true,
        ..cfg_for(sid)
    });
    let _ = dispatch(&srv, sid, 0, &[timed(5, 1, 0)]);
    assert_eq!(
        srv.note_crc_error(sid),
        1,
        "safe state purged the stored request"
    );
    assert!(srv.stream_faulted(sid));
    assert!(srv.stream_status(sid).unwrap().crc_faulted);
    let d = dispatch(
        &srv,
        sid,
        1,
        &[abb_bytes(5, true, 6, Evt::default(), vec![0x77])],
    );
    assert_eq!(d.members[0].result, DispatchResult::StreamFaulted);
    let resp = decode_one(d.members[0].response.as_ref().unwrap());
    assert_eq!(
        (resp.info.transaction_num, resp.payload.clone()),
        (6, vec![12])
    ); // POCI_FAILURE
    assert_eq!(ep.read(1).unwrap(), vec![0x00], "not executed");
    srv.release_stream(sid);
    assert!(!srv.stream_faulted(sid));
    assert_eq!(
        dispatch(&srv, sid, 2, &[read_bytes(5, 7, 1)]).members[0].result,
        DispatchResult::Ok
    );
}

#[test]
//fusa:test REQ-E2E-022
//fusa:test REQ-E2E-023
fn a_crc_failure_without_rx_enforce_e2e_only_drops_the_request() {
    let sid = stream(1);
    let (srv, _ep) = configured(cfg_for(sid));
    let _ = dispatch(&srv, sid, 0, &[timed(5, 1, 0)]);
    assert_eq!(srv.note_crc_error(sid), 0);
    assert!(!srv.stream_faulted(sid));
    assert_eq!(srv.stream_status(sid), Some(StreamStatus::default()));
    assert_eq!(srv.pending_count(sid, 5), 1);
    // An unconfigured stream: drop only.
    assert_eq!(srv.note_crc_error(stream(9)), 0);
    assert!(!srv.stream_faulted(stream(9)));
}

#[test]
//fusa:test REQ-E2E-021
fn the_crc_fault_tracker_is_bounded() {
    let mut t = StreamFaultTracker::default();
    for id in 0..STREAM_FAULT_TRACKER_MAX_STREAMS as u64 {
        assert!(t.on_crc_error(id, false));
    }
    assert!(!t.on_crc_error(999, true), "full: unchanged");
    assert!(!t.is_faulted(999));
    assert!(t.on_crc_error(3, true));
    assert!(t.is_faulted(3));
    t.reset(3);
    assert!(!t.is_faulted(3));
}

#[test]
//fusa:test REQ-WDG-010
//fusa:test REQ-E2E-046
fn the_watchdog_is_kicked_per_request_and_evaluated_against_the_stream_config() {
    let sid = stream(1);
    let (srv, _ep) = configured(RequestStreamConfigEntry {
        rx_wd_enable: true,
        rx_wd_timeout_interval: 100,
        rx_wd_safestate_enable: true,
        rx_wd_info_enable: true,
        ..cfg_for(sid)
    });
    assert_eq!(srv.last_watchdog_kick(1), None);
    assert_eq!(srv.last_watchdog_kick(0), None);
    let now = Instant::now();
    let _ = srv.dispatch_frame(&frame(sid, 0, &[timed(5, 1, 0)]), &Peer::from_header(now));
    assert_eq!(srv.last_watchdog_kick(1), Some(now));
    assert_eq!(srv.check_watchdog(1, 99), Some(WatchdogCheck::default()));
    assert_eq!(srv.pending_count(sid, 5), 1);
    let fired = srv.check_watchdog(1, 100).unwrap();
    assert!(fired.overflowed && fired.enter_safe_state && fired.notify);
    assert_eq!(srv.pending_count(sid, 5), 0, "safe state purged the stream");
    assert!(srv.stream_status(sid).unwrap().wd_blocked);
    assert_eq!(srv.check_watchdog(2, 1000), None);
    assert_eq!(srv.check_watchdog(0, 1000), None);
    // Disabled watchdog never overflows; info without safe state latches nothing.
    srv.set_request_stream_cfg(&[RequestStreamConfigEntry {
        rx_wd_enable: true,
        rx_wd_timeout_interval: 1,
        rx_wd_info_enable: true,
        ..cfg_for(sid)
    }])
    .unwrap();
    srv.release_stream(sid);
    let info = srv.check_watchdog(1, 5).unwrap();
    assert!(info.overflowed && info.notify && !info.enter_safe_state);
    assert!(!srv.stream_status(sid).unwrap().wd_blocked);
    srv.set_request_stream_cfg(&[cfg_for(sid)]).unwrap();
    assert_eq!(
        srv.check_watchdog(1, u64::MAX),
        Some(WatchdogCheck::default())
    );
}

#[test]
//fusa:test REQ-MOCK-033
fn broadcast_safe_state_resolves_bound_endpoints_on_the_streams_own_id() {
    let sid = stream(1);
    let (srv, _ep) = configured(cfg_for(sid));
    // The same byte_bus_id on another stream is not purged.
    srv.register_endpoint(stream(2), 5, MockEndpoint::new(EndpointType::Gpio, vec![]))
        .unwrap();
    let _ = dispatch(&srv, sid, 0, &[timed(5, 1, 0)]);
    let _ = dispatch(&srv, stream(2), 0, &[timed(5, 1, 0)]);
    assert_eq!(srv.broadcast_safe_state(0), 0);
    assert_eq!(srv.broadcast_safe_state(1), 1);
    assert_eq!(srv.pending_count(stream(2), 5), 1);
    // A row naming an unconfigured request stream falls back to the
    // first registered slot at that byte_bus_id (c-RCP's unscoped find_slot).
    srv.set_ep_id_map(&[
        EpByteBusIdMapEntry {
            map_stream_index: 7,
            map_ep_nr: 1,
            map_byte_bus_id: 5,
        },
        EpByteBusIdMapEntry {
            map_stream_index: 7,
            map_ep_nr: 0,
            map_byte_bus_id: EP0_BYTE_BUS_ID,
        },
        EpByteBusIdMapEntry {
            map_stream_index: 7,
            map_ep_nr: 2,
            map_byte_bus_id: 44,
        },
    ])
    .unwrap();
    let _ = dispatch(&srv, sid, 1, &[timed(5, 2, 0)]);
    assert_eq!(srv.broadcast_safe_state(7), 1);
    assert_eq!(srv.pending_count(sid, 5), 0);
    assert_eq!(srv.pending_count(stream(2), 5), 1);
}

#[test]
//fusa:test REQ-E2E-046
fn stream_status_aggregates_any_latch() {
    let mut s = StreamStatus::default();
    assert!(!s.rx_blocked());
    for f in [
        |s: &mut StreamStatus| s.crc_faulted = true,
        |s: &mut StreamStatus| s.seq_blocked = true,
        |s: &mut StreamStatus| s.wd_blocked = true,
        |s: &mut StreamStatus| s.overflow_blocked = true,
    ] {
        s = StreamStatus::default();
        f(&mut s);
        assert!(s.rx_blocked());
    }
    assert_eq!(
        RcServer::new(GeneralRegisters::default()).stream_status(stream(1)),
        None
    );
}

// ── Discovery through the pipeline (REQ-UDP-009..011) ───────────────────────

#[test]
//fusa:test REQ-UDP-009
//fusa:test REQ-UDP-010
//fusa:test REQ-UDP-011
fn discovery_is_answered_inside_the_one_dispatch_pipeline() {
    let srv = RcServer::new(GeneralRegisters {
        svr_vendor_id: 0x0102,
        ..Default::default()
    });
    let broadcast = discovery::DISCOVERY_BROADCAST_STREAM_ID;
    let read = encode_acf_abb(&discovery::build_discovery_request(0x11)).unwrap();
    let d = dispatch(&srv, broadcast, 0, &[read]);
    assert_eq!(d.members[0].result, DispatchResult::Ok);
    let regs =
        GeneralRegisters::decode(&decode_one(d.members[0].response.as_ref().unwrap()).payload)
            .unwrap();
    assert_eq!(regs.svr_vendor_id, 0x0102);

    let mut configure = discovery::build_discovery_request(0);
    configure.info.op = true;
    let configure = encode_acf_abb(&configure).unwrap();
    srv.set_discovery_timeout(Duration::from_secs(60));
    assert_eq!(
        dispatch(&srv, stream(5), 0, std::slice::from_ref(&configure)).members[0].result,
        DispatchResult::Ok
    );
    assert_eq!(srv.discovery_claim().unwrap().claimant(), stream(5));
    let d = dispatch(&srv, stream(6), 0, &[configure]);
    assert_eq!(d.members[0].result, DispatchResult::Rejected);
    assert_eq!(
        decode_one(d.members[0].response.as_ref().unwrap()).payload,
        vec![3]
    );

    // Anything else under the broadcast sentinel, ABB or GBB: INVALID_PARAMETER.
    for m in [read_bytes(7, 1, 0), timed(7, 2, 0)] {
        let d = dispatch(&srv, broadcast, 0, &[m]);
        assert_eq!(d.members[0].result, DispatchResult::Rejected);
        assert_eq!(
            decode_one(d.members[0].response.as_ref().unwrap()).payload,
            vec![15]
        );
    }
}

// ── dispatch_message (REQ-MOCKSRV-011) ──────────────────────────────────────

#[test]
//fusa:test REQ-MOCKSRV-011
fn dispatch_message_routes_an_in_process_request_through_dispatch_frame() {
    let (srv, ep) = server_with_endpoint(vec![1, 2, 3]);
    let sid = stream(1);
    let mut read = abb_request(5, false, Vec::new());
    read.info.read_size_segment = ReadSizeOrSegment(u16::MAX); // clamped to 12 bits
    assert_eq!(
        srv.dispatch_message(sid, &read).unwrap().payload,
        vec![1, 2, 3]
    );
    srv.dispatch_message(sid, &abb_request(5, true, vec![9]))
        .unwrap();
    assert_eq!(ep.read(4).unwrap(), vec![9]);
    // An err=1 response becomes the matching error.
    let write = abb_request(EP0_BYTE_BUS_ID, true, vec![0x12, 0x34]);
    assert_eq!(
        srv.dispatch_message(sid, &write).unwrap_err(),
        RcpError::LockedMemAccess
    );
    // No response at all: NotConnected.
    assert_eq!(
        srv.dispatch_message(sid, &abb_request(9, false, Vec::new()))
            .unwrap_err(),
        RcpError::NotConnected
    );
    // Too large for one ACF message.
    assert_eq!(
        srv.dispatch_message(sid, &abb_request(5, true, vec![0; 2037]))
            .unwrap_err(),
        RcpError::PayloadTooLarge
    );
    // Not encodable (byte_bus_id beyond 11 bits).
    assert_eq!(
        srv.dispatch_message(sid, &abb_request(0x0800, false, Vec::new()))
            .unwrap_err(),
        RcpError::InvalidSize
    );
    // A queued request's Acknowledge is returned as is.
    srv.set_endpoint_enable(sid, 5, false);
    let mut acked = abb_request(5, true, vec![1]);
    acked.info.evt = ACK_EVT;
    let ack = srv.dispatch_message(sid, &acked).unwrap();
    assert_eq!(ack.info.response_kind(), ResponseKind::Acknowledge);
}

#[test]
//fusa:test REQ-MOCKSRV-011
fn dispatch_message_keeps_one_sequence_counter_per_request_stream() {
    // Regression: one server-wide counter gave stream A a gap whenever a
    // call on stream B came between two calls on A, latching seq_blocked
    // and broadcasting safe state on A.
    let (a, b) = (stream(1), stream(2));
    let (srv, _ep) = configured(cfg_for(a));
    srv.set_request_stream_cfg(&[
        RequestStreamConfigEntry {
            rx_seq_safestate_enable: true,
            rx_enforce_seq: true,
            ..cfg_for(a)
        },
        RequestStreamConfigEntry {
            rx_seq_safestate_enable: true,
            rx_enforce_seq: true,
            ..cfg_for(b)
        },
    ])
    .unwrap();
    let read = abb_request(5, false, Vec::new());
    for sid in [a, a, b, a, b, b, a] {
        let _ = srv.dispatch_message(sid, &read);
    }
    assert_eq!(srv.stream_status(a), Some(StreamStatus::default()));
    assert_eq!(srv.stream_status(b), Some(StreamStatus::default()));
    // Stream A still answers: no frame of it was rejected by the gate.
    assert!(srv.dispatch_message(a, &read).is_ok());
}

#[test]
//fusa:test REQ-MOCKSRV-011
fn concurrent_dispatch_message_calls_reach_the_sequence_gate_in_order() {
    // Regression: the number was taken before the core lock, so two
    // concurrent callers on one stream could reach the gate out of order
    // and the lower number was rejected whole under rx_enforce_seq.
    let sid = stream(1);
    let (srv, _ep) = configured(RequestStreamConfigEntry {
        rx_enforce_seq: true,
        ..cfg_for(sid)
    });
    let read = abb_request(5, false, Vec::new());
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let srv = Arc::clone(&srv);
            let read = read.clone();
            std::thread::spawn(move || {
                (0..300)
                    .map(|_| srv.dispatch_message(sid, &read))
                    .filter(Result::is_err)
                    .count()
            })
        })
        .collect();
    let failures: usize = threads.into_iter().map(|t| t.join().unwrap()).sum();
    assert_eq!(failures, 0, "a frame was rejected by the sequence gate");
    assert_eq!(srv.stream_status(sid), Some(StreamStatus::default()));
}

#[test]
//fusa:test REQ-MOCKSRV-011
fn wire_error_codes_map_back_to_their_errors() {
    for code in 1u8..=17 {
        let e = error_from_wire_code(Some(code));
        assert_eq!(e.tc18_wire_code(), Some(code));
    }
    assert!(matches!(error_from_wire_code(Some(0)), RcpError::Other(_)));
    assert!(matches!(error_from_wire_code(Some(18)), RcpError::Other(_)));
    assert!(matches!(error_from_wire_code(None), RcpError::Other(_)));
    assert!(message_from_dispatch(FrameDispatch::failed(RcpError::ShortFrame)).is_err());
    assert_eq!(
        message_from_dispatch(FrameDispatch::default()).unwrap_err(),
        RcpError::NotConnected
    );
}

#[test]
//fusa:test REQ-MOCK-031
fn peer_constructors_set_the_identity_source() {
    let now = Instant::now();
    assert_eq!(Peer::from_header(now).stream_id, None);
    assert_eq!(Peer::from_header(now).received_at, now);
    assert_eq!(Peer::stream(stream(3)).stream_id, Some(stream(3)));
}

#[test]
//fusa:test REQ-MOCK-024
fn a_gated_cancellation_is_applied_when_it_becomes_due_on_tick() {
    let (srv, _ep) = server_with_endpoint(vec![]);
    let sid = stream(1);
    srv.set_time_sync_supported(true);
    let _ = dispatch(&srv, sid, 0, &[timed(5, 1, 0), timed(5, 2, 0)]);
    // Store a cancellation directly under a TSCF gate (no TSCF ingress yet).
    {
        let key = srv.store_key(sid, 5).unwrap();
        let mut core = srv.core.lock().unwrap();
        let gate = PresentationGate {
            avtp_timestamp: 0,
            gptp_reference_now: 0,
        };
        let a = core
            .store_mut(key)
            .unwrap()
            .admit(&clear_single(5, 3, 1), 0, Some(gate));
        assert_eq!(a.result, AdmitResult::Pending);
    }
    let mut c = ctx(0);
    c.gptp_locked = true;
    let done = srv.tick(sid, 5, &c).unwrap();
    assert_eq!(done.response, None);
    assert_eq!(
        srv.pending_count(sid, 5),
        1,
        "target removed, cancellation released"
    );
}

#[test]
//fusa:test REQ-MOCKSRV-006
fn a_gbb_standard_request_executes_through_the_handler() {
    let (srv, ep) = server_with_endpoint(vec![0x10, 0x20]);
    let gbb = encode_acf_gbb(&AcfGbbMessage {
        info: ByteMessageInfo {
            byte_bus_id: 5,
            mtv: true,
            read_size_segment: ReadSizeOrSegment(1),
            transaction_num: 8,
            ..Default::default()
        },
        message_timestamp: 1234,
        payload: Vec::new(),
    })
    .unwrap();
    let d = dispatch(&srv, stream(1), 0, &[gbb]);
    let resp = decode_one(d.members[0].response.as_ref().unwrap());
    assert_eq!((resp.info.transaction_num, resp.payload), (8, vec![0x10]));
    assert_eq!(ep.read(2).unwrap(), vec![0x10, 0x20]);
    assert_eq!(request_message(&[0x1C]), None);
}

// ── c-RCP golden response vectors ──────────────────────────────────────────

/// Byte-for-byte parity of this module's response builders with c-RCP's
/// `rcp_acf_build_acknowledge_response` / `_error_response` /
/// `_acknowledge_rejected_response` (`c-RCP/src/acf.c:298-360`), against
/// vectors generated by a C harness linked to c-RCP's real `librcp.a`
/// (`testdata/c_rcp_server_response_golden.txt`).
#[test]
//fusa:test REQ-ACF-033
//fusa:test REQ-WIREERR-006
//fusa:test REQ-MOCK-029
fn response_builders_match_c_rcp_golden_vectors() {
    let golden = include_str!("../../testdata/c_rcp_server_response_golden.txt");
    let mut checked = 0;
    for line in golden.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = line.split_whitespace().collect();
        let [kind, bb, tn, code, hex] = f[..] else {
            panic!("malformed golden line {line:?}");
        };
        let bb: u16 = bb.parse().unwrap();
        let tn: u8 = tn.parse().unwrap();
        let code: u8 = code.parse().unwrap();
        let want: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        let error = error_from_wire_code(Some(code));
        let got = match kind {
            "ack" => acknowledge_response(bb, tn),
            "err" => wire_error_response(bb, tn, &error),
            "ackrej" => acknowledge_rejected_response(bb, tn, &error),
            other => panic!("unknown builder {other}"),
        };
        assert_eq!(got.as_deref(), Some(&want[..]), "{line}");
        checked += 1;
    }
    assert_eq!(checked, 9 * (1 + 2 * 17));
}

// ── Response routing / response-queue config + heartbeat (issue #195) ───────

fn queue_row(stream_uid: u16, flush_time: u16) -> ResponseStreamConfigEntry {
    ResponseStreamConfigEntry {
        resp_stream_uid: stream_uid,
        resp_flush_time: flush_time,
        ..ResponseStreamConfigEntry::default()
    }
}

#[test]
//fusa:test REQ-RMAP-034
fn set_response_queue_cfg_bounds_the_table_and_syncs_capacity() {
    let srv = RcServer::new(GeneralRegisters::default());
    let rows = vec![queue_row(1, 0); RESPONSE_QUEUE_CFG_MAX_ENTRIES + 1];
    assert_eq!(
        srv.set_response_queue_cfg(&rows),
        Err(RcpError::InvalidSize)
    );
    assert!(srv.response_queue_cfg().is_empty());
    srv.set_response_queue_cfg(&rows[..2]).unwrap();
    assert_eq!(srv.response_queue_cfg(), rows[..2].to_vec());
    assert_eq!(srv.general_registers().svr_response_stream_cfg.capacity, 2);
    srv.set_response_queue_cfg(&rows[..1]).unwrap();
    assert_eq!(srv.general_registers().svr_response_stream_cfg.capacity, 1);
}

#[test]
//fusa:test REQ-SRVCFG-003
//fusa:test REQ-RMAP-048
//fusa:test REQ-RMAP-049
fn response_route_follows_the_ack_and_resp_stream_pointers() {
    let sid = stream(1);
    let srv = RcServer::new(GeneralRegisters::default());
    let mut resp_msg = abb_request(5, false, vec![]);
    resp_msg.info.rsp = true;
    let resp = encode_acf_abb(&resp_msg).unwrap();
    resp_msg.info.evt = Evt {
        ack: true,
        sub_opcode: 0x07,
    };
    let ack_resp = encode_acf_abb(&resp_msg).unwrap();
    // Unconfigured request stream: straight back, whatever the response.
    assert_eq!(srv.response_route(sid, &resp), ResponseRoute::Direct);
    srv.set_request_stream_cfg(&[RequestStreamConfigEntry {
        rx_ack_stream_index: 2,
        rx_resp_stream_index: 1,
        ..cfg_for(sid)
    }])
    .unwrap();
    // Pointers set, but no response-queue rows installed: Direct.
    assert_eq!(srv.response_route(sid, &resp), ResponseRoute::Direct);
    srv.set_response_queue_cfg(&[queue_row(0x10, 0), queue_row(0x20, 0)])
        .unwrap();
    assert_eq!(srv.response_route(sid, &resp), ResponseRoute::Queue(1));
    assert_eq!(srv.response_route(sid, &ack_resp), ResponseRoute::Queue(2));
    assert_eq!(srv.response_route(sid, &[0u8; 3]), ResponseRoute::Direct);
    assert_eq!(srv.response_route(stream(9), &resp), ResponseRoute::Direct);
    srv.set_response_queue_cfg(&[queue_row(0x10, 0)]).unwrap();
    assert_eq!(
        srv.response_route(sid, &ack_resp),
        ResponseRoute::Direct,
        "a pointer past the installed rows has no queue"
    );
    srv.set_request_stream_cfg(&[RequestStreamConfigEntry {
        rx_ack_stream_index: 0,
        rx_resp_stream_index: 0,
        ..cfg_for(sid)
    }])
    .unwrap();
    assert_eq!(srv.response_route(sid, &resp), ResponseRoute::Suppress);
    assert_eq!(srv.response_route(sid, &ack_resp), ResponseRoute::Suppress);
}

/// Golden vectors from c-RCP's real `rcp_mock_server_check_response_queue_heartbeat()`
/// (`mock.c:483`): two rows `{stream_uid 0x0B0C, flush_time 100}` and
/// `{stream_uid 0x0002, flush_time 0}`, mac `02:AA:BB:CC:DD:EE`.
#[test]
//fusa:test REQ-RMAP-065
//fusa:test REQ-SRV-017
fn response_queue_heartbeat_matches_c_rcp_golden_vectors() {
    let srv = RcServer::new(GeneralRegisters::default());
    srv.set_response_queue_cfg(&[queue_row(0x0B0C, 100), queue_row(0x0002, 0)])
        .unwrap();
    let mac = [0x02, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE];
    assert_eq!(
        srv.check_response_queue_heartbeat(1, mac, 1000),
        None,
        "seed"
    );
    assert_eq!(srv.check_response_queue_heartbeat(1, mac, 1099), None);
    assert_eq!(
        srv.check_response_queue_heartbeat(1, mac, 1100),
        Some(vec![
            0x82, 0x80, 0x00, 0x00, 0x02, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0x0B, 0x0C
        ])
    );
    assert_eq!(
        srv.check_response_queue_heartbeat(1, mac, 1150),
        None,
        "timer reset"
    );
    assert_eq!(
        srv.check_response_queue_heartbeat(1, mac, 900),
        None,
        "non-monotonic"
    );
    assert_eq!(srv.check_response_queue_heartbeat(0, mac, 5000), None);
    assert_eq!(srv.check_response_queue_heartbeat(3, mac, 5000), None);
    assert_eq!(srv.check_response_queue_heartbeat(2, mac, 0), None);
    assert_eq!(
        srv.check_response_queue_heartbeat(2, mac, 1_000_000),
        None,
        "Flush_time 0 never fires"
    );
}

#[test]
//fusa:test REQ-RMAP-064
fn a_noted_transmission_restarts_the_flush_time_interval() {
    let srv = RcServer::new(GeneralRegisters::default());
    srv.set_response_queue_cfg(&[queue_row(1, 100)]).unwrap();
    let mac = [0x02; 6];
    srv.note_response_queue_transmit(1, 1000);
    srv.note_response_queue_transmit(0, 0);
    srv.note_response_queue_transmit(2, 0);
    assert_eq!(srv.check_response_queue_heartbeat(1, mac, 1099), None);
    srv.note_response_queue_transmit(1, 1090);
    assert_eq!(srv.check_response_queue_heartbeat(1, mac, 1150), None);
    assert!(srv.check_response_queue_heartbeat(1, mac, 1190).is_some());
}
