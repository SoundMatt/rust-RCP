//! The shared AVTPDU transport trait ([`AvtpTransport`]) and its in-process
//! reference implementation ([`LoopbackTransport`]) — issue #205.
//!
//! Port of c-RCP's `rcp_avtp_transport_t` vtable
//! (`c-RCP/include/rcp/avtp.h:388-441`) and its loopback transport
//! (`c-RCP/include/rcp/avtp.h:443-454`, `c-RCP/src/avtp.c:314-461`). Every
//! carrier this crate supports implements the one trait:
//!
//! | Transport | rust | c-RCP |
//! |---|---|---|
//! | Loopback | [`LoopbackTransport`] | `avtp.h:454` |
//! | UDP (dial / bind) | [`crate::udp::UdpAvtpTransport`] | `udp.h:212` / `udp.h:222` |
//! | L2 (raw Ethernet) | [`crate::l2::L2AvtpTransport`] | `l2.h:154` |
//! | Shared memory | [`crate::shmem::shmem_avtp_pair_new`] | `shmem.h:91` |
//! | TSN / 802.1p | [`crate::tsn::TsnTransport`] | `tsn.h:108` |
//!
//! A transport only moves already-framed AVTPDUs (as produced by
//! [`super::encode_ntscf_frame`]/[`super::encode_tscf_frame`]); the
//! carrier-specific framing (Annex J sequence number, Ethernet II header)
//! is applied and stripped inside each implementation.
//!
//! # Mapping from the C vtable
//!
//! - `send(self, frame, frame_len)` → [`AvtpTransport::send_frame`].
//! - `recv(self, ctx, buf, buf_cap, out_len)` →
//!   [`AvtpTransport::recv_frame`]; the `rcp_context_t` deadline is an
//!   `Option<Instant>` (`None` = `rcp_context_background()`, never done),
//!   the same representation [`crate::relay::Context`] uses.
//! - `close(self)` → [`AvtpTransport::close`].
//! - `destroy` / `retain` / `release` (`avtp.c:296-312`) → `Arc` + `Drop`.
//!   The refcount is the `Arc`'s; there is no separate destroy hook.
//! - `base.time_sync_supported` → [`AvtpTransport::time_sync_supported`].
//! - C's `RCP_ERR_CLOSED` / `RCP_ERR_TIMEOUT` / `RCP_ERR_BUSY` map to
//!   [`RcpError::Closed`] / [`RcpError::Timeout`] / [`RcpError::Busy`].
//!
//! The core is synchronous, as in c-RCP (issue #7's binding decision);
//! only the RELAY binding surface ([`crate::relay`]) is async.

use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use super::{StreamId, TSCF_HEADER_LEN};
use crate::acf::{self, AcfAbbMessage, AcfGbbMessage};
use crate::RcpError;

/// Largest AVTPDU any transport in this crate moves: a TSCF header (the
/// larger of the two header variants) plus its maximum 16-bit
/// `stream_data_length` payload. Mirrors c-RCP's
/// `RCP_AVTP_TSCF_HEADER_LEN + RCP_AVTP_TSCF_MAX_PAYLOAD`
/// (`c-RCP/src/udp.c:96-97`); a buffer of this size never makes
/// [`AvtpTransport::recv_frame`] return [`RcpError::Busy`].
//fusa:req REQ-AVTP-036
pub const AVTP_MAX_FRAME_LEN: usize = TSCF_HEADER_LEN + 0xFFFF;

/// One carrier of already-framed AVTPDUs — port of c-RCP's
/// `rcp_avtp_transport_vtable_t` (`c-RCP/include/rcp/avtp.h:390-414`).
/// See this module's doc comment for the C-to-rust mapping.
//fusa:req REQ-AVTP-036
pub trait AvtpTransport: Send + Sync {
    /// Send one already-framed AVTPDU as a single unit on the underlying
    /// carrier. `Err(RcpError::Closed)` once [`Self::close`] has been
    /// called.
    fn send_frame(&self, frame: &[u8]) -> Result<(), RcpError>;

    /// Block until one AVTPDU is available or `deadline` passes, copying it
    /// into `buf` and returning its length. `Err(RcpError::Timeout)` once
    /// `deadline` is reached, `Err(RcpError::Closed)` if the transport has
    /// been closed with nothing pending, `Err(RcpError::Busy)` if the
    /// received frame is larger than `buf`. `deadline = None` waits
    /// without limit.
    fn recv_frame(&self, deadline: Option<Instant>, buf: &mut [u8]) -> Result<usize, RcpError>;

    /// Release transport resources and unblock any in-progress
    /// [`Self::recv_frame`]. Safe to call multiple times.
    fn close(&self) -> Result<(), RcpError>;

    /// Whether this RC Node instance has gPTP time synchronization enabled
    /// (c-RCP's `base.time_sync_supported`, `avtp.h:421-426`); fixed at
    /// construction time.
    fn time_sync_supported(&self) -> bool;
}

/// A shared handle is itself a transport — the rust counterpart of
/// c-RCP's `rcp_avtp_transport_retain()` (`c-RCP/src/avtp.c:298-302`), so a
/// decorator such as [`crate::tsn::TsnTransport`] can wrap an
/// `Arc<dyn AvtpTransport>` its caller keeps using.
impl<T: AvtpTransport + ?Sized> AvtpTransport for std::sync::Arc<T> {
    //fusa:req REQ-AVTP-036
    fn send_frame(&self, frame: &[u8]) -> Result<(), RcpError> {
        (**self).send_frame(frame)
    }

    //fusa:req REQ-AVTP-036
    fn recv_frame(&self, deadline: Option<Instant>, buf: &mut [u8]) -> Result<usize, RcpError> {
        (**self).recv_frame(deadline, buf)
    }

    //fusa:req REQ-AVTP-036
    fn close(&self) -> Result<(), RcpError> {
        (**self).close()
    }

    //fusa:req REQ-AVTP-036
    fn time_sync_supported(&self) -> bool {
        (**self).time_sync_supported()
    }
}

/// `rcp_context_done()` for an `Option<Instant>` deadline.
pub(crate) fn deadline_done(deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|d| Instant::now() >= d)
}

/// The time left until `deadline` (zero once passed), or `None` for no
/// deadline.
pub(crate) fn remaining(deadline: Option<Instant>) -> Option<Duration> {
    deadline.map(|d| d.saturating_duration_since(Instant::now()))
}

/// Poll-slice length for one wait inside a socket-backed
/// [`AvtpTransport::recv_frame`] loop: [`SOCKET_POLL_SLICE`], shortened to
/// the time left before `deadline`, and never zero (a zero socket timeout
/// means "block forever" for `SO_RCVTIMEO` and is rejected by
/// `std::net::UdpSocket::set_read_timeout`).
pub(crate) fn poll_slice(deadline: Option<Instant>) -> Duration {
    let slice = match remaining(deadline) {
        Some(r) if r < SOCKET_POLL_SLICE => r,
        _ => SOCKET_POLL_SLICE,
    };
    slice.max(Duration::from_millis(1))
}

/// Socket-backed `recv_frame` polling granularity, so that
/// [`AvtpTransport::close`] from another thread unblocks an in-progress
/// receive within one slice. Mirrors c-RCP's `RCP_UDP_AVTP_POLL_MS` /
/// `RCP_L2_POLL_MS` (20 ms, `c-RCP/src/udp.c:101`).
pub(crate) const SOCKET_POLL_SLICE: Duration = Duration::from_millis(20);

// ── Loopback transport ───────────────────────────────────────────────────────

struct LoopbackState {
    closed: bool,
    items: VecDeque<Vec<u8>>,
}

/// In-process, zero-I/O transport backed by a bounded FIFO of frames —
/// port of `rcp_avtp_loopback_transport_new()`
/// (`c-RCP/include/rcp/avtp.h:443-454`, `c-RCP/src/avtp.c:314-461`).
/// [`AvtpTransport::send_frame`] enqueues a copy and
/// [`AvtpTransport::recv_frame`] dequeues it, so one instance echoes its own
/// traffic; see [`crate::shmem::shmem_avtp_pair_new`] for two cross-wired
/// ends.
//fusa:req REQ-AVTP-017
pub struct LoopbackTransport {
    time_sync_supported: bool,
    capacity: usize,
    state: Mutex<LoopbackState>,
    cv: Condvar,
}

impl LoopbackTransport {
    /// A loopback transport holding at most `queue_capacity` undelivered
    /// frames (`0` is treated as `1`, as in c-RCP `avtp.c:442`).
    //fusa:req REQ-AVTP-017
    //fusa:req REQ-AVTP-020
    pub fn new(time_sync_supported: bool, queue_capacity: usize) -> Self {
        let capacity = queue_capacity.max(1);
        LoopbackTransport {
            time_sync_supported,
            capacity,
            state: Mutex::new(LoopbackState {
                closed: false,
                items: VecDeque::with_capacity(capacity),
            }),
            cv: Condvar::new(),
        }
    }
}

impl AvtpTransport for LoopbackTransport {
    /// `Err(Closed)` after [`AvtpTransport::close`]; `Err(Busy)` once
    /// `queue_capacity` frames are undelivered (`avtp.c:331-355`).
    //fusa:req REQ-AVTP-019
    //fusa:req REQ-AVTP-020
    fn send_frame(&self, frame: &[u8]) -> Result<(), RcpError> {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if st.closed {
            return Err(RcpError::Closed);
        }
        if st.items.len() >= self.capacity {
            return Err(RcpError::Busy);
        }
        st.items.push_back(frame.to_vec());
        self.cv.notify_one();
        Ok(())
    }

    /// FIFO dequeue (`avtp.c:357-398`). Frames queued before
    /// [`AvtpTransport::close`] stay deliverable; a frame larger than `buf`
    /// is left queued and `Err(Busy)` returned.
    //fusa:req REQ-AVTP-017
    //fusa:req REQ-AVTP-018
    //fusa:req REQ-AVTP-028
    fn recv_frame(&self, deadline: Option<Instant>, buf: &mut [u8]) -> Result<usize, RcpError> {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        while st.items.is_empty() && !st.closed && !deadline_done(deadline) {
            st = match remaining(deadline) {
                Some(r) => {
                    self.cv
                        .wait_timeout(st, r)
                        .unwrap_or_else(|e| e.into_inner())
                        .0
                }
                None => self.cv.wait(st).unwrap_or_else(|e| e.into_inner()),
            };
        }
        let Some(item) = st.items.front() else {
            return Err(if st.closed {
                RcpError::Closed
            } else {
                RcpError::Timeout
            });
        };
        if item.len() > buf.len() {
            return Err(RcpError::Busy);
        }
        let n = item.len();
        buf[..n].copy_from_slice(item);
        st.items.pop_front();
        Ok(n)
    }

    /// Mark closed and wake every waiting receiver (`avtp.c:400-409`).
    //fusa:req REQ-AVTP-019
    //fusa:req REQ-AVTP-028
    fn close(&self) -> Result<(), RcpError> {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        st.closed = true;
        self.cv.notify_all();
        Ok(())
    }

    //fusa:req REQ-AVTP-036
    fn time_sync_supported(&self) -> bool {
        self.time_sync_supported
    }
}

// ── Request/response helpers over any AvtpTransport ──────────────────────────

/// Send `msg` as an NTSCF-framed ACF_ABB request under `local_stream` over
/// `transport`, receive one frame, decode it as an ACF_ABB response and
/// verify it echoes the request's `byte_bus_id`
/// ([`crate::acf::verify_echo_back`]).
///
/// This is the one client request path every per-carrier compatibility
/// wrapper ([`crate::udp::UdpTransport::send_acf_abb`],
/// [`crate::l2::L2Transport::send_acf_abb`],
/// [`crate::shmem::ShmBridge::send_acf_abb`]) now delegates to.
/// `Some(Duration::ZERO)` is `Err(RcpError::Timeout)` without sending,
/// matching those wrappers' historical behavior; `None` waits without
/// limit.
//fusa:req REQ-AVTP-037
pub fn request_acf_abb(
    transport: &dyn AvtpTransport,
    local_stream: StreamId,
    msg: &AcfAbbMessage,
    sequence_num: u8,
    timeout: Option<Duration>,
) -> Result<AcfAbbMessage, RcpError> {
    let resp_frame = round_trip(
        transport,
        local_stream,
        &acf::encode_acf_abb(msg)?,
        sequence_num,
        timeout,
    )?;
    let (_, resp_payload) = super::decode_ntscf_frame(&resp_frame)?;
    let resp = acf::decode_acf_abb(resp_payload)?;
    acf::verify_echo_back(&msg.info, &resp.info)?;
    Ok(resp)
}

/// Same as [`request_acf_abb`], for an ACF_GBB request/response pair.
//fusa:req REQ-AVTP-037
pub fn request_acf_gbb(
    transport: &dyn AvtpTransport,
    local_stream: StreamId,
    msg: &AcfGbbMessage,
    sequence_num: u8,
    timeout: Option<Duration>,
) -> Result<AcfGbbMessage, RcpError> {
    let resp_frame = round_trip(
        transport,
        local_stream,
        &acf::encode_acf_gbb(msg)?,
        sequence_num,
        timeout,
    )?;
    let (_, resp_payload) = super::decode_ntscf_frame(&resp_frame)?;
    let resp = acf::decode_acf_gbb(resp_payload)?;
    acf::verify_echo_back(&msg.info, &resp.info)?;
    Ok(resp)
}

fn round_trip(
    transport: &dyn AvtpTransport,
    local_stream: StreamId,
    acf_payload: &[u8],
    sequence_num: u8,
    timeout: Option<Duration>,
) -> Result<Vec<u8>, RcpError> {
    if timeout == Some(Duration::ZERO) {
        return Err(RcpError::Timeout);
    }
    let frame = super::encode_ntscf_frame(local_stream, sequence_num, acf_payload)?;
    transport.send_frame(&frame)?;
    let deadline = timeout.map(|d| Instant::now() + d);
    let mut buf = vec![0u8; AVTP_MAX_FRAME_LEN];
    let n = transport.recv_frame(deadline, &mut buf)?;
    buf.truncate(n);
    Ok(buf)
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::acf::ByteMessageInfo;
    use std::sync::Arc;

    fn ms(n: u64) -> Option<Instant> {
        Some(Instant::now() + Duration::from_millis(n))
    }

    #[test]
    //fusa:test REQ-AVTP-017
    //fusa:test REQ-AVTP-036
    fn loopback_delivers_frames_in_fifo_order() {
        let t = LoopbackTransport::new(true, 4);
        assert!(t.time_sync_supported());
        t.send_frame(&[1, 2, 3]).unwrap();
        t.send_frame(&[4]).unwrap();
        t.send_frame(&[]).unwrap();
        let mut buf = [0u8; 8];
        assert_eq!(t.recv_frame(None, &mut buf).unwrap(), 3);
        assert_eq!(&buf[..3], &[1, 2, 3]);
        assert_eq!(t.recv_frame(None, &mut buf).unwrap(), 1);
        assert_eq!(buf[0], 4);
        assert_eq!(t.recv_frame(None, &mut buf).unwrap(), 0);
        assert!(!LoopbackTransport::new(false, 1).time_sync_supported());
    }

    #[test]
    //fusa:test REQ-AVTP-018
    fn loopback_recv_times_out_at_the_deadline() {
        let t = LoopbackTransport::new(false, 1);
        let mut buf = [0u8; 4];
        let start = Instant::now();
        assert_eq!(t.recv_frame(ms(30), &mut buf), Err(RcpError::Timeout));
        assert!(start.elapsed() >= Duration::from_millis(30));
        // An already-passed deadline is reported without waiting.
        assert_eq!(
            t.recv_frame(Some(Instant::now()), &mut buf),
            Err(RcpError::Timeout)
        );
    }

    #[test]
    //fusa:test REQ-AVTP-019
    //fusa:test REQ-AVTP-028
    fn loopback_close_rejects_send_and_drains_then_rejects_recv() {
        let t = LoopbackTransport::new(false, 2);
        t.send_frame(&[9]).unwrap();
        t.close().unwrap();
        t.close().unwrap(); // idempotent
        assert_eq!(t.send_frame(&[1]), Err(RcpError::Closed));
        let mut buf = [0u8; 4];
        // Frames queued before close() remain deliverable.
        assert_eq!(t.recv_frame(None, &mut buf).unwrap(), 1);
        assert_eq!(t.recv_frame(None, &mut buf), Err(RcpError::Closed));
    }

    #[test]
    //fusa:test REQ-AVTP-028
    fn loopback_close_unblocks_a_waiting_receiver() {
        let t = Arc::new(LoopbackTransport::new(false, 1));
        let t2 = Arc::clone(&t);
        let h = std::thread::spawn(move || {
            let mut buf = [0u8; 4];
            t2.recv_frame(None, &mut buf)
        });
        std::thread::sleep(Duration::from_millis(20));
        t.close().unwrap();
        assert_eq!(h.join().unwrap(), Err(RcpError::Closed));
    }

    #[test]
    //fusa:test REQ-AVTP-017
    fn loopback_send_wakes_a_waiting_receiver() {
        let t = Arc::new(LoopbackTransport::new(false, 1));
        let t2 = Arc::clone(&t);
        let h = std::thread::spawn(move || {
            let mut buf = [0u8; 4];
            t2.recv_frame(ms(5000), &mut buf).map(|n| buf[..n].to_vec())
        });
        std::thread::sleep(Duration::from_millis(20));
        t.send_frame(&[7, 7]).unwrap();
        assert_eq!(h.join().unwrap().unwrap(), vec![7, 7]);
    }

    #[test]
    //fusa:test REQ-AVTP-020
    fn loopback_enforces_queue_capacity_and_zero_means_one() {
        let t = LoopbackTransport::new(false, 2);
        t.send_frame(&[1]).unwrap();
        t.send_frame(&[2]).unwrap();
        assert_eq!(t.send_frame(&[3]), Err(RcpError::Busy));
        let z = LoopbackTransport::new(false, 0);
        z.send_frame(&[1]).unwrap();
        assert_eq!(z.send_frame(&[2]), Err(RcpError::Busy));
    }

    #[test]
    //fusa:test REQ-AVTP-017
    fn loopback_too_small_buffer_leaves_the_frame_queued() {
        let t = LoopbackTransport::new(false, 1);
        t.send_frame(&[1, 2, 3]).unwrap();
        let mut small = [0u8; 2];
        assert_eq!(t.recv_frame(None, &mut small), Err(RcpError::Busy));
        let mut big = [0u8; 3];
        assert_eq!(t.recv_frame(None, &mut big).unwrap(), 3);
        assert_eq!(big, [1, 2, 3]);
    }

    #[test]
    //fusa:test REQ-AVTP-036
    fn poll_slice_is_bounded_and_never_zero() {
        assert_eq!(poll_slice(None), SOCKET_POLL_SLICE);
        assert_eq!(poll_slice(ms(10_000)), SOCKET_POLL_SLICE);
        assert_eq!(poll_slice(Some(Instant::now())), Duration::from_millis(1));
        assert!(poll_slice(ms(5)) <= Duration::from_millis(5));
        assert!(remaining(None).is_none());
        assert!(!deadline_done(None));
        assert!(deadline_done(Some(Instant::now())));
    }

    fn sid() -> StreamId {
        StreamId::new([0x02, 0, 0, 0, 0, 1], 0x0001)
    }

    #[test]
    //fusa:test REQ-AVTP-037
    fn request_helpers_pass_now_plus_timeout_as_the_recv_deadline() {
        let (a, _b) = crate::shmem::shmem_avtp_pair_new(false, 1);
        let abb = AcfAbbMessage {
            info: ByteMessageInfo::default(),
            payload: vec![],
        };
        let start = Instant::now();
        // Nobody answers on the far side: the deadline must end the wait.
        assert_eq!(
            request_acf_abb(&a, sid(), &abb, 0, Some(Duration::from_millis(30))),
            Err(RcpError::Timeout)
        );
        assert!(start.elapsed() >= Duration::from_millis(30));
    }

    #[test]
    //fusa:test REQ-AVTP-037
    fn request_helpers_round_trip_over_loopback_and_reject_mismatch() {
        // A loopback echoes the request itself back; the request's own info
        // satisfies the echo-back rule for the same byte_bus_id.
        let t = LoopbackTransport::new(false, 1);
        let abb = AcfAbbMessage {
            info: ByteMessageInfo {
                byte_bus_id: 5,
                ..Default::default()
            },
            payload: vec![0xAB],
        };
        let resp = request_acf_abb(&t, sid(), &abb, 3, Some(Duration::from_secs(1))).unwrap();
        assert_eq!(resp.payload, vec![0xAB]);

        let gbb = AcfGbbMessage {
            info: ByteMessageInfo {
                byte_bus_id: 6,
                // A timed GBB: only mtv = 1 carries message_timestamp on the
                // wire (REQ-ACF-011; c-RCP acf.c zeroes it for mtv = 0).
                mtv: true,
                ..Default::default()
            },
            message_timestamp: 0x1122,
            payload: vec![0xCD],
        };
        let resp = request_acf_gbb(&t, sid(), &gbb, 4, None).unwrap();
        assert_eq!(resp.message_timestamp, 0x1122);

        // Zero timeout: nothing sent.
        assert_eq!(
            request_acf_abb(&t, sid(), &abb, 0, Some(Duration::ZERO)),
            Err(RcpError::Timeout)
        );
        assert_eq!(
            request_acf_gbb(&t, sid(), &gbb, 0, Some(Duration::ZERO)),
            Err(RcpError::Timeout)
        );
        // Nothing queued → recv deadline.
        let mut buf = [0u8; 1];
        assert_eq!(
            t.recv_frame(Some(Instant::now()), &mut buf),
            Err(RcpError::Timeout)
        );

        // A response for a different byte_bus_id fails the echo-back check.
        let other = AcfAbbMessage {
            info: ByteMessageInfo {
                byte_bus_id: 9,
                rsp: true,
                ..Default::default()
            },
            payload: vec![],
        };
        let wrong =
            super::super::encode_ntscf_frame(sid(), 0, &acf::encode_acf_abb(&other).unwrap())
                .unwrap();
        struct Fixed(Vec<u8>);
        impl AvtpTransport for Fixed {
            fn send_frame(&self, _f: &[u8]) -> Result<(), RcpError> {
                Ok(())
            }
            fn recv_frame(&self, _d: Option<Instant>, b: &mut [u8]) -> Result<usize, RcpError> {
                b[..self.0.len()].copy_from_slice(&self.0);
                Ok(self.0.len())
            }
            fn close(&self) -> Result<(), RcpError> {
                Ok(())
            }
            fn time_sync_supported(&self) -> bool {
                false
            }
        }
        let f = Fixed(wrong);
        assert!(f.close().is_ok());
        assert!(!f.time_sync_supported());
        assert_eq!(
            request_acf_abb(&f, sid(), &abb, 0, None),
            Err(RcpError::EpError)
        );
    }
}
