//! In-process test doubles for this crate's OPEN Alliance TC18 RC Server.
//!
//! The RC Server itself and its dispatch engine live in [`crate::server`]
//! (issue #224): [`crate::server::RcServer`] holds the request store and the
//! per-stream safety state, and [`crate::server::RcServer::dispatch_frame`]
//! is the single frame-level entry point every transport uses. This module
//! keeps only test doubles: [`MockEndpoint`], a byte-buffer-backed
//! [`Endpoint`].
//!
//! [`RcServer`] and [`Endpoint`] are re-exported here so existing
//! `rcp::mock::{RcServer, Endpoint}` paths (and the endpoint decorator
//! modules built on them) keep resolving.
//!
//! All operations in this module execute synchronously in memory with no
//! network I/O, and are safe for concurrent use.

use std::sync::{Arc, Mutex};

use crate::regmap::EndpointType;
use crate::RcpError;

pub use crate::server::{Endpoint, RcServer};

/// A trivial byte-buffer-backed [`Endpoint`] test double.
///
/// [`MockEndpoint::write`] replaces the held buffer wholesale with
/// `payload`; [`MockEndpoint::read`] returns up to `read_size` bytes from
/// the front of whatever is currently held, or the whole buffer if
/// `read_size` (as an unsigned byte count) is not smaller than it — this
/// crate's own simplification, not a transcription of any real per-endpoint
/// read-chunking rule.
pub struct MockEndpoint {
    ep_type: EndpointType,
    buf: Mutex<Vec<u8>>,
}

impl MockEndpoint {
    /// Construct a mock endpoint of type `ep_type`, initially holding
    /// `initial` as its buffer.
    //fusa:req REQ-MOCKSRV-009
    pub fn new(ep_type: EndpointType, initial: Vec<u8>) -> Arc<Self> {
        Arc::new(Self {
            ep_type,
            buf: Mutex::new(initial),
        })
    }
}

impl Endpoint for MockEndpoint {
    fn ep_type(&self) -> EndpointType {
        self.ep_type
    }

    //fusa:req REQ-MOCKSRV-010
    fn read(&self, read_size: u16) -> Result<Vec<u8>, RcpError> {
        let buf = self.buf.lock().unwrap();
        let n = (read_size as usize).min(buf.len());
        Ok(buf[..n].to_vec())
    }

    //fusa:req REQ-MOCKSRV-009
    fn write(&self, payload: &[u8]) -> Result<(), RcpError> {
        *self.buf.lock().unwrap() = payload.to_vec();
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    //fusa:test REQ-MOCKSRV-009
    fn mock_endpoint_read_returns_last_written_bytes() {
        let ep = MockEndpoint::new(EndpointType::Gpio, Vec::new());
        assert_eq!(ep.ep_type(), EndpointType::Gpio);
        ep.write(&[1, 2, 3]).unwrap();
        assert_eq!(ep.read(3).unwrap(), vec![1, 2, 3]);
    }

    #[test]
    //fusa:test REQ-MOCKSRV-010
    fn mock_endpoint_read_size_exceeding_buffer_does_not_panic() {
        let ep = MockEndpoint::new(EndpointType::Gpio, vec![1, 2]);
        let out = ep.read(255).unwrap();
        assert_eq!(out, vec![1, 2]);
    }

    #[test]
    //fusa:test REQ-MOCKSRV-010
    fn mock_endpoint_read_on_empty_buffer_does_not_panic() {
        let ep = MockEndpoint::new(EndpointType::Gpio, Vec::new());
        assert_eq!(ep.read(10).unwrap(), Vec::<u8>::new());
    }
}
