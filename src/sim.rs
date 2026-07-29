// fusa:req REQ-SIM-001
// fusa:req REQ-SIM-002
// fusa:req REQ-SIM-003
// fusa:req REQ-SIM-004
// fusa:req REQ-SIM-006
// fusa:req REQ-SIM-007
// fusa:req REQ-SIM-008

//! Deterministic simulation endpoint for integration and hardware-in-loop
//! tests.
//!
//! Records all read/write calls dispatched and allows pre-programming
//! response sequences.
//!
//! `ROADMAP.md` Milestone 9 ("All ADAPT-disposition packages retargeted...")
//! cutover: per this module's own ADAPT disposition ("deterministic
//! test-double concept persists; rebuild against the new endpoint trait"),
//! [`SimEndpoint`] replaces the legacy `SimController`, implementing
//! [`crate::mock::Endpoint`] directly (as [`crate::mock::MockEndpoint`]
//! already does) rather than wrapping one.
//!
//! Two changes from the old type, both flagged per Guiding Principle 5
//! rather than silently made:
//!
//! - The old single `queue_response` FIFO (one `Result<Response, _>` queue
//!   feeding every `send`) becomes two independent FIFOs,
//!   [`Self::queue_read_response`]/[`Self::queue_write_response`], since
//!   `Endpoint::read`/`Endpoint::write` return different `Result` types —
//!   the same split `record`/`observe`'s own retargeting notes already
//!   made for the identical reason.
//! - The old `publish`/`subscribe` `Status` broadcast has no replacement
//!   here: `src/mock.rs`'s own doc comment already recorded that this
//!   crate's new core has no live asynchronous-notification mechanism, so
//!   [`RcServer`](crate::mock::RcServer) does not model one either, and
//!   this module — a test double for exactly that new core — does not
//!   invent one that would only exist here.
//!
//! `REQ-SIM-005` ("Zone mismatch returns ZoneMismatch"), which described
//! only the deleted `Zone`-keyed rejection with no surviving analog (an
//! `Endpoint` is not addressed by `Zone` at all), is retired in
//! `.fusa-reqs.json` rather than force-retargeted, per this bullet's own
//! "retarget in place, or explicitly retire if no equivalent behavior
//! exists" instruction.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::mock::Endpoint;
use crate::regmap::EndpointType;
use crate::RcpError;

// ── SimEndpoint ────────────────────────────────────────────────────────────────

/// A single recorded call.
#[derive(Clone, Debug)]
pub enum SimCall {
    Read { read_size: u8 },
    Write { payload: Vec<u8> },
}

struct Inner {
    calls: Vec<SimCall>,
    read_responses: VecDeque<Result<Vec<u8>, RcpError>>,
    write_responses: VecDeque<Result<(), RcpError>>,
}

/// A deterministic simulation endpoint.
///
/// Pre-program response sequences with [`Self::queue_read_response`]/
/// [`Self::queue_write_response`]; a `read` with no queued response
/// returns the endpoint's held buffer (mirroring
/// [`crate::mock::MockEndpoint::read`]); a `write` with no queued response
/// returns `Ok(())`.
// fusa:req REQ-SIM-001
pub struct SimEndpoint {
    ep_type: EndpointType,
    buf: Mutex<Vec<u8>>,
    inner: Mutex<Inner>,
    closed: AtomicBool,
}

impl SimEndpoint {
    pub fn new(ep_type: EndpointType) -> Arc<Self> {
        Arc::new(SimEndpoint {
            ep_type,
            buf: Mutex::new(Vec::new()),
            inner: Mutex::new(Inner {
                calls: Vec::new(),
                read_responses: VecDeque::new(),
                write_responses: VecDeque::new(),
            }),
            closed: AtomicBool::new(false),
        })
    }

    /// Pre-program the next response returned by `read`.
    // fusa:req REQ-SIM-002
    pub fn queue_read_response(&self, r: Result<Vec<u8>, RcpError>) {
        self.inner.lock().unwrap().read_responses.push_back(r);
    }

    /// Pre-program the next response returned by `write`.
    // fusa:req REQ-SIM-002
    pub fn queue_write_response(&self, r: Result<(), RcpError>) {
        self.inner.lock().unwrap().write_responses.push_back(r);
    }

    /// Return all calls dispatched since creation (or last
    /// [`Self::clear_calls`]).
    // fusa:req REQ-SIM-003
    pub fn calls(&self) -> Vec<SimCall> {
        self.inner.lock().unwrap().calls.clone()
    }

    /// Clear the recorded call log.
    pub fn clear_calls(&self) {
        self.inner.lock().unwrap().calls.clear();
    }

    /// Mark this endpoint closed; subsequent calls return
    /// `Err(RcpError::Closed)`. An inherent method (not part of
    /// [`Endpoint`], which defines no `close`) — see this module's doc
    /// comment.
    // fusa:req REQ-SIM-008
    pub fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }
}

impl Endpoint for SimEndpoint {
    fn ep_type(&self) -> EndpointType {
        self.ep_type
    }

    // fusa:req REQ-SIM-004
    fn read(&self, read_size: u8) -> Result<Vec<u8>, RcpError> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(RcpError::Closed);
        }
        let mut g = self.inner.lock().unwrap();
        g.calls.push(SimCall::Read { read_size });
        if let Some(queued) = g.read_responses.pop_front() {
            return queued;
        }
        drop(g);
        let buf = self.buf.lock().unwrap();
        let n = (read_size as usize).min(buf.len());
        Ok(buf[..n].to_vec())
    }

    // fusa:req REQ-SIM-004
    fn write(&self, payload: &[u8]) -> Result<(), RcpError> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(RcpError::Closed);
        }
        let mut g = self.inner.lock().unwrap();
        g.calls.push(SimCall::Write {
            payload: payload.to_vec(),
        });
        if let Some(queued) = g.write_responses.pop_front() {
            return queued;
        }
        drop(g);
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
    // fusa:test REQ-SIM-001
    fn new_sim_endpoint_accepts_writes() {
        let sim = SimEndpoint::new(EndpointType::Gpio);
        sim.write(b"hi").unwrap();
    }

    #[test]
    // fusa:test REQ-SIM-003
    fn records_dispatched_calls() {
        let sim = SimEndpoint::new(EndpointType::Gpio);
        for i in 1u8..=3 {
            sim.write(&[i]).unwrap();
        }
        let calls = sim.calls();
        assert_eq!(calls.len(), 3);
        match &calls[0] {
            SimCall::Write { payload } => assert_eq!(payload, &vec![1u8]),
            _ => panic!("expected Write call"),
        }
    }

    #[test]
    // fusa:test REQ-SIM-002
    fn queued_read_responses_delivered_in_order() {
        let sim = SimEndpoint::new(EndpointType::Gpio);
        sim.queue_read_response(Ok(vec![0xAB]));
        sim.queue_read_response(Err(RcpError::Timeout));

        let r1 = sim.read(1).unwrap();
        assert_eq!(r1, vec![0xAB]);
        let r2 = sim.read(1).unwrap_err();
        assert_eq!(r2, RcpError::Timeout);
    }

    #[test]
    // fusa:test REQ-SIM-002
    fn queued_write_responses_delivered_in_order() {
        let sim = SimEndpoint::new(EndpointType::Gpio);
        sim.queue_write_response(Err(RcpError::Busy));
        let err = sim.write(b"x").unwrap_err();
        assert_eq!(err, RcpError::Busy);
    }

    #[test]
    // fusa:test REQ-SIM-004
    fn write_then_read_round_trips_through_buffer() {
        let sim = SimEndpoint::new(EndpointType::Gpio);
        sim.write(b"test").unwrap();
        let got = sim.read(4).unwrap();
        assert_eq!(got, b"test");
    }

    #[test]
    // fusa:test REQ-SIM-006
    // fusa:test REQ-SIM-007
    fn clear_calls_empties_log() {
        let sim = SimEndpoint::new(EndpointType::Gpio);
        sim.write(b"x").unwrap();
        assert_eq!(sim.calls().len(), 1);
        sim.clear_calls();
        assert!(sim.calls().is_empty());
    }

    #[test]
    // fusa:test REQ-SIM-008
    fn call_after_close_returns_closed() {
        let sim = SimEndpoint::new(EndpointType::Gpio);
        sim.close();
        let err = sim.write(b"x").unwrap_err();
        assert_eq!(err, RcpError::Closed);
        let err = sim.read(1).unwrap_err();
        assert_eq!(err, RcpError::Closed);
    }
}
