// fusa:req REQ-CANBR-001
// fusa:req REQ-CANBR-002
// fusa:req REQ-CANBR-003
// fusa:req REQ-CANBR-004
// fusa:req REQ-CANBR-005

//! CAN bus bridge — transports RCP frames over a CAN 2.0B / CAN FD channel.
//!
//! This module provides the frame-mapping layer only; actual CAN I/O is
//! injected via the [`CanSocket`] trait so the core remains hardware-agnostic.

use std::sync::Arc;
use std::time::Duration;

use crate::{Command, Controller, RcpError, Response, Subscription, Zone};

// ── CAN constants ─────────────────────────────────────────────────────────────

/// Maximum CAN FD payload in bytes.
// fusa:req REQ-CANBR-001
pub const CAN_FD_MAX_PAYLOAD: usize = 64;

// ── CanSocket trait ───────────────────────────────────────────────────────────

/// Abstract CAN socket interface for testability.
// fusa:req REQ-CANBR-002
pub trait CanSocket: Send + Sync {
    fn send_frame(&self, can_id: u32, data: &[u8]) -> Result<(), RcpError>;
    fn recv_frame(&self, timeout: Option<Duration>) -> Result<(u32, Vec<u8>), RcpError>;
}

// ── CanBridge ─────────────────────────────────────────────────────────────────

/// RCP-over-CAN bridge.
///
/// Commands are encoded with `can_id = zone_id << 8 | cmd_type`.
// fusa:req REQ-CANBR-003
pub struct CanBridge {
    zone: Zone,
    socket: Arc<dyn CanSocket>,
}

impl CanBridge {
    pub fn new(zone: Zone, socket: Arc<dyn CanSocket>) -> Self {
        CanBridge { zone, socket }
    }

    fn can_id(zone: Zone, cmd_type: u16) -> u32 {
        ((zone.0 as u32) << 8) | (cmd_type as u32 & 0xFF)
    }
}

impl Controller for CanBridge {
    fn zone(&self) -> Zone {
        self.zone
    }

    // fusa:req REQ-CANBR-004
    fn send(&self, cmd: &Command, timeout: Option<Duration>) -> Result<Response, RcpError> {
        if timeout == Some(Duration::ZERO) {
            return Err(RcpError::Timeout);
        }
        if cmd.zone != self.zone {
            return Err(RcpError::ZoneMismatch);
        }

        let payload = cmd.payload.as_deref().unwrap_or(&[]);
        if payload.len() > CAN_FD_MAX_PAYLOAD {
            return Err(RcpError::PayloadTooLarge);
        }

        let can_id = Self::can_id(self.zone, cmd.cmd_type.0);
        self.socket.send_frame(can_id, payload)?;

        let (_resp_id, resp_data) = self.socket.recv_frame(timeout)?;
        Ok(Response {
            command_id: cmd.id,
            zone: self.zone,
            status: if resp_data.first() == Some(&0) {
                crate::ResponseStatus::OK
            } else {
                crate::ResponseStatus::ERROR
            },
            payload: if resp_data.len() > 1 {
                Some(resp_data[1..].to_vec())
            } else {
                None
            },
        })
    }

    fn subscribe(&self) -> Result<Subscription, RcpError> {
        Err(RcpError::NotFound)
    }

    // fusa:req REQ-CANBR-005
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
    use crate::{Command, ResponseStatus, Zone};
    use std::sync::Mutex;

    struct MockCan {
        tx: Mutex<Vec<(u32, Vec<u8>)>>,
    }
    impl MockCan {
        fn new() -> Arc<Self> {
            Arc::new(MockCan {
                tx: Mutex::new(vec![]),
            })
        }
    }
    impl CanSocket for MockCan {
        fn send_frame(&self, id: u32, data: &[u8]) -> Result<(), RcpError> {
            self.tx.lock().unwrap().push((id, data.to_vec()));
            Ok(())
        }
        fn recv_frame(&self, _: Option<Duration>) -> Result<(u32, Vec<u8>), RcpError> {
            Ok((0, vec![0u8])) // status byte 0 = OK
        }
    }

    #[test]
    // fusa:test REQ-CANBR-002
    // fusa:test REQ-CANBR-003
    // fusa:test REQ-CANBR-004
    fn bridge_sends_can_frame() {
        let sock = MockCan::new();
        let bridge = CanBridge::new(Zone::FRONT_LEFT, Arc::clone(&sock) as Arc<dyn CanSocket>);
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            ..Default::default()
        };
        let resp = bridge.send(&cmd, None).unwrap();
        assert_eq!(resp.status, ResponseStatus::OK);
        assert_eq!(sock.tx.lock().unwrap().len(), 1);
    }

    #[test]
    // fusa:test REQ-CANBR-004
    fn zone_mismatch_rejected() {
        let sock = MockCan::new();
        let bridge = CanBridge::new(Zone::FRONT_LEFT, Arc::clone(&sock) as Arc<dyn CanSocket>);
        let cmd = Command {
            zone: Zone::REAR_RIGHT,
            ..Default::default()
        };
        let err = bridge.send(&cmd, None).unwrap_err();
        assert_eq!(err, RcpError::ZoneMismatch);
    }

    #[test]
    // fusa:test REQ-CANBR-004
    fn oversized_payload_rejected() {
        let sock = MockCan::new();
        let bridge = CanBridge::new(Zone::FRONT_LEFT, Arc::clone(&sock) as Arc<dyn CanSocket>);
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            payload: Some(vec![0u8; CAN_FD_MAX_PAYLOAD + 1]),
            ..Default::default()
        };
        let err = bridge.send(&cmd, None).unwrap_err();
        assert_eq!(err, RcpError::PayloadTooLarge);
    }

    #[test]
    // fusa:test REQ-CANBR-001
    fn can_fd_max_payload_constant() {
        assert_eq!(CAN_FD_MAX_PAYLOAD, 64);
    }

    #[test]
    // fusa:test REQ-CANBR-005
    fn close_is_noop() {
        let sock = MockCan::new();
        let bridge = CanBridge::new(Zone::FRONT_LEFT, Arc::clone(&sock) as Arc<dyn CanSocket>);
        assert!(bridge.close().is_ok());
        assert!(bridge.close().is_ok());
    }
}
