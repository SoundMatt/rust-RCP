// fusa:req REQ-RED-001
// fusa:req REQ-RED-002
// fusa:req REQ-RED-003
// fusa:req REQ-RED-004
// fusa:req REQ-RED-005
// fusa:req REQ-RED-006
// fusa:req REQ-RED-007
// fusa:req REQ-RED-008

//! Redundant controller pair with automatic failover (1-of-2 hot standby).
//!
//! All commands are sent to the primary. On primary failure, the secondary
//! is promoted and becomes the new primary. The failed controller is closed.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::{Command, Controller, RcpError, Response, Subscription, Zone};

// ── RedundancyController ──────────────────────────────────────────────────────

struct Inner {
    primary: Arc<dyn Controller>,
    secondary: Option<Arc<dyn Controller>>,
    failovers: u32,
}

/// Hot-standby redundant controller.
// fusa:req REQ-RED-001
pub struct RedundancyController {
    zone: Zone,
    state: Mutex<Inner>,
}

impl RedundancyController {
    /// Create with a primary and a secondary controller.
    // fusa:req REQ-RED-002
    pub fn new(primary: Arc<dyn Controller>, secondary: Arc<dyn Controller>) -> Self {
        let zone = primary.zone();
        RedundancyController {
            zone,
            state: Mutex::new(Inner {
                primary,
                secondary: Some(secondary),
                failovers: 0,
            }),
        }
    }

    /// Number of times failover has occurred.
    // fusa:req REQ-RED-006
    pub fn failover_count(&self) -> u32 {
        self.state.lock().unwrap().failovers
    }

    /// True if a secondary is still available.
    // fusa:req REQ-RED-007
    pub fn has_secondary(&self) -> bool {
        self.state.lock().unwrap().secondary.is_some()
    }
}

impl Controller for RedundancyController {
    fn zone(&self) -> Zone {
        self.zone
    }

    // fusa:req REQ-RED-003
    // fusa:req REQ-RED-004
    // fusa:req REQ-RED-005
    fn send(&self, cmd: &Command, timeout: Option<Duration>) -> Result<Response, RcpError> {
        let result = {
            let g = self.state.lock().unwrap();
            g.primary.send(cmd, timeout)
        };

        match result {
            Ok(resp) => Ok(resp),
            Err(primary_err) => {
                let mut g = self.state.lock().unwrap();
                match g.secondary.take() {
                    None => Err(primary_err),
                    Some(sec) => {
                        // Promote secondary
                        let old_primary = std::mem::replace(&mut g.primary, sec);
                        let _ = old_primary.close();
                        g.failovers += 1;
                        drop(g);
                        // Retry on new primary
                        self.state.lock().unwrap().primary.send(cmd, timeout)
                    }
                }
            }
        }
    }

    // fusa:req REQ-RED-008
    fn subscribe(&self) -> Result<Subscription, RcpError> {
        self.state.lock().unwrap().primary.subscribe()
    }

    fn close(&self) -> Result<(), RcpError> {
        let g = self.state.lock().unwrap();
        let _ = g.primary.close();
        if let Some(ref sec) = g.secondary {
            let _ = sec.close();
        }
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockController;
    use crate::{Command, Response, ResponseStatus, Zone};

    fn ok_ctrl(zone: Zone) -> Arc<dyn Controller> {
        let h: crate::mock::Handler = Box::new(move |cmd| Response {
            command_id: cmd.id,
            zone: cmd.zone,
            status: ResponseStatus::OK,
            payload: None,
        });
        MockController::new(zone, Some(h)) as Arc<dyn Controller>
    }

    fn failing_ctrl(zone: Zone) -> Arc<dyn Controller> {
        let h: crate::mock::Handler = Box::new(|_cmd| Response {
            command_id: 0,
            zone: Zone::UNKNOWN,
            status: ResponseStatus::OK,
            payload: None,
        });
        let ctrl = MockController::new(zone, Some(h));
        ctrl.close().unwrap();
        ctrl as Arc<dyn Controller>
    }

    #[test]
    // fusa:test REQ-RED-001
    // fusa:test REQ-RED-003
    fn primary_success_no_failover() {
        let r = RedundancyController::new(ok_ctrl(Zone::FRONT_LEFT), ok_ctrl(Zone::FRONT_LEFT));
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            ..Default::default()
        };
        r.send(&cmd, None).unwrap();
        assert_eq!(r.failover_count(), 0);
    }

    #[test]
    // fusa:test REQ-RED-004
    // fusa:test REQ-RED-005
    fn primary_failure_triggers_failover_to_secondary() {
        let r =
            RedundancyController::new(failing_ctrl(Zone::FRONT_LEFT), ok_ctrl(Zone::FRONT_LEFT));
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            ..Default::default()
        };
        r.send(&cmd, None).unwrap();
        assert_eq!(r.failover_count(), 1);
    }

    #[test]
    // fusa:test REQ-RED-006
    fn failover_count_increments() {
        let r =
            RedundancyController::new(failing_ctrl(Zone::FRONT_LEFT), ok_ctrl(Zone::FRONT_LEFT));
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            ..Default::default()
        };
        r.send(&cmd, None).unwrap(); // triggers failover
        assert_eq!(r.failover_count(), 1);
        // Secondary is now primary; no more secondary
        assert!(!r.has_secondary());
    }

    #[test]
    // fusa:test REQ-RED-007
    fn no_secondary_after_failover() {
        let r =
            RedundancyController::new(failing_ctrl(Zone::FRONT_LEFT), ok_ctrl(Zone::FRONT_LEFT));
        assert!(r.has_secondary());
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            ..Default::default()
        };
        r.send(&cmd, None).unwrap();
        assert!(!r.has_secondary());
    }

    #[test]
    // fusa:test REQ-RED-005
    fn both_failed_returns_error() {
        let r = RedundancyController::new(
            failing_ctrl(Zone::FRONT_LEFT),
            failing_ctrl(Zone::FRONT_LEFT),
        );
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            ..Default::default()
        };
        let err = r.send(&cmd, None).unwrap_err();
        assert_eq!(err, RcpError::Closed);
    }

    #[test]
    // fusa:test REQ-RED-002
    fn zone_matches_primary() {
        let r = RedundancyController::new(ok_ctrl(Zone::REAR_RIGHT), ok_ctrl(Zone::REAR_RIGHT));
        assert_eq!(r.zone(), Zone::REAR_RIGHT);
    }

    #[test]
    // fusa:test REQ-RED-008
    fn subscribe_forwarded_to_primary() {
        let r = RedundancyController::new(ok_ctrl(Zone::FRONT_LEFT), ok_ctrl(Zone::FRONT_LEFT));
        r.subscribe().unwrap();
    }
}
