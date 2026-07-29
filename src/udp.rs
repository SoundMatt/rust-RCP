// fusa:req REQ-UDP-001
// fusa:req REQ-UDP-002
// fusa:req REQ-UDP-003
// fusa:req REQ-UDP-004
// fusa:req REQ-UDP-005
// fusa:req REQ-UDP-006
// fusa:req REQ-UDP-007

//! UDP unicast transport for the TC18 AVTPDU/ACF wire format.
//!
//! `ROADMAP.md` Milestone 9 (`wire` REPLACE-disposition cutover): the old
//! `Zone`/`Controller`-based `UdpBridge`, which serialized `Command`/
//! `Response` through the now-deleted `crate::wire` 16-byte frame, is
//! REPLACEd outright — deleted, not adapted, the same discipline
//! `src/watchdog.rs`/`src/powerstate.rs` used in Milestones 6/7 — with
//! [`UdpTransport`]: a transport addressed by [`crate::avtp::StreamId`]
//! instead of `Zone`, sending/receiving NTSCF-headed AVTPDU frames
//! ([`crate::avtp::encode_ntscf_frame`]/[`crate::avtp::decode_ntscf_frame`])
//! that carry an ACF_ABB or ACF_GBB payload ([`crate::acf`]), with
//! `byte_bus_id`-addressed routing resolved through [`resolve_endpoint`]
//! ([`crate::ep0::route_byte_bus_id`]/[`crate::addressing::EndpointTable`])
//! rather than a `Zone` lookup.
//!
//! This module closes `wire`'s own row of the Satellite Package Disposition
//! table (`ROADMAP.md`), not `udp`'s: `udp` is REPLACE-dispositioned in its
//! own right, and a real RC-Server/register-map-driven request dispatch or
//! discovery integration is still separate, later work. What this module
//! covers is the transport-framing cutover `wire`'s retirement requires —
//! [`UdpTransport::send_acf_abb`]/[`UdpTransport::send_acf_gbb`] round-trip
//! an ACF message over an actual socket and verify the echo-back rule
//! ([`crate::acf::verify_echo_back`]) end-to-end, and [`resolve_endpoint`]
//! demonstrates `crate::ep0`/`crate::addressing` composing correctly for
//! addressing purposes — neither wires into a full request/response
//! dispatch loop, which does not exist anywhere in this crate yet.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use crate::acf::{self, AcfAbbMessage, AcfGbbMessage};
use crate::addressing::{EndpointId, EndpointTable};
use crate::avtp::{self, StreamId};
use crate::ep0::{self, RequestRoute};
use crate::RcpError;

// ── UdpSocket trait ───────────────────────────────────────────────────────────

/// Abstract UDP socket for testability. Unchanged in shape from this
/// module's pre-Milestone-9 version — only the bytes carried over it
/// changed.
// fusa:req REQ-UDP-001
pub trait UdpSocket: Send + Sync {
    fn send_to(&self, buf: &[u8], addr: SocketAddr) -> Result<usize, RcpError>;
    fn recv_from(&self, timeout: Option<Duration>) -> Result<(Vec<u8>, SocketAddr), RcpError>;
}

// ── UdpTransport ─────────────────────────────────────────────────────────────

/// RCP-over-UDP transport, addressed by `local_stream`
/// ([`crate::avtp::StreamId`]) rather than the legacy `Zone`.
///
/// `sequence_num` (the NTSCF header's per-stream sequence counter) is a
/// caller-supplied value on every send rather than state this struct owns —
/// matching this crate's existing discipline of taking such values as
/// explicit parameters (e.g. `crate::discovery`'s `now: Instant`) rather
/// than hiding a counter/clock behind an method that looks pure.
// fusa:req REQ-UDP-002
pub struct UdpTransport {
    local_stream: StreamId,
    socket: Arc<dyn UdpSocket>,
    remote: SocketAddr,
}

impl UdpTransport {
    /// Construct a transport bound to `local_stream`, sending to `remote`
    /// over `socket`.
    pub fn new(local_stream: StreamId, socket: Arc<dyn UdpSocket>, remote: SocketAddr) -> Self {
        UdpTransport {
            local_stream,
            socket,
            remote,
        }
    }

    /// This transport's local [`StreamId`].
    pub fn local_stream(&self) -> StreamId {
        self.local_stream
    }

    /// Send an ACF_ABB request wrapped in an NTSCF frame addressed under
    /// `local_stream`, and decode the ACF_ABB response, verifying it
    /// echoes the request's `byte_bus_id`
    /// ([`crate::acf::verify_echo_back`]).
    ///
    /// Returns `Err(RcpError::Timeout)` immediately for a zero `timeout`,
    /// matching this module's pre-Milestone-9 behavior.
    // fusa:req REQ-UDP-003
    // fusa:req REQ-UDP-004
    // fusa:req REQ-WIRE-006
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
        self.socket.send_to(&frame, self.remote)?;
        let (resp_frame, _) = self.socket.recv_from(timeout)?;
        let (_, resp_payload) = avtp::decode_ntscf_frame(&resp_frame)?;
        let resp = acf::decode_acf_abb(resp_payload)?;
        acf::verify_echo_back(&msg.info, &resp.info)?;
        Ok(resp)
    }

    /// Same as [`Self::send_acf_abb`], for an ACF_GBB request/response pair.
    // fusa:req REQ-UDP-003
    // fusa:req REQ-UDP-004
    // fusa:req REQ-WIRE-006
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
        self.socket.send_to(&frame, self.remote)?;
        let (resp_frame, _) = self.socket.recv_from(timeout)?;
        let (_, resp_payload) = avtp::decode_ntscf_frame(&resp_frame)?;
        let resp = acf::decode_acf_gbb(resp_payload)?;
        acf::verify_echo_back(&msg.info, &resp.info)?;
        Ok(resp)
    }

    /// No-op, matching this module's pre-Milestone-9 behavior.
    // fusa:req REQ-UDP-005
    pub fn close(&self) -> Result<(), RcpError> {
        Ok(())
    }
}

// ── Endpoint routing ──────────────────────────────────────────────────────────

/// Where a `(stream_id, byte_bus_id)`-addressed request actually resolves
/// to, once [`crate::addressing::EndpointTable`] has been consulted for the
/// `DeviceEndpoint` case — unlike [`crate::ep0::RequestRoute`], which stops
/// at the routing decision itself and never performs the lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// fusa:req REQ-UDP-006
pub enum ResolvedEndpoint {
    /// `byte_bus_id` was the reserved EP0 address.
    Ep0,
    /// `byte_bus_id` resolved, via `endpoints`, to this device endpoint.
    Device(EndpointId),
}

/// Resolve `byte_bus_id`, under `stream_id`, to a [`ResolvedEndpoint`],
/// composing [`crate::ep0::route_byte_bus_id`] with `endpoints` — the
/// Milestone 9 "route addressing through `EndpointTable`/
/// `route_byte_bus_id` instead of the old `Zone` field" item, applied to
/// this transport's addressing.
///
/// Returns `Err(RcpError::EpNotFound)` if `byte_bus_id` is not the reserved
/// EP0 address and no endpoint is registered under `(stream_id,
/// byte_bus_id)`. Never panics for any input.
// fusa:req REQ-UDP-006
// fusa:req REQ-UDP-007
pub fn resolve_endpoint(
    endpoints: &EndpointTable,
    stream_id: StreamId,
    byte_bus_id: u16,
) -> Result<ResolvedEndpoint, RcpError> {
    match ep0::route_byte_bus_id(byte_bus_id) {
        RequestRoute::Ep0 => Ok(ResolvedEndpoint::Ep0),
        RequestRoute::DeviceEndpoint => endpoints
            .lookup(stream_id, byte_bus_id)
            .map(ResolvedEndpoint::Device)
            .ok_or(RcpError::EpNotFound),
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

    /// A mock socket that echoes back a well-formed ACF_ABB response,
    /// copying `byte_bus_id` from whatever request it received (satisfying
    /// the echo-back rule) unless `mismatch` is set, in which case it
    /// deliberately returns a different `byte_bus_id`.
    struct EchoUdp {
        mismatch: bool,
    }

    impl UdpSocket for EchoUdp {
        fn send_to(&self, buf: &[u8], _addr: SocketAddr) -> Result<usize, RcpError> {
            Ok(buf.len())
        }

        fn recv_from(&self, _timeout: Option<Duration>) -> Result<(Vec<u8>, SocketAddr), RcpError> {
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
            let frame = avtp::encode_ntscf_frame(local_stream(), 1, &payload).unwrap();
            let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
            Ok((frame, addr))
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

    // ── send_acf_abb ───────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-UDP-001
    // fusa:test REQ-UDP-002
    // fusa:test REQ-UDP-003
    // fusa:test REQ-WIRE-006
    fn send_acf_abb_round_trips_over_socket() {
        let addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();
        let socket = Arc::new(EchoUdp { mismatch: false });
        let transport = UdpTransport::new(local_stream(), socket, addr);
        let resp = transport.send_acf_abb(&request(7), 0, None).unwrap();
        assert_eq!(resp.info.byte_bus_id, 7);
        assert!(resp.info.rsp);
    }

    #[test]
    // fusa:test REQ-UDP-004
    fn send_acf_abb_rejects_echo_back_mismatch() {
        let addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();
        let socket = Arc::new(EchoUdp { mismatch: true });
        let transport = UdpTransport::new(local_stream(), socket, addr);
        let err = transport.send_acf_abb(&request(7), 0, None).unwrap_err();
        assert_eq!(err, RcpError::EpError);
    }

    #[test]
    // fusa:test REQ-UDP-003
    fn send_acf_abb_rejects_zero_timeout() {
        let addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();
        let socket = Arc::new(EchoUdp { mismatch: false });
        let transport = UdpTransport::new(local_stream(), socket, addr);
        let err = transport
            .send_acf_abb(&request(7), 0, Some(Duration::ZERO))
            .unwrap_err();
        assert_eq!(err, RcpError::Timeout);
    }

    #[test]
    // fusa:test REQ-UDP-002
    fn local_stream_getter_matches_constructor() {
        let addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();
        let socket = Arc::new(EchoUdp { mismatch: false });
        let sid = local_stream();
        let transport = UdpTransport::new(sid, socket, addr);
        assert_eq!(transport.local_stream(), sid);
    }

    #[test]
    // fusa:test REQ-UDP-005
    fn close_is_noop() {
        let addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();
        let socket = Arc::new(EchoUdp { mismatch: false });
        let transport = UdpTransport::new(local_stream(), socket, addr);
        assert!(transport.close().is_ok());
        assert!(transport.close().is_ok());
    }

    // ── resolve_endpoint ───────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-UDP-006
    fn resolve_endpoint_routes_ep0() {
        let endpoints = EndpointTable::new();
        let resolved = resolve_endpoint(&endpoints, local_stream(), ep0::EP0_BYTE_BUS_ID).unwrap();
        assert_eq!(resolved, ResolvedEndpoint::Ep0);
    }

    #[test]
    // fusa:test REQ-UDP-006
    // fusa:test REQ-UDP-007
    fn resolve_endpoint_routes_registered_device_endpoint() {
        let mut endpoints = EndpointTable::new();
        let sid = local_stream();
        endpoints.register(sid, 7, EndpointId(42)).unwrap();
        let resolved = resolve_endpoint(&endpoints, sid, 7).unwrap();
        assert_eq!(resolved, ResolvedEndpoint::Device(EndpointId(42)));
    }

    #[test]
    // fusa:test REQ-UDP-007
    fn resolve_endpoint_rejects_unregistered_device_endpoint() {
        let endpoints = EndpointTable::new();
        let err = resolve_endpoint(&endpoints, local_stream(), 7).unwrap_err();
        assert_eq!(err, RcpError::EpNotFound);
    }

    #[test]
    // fusa:test REQ-UDP-007
    fn resolve_endpoint_does_not_leak_across_streams() {
        let mut endpoints = EndpointTable::new();
        let sid_a = local_stream();
        let sid_b = StreamId::new([0x02, 0x11, 0x22, 0x33, 0x44, 0x66], 0x0002);
        endpoints.register(sid_a, 7, EndpointId(1)).unwrap();
        let err = resolve_endpoint(&endpoints, sid_b, 7).unwrap_err();
        assert_eq!(err, RcpError::EpNotFound);
    }
}
