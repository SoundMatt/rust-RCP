// fusa:req REQ-PWR-001
// fusa:req REQ-PWR-002
// fusa:req REQ-PWR-003
// fusa:req REQ-PWR-004
// fusa:req REQ-PWR-005
// fusa:req REQ-PWR-006
// fusa:req REQ-PWR-007
// fusa:req REQ-PWR-008

//! Power state machine for zone controllers.
//!
//! Models the ACTIVE → SLEEP → STANDBY transitions triggered by
//! `CommandType::SLEEP` and `CommandType::WAKE`.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::{
    Command, CommandType, Controller, RcpError, Response, ResponseStatus, Subscription, Zone,
};

// ── PowerState ────────────────────────────────────────────────────────────────

/// Current power state of a zone controller.
// fusa:req REQ-PWR-002
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerState {
    Active,
    Sleep,
    Standby,
}

impl PowerState {
    pub fn as_str(self) -> &'static str {
        match self {
            PowerState::Active => "active",
            PowerState::Sleep => "sleep",
            PowerState::Standby => "standby",
        }
    }
}

// ── PowerStateController ──────────────────────────────────────────────────────

struct Inner {
    state: PowerState,
    closed: bool,
}

/// Wraps an inner controller and enforces power state transitions.
///
/// - `SLEEP` command transitions to `Sleep`; further non-WAKE commands return
///   `ResponseStatus::BUSY`.
/// - `WAKE` command transitions from `Sleep` back to `Active`.
// fusa:req REQ-PWR-003
pub struct PowerStateController {
    inner: Arc<dyn Controller>,
    st: Mutex<Inner>,
}

impl PowerStateController {
    pub fn new(inner: Arc<dyn Controller>) -> Self {
        PowerStateController {
            inner,
            st: Mutex::new(Inner {
                state: PowerState::Active,
                closed: false,
            }),
        }
    }

    /// Query the current power state.
    // fusa:req REQ-PWR-004
    pub fn power_state(&self) -> PowerState {
        self.st.lock().unwrap().state
    }
}

impl Controller for PowerStateController {
    fn zone(&self) -> Zone {
        self.inner.zone()
    }

    // fusa:req REQ-PWR-005
    // fusa:req REQ-PWR-006
    // fusa:req REQ-PWR-007
    fn send(&self, cmd: &Command, timeout: Option<Duration>) -> Result<Response, RcpError> {
        let mut g = self.st.lock().unwrap();
        if g.closed {
            return Err(RcpError::Closed);
        }

        match cmd.cmd_type {
            CommandType::SLEEP => {
                g.state = PowerState::Sleep;
                return Ok(Response {
                    command_id: cmd.id,
                    zone: cmd.zone,
                    status: ResponseStatus::OK,
                    payload: None,
                });
            }
            CommandType::WAKE => {
                g.state = PowerState::Active;
                return Ok(Response {
                    command_id: cmd.id,
                    zone: cmd.zone,
                    status: ResponseStatus::OK,
                    payload: None,
                });
            }
            _ if g.state == PowerState::Sleep => {
                return Ok(Response {
                    command_id: cmd.id,
                    zone: cmd.zone,
                    status: ResponseStatus::BUSY,
                    payload: None,
                });
            }
            _ => {}
        }
        drop(g);
        self.inner.send(cmd, timeout)
    }

    fn subscribe(&self) -> Result<Subscription, RcpError> {
        self.inner.subscribe()
    }

    // fusa:req REQ-PWR-008
    fn close(&self) -> Result<(), RcpError> {
        self.st.lock().unwrap().closed = true;
        self.inner.close()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockController;
    use crate::{Command, CommandType, Zone};

    fn ctrl() -> Arc<dyn Controller> {
        MockController::new(Zone::FRONT_LEFT, None) as Arc<dyn Controller>
    }

    #[test]
    // fusa:test REQ-PWR-003
    // fusa:test REQ-PWR-004
    fn initial_state_is_active() {
        let ps = PowerStateController::new(ctrl());
        assert_eq!(ps.power_state(), PowerState::Active);
    }

    #[test]
    // fusa:test REQ-PWR-005
    fn sleep_command_transitions_to_sleep() {
        let ps = PowerStateController::new(ctrl());
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            cmd_type: CommandType::SLEEP,
            ..Default::default()
        };
        let resp = ps.send(&cmd, None).unwrap();
        assert_eq!(resp.status, ResponseStatus::OK);
        assert_eq!(ps.power_state(), PowerState::Sleep);
    }

    #[test]
    // fusa:test REQ-PWR-006
    fn commands_during_sleep_return_busy() {
        let ps = PowerStateController::new(ctrl());
        let sleep_cmd = Command {
            zone: Zone::FRONT_LEFT,
            cmd_type: CommandType::SLEEP,
            ..Default::default()
        };
        ps.send(&sleep_cmd, None).unwrap();

        let get_cmd = Command {
            zone: Zone::FRONT_LEFT,
            cmd_type: CommandType::GET,
            ..Default::default()
        };
        let resp = ps.send(&get_cmd, None).unwrap();
        assert_eq!(resp.status, ResponseStatus::BUSY);
    }

    #[test]
    // fusa:test REQ-PWR-007
    fn wake_command_transitions_to_active() {
        let ps = PowerStateController::new(ctrl());
        let sleep = Command {
            zone: Zone::FRONT_LEFT,
            cmd_type: CommandType::SLEEP,
            ..Default::default()
        };
        let wake = Command {
            zone: Zone::FRONT_LEFT,
            cmd_type: CommandType::WAKE,
            ..Default::default()
        };
        ps.send(&sleep, None).unwrap();
        assert_eq!(ps.power_state(), PowerState::Sleep);
        ps.send(&wake, None).unwrap();
        assert_eq!(ps.power_state(), PowerState::Active);
    }

    #[test]
    // fusa:test REQ-PWR-008
    fn send_after_close_returns_closed() {
        let ps = PowerStateController::new(ctrl());
        ps.close().unwrap();
        let err = ps
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

    #[test]
    // fusa:test REQ-PWR-002
    fn power_state_variants_are_distinct() {
        assert_ne!(PowerState::Active, PowerState::Sleep);
        assert_ne!(PowerState::Sleep, PowerState::Standby);
        assert_ne!(PowerState::Active, PowerState::Standby);
    }
}
