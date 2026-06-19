// fusa:req REQ-LINBR-001
// fusa:req REQ-LINBR-002
// fusa:req REQ-LINBR-003
// fusa:req REQ-LINBR-004

//! LIN bus bridge — transports RCP commands over a LIN 2.x channel.
//!
//! LIN frames are limited to 8 bytes; payloads exceeding this are rejected
//! with `RcpError::PayloadTooLarge`.

use std::sync::Arc;
use std::time::Duration;

use crate::{Command, Controller, RcpError, Response, ResponseStatus, Subscription, Zone};

/// Maximum LIN frame data length.
// fusa:req REQ-LINBR-001
pub const LIN_MAX_DATA: usize = 8;

/// Abstract LIN master interface.
// fusa:req REQ-LINBR-002
pub trait LinMaster: Send + Sync {
    fn send_frame(&self, pid: u8, data: &[u8]) -> Result<(), RcpError>;
    fn recv_frame(&self, pid: u8, timeout: Option<Duration>) -> Result<Vec<u8>, RcpError>;
}

/// RCP-over-LIN bridge.
// fusa:req REQ-LINBR-003
pub struct LinBridge {
    zone:   Zone,
    master: Arc<dyn LinMaster>,
}

impl LinBridge {
    pub fn new(zone: Zone, master: Arc<dyn LinMaster>) -> Self {
        LinBridge { zone, master }
    }
}

impl Controller for LinBridge {
    fn zone(&self) -> Zone { self.zone }

    // fusa:req REQ-LINBR-004
    fn send(&self, cmd: &Command, timeout: Option<Duration>) -> Result<Response, RcpError> {
        if timeout == Some(Duration::ZERO) { return Err(RcpError::Timeout); }
        if cmd.zone != self.zone { return Err(RcpError::ZoneMismatch); }
        let payload = cmd.payload.as_deref().unwrap_or(&[]);
        if payload.len() > LIN_MAX_DATA { return Err(RcpError::PayloadTooLarge); }
        let pid = (self.zone.0 << 2) | (cmd.cmd_type.0 as u8 & 0x03);
        self.master.send_frame(pid, payload)?;
        let resp_data = self.master.recv_frame(pid, timeout)?;
        Ok(Response {
            command_id: cmd.id, zone: self.zone,
            status: if resp_data.first() == Some(&0) { ResponseStatus::OK } else { ResponseStatus::ERROR },
            payload: None,
        })
    }

    fn subscribe(&self) -> Result<Subscription, RcpError> { Err(RcpError::NotFound) }

    fn close(&self) -> Result<(), RcpError> { Ok(()) }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Zone};

    struct MockLin;
    impl LinMaster for MockLin {
        fn send_frame(&self, _: u8, _: &[u8]) -> Result<(), RcpError> { Ok(()) }
        fn recv_frame(&self, _: u8, _: Option<Duration>) -> Result<Vec<u8>, RcpError> {
            Ok(vec![0u8]) // OK
        }
    }

    fn bridge() -> LinBridge {
        LinBridge::new(Zone::FRONT_LEFT, Arc::new(MockLin))
    }

    #[test]
    // fusa:test REQ-LINBR-001
    fn lin_max_data_is_eight() { assert_eq!(LIN_MAX_DATA, 8); }

    #[test]
    // fusa:test REQ-LINBR-002
    // fusa:test REQ-LINBR-003
    // fusa:test REQ-LINBR-004
    fn basic_send_ok() {
        bridge().send(&Command { zone: Zone::FRONT_LEFT, ..Default::default() }, None).unwrap();
    }

    #[test]
    // fusa:test REQ-LINBR-004
    fn payload_too_large() {
        let err = bridge().send(&Command {
            zone: Zone::FRONT_LEFT,
            payload: Some(vec![0u8; LIN_MAX_DATA + 1]),
            ..Default::default()
        }, None).unwrap_err();
        assert_eq!(err, RcpError::PayloadTooLarge);
    }

    #[test]
    // fusa:test REQ-LINBR-004
    fn zone_mismatch() {
        let err = bridge().send(&Command { zone: Zone::REAR_LEFT, ..Default::default() }, None).unwrap_err();
        assert_eq!(err, RcpError::ZoneMismatch);
    }
}
