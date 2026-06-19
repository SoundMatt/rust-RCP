// fusa:req REQ-FI-001
// fusa:req REQ-FI-002
// fusa:req REQ-FI-003
// fusa:req REQ-FI-004
// fusa:req REQ-FI-005
// fusa:req REQ-FI-006
// fusa:req REQ-FI-007

//! Fault injection — deterministic error injection for safety test campaigns.
//!
//! Wraps an inner controller; errors are injected via pre-programmed rules
//! (nth-call injection, always-inject, or probability-based for non-safety tests).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::{Command, Controller, RcpError, Response, Subscription, Zone};

// ── FaultRule ─────────────────────────────────────────────────────────────────

/// Rule controlling when a fault is injected.
// fusa:req REQ-FI-002
#[derive(Clone, Debug)]
pub enum FaultRule {
    /// Inject on every call.
    Always,
    /// Inject on the Nth call (1-based).
    OnNthCall(u64),
    /// Inject on every call after (and including) the Nth.
    AfterNthCall(u64),
}

/// A configured fault to inject.
// fusa:req REQ-FI-001
#[derive(Clone, Debug)]
pub struct FaultSpec {
    pub rule:  FaultRule,
    pub error: RcpError,
}

// ── FaultInjectController ─────────────────────────────────────────────────────

struct Inner {
    faults:  Vec<FaultSpec>,
    call_no: u64,
}

/// Fault-injecting controller wrapper.
// fusa:req REQ-FI-003
pub struct FaultInjectController {
    inner: Arc<dyn Controller>,
    state: Mutex<Inner>,
    total: AtomicU64,
}

impl FaultInjectController {
    pub fn new(inner: Arc<dyn Controller>) -> Self {
        FaultInjectController {
            inner,
            state: Mutex::new(Inner { faults: Vec::new(), call_no: 0 }),
            total: AtomicU64::new(0),
        }
    }

    /// Install a fault rule.
    // fusa:req REQ-FI-004
    pub fn inject(&self, spec: FaultSpec) {
        self.state.lock().unwrap().faults.push(spec);
    }

    /// Remove all fault rules.
    // fusa:req REQ-FI-005
    pub fn clear(&self) {
        self.state.lock().unwrap().faults.clear();
    }

    /// Total number of `send` calls made (including faulted ones).
    pub fn call_count(&self) -> u64 {
        self.total.load(Ordering::SeqCst)
    }
}

impl Controller for FaultInjectController {
    fn zone(&self) -> Zone { self.inner.zone() }

    // fusa:req REQ-FI-006
    fn send(&self, cmd: &Command, timeout: Option<Duration>) -> Result<Response, RcpError> {
        let call_no = {
            let mut g = self.state.lock().unwrap();
            g.call_no += 1;
            g.call_no
        };
        self.total.fetch_add(1, Ordering::SeqCst);

        let fault = {
            let g = self.state.lock().unwrap();
            g.faults.iter().find_map(|spec| {
                let triggered = match spec.rule {
                    FaultRule::Always           => true,
                    FaultRule::OnNthCall(n)     => call_no == n,
                    FaultRule::AfterNthCall(n)  => call_no >= n,
                };
                if triggered { Some(spec.error.clone()) } else { None }
            })
        };

        if let Some(err) = fault {
            return Err(err);
        }

        self.inner.send(cmd, timeout)
    }

    fn subscribe(&self) -> Result<Subscription, RcpError> { self.inner.subscribe() }

    // fusa:req REQ-FI-007
    fn close(&self) -> Result<(), RcpError> { self.inner.close() }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockController;
    use crate::{Command, Zone};

    fn fi() -> FaultInjectController {
        let inner = MockController::new(Zone::FRONT_LEFT, None) as Arc<dyn Controller>;
        FaultInjectController::new(inner)
    }

    #[test]
    // fusa:test REQ-FI-001
    // fusa:test REQ-FI-003
    fn no_fault_passes_through() {
        let fi = fi();
        let cmd = Command { zone: Zone::FRONT_LEFT, ..Default::default() };
        fi.send(&cmd, None).unwrap();
    }

    #[test]
    // fusa:test REQ-FI-002
    // fusa:test REQ-FI-006
    fn always_fault_injects_every_call() {
        let fi = fi();
        fi.inject(FaultSpec { rule: FaultRule::Always, error: RcpError::Timeout });
        let cmd = Command { zone: Zone::FRONT_LEFT, ..Default::default() };
        for _ in 0..3 {
            let err = fi.send(&cmd, None).unwrap_err();
            assert_eq!(err, RcpError::Timeout);
        }
    }

    #[test]
    // fusa:test REQ-FI-002
    // fusa:test REQ-FI-006
    fn nth_call_fault_triggers_only_on_n() {
        let fi = fi();
        fi.inject(FaultSpec { rule: FaultRule::OnNthCall(2), error: RcpError::Busy });
        let cmd = Command { zone: Zone::FRONT_LEFT, ..Default::default() };
        fi.send(&cmd, None).unwrap();          // call 1 — ok
        let err = fi.send(&cmd, None).unwrap_err(); // call 2 — fault
        assert_eq!(err, RcpError::Busy);
        fi.send(&cmd, None).unwrap();          // call 3 — ok again
    }

    #[test]
    // fusa:test REQ-FI-002
    // fusa:test REQ-FI-006
    fn after_nth_call_triggers_from_n_onwards() {
        let fi = fi();
        fi.inject(FaultSpec { rule: FaultRule::AfterNthCall(3), error: RcpError::NotConnected });
        let cmd = Command { zone: Zone::FRONT_LEFT, ..Default::default() };
        fi.send(&cmd, None).unwrap(); // 1 — ok
        fi.send(&cmd, None).unwrap(); // 2 — ok
        let e = fi.send(&cmd, None).unwrap_err(); // 3 — fault
        assert_eq!(e, RcpError::NotConnected);
        let e = fi.send(&cmd, None).unwrap_err(); // 4 — fault
        assert_eq!(e, RcpError::NotConnected);
    }

    #[test]
    // fusa:test REQ-FI-004
    fn inject_multiple_rules_first_match_wins() {
        let fi = fi();
        fi.inject(FaultSpec { rule: FaultRule::OnNthCall(1), error: RcpError::Timeout });
        fi.inject(FaultSpec { rule: FaultRule::Always, error: RcpError::Busy });
        let cmd = Command { zone: Zone::FRONT_LEFT, ..Default::default() };
        let err = fi.send(&cmd, None).unwrap_err();
        // First matching rule wins (OnNthCall(1) matches on call 1)
        assert_eq!(err, RcpError::Timeout);
    }

    #[test]
    // fusa:test REQ-FI-005
    fn clear_removes_all_faults() {
        let fi = fi();
        fi.inject(FaultSpec { rule: FaultRule::Always, error: RcpError::Timeout });
        fi.clear();
        let cmd = Command { zone: Zone::FRONT_LEFT, ..Default::default() };
        fi.send(&cmd, None).unwrap();
    }

    #[test]
    // fusa:test REQ-FI-007
    fn close_forwarded() {
        let fi = fi();
        fi.close().unwrap();
    }
}
