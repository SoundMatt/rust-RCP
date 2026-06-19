// fusa:req REQ-SIM-001
// fusa:req REQ-SIM-002
// fusa:req REQ-SIM-003
// fusa:req REQ-SIM-004
// fusa:req REQ-SIM-005
// fusa:req REQ-SIM-006
// fusa:req REQ-SIM-007
// fusa:req REQ-SIM-008

//! Deterministic simulation controller for integration and hardware-in-loop tests.
//!
//! Records all commands dispatched and allows pre-programming response sequences.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::{Command, Controller, RcpError, Response, ResponseStatus, Status, Subscription, Zone};

// ── SimController ─────────────────────────────────────────────────────────────

struct Inner {
    commands: Vec<Command>,
    responses: VecDeque<Result<Response, RcpError>>,
    closed: bool,
    subs: Vec<std::sync::mpsc::SyncSender<Arc<Status>>>,
    seq: u32,
}

/// A deterministic simulation controller.
///
/// Pre-program response sequences with [`queue_response`]; all other sends
/// return `ResponseStatus::OK` with no payload.
// fusa:req REQ-SIM-001
pub struct SimController {
    zone: Zone,
    inner: Mutex<Inner>,
}

impl SimController {
    pub fn new(zone: Zone) -> Arc<Self> {
        Arc::new(SimController {
            zone,
            inner: Mutex::new(Inner {
                commands: Vec::new(),
                responses: VecDeque::new(),
                closed: false,
                subs: Vec::new(),
                seq: 0,
            }),
        })
    }

    /// Pre-program the next response returned by `send`.
    // fusa:req REQ-SIM-002
    pub fn queue_response(&self, r: Result<Response, RcpError>) {
        self.inner.lock().unwrap().responses.push_back(r);
    }

    /// Return all commands dispatched since creation (or last `clear_commands`).
    // fusa:req REQ-SIM-003
    pub fn commands(&self) -> Vec<Command> {
        self.inner.lock().unwrap().commands.clone()
    }

    /// Clear the recorded command log.
    pub fn clear_commands(&self) {
        self.inner.lock().unwrap().commands.clear();
    }

    /// Publish a status update to all subscribers.
    // fusa:req REQ-SIM-007
    pub fn publish(&self, payload: Option<Vec<u8>>) {
        let mut g = self.inner.lock().unwrap();
        g.seq += 1;
        let status = Arc::new(Status {
            zone: self.zone,
            seq: g.seq,
            healthy: true,
            payload,
        });
        g.subs.retain(|tx| tx.try_send(Arc::clone(&status)).is_ok());
    }
}

impl Controller for SimController {
    fn zone(&self) -> Zone {
        self.zone
    }

    // fusa:req REQ-SIM-004
    // fusa:req REQ-SIM-005
    fn send(&self, cmd: &Command, timeout: Option<Duration>) -> Result<Response, RcpError> {
        let mut g = self.inner.lock().unwrap();
        if g.closed {
            return Err(RcpError::Closed);
        }
        if timeout == Some(Duration::ZERO) {
            return Err(RcpError::Timeout);
        }
        if cmd.zone != self.zone {
            return Err(RcpError::ZoneMismatch);
        }
        g.commands.push(cmd.clone());
        if let Some(queued) = g.responses.pop_front() {
            return queued;
        }
        Ok(Response {
            command_id: cmd.id,
            zone: cmd.zone,
            status: ResponseStatus::OK,
            payload: None,
        })
    }

    // fusa:req REQ-SIM-006
    fn subscribe(&self) -> Result<Subscription, RcpError> {
        let mut g = self.inner.lock().unwrap();
        if g.closed {
            return Err(RcpError::Closed);
        }
        let (tx, rx) = std::sync::mpsc::sync_channel(16);
        g.subs.push(tx);
        Ok(Subscription { rx })
    }

    // fusa:req REQ-SIM-008
    fn close(&self) -> Result<(), RcpError> {
        let mut g = self.inner.lock().unwrap();
        g.closed = true;
        g.subs.clear();
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Priority, Zone};

    #[test]
    // fusa:test REQ-SIM-001
    fn new_sim_controller_accepts_commands() {
        let sim = SimController::new(Zone::FRONT_LEFT);
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            ..Default::default()
        };
        let resp = sim.send(&cmd, None).unwrap();
        assert_eq!(resp.status, ResponseStatus::OK);
    }

    #[test]
    // fusa:test REQ-SIM-003
    fn records_dispatched_commands() {
        let sim = SimController::new(Zone::FRONT_LEFT);
        for i in 1u32..=3 {
            sim.send(
                &Command {
                    id: i,
                    zone: Zone::FRONT_LEFT,
                    ..Default::default()
                },
                None,
            )
            .unwrap();
        }
        let cmds = sim.commands();
        assert_eq!(cmds.len(), 3);
        assert_eq!(cmds[0].id, 1);
        assert_eq!(cmds[2].id, 3);
    }

    #[test]
    // fusa:test REQ-SIM-002
    fn queued_responses_delivered_in_order() {
        let sim = SimController::new(Zone::FRONT_LEFT);
        sim.queue_response(Ok(Response {
            status: ResponseStatus::ERROR,
            ..Default::default()
        }));
        sim.queue_response(Err(RcpError::Timeout));

        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            ..Default::default()
        };
        let r1 = sim.send(&cmd, None).unwrap();
        assert_eq!(r1.status, ResponseStatus::ERROR);
        let r2 = sim.send(&cmd, None).unwrap_err();
        assert_eq!(r2, RcpError::Timeout);
    }

    #[test]
    // fusa:test REQ-SIM-004
    fn zero_timeout_returns_timeout_error() {
        let sim = SimController::new(Zone::FRONT_LEFT);
        let err = sim
            .send(
                &Command {
                    zone: Zone::FRONT_LEFT,
                    ..Default::default()
                },
                Some(Duration::ZERO),
            )
            .unwrap_err();
        assert_eq!(err, RcpError::Timeout);
    }

    #[test]
    // fusa:test REQ-SIM-005
    fn zone_mismatch_returns_error() {
        let sim = SimController::new(Zone::FRONT_LEFT);
        let err = sim
            .send(
                &Command {
                    zone: Zone::REAR_RIGHT,
                    ..Default::default()
                },
                None,
            )
            .unwrap_err();
        assert_eq!(err, RcpError::ZoneMismatch);
    }

    #[test]
    // fusa:test REQ-SIM-006
    fn subscribe_receives_publish() {
        let sim = SimController::new(Zone::FRONT_LEFT);
        let sub = sim.subscribe().unwrap();
        sim.publish(Some(b"hello".to_vec()));
        let status = sub.recv_timeout(Duration::from_millis(200)).unwrap();
        assert_eq!(status.payload.as_deref(), Some(b"hello".as_ref()));
    }

    #[test]
    // fusa:test REQ-SIM-007
    fn publish_increments_seq() {
        let sim = SimController::new(Zone::CENTRAL);
        let sub = sim.subscribe().unwrap();
        sim.publish(None);
        sim.publish(None);
        let s1 = sub.recv_timeout(Duration::from_millis(100)).unwrap();
        let s2 = sub.recv_timeout(Duration::from_millis(100)).unwrap();
        assert!(s2.seq > s1.seq);
    }

    #[test]
    // fusa:test REQ-SIM-008
    fn send_after_close_returns_closed() {
        let sim = SimController::new(Zone::FRONT_LEFT);
        sim.close().unwrap();
        let err = sim
            .send(
                &Command {
                    zone: Zone::FRONT_LEFT,
                    ..Default::default()
                },
                None,
            )
            .unwrap_err();
        assert_eq!(err, RcpError::Closed);
    }
}
