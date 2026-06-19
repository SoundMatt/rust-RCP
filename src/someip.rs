// fusa:req REQ-SOMEIP-001
// fusa:req REQ-SOMEIP-002
// fusa:req REQ-SOMEIP-003
// fusa:req REQ-SOMEIP-004
// fusa:req REQ-SOMEIP-005

//! SOME/IP bridge — encodes RCP frames in AUTOSAR SOME/IP format.
//!
//! SOME/IP header: ServiceID(2) | MethodID(2) | Length(4) | ClientID(2) |
//!                 SessionID(2) | ProtVer(1) | IfVer(1) | MessageType(1) | RetCode(1)

use std::sync::Arc;
use std::time::Duration;

use crate::{Command, Controller, RcpError, Response, ResponseStatus, Subscription, Zone};

// ── SOME/IP constants ─────────────────────────────────────────────────────────

/// SOME/IP header length in bytes.
// fusa:req REQ-SOMEIP-001
pub const SOMEIP_HEADER_LEN: usize = 16;

/// SOME/IP protocol version for RCP.
pub const SOMEIP_PROTO_VER: u8 = 0x01;

/// Message type: Request.
pub const SOMEIP_MSG_REQUEST: u8 = 0x00;
/// Message type: Response.
pub const SOMEIP_MSG_RESPONSE: u8 = 0x80;

// ── Codec ─────────────────────────────────────────────────────────────────────

/// Encode an RCP command as a SOME/IP request frame.
// fusa:req REQ-SOMEIP-002
pub fn encode_request(cmd: &Command, service_id: u16, session_id: u16) -> Vec<u8> {
    let payload = cmd.payload.as_deref().unwrap_or(&[]);
    let length = 8u32 + payload.len() as u32;
    let mut buf = Vec::with_capacity(SOMEIP_HEADER_LEN + payload.len());
    buf.extend_from_slice(&service_id.to_be_bytes());
    buf.extend_from_slice(&cmd.cmd_type.0.to_be_bytes());
    buf.extend_from_slice(&length.to_be_bytes());
    buf.extend_from_slice(&(cmd.zone.0 as u16).to_be_bytes()); // client_id = zone
    buf.extend_from_slice(&session_id.to_be_bytes());
    buf.push(SOMEIP_PROTO_VER);
    buf.push(0x01); // interface version
    buf.push(SOMEIP_MSG_REQUEST);
    buf.push(0x00); // return code: OK
    buf.extend_from_slice(payload);
    buf
}

/// Decode a SOME/IP response frame into a [`Response`].
// fusa:req REQ-SOMEIP-003
pub fn decode_response(buf: &[u8], cmd: &Command) -> Result<Response, RcpError> {
    if buf.len() < SOMEIP_HEADER_LEN { return Err(RcpError::ShortFrame); }
    let msg_type = buf[14];
    let ret_code = buf[15];
    if msg_type != SOMEIP_MSG_RESPONSE { return Err(RcpError::Other("not a response".into())); }
    let status = if ret_code == 0 { ResponseStatus::OK } else { ResponseStatus::ERROR };
    let payload = if buf.len() > SOMEIP_HEADER_LEN {
        Some(buf[SOMEIP_HEADER_LEN..].to_vec())
    } else {
        None
    };
    Ok(Response { command_id: cmd.id, zone: cmd.zone, status, payload })
}

// ── SomeIpSocket trait ────────────────────────────────────────────────────────

/// Abstract SOME/IP transport interface.
// fusa:req REQ-SOMEIP-004
pub trait SomeIpSocket: Send + Sync {
    fn send(&self, frame: &[u8]) -> Result<(), RcpError>;
    fn recv(&self, timeout: Option<Duration>) -> Result<Vec<u8>, RcpError>;
}

/// SOME/IP bridge controller.
// fusa:req REQ-SOMEIP-005
pub struct SomeIpBridge {
    zone:       Zone,
    socket:     Arc<dyn SomeIpSocket>,
    service_id: u16,
    session:    std::sync::atomic::AtomicU16,
}

impl SomeIpBridge {
    pub fn new(zone: Zone, socket: Arc<dyn SomeIpSocket>, service_id: u16) -> Self {
        SomeIpBridge { zone, socket, service_id, session: std::sync::atomic::AtomicU16::new(1) }
    }
}

impl Controller for SomeIpBridge {
    fn zone(&self) -> Zone { self.zone }

    fn send(&self, cmd: &Command, timeout: Option<Duration>) -> Result<Response, RcpError> {
        if timeout == Some(Duration::ZERO) { return Err(RcpError::Timeout); }
        if cmd.zone != self.zone { return Err(RcpError::ZoneMismatch); }
        let session_id = self.session.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let frame = encode_request(cmd, self.service_id, session_id);
        self.socket.send(&frame)?;
        let resp_buf = self.socket.recv(timeout)?;
        decode_response(&resp_buf, cmd)
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

    struct MockSocket;
    impl SomeIpSocket for MockSocket {
        fn send(&self, _: &[u8]) -> Result<(), RcpError> { Ok(()) }
        fn recv(&self, _: Option<Duration>) -> Result<Vec<u8>, RcpError> {
            let mut buf = vec![0u8; SOMEIP_HEADER_LEN];
            buf[14] = SOMEIP_MSG_RESPONSE;
            buf[15] = 0; // OK
            Ok(buf)
        }
    }

    #[test]
    // fusa:test REQ-SOMEIP-001
    fn header_len_constant() { assert_eq!(SOMEIP_HEADER_LEN, 16); }

    #[test]
    // fusa:test REQ-SOMEIP-002
    fn encode_request_has_correct_length() {
        let cmd = Command { zone: Zone::FRONT_LEFT, payload: Some(b"abc".to_vec()), ..Default::default() };
        let frame = encode_request(&cmd, 0x0100, 1);
        assert_eq!(frame.len(), SOMEIP_HEADER_LEN + 3);
    }

    #[test]
    // fusa:test REQ-SOMEIP-003
    fn decode_response_ok() {
        let cmd = Command { zone: Zone::FRONT_LEFT, ..Default::default() };
        let mut buf = vec![0u8; SOMEIP_HEADER_LEN];
        buf[14] = SOMEIP_MSG_RESPONSE;
        let resp = decode_response(&buf, &cmd).unwrap();
        assert_eq!(resp.status, ResponseStatus::OK);
    }

    #[test]
    // fusa:test REQ-SOMEIP-003
    fn decode_short_frame_error() {
        let cmd = Command { zone: Zone::FRONT_LEFT, ..Default::default() };
        assert_eq!(decode_response(&[0u8; 4], &cmd), Err(RcpError::ShortFrame));
    }

    #[test]
    // fusa:test REQ-SOMEIP-004
    // fusa:test REQ-SOMEIP-005
    fn bridge_send_ok() {
        let b = SomeIpBridge::new(Zone::FRONT_LEFT, Arc::new(MockSocket), 0x0100);
        let resp = b.send(&Command { zone: Zone::FRONT_LEFT, ..Default::default() }, None).unwrap();
        assert_eq!(resp.status, ResponseStatus::OK);
    }
}
