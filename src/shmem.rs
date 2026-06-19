// fusa:req REQ-SHM-001
// fusa:req REQ-SHM-002
// fusa:req REQ-SHM-003
// fusa:req REQ-SHM-004
// fusa:req REQ-SHM-005

//! Shared-memory transport bridge (intra-host IPC).
//!
//! Uses a pair of in-process ring buffers protected by `Mutex` to simulate
//! shared memory. Production deployments replace these with OS shared-memory
//! regions via the [`ShmChannel`] trait.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::{Command, Controller, RcpError, Response, ResponseStatus, Subscription, Zone};

// ── ShmChannel trait ──────────────────────────────────────────────────────────

/// Abstract shared-memory channel for testability.
// fusa:req REQ-SHM-001
pub trait ShmChannel: Send + Sync {
    fn write(&self, data: &[u8]) -> Result<(), RcpError>;
    fn read(&self, timeout: Option<Duration>) -> Result<Vec<u8>, RcpError>;
}

// ── In-process ring buffer implementation ─────────────────────────────────────

/// Simple in-process FIFO channel (for tests and integration).
// fusa:req REQ-SHM-002
pub struct InProcChannel {
    buf: Mutex<std::collections::VecDeque<Vec<u8>>>,
    cvar: std::sync::Condvar,
}

impl InProcChannel {
    pub fn new() -> Arc<Self> {
        Arc::new(InProcChannel {
            buf: Mutex::new(std::collections::VecDeque::new()),
            cvar: std::sync::Condvar::new(),
        })
    }
}

impl Default for InProcChannel {
    fn default() -> Self {
        InProcChannel {
            buf: Mutex::new(std::collections::VecDeque::new()),
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

/// Shared-memory bridge controller.
///
/// The caller must wire `tx` (write) and `rx` (read) channels to the peer process.
// fusa:req REQ-SHM-003
pub struct ShmBridge {
    zone: Zone,
    tx: Arc<dyn ShmChannel>,
    rx: Arc<dyn ShmChannel>,
}

impl ShmBridge {
    pub fn new(zone: Zone, tx: Arc<dyn ShmChannel>, rx: Arc<dyn ShmChannel>) -> Self {
        ShmBridge { zone, tx, rx }
    }
}

impl Controller for ShmBridge {
    fn zone(&self) -> Zone {
        self.zone
    }

    // fusa:req REQ-SHM-004
    fn send(&self, cmd: &Command, timeout: Option<Duration>) -> Result<Response, RcpError> {
        if timeout == Some(Duration::ZERO) {
            return Err(RcpError::Timeout);
        }
        if cmd.zone != self.zone {
            return Err(RcpError::ZoneMismatch);
        }
        self.tx.write(cmd.payload.as_deref().unwrap_or(&[]))?;
        let data = self.rx.read(timeout)?;
        Ok(Response {
            command_id: cmd.id,
            zone: self.zone,
            status: if data.first() == Some(&0) {
                ResponseStatus::OK
            } else {
                ResponseStatus::ERROR
            },
            payload: None,
        })
    }

    fn subscribe(&self) -> Result<Subscription, RcpError> {
        Err(RcpError::NotFound)
    }

    // fusa:req REQ-SHM-005
    fn close(&self) -> Result<(), RcpError> {
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Zone};

    fn make_bridge() -> ShmBridge {
        let tx = InProcChannel::new() as Arc<dyn ShmChannel>;
        let rx = InProcChannel::new() as Arc<dyn ShmChannel>;
        // Prime rx with an OK response
        rx.write(&[0u8]).unwrap();
        ShmBridge::new(Zone::FRONT_LEFT, tx, rx)
    }

    #[test]
    // fusa:test REQ-SHM-003
    // fusa:test REQ-SHM-004
    fn shm_bridge_send_ok() {
        let b = make_bridge();
        let resp = b
            .send(
                &Command {
                    zone: Zone::FRONT_LEFT,
                    ..Default::default()
                },
                None,
            )
            .unwrap();
        assert_eq!(resp.status, ResponseStatus::OK);
    }

    #[test]
    // fusa:test REQ-SHM-004
    fn zone_mismatch_rejected() {
        let b = make_bridge();
        let err = b
            .send(
                &Command {
                    zone: Zone::REAR_LEFT,
                    ..Default::default()
                },
                None,
            )
            .unwrap_err();
        assert_eq!(err, RcpError::ZoneMismatch);
    }

    #[test]
    // fusa:test REQ-SHM-001
    // fusa:test REQ-SHM-002
    fn in_proc_channel_fifo() {
        let ch = InProcChannel::new();
        ch.write(b"first").unwrap();
        ch.write(b"second").unwrap();
        assert_eq!(ch.read(None).unwrap(), b"first");
        assert_eq!(ch.read(None).unwrap(), b"second");
    }

    #[test]
    // fusa:test REQ-SHM-002
    fn in_proc_channel_timeout() {
        let ch = InProcChannel::new();
        let err = ch.read(Some(Duration::from_millis(10))).unwrap_err();
        assert_eq!(err, RcpError::Timeout);
    }

    #[test]
    // fusa:test REQ-SHM-005
    fn close_is_noop() {
        let b = make_bridge();
        assert!(b.close().is_ok());
        assert!(b.close().is_ok());
    }
}
