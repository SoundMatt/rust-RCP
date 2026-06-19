// fusa:req REQ-UDS-001
// fusa:req REQ-UDS-002
// fusa:req REQ-UDS-003
// fusa:req REQ-UDS-004
// fusa:req REQ-UDS-005

//! UDS (Unified Diagnostic Services / ISO 14229) bridge.
//!
//! Maps RCP commands to UDS service IDs. Used for ECU diagnostics and
//! software updates via OBD-II or direct CAN diagnostic session.

use std::sync::Arc;
use std::time::Duration;

use crate::{
    Command, CommandType, Controller, RcpError, Response, ResponseStatus, Subscription, Zone,
};

// ── UDS service IDs ───────────────────────────────────────────────────────────

// fusa:req REQ-UDS-001
pub const UDS_SID_READ_DATA: u8 = 0x22;
pub const UDS_SID_WRITE_DATA: u8 = 0x2E;
pub const UDS_SID_ECU_RESET: u8 = 0x11;
pub const UDS_SID_COMM_CONTROL: u8 = 0x28;

/// Map RCP command type to UDS service ID.
// fusa:req REQ-UDS-002
pub fn cmd_to_uds_sid(cmd_type: CommandType) -> u8 {
    match cmd_type {
        CommandType::GET => UDS_SID_READ_DATA,
        CommandType::SET => UDS_SID_WRITE_DATA,
        CommandType::RESET => UDS_SID_ECU_RESET,
        _ => UDS_SID_COMM_CONTROL,
    }
}

// ── UdsTransport trait ────────────────────────────────────────────────────────

/// Abstract UDS transport (e.g., DoIP, ISO 15765-2).
// fusa:req REQ-UDS-003
pub trait UdsTransport: Send + Sync {
    fn request(&self, sid: u8, data: &[u8], timeout: Option<Duration>)
        -> Result<Vec<u8>, RcpError>;
}

/// UDS bridge controller.
// fusa:req REQ-UDS-004
pub struct UdsBridge {
    zone: Zone,
    transport: Arc<dyn UdsTransport>,
}

impl UdsBridge {
    pub fn new(zone: Zone, transport: Arc<dyn UdsTransport>) -> Self {
        UdsBridge { zone, transport }
    }
}

impl Controller for UdsBridge {
    fn zone(&self) -> Zone {
        self.zone
    }

    // fusa:req REQ-UDS-004
    fn send(&self, cmd: &Command, timeout: Option<Duration>) -> Result<Response, RcpError> {
        if timeout == Some(Duration::ZERO) {
            return Err(RcpError::Timeout);
        }
        if cmd.zone != self.zone {
            return Err(RcpError::ZoneMismatch);
        }
        let sid = cmd_to_uds_sid(cmd.cmd_type);
        let data = cmd.payload.as_deref().unwrap_or(&[]);
        let resp_data = self.transport.request(sid, data, timeout)?;
        let pos_resp = sid | 0x40; // UDS positive response SID
        let status = if resp_data.first() == Some(&pos_resp) {
            ResponseStatus::OK
        } else {
            ResponseStatus::ERROR
        };
        Ok(Response {
            command_id: cmd.id,
            zone: self.zone,
            status,
            payload: None,
        })
    }

    fn subscribe(&self) -> Result<Subscription, RcpError> {
        Err(RcpError::NotFound)
    }

    // fusa:req REQ-UDS-005
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
    use crate::{Command, CommandType, Zone};

    struct MockUds;
    impl UdsTransport for MockUds {
        fn request(&self, sid: u8, _: &[u8], _: Option<Duration>) -> Result<Vec<u8>, RcpError> {
            Ok(vec![sid | 0x40]) // positive response
        }
    }

    #[test]
    // fusa:test REQ-UDS-001
    fn uds_sids_are_correct() {
        assert_eq!(UDS_SID_READ_DATA, 0x22);
        assert_eq!(UDS_SID_WRITE_DATA, 0x2E);
        assert_eq!(UDS_SID_ECU_RESET, 0x11);
    }

    #[test]
    // fusa:test REQ-UDS-002
    fn cmd_to_uds_sid_mapping() {
        assert_eq!(cmd_to_uds_sid(CommandType::GET), UDS_SID_READ_DATA);
        assert_eq!(cmd_to_uds_sid(CommandType::SET), UDS_SID_WRITE_DATA);
        assert_eq!(cmd_to_uds_sid(CommandType::RESET), UDS_SID_ECU_RESET);
    }

    #[test]
    // fusa:test REQ-UDS-003
    // fusa:test REQ-UDS-004
    fn uds_bridge_send_ok() {
        let b = UdsBridge::new(Zone::FRONT_LEFT, Arc::new(MockUds));
        let resp = b
            .send(
                &Command {
                    zone: Zone::FRONT_LEFT,
                    cmd_type: CommandType::GET,
                    ..Default::default()
                },
                None,
            )
            .unwrap();
        assert_eq!(resp.status, ResponseStatus::OK);
    }

    #[test]
    // fusa:test REQ-UDS-004
    fn zone_mismatch() {
        let b = UdsBridge::new(Zone::FRONT_LEFT, Arc::new(MockUds));
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
    // fusa:test REQ-UDS-005
    fn close_is_noop() {
        let b = UdsBridge::new(Zone::FRONT_LEFT, Arc::new(MockUds));
        assert!(b.close().is_ok());
        assert!(b.close().is_ok());
    }
}
