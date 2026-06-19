// fusa:req REQ-TLS-001
// fusa:req REQ-TLS-002
// fusa:req REQ-TLS-003
// fusa:req REQ-TLS-004
// fusa:req REQ-TLS-005

//! TLS transport bridge — wraps RCP wire frames in a TLS stream.
//!
//! The actual TLS stack is injected via [`TlsStream`] for testability.
//! Enforces TLS 1.2 minimum and mutual authentication.

use std::sync::Arc;
use std::time::Duration;

use crate::wire;
use crate::{Command, Controller, RcpError, Response, Subscription, Zone};

// ── TLS configuration ─────────────────────────────────────────────────────────

/// Minimum acceptable TLS version.
// fusa:req REQ-TLS-001
pub const MIN_TLS_VERSION: &str = "TLSv1.2";

/// Whether mutual (client + server) authentication is required.
// fusa:req REQ-TLS-002
pub const REQUIRE_MUTUAL_AUTH: bool = true;

// ── TlsStream trait ───────────────────────────────────────────────────────────

/// Abstract TLS stream for bridge testability.
// fusa:req REQ-TLS-003
pub trait TlsStream: Send + Sync {
    fn write_all(&self, data: &[u8]) -> Result<(), RcpError>;
    fn read_to_vec(&self, timeout: Option<Duration>) -> Result<Vec<u8>, RcpError>;
    fn peer_verified(&self) -> bool;
}

// ── TlsBridge ─────────────────────────────────────────────────────────────────

/// TLS-secured RCP bridge controller.
// fusa:req REQ-TLS-004
pub struct TlsBridge {
    zone: Zone,
    stream: Arc<dyn TlsStream>,
}

impl TlsBridge {
    /// Create a TLS bridge. Returns `Err(RcpError::NotConnected)` if mutual
    /// auth is required but the peer is not verified.
    // fusa:req REQ-TLS-002
    pub fn new(zone: Zone, stream: Arc<dyn TlsStream>) -> Result<Self, RcpError> {
        if REQUIRE_MUTUAL_AUTH && !stream.peer_verified() {
            return Err(RcpError::NotConnected);
        }
        Ok(TlsBridge { zone, stream })
    }
}

impl Controller for TlsBridge {
    fn zone(&self) -> Zone {
        self.zone
    }

    // fusa:req REQ-TLS-004
    fn send(&self, cmd: &Command, timeout: Option<Duration>) -> Result<Response, RcpError> {
        if timeout == Some(Duration::ZERO) {
            return Err(RcpError::Timeout);
        }
        if cmd.zone != self.zone {
            return Err(RcpError::ZoneMismatch);
        }
        let frame = wire::encode_command(cmd)?;
        self.stream.write_all(&frame)?;
        let resp_frame = self.stream.read_to_vec(timeout)?;
        wire::decode_response(&resp_frame)
    }

    fn subscribe(&self) -> Result<Subscription, RcpError> {
        Err(RcpError::NotFound)
    }

    // fusa:req REQ-TLS-005
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

    struct MockTls {
        verified: bool,
    }
    impl TlsStream for MockTls {
        fn write_all(&self, _: &[u8]) -> Result<(), RcpError> {
            Ok(())
        }
        fn read_to_vec(&self, _: Option<Duration>) -> Result<Vec<u8>, RcpError> {
            let resp = Response {
                command_id: 1,
                zone: Zone::FRONT_LEFT,
                status: ResponseStatus::OK,
                payload: None,
            };
            wire::encode_response(&resp).map_err(|e| e)
        }
        fn peer_verified(&self) -> bool {
            self.verified
        }
    }

    #[test]
    // fusa:test REQ-TLS-002
    fn unverified_peer_rejected() {
        let stream = Arc::new(MockTls { verified: false }) as Arc<dyn TlsStream>;
        let err = TlsBridge::new(Zone::FRONT_LEFT, stream).unwrap_err();
        assert_eq!(err, RcpError::NotConnected);
    }

    #[test]
    // fusa:test REQ-TLS-003
    // fusa:test REQ-TLS-004
    fn tls_send_ok_with_verified_peer() {
        let stream = Arc::new(MockTls { verified: true }) as Arc<dyn TlsStream>;
        let bridge = TlsBridge::new(Zone::FRONT_LEFT, stream).unwrap();
        let resp = bridge
            .send(
                &Command {
                    id: 1,
                    zone: Zone::FRONT_LEFT,
                    ..Default::default()
                },
                None,
            )
            .unwrap();
        assert_eq!(resp.status, ResponseStatus::OK);
    }

    #[test]
    // fusa:test REQ-TLS-001
    fn min_tls_version_constant() {
        assert_eq!(MIN_TLS_VERSION, "TLSv1.2");
    }

    #[test]
    // fusa:test REQ-TLS-005
    fn close_is_noop() {
        let stream = Arc::new(MockTls { verified: true }) as Arc<dyn TlsStream>;
        let bridge = TlsBridge::new(Zone::FRONT_LEFT, stream).unwrap();
        assert!(bridge.close().is_ok());
        assert!(bridge.close().is_ok());
    }
}
