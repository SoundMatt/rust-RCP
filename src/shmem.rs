//fusa:req REQ-SHM-001
//fusa:req REQ-SHM-002
//fusa:req REQ-SHM-003
//fusa:req REQ-SHM-004
//fusa:req REQ-SHM-005

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
//! `src/tlstransport.rs`'s `TlsBridge` was retargeted by the `wire`
//! REPLACE cutover earlier in this milestone: addressed by
//! [`crate::avtp::StreamId`] instead of `Zone`, and carrying NTSCF-wrapped
//! ACF_ABB/ACF_GBB messages instead of the deleted `wire::encode_command`
//! frame.

use std::sync::Arc;
use std::time::Duration;

use crate::acf::{self, AcfAbbMessage, AcfGbbMessage};
use crate::avtp::{self, StreamId};
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

    /// Send an ACF_ABB request wrapped in an NTSCF frame, and decode the
    /// ACF_ABB response, verifying it echoes the request's `byte_bus_id` —
    /// the same framing `crate::tlstransport::TlsBridge::send_acf_abb`
    /// uses, over an [`ShmChannel`] pair instead of a TLS stream.
    //fusa:req REQ-SHM-004
    pub fn send_acf_abb(
        &self,
        msg: &AcfAbbMessage,
        sequence_num: u8,
        timeout: Option<Duration>,
    ) -> Result<AcfAbbMessage, RcpError> {
        if timeout == Some(Duration::ZERO) {
            return Err(RcpError::Timeout);
        }
        let payload = acf::encode_acf_abb(msg)?;
        let frame = avtp::encode_ntscf_frame(self.local_stream, sequence_num, &payload)?;
        self.tx.write(&frame)?;
        let resp_frame = self.rx.read(timeout)?;
        let (_, resp_payload) = avtp::decode_ntscf_frame(&resp_frame)?;
        let resp = acf::decode_acf_abb(resp_payload)?;
        acf::verify_echo_back(&msg.info, &resp.info)?;
        Ok(resp)
    }

    /// Same as [`Self::send_acf_abb`], for an ACF_GBB request/response pair.
    //fusa:req REQ-SHM-004
    pub fn send_acf_gbb(
        &self,
        msg: &AcfGbbMessage,
        sequence_num: u8,
        timeout: Option<Duration>,
    ) -> Result<AcfGbbMessage, RcpError> {
        if timeout == Some(Duration::ZERO) {
            return Err(RcpError::Timeout);
        }
        let payload = acf::encode_acf_gbb(msg)?;
        let frame = avtp::encode_ntscf_frame(self.local_stream, sequence_num, &payload)?;
        self.tx.write(&frame)?;
        let resp_frame = self.rx.read(timeout)?;
        let (_, resp_payload) = avtp::decode_ntscf_frame(&resp_frame)?;
        let resp = acf::decode_acf_gbb(resp_payload)?;
        acf::verify_echo_back(&msg.info, &resp.info)?;
        Ok(resp)
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
    use crate::acf::ByteMessageInfo;

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
}
