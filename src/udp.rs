// fusa:req REQ-UDP-001
// fusa:req REQ-UDP-002
// fusa:req REQ-UDP-003
// fusa:req REQ-UDP-004
// fusa:req REQ-UDP-005

//! UDP unicast transport bridge.
//!
//! Serialises RCP wire frames over UDP datagrams. Supports `#[cfg(not(test))]`
//! production paths and injects a mock socket in tests.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use crate::wire;
use crate::{Command, Controller, RcpError, Response, Subscription, Zone};

// ── UdpSocket trait ───────────────────────────────────────────────────────────

/// Abstract UDP socket for testability.
// fusa:req REQ-UDP-001
pub trait UdpSocket: Send + Sync {
    fn send_to(&self, buf: &[u8], addr: SocketAddr) -> Result<usize, RcpError>;
    fn recv_from(&self, timeout: Option<Duration>) -> Result<(Vec<u8>, SocketAddr), RcpError>;
}

// ── UdpBridge ─────────────────────────────────────────────────────────────────

/// RCP-over-UDP bridge controller.
// fusa:req REQ-UDP-002
pub struct UdpBridge {
    zone: Zone,
    socket: Arc<dyn UdpSocket>,
    remote: SocketAddr,
}

impl UdpBridge {
    pub fn new(zone: Zone, socket: Arc<dyn UdpSocket>, remote: SocketAddr) -> Self {
        UdpBridge {
            zone,
            socket,
            remote,
        }
    }
}

impl Controller for UdpBridge {
    fn zone(&self) -> Zone {
        self.zone
    }

    // fusa:req REQ-UDP-003
    // fusa:req REQ-UDP-004
    fn send(&self, cmd: &Command, timeout: Option<Duration>) -> Result<Response, RcpError> {
        if timeout == Some(Duration::ZERO) {
            return Err(RcpError::Timeout);
        }
        if cmd.zone != self.zone {
            return Err(RcpError::ZoneMismatch);
        }

        let frame = wire::encode_command(cmd);
        self.socket.send_to(&frame, self.remote)?;
        let (resp_frame, _) = self.socket.recv_from(timeout)?;
        wire::decode_response(&resp_frame)
    }

    fn subscribe(&self) -> Result<Subscription, RcpError> {
        Err(RcpError::NotFound)
    }

    // fusa:req REQ-UDP-005
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
    use crate::{Command, Response, ResponseStatus, Zone};

    struct MockUdp;
    impl UdpSocket for MockUdp {
        fn send_to(&self, _: &[u8], _: SocketAddr) -> Result<usize, RcpError> {
            Ok(0)
        }
        fn recv_from(&self, _: Option<Duration>) -> Result<(Vec<u8>, SocketAddr), RcpError> {
            let resp = Response {
                command_id: 1,
                zone: Zone::FRONT_LEFT,
                status: ResponseStatus::OK,
                payload: None,
            };
            let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
            Ok((wire::encode_response(&resp), addr))
        }
    }

    #[test]
    // fusa:test REQ-UDP-001
    // fusa:test REQ-UDP-002
    // fusa:test REQ-UDP-003
    fn udp_bridge_send_ok() {
        let addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();
        let b = UdpBridge::new(Zone::FRONT_LEFT, Arc::new(MockUdp), addr);
        let cmd = Command {
            id: 1,
            zone: Zone::FRONT_LEFT,
            ..Default::default()
        };
        let resp = b.send(&cmd, None).unwrap();
        assert_eq!(resp.status, ResponseStatus::OK);
    }

    #[test]
    // fusa:test REQ-UDP-004
    fn zone_mismatch_rejected() {
        let addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();
        let b = UdpBridge::new(Zone::FRONT_LEFT, Arc::new(MockUdp), addr);
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
    // fusa:test REQ-UDP-005
    fn close_is_noop() {
        let addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();
        let b = UdpBridge::new(Zone::FRONT_LEFT, Arc::new(MockUdp), addr);
        assert!(b.close().is_ok());
        assert!(b.close().is_ok());
    }
}
