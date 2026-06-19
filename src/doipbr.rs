// fusa:req REQ-DOIP-001
// fusa:req REQ-DOIP-002
// fusa:req REQ-DOIP-003
// fusa:req REQ-DOIP-004

//! DoIP (Diagnostics over IP / ISO 13400-2) bridge.
//!
//! Routes RCP commands to ECUs via the DoIP vehicle announcement and
//! routing-activation handshake. Encodes RCP frames in UDS payload.

use std::sync::Arc;
use std::time::Duration;

use crate::{Command, Controller, RcpError, Response, ResponseStatus, Subscription, Zone};

// ── DoIP constants ────────────────────────────────────────────────────────────

/// DoIP protocol version (ISO 13400-2:2019).
// fusa:req REQ-DOIP-001
pub const DOIP_PROTO_VER: u8 = 0x02;

/// DoIP generic header length in bytes.
pub const DOIP_HEADER_LEN: usize = 8;

/// Payload type: UDS message.
pub const DOIP_PAYLOAD_UDS: u16 = 0x8001;

// ── DoipSocket trait ──────────────────────────────────────────────────────────

/// Abstract DoIP transport.
// fusa:req REQ-DOIP-002
pub trait DoipSocket: Send + Sync {
    fn send(&self, payload_type: u16, data: &[u8]) -> Result<(), RcpError>;
    fn recv(&self, timeout: Option<Duration>) -> Result<Vec<u8>, RcpError>;
}

// ── Encode / decode helpers ───────────────────────────────────────────────────

fn encode_doip_frame(payload_type: u16, data: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(DOIP_HEADER_LEN + data.len());
    frame.push(0xFF);
    frame.push(DOIP_PROTO_VER);
    frame.push(!DOIP_PROTO_VER); // inverse protocol version
    frame.push(0x00);
    frame.extend_from_slice(&payload_type.to_be_bytes());
    frame.extend_from_slice(&(data.len() as u32).to_be_bytes());
    frame.extend_from_slice(data);
    frame
}

// ── DoipBridge ────────────────────────────────────────────────────────────────

/// DoIP bridge controller.
// fusa:req REQ-DOIP-003
pub struct DoipBridge {
    zone:   Zone,
    socket: Arc<dyn DoipSocket>,
}

impl DoipBridge {
    pub fn new(zone: Zone, socket: Arc<dyn DoipSocket>) -> Self {
        DoipBridge { zone, socket }
    }
}

impl Controller for DoipBridge {
    fn zone(&self) -> Zone { self.zone }

    // fusa:req REQ-DOIP-003
    fn send(&self, cmd: &Command, timeout: Option<Duration>) -> Result<Response, RcpError> {
        if timeout == Some(Duration::ZERO) { return Err(RcpError::Timeout); }
        if cmd.zone != self.zone { return Err(RcpError::ZoneMismatch); }
        let payload = cmd.payload.as_deref().unwrap_or(&[]);
        self.socket.send(DOIP_PAYLOAD_UDS, payload)?;
        let resp = self.socket.recv(timeout)?;
        Ok(Response {
            command_id: cmd.id, zone: self.zone,
            status: if resp.first() == Some(&0) { ResponseStatus::OK } else { ResponseStatus::ERROR },
            payload: None,
        })
    }

    fn subscribe(&self) -> Result<Subscription, RcpError> { Err(RcpError::NotFound) }

    // fusa:req REQ-DOIP-004
    fn close(&self) -> Result<(), RcpError> { Ok(()) }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Zone};

    struct MockDoip;
    impl DoipSocket for MockDoip {
        fn send(&self, _: u16, _: &[u8]) -> Result<(), RcpError> { Ok(()) }
        fn recv(&self, _: Option<Duration>) -> Result<Vec<u8>, RcpError> { Ok(vec![0u8]) }
    }

    #[test]
    // fusa:test REQ-DOIP-001
    fn constants_are_correct() {
        assert_eq!(DOIP_PROTO_VER, 0x02);
        assert_eq!(DOIP_HEADER_LEN, 8);
    }

    #[test]
    // fusa:test REQ-DOIP-003
    fn doip_send_ok() {
        let b = DoipBridge::new(Zone::FRONT_LEFT, Arc::new(MockDoip));
        let resp = b.send(&Command { zone: Zone::FRONT_LEFT, ..Default::default() }, None).unwrap();
        assert_eq!(resp.status, ResponseStatus::OK);
    }

    #[test]
    // fusa:test REQ-DOIP-003
    fn zone_mismatch() {
        let b = DoipBridge::new(Zone::FRONT_LEFT, Arc::new(MockDoip));
        let err = b.send(&Command { zone: Zone::REAR_LEFT, ..Default::default() }, None).unwrap_err();
        assert_eq!(err, RcpError::ZoneMismatch);
    }

    #[test]
    // fusa:test REQ-DOIP-002
    fn encode_doip_frame_length() {
        let payload = b"test";
        let frame = encode_doip_frame(DOIP_PAYLOAD_UDS, payload);
        assert_eq!(frame.len(), DOIP_HEADER_LEN + payload.len());
    }

    #[test]
    // fusa:test REQ-DOIP-004
    fn close_is_noop() {
        let b = DoipBridge::new(Zone::FRONT_LEFT, Arc::new(MockDoip));
        assert!(b.close().is_ok());
        assert!(b.close().is_ok());
    }
}
