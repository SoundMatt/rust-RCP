//fusa:req REQ-SHM-001
//fusa:req REQ-SHM-002
//fusa:req REQ-SHM-003
//fusa:req REQ-SHM-004
//fusa:req REQ-SHM-005
//fusa:req REQ-SHMEM-001
//fusa:req REQ-SHMEM-002
//fusa:req REQ-SHMEM-003
//fusa:req REQ-SHMEM-004
//fusa:req REQ-SHMEM-005
//fusa:req REQ-SHMEM-006
//fusa:req REQ-SHMEM-007
//fusa:req REQ-SHMEM-008
//fusa:req REQ-SHMEM-009
//fusa:req REQ-SHMEM-010

//! Shared-memory transport bridge (intra-host IPC).
//!
//! Uses a pair of in-process ring buffers protected by `Mutex` to simulate
//! shared memory. Production deployments replace these with OS shared-memory
//! regions via the [`ShmChannel`] trait.
//!
//! `ROADMAP.md` Milestone 9 ("All ADAPT-disposition packages retargeted...")
//! cutover: per this module's own ADAPT disposition ("transport is
//! byte-agnostic; just needs to carry new AVTPDU bytes instead of old wire
//! frames"), [`ShmChannel`] is unchanged — it was already a plain
//! byte-in/byte-out abstraction with no `Zone`/`Command` coupling of its
//! own — and [`ShmBridge`] is retargeted exactly the way
//! `crate::udp::UdpTransport` was retargeted by the `wire` REPLACE
//! cutover earlier in this milestone: addressed by
//! [`crate::avtp::StreamId`] instead of `Zone`, and carrying NTSCF-wrapped
//! ACF_ABB/ACF_GBB messages instead of the deleted `wire::encode_command`
//! frame.
//!
//! # [`shmem_avtp_pair_new`] — two cross-wired [`AvtpTransport`] ends (issue #205)
//!
//! Port of c-RCP's `rcp_shmem_avtp_pair_new()` (`c-RCP/include/rcp/
//! shmem.h:71-93`, `c-RCP/src/shmem.c`): two in-process transports sharing
//! one pair of bounded FIFOs, so two distinct RC Nodes (e.g. an RC Client
//! and an RC Server under test) can talk without a socket — unlike
//! [`crate::avtp::LoopbackTransport`], which echoes a single node's own
//! traffic. [`ShmBridge`]'s `send_acf_abb`/`send_acf_gbb` are now thin
//! wrappers over [`crate::avtp::request_acf_abb`]/
//! [`crate::avtp::request_acf_gbb`] on its [`ShmChannel`] pair.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::acf::{AcfAbbMessage, AcfGbbMessage};
use crate::avtp::{self, AvtpTransport, StreamId};
use crate::RcpError;

// ── ShmChannel trait ──────────────────────────────────────────────────────────

/// Abstract shared-memory channel for testability.
//fusa:req REQ-SHM-001
pub trait ShmChannel: Send + Sync {
    fn write(&self, data: &[u8]) -> Result<(), RcpError>;
    fn read(&self, timeout: Option<Duration>) -> Result<Vec<u8>, RcpError>;
}

// ── In-process ring buffer implementation ─────────────────────────────────────

/// Simple in-process FIFO channel (for tests and integration).
//fusa:req REQ-SHM-002
pub struct InProcChannel {
    buf: std::sync::Mutex<std::collections::VecDeque<Vec<u8>>>,
    cvar: std::sync::Condvar,
}

impl InProcChannel {
    pub fn new() -> Arc<Self> {
        Arc::new(InProcChannel {
            buf: std::sync::Mutex::new(std::collections::VecDeque::new()),
            cvar: std::sync::Condvar::new(),
        })
    }
}

impl Default for InProcChannel {
    fn default() -> Self {
        InProcChannel {
            buf: std::sync::Mutex::new(std::collections::VecDeque::new()),
            cvar: std::sync::Condvar::new(),
        }
    }
}

impl ShmChannel for InProcChannel {
    fn write(&self, data: &[u8]) -> Result<(), RcpError> {
        self.buf.lock().unwrap().push_back(data.to_vec());
        self.cvar.notify_one();
        Ok(())
    }

    fn read(&self, timeout: Option<Duration>) -> Result<Vec<u8>, RcpError> {
        let mut buf = self.buf.lock().unwrap();
        let result = match timeout {
            None => loop {
                if let Some(v) = buf.pop_front() {
                    break Ok(v);
                }
                buf = self.cvar.wait(buf).unwrap();
            },
            Some(d) => {
                let deadline = std::time::Instant::now() + d;
                loop {
                    if let Some(v) = buf.pop_front() {
                        break Ok(v);
                    }
                    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                    if remaining.is_zero() {
                        break Err(RcpError::Timeout);
                    }
                    let (b, _) = self.cvar.wait_timeout(buf, remaining).unwrap();
                    buf = b;
                }
            }
        };
        result
    }
}

// ── Cross-wired AVTPDU transport pair ─────────────────────────────────────────

/// Shared state of one [`shmem_avtp_pair_new`] pair (c-RCP's
/// `rcp_shmem_pair_core_t`, `c-RCP/src/shmem.c:14-33`). Index `0` is side
/// A, `1` is side B; `queues[i]` holds the frames side `i` has sent and the
/// other side has not yet received.
struct PairCore {
    closed: [bool; 2],
    queues: [VecDeque<Vec<u8>>; 2],
}

struct PairShared {
    core: Mutex<PairCore>,
    cv: Condvar,
    capacity: usize,
    time_sync_supported: bool,
}

/// One side of a [`shmem_avtp_pair_new`] pair — port of c-RCP's
/// `rcp_shmem_side_t` (`c-RCP/src/shmem.c:35-39`): whatever one side
/// sends, the other side receives.
//fusa:req REQ-SHMEM-001
pub struct ShmemAvtpTransport {
    shared: Arc<PairShared>,
    side: usize,
}

/// Create two cross-wired [`AvtpTransport`] ends sharing one pair of
/// bounded FIFOs of `queue_capacity` frames each way (`0` is treated as `1`)
/// — port of `rcp_shmem_avtp_pair_new()` (`c-RCP/include/rcp/shmem.h:
/// 71-93`, `c-RCP/src/shmem.c:196-260`). Both sides share
/// `time_sync_supported`. Dropping one side leaves the other usable; the
/// shared state is freed once both are dropped. c-RCP's
/// `RCP_SHMEM_ERR_ALLOC` has no counterpart (allocation failure aborts).
//fusa:req REQ-SHMEM-001
//fusa:req REQ-SHMEM-008
//fusa:req REQ-SHMEM-009
pub fn shmem_avtp_pair_new(
    time_sync_supported: bool,
    queue_capacity: usize,
) -> (ShmemAvtpTransport, ShmemAvtpTransport) {
    let shared = Arc::new(PairShared {
        core: Mutex::new(PairCore {
            closed: [false; 2],
            queues: [VecDeque::new(), VecDeque::new()],
        }),
        cv: Condvar::new(),
        capacity: queue_capacity.max(1),
        time_sync_supported,
    });
    (
        ShmemAvtpTransport {
            shared: Arc::clone(&shared),
            side: 0,
        },
        ShmemAvtpTransport { shared, side: 1 },
    )
}

impl ShmemAvtpTransport {
    fn lock(&self) -> std::sync::MutexGuard<'_, PairCore> {
        self.shared.core.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl AvtpTransport for ShmemAvtpTransport {
    /// Enqueue a copy for the peer (`shmem_side_send()`, `shmem.c:51-77`):
    /// `Err(Closed)` once this side is closed, `Err(Busy)` when this side's
    /// outgoing queue is full. A closed *peer* does not fail the send, as in
    /// c-RCP.
    //fusa:req REQ-SHMEM-002
    //fusa:req REQ-SHMEM-003
    //fusa:req REQ-SHMEM-005
    //fusa:req REQ-SHMEM-006
    fn send_frame(&self, frame: &[u8]) -> Result<(), RcpError> {
        let mut core = self.lock();
        if core.closed[self.side] {
            return Err(RcpError::Closed);
        }
        if core.queues[self.side].len() >= self.shared.capacity {
            return Err(RcpError::Busy);
        }
        core.queues[self.side].push_back(frame.to_vec());
        self.shared.cv.notify_all();
        Ok(())
    }

    /// Dequeue what the peer sent (`shmem_side_recv()`, `shmem.c:85-133`).
    /// Frames queued before either side closed stay deliverable; once
    /// drained, a closed side (own or peer) is `Err(Closed)`, otherwise the
    /// deadline is `Err(Timeout)`. A frame larger than `buf` is left queued
    /// and `Err(Busy)` returned.
    //fusa:req REQ-SHMEM-002
    //fusa:req REQ-SHMEM-003
    //fusa:req REQ-SHMEM-004
    //fusa:req REQ-SHMEM-007
    //fusa:req REQ-SHMEM-010
    fn recv_frame(&self, deadline: Option<Instant>, buf: &mut [u8]) -> Result<usize, RcpError> {
        let peer = 1 - self.side;
        let mut core = self.lock();
        while core.queues[peer].is_empty()
            && !core.closed[self.side]
            && !core.closed[peer]
            && !avtp::deadline_done(deadline)
        {
            core = match avtp::remaining(deadline) {
                Some(r) => {
                    self.shared
                        .cv
                        .wait_timeout(core, r)
                        .unwrap_or_else(|e| e.into_inner())
                        .0
                }
                None => self.shared.cv.wait(core).unwrap_or_else(|e| e.into_inner()),
            };
        }
        let closed = core.closed[self.side] || core.closed[peer];
        let Some(item) = core.queues[peer].front() else {
            return Err(if closed {
                RcpError::Closed
            } else {
                RcpError::Timeout
            });
        };
        let n = item.len();
        let dst = buf.get_mut(..n).ok_or(RcpError::Busy)?;
        dst.copy_from_slice(item);
        core.queues[peer].pop_front();
        Ok(n)
    }

    /// Close this side and wake every waiter on the pair
    /// (`shmem_side_close()`, `shmem.c:139-151`).
    //fusa:req REQ-SHMEM-005
    //fusa:req REQ-SHMEM-010
    fn close(&self) -> Result<(), RcpError> {
        self.lock().closed[self.side] = true;
        self.shared.cv.notify_all();
        Ok(())
    }

    //fusa:req REQ-SHMEM-001
    fn time_sync_supported(&self) -> bool {
        self.shared.time_sync_supported
    }
}

/// [`ShmBridge`]'s compatibility adapter: a `tx`/`rx` [`ShmChannel`] pair as
/// an [`AvtpTransport`]. A receive is a single [`ShmChannel::read`] whose
/// error is returned unchanged — this wrapper's historical behavior.
struct ShmChannelLink<'a> {
    tx: &'a dyn ShmChannel,
    rx: &'a dyn ShmChannel,
}

impl AvtpTransport for ShmChannelLink<'_> {
    fn send_frame(&self, frame: &[u8]) -> Result<(), RcpError> {
        self.tx.write(frame)
    }

    fn recv_frame(&self, deadline: Option<Instant>, buf: &mut [u8]) -> Result<usize, RcpError> {
        let frame = self.rx.read(avtp::remaining(deadline))?;
        let dst = buf.get_mut(..frame.len()).ok_or(RcpError::Busy)?;
        dst.copy_from_slice(&frame);
        Ok(frame.len())
    }

    fn close(&self) -> Result<(), RcpError> {
        Ok(())
    }

    fn time_sync_supported(&self) -> bool {
        false
    }
}

// ── ShmBridge ─────────────────────────────────────────────────────────────────

/// Shared-memory bridge, addressed by `local_stream`
/// ([`crate::avtp::StreamId`]) rather than the legacy `Zone`.
///
/// The caller must wire `tx` (write) and `rx` (read) channels to the peer process.
//fusa:req REQ-SHM-003
pub struct ShmBridge {
    local_stream: StreamId,
    tx: Arc<dyn ShmChannel>,
    rx: Arc<dyn ShmChannel>,
}

impl ShmBridge {
    pub fn new(local_stream: StreamId, tx: Arc<dyn ShmChannel>, rx: Arc<dyn ShmChannel>) -> Self {
        ShmBridge {
            local_stream,
            tx,
            rx,
        }
    }

    /// This bridge's local [`StreamId`].
    pub fn local_stream(&self) -> StreamId {
        self.local_stream
    }

    fn link(&self) -> ShmChannelLink<'_> {
        ShmChannelLink {
            tx: self.tx.as_ref(),
            rx: self.rx.as_ref(),
        }
    }

    /// Send an ACF_ABB request wrapped in an NTSCF frame, and decode the
    /// ACF_ABB response, verifying it echoes the request's `byte_bus_id` —
    /// the same framing `crate::udp::UdpTransport::send_acf_abb` uses,
    /// over an [`ShmChannel`] pair instead of a UDP socket.
    //fusa:req REQ-SHM-004
    pub fn send_acf_abb(
        &self,
        msg: &AcfAbbMessage,
        sequence_num: u8,
        timeout: Option<Duration>,
    ) -> Result<AcfAbbMessage, RcpError> {
        avtp::request_acf_abb(&self.link(), self.local_stream, msg, sequence_num, timeout)
    }

    /// Same as [`Self::send_acf_abb`], for an ACF_GBB request/response pair.
    //fusa:req REQ-SHM-004
    pub fn send_acf_gbb(
        &self,
        msg: &AcfGbbMessage,
        sequence_num: u8,
        timeout: Option<Duration>,
    ) -> Result<AcfGbbMessage, RcpError> {
        avtp::request_acf_gbb(&self.link(), self.local_stream, msg, sequence_num, timeout)
    }

    /// No-op, matching this module's pre-Milestone-9 behavior.
    //fusa:req REQ-SHM-005
    pub fn close(&self) -> Result<(), RcpError> {
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::acf::{self, ByteMessageInfo};

    fn local_stream() -> StreamId {
        StreamId::new([0x02, 0x11, 0x22, 0x33, 0x44, 0x55], 0x0002)
    }

    fn make_bridge() -> ShmBridge {
        let tx = InProcChannel::new() as Arc<dyn ShmChannel>;
        let rx = InProcChannel::new() as Arc<dyn ShmChannel>;
        ShmBridge::new(local_stream(), tx, rx)
    }

    fn request(byte_bus_id: u16) -> AcfAbbMessage {
        AcfAbbMessage {
            info: ByteMessageInfo {
                byte_bus_id,
                op: true,
                ..Default::default()
            },
            payload: vec![0x01, 0x02],
        }
    }

    fn queue_response(rx: &Arc<dyn ShmChannel>, byte_bus_id: u16) {
        let resp = AcfAbbMessage {
            info: ByteMessageInfo {
                byte_bus_id,
                rsp: true,
                ..Default::default()
            },
            payload: vec![0xAA],
        };
        let payload = acf::encode_acf_abb(&resp).unwrap();
        let frame = avtp::encode_ntscf_frame(local_stream(), 1, &payload).unwrap();
        rx.write(&frame).unwrap();
    }

    #[test]
    //fusa:test REQ-SHM-003
    //fusa:test REQ-SHM-004
    fn shm_bridge_send_acf_abb_ok() {
        let tx = InProcChannel::new() as Arc<dyn ShmChannel>;
        let rx = InProcChannel::new() as Arc<dyn ShmChannel>;
        queue_response(&rx, 7);
        let b = ShmBridge::new(local_stream(), tx, rx);
        let resp = b.send_acf_abb(&request(7), 0, None).unwrap();
        assert_eq!(resp.info.byte_bus_id, 7);
        assert!(resp.info.rsp);
    }

    #[test]
    //fusa:test REQ-SHM-004
    fn shm_bridge_rejects_echo_back_mismatch() {
        let tx = InProcChannel::new() as Arc<dyn ShmChannel>;
        let rx = InProcChannel::new() as Arc<dyn ShmChannel>;
        queue_response(&rx, 99); // mismatched byte_bus_id
        let b = ShmBridge::new(local_stream(), tx, rx);
        let err = b.send_acf_abb(&request(7), 0, None).unwrap_err();
        assert_eq!(err, RcpError::EpError);
    }

    #[test]
    //fusa:test REQ-SHM-004
    fn zero_timeout_rejected() {
        let b = make_bridge();
        let err = b
            .send_acf_abb(&request(7), 0, Some(Duration::ZERO))
            .unwrap_err();
        assert_eq!(err, RcpError::Timeout);
    }

    #[test]
    //fusa:test REQ-SHM-001
    //fusa:test REQ-SHM-002
    fn in_proc_channel_fifo() {
        let ch = InProcChannel::new();
        ch.write(b"first").unwrap();
        ch.write(b"second").unwrap();
        assert_eq!(ch.read(None).unwrap(), b"first");
        assert_eq!(ch.read(None).unwrap(), b"second");
    }

    #[test]
    //fusa:test REQ-SHM-002
    fn in_proc_channel_timeout() {
        let ch = InProcChannel::new();
        let err = ch.read(Some(Duration::from_millis(10))).unwrap_err();
        assert_eq!(err, RcpError::Timeout);
    }

    #[test]
    //fusa:test REQ-SHM-005
    fn close_is_noop() {
        let b = make_bridge();
        assert!(b.close().is_ok());
        assert!(b.close().is_ok());
    }

    // ── shmem_avtp_pair_new ───────────────────────────────────────────────

    fn soon() -> Option<Instant> {
        Some(Instant::now() + Duration::from_millis(20))
    }

    #[test]
    //fusa:test REQ-SHMEM-001
    //fusa:test REQ-SHMEM-002
    //fusa:test REQ-SHMEM-003
    fn pair_cross_wires_in_fifo_order_and_never_echoes() {
        let (a, b) = shmem_avtp_pair_new(true, 4);
        assert!(a.time_sync_supported() && b.time_sync_supported());
        assert!(!shmem_avtp_pair_new(false, 1).0.time_sync_supported());
        a.send_frame(&[1]).unwrap();
        a.send_frame(&[2, 2]).unwrap();
        b.send_frame(&[9]).unwrap();
        let mut buf = [0u8; 8];
        assert_eq!(b.recv_frame(None, &mut buf).unwrap(), 1);
        assert_eq!(buf[0], 1);
        assert_eq!(b.recv_frame(None, &mut buf).unwrap(), 2);
        assert_eq!(&buf[..2], &[2, 2]);
        // B never sees its own send; A sees exactly B's.
        assert_eq!(b.recv_frame(soon(), &mut buf), Err(RcpError::Timeout));
        assert_eq!(a.recv_frame(None, &mut buf).unwrap(), 1);
        assert_eq!(buf[0], 9);
    }

    #[test]
    //fusa:test REQ-SHMEM-004
    fn pair_recv_times_out_when_nothing_is_queued() {
        let (a, _b) = shmem_avtp_pair_new(false, 1);
        let mut buf = [0u8; 4];
        let start = Instant::now();
        assert_eq!(a.recv_frame(soon(), &mut buf), Err(RcpError::Timeout));
        assert!(start.elapsed() >= Duration::from_millis(20));
    }

    #[test]
    //fusa:test REQ-SHMEM-005
    //fusa:test REQ-SHMEM-010
    fn pair_close_rejects_own_send_and_drains_before_closed() {
        let (a, b) = shmem_avtp_pair_new(false, 2);
        a.send_frame(&[5]).unwrap();
        a.close().unwrap();
        assert_eq!(a.send_frame(&[6]), Err(RcpError::Closed));
        // The peer's own send still succeeds (c-RCP checks own_closed only).
        b.send_frame(&[7]).unwrap();
        let mut buf = [0u8; 4];
        // B drains what A queued before closing, then sees Closed.
        assert_eq!(b.recv_frame(None, &mut buf).unwrap(), 1);
        assert_eq!(b.recv_frame(None, &mut buf), Err(RcpError::Closed));
        // A (closed) still drains B's queued frame, then sees Closed.
        assert_eq!(a.recv_frame(None, &mut buf).unwrap(), 1);
        assert_eq!(a.recv_frame(None, &mut buf), Err(RcpError::Closed));
    }

    #[test]
    //fusa:test REQ-SHMEM-010
    fn pair_peer_close_unblocks_a_waiting_receiver() {
        let (a, b) = shmem_avtp_pair_new(false, 1);
        let h = std::thread::spawn(move || {
            let mut buf = [0u8; 4];
            a.recv_frame(None, &mut buf)
        });
        std::thread::sleep(Duration::from_millis(20));
        b.close().unwrap();
        assert_eq!(h.join().unwrap(), Err(RcpError::Closed));
    }

    #[test]
    //fusa:test REQ-SHMEM-006
    fn pair_send_rejects_once_the_outgoing_queue_is_full() {
        let (a, b) = shmem_avtp_pair_new(false, 0); // 0 → 1
        a.send_frame(&[1]).unwrap();
        assert_eq!(a.send_frame(&[2]), Err(RcpError::Busy));
        // The reverse direction has its own capacity.
        b.send_frame(&[3]).unwrap();
    }

    #[test]
    //fusa:test REQ-SHMEM-007
    fn pair_too_small_buffer_leaves_the_item_queued() {
        let (a, b) = shmem_avtp_pair_new(false, 1);
        a.send_frame(&[1, 2, 3]).unwrap();
        let mut small = [0u8; 1];
        assert_eq!(b.recv_frame(None, &mut small), Err(RcpError::Busy));
        let mut buf = [0u8; 3];
        assert_eq!(b.recv_frame(None, &mut buf).unwrap(), 3);
    }

    #[test]
    //fusa:test REQ-SHMEM-008
    //fusa:test REQ-SHMEM-009
    fn pair_dropping_one_side_leaves_the_other_usable() {
        let (a, b) = shmem_avtp_pair_new(false, 2);
        let weak = Arc::downgrade(&a.shared);
        b.send_frame(&[4]).unwrap();
        drop(b);
        let mut buf = [0u8; 4];
        assert_eq!(a.recv_frame(None, &mut buf).unwrap(), 1);
        a.send_frame(&[5]).unwrap();
        assert!(weak.upgrade().is_some());
        drop(a);
        // Shared state freed once both sides are gone.
        assert!(weak.upgrade().is_none());
    }

    #[test]
    //fusa:test REQ-SHM-004
    fn shm_bridge_round_trips_gbb_and_maps_oversized_to_busy() {
        let tx = InProcChannel::new() as Arc<dyn ShmChannel>;
        let rx = InProcChannel::new() as Arc<dyn ShmChannel>;
        let resp = AcfGbbMessage {
            info: ByteMessageInfo {
                byte_bus_id: 4,
                rsp: true,
                ..Default::default()
            },
            message_timestamp: 1,
            payload: vec![0x11],
        };
        let frame =
            avtp::encode_ntscf_frame(local_stream(), 1, &acf::encode_acf_gbb(&resp).unwrap())
                .unwrap();
        rx.write(&frame).unwrap();
        let b = ShmBridge::new(local_stream(), tx, rx.clone());
        let msg = AcfGbbMessage {
            info: ByteMessageInfo {
                byte_bus_id: 4,
                ..Default::default()
            },
            message_timestamp: 0,
            payload: vec![],
        };
        assert_eq!(b.send_acf_gbb(&msg, 0, None).unwrap().payload, vec![0x11]);
        rx.write(&vec![0u8; avtp::AVTP_MAX_FRAME_LEN + 1]).unwrap();
        assert_eq!(b.send_acf_gbb(&msg, 0, None), Err(RcpError::Busy));
        let link = b.link();
        assert!(link.close().is_ok());
        assert!(!link.time_sync_supported());
    }
}
