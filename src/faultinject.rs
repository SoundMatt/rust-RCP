// fusa:req REQ-FI-001
// fusa:req REQ-FI-002
// fusa:req REQ-FI-003
// fusa:req REQ-FI-004
// fusa:req REQ-FI-005
// fusa:req REQ-FI-006
// fusa:req REQ-FI-007

//! Fault injection — deterministic error injection for safety test campaigns.
//!
//! Wraps an inner [`Endpoint`]; errors are injected via pre-programmed
//! rules (nth-call injection, always-inject, or after-nth-call).
//!
//! `ROADMAP.md` Milestone 9 ("All ADAPT-disposition packages retargeted...")
//! cutover: per this module's own ADAPT disposition ("generic
//! fault-injection decorator for safety test campaigns; retarget to the
//! new trait"), [`FaultInjectEndpoint`] replaces the legacy
//! `FaultInjectController`, wrapping [`crate::mock::Endpoint`] instead of
//! `Controller`. A single shared call counter still spans both
//! [`Endpoint::read`] and [`Endpoint::write`] calls, exactly as the old
//! type's counter spanned every `send`, so a fault rule keyed on "the Nth
//! call" fires on the Nth call of either kind.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::mock::Endpoint;
use crate::regmap::EndpointType;
use crate::RcpError;

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
    pub rule: FaultRule,
    pub error: RcpError,
}

// ── FaultInjectEndpoint ───────────────────────────────────────────────────────

struct Inner {
    faults: Vec<FaultSpec>,
    call_no: u64,
}

/// Fault-injecting endpoint wrapper.
// fusa:req REQ-FI-003
pub struct FaultInjectEndpoint {
    inner: Arc<dyn Endpoint>,
    state: Mutex<Inner>,
    total: AtomicU64,
}

impl FaultInjectEndpoint {
    pub fn new(inner: Arc<dyn Endpoint>) -> Self {
        FaultInjectEndpoint {
            inner,
            state: Mutex::new(Inner {
                faults: Vec::new(),
                call_no: 0,
            }),
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

    /// Total number of `read`/`write` calls made (including faulted ones).
    pub fn call_count(&self) -> u64 {
        self.total.load(Ordering::SeqCst)
    }

    /// Advance the shared call counter and return the fault (if any)
    /// triggered by the resulting call number.
    fn next_fault(&self) -> Option<RcpError> {
        let call_no = {
            let mut g = self.state.lock().unwrap();
            g.call_no += 1;
            g.call_no
        };
        self.total.fetch_add(1, Ordering::SeqCst);

        let g = self.state.lock().unwrap();
        g.faults.iter().find_map(|spec| {
            let triggered = match spec.rule {
                FaultRule::Always => true,
                FaultRule::OnNthCall(n) => call_no == n,
                FaultRule::AfterNthCall(n) => call_no >= n,
            };
            if triggered {
                Some(spec.error.clone())
            } else {
                None
            }
        })
    }
}

impl Endpoint for FaultInjectEndpoint {
    fn ep_type(&self) -> EndpointType {
        self.inner.ep_type()
    }

    // fusa:req REQ-FI-006
    fn read(&self, read_size: u8) -> Result<Vec<u8>, RcpError> {
        if let Some(err) = self.next_fault() {
            return Err(err);
        }
        self.inner.read(read_size)
    }

    // fusa:req REQ-FI-006
    fn write(&self, payload: &[u8]) -> Result<(), RcpError> {
        if let Some(err) = self.next_fault() {
            return Err(err);
        }
        self.inner.write(payload)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockEndpoint;

    fn fi() -> FaultInjectEndpoint {
        let inner = MockEndpoint::new(EndpointType::Gpio, vec![0u8; 4]) as Arc<dyn Endpoint>;
        FaultInjectEndpoint::new(inner)
    }

    #[test]
    // fusa:test REQ-FI-001
    // fusa:test REQ-FI-003
    fn no_fault_passes_through() {
        let fi = fi();
        fi.write(b"x").unwrap();
    }

    #[test]
    // fusa:test REQ-FI-002
    // fusa:test REQ-FI-006
    fn always_fault_injects_every_call() {
        let fi = fi();
        fi.inject(FaultSpec {
            rule: FaultRule::Always,
            error: RcpError::Timeout,
        });
        for _ in 0..3 {
            let err = fi.write(b"x").unwrap_err();
            assert_eq!(err, RcpError::Timeout);
        }
    }

    #[test]
    // fusa:test REQ-FI-002
    // fusa:test REQ-FI-006
    fn nth_call_fault_triggers_only_on_n() {
        let fi = fi();
        fi.inject(FaultSpec {
            rule: FaultRule::OnNthCall(2),
            error: RcpError::Busy,
        });
        fi.write(b"x").unwrap(); // call 1 — ok
        let err = fi.write(b"x").unwrap_err(); // call 2 — fault
        assert_eq!(err, RcpError::Busy);
        fi.write(b"x").unwrap(); // call 3 — ok again
    }

    #[test]
    // fusa:test REQ-FI-002
    // fusa:test REQ-FI-006
    fn after_nth_call_triggers_from_n_onwards() {
        let fi = fi();
        fi.inject(FaultSpec {
            rule: FaultRule::AfterNthCall(3),
            error: RcpError::NotConnected,
        });
        fi.write(b"x").unwrap(); // 1 — ok
        fi.write(b"x").unwrap(); // 2 — ok
        let e = fi.write(b"x").unwrap_err(); // 3 — fault
        assert_eq!(e, RcpError::NotConnected);
        let e = fi.read(1).unwrap_err(); // 4 — fault (shared counter)
        assert_eq!(e, RcpError::NotConnected);
    }

    #[test]
    // fusa:test REQ-FI-004
    fn inject_multiple_rules_first_match_wins() {
        let fi = fi();
        fi.inject(FaultSpec {
            rule: FaultRule::OnNthCall(1),
            error: RcpError::Timeout,
        });
        fi.inject(FaultSpec {
            rule: FaultRule::Always,
            error: RcpError::Busy,
        });
        let err = fi.write(b"x").unwrap_err();
        // First matching rule wins (OnNthCall(1) matches on call 1)
        assert_eq!(err, RcpError::Timeout);
    }

    #[test]
    // fusa:test REQ-FI-005
    fn clear_removes_all_faults() {
        let fi = fi();
        fi.inject(FaultSpec {
            rule: FaultRule::Always,
            error: RcpError::Timeout,
        });
        fi.clear();
        fi.write(b"x").unwrap();
    }

    #[test]
    // fusa:test REQ-FI-007
    fn call_count_tracks_both_ops() {
        let fi = fi();
        fi.write(b"x").unwrap();
        fi.read(1).unwrap();
        assert_eq!(fi.call_count(), 2);
    }
}
