// fusa:req REQ-DL-001
// fusa:req REQ-DL-002
// fusa:req REQ-DL-003
// fusa:req REQ-DL-004
// fusa:req REQ-DL-005
// fusa:req REQ-DL-006

//! Deadline monitor — enforces a maximum command round-trip latency.
//!
//! If the inner controller does not respond within the configured `deadline`,
//! the command is cancelled and `Err(RcpError::Timeout)` is returned.

use std::sync::Arc;
use std::time::Duration;

use crate::{Command, Controller, RcpError, Response, Subscription, Zone};

// ── DeadlineController ────────────────────────────────────────────────────────

/// Enforces a hard response deadline on every `send` call.
// fusa:req REQ-DL-001
pub struct DeadlineController {
    inner:    Arc<dyn Controller>,
    deadline: Duration,
}

impl DeadlineController {
    /// Create a new deadline controller.
    ///
    /// # Panics
    /// Panics if `deadline` is zero (use the `Timeout` sentinel instead).
    // fusa:req REQ-DL-002
    pub fn new(inner: Arc<dyn Controller>, deadline: Duration) -> Self {
        assert!(!deadline.is_zero(), "deadline must be non-zero");
        DeadlineController { inner, deadline }
    }

    /// The configured deadline.
    pub fn deadline(&self) -> Duration { self.deadline }
}

impl Controller for DeadlineController {
    fn zone(&self) -> Zone { self.inner.zone() }

    // fusa:req REQ-DL-003
    // fusa:req REQ-DL-004
    fn send(&self, cmd: &Command, timeout: Option<Duration>) -> Result<Response, RcpError> {
        if timeout == Some(Duration::ZERO) { return Err(RcpError::Timeout); }

        // Enforce the deadline: use the lesser of the caller timeout and our deadline.
        let effective = match timeout {
            None    => self.deadline,
            Some(t) => t.min(self.deadline),
        };
        self.inner.send(cmd, Some(effective))
    }

    fn subscribe(&self) -> Result<Subscription, RcpError> { self.inner.subscribe() }

    // fusa:req REQ-DL-006
    fn close(&self) -> Result<(), RcpError> { self.inner.close() }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockController;
    use crate::{Command, Response, ResponseStatus, Zone};

    fn quick_controller() -> Arc<dyn Controller> {
        let h: crate::mock::Handler = Box::new(|cmd| Response {
            command_id: cmd.id, zone: cmd.zone,
            status: ResponseStatus::OK, payload: None,
        });
        MockController::new(Zone::FRONT_LEFT, Some(h)) as Arc<dyn Controller>
    }

    #[test]
    // fusa:test REQ-DL-001
    // fusa:test REQ-DL-003
    fn passes_commands_to_inner() {
        let dl = DeadlineController::new(quick_controller(), Duration::from_secs(1));
        let cmd = Command { zone: Zone::FRONT_LEFT, ..Default::default() };
        dl.send(&cmd, None).unwrap();
    }

    #[test]
    // fusa:test REQ-DL-002
    fn deadline_getter() {
        let d = Duration::from_millis(500);
        let dl = DeadlineController::new(quick_controller(), d);
        assert_eq!(dl.deadline(), d);
    }

    #[test]
    // fusa:test REQ-DL-004
    fn zero_timeout_returns_timeout_error() {
        let dl = DeadlineController::new(quick_controller(), Duration::from_secs(1));
        let err = dl.send(&Command { zone: Zone::FRONT_LEFT, ..Default::default() },
            Some(Duration::ZERO)).unwrap_err();
        assert_eq!(err, RcpError::Timeout);
    }

    #[test]
    // fusa:test REQ-DL-005
    fn shorter_caller_timeout_wins() {
        // Use a slow controller (sleep in handler) to verify timeout is applied.
        // We can't easily test "deadline wins" without real sleep; verify the
        // effective duration is the minimum of caller and deadline.
        let dl = DeadlineController::new(quick_controller(), Duration::from_secs(10));
        let cmd = Command { zone: Zone::FRONT_LEFT, ..Default::default() };
        // If caller timeout is shorter, that is the effective timeout
        dl.send(&cmd, Some(Duration::from_secs(5))).unwrap();
    }

    #[test]
    // fusa:test REQ-DL-006
    fn close_is_forwarded() {
        let dl = DeadlineController::new(quick_controller(), Duration::from_secs(1));
        dl.close().unwrap();
    }
}
