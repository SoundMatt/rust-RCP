// fusa:req REQ-GRPC-001
// fusa:req REQ-GRPC-002
// fusa:req REQ-GRPC-003
// fusa:req REQ-GRPC-004

//! gRPC bridge — wraps an RCP controller behind a gRPC stub interface.
//!
//! Only the message-mapping layer is implemented here; actual gRPC transport
//! is injected via the [`GrpcStub`] trait so the core remains portable.

use std::sync::Arc;
use std::time::Duration;

use crate::{Command, Controller, RcpError, Response, ResponseStatus, Subscription, Zone};

// ── Message types ─────────────────────────────────────────────────────────────

/// Serialised gRPC request bytes (protobuf encoding).
// fusa:req REQ-GRPC-001
pub struct GrpcRequest(pub Vec<u8>);
/// Serialised gRPC response bytes (protobuf encoding).
pub struct GrpcResponse(pub Vec<u8>);

/// Minimal encoding: [zone(1), cmd_type(2 LE), id(4 LE), payload...]
// fusa:req REQ-GRPC-002
pub fn encode_grpc_request(cmd: &Command) -> GrpcRequest {
    let mut buf = Vec::new();
    buf.push(cmd.zone.0);
    buf.extend_from_slice(&cmd.cmd_type.0.to_le_bytes());
    buf.extend_from_slice(&cmd.id.to_le_bytes());
    if let Some(p) = &cmd.payload { buf.extend_from_slice(p); }
    GrpcRequest(buf)
}

/// Decode a gRPC response: [status(1), payload...]
// fusa:req REQ-GRPC-002
pub fn decode_grpc_response(resp: GrpcResponse, cmd: &Command) -> Response {
    let status = if resp.0.first() == Some(&0) { ResponseStatus::OK } else { ResponseStatus::ERROR };
    let payload = if resp.0.len() > 1 { Some(resp.0[1..].to_vec()) } else { None };
    Response { command_id: cmd.id, zone: cmd.zone, status, payload }
}

// ── GrpcStub trait ────────────────────────────────────────────────────────────

/// Abstract gRPC stub for testability.
// fusa:req REQ-GRPC-003
pub trait GrpcStub: Send + Sync {
    fn unary_call(&self, req: GrpcRequest, timeout: Option<Duration>) -> Result<GrpcResponse, RcpError>;
}

/// gRPC bridge controller.
// fusa:req REQ-GRPC-004
pub struct GrpcBridge {
    zone: Zone,
    stub: Arc<dyn GrpcStub>,
}

impl GrpcBridge {
    pub fn new(zone: Zone, stub: Arc<dyn GrpcStub>) -> Self {
        GrpcBridge { zone, stub }
    }
}

impl Controller for GrpcBridge {
    fn zone(&self) -> Zone { self.zone }

    fn send(&self, cmd: &Command, timeout: Option<Duration>) -> Result<Response, RcpError> {
        if timeout == Some(Duration::ZERO) { return Err(RcpError::Timeout); }
        if cmd.zone != self.zone { return Err(RcpError::ZoneMismatch); }
        let req = encode_grpc_request(cmd);
        let resp = self.stub.unary_call(req, timeout)?;
        Ok(decode_grpc_response(resp, cmd))
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

    struct MockStub;
    impl GrpcStub for MockStub {
        fn unary_call(&self, _: GrpcRequest, _: Option<Duration>) -> Result<GrpcResponse, RcpError> {
            Ok(GrpcResponse(vec![0u8])) // OK
        }
    }

    #[test]
    // fusa:test REQ-GRPC-003
    // fusa:test REQ-GRPC-004
    fn grpc_bridge_send_ok() {
        let b = GrpcBridge::new(Zone::FRONT_LEFT, Arc::new(MockStub));
        let resp = b.send(&Command { zone: Zone::FRONT_LEFT, ..Default::default() }, None).unwrap();
        assert_eq!(resp.status, ResponseStatus::OK);
    }

    #[test]
    // fusa:test REQ-GRPC-001
    // fusa:test REQ-GRPC-002
    fn encode_decode_round_trip() {
        let cmd = Command { id: 7, zone: Zone::FRONT_LEFT, ..Default::default() };
        let req = encode_grpc_request(&cmd);
        assert_eq!(req.0[0], Zone::FRONT_LEFT.0);
        let grpc_resp = GrpcResponse(vec![0u8, 1, 2]);
        let resp = decode_grpc_response(grpc_resp, &cmd);
        assert_eq!(resp.status, ResponseStatus::OK);
        assert_eq!(resp.payload, Some(vec![1, 2]));
    }

    #[test]
    // fusa:test REQ-GRPC-004
    fn zone_mismatch() {
        let b = GrpcBridge::new(Zone::FRONT_LEFT, Arc::new(MockStub));
        let err = b.send(&Command { zone: Zone::REAR_LEFT, ..Default::default() }, None).unwrap_err();
        assert_eq!(err, RcpError::ZoneMismatch);
    }
}
