//! The OPEN Alliance TC18 RC Server: per-endpoint request store and the
//! single frame-level dispatch entry point.
//!
//! This module ports two pieces of c-RCP:
//!
//! - **`c-RCP/src/server.c`** — the per-endpoint request store
//!   ([`ServerEndpoint`]): the `ep_enable` pre-load queue and its drain
//!   (`server.c:51-166`), request-type-aware admission
//!   ([`ServerEndpoint::admit`], `server.c:317-545`) into a bounded
//!   [`SERVER_MAX_PENDING`]-slot conditional-request store (`claim_slot`,
//!   `server.c:184`, which reports `REQUEST_STORAGE_OVERFLOW` when full),
//!   the execution-condition tick ([`ServerEndpoint::select_due`] /
//!   [`ServerEndpoint::complete`], `server.c:785`/`:822`, with the
//!   `ep_enable` gating by kind of `server.c:703`), and cancellation /
//!   watchdog purge (`server.c:1000-1026`). The scheduler ordering it uses
//!   ([`sched_classify`], [`SchedKind::rank`], [`sched_compare`],
//!   [`split_frame_members`]) is `c-RCP/src/scheduler.c`.
//! - **The server half of `c-RCP/src/mock.c`** — [`RcServer`], which owns the
//!   per-request-stream safety configuration and state `rcp_mock_server_t`
//!   holds (`mock.c:446-1124`): the request-stream configuration table
//!   ([`RcServer::set_request_stream_cfg`]: `rx_enforce_seq`,
//!   `rx_enforce_e2e`, the watchdog, `rx_ovrflw_safestate_enable`, the
//!   ack/response stream indices), the EP_ID_config endpoint↔stream map
//!   ([`RcServer::set_ep_id_map`]), one [`SequenceTracker`] per request
//!   stream, the per-stream CRC fault tracker, and the `rx_stream_status`
//!   blocked latch ([`StreamStatus`]). It also holds the response-queue
//!   configuration table and its heartbeat bookkeeping
//!   ([`RcServer::set_response_queue_cfg`],
//!   [`RcServer::check_response_queue_heartbeat`], `mock.c:463-534`) and
//!   routes responses by `rx_ack_stream_index` / `rx_resp_stream_index`
//!   ([`RcServer::response_route`], `mock.c:1716`); the queues themselves
//!   are [`crate::respqueue`].
//!
//! [`RcServer::dispatch_frame`] is the **one** frame-level entry point: the
//! UDP transport ([`crate::udp::UdpRcServer::serve_one`]), the RELAY adapter
//! ([`mod@crate::adapt`], via [`RcServer::dispatch_message`]) and the CLI all go
//! through it. It reads the addressed stream's own configuration to decide
//! which checks apply, instead of c-RCP's split into `dispatch_frame` /
//! `dispatch_frame_e2e` / `dispatch` / `dispatch_e2e`, where the checks that
//! run depend on which entry point the caller picks (issue #224, "Don't
//! copy"). Later units extend this pipeline (lifecycle gate, CRC unwrap,
//! fragment reassembly, TSCF); see [`RcServer::dispatch_frame`]'s doc
//! comment for the stages that exist today.
//!
//! ## Blocked-latch semantics (c-RCP parity)
//!
//! c-RCP parity: `c-RCP/src/e2e.c:690-737` latches `seq_blocked`,
//! `wd_blocked` and `overflow_blocked` only when the matching cause also
//! enters safe state, and those three latches are status bits only — they
//! never block a later request. Only the CRC fault tracker
//! (`c-RCP/src/e2e.c:635-668`, consulted at `mock.c:1925`) blocks the
//! stream. TC18 RC5 §12.7.7 Table 24 describes `rx_enforce_seq` as
//! "stream is blocked upon out of order reception"; this crate copies
//! c-RCP's narrower behaviour for byte-for-byte interop.
//!
//! ## Concurrency
//!
//! [`RcServer`] serializes dispatch: one internal lock is held for the
//! whole of [`RcServer::dispatch_frame`], [`RcServer::tick`] and
//! [`RcServer::drain_endpoint`], matching c-RCP's single-threaded model.
//! An [`Endpoint`] implementation must therefore not call back into the
//! same [`RcServer`] from its own `read`/`write`.

use std::cmp::Ordering as CmpOrdering;
use std::collections::{HashMap, VecDeque};
use std::ops::Range;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::acf::{
    build_error_response, build_response_info, decode_acf_abb, decode_acf_gbb,
    decode_byte_message_info, encode_acf_abb, verify_echo_back, AcfAbbMessage, ByteMessageInfo,
    Evt, ResponseKind, ACF_ABB_HEADER_LEN, ACF_ABB_MSG_TYPE, ACF_GBB_HEADER_LEN, ACF_GBB_MSG_TYPE,
    ACF_MSG_LENGTH_9BIT_MAX, BYTE_MESSAGE_INFO_LEN, QUADLET_LEN, READ_SIZE_SEGMENT_12BIT_MAX,
};
use crate::addressing::{EndpointId, EndpointTable};
use crate::avtp::{decode_ntscf_frame, encode_ntscf_frame, extend_timestamp, StreamId};
use crate::discovery::{self, DiscoveryAccessKind, DiscoveryClaim};
use crate::ep0::{check_ep0_access_for_stream, route_byte_bus_id, RequestRoute};
use crate::lifecycle::{
    check_field_writable, FieldKind, LifecycleError, PlausibilitySnapshot, RcServerState,
    RegisterCategory, WriterCtx,
};
use crate::regmap::{
    EndpointType, EpByteBusIdMapEntry, GeneralRegisters, RequestStreamConfigEntry,
    ResponseStreamConfigEntry,
};
use crate::request::{
    complete_repetition, decode_chained_member, decode_clear_single, decode_compound_request,
    decode_timed_request, decode_triggered_request, peek_request_type, timed_due, CompoundStep,
    RequestKind, SequenceTracker, SequencerBank, SequencerState, TriggeredRuntime, TriggeredStep,
};
use crate::RcpError;

// ── Endpoint abstraction ──────────────────────────────────────────────────────

/// The per-endpoint behaviour [`RcServer`] executes a device-endpoint
/// request against, once the request store has decided it runs now.
///
/// This is the handler c-RCP registers per endpoint slot
/// (`rcp_mock_endpoint_handler_fn`, `c-RCP/src/mock.c:1142`): c-RCP does not
/// wire its `ep_*.c` modules into dispatch either (`c-RCP/include/rcp/mock.h:57-71`),
/// so concrete endpoint types are codec helpers and the handler comes from
/// configuration. [`crate::mock::MockEndpoint`] is the in-process test
/// double.
///
/// Takes `&self` rather than `&mut self` so an implementation can be shared
/// behind `Arc<dyn Endpoint>`. Never required to panic for any input.
pub trait Endpoint: Send + Sync {
    /// This endpoint's register-map type discriminant.
    fn ep_type(&self) -> EndpointType;

    /// Answer a read addressed to this endpoint. `read_size` is the
    /// request's raw [`crate::acf::ReadSizeOrSegment::as_read_size`] value.
    fn read(&self, read_size: u16) -> Result<Vec<u8>, RcpError>;

    /// Apply a write addressed to this endpoint, given the request's raw
    /// payload bytes.
    fn write(&self, payload: &[u8]) -> Result<(), RcpError>;
}

// ── Scheduler (c-RCP src/scheduler.c) ────────────────────────────────────────

/// The scheduling class of one request: c-RCP's `rcp_sched_kind_t`
/// (`c-RCP/include/rcp/scheduler.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-SCHED-001
pub enum SchedKind {
    /// An unconditional request (ACF_ABB, timestamped ACF_GBB, or an
    /// unrecognized repurposed opcode).
    #[default]
    Standard,
    /// Clear-all / clear-non-safestate / clear-single.
    Cancellation,
    /// Triggered (`0x0E`/`0x8E`).
    Triggered,
    /// Timed (`0x0A`).
    Timed,
    /// Compound (`0x0F`/`0x8F`).
    Compound,
    /// Compound-wait (`0x0B`/`0x8B`).
    CompoundWait,
    /// Chained (`0x01`).
    Chained,
}

impl SchedKind {
    /// TC18 §12.9.2's execution priority: cancellation (6) > triggered (5) >
    /// timed (4) > compound (3) > compound-wait (2) > chained (1) > standard
    /// (0). Mirrors `rcp_sched_kind_rank()` (`c-RCP/src/scheduler.c`).
    //fusa:req REQ-SCHED-002
    pub fn rank(self) -> u8 {
        match self {
            Self::Cancellation => 6,
            Self::Triggered => 5,
            Self::Timed => 4,
            Self::Compound => 3,
            Self::CompoundWait => 2,
            Self::Chained => 1,
            Self::Standard => 0,
        }
    }
}

/// Classify a request by its repurposed `request_type` opcode. Returns
/// [`SchedKind::Standard`] when `is_repurposed` is false and for any opcode
/// this crate does not recognize (never over-privileged). Mirrors
/// `rcp_sched_classify()` (`c-RCP/src/scheduler.c`). Never panics.
//fusa:req REQ-SCHED-001
pub fn sched_classify(is_repurposed: bool, request_type: u8) -> SchedKind {
    if !is_repurposed {
        return SchedKind::Standard;
    }
    match RequestKind::from_u8(request_type) {
        Ok(RequestKind::ClearAll | RequestKind::ClearNonSafestate | RequestKind::ClearSingle) => {
            SchedKind::Cancellation
        }
        Ok(RequestKind::Triggered | RequestKind::SafetyTriggered) => SchedKind::Triggered,
        Ok(RequestKind::Timed) => SchedKind::Timed,
        Ok(RequestKind::Compound | RequestKind::SafetyCompound) => SchedKind::Compound,
        Ok(RequestKind::CompoundWait | RequestKind::SafetyCompoundWait) => SchedKind::CompoundWait,
        Ok(RequestKind::Chained) => SchedKind::Chained,
        _ => SchedKind::Standard,
    }
}

/// Total order over `(kind, arrival sequence)`: the higher
/// [`SchedKind::rank`] first, then the earlier arrival. Returns
/// [`CmpOrdering::Less`] when `a` runs before `b`. Mirrors
/// `rcp_sched_compare()` (`c-RCP/src/scheduler.c`). Never panics.
//fusa:req REQ-SCHED-003
pub fn sched_compare(a: (SchedKind, u64), b: (SchedKind, u64)) -> CmpOrdering {
    b.0.rank().cmp(&a.0.rank()).then(a.1.cmp(&b.1))
}

/// Split an ACF payload stream into its members' byte ranges, delimited by
/// each member's own `acf_msg_type`/`acf_msg_length`.
///
/// Returns an empty vector for an empty stream, or when any member is
/// malformed (unknown `acf_msg_type`, header truncated, declared length
/// shorter than its header, or running past the end) — the whole stream is
/// then rejected, never partially split. Mirrors
/// `rcp_sched_split_frame_members()` (`c-RCP/src/scheduler.c`). Never
/// panics.
//fusa:req REQ-SCHED-004
//fusa:req REQ-SCHED-005
//fusa:req REQ-SCHED-006
pub fn split_frame_members(b: &[u8]) -> Vec<Range<usize>> {
    let mut members = Vec::new();
    let mut offset = 0usize;
    while offset < b.len() {
        match member_len_at(&b[offset..]) {
            Some(len) => {
                members.push(offset..offset + len);
                offset += len;
            }
            None => return Vec::new(),
        }
    }
    members
}

/// The byte length of the ACF member at the front of `rest`, or `None` if
/// it is malformed (see [`split_frame_members`]).
fn member_len_at(rest: &[u8]) -> Option<usize> {
    if rest.len() < 2 {
        return None;
    }
    let header_len = match rest[0] >> 1 {
        ACF_ABB_MSG_TYPE => ACF_ABB_HEADER_LEN,
        ACF_GBB_MSG_TYPE => ACF_GBB_HEADER_LEN,
        _ => return None,
    };
    let quadlets = (usize::from(rest[0] & 0x01) << 8) | usize::from(rest[1]);
    let msg_len = quadlets * QUADLET_LEN;
    (rest.len() >= header_len && msg_len >= header_len && rest.len() >= msg_len).then_some(msg_len)
}

// ── Response builders (c-RCP src/acf.c:298-357) ──────────────────────────────

/// TC18 §11.3.1's `evt[3:0] = 0xF` acknowledge marker.
const EVT_ACKNOWLEDGE: Evt = Evt {
    ack: true,
    sub_opcode: 0x07,
};

/// A response header exactly as c-RCP's `rcp_acf_build_*_response()`
/// helpers pack it: `op = RCP_ACF_OP_NONE`, `rsp = 1`, every other field
/// zero except the ones given.
///
/// c-RCP parity: `RCP_ACF_OP_NONE` encodes as the write sense, `op = 1`
/// on the wire (`c-RCP/include/rcp/acf.h:205-225`), so every one of these
/// responses carries `op = 1` whatever the request's `op` was. TC18 RC5
/// §11.3 Table 17 (TC18-11.3-003) has the response mirror the request's
/// `op`. Proven against c-RCP's real builders by
/// `testdata/c_rcp_server_response_golden.txt`.
fn crcp_response(
    byte_bus_id: u16,
    transaction_num: u8,
    evt: Evt,
    err: bool,
    payload: Vec<u8>,
) -> Option<Vec<u8>> {
    encode_acf_abb(&AcfAbbMessage {
        info: ByteMessageInfo {
            byte_bus_id,
            transaction_num,
            evt,
            op: true,
            rsp: true,
            err,
            ..ByteMessageInfo::default()
        },
        payload,
    })
    .ok()
}

/// `rcp_acf_build_error_response()` (`c-RCP/src/acf.c:298`): `evt = 0`,
/// `err = 1`, the Table 30 code as the one-byte payload. `None` for an
/// error with no wire code.
///
/// c-RCP parity: `c-RCP/src/acf.c:298-315` always writes `evt = 0`,
/// `op = RCP_ACF_OP_NONE` (wire `op = 1`, see [`crcp_response`]),
/// `rsp = 1`, `err = 1`, whatever the
/// request carried. TC18 RC5 §11.3 Table 17 (TC18-11.3-003) says a
/// response mirrors the originating request's `op`, and this crate's
/// execution-path `build_error_response` copies `evt`/`op` from the
/// request; the store/gate paths that use this builder copy c-RCP
/// instead.
fn wire_error_response(byte_bus_id: u16, transaction_num: u8, error: &RcpError) -> Option<Vec<u8>> {
    let code = error.tc18_wire_code()?;
    crcp_response(
        byte_bus_id,
        transaction_num,
        Evt::default(),
        true,
        vec![code],
    )
}

/// `rcp_acf_build_acknowledge_response()` (`c-RCP/src/acf.c:322`).
fn acknowledge_response(byte_bus_id: u16, transaction_num: u8) -> Option<Vec<u8>> {
    crcp_response(
        byte_bus_id,
        transaction_num,
        EVT_ACKNOWLEDGE,
        false,
        Vec::new(),
    )
}

/// `rcp_acf_build_acknowledge_rejected_response()` (`c-RCP/src/acf.c:338`).
fn acknowledge_rejected_response(
    byte_bus_id: u16,
    transaction_num: u8,
    error: &RcpError,
) -> Option<Vec<u8>> {
    let code = error.tc18_wire_code()?;
    crcp_response(
        byte_bus_id,
        transaction_num,
        EVT_ACKNOWLEDGE,
        true,
        vec![code],
    )
}

/// The `byte_message_info` header at the front of `frame`, if it is at
/// least [`BYTE_MESSAGE_INFO_LEN`] long (c-RCP's
/// `frame_len >= 8 && rcp_acf_unpack_header()` idiom).
fn peek_header(frame: &[u8]) -> Option<ByteMessageInfo> {
    frame
        .get(..BYTE_MESSAGE_INFO_LEN)
        .and_then(|h| decode_byte_message_info(h).ok())
}

/// A wire error code back to its [`RcpError`] (the inverse of
/// [`RcpError::tc18_wire_code`]).
fn error_from_wire_code(code: Option<u8>) -> RcpError {
    const BY_CODE: [RcpError; 17] = [
        RcpError::UnsupportedCmd,
        RcpError::SequencerNotKnown,
        RcpError::UnauthorizedAccess,
        RcpError::LockedMemAccess,
        RcpError::RequestCanceled,
        RcpError::RequestNotFound,
        RcpError::EpError,
        RcpError::EpNotFound,
        RcpError::PwmInNoSignal,
        RcpError::ReqStorageOvfl,
        RcpError::RequestRejected,
        RcpError::PociFailure,
        RcpError::PresentationTimeTooFar,
        RcpError::GptpFail,
        RcpError::InvalidParameter,
        RcpError::ChainAborted,
        RcpError::ChainError,
    ];
    code.and_then(|c| BY_CODE.get(usize::from(c).wrapping_sub(1)).cloned())
        .unwrap_or_else(|| RcpError::Other(format!("unknown wire error code {code:?}")))
}

// ── Request-type predicates ──────────────────────────────────────────────────

/// `rcp_e2e_is_safety_request()` (`c-RCP/src/e2e.c:411`): the MSB tag.
fn is_safety_request(request_type: u8) -> bool {
    request_type & 0x80 != 0
}

/// `rcp_request_type_is_compound_wait()`.
fn is_compound_wait_type(request_type: u8) -> bool {
    matches!(
        RequestKind::from_u8(request_type),
        Ok(RequestKind::CompoundWait | RequestKind::SafetyCompoundWait)
    )
}

/// Whether a disabled endpoint must still execute `frame` immediately: a
/// configuration-write request (`evt[2:0] == 111b`), for an ACF_ABB request
/// or any ACF_GBB request kind except Compound Wait (whose `evt[2:0]` is a
/// comparison selector). Mirrors `rcp_server_endpoint_submit()`
/// (`c-RCP/src/server.c:56-102`).
//fusa:req REQ-SRV-015
fn is_configuration_request(frame: &[u8]) -> bool {
    let Some(hdr) = peek_header(frame) else {
        return false;
    };
    if hdr.evt.sub_opcode & 0x07 != 0x07 {
        return false;
    }
    match hdr.acf_msg_type {
        ACF_ABB_MSG_TYPE => true,
        ACF_GBB_MSG_TYPE => frame.get(8).is_some_and(|rt| !is_compound_wait_type(*rt)),
        _ => false,
    }
}

/// `build_store_ack()` (`c-RCP/src/server.c:220`): an Acknowledge for
/// `frame` iff its own `evt[3]` asks for one.
//fusa:req REQ-SRV-016
fn build_store_ack(frame: &[u8]) -> Option<Vec<u8>> {
    let hdr = peek_header(frame)?;
    if !hdr.evt.ack {
        return None;
    }
    acknowledge_response(hdr.byte_bus_id, hdr.transaction_num)
}

// ── Compound-wait comparison (c-RCP src/acf.c:217-290) ───────────────────────

const COMPOUND_WAIT_MODE_RESERVED: u8 = 0x3;

/// `rcp_acf_compound_wait_evt_valid()`: every `evt[2:0]` except the
/// reserved `011b`.
//fusa:req REQ-SRV-019
fn compound_wait_evt_valid(evt: u8) -> bool {
    evt & 0x7 != COMPOUND_WAIT_MODE_RESERVED
}

fn be16(p: &[u8]) -> u16 {
    u16::from_be_bytes([p[0], p[1]])
}

/// `rcp_acf_compound_wait_match()`: compare a compound-wait request's own
/// `byte_msg_payload` against the endpoint's current status bytes under
/// the comparison mode `evt[2:0]` selects (TC18 §13.5.1).
///
/// c-RCP parity: the high/low 16-bit modes compare `payload >= status`
/// (`c-RCP/src/acf.c:271-280`), copied as is.
//fusa:req REQ-SRV-020
fn compound_wait_match(evt: u8, payload: &[u8], status: &[u8]) -> bool {
    if status.len() < payload.len() {
        return false;
    }
    let status = &status[..payload.len()];
    match evt & 0x7 {
        0x0 => payload == status,
        0x1 => payload
            .iter()
            .zip(status)
            .all(|(p, s)| (p & s) | !p == 0xFF),
        0x2 => payload.iter().zip(status).all(|(p, s)| p & s == 0),
        0x4..=0x7 if payload.len() >= 4 => compound_wait_word_cmp(evt & 0x7, payload, status),
        _ => false,
    }
}

/// The four 16-bit word comparisons of [`compound_wait_match`]
/// (`payload.len() >= 4` already checked).
fn compound_wait_word_cmp(mode: u8, payload: &[u8], status: &[u8]) -> bool {
    let (p, s) = if mode < 0x6 {
        (be16(payload), be16(status))
    } else {
        (be16(&payload[2..]), be16(&status[2..]))
    };
    if mode & 0x1 == 0 {
        p >= s
    } else {
        p <= s
    }
}

// ── Sequencer interplay (c-RCP src/request.c:295-372) ────────────────────────

fn sequencer_state(seqs: &SequencerBank, index: u8) -> Option<u8> {
    seqs.read(index).ok().map(|s| s.0)
}

/// `rcp_compound_start_condition_met()`: a disabled (`0`) sequencer never
/// starts anything; `start_state == 0` means "any state".
fn compound_start_condition_met(seqs: &SequencerBank, step: &CompoundStep) -> bool {
    match sequencer_state(seqs, step.sequencer_index) {
        None | Some(0) => false,
        Some(current) => step.start_state == 0 || current == step.start_state,
    }
}

/// `rcp_compound_advance_guard()`: still in `start_state` (never `0`).
fn compound_advance_guard(seqs: &SequencerBank, step: &CompoundStep) -> bool {
    matches!(sequencer_state(seqs, step.sequencer_index), Some(c) if c != 0 && c == step.start_state)
}

/// `apply_next_state()`: `next_state == 0` stays put.
fn apply_next_state(seqs: &mut SequencerBank, step: &CompoundStep) -> bool {
    if step.next_state == 0 {
        return sequencer_state(seqs, step.sequencer_index).is_some();
    }
    seqs.force_state(step.sequencer_index, SequencerState(step.next_state))
        .is_ok()
}

/// `rcp_compound_tick()`.
fn compound_tick(seqs: &mut SequencerBank, step: &CompoundStep, elapsed: u32) -> bool {
    elapsed >= u32::from(step.exec_delay)
        && compound_advance_guard(seqs, step)
        && apply_next_state(seqs, step)
}

/// `rcp_compound_wait_tick()`.
fn compound_wait_tick(seqs: &mut SequencerBank, step: &CompoundStep, condition_met: bool) -> bool {
    condition_met && compound_advance_guard(seqs, step) && apply_next_state(seqs, step)
}

// ── The per-endpoint request store (c-RCP src/server.c) ──────────────────────

/// How many conditional requests one endpoint's request store holds at
/// once: `RCP_SERVER_MAX_PENDING` (`c-RCP/include/rcp/server.h:122`).
//fusa:req REQ-SRV-005
pub const SERVER_MAX_PENDING: usize = 32;

/// One request held in an endpoint's request store, from admission until
/// it executes (and, for a repeating request, until its repetitions are
/// exhausted). Mirrors `rcp_server_pending_t`
/// (`c-RCP/include/rcp/server.h:124-200`); only the fields matching `kind`
/// are meaningful.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
//fusa:req REQ-SRV-025
pub struct PendingRequest {
    /// Scheduling class.
    pub kind: SchedKind,
    /// The repurposed opcode byte (`0` for a standard request).
    pub request_type: u8,
    /// The request's own `transaction_num`.
    pub transaction_num: u8,
    /// Arrival order, for FIFO tie-breaking.
    pub sequence: u64,
    /// An owned copy of the whole ACF message.
    pub frame: Vec<u8>,
    /// Compound / compound-wait sub-fields.
    pub compound: CompoundStep,
    /// Compound-wait `evt` (comparison mode).
    pub compound_wait_evt: u8,
    /// Compound-wait comparison target (`byte_msg_payload`).
    pub compound_wait_target: Vec<u8>,
    /// Triggered sub-fields.
    pub triggered: TriggeredStep,
    /// Triggered occurrence counter.
    pub triggered_runtime: TriggeredRuntime,
    /// Timed presentation time (48-bit gPTP ns).
    pub presentation_time: u64,
    /// Chained `chain_exec_delay`.
    pub chain_exec_delay: u16,
    /// Chained abort/continue selector.
    pub cs: bool,
    /// Which chain this entry belongs to (`0` = none).
    pub chain_group: u32,
    /// Position within its chain (`0` = the anchor).
    pub chain_position: u8,
    /// Whether the start condition has held (exec-delay timer running).
    pub armed: bool,
    /// Tick at which the exec-delay timer started.
    pub armed_at: u32,
    /// Chained only: the predecessor has finalized.
    pub predecessor_done: bool,
    /// Whether a TSCF presentation-time gate applies.
    pub has_presentation_gate: bool,
    /// The TSCF presentation time, extended to the 48-bit domain.
    pub presentation_gate_ns: u64,
}

/// The TSCF envelope's presentation time, for [`ServerEndpoint::admit`]
/// (c-RCP's `tv`/`avtp_timestamp`/`gptp_reference_now` triple).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
//fusa:req REQ-SRV-004
pub struct PresentationGate {
    /// The TSCF header's 32-bit `avtp_timestamp`.
    pub avtp_timestamp: u32,
    /// Current gPTP time used to extend it to 48 bits.
    pub gptp_reference_now: u64,
}

/// What [`ServerEndpoint::submit`] decided.
#[derive(Debug, Clone, PartialEq, Eq)]
//fusa:req REQ-SRV-001
//fusa:req REQ-SRV-002
pub enum Submission {
    /// The caller must execute the request now.
    ExecuteNow,
    /// The request was queued; `ack` is the Acknowledge to send, if the
    /// request's `evt[3]` asked for one.
    Queued {
        /// The store acknowledge, if requested.
        ack: Option<Vec<u8>>,
    },
}

/// What [`ServerEndpoint::admit`] decided: `rcp_server_admit_t`
/// (`c-RCP/include/rcp/server.h:296-330`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
//fusa:req REQ-SRV-004
pub enum AdmitResult {
    /// Standard request, endpoint enabled (or a configuration request):
    /// execute now.
    ExecuteNow,
    /// Standard request, endpoint disabled: queued for drain.
    Queued,
    /// Placed in the request store.
    Pending,
    /// Cancellation request: the caller applies it now, unstored.
    Cancellation,
    /// Not stored, not executed.
    Rejected,
    /// The endpoint is refusing admissions (sleep-request drain).
    Suspended,
}

/// The full outcome of [`ServerEndpoint::admit`].
#[derive(Debug, Clone, PartialEq)]
//fusa:req REQ-SRV-026
pub struct Admission {
    /// The routing decision.
    pub result: AdmitResult,
    /// The repurposed opcode for a conditional/cancellation request, `0`
    /// for a standard one.
    pub request_type: u8,
    /// The store index a [`AdmitResult::Pending`] request was placed at.
    pub index: Option<usize>,
    /// The TC18 error a rejection determined, if any.
    pub error: Option<RcpError>,
    /// The store Acknowledge for a queued/stored request whose `evt[3]`
    /// asked for one.
    pub ack: Option<Vec<u8>>,
}

impl Admission {
    fn of(result: AdmitResult, request_type: u8) -> Self {
        Self {
            result,
            request_type,
            index: None,
            error: None,
            ack: None,
        }
    }

    fn rejected(request_type: u8, error: Option<RcpError>) -> Self {
        Self {
            error,
            ..Self::of(AdmitResult::Rejected, request_type)
        }
    }
}

/// The result of [`ServerEndpoint::cancel_single`]: c-RCP's
/// `rcp_cancel_result_t` restricted to the outcomes reachable for a queued
/// request (this store runs a selected request to completion
/// synchronously, so nothing is ever observably mid-execution).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
//fusa:req REQ-SRV-040
pub enum CancelResult {
    /// The request was found and removed.
    Canceled,
    /// No stored request carries that `transaction_num`.
    NotFound,
}

/// Everything [`ServerEndpoint::select_due`] needs to evaluate a stored
/// request's execution condition: `rcp_server_tick_ctx_t`
/// (`c-RCP/include/rcp/server.h:470-515`), minus the sequencer table, which
/// is passed separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-SRV-006
pub struct TickCtx<'a> {
    /// Current tick count, in the endpoint's `ep_delay_time` unit.
    pub now: u32,
    /// Current gPTP time, ns modulo 2^48.
    pub gptp_now: u64,
    /// Whether a gPTP time base is locked.
    pub gptp_locked: bool,
    /// Whether the endpoint is idle.
    pub endpoint_idle: bool,
    /// Whether the endpoint has reached its configured safe state.
    pub in_safe_state: bool,
    /// The endpoint's current status bytes (compound-wait comparisons).
    pub current_status: &'a [u8],
}

/// One endpoint's `ep_enable` flag and pre-load queue, its admission
/// suspension flag, and its bounded conditional-request store:
/// `rcp_server_endpoint_t` (`c-RCP/include/rcp/server.h:206-235`).
#[derive(Debug, Clone, PartialEq)]
//fusa:req REQ-SRV-001
pub struct ServerEndpoint {
    ep_enable: bool,
    admission_suspended: bool,
    queue: VecDeque<Vec<u8>>,
    pending: Vec<Option<PendingRequest>>,
    next_sequence: u64,
}

impl ServerEndpoint {
    /// An empty store with the given initial `ep_enable`
    /// (`rcp_server_endpoint_init()`).
    //fusa:req REQ-SRV-001
    pub fn new(ep_enable: bool) -> Self {
        Self {
            ep_enable,
            admission_suspended: false,
            queue: VecDeque::new(),
            pending: vec![None; SERVER_MAX_PENDING],
            next_sequence: 0,
        }
    }

    /// The current `ep_enable` value.
    //fusa:req REQ-SRV-003
    pub fn ep_enable(&self) -> bool {
        self.ep_enable
    }

    /// Set `ep_enable`. Executes and discards nothing; call
    /// [`Self::drain_one`] afterwards (`rcp_server_endpoint_set_enable()`).
    //fusa:req REQ-SRV-003
    pub fn set_enable(&mut self, enable: bool) {
        self.ep_enable = enable;
    }

    /// Set the sleep-request admission suspension
    /// (`rcp_server_endpoint_set_admission_suspended()`).
    //fusa:req REQ-PWRMODE-028
    pub fn set_admission_suspended(&mut self, suspended: bool) {
        self.admission_suspended = suspended;
    }

    /// Submit one standard request. An enabled endpoint, or a configuration
    /// request on a disabled one, returns [`Submission::ExecuteNow`];
    /// otherwise a copy is queued and the store Acknowledge is built if
    /// `evt[3]` asks for one (`rcp_server_endpoint_submit()`,
    /// `c-RCP/src/server.c:51-127`). Does not consult admission
    /// suspension. Never panics.
    //fusa:req REQ-SRV-001
    //fusa:req REQ-SRV-002
    //fusa:req REQ-SRV-015
    //fusa:req REQ-SRV-016
    pub fn submit(&mut self, frame: &[u8]) -> Submission {
        if self.ep_enable || is_configuration_request(frame) {
            return Submission::ExecuteNow;
        }
        self.queue.push_back(frame.to_vec());
        Submission::Queued {
            ack: build_store_ack(frame),
        }
    }

    /// Dequeue the oldest queued request — only while enabled
    /// (`rcp_server_endpoint_drain_one()`).
    //fusa:req REQ-SRV-003
    //fusa:req REQ-SRV-023
    pub fn drain_one(&mut self) -> Option<Vec<u8>> {
        if !self.ep_enable {
            return None;
        }
        self.queue.pop_front()
    }

    /// Number of queued (undrained) standard requests.
    //fusa:req REQ-SRV-001
    pub fn queue_len(&self) -> usize {
        self.queue.len()
    }

    /// Number of requests in the conditional-request store
    /// (`rcp_server_endpoint_pending_count()`).
    //fusa:req REQ-SRV-021
    pub fn pending_count(&self) -> usize {
        self.pending.iter().flatten().count()
    }

    /// The stored request at `index`, if any.
    //fusa:req REQ-SRV-021
    pub fn pending(&self, index: usize) -> Option<&PendingRequest> {
        self.pending.get(index).and_then(Option::as_ref)
    }

    /// Classify and route one arriving ACF message
    /// (`rcp_server_endpoint_admit_with_ack()`, `c-RCP/src/server.c:317-545`).
    ///
    /// Admission suspension is checked first ([`AdmitResult::Suspended`]).
    /// A response frame (`rsp = 1`) is rejected with
    /// [`RcpError::InvalidParameter`]. A message that is not a repurposed
    /// ACF_GBB, or whose opcode is unrecognized, is a standard request
    /// ([`Self::submit`], or the store under a TSCF `gate`). A cancellation
    /// returns [`AdmitResult::Cancellation`] unstored (stored under a
    /// `gate`). Every other kind is decoded and stored
    /// ([`AdmitResult::Pending`]); a full store rejects with
    /// [`RcpError::ReqStorageOvfl`], an undecodable body rejects with no
    /// error code, and a compound-wait request's reserved `evt[2:0] = 011b`
    /// rejects with [`RcpError::UnsupportedCmd`]. `now` is the current tick.
    ///
    /// c-RCP parity: a full store under the TSCF gate rejects with no error
    /// code (`admit_under_tscf_gate()`, `c-RCP/src/server.c:276`), and an
    /// undecodable conditional body is answered with nothing
    /// (`c-RCP/src/server.c:463-512`); TC18 RC5 would send an error
    /// response. Copied as is. Never panics.
    //fusa:req REQ-SRV-004
    //fusa:req REQ-SRV-005
    //fusa:req REQ-SRV-024
    //fusa:req REQ-SRV-025
    //fusa:req REQ-SRV-026
    //fusa:req REQ-PWRMODE-028
    //fusa:req REQ-ACF-050
    pub fn admit(&mut self, frame: &[u8], now: u32, gate: Option<PresentationGate>) -> Admission {
        if self.admission_suspended {
            return Admission::of(AdmitResult::Suspended, 0);
        }
        if peek_header(frame).is_some_and(|h| h.rsp) {
            return Admission::rejected(0, Some(RcpError::InvalidParameter));
        }
        let gate_ns = gate.map(|g| extend_timestamp(g.avtp_timestamp, g.gptp_reference_now));
        let rt = peek_request_type(frame).ok();
        match rt.map(|rt| (sched_classify(true, rt), rt)) {
            None | Some((SchedKind::Standard, _)) => self.admit_standard(frame, now, gate_ns),
            Some((SchedKind::Cancellation, rt)) => self.admit_cancellation(frame, now, gate_ns, rt),
            Some((kind, rt)) => self.admit_conditional(frame, now, gate_ns, kind, rt),
        }
    }

    fn admit_standard(&mut self, frame: &[u8], now: u32, gate_ns: Option<u64>) -> Admission {
        if let Some(ns) = gate_ns {
            return self.admit_under_gate(frame, now, ns, SchedKind::Standard, 0);
        }
        match self.submit(frame) {
            Submission::ExecuteNow => Admission::of(AdmitResult::ExecuteNow, 0),
            Submission::Queued { ack } => Admission {
                ack,
                ..Admission::of(AdmitResult::Queued, 0)
            },
        }
    }

    fn admit_cancellation(
        &mut self,
        frame: &[u8],
        now: u32,
        gate_ns: Option<u64>,
        rt: u8,
    ) -> Admission {
        match gate_ns {
            Some(ns) => self.admit_under_gate(frame, now, ns, SchedKind::Cancellation, rt),
            None => Admission::of(AdmitResult::Cancellation, rt),
        }
    }

    /// `admit_under_tscf_gate()` (`c-RCP/src/server.c:269-296`).
    fn admit_under_gate(
        &mut self,
        frame: &[u8],
        now: u32,
        gate_ns: u64,
        kind: SchedKind,
        rt: u8,
    ) -> Admission {
        let Some((index, sequence)) = self.claim_slot() else {
            return Admission::rejected(rt, None);
        };
        self.pending[index] = Some(PendingRequest {
            kind,
            request_type: rt,
            sequence,
            has_presentation_gate: true,
            presentation_gate_ns: gate_ns,
            armed_at: now,
            frame: frame.to_vec(),
            ..PendingRequest::default()
        });
        Self::stored(rt, index, frame)
    }

    fn admit_conditional(
        &mut self,
        frame: &[u8],
        now: u32,
        gate_ns: Option<u64>,
        kind: SchedKind,
        rt: u8,
    ) -> Admission {
        let Some((index, sequence)) = self.claim_slot() else {
            // TC18 §12.7.7 Table 24 rx_ovrflw_safestate_enable's per-request
            // half; the stream-wide escalation is RcServer's job.
            return Admission::rejected(rt, Some(RcpError::ReqStorageOvfl));
        };
        let mut req = PendingRequest {
            kind,
            request_type: rt,
            sequence,
            ..PendingRequest::default()
        };
        if let Err(error) = decode_conditional(&mut req, frame) {
            return Admission::rejected(rt, error);
        }
        req.has_presentation_gate = gate_ns.is_some();
        req.presentation_gate_ns = gate_ns.unwrap_or(0);
        req.armed_at = now;
        req.frame = frame.to_vec();
        self.pending[index] = Some(req);
        Self::stored(rt, index, frame)
    }

    fn stored(rt: u8, index: usize, frame: &[u8]) -> Admission {
        Admission {
            index: Some(index),
            ack: build_store_ack(frame),
            ..Admission::of(AdmitResult::Pending, rt)
        }
    }

    /// `claim_slot()` (`c-RCP/src/server.c:184-198`): the first free slot
    /// and a fresh arrival sequence, or `None` when the store is full. The
    /// sequence is consumed even if the caller then fails to decode.
    //fusa:req REQ-SRV-005
    fn claim_slot(&mut self) -> Option<(usize, u64)> {
        let index = self.pending.iter().position(Option::is_none)?;
        let sequence = self.next_sequence;
        self.next_sequence += 1;
        Some((index, sequence))
    }

    /// Re-evaluate every stored request against `ctx` (arming exec-delay
    /// timers whose start condition now holds) and return the index of the
    /// single highest-priority request due now ([`sched_compare`]).
    ///
    /// A Compound/Compound-Wait/Triggered/Timed/Chained request is never due
    /// while the endpoint is disabled; a safety-tagged request is never due
    /// before `ctx.in_safe_state`; a TSCF-gated request never before its
    /// presentation time on a locked time base. Mirrors
    /// `rcp_server_endpoint_select_due()` (`c-RCP/src/server.c:785-815`).
    //fusa:req REQ-SRV-006
    //fusa:req REQ-SRV-007
    //fusa:req REQ-SRV-008
    //fusa:req REQ-SRV-015
    //fusa:req REQ-SRV-016
    //fusa:req REQ-SRV-020
    //fusa:req REQ-SRV-027
    //fusa:req REQ-SRV-028
    //fusa:req REQ-SRV-029
    //fusa:req REQ-SRV-030
    //fusa:req REQ-SRV-031
    //fusa:req REQ-SRV-032
    //fusa:req REQ-SRV-038
    pub fn select_due(&mut self, ctx: &TickCtx<'_>, seqs: &SequencerBank) -> Option<usize> {
        let ep_enable = self.ep_enable;
        let mut best: Option<(usize, SchedKind, u64)> = None;
        for (i, slot) in self.pending.iter_mut().enumerate() {
            let Some(req) = slot else { continue };
            if !is_due(req, ep_enable, ctx, seqs) {
                continue;
            }
            let better = best.map_or(true, |(_, k, s)| {
                sched_compare((req.kind, req.sequence), (k, s)) == CmpOrdering::Less
            });
            if better {
                best = Some((i, req.kind, req.sequence));
            }
        }
        best.map(|(i, _, _)| i)
    }

    /// Finalize the request at `index` after the caller executed it: a
    /// compound request advances its sequencer through the advance guard, a
    /// compound-wait request likewise when its comparison holds, a
    /// triggered request resets its counter; then the repetition rule
    /// ([`complete_repetition`]). Timed, chained, standard and cancellation
    /// entries are always removed. Returns `true` iff the request stays
    /// stored (re-armed). Mirrors `rcp_server_endpoint_complete()`
    /// (`c-RCP/src/server.c:822-893`).
    //fusa:req REQ-SRV-009
    //fusa:req REQ-SRV-010
    //fusa:req REQ-SRV-033
    //fusa:req REQ-SRV-034
    //fusa:req REQ-SRV-035
    //fusa:req REQ-SRV-036
    //fusa:req REQ-SRV-037
    pub fn complete(&mut self, index: usize, ctx: &TickCtx<'_>, seqs: &mut SequencerBank) -> bool {
        let Some(slot) = self.pending.get_mut(index) else {
            return false;
        };
        let Some(req) = slot.as_mut() else {
            return false;
        };
        let stays = complete_one(req, ctx, seqs);
        if !stays {
            *slot = None;
        }
        stays
    }

    /// Offer one trigger occurrence to every stored triggered request;
    /// returns how many counted it (`rcp_server_endpoint_notify_trigger()`).
    //fusa:req REQ-SRV-011
    pub fn notify_trigger(&mut self, source_ep: u8, signal_nr: u8) -> usize {
        let mut matched = 0;
        for r in self.pending.iter_mut().flatten() {
            if r.kind == SchedKind::Triggered
                && r.triggered_runtime
                    .record_occurrence(&r.triggered, source_ep, signal_nr)
            {
                matched += 1;
            }
        }
        matched
    }

    /// Record that the stored chained request at `index` has had its
    /// predecessor finalize at `now`, starting its `chain_exec_delay`.
    /// Returns `false`, changing nothing, unless `index` names a stored
    /// chained request (`rcp_server_endpoint_chain_predecessor_done()`).
    //fusa:req REQ-SRV-012
    //fusa:req REQ-SRV-039
    pub fn chain_predecessor_done(&mut self, index: usize, now: u32) -> bool {
        match self.pending.get_mut(index).and_then(Option::as_mut) {
            Some(req) if req.kind == SchedKind::Chained => {
                req.predecessor_done = true;
                req.armed_at = now;
                true
            }
            _ => false,
        }
    }

    /// Remove every stored request; returns how many
    /// (`rcp_server_endpoint_cancel_all()`).
    //fusa:req REQ-SRV-013
    pub fn cancel_all(&mut self) -> usize {
        self.remove_where(|_| true)
    }

    /// Remove the first stored request whose `transaction_num` is
    /// `clear_transaction_num` (`rcp_server_endpoint_cancel_single()`).
    /// Does not cascade to chained successors.
    //fusa:req REQ-SRV-040
    pub fn cancel_single(&mut self, clear_transaction_num: u8) -> CancelResult {
        let found = self.pending.iter().position(|s| {
            s.as_ref()
                .is_some_and(|r| r.transaction_num == clear_transaction_num)
        });
        match found {
            Some(i) => {
                self.pending[i] = None;
                CancelResult::Canceled
            }
            None => CancelResult::NotFound,
        }
    }

    /// Remove every non-safety-tagged stored request
    /// (`rcp_server_endpoint_cancel_non_safestate()`).
    //fusa:req REQ-SRV-041
    pub fn cancel_non_safestate(&mut self) -> usize {
        self.remove_where(|r| !is_safety_request(r.request_type))
    }

    /// The watchdog-overflow purge: keep only the safety sequence
    /// (`rcp_server_endpoint_watchdog_purge()`).
    //fusa:req REQ-SRV-014
    pub fn watchdog_purge(&mut self) -> usize {
        self.remove_where(|r| !is_safety_request(r.request_type))
    }

    /// Remove every stored member of chain `chain_group` at or after
    /// `min_position`; `chain_group == 0` matches nothing
    /// (`rcp_server_endpoint_cancel_chain_from()`).
    //fusa:req REQ-CANCEL-012
    pub fn cancel_chain_from(&mut self, chain_group: u32, min_position: u8) -> usize {
        if chain_group == 0 {
            return 0;
        }
        self.remove_where(|r| r.chain_group == chain_group && r.chain_position >= min_position)
    }

    fn remove_where(&mut self, pred: impl Fn(&PendingRequest) -> bool) -> usize {
        let mut removed = 0;
        for slot in &mut self.pending {
            if slot.as_ref().is_some_and(&pred) {
                *slot = None;
                removed += 1;
            }
        }
        removed
    }

    /// The store index of the most recently admitted request (highest
    /// sequence), `last_pending_index()` (`c-RCP/src/mock.c:3014`).
    fn last_pending_index(&self) -> Option<usize> {
        self.pending
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.as_ref().map(|r| (i, r.sequence)))
            .max_by_key(|&(i, seq)| (seq, i))
            .map(|(i, _)| i)
    }
}

/// Decode a conditional request body into `req` (the per-kind switch of
/// `rcp_server_endpoint_admit_with_ack()`, `c-RCP/src/server.c:446-512`).
/// `Err(None)` for an undecodable body, `Err(Some(UnsupportedCmd))` for a
/// compound-wait request's reserved `evt`.
//fusa:req REQ-SRV-019
//fusa:req REQ-SRV-022
//fusa:req REQ-SRV-042
fn decode_conditional(req: &mut PendingRequest, frame: &[u8]) -> Result<(), Option<RcpError>> {
    match req.kind {
        SchedKind::Compound | SchedKind::CompoundWait => {
            let c = decode_compound_request(frame).map_err(|_| None)?;
            let evt = c.evt.raw_nibble();
            if req.kind == SchedKind::CompoundWait {
                if !compound_wait_evt_valid(evt) {
                    return Err(Some(RcpError::UnsupportedCmd));
                }
                req.compound_wait_evt = evt;
                req.compound_wait_target = c.payload;
            }
            req.compound = c.step;
            req.transaction_num = c.transaction_num;
        }
        SchedKind::Triggered => {
            let t = decode_triggered_request(frame).map_err(|_| None)?;
            req.triggered = t.step;
            req.transaction_num = t.transaction_num;
            req.triggered_runtime.enter_started();
        }
        SchedKind::Timed => {
            let t = decode_timed_request(frame).map_err(|_| None)?;
            req.presentation_time = t.presentation_time;
            req.transaction_num = t.transaction_num;
        }
        _ => decode_chained_into(req, frame)?,
    }
    Ok(())
}

fn decode_chained_into(req: &mut PendingRequest, frame: &[u8]) -> Result<(), Option<RcpError>> {
    let c = decode_chained_member(frame).map_err(|_| None)?;
    req.chain_exec_delay = c.chain_exec_delay;
    req.cs = c.cs;
    req.transaction_num = c.transaction_num;
    Ok(())
}

/// `kind_is_gated_by_ep_enable()` (`c-RCP/src/server.c:703`).
///
/// c-RCP parity: `c-RCP/src/server.c:690-712` excludes Standard and
/// Cancellation requests, which reach the store only through the TSCF
/// presentation-time gate, so a stored TSCF-gated Standard request runs
/// on an endpoint whose `ep_enable` is 0. TC18 RC5 §12.3.1.3
/// (TC18-12.3.1.3-002) says a disabled endpoint executes only config
/// requests and keeps operational ones stored until it is enabled.
fn kind_is_gated_by_ep_enable(kind: SchedKind) -> bool {
    !matches!(kind, SchedKind::Standard | SchedKind::Cancellation)
}

/// The envelope-level gates of `is_due()` (`c-RCP/src/server.c:720-760`):
/// `ep_enable` by kind, the safety tag, and the TSCF presentation time.
fn envelope_gates_open(req: &PendingRequest, ep_enable: bool, ctx: &TickCtx<'_>) -> bool {
    let enabled = ep_enable || !kind_is_gated_by_ep_enable(req.kind);
    let safety_ok = !is_safety_request(req.request_type) || ctx.in_safe_state;
    let gate_ok = !req.has_presentation_gate
        || (ctx.gptp_locked && timed_due(req.presentation_gate_ns, ctx.gptp_now));
    enabled && safety_ok && gate_ok
}

/// `is_due()` (`c-RCP/src/server.c:720-777`).
fn is_due(
    req: &mut PendingRequest,
    ep_enable: bool,
    ctx: &TickCtx<'_>,
    seqs: &SequencerBank,
) -> bool {
    envelope_gates_open(req, ep_enable, ctx)
        && arm_if_startable(req, ctx, seqs)
        && auxiliary_condition_met(req, ctx)
        && delay_expired(req, ctx, ctx.now.wrapping_sub(req.armed_at))
}

/// `start_condition_holds()` (`c-RCP/src/server.c:549-598`).
fn start_condition_holds(req: &PendingRequest, seqs: &SequencerBank) -> bool {
    match req.kind {
        SchedKind::Compound | SchedKind::CompoundWait => {
            compound_start_condition_met(seqs, &req.compound)
        }
        SchedKind::Triggered => req.triggered_runtime.threshold_reached(&req.triggered),
        SchedKind::Chained => req.predecessor_done,
        SchedKind::Timed | SchedKind::Standard | SchedKind::Cancellation => true,
    }
}

/// `arm_if_startable()` (`c-RCP/src/server.c:602-617`): a chained request
/// keeps the instant its predecessor finalized.
//fusa:req REQ-SRV-038
fn arm_if_startable(req: &mut PendingRequest, ctx: &TickCtx<'_>, seqs: &SequencerBank) -> bool {
    if req.armed {
        return true;
    }
    if !start_condition_holds(req, seqs) {
        return false;
    }
    req.armed = true;
    if req.kind != SchedKind::Chained {
        req.armed_at = ctx.now;
    }
    true
}

/// `auxiliary_condition_met()` (`c-RCP/src/server.c:621-660`).
fn auxiliary_condition_met(req: &PendingRequest, ctx: &TickCtx<'_>) -> bool {
    match req.kind {
        SchedKind::CompoundWait => compound_wait_match(
            req.compound_wait_evt,
            &req.compound_wait_target,
            ctx.current_status,
        ),
        SchedKind::Compound | SchedKind::Triggered | SchedKind::Chained => ctx.endpoint_idle,
        SchedKind::Timed => ctx.gptp_locked,
        SchedKind::Standard | SchedKind::Cancellation => true,
    }
}

/// `delay_expired()` (`c-RCP/src/server.c:664-697`).
fn delay_expired(req: &PendingRequest, ctx: &TickCtx<'_>, elapsed: u32) -> bool {
    match req.kind {
        SchedKind::Compound | SchedKind::CompoundWait => {
            elapsed >= u32::from(req.compound.exec_delay)
        }
        SchedKind::Triggered => req.triggered.exec_delay_elapsed(elapsed),
        SchedKind::Timed => timed_due(req.presentation_time, ctx.gptp_now),
        SchedKind::Chained => elapsed >= u32::from(req.chain_exec_delay),
        SchedKind::Standard | SchedKind::Cancellation => true,
    }
}

/// The completion action and repetition rule of
/// `rcp_server_endpoint_complete()`; `true` iff `req` stays stored.
fn complete_one(req: &mut PendingRequest, ctx: &TickCtx<'_>, seqs: &mut SequencerBank) -> bool {
    let elapsed = ctx.now.wrapping_sub(req.armed_at);
    let repeat = match req.kind {
        SchedKind::Compound => {
            let _ = compound_tick(seqs, &req.compound, elapsed);
            &mut req.compound.repeat_count
        }
        SchedKind::CompoundWait => {
            let met = compound_wait_match(
                req.compound_wait_evt,
                &req.compound_wait_target,
                ctx.current_status,
            );
            let _ = compound_wait_tick(seqs, &req.compound, met);
            &mut req.compound.repeat_count
        }
        SchedKind::Triggered => {
            let step = req.triggered;
            let _ = req.triggered_runtime.tick(&step, elapsed, true);
            &mut req.triggered.repeat_count
        }
        _ => return false,
    };
    if !complete_repetition(repeat) {
        return false;
    }
    req.armed = false;
    req.armed_at = ctx.now;
    if req.kind == SchedKind::Triggered {
        req.triggered_runtime.enter_started();
    }
    true
}

// ── Per-stream safety state (c-RCP src/e2e.c:563-737) ────────────────────────

/// How many request-stream configuration rows (and so per-stream trackers)
/// the server holds: `RCP_REGMAP_REQUEST_STREAM_CFG_MAX_ENTRIES`.
//fusa:req REQ-SRVCFG-001
pub const REQUEST_STREAM_CFG_MAX_ENTRIES: usize = 64;

/// How many response-queue configuration rows (TC18 RC5 §12.7.9 Table 27)
/// the server holds: `RCP_REGMAP_RESPONSE_QUEUE_CFG_MAX_ENTRIES`.
//fusa:req REQ-RMAP-034
pub const RESPONSE_QUEUE_CFG_MAX_ENTRIES: usize = 64;

/// How many EP_ID_config rows the server holds:
/// `RCP_REGMAP_EP_ID_MAP_MAX_ENTRIES`.
//fusa:req REQ-SRVCFG-002
pub const EP_ID_MAP_MAX_ENTRIES: usize = 64;

/// How many distinct streams the CRC fault tracker can latch:
/// `RCP_E2E_STREAM_FAULT_TRACKER_MAX_STREAMS`.
//fusa:req REQ-E2E-021
pub const STREAM_FAULT_TRACKER_MAX_STREAMS: usize = 16;

/// How many members of one frame are dispatched:
/// `RCP_MOCK_MAX_FRAME_MEMBERS`.
//fusa:req REQ-MOCK-019
pub const MAX_FRAME_MEMBERS: usize = 32;

/// One request stream's TC18 RC5 Table 24 `rx_stream_status` aggregate:
/// `rcp_e2e_stream_status_t` (`c-RCP/include/rcp/e2e.h`). See this
/// module's doc comment, "Blocked-latch semantics", for why only
/// `crc_faulted` blocks anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-E2E-046
pub struct StreamStatus {
    /// CRC cause (latched only under `rx_enforce_e2e`).
    pub crc_faulted: bool,
    /// Sequence cause (latched only when it entered safe state).
    pub seq_blocked: bool,
    /// Watchdog cause (latched only when it entered safe state).
    pub wd_blocked: bool,
    /// Storage-overflow cause (latched only when it entered safe state).
    pub overflow_blocked: bool,
}

impl StreamStatus {
    /// The aggregate bit: any of the four latches
    /// (`rcp_e2e_stream_status_rx_blocked()`).
    //fusa:req REQ-E2E-046
    pub fn rx_blocked(&self) -> bool {
        self.crc_faulted || self.seq_blocked || self.wd_blocked || self.overflow_blocked
    }
}

/// The per-stream-id CRC fault latch: `rcp_e2e_stream_fault_tracker_t`
/// (`c-RCP/src/e2e.c:598-668`). Bounded to
/// [`STREAM_FAULT_TRACKER_MAX_STREAMS`] streams.
#[derive(Debug, Clone, Default)]
struct StreamFaultTracker {
    slots: Vec<(u64, bool)>,
}

impl StreamFaultTracker {
    /// `rcp_e2e_stream_fault_tracker_on_crc_error()`: latches under
    /// `rx_enforce_e2e`; `false` (unchanged) when full.
    //fusa:req REQ-E2E-021
    //fusa:req REQ-E2E-022
    fn on_crc_error(&mut self, stream_id: u64, rx_enforce_e2e: bool) -> bool {
        let index = match self.slots.iter().position(|s| s.0 == stream_id) {
            Some(i) => i,
            None if self.slots.len() < STREAM_FAULT_TRACKER_MAX_STREAMS => {
                self.slots.push((stream_id, false));
                self.slots.len() - 1
            }
            None => return false,
        };
        self.slots[index].1 |= rx_enforce_e2e;
        true
    }

    //fusa:req REQ-E2E-021
    fn is_faulted(&self, stream_id: u64) -> bool {
        self.slots.iter().any(|s| s.0 == stream_id && s.1)
    }

    //fusa:req REQ-E2E-023
    fn reset(&mut self, stream_id: u64) {
        for slot in self.slots.iter_mut().filter(|s| s.0 == stream_id) {
            slot.1 = false;
        }
    }
}

/// One evaluation of a request stream's watchdog: `rcp_e2e_wd_result_t`
/// (`c-RCP/include/rcp/e2e.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-WDG-010
pub struct WatchdogCheck {
    /// The watchdog is enabled and has expired.
    pub overflowed: bool,
    /// ... and `rx_wd_safestate_enable` drives safe state.
    pub enter_safe_state: bool,
    /// ... and `rx_wd_info_enable` asks for a notification.
    pub notify: bool,
}

/// `rcp_regmap_ep_id_map_row_init_default()` (`c-RCP/src/regmap.c:2551`):
/// request stream 1, EP0, `byte_bus_id` 0.
fn default_ep_id_map() -> Vec<EpByteBusIdMapEntry> {
    vec![EpByteBusIdMapEntry {
        map_stream_index: 1,
        map_ep_nr: 0,
        map_byte_bus_id: 0,
    }]
}

/// `rcp_regmap_ep_id_map_byte_bus_ids_for_stream()`
/// (`c-RCP/src/regmap.c:2616`): the distinct `byte_bus_id`s bound to
/// `stream_index`, in table order.
fn byte_bus_ids_for_stream(map: &[EpByteBusIdMapEntry], stream_index: u8) -> Vec<u16> {
    let mut out: Vec<u16> = Vec::new();
    for row in map.iter().filter(|r| r.map_stream_index == stream_index) {
        if !out.contains(&row.map_byte_bus_id) {
            out.push(row.map_byte_bus_id);
        }
    }
    out
}

// ── Dispatch results ─────────────────────────────────────────────────────────

/// The outcome of dispatching one frame member: c-RCP's
/// `rcp_mock_dispatch_result_t` (`c-RCP/include/rcp/mock.h:916-995`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
//fusa:req REQ-MOCK-021
pub enum DispatchResult {
    /// Executed now.
    Ok,
    /// Queued on a disabled endpoint.
    Queued,
    /// Dropped before admission (reserved for the lifecycle gate).
    Dropped,
    /// The member did not decode, or no endpoint is registered at its
    /// `(stream_id, byte_bus_id)`: dropped without a response.
    ErrUnknownBus,
    /// Stored in the request store.
    Pending,
    /// A cancellation was applied.
    Cancelled,
    /// Rejected at admission.
    Rejected,
    /// A chained member with no predecessor.
    ChainError,
    /// A chained member whose chain was aborted.
    ChainAborted,
    /// CRC check failed (reserved for the E2E unwrap stage).
    CrcError,
    /// The stream is latched faulted by an earlier CRC failure.
    StreamFaulted,
    /// The frame's sequence number was rejected by `rx_enforce_seq`.
    SeqError,
}

/// One member's dispatch outcome: `rcp_mock_frame_member_result_t`.
#[derive(Debug, Clone, PartialEq, Eq)]
//fusa:req REQ-MOCK-019
pub struct FrameMemberResult {
    /// The outcome.
    pub result: DispatchResult,
    /// The member's `byte_bus_id` (`0` if it did not decode).
    pub byte_bus_id: u16,
    /// The encoded ACF response to send, if any.
    pub response: Option<Vec<u8>>,
}

/// The outcome of [`RcServer::dispatch_frame`] for one whole AVTPDU.
#[derive(Debug, Clone, PartialEq, Default)]
//fusa:req REQ-MOCK-019
//fusa:req REQ-MOCK-020
pub struct FrameDispatch {
    /// The requesting stream the frame was dispatched under (`None` if the
    /// AVTPDU header did not decode).
    pub requester: Option<StreamId>,
    /// The request frame's AVTPDU `sequence_num`.
    pub sequence_num: Option<u8>,
    /// A frame-level failure: the AVTPDU header did not decode, or the ACF
    /// payload stream was empty or malformed. No member was dispatched.
    pub error: Option<RcpError>,
    /// Every dispatched member's outcome, in frame order.
    pub members: Vec<FrameMemberResult>,
}

impl FrameDispatch {
    fn failed(error: RcpError) -> Self {
        Self {
            error: Some(error),
            ..Self::default()
        }
    }

    /// Every member's response, concatenated in frame order — the ACF
    /// payload of the one response AVTPDU (TC18 §12.9.1.1). Empty when no
    /// member produced a response.
    //fusa:req REQ-MOCK-019
    pub fn response_bytes(&self) -> Vec<u8> {
        self.members
            .iter()
            .filter_map(|m| m.response.as_deref())
            .flatten()
            .copied()
            .collect()
    }
}

/// Who sent a frame to [`RcServer::dispatch_frame`], and when.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
//fusa:req REQ-MOCK-031
pub struct Peer {
    /// The requesting client's stream identity; `None` takes it from the
    /// AVTPDU header's own `stream_id` (a network transport).
    pub stream_id: Option<StreamId>,
    /// Time of receipt (watchdog kick, discovery-claim expiry).
    pub received_at: Instant,
}

impl Peer {
    /// A peer identified by the frame's own header, received at `now`.
    //fusa:req REQ-MOCK-031
    pub fn from_header(now: Instant) -> Self {
        Self {
            stream_id: None,
            received_at: now,
        }
    }

    /// An in-process peer with a caller-established identity, received now.
    //fusa:req REQ-MOCK-031
    pub fn stream(stream_id: StreamId) -> Self {
        Self {
            stream_id: Some(stream_id),
            received_at: Instant::now(),
        }
    }
}

/// A request executed by [`RcServer::tick`] or [`RcServer::drain_endpoint`].
#[derive(Debug, Clone, PartialEq, Eq)]
//fusa:req REQ-MOCK-016
pub struct Executed {
    /// The encoded ACF response, if the execution produced one.
    pub response: Option<Vec<u8>>,
}

// ── RcServer ──────────────────────────────────────────────────────────────────

/// Which request store a request addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StoreKey {
    Ep0,
    Device(EndpointId),
}

/// One registered device endpoint's slot.
#[derive(Debug, Clone)]
struct EndpointSlot {
    byte_bus_id: u16,
    store: ServerEndpoint,
}

/// Frame-order chain bookkeeping for [`RcServer::dispatch_frame`].
#[derive(Debug, Default)]
struct ChainState {
    aborted: bool,
    prev_errored: bool,
    group: u32,
    position: usize,
}

/// The dispatch-time state behind one lock.
struct ServerCore {
    slots: HashMap<EndpointId, EndpointSlot>,
    ep0: ServerEndpoint,
    request_stream_cfg: Vec<RequestStreamConfigEntry>,
    seq_tracker: Vec<SequenceTracker>,
    stream_status: Vec<StreamStatus>,
    watchdog_kick: Vec<Option<Instant>>,
    ep_id_map: Vec<EpByteBusIdMapEntry>,
    sequencers: SequencerBank,
    fault_tracker: StreamFaultTracker,
    discovery_claim: Option<DiscoveryClaim>,
    discovery_timeout: Duration,
    time_sync_supported: bool,
    /// The in-process client's next request `sequence_num` per request
    /// stream (keyed by `stream_id`) for [`RcServer::dispatch_message`].
    /// Held under the core lock so a number is assigned and gated in one
    /// critical section.
    message_seq: HashMap<u64, u8>,
    /// The response-queue configuration table (`response_queue_cfg[]`,
    /// `c-RCP/src/mock.c:193`).
    response_queue_cfg: Vec<ResponseStreamConfigEntry>,
    /// Per response queue, the last-transmit moment in microseconds once
    /// seeded (`response_queue_seeded[]` / `_last_transmit_us[]`,
    /// `c-RCP/src/mock.c:213-214`).
    response_queue_last_transmit_us: Vec<Option<u64>>,
}

impl ServerCore {
    fn new() -> Self {
        Self {
            slots: HashMap::new(),
            ep0: ServerEndpoint::new(true),
            request_stream_cfg: Vec::new(),
            seq_tracker: vec![SequenceTracker::new(); REQUEST_STREAM_CFG_MAX_ENTRIES],
            stream_status: vec![StreamStatus::default(); REQUEST_STREAM_CFG_MAX_ENTRIES],
            watchdog_kick: vec![None; REQUEST_STREAM_CFG_MAX_ENTRIES],
            ep_id_map: default_ep_id_map(),
            sequencers: SequencerBank::new(0),
            fault_tracker: StreamFaultTracker::default(),
            discovery_claim: None,
            discovery_timeout: discovery::DISCOVERY_TIME_OUT,
            time_sync_supported: false,
            message_seq: HashMap::new(),
            response_queue_cfg: Vec::new(),
            response_queue_last_transmit_us: vec![None; RESPONSE_QUEUE_CFG_MAX_ENTRIES],
        }
    }

    /// `rcp_regmap_request_stream_cfg_resolve_index()`: the 1-based
    /// request-stream index of `stream`, or `0`.
    fn resolve_index(&self, stream: StreamId) -> u8 {
        self.request_stream_cfg
            .iter()
            .position(|c| c.rx_stream_id == stream.to_u64())
            .map_or(0, |i| (i + 1) as u8) // safe: i < REQUEST_STREAM_CFG_MAX_ENTRIES (64)
    }

    fn cfg(&self, index: u8) -> Option<&RequestStreamConfigEntry> {
        usize::from(index)
            .checked_sub(1)
            .and_then(|i| self.request_stream_cfg.get(i))
    }

    fn status_mut(&mut self, index: u8) -> Option<&mut StreamStatus> {
        usize::from(index)
            .checked_sub(1)
            .and_then(|i| self.stream_status.get_mut(i))
    }

    fn store_mut(&mut self, key: StoreKey) -> Option<&mut ServerEndpoint> {
        match key {
            StoreKey::Ep0 => Some(&mut self.ep0),
            StoreKey::Device(id) => self.slots.get_mut(&id).map(|s| &mut s.store),
        }
    }

    fn store(&self, key: StoreKey) -> Option<&ServerEndpoint> {
        match key {
            StoreKey::Ep0 => Some(&self.ep0),
            StoreKey::Device(id) => self.slots.get(&id).map(|s| &s.store),
        }
    }

    /// REQ-RMAP-048/049: suppress a response whose stream routing pointer
    /// is `0` (`suppress_response_per_stream_cfg()`, `c-RCP/src/mock.c:1716`).
    //fusa:req REQ-RMAP-048
    //fusa:req REQ-RMAP-049
    fn suppress(&self, stream: StreamId, response: Option<Vec<u8>>) -> Option<Vec<u8>> {
        let bytes = response?;
        (self.route(stream, &bytes) != ResponseRoute::Suppress).then_some(bytes)
    }

    /// See [`RcServer::response_route`].
    //fusa:req REQ-RMAP-048
    //fusa:req REQ-RMAP-049
    //fusa:req REQ-SRVCFG-003
    fn route(&self, stream: StreamId, response: &[u8]) -> ResponseRoute {
        let Some(cfg) = self.cfg(self.resolve_index(stream)) else {
            return ResponseRoute::Direct;
        };
        let Some(hdr) = peek_header(response) else {
            return ResponseRoute::Direct;
        };
        let index = match hdr.response_kind() {
            ResponseKind::Acknowledge => cfg.rx_ack_stream_index,
            _ => cfg.rx_resp_stream_index,
        };
        if index == 0 {
            ResponseRoute::Suppress
        } else if usize::from(index) <= self.response_queue_cfg.len() {
            ResponseRoute::Queue(index)
        } else {
            ResponseRoute::Direct
        }
    }
}

/// Where a response to a request on a given request stream goes, by that
/// stream's `rx_ack_stream_index` (an Acknowledge) or
/// `rx_resp_stream_index` (every other response) (TC18 RC5 §12.7.7 Table
/// 24): see [`RcServer::response_route`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
//fusa:req REQ-SRVCFG-003
pub enum ResponseRoute {
    /// The governing pointer is `0`: "no acknowledge / response is to be
    /// sent". [`RcServer::dispatch_frame`] has already dropped it.
    Suppress,
    /// Into the 1-based response queue (a [`ResponseStreamConfigEntry`]
    /// row installed by [`RcServer::set_response_queue_cfg`]).
    Queue(u8),
    /// No response queue applies — the request stream is unconfigured,
    /// the response did not classify, or the pointer names no installed
    /// response-queue row: sent straight back to the requester.
    Direct,
}

/// An OPEN Alliance TC18 RC Server: lifecycle state, general registers,
/// root client, the endpoint table, every endpoint's request store, and
/// the per-request-stream safety configuration and state
/// (`rcp_mock_server_t`, `c-RCP/src/mock.c:94-270`). See this module's doc
/// comment.
pub struct RcServer {
    state: Mutex<RcServerState>,
    general: Mutex<GeneralRegisters>,
    root_client: Mutex<Option<StreamId>>,
    endpoints: Mutex<EndpointTable>,
    endpoint_impls: Mutex<HashMap<EndpointId, Arc<dyn Endpoint>>>,
    next_endpoint_id: AtomicU32,
    /// Free-running NTSCF `sequence_num` for [`Self::handle_ntscf_frame`]'s
    /// response frames (this server's own simplification).
    sequence_num: AtomicU32,
    core: Mutex<ServerCore>,
}

impl RcServer {
    /// A fresh RC Server at [`RcServerState::INITIAL`], no root client, no
    /// endpoints, no request streams configured, an EP_ID_config holding
    /// only its power-on default row, and no sequencers.
    //fusa:req REQ-MOCKSRV-001
    pub fn new(general: GeneralRegisters) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(RcServerState::INITIAL),
            general: Mutex::new(general),
            root_client: Mutex::new(None),
            endpoints: Mutex::new(EndpointTable::new()),
            endpoint_impls: Mutex::new(HashMap::new()),
            next_endpoint_id: AtomicU32::new(0),
            sequence_num: AtomicU32::new(0),
            core: Mutex::new(ServerCore::new()),
        })
    }

    /// This server's current lifecycle state.
    //fusa:req REQ-MOCKSRV-002
    pub fn state(&self) -> RcServerState {
        *self.state.lock().unwrap()
    }

    /// A snapshot of this server's current [`GeneralRegisters`].
    //fusa:req REQ-MOCKSRV-001
    pub fn general_registers(&self) -> GeneralRegisters {
        *self.general.lock().unwrap()
    }

    /// Move from the current state to `target` on behalf of `writer`
    /// ([`RcServerState::try_transition`]; c-RCP
    /// `rcp_mock_server_transition`, `c-RCP/src/mock.c:363`). Unchanged on
    /// failure.
    //fusa:req REQ-MOCKSRV-002
    pub fn try_transition(
        &self,
        target: RcServerState,
        snap: Option<&PlausibilitySnapshot<'_>>,
        writer: WriterCtx,
        all_other_eps_idle: bool,
    ) -> Result<(), LifecycleError> {
        let mut state = self.state.lock().unwrap();
        *state = state.try_transition(target, snap, writer, all_other_eps_idle)?;
        Ok(())
    }

    /// Designate `stream` (or nobody) as the root client — the one stream
    /// permitted to write EP0.
    //fusa:req REQ-MOCKSRV-004
    pub fn set_root_client(&self, stream: Option<StreamId>) {
        *self.root_client.lock().unwrap() = stream;
    }

    /// Register `endpoint` under `(stream_id, byte_bus_id)`, enabled, with
    /// an empty request store, and return its fresh [`EndpointId`].
    ///
    /// Returns [`EndpointTable::register`]'s error —
    /// `Err(RcpError::InvalidSize)` for an oversized `byte_bus_id`,
    /// `Err(RcpError::EpError)` for an already-registered pair — without
    /// allocating an id or storing anything.
    //fusa:req REQ-MOCKSRV-003
    //fusa:req REQ-MOCK-031
    pub fn register_endpoint(
        &self,
        stream_id: StreamId,
        byte_bus_id: u16,
        endpoint: Arc<dyn Endpoint>,
    ) -> Result<EndpointId, RcpError> {
        let mut core = self.core.lock().unwrap();
        let id = EndpointId(self.next_endpoint_id.load(Ordering::SeqCst));
        self.endpoints
            .lock()
            .unwrap()
            .register(stream_id, byte_bus_id, id)?;
        self.next_endpoint_id.fetch_add(1, Ordering::SeqCst);
        self.endpoint_impls.lock().unwrap().insert(id, endpoint);
        core.slots.insert(
            id,
            EndpointSlot {
                byte_bus_id,
                store: ServerEndpoint::new(true),
            },
        );
        Ok(id)
    }

    fn store_key(&self, stream: StreamId, byte_bus_id: u16) -> Option<StoreKey> {
        match route_byte_bus_id(byte_bus_id) {
            RequestRoute::Ep0 => Some(StoreKey::Ep0),
            RequestRoute::DeviceEndpoint => self
                .endpoints
                .lock()
                .unwrap()
                .lookup(stream, byte_bus_id)
                .map(StoreKey::Device),
        }
    }

    /// Run `f` on the store at `(stream, byte_bus_id)`, if registered.
    fn with_store<R>(
        &self,
        stream: StreamId,
        byte_bus_id: u16,
        f: impl FnOnce(&mut ServerEndpoint) -> R,
    ) -> Option<R> {
        let key = self.store_key(stream, byte_bus_id)?;
        self.core.lock().unwrap().store_mut(key).map(f)
    }

    /// Set a registered endpoint's `ep_enable`; `false` if unregistered
    /// (`rcp_mock_server_set_endpoint_enable_on_stream()`).
    //fusa:req REQ-MOCK-010
    pub fn set_endpoint_enable(&self, stream: StreamId, byte_bus_id: u16, enable: bool) -> bool {
        self.with_store(stream, byte_bus_id, |s| s.set_enable(enable))
            .is_some()
    }

    /// Set a registered endpoint's sleep-request admission suspension;
    /// `false` if unregistered.
    //fusa:req REQ-PWRMODE-028
    pub fn set_endpoint_admission_suspended(
        &self,
        stream: StreamId,
        byte_bus_id: u16,
        suspended: bool,
    ) -> bool {
        self.with_store(stream, byte_bus_id, |s| {
            s.set_admission_suspended(suspended)
        })
        .is_some()
    }

    /// Queued (undrained) standard requests on an endpoint; `0` if
    /// unregistered (`rcp_mock_server_endpoint_queue_len()`).
    //fusa:req REQ-MOCK-011
    pub fn endpoint_queue_len(&self, stream: StreamId, byte_bus_id: u16) -> usize {
        self.with_store(stream, byte_bus_id, |s| s.queue_len())
            .unwrap_or(0)
    }

    /// Stored conditional requests on an endpoint; `0` if unregistered
    /// (`rcp_mock_server_pending_count_on_stream()`).
    //fusa:req REQ-MOCK-027
    pub fn pending_count(&self, stream: StreamId, byte_bus_id: u16) -> usize {
        self.with_store(stream, byte_bus_id, |s| s.pending_count())
            .unwrap_or(0)
    }

    /// Apply the watchdog purge to one endpoint; returns how many requests
    /// were removed (`rcp_mock_server_watchdog_purge_on_stream()`).
    //fusa:req REQ-MOCK-026
    pub fn watchdog_purge(&self, stream: StreamId, byte_bus_id: u16) -> usize {
        self.with_store(stream, byte_bus_id, |s| s.watchdog_purge())
            .unwrap_or(0)
    }

    /// Dequeue and execute the oldest queued request of a re-enabled
    /// endpoint. `None` if unregistered, disabled or empty
    /// (`rcp_mock_server_drain_endpoint_on_stream()`).
    //fusa:req REQ-MOCK-016
    //fusa:req REQ-MOCK-017
    //fusa:req REQ-MOCK-018
    pub fn drain_endpoint(&self, stream: StreamId, byte_bus_id: u16) -> Option<Executed> {
        let frame = self.with_store(stream, byte_bus_id, |s| s.drain_one())??;
        Some(Executed {
            response: self.run_handler(stream, &frame),
        })
    }

    /// Execute at most one due stored request on an endpoint: select the
    /// highest-priority due request against this server's own sequencer
    /// bank, run it (a cancellation is applied instead), and finalize it.
    /// `None` when unregistered or nothing is due
    /// (`rcp_mock_server_tick_on_stream()`, `c-RCP/src/mock.c:3587-3668`).
    //fusa:req REQ-MOCK-023
    //fusa:req REQ-MOCK-024
    pub fn tick(&self, stream: StreamId, byte_bus_id: u16, ctx: &TickCtx<'_>) -> Option<Executed> {
        let key = self.store_key(stream, byte_bus_id)?;
        let mut guard = self.core.lock().unwrap();
        let core = &mut *guard;
        let store = match key {
            StoreKey::Ep0 => &mut core.ep0,
            StoreKey::Device(id) => &mut core.slots.get_mut(&id)?.store,
        };
        let index = store.select_due(ctx, &core.sequencers)?;
        let req = store.pending(index)?.clone();
        let response = if req.kind == SchedKind::Cancellation {
            apply_cancellation(store, req.request_type, &req.frame, byte_bus_id)
        } else {
            self.run_handler(stream, &req.frame)
        };
        let _ = store.complete(index, ctx, &mut core.sequencers);
        Some(Executed { response })
    }

    /// Offer one trigger occurrence to every endpoint's store; returns the
    /// total that counted it (`rcp_mock_server_notify_trigger()`).
    //fusa:req REQ-MOCK-025
    pub fn notify_trigger(&self, source_ep: u8, signal_nr: u8) -> usize {
        let mut core = self.core.lock().unwrap();
        let ep0 = core.ep0.notify_trigger(source_ep, signal_nr);
        ep0 + core
            .slots
            .values_mut()
            .map(|s| s.store.notify_trigger(source_ep, signal_nr))
            .sum::<usize>()
    }

    /// Replace the sequencer bank with a fresh one of `count` sequencers
    /// and sync `svr_sequencers_max` (`rcp_mock_server_set_sequencer_count()`).
    //fusa:req REQ-MOCK-022
    pub fn set_sequencer_count(&self, count: u8) {
        self.core.lock().unwrap().sequencers = SequencerBank::new(count);
        self.general.lock().unwrap().svr_sequencers_max = count;
    }

    /// Run `f` against this server's own sequencer bank
    /// (`rcp_mock_server_sequencers()`).
    //fusa:req REQ-MOCK-022
    pub fn with_sequencers<R>(&self, f: impl FnOnce(&mut SequencerBank) -> R) -> R {
        f(&mut self.core.lock().unwrap().sequencers)
    }

    /// Install the request-stream configuration table (TC18 §12.7.7 Table
    /// 24) and sync `svr_request_stream_cfg`'s capacity. More than
    /// [`REQUEST_STREAM_CFG_MAX_ENTRIES`] rows is
    /// `Err(RcpError::InvalidSize)`, leaving the table unchanged
    /// (`rcp_mock_server_set_request_stream_cfg()`). Per-stream trackers are
    /// index-parallel with the rows and are not reset.
    //fusa:req REQ-SRVCFG-001
    pub fn set_request_stream_cfg(
        &self,
        entries: &[RequestStreamConfigEntry],
    ) -> Result<(), RcpError> {
        if entries.len() > REQUEST_STREAM_CFG_MAX_ENTRIES {
            return Err(RcpError::InvalidSize);
        }
        self.core.lock().unwrap().request_stream_cfg = entries.to_vec();
        self.general.lock().unwrap().svr_request_stream_cfg.capacity = entries.len() as u16; // safe: <= 64 rows
        Ok(())
    }

    /// The installed request-stream configuration table.
    //fusa:req REQ-SRVCFG-001
    pub fn request_stream_cfg(&self) -> Vec<RequestStreamConfigEntry> {
        self.core.lock().unwrap().request_stream_cfg.clone()
    }

    /// The 1-based request-stream index `stream` resolves to, or `0`
    /// (`rcp_regmap_request_stream_cfg_resolve_index()`).
    //fusa:req REQ-SRVCFG-001
    pub fn request_stream_index(&self, stream: StreamId) -> u8 {
        self.core.lock().unwrap().resolve_index(stream)
    }

    /// Install the EP_ID_config endpoint↔stream map (TC18 §12.7.8 Table
    /// 23) and sync `svr_ep_bytebus_id_map`'s capacity. More than
    /// [`EP_ID_MAP_MAX_ENTRIES`] rows is `Err(RcpError::InvalidSize)`
    /// (`rcp_mock_server_set_ep_id_map()`).
    //fusa:req REQ-SRVCFG-002
    pub fn set_ep_id_map(&self, entries: &[EpByteBusIdMapEntry]) -> Result<(), RcpError> {
        if entries.len() > EP_ID_MAP_MAX_ENTRIES {
            return Err(RcpError::InvalidSize);
        }
        self.core.lock().unwrap().ep_id_map = entries.to_vec();
        self.general.lock().unwrap().svr_ep_bytebus_id_map.capacity = entries.len() as u16; // safe: <= 64 rows
        Ok(())
    }

    /// The installed EP_ID_config map (power-on: one default row binding
    /// request stream 1 to EP0).
    //fusa:req REQ-SRVCFG-002
    pub fn ep_id_map(&self) -> Vec<EpByteBusIdMapEntry> {
        self.core.lock().unwrap().ep_id_map.clone()
    }

    /// Install the response-queue configuration table (TC18 RC5 §12.7.9
    /// Table 27) and sync `svr_response_stream_cfg`'s capacity. More than
    /// [`RESPONSE_QUEUE_CFG_MAX_ENTRIES`] rows is
    /// `Err(RcpError::InvalidSize)`, leaving the table unchanged
    /// (`rcp_mock_server_set_response_queue_cfg()`, `c-RCP/src/mock.c:463`).
    /// The heartbeat bookkeeping is index-parallel with the rows and is not
    /// reset, as in c-RCP.
    //fusa:req REQ-RMAP-034
    pub fn set_response_queue_cfg(
        &self,
        entries: &[ResponseStreamConfigEntry],
    ) -> Result<(), RcpError> {
        if entries.len() > RESPONSE_QUEUE_CFG_MAX_ENTRIES {
            return Err(RcpError::InvalidSize);
        }
        self.core.lock().unwrap().response_queue_cfg = entries.to_vec();
        self.general
            .lock()
            .unwrap()
            .svr_response_stream_cfg
            .capacity = entries.len() as u16; // safe: <= 64 rows
        Ok(())
    }

    /// The installed response-queue configuration table.
    //fusa:req REQ-RMAP-034
    pub fn response_queue_cfg(&self) -> Vec<ResponseStreamConfigEntry> {
        self.core.lock().unwrap().response_queue_cfg.clone()
    }

    /// Where `response` — built for a request received on `stream` — is
    /// routed, by the request stream's `rx_ack_stream_index` (Acknowledge)
    /// or `rx_resp_stream_index` (everything else): the same
    /// classification [`Self::dispatch_frame`] suppresses by
    /// (`suppress_response_per_stream_cfg()`, `c-RCP/src/mock.c:1716`).
    ///
    /// c-RCP parity: c-RCP consumes the two pointers only for suppression
    /// and owns no transport to deliver on the selected response stream
    /// (`mock.c:1690-1699`); this crate also routes a non-zero pointer that
    /// names an installed response-queue row to that queue
    /// ([`crate::udp::UdpRcServer`] transmits it). Everything c-RCP sends
    /// straight back (unconfigured stream, unclassifiable response, a
    /// pointer past the installed rows) stays [`ResponseRoute::Direct`].
    //fusa:req REQ-SRVCFG-003
    pub fn response_route(&self, stream: StreamId, response: &[u8]) -> ResponseRoute {
        self.core.lock().unwrap().route(stream, response)
    }

    /// The response-queue heartbeat (TC18 RC5 §12.7.9 `Flush_time`,
    /// §13.7.1.1): `Some(heartbeat)` once `response_stream_index`'s
    /// `Flush_time` has elapsed since its last transmission, where the
    /// heartbeat is an empty NTSCF AVTPDU on the queue's stream
    /// ([`crate::respqueue::response_queue_stream_id`] with `mac`), and the
    /// queue's last-transmit time becomes `now_us`
    /// (`rcp_mock_server_check_response_queue_heartbeat()`,
    /// `c-RCP/src/mock.c:483-534`).
    ///
    /// `None` for an index outside `1..=` the installed rows, on the first
    /// check of a queue (which only seeds its last-transmit time to
    /// `now_us`), for a `now_us` earlier than the last transmission
    /// (non-monotonic input), and while `Flush_time` (`0`: never) has not
    /// elapsed.
    ///
    /// c-RCP parity: the heartbeat's NTSCF `sequence_num` is always `0`
    /// (c-RCP encodes a zeroed header with only `sv` and `stream_id` set,
    /// `mock.c:517-521`), not the queue stream's running sequence number;
    /// copied as-is.
    //fusa:req REQ-RMAP-065
    //fusa:req REQ-SRV-017
    pub fn check_response_queue_heartbeat(
        &self,
        response_stream_index: u8,
        mac: [u8; 6],
        now_us: u64,
    ) -> Option<Vec<u8>> {
        let mut core = self.core.lock().unwrap();
        let idx = usize::from(response_stream_index).checked_sub(1)?;
        let cfg = *core.response_queue_cfg.get(idx)?;
        let slot = &mut core.response_queue_last_transmit_us[idx];
        let Some(last) = *slot else {
            *slot = Some(now_us);
            return None;
        };
        let elapsed = now_us.checked_sub(last)?;
        if !crate::respqueue::should_flush_by_time(elapsed, u64::from(cfg.resp_flush_time)) {
            return None;
        }
        let stream = crate::respqueue::response_queue_stream_id(&cfg, mac);
        let heartbeat = encode_ntscf_frame(stream, 0, &[]).ok()?;
        *slot = Some(now_us);
        Some(heartbeat)
    }

    /// Record that `response_stream_index` transmitted at `now_us`,
    /// restarting its `Flush_time` interval ("time since the last
    /// transmission", TC18 RC5 §12.7.9). Ignored for an index outside the
    /// installed rows.
    ///
    /// c-RCP parity: c-RCP advances this bookkeeping only from its
    /// heartbeat check (`mock.c:527-529`), having no transmit path of its
    /// own; [`crate::udp::UdpRcServer`] calls this after a
    /// `flush_on_count`-triggered transmission.
    //fusa:req REQ-RMAP-064
    pub fn note_response_queue_transmit(&self, response_stream_index: u8, now_us: u64) {
        let mut core = self.core.lock().unwrap();
        let count = core.response_queue_cfg.len();
        if let Some(idx) = usize::from(response_stream_index)
            .checked_sub(1)
            .filter(|&i| i < count)
        {
            core.response_queue_last_transmit_us[idx] = Some(now_us);
        }
    }

    /// Whether gPTP time synchronization is established. While `false`
    /// (the default) a Timed request is rejected with `GPTP_FAIL`
    /// (REQ-WIREERR-006, `c-RCP/src/mock.c:1569`).
    //fusa:req REQ-WIREERR-006
    pub fn set_time_sync_supported(&self, supported: bool) {
        self.core.lock().unwrap().time_sync_supported = supported;
    }

    /// The discovery-stream claim currently held, if any
    /// (`rcp_mock_server_discovery_claim()`).
    //fusa:req REQ-UDP-010
    pub fn discovery_claim(&self) -> Option<DiscoveryClaim> {
        self.core.lock().unwrap().discovery_claim
    }

    /// Set the discovery-claim timeout
    /// (`rcp_mock_server_set_discovery_timeout_us()`).
    //fusa:req REQ-UDP-010
    pub fn set_discovery_timeout(&self, timeout: Duration) {
        self.core.lock().unwrap().discovery_timeout = timeout;
    }

    /// The `rx_stream_status` latches of `stream`'s request stream, or
    /// `None` if `stream` is not a configured request stream.
    //fusa:req REQ-E2E-046
    pub fn stream_status(&self, stream: StreamId) -> Option<StreamStatus> {
        let mut core = self.core.lock().unwrap();
        let index = core.resolve_index(stream);
        core.status_mut(index).copied()
    }

    /// The aggregate `rx_stream_status` bit; `false` for an unconfigured
    /// stream (`rcp_mock_server_stream_status_rx_blocked()`).
    //fusa:req REQ-E2E-046
    pub fn stream_status_rx_blocked(&self, stream: StreamId) -> bool {
        self.stream_status(stream).is_some_and(|s| s.rx_blocked())
    }

    /// Whether `stream` is latched faulted by a CRC failure under
    /// `rx_enforce_e2e` (every later request is then answered
    /// `POCI_FAILURE` until released).
    //fusa:req REQ-E2E-021
    pub fn stream_faulted(&self, stream: StreamId) -> bool {
        self.core
            .lock()
            .unwrap()
            .fault_tracker
            .is_faulted(stream.to_u64())
    }

    /// Release `stream`: clear its CRC fault latch and all four
    /// `rx_stream_status` latches (`rcp_e2e_stream_fault_tracker_reset()`
    /// plus the four `rcp_e2e_stream_status_reset_*()`).
    //fusa:req REQ-E2E-023
    pub fn release_stream(&self, stream: StreamId) {
        let mut core = self.core.lock().unwrap();
        core.fault_tracker.reset(stream.to_u64());
        let index = core.resolve_index(stream);
        if let Some(status) = core.status_mut(index) {
            *status = StreamStatus::default();
        }
    }

    /// Record one CRC failure on `stream`: latches the stream faulted and
    /// its `rx_stream_status` CRC cause under the stream's `rx_enforce_e2e`,
    /// and then enters safe state on the stream (TC18 §12.7.7 Table 24's
    /// two consequences of the same bit). Called by the E2E unwrap stage
    /// (`c-RCP/src/mock.c:1957-2016`).
    ///
    /// Reads `rx_enforce_e2e` from the request-stream configuration, not
    /// from a per-endpoint stand-in as c-RCP's mock does
    /// (`c-RCP/src/mock.c:83`): an unconfigured stream is treated as
    /// `rx_enforce_e2e = 0` (drop the request only).
    //fusa:req REQ-E2E-021
    //fusa:req REQ-E2E-045
    //fusa:req REQ-E2E-046
    #[allow(dead_code)] // wired by the E2E unwrap stage (issues #180, #88)
    pub(crate) fn note_crc_error(&self, stream: StreamId) -> usize {
        let mut core = self.core.lock().unwrap();
        let index = core.resolve_index(stream);
        let enforce = core.cfg(index).is_some_and(|c| c.rx_enforce_e2e);
        let _ = core.fault_tracker.on_crc_error(stream.to_u64(), enforce);
        if let Some(status) = core.status_mut(index) {
            status.crc_faulted |= enforce;
        }
        if enforce {
            self.broadcast_locked(&mut core, index)
        } else {
            0
        }
    }

    /// The instant `request_stream_index`'s watchdog was last kicked by a
    /// received request, if ever.
    //fusa:req REQ-WDG-010
    pub fn last_watchdog_kick(&self, request_stream_index: u8) -> Option<Instant> {
        let core = self.core.lock().unwrap();
        usize::from(request_stream_index)
            .checked_sub(1)
            .and_then(|i| core.watchdog_kick.get(i).copied().flatten())
    }

    /// Evaluate `request_stream_index`'s watchdog against a caller-measured
    /// `elapsed_since_last_kick_ms` (see [`Self::last_watchdog_kick`]):
    /// overflowed iff `rx_wd_enable` and `elapsed >= rx_wd_timeout_interval`
    /// (read as milliseconds, as c-RCP's `rx_wd_timeout_ms`). Latches the
    /// `rx_stream_status` watchdog cause when it enters safe state, and then
    /// enters safe state on the stream. `None` for an unconfigured index
    /// (`rcp_mock_server_check_watchdog()`, `c-RCP/src/mock.c:992-1022`).
    //fusa:req REQ-WDG-010
    //fusa:req REQ-E2E-046
    pub fn check_watchdog(
        &self,
        request_stream_index: u8,
        elapsed_since_last_kick_ms: u64,
    ) -> Option<WatchdogCheck> {
        let mut core = self.core.lock().unwrap();
        let cfg = *core.cfg(request_stream_index)?;
        let overflowed =
            cfg.rx_wd_enable && elapsed_since_last_kick_ms >= u64::from(cfg.rx_wd_timeout_interval);
        let check = WatchdogCheck {
            overflowed,
            enter_safe_state: overflowed && cfg.rx_wd_safestate_enable,
            notify: overflowed && cfg.rx_wd_info_enable,
        };
        if check.enter_safe_state {
            if let Some(status) = core.status_mut(request_stream_index) {
                status.wd_blocked = true;
            }
            let _ = self.broadcast_locked(&mut core, request_stream_index);
        }
        Some(check)
    }

    /// Enter safe state on every endpoint EP_ID_config binds to
    /// `request_stream_index`: purge each one's non-safety stored requests
    /// (only the safety sequence survives). Returns how many were purged
    /// (`rcp_mock_server_broadcast_safe_state()`, `c-RCP/src/mock.c:3739`).
    //fusa:req REQ-MOCK-033
    //fusa:req REQ-E2E-029
    //fusa:req REQ-E2E-030
    //fusa:req REQ-E2E-045
    pub fn broadcast_safe_state(&self, request_stream_index: u8) -> usize {
        let mut core = self.core.lock().unwrap();
        self.broadcast_locked(&mut core, request_stream_index)
    }

    /// [`Self::broadcast_safe_state`] with the core lock held.
    ///
    /// c-RCP parity: `rcp_mock_server_broadcast_safe_state()`
    /// (`c-RCP/src/mock.c:3739-3781`) purges only non-safety requests
    /// (`rcp_server_endpoint_watchdog_purge`, `c-RCP/src/server.c:1022`)
    /// for every safe-state cause (sequence gap, storage overflow,
    /// watchdog), and for an EP_ID_config row naming an unconfigured
    /// request stream falls back to the unscoped `find_slot()`
    /// (`mock.c:3770-3777`, [`unscoped_store_key`]), which may purge a
    /// slot on another stream. TC18 RC5 defines the keep-0x8F / clear-0x0F
    /// rule only for a watchdog overflow (§11.2.2.2, TC18-11.2.2.2-006),
    /// and request storage is per request stream (§11.2.3.2,
    /// TC18-11.2.3.2-001).
    fn broadcast_locked(&self, core: &mut ServerCore, request_stream_index: u8) -> usize {
        if request_stream_index == 0 {
            return 0;
        }
        let stream = core
            .cfg(request_stream_index)
            .map(|c| StreamId::from_u64(c.rx_stream_id));
        let bound = byte_bus_ids_for_stream(&core.ep_id_map, request_stream_index);
        let mut purged = 0;
        for byte_bus_id in bound {
            let key = match stream {
                Some(s) => self.store_key(s, byte_bus_id),
                None => unscoped_store_key(core, byte_bus_id),
            };
            purged += key
                .and_then(|k| core.store_mut(k))
                .map_or(0, ServerEndpoint::watchdog_purge);
        }
        purged
    }

    /// The once-per-frame sequence gate (`frame_seq_gate_admits()`,
    /// `c-RCP/src/mock.c:3038-3066`): latches the sequence cause and enters
    /// safe state when the evaluation says so, regardless of acceptance.
    ///
    /// c-RCP parity: `c-RCP/src/mock.c:3055-3061` broadcasts safe state on
    /// `enter_safe_state` even when the frame is accepted (a forward gap
    /// with ordering still held), and then still dispatches the gapped
    /// frame. The gap test itself follows TC18 RC5 §12.7.7 Table 24
    /// (TC18-12.7.7-001) `rx_seq_safestate_enable`: "bring all endpoints
    /// to safety state if Sequence_Nr has no single increment". c-RCP's
    /// reaction diverges from that text: it purges only the non-safety
    /// requests of the endpoints EP_ID_config binds to this one request
    /// stream ([`Self::broadcast_safe_state`]), not all endpoints, and
    /// it does not drive any endpoint to a safety state. Copied as is.
    //fusa:req REQ-E2E-028
    //fusa:req REQ-E2E-029
    fn frame_seq_gate_admits(&self, core: &mut ServerCore, stream: StreamId, seq: u8) -> bool {
        let index = core.resolve_index(stream);
        let Some(cfg) = core.cfg(index).copied() else {
            return true;
        };
        let i = usize::from(index) - 1;
        let outcome =
            core.seq_tracker[i].evaluate(cfg.rx_enforce_seq, cfg.rx_seq_safestate_enable, seq);
        if outcome.enter_safe_state {
            core.stream_status[i].seq_blocked = true;
            let _ = self.broadcast_locked(core, index);
        }
        outcome.accept
    }

    /// The single frame-level entry point: dispatch one whole received
    /// NTSCF AVTPDU from `peer`.
    ///
    /// Pipeline, mirroring `rcp_mock_server_dispatch_frame()` /
    /// `_dispatch_frame_e2e()` (`c-RCP/src/mock.c:3068-3401`) and, per
    /// member, `rcp_mock_server_dispatch_e2e()` / `dispatch_plain()`
    /// (`mock.c:1438-1760`, `:1892-2021`):
    ///
    /// 1. Decode the AVTPDU header and split the ACF payload into members
    ///    ([`split_frame_members`]); a malformed frame dispatches nothing
    ///    and reports [`FrameDispatch::error`].
    /// 2. Sequence gate, once per frame (`rx_enforce_seq`): a rejected frame
    ///    reports [`DispatchResult::SeqError`] for every member, no response.
    /// 3. Per member, in frame order: chained-member sequencing
    ///    (`CHAIN_ERROR`/`CHAIN_ABORTED`); watchdog kick; the CRC fault
    ///    latch (`POCI_FAILURE`); discovery; endpoint lookup by
    ///    `(stream_id, byte_bus_id)` (unknown: dropped silently); admission
    ///    into the endpoint's request store (execute / queue / store /
    ///    cancel / reject), the Timed `GPTP_FAIL` gate, the storage-overflow
    ///    safe-state escalation and the sequencer ownership gate; then the
    ///    `rx_ack_stream_index`/`rx_resp_stream_index` suppression.
    ///
    /// Never panics and never fails as a whole: every outcome is reported
    /// per member (c-RCP's per-member results), so one malformed datagram
    /// cannot stop a server loop.
    ///
    /// c-RCP parity: when a frame carries more than [`MAX_FRAME_MEMBERS`]
    /// members, c-RCP processes only the first `MAX_FRAME_MEMBERS - 1`
    /// (`c-RCP/src/mock.c:3121-3133`, where the last stored offset has no
    /// known end); copied as is.
    //fusa:req REQ-MOCK-019
    //fusa:req REQ-MOCK-020
    //fusa:req REQ-MOCK-029
    //fusa:req REQ-MOCKSRV-008
    pub fn dispatch_frame(&self, frame: &[u8], peer: &Peer) -> FrameDispatch {
        let mut core = self.core.lock().unwrap();
        self.dispatch_frame_locked(&mut core, frame, peer)
    }

    /// [`Self::dispatch_frame`] with the core lock already held.
    fn dispatch_frame_locked(
        &self,
        core: &mut ServerCore,
        frame: &[u8],
        peer: &Peer,
    ) -> FrameDispatch {
        let (hdr, acf) = match decode_ntscf_frame(frame) {
            Ok(decoded) => decoded,
            Err(e) => return FrameDispatch::failed(e),
        };
        let stream = peer
            .stream_id
            .unwrap_or_else(|| StreamId::from_u64(hdr.stream_id));
        let mut ranges = split_frame_members(acf);
        let mut dispatch = FrameDispatch {
            requester: Some(stream),
            sequence_num: Some(hdr.sequence_num),
            ..FrameDispatch::default()
        };
        if ranges.is_empty() {
            dispatch.error = Some(RcpError::ShortFrame);
            return dispatch;
        }
        if ranges.len() > MAX_FRAME_MEMBERS {
            ranges.truncate(MAX_FRAME_MEMBERS - 1);
        }
        if !self.frame_seq_gate_admits(core, stream, hdr.sequence_num) {
            dispatch.members = ranges
                .iter()
                .map(|_| member(DispatchResult::SeqError, 0, None))
                .collect();
            return dispatch;
        }
        let mut chain = ChainState::default();
        for (i, range) in ranges.into_iter().enumerate() {
            let m =
                self.dispatch_in_frame(core, &mut chain, i, stream, &acf[range], peer.received_at);
            dispatch.members.push(m);
        }
        dispatch
    }

    /// One member of [`Self::dispatch_frame`]'s loop, with the chain
    /// bookkeeping of `c-RCP/src/mock.c:3143-3233`.
    fn dispatch_in_frame(
        &self,
        core: &mut ServerCore,
        chain: &mut ChainState,
        i: usize,
        stream: StreamId,
        bytes: &[u8],
        now: Instant,
    ) -> FrameMemberResult {
        let Some(byte_bus_id) = peek_member_byte_bus_id(bytes) else {
            chain.prev_errored = true;
            return member(DispatchResult::ErrUnknownBus, 0, None);
        };
        let chained = is_chained_member(bytes);
        if chained.is_none() {
            chain.group = i as u32 + 1; // safe: i < MAX_FRAME_MEMBERS (32)
            chain.position = 0;
        } else {
            chain.position += 1;
        }
        if let Some(early) =
            chained.and_then(|(cs, tn)| chain_gate(chain, i > 0, cs, byte_bus_id, tn))
        {
            chain.prev_errored = true;
            return early;
        }
        let (result, response) = self.dispatch_member(core, stream, byte_bus_id, bytes, now);
        chain.prev_errored = matches!(
            result,
            DispatchResult::ErrUnknownBus
                | DispatchResult::Rejected
                | DispatchResult::Dropped
                | DispatchResult::CrcError
        );
        if result == DispatchResult::Pending {
            self.tag_chain(core, stream, byte_bus_id, chain);
        }
        member(result, byte_bus_id, response)
    }

    /// Record chain group/position on the request just stored
    /// (REQ-CANCEL-012, `c-RCP/src/mock.c:3218-3227`).
    //fusa:req REQ-CANCEL-012
    fn tag_chain(
        &self,
        core: &mut ServerCore,
        stream: StreamId,
        byte_bus_id: u16,
        chain: &ChainState,
    ) {
        let Some(store) = self
            .store_key(stream, byte_bus_id)
            .and_then(|k| core.store_mut(k))
        else {
            return;
        };
        let Some(last) = store.last_pending_index() else {
            return;
        };
        let _ = store.chain_predecessor_done(last, 0);
        if let Some(Some(req)) = store.pending.get_mut(last) {
            req.chain_group = chain.group;
            req.chain_position = chain.position as u8; // safe: < MAX_FRAME_MEMBERS (32)
        }
    }

    /// One member's checks after chain sequencing (`dispatch_e2e()` →
    /// `dispatch_plain()` → suppression).
    //fusa:req REQ-WDG-010
    //fusa:req REQ-E2E-021
    fn dispatch_member(
        &self,
        core: &mut ServerCore,
        stream: StreamId,
        byte_bus_id: u16,
        bytes: &[u8],
        now: Instant,
    ) -> (DispatchResult, Option<Vec<u8>>) {
        let index = core.resolve_index(stream);
        if let Some(kick) = usize::from(index)
            .checked_sub(1)
            .and_then(|i| core.watchdog_kick.get_mut(i))
        {
            *kick = Some(now);
        }
        let (result, response) = if core.fault_tracker.is_faulted(stream.to_u64()) {
            let tn = peek_header(bytes).map(|h| h.transaction_num);
            let response =
                tn.and_then(|tn| wire_error_response(byte_bus_id, tn, &RcpError::PociFailure));
            (DispatchResult::StreamFaulted, response)
        } else if let Some(outcome) = self.dispatch_discovery(core, stream, bytes, now) {
            outcome
        } else {
            self.dispatch_plain(core, stream, byte_bus_id, bytes)
        };
        (result, core.suppress(stream, response))
    }

    /// Discovery requests and broadcast-sentinel misuse
    /// ([`crate::udp::UdpRcServer`]'s former `dispatch_request`, now owned
    /// by the server that holds the claim). `None` for every other request.
    //fusa:req REQ-UDP-009
    //fusa:req REQ-UDP-010
    //fusa:req REQ-UDP-011
    fn dispatch_discovery(
        &self,
        core: &mut ServerCore,
        stream: StreamId,
        bytes: &[u8],
        now: Instant,
    ) -> Option<(DispatchResult, Option<Vec<u8>>)> {
        let abb = decode_acf_abb(bytes).ok();
        let outcome = match &abb {
            Some(msg) if discovery::is_discovery_request(msg) => {
                Some((msg, self.discovery_read(core, stream, msg, now)))
            }
            Some(msg) if discovery::is_discovery_configure_request(msg) => {
                Some((msg, discovery_configure(core, stream, msg, now)))
            }
            _ => None,
        };
        if let Some((msg, result)) = outcome {
            return Some(answer(&msg.info, result));
        }
        if !discovery::is_discovery_broadcast_stream_id(stream) {
            return None;
        }
        let info = peek_header(bytes)?;
        Some(answer(&info, Err(RcpError::InvalidParameter)))
    }

    fn discovery_read(
        &self,
        core: &ServerCore,
        stream: StreamId,
        msg: &AcfAbbMessage,
        now: Instant,
    ) -> Result<AcfAbbMessage, RcpError> {
        discovery::check_discovery_access(
            core.discovery_claim,
            stream,
            DiscoveryAccessKind::Read,
            now,
            core.discovery_timeout,
        )?;
        discovery::build_discovery_response(&msg.info, self.state(), &self.general_registers())
    }

    /// `dispatch_plain_inner()` (`c-RCP/src/mock.c:1438-1714`) after the
    /// lifecycle gate (not yet modelled here).
    //fusa:req REQ-MOCK-013
    //fusa:req REQ-MOCK-021
    //fusa:req REQ-MOCK-030
    //fusa:req REQ-E2E-030
    fn dispatch_plain(
        &self,
        core: &mut ServerCore,
        stream: StreamId,
        byte_bus_id: u16,
        bytes: &[u8],
    ) -> (DispatchResult, Option<Vec<u8>>) {
        let Some(key) = self.store_key(stream, byte_bus_id) else {
            // TC18 §12.9.1: dropped without further notification.
            return (DispatchResult::ErrUnknownBus, None);
        };
        let time_sync = core.time_sync_supported;
        let Some(store) = core.store_mut(key) else {
            return (DispatchResult::ErrUnknownBus, None);
        };
        let admission = store.admit(bytes, 0, None);
        if let Some(rejected) = reject_unsynced_timed(store, &admission, time_sync, byte_bus_id) {
            return rejected;
        }
        if admission.error == Some(RcpError::ReqStorageOvfl) {
            self.escalate_overflow(core, stream);
        }
        if let Some(rejected) = reject_unowned_sequencer(core, key, stream, &admission, byte_bus_id)
        {
            return rejected;
        }
        self.finish_admission(core, key, stream, byte_bus_id, bytes, admission)
    }

    /// REQ-E2E-030: a storage overflow escalates to the whole request
    /// stream under `rx_ovrflw_safestate_enable` (`c-RCP/src/mock.c:1588-1607`).
    //fusa:req REQ-E2E-030
    fn escalate_overflow(&self, core: &mut ServerCore, stream: StreamId) {
        let index = core.resolve_index(stream);
        let Some(enter) = core.cfg(index).map(|c| c.rx_ovrflw_safestate_enable) else {
            return;
        };
        if let Some(status) = core.status_mut(index) {
            status.overflow_blocked |= enter;
        }
        if enter {
            let _ = self.broadcast_locked(core, index);
        }
    }

    /// `finish_admission()` (`c-RCP/src/mock.c:1361-1426`).
    //fusa:req REQ-MOCK-014
    //fusa:req REQ-MOCK-015
    //fusa:req REQ-SRV-016
    fn finish_admission(
        &self,
        core: &mut ServerCore,
        key: StoreKey,
        stream: StreamId,
        byte_bus_id: u16,
        bytes: &[u8],
        admission: Admission,
    ) -> (DispatchResult, Option<Vec<u8>>) {
        match admission.result {
            AdmitResult::ExecuteNow => (DispatchResult::Ok, self.run_handler(stream, bytes)),
            AdmitResult::Queued => (DispatchResult::Queued, admission.ack),
            AdmitResult::Pending => (DispatchResult::Pending, admission.ack),
            AdmitResult::Cancellation => {
                let response = core.store_mut(key).and_then(|s| {
                    apply_cancellation(s, admission.request_type, bytes, byte_bus_id)
                });
                (DispatchResult::Cancelled, response)
            }
            AdmitResult::Rejected | AdmitResult::Suspended => (
                DispatchResult::Rejected,
                admission_reject_response(admission.error.as_ref(), bytes, byte_bus_id),
            ),
        }
    }

    /// Execute one standard request against its endpoint (`run_handler()`,
    /// `c-RCP/src/mock.c:1142`): an ACF_ABB or ACF_GBB message is answered
    /// through [`Self::handle_abb`]; a dispatch error with a Table 27 code
    /// becomes an `err = 1` response, any other error no response.
    fn run_handler(&self, stream: StreamId, frame: &[u8]) -> Option<Vec<u8>> {
        let request = request_message(frame)?;
        let response = match self.handle_abb(stream, &request) {
            Ok(response) => response,
            Err(e) => build_error_response(&request.info, &e)?,
        };
        encode_acf_abb(&response).ok()
    }

    /// Answer one already-decoded [`AcfAbbMessage`] request from
    /// `stream_id` — the execution step [`Self::dispatch_frame`] runs once
    /// admission decides a request executes now.
    ///
    /// Routing follows [`crate::ep0::route_byte_bus_id`]:
    ///
    /// - [`RequestRoute::Ep0`]: every EP0 access is treated as a General
    ///   access. A read is gated by [`check_ep0_access_for_stream`] and
    ///   returns the current [`GeneralRegisters::encode`] snapshot. A write
    ///   is always rejected with `Err(RcpError::LockedMemAccess)`: TC18 RC5
    ///   §12.7.5 Table 20 marks every General register read-only, as c-RCP's
    ///   `rcp_regmap_general_decode_write_request` does
    ///   (`c-RCP/src/regmap.c:262`, REQ-RMAP-025).
    /// - [`RequestRoute::DeviceEndpoint`]: resolved through
    ///   [`EndpointTable::lookup`] (`Err(RcpError::EpNotFound)` if
    ///   unregistered), then [`Endpoint::read`]/[`Endpoint::write`].
    ///
    /// Every response echoes `request.info.byte_bus_id`
    /// ([`build_response_info`], checked by [`verify_echo_back`]).
    //fusa:req REQ-MOCKSRV-004
    //fusa:req REQ-MOCKSRV-005
    //fusa:req REQ-MOCKSRV-006
    //fusa:req REQ-MOCKSRV-007
    pub(crate) fn handle_abb(
        &self,
        stream_id: StreamId,
        request: &AcfAbbMessage,
    ) -> Result<AcfAbbMessage, RcpError> {
        let response_payload = match route_byte_bus_id(request.info.byte_bus_id) {
            RequestRoute::Ep0 => self.handle_ep0(stream_id, request)?,
            RequestRoute::DeviceEndpoint => self.handle_device(stream_id, request)?,
        };
        let response_info = build_response_info(&request.info, request.info);
        verify_echo_back(&request.info, &response_info)?;
        Ok(AcfAbbMessage {
            info: response_info,
            payload: response_payload,
        })
    }

    fn handle_ep0(
        &self,
        stream_id: StreamId,
        request: &AcfAbbMessage,
    ) -> Result<Vec<u8>, RcpError> {
        let state = self.state();
        let root_client = *self.root_client.lock().unwrap();
        if request.info.op {
            // TC18 Table 20 / c-RCP REQ-RMAP-025: the General block is
            // read-only, LOCKED_MEM_ACCESS for every writer.
            let writer = WriterCtx {
                via_root_client_ep0: crate::ep0::is_root_client(root_client, stream_id),
                ..WriterCtx::default()
            };
            return Err(check_field_writable(state, FieldKind::ReadOnly, writer)
                .err()
                .unwrap_or(RcpError::LockedMemAccess));
        }
        check_ep0_access_for_stream(
            state,
            RegisterCategory::General,
            &request.info,
            stream_id,
            root_client,
        )?;
        Ok(self.general.lock().unwrap().encode().to_vec())
    }

    fn handle_device(
        &self,
        stream_id: StreamId,
        request: &AcfAbbMessage,
    ) -> Result<Vec<u8>, RcpError> {
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
            Ok(Vec::new())
        } else {
            endpoint.read(request.info.read_size_segment.as_read_size())
        }
    }

    /// Answer one whole NTSCF-framed request from `stream_id`: a thin
    /// wrapper over [`Self::dispatch_frame`] that frames every member's
    /// response into one NTSCF AVTPDU under `stream_id`, with this
    /// server's own free-running `sequence_num`.
    ///
    /// Returns `Err` only for a frame-level failure
    /// ([`FrameDispatch::error`]) or a response too large to frame.
    //fusa:req REQ-MOCKSRV-008
    pub fn handle_ntscf_frame(
        &self,
        stream_id: StreamId,
        frame: &[u8],
    ) -> Result<Vec<u8>, RcpError> {
        let dispatch = self.dispatch_frame(frame, &Peer::stream(stream_id));
        if let Some(e) = dispatch.error {
            return Err(e);
        }
        let seq = self.sequence_num.fetch_add(1, Ordering::SeqCst) as u8; // safe: free-running, wraps mod 256 by design
        encode_ntscf_frame(stream_id, seq, &dispatch.response_bytes())
    }

    /// Dispatch one in-process request message through
    /// [`Self::dispatch_frame`] — the path [`mod@crate::adapt`] and the CLI use.
    ///
    /// The request is framed as a one-member NTSCF AVTPDU under
    /// `stream_id` with the in-process client's own request
    /// `sequence_num` for that stream: one free-running counter per request
    /// stream (wrapping mod 256), assigned under the core lock that also
    /// covers the dispatch, so streams never share a counter and concurrent
    /// callers on one stream are gated in assignment order. A `read_size_segment` wider than the 12-bit wire
    /// field is clamped to its maximum; a payload too large for one ACF
    /// message is `Err(RcpError::PayloadTooLarge)`. Returns the decoded
    /// response; an `err = 1` (non-Acknowledge) response becomes the
    /// matching `Err(RcpError)`, and no response at all (dropped, queued
    /// without acknowledge, suppressed) is `Err(RcpError::NotConnected)`.
    //fusa:req REQ-MOCKSRV-011
    pub fn dispatch_message(
        &self,
        stream_id: StreamId,
        request: &AcfAbbMessage,
    ) -> Result<AcfAbbMessage, RcpError> {
        let max_payload = usize::from(ACF_MSG_LENGTH_9BIT_MAX) * QUADLET_LEN - ACF_ABB_HEADER_LEN;
        if request.payload.len() > max_payload {
            return Err(RcpError::PayloadTooLarge);
        }
        let mut request = request.clone();
        request.info.read_size_segment.0 = request
            .info
            .read_size_segment
            .0
            .min(READ_SIZE_SEGMENT_12BIT_MAX);
        let body = encode_acf_abb(&request)?;
        // One counter per request stream, assigned and dispatched under
        // the same core lock: interleaving streams cannot open a gap on
        // either, and concurrent callers on one stream reach the sequence
        // gate in the order their numbers were assigned.
        let mut core = self.core.lock().unwrap();
        let next = core.message_seq.entry(stream_id.to_u64()).or_insert(0);
        let seq = *next;
        let frame = encode_ntscf_frame(stream_id, seq, &body)?;
        *next = next.wrapping_add(1);
        message_from_dispatch(self.dispatch_frame_locked(
            &mut core,
            &frame,
            &Peer::stream(stream_id),
        ))
    }
}

/// A [`FrameMemberResult`].
fn member(
    result: DispatchResult,
    byte_bus_id: u16,
    response: Option<Vec<u8>>,
) -> FrameMemberResult {
    FrameMemberResult {
        result,
        byte_bus_id,
        response,
    }
}

/// `peek_member_byte_bus_id()` (`c-RCP/src/mock.c:2950-2980`): the
/// member's `byte_bus_id` if it decodes as ACF_ABB or ACF_GBB.
fn peek_member_byte_bus_id(bytes: &[u8]) -> Option<u16> {
    match bytes.first().map(|b| b >> 1) {
        Some(ACF_ABB_MSG_TYPE) => decode_acf_abb(bytes).ok().map(|m| m.info.byte_bus_id),
        Some(ACF_GBB_MSG_TYPE) => decode_acf_gbb(bytes).ok().map(|m| m.info.byte_bus_id),
        _ => None,
    }
}

/// `is_chained_member()` (`c-RCP/src/mock.c:2987-3008`): `(cs,
/// transaction_num)` of a decodable chained member.
fn is_chained_member(bytes: &[u8]) -> Option<(bool, u8)> {
    if peek_request_type(bytes).ok()? != RequestKind::Chained.to_u8() {
        return None;
    }
    decode_chained_member(bytes)
        .ok()
        .map(|c| (c.cs, c.transaction_num))
}

/// `rcp_chained_advance()` (`c-RCP/src/request.c:675-695`) and the
/// resulting error response; `None` when the member may proceed.
//fusa:req REQ-MOCK-029
fn chain_gate(
    chain: &mut ChainState,
    has_predecessor: bool,
    cs: bool,
    byte_bus_id: u16,
    tn: u8,
) -> Option<FrameMemberResult> {
    let (result, error) = if !has_predecessor {
        chain.aborted = true;
        (DispatchResult::ChainError, RcpError::ChainError)
    } else if chain.aborted || (chain.prev_errored && cs) {
        chain.aborted = true;
        (DispatchResult::ChainAborted, RcpError::ChainAborted)
    } else {
        return None;
    };
    Some(member(
        result,
        byte_bus_id,
        wire_error_response(byte_bus_id, tn, &error),
    ))
}

/// The response to a discovery-path result (rust-shaped error responses,
/// unchanged from the former `UdpRcServer::dispatch_request`).
fn answer(
    info: &ByteMessageInfo,
    result: Result<AcfAbbMessage, RcpError>,
) -> (DispatchResult, Option<Vec<u8>>) {
    match result {
        Ok(response) => (DispatchResult::Ok, encode_acf_abb(&response).ok()),
        Err(e) => (
            DispatchResult::Rejected,
            build_error_response(info, &e).and_then(|r| encode_acf_abb(&r).ok()),
        ),
    }
}

/// A discovery-stream configure/claim attempt.
fn discovery_configure(
    core: &mut ServerCore,
    stream: StreamId,
    msg: &AcfAbbMessage,
    now: Instant,
) -> Result<AcfAbbMessage, RcpError> {
    let timeout = core.discovery_timeout;
    discovery::check_discovery_access(
        core.discovery_claim,
        stream,
        DiscoveryAccessKind::Configure,
        now,
        timeout,
    )?;
    let claim = discovery::try_claim_discovery_stream(core.discovery_claim, stream, now, timeout)?;
    core.discovery_claim = Some(claim);
    Ok(AcfAbbMessage {
        info: build_response_info(&msg.info, ByteMessageInfo::default()),
        payload: Vec::new(),
    })
}

/// The first registered slot (by registration order) at `byte_bus_id` on
/// any stream — c-RCP's unscoped `find_slot()` fallback for an
/// EP_ID_config row naming an unconfigured request stream
/// (`c-RCP/src/mock.c:3770-3776`).
///
/// c-RCP parity: this crosses request streams, which TC18 RC5's
/// per-stream request storage (§11.2.3.2, TC18-11.2.3.2-001) does not
/// allow; copied for the misconfigured case only, as c-RCP does.
fn unscoped_store_key(core: &ServerCore, byte_bus_id: u16) -> Option<StoreKey> {
    if route_byte_bus_id(byte_bus_id) == RequestRoute::Ep0 {
        return Some(StoreKey::Ep0);
    }
    core.slots
        .iter()
        .filter(|(_, s)| s.byte_bus_id == byte_bus_id)
        .map(|(id, _)| *id)
        .min_by_key(|id| id.0)
        .map(StoreKey::Device)
}

/// REQ-WIREERR-006: a Timed request admitted while gPTP is not established
/// is un-admitted and answered `GPTP_FAIL` (`c-RCP/src/mock.c:1569-1586`).
//fusa:req REQ-WIREERR-006
fn reject_unsynced_timed(
    store: &mut ServerEndpoint,
    admission: &Admission,
    time_sync_supported: bool,
    byte_bus_id: u16,
) -> Option<(DispatchResult, Option<Vec<u8>>)> {
    let timed = admission.result == AdmitResult::Pending
        && admission.request_type == RequestKind::Timed.to_u8();
    if !timed || time_sync_supported {
        return None;
    }
    let tn = store.pending(admission.index?)?.transaction_num;
    let _ = store.cancel_single(tn);
    Some((
        DispatchResult::Rejected,
        wire_error_response(byte_bus_id, tn, &RcpError::GptpFail),
    ))
}

/// REQ-SEQ-013: a stored compound/compound-wait step naming a sequencer its
/// request stream does not own is un-admitted and answered
/// `SEQUENCER_NOT_KNOWN`/`UNAUTHORIZED_ACCESS` (`c-RCP/src/mock.c:1649-1676`).
//fusa:req REQ-SEQ-013
//fusa:req REQ-WIREERR-005
fn reject_unowned_sequencer(
    core: &mut ServerCore,
    key: StoreKey,
    stream: StreamId,
    admission: &Admission,
    byte_bus_id: u16,
) -> Option<(DispatchResult, Option<Vec<u8>>)> {
    if admission.result != AdmitResult::Pending {
        return None;
    }
    let requester = core.resolve_index(stream);
    let req = core.store(key)?.pending(admission.index?)?;
    if !matches!(req.kind, SchedKind::Compound | SchedKind::CompoundWait) {
        return None;
    }
    let (step, tn) = (req.compound, req.transaction_num);
    let error = core
        .sequencers
        .check_compound_admission(&step, requester)
        .err()?;
    let _ = core.store_mut(key)?.cancel_single(tn);
    Some((
        DispatchResult::Rejected,
        wire_error_response(byte_bus_id, tn, &error),
    ))
}

/// `admission_reject_response_shape()` + `finish_admission()`'s rejected
/// case (`c-RCP/src/mock.c:1290-1300`, `:1394-1424`): `UNSUPPORTED_CMD` is
/// an unconditional Error Response (TC18 §13.5.1 "err-response"); every
/// other code an Acknowledge-rejected response, only if `evt[3]` asked.
///
/// c-RCP parity: `c-RCP/src/mock.c:1290-1300` and `:1394-1424` answer
/// every admission rejection other than `UNSUPPORTED_CMD` with an
/// Acknowledge-rejected response (`evt = 0xF`, `err = 1`,
/// `c-RCP/src/acf.c:338`), and send nothing when the request did not set
/// `evt[3]`. TC18 RC5 §12.9.1 (TC18-12.9.1-009) says a request that could
/// not be executed for any reason gets an `err = 1` error response unless
/// suppressed via EP_GENERIC_config; it does not make that depend on
/// `evt[3]`.
//fusa:req REQ-SRV-022
//fusa:req REQ-ACF-033
fn admission_reject_response(
    error: Option<&RcpError>,
    bytes: &[u8],
    byte_bus_id: u16,
) -> Option<Vec<u8>> {
    let error = error?;
    let hdr = peek_header(bytes)?;
    if *error == RcpError::UnsupportedCmd {
        return wire_error_response(byte_bus_id, hdr.transaction_num, error);
    }
    if !hdr.evt.ack {
        return None;
    }
    acknowledge_rejected_response(byte_bus_id, hdr.transaction_num, error)
}

/// `apply_cancellation()` (`c-RCP/src/mock.c:1180-1245`): clear-all and
/// clear-non-safestate remove from the store; clear-single removes its
/// target and cascades to the target's chained successors, or answers
/// `REQUEST_NOT_FOUND` (carrying the cancellation's own `transaction_num`).
///
/// c-RCP parity: no `REQUEST_CANCELED` response is sent for any removed
/// request (c-RCP#163), and an undecodable clear-single is ignored.
//fusa:req REQ-MOCK-028
//fusa:req REQ-CANCEL-012
fn apply_cancellation(
    store: &mut ServerEndpoint,
    request_type: u8,
    bytes: &[u8],
    byte_bus_id: u16,
) -> Option<Vec<u8>> {
    match RequestKind::from_u8(request_type) {
        Ok(RequestKind::ClearAll) => {
            let _ = store.cancel_all();
            None
        }
        Ok(RequestKind::ClearNonSafestate) => {
            let _ = store.cancel_non_safestate();
            None
        }
        Ok(RequestKind::ClearSingle) => clear_single(store, bytes, byte_bus_id),
        _ => None,
    }
}

fn clear_single(store: &mut ServerEndpoint, bytes: &[u8], byte_bus_id: u16) -> Option<Vec<u8>> {
    let req = decode_clear_single(bytes).ok()?;
    let target = req.clear_transaction_num;
    let (group, position) = store
        .pending
        .iter()
        .flatten()
        .find(|r| r.transaction_num == target)
        .map_or((0, 0), |r| (r.chain_group, r.chain_position));
    match store.cancel_single(target) {
        CancelResult::NotFound => {
            wire_error_response(byte_bus_id, req.transaction_num, &RcpError::RequestNotFound)
        }
        CancelResult::Canceled => {
            let _ = store.cancel_chain_from(group, position);
            None
        }
    }
}

/// The request message an endpoint handler executes: an ACF_ABB message
/// as is, an ACF_GBB message as its header and payload.
fn request_message(frame: &[u8]) -> Option<AcfAbbMessage> {
    match frame.first().map(|b| b >> 1) {
        Some(ACF_GBB_MSG_TYPE) => decode_acf_gbb(frame).ok().map(|m| AcfAbbMessage {
            info: m.info,
            payload: m.payload,
        }),
        _ => decode_acf_abb(frame).ok(),
    }
}

/// The first member's response of an in-process dispatch, as a message.
fn message_from_dispatch(dispatch: FrameDispatch) -> Result<AcfAbbMessage, RcpError> {
    if let Some(e) = dispatch.error {
        return Err(e);
    }
    let bytes = dispatch
        .members
        .into_iter()
        .next()
        .and_then(|m| m.response)
        .ok_or(RcpError::NotConnected)?;
    let response = decode_acf_abb(&bytes)?;
    if response.info.err && response.info.response_kind() == ResponseKind::Error {
        return Err(error_from_wire_code(response.payload.first().copied()));
    }
    Ok(response)
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests;
