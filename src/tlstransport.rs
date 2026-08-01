//fusa:req REQ-TLS-001
//fusa:req REQ-TLS-002
//fusa:req REQ-TLS-003
//fusa:req REQ-TLS-004
//fusa:req REQ-TLS-005

//! TLS transport bridge — wraps TC18 AVTPDU/ACF frames in a TLS stream.
//!
//! The actual TLS stack is injected via [`TlsStream`] for testability.
//! Enforces TLS 1.2 minimum and mutual authentication.
//!
//! `ROADMAP.md` Milestone 9 (`wire` REPLACE disposition) cutover: per this
//! module's own ADAPT disposition in the Satellite Package Disposition
//! table ("TLS-wrapping mechanics and mutual-auth posture are
//! transport-layer and survive; only the encode/decode calls need
//! updating to the new wire format"), [`MIN_TLS_VERSION`],
//! [`REQUIRE_MUTUAL_AUTH`], the [`TlsStream`] trait, and [`TlsBridge::new`]'s
//! mutual-auth gate are all unchanged from this module's pre-Milestone-9
//! version. Only the encode/decode step changes: [`TlsBridge`] no longer
//! serializes a `Zone`-addressed `Command`/`Response` through the deleted
//! `crate::wire` frame; it is now addressed by
//! [`crate::avtp::StreamId`] and carries NTSCF-wrapped ACF_ABB/ACF_GBB
//! messages ([`crate::avtp::encode_ntscf_frame`]/
//! [`crate::avtp::decode_ntscf_frame`], [`crate::acf`]), mirroring
//! `crate::udp::UdpTransport`'s identical cutover for the same reason —
//! both were `crate::wire`'s only two callers.

use std::sync::Arc;
use std::time::Duration;

use crate::acf::{self, AcfAbbMessage, AcfGbbMessage};
use crate::avtp::{self, StreamId};
use crate::RcpError;

// ── TLS configuration ─────────────────────────────────────────────────────────

/// Minimum acceptable TLS version.
//fusa:req REQ-TLS-001
pub const MIN_TLS_VERSION: &str = "TLSv1.2";

/// Whether mutual (client + server) authentication is required.
//fusa:req REQ-TLS-002
pub const REQUIRE_MUTUAL_AUTH: bool = true;

// ── TlsStream trait ───────────────────────────────────────────────────────────

/// Abstract TLS stream for bridge testability.
//fusa:req REQ-TLS-003
pub trait TlsStream: Send + Sync {
    fn write_all(&self, data: &[u8]) -> Result<(), RcpError>;
    fn read_to_vec(&self, timeout: Option<Duration>) -> Result<Vec<u8>, RcpError>;
    fn peer_verified(&self) -> bool;
}

// ── TlsBridge ─────────────────────────────────────────────────────────────────

/// TLS-secured transport, addressed by `local_stream`
/// ([`crate::avtp::StreamId`]) rather than the legacy `Zone`.
//fusa:req REQ-TLS-004
pub struct TlsBridge {
    local_stream: StreamId,
    stream: Arc<dyn TlsStream>,
}

impl TlsBridge {
    /// Create a TLS bridge. Returns `Err(RcpError::NotConnected)` if mutual
    /// auth is required but the peer is not verified. Unchanged from this
    /// module's pre-Milestone-9 version, per its ADAPT scope.
    //fusa:req REQ-TLS-002
    pub fn new(local_stream: StreamId, stream: Arc<dyn TlsStream>) -> Result<Self, RcpError> {
        if REQUIRE_MUTUAL_AUTH && !stream.peer_verified() {
            return Err(RcpError::NotConnected);
        }
        Ok(TlsBridge {
            local_stream,
            stream,
        })
    }

    /// This bridge's local [`StreamId`].
    pub fn local_stream(&self) -> StreamId {
        self.local_stream
    }

    /// Send an ACF_ABB request wrapped in an NTSCF frame, and decode the
    /// ACF_ABB response, verifying it echoes the request's `byte_bus_id`
    /// ([`crate::acf::verify_echo_back`]) — the same framing
    /// `crate::udp::UdpTransport::send_acf_abb` uses, over a [`TlsStream`]
    /// instead of a UDP socket.
    //fusa:req REQ-TLS-004
    //fusa:req REQ-WIRE-006
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
        self.stream.write_all(&frame)?;
        let resp_frame = self.stream.read_to_vec(timeout)?;
        let (_, resp_payload) = avtp::decode_ntscf_frame(&resp_frame)?;
        let resp = acf::decode_acf_abb(resp_payload)?;
        acf::verify_echo_back(&msg.info, &resp.info)?;
        Ok(resp)
    }

    /// Same as [`Self::send_acf_abb`], for an ACF_GBB request/response pair.
    //fusa:req REQ-TLS-004
    //fusa:req REQ-WIRE-006
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
        self.stream.write_all(&frame)?;
        let resp_frame = self.stream.read_to_vec(timeout)?;
        let (_, resp_payload) = avtp::decode_ntscf_frame(&resp_frame)?;
        let resp = acf::decode_acf_gbb(resp_payload)?;
        acf::verify_echo_back(&msg.info, &resp.info)?;
        Ok(resp)
    }

    /// No-op, matching this module's pre-Milestone-9 behavior.
    //fusa:req REQ-TLS-005
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
        StreamId::new([0x02, 0x11, 0x22, 0x33, 0x44, 0x55], 0x0001)
    }

    struct MockTls {
        verified: bool,
        mismatch: bool,
    }

    impl TlsStream for MockTls {
        fn write_all(&self, _: &[u8]) -> Result<(), RcpError> {
            Ok(())
        }
        fn read_to_vec(&self, _: Option<Duration>) -> Result<Vec<u8>, RcpError> {
            let byte_bus_id = if self.mismatch { 99 } else { 7 };
            let resp = AcfAbbMessage {
                info: ByteMessageInfo {
                    byte_bus_id,
                    rsp: true,
                    ..Default::default()
                },
                payload: vec![0xAA],
            };
            let payload = acf::encode_acf_abb(&resp).unwrap();
            Ok(avtp::encode_ntscf_frame(local_stream(), 1, &payload).unwrap())
        }
        fn peer_verified(&self) -> bool {
            self.verified
        }
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

    #[test]
    //fusa:test REQ-TLS-002
    fn unverified_peer_rejected() {
        let stream = Arc::new(MockTls {
            verified: false,
            mismatch: false,
        }) as Arc<dyn TlsStream>;
        let err = TlsBridge::new(local_stream(), stream).err().unwrap();
        assert_eq!(err, RcpError::NotConnected);
    }

    #[test]
    //fusa:test REQ-TLS-002
    //fusa:test REQ-TLS-003
    //fusa:test REQ-TLS-004
    //fusa:test REQ-WIRE-006
    fn tls_send_acf_abb_ok_with_verified_peer() {
        let stream = Arc::new(MockTls {
            verified: true,
            mismatch: false,
        }) as Arc<dyn TlsStream>;
        let bridge = TlsBridge::new(local_stream(), stream).unwrap();
        let resp = bridge.send_acf_abb(&request(7), 0, None).unwrap();
        assert_eq!(resp.info.byte_bus_id, 7);
        assert!(resp.info.rsp);
    }

    #[test]
    //fusa:test REQ-TLS-004
    fn tls_send_acf_abb_rejects_echo_back_mismatch() {
        let stream = Arc::new(MockTls {
            verified: true,
            mismatch: true,
        }) as Arc<dyn TlsStream>;
        let bridge = TlsBridge::new(local_stream(), stream).unwrap();
        let err = bridge.send_acf_abb(&request(7), 0, None).unwrap_err();
        assert_eq!(err, RcpError::EpError);
    }

    #[test]
    //fusa:test REQ-TLS-001
    fn min_tls_version_constant() {
        assert_eq!(MIN_TLS_VERSION, "TLSv1.2");
    }

    #[test]
    //fusa:test REQ-TLS-005
    fn close_is_noop() {
        let stream = Arc::new(MockTls {
            verified: true,
            mismatch: false,
        }) as Arc<dyn TlsStream>;
        let bridge = TlsBridge::new(local_stream(), stream).unwrap();
        assert!(bridge.close().is_ok());
        assert!(bridge.close().is_ok());
    }
}
