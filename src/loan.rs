// fusa:req REQ-LOAN-001
// fusa:req REQ-LOAN-002
// fusa:req REQ-LOAN-003
// fusa:req REQ-LOAN-004
// fusa:req REQ-LOAN-005
// fusa:req REQ-LOAN-006
// fusa:req REQ-LOAN-007

//! Pool-based zero-copy payload loaning.
//!
//! `LoanPool` maintains a set of pre-allocated buffers; callers obtain a
//! [`crate::Loan`] from the pool, fill it, and pass it to `send_loaned`.
//! On completion the buffer is automatically returned to the pool.

use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use crate::{Command, Controller, Loan, LoaningController, RcpError, Response, Subscription, Zone};

// ── LoanPool ──────────────────────────────────────────────────────────────────

/// Pre-allocated buffer pool.
// fusa:req REQ-LOAN-001
pub struct LoanPool {
    state: Arc<(Mutex<Vec<Vec<u8>>>, Condvar)>,
    size: usize,
}

impl LoanPool {
    /// Create a pool with `count` buffers each of `size` bytes.
    // fusa:req REQ-LOAN-002
    pub fn new(count: usize, size: usize) -> Self {
        let pool: Vec<Vec<u8>> = (0..count).map(|_| vec![0u8; size]).collect();
        LoanPool {
            state: Arc::new((Mutex::new(pool), Condvar::new())),
            size,
        }
    }

    /// Obtain a buffer from the pool, blocking until one is available.
    // fusa:req REQ-LOAN-003
    pub fn acquire(&self) -> Loan {
        let state = Arc::clone(&self.state);
        let buf = {
            let (lock, cvar) = &*self.state;
            let mut pool = lock.lock().unwrap();
            loop {
                if let Some(b) = pool.pop() {
                    break b;
                }
                pool = cvar.wait(pool).unwrap();
            }
        };
        Loan::new(buf, move |returned| {
            let (lock, cvar) = &*state;
            lock.lock().unwrap().push(returned);
            cvar.notify_one();
        })
    }

    /// Try to obtain a buffer without blocking. Returns `None` if pool is empty.
    // fusa:req REQ-LOAN-004
    pub fn try_acquire(&self) -> Option<Loan> {
        let state = Arc::clone(&self.state);
        let (lock, _) = &*self.state;
        let buf = lock.lock().unwrap().pop()?;
        Some(Loan::new(buf, move |returned| {
            let (lock, cvar) = &*state;
            lock.lock().unwrap().push(returned);
            cvar.notify_one();
        }))
    }

    /// Buffer size this pool provides.
    pub fn buffer_size(&self) -> usize {
        self.size
    }

    /// Number of buffers currently available.
    pub fn available(&self) -> usize {
        self.state.0.lock().unwrap().len()
    }
}

// ── LoanPoolController ────────────────────────────────────────────────────────

/// A controller backed by a `LoanPool` for zero-copy sends.
// fusa:req REQ-LOAN-005
pub struct LoanPoolController {
    inner: Arc<dyn Controller>,
    pool: Arc<LoanPool>,
}

impl LoanPoolController {
    pub fn new(inner: Arc<dyn Controller>, pool: Arc<LoanPool>) -> Self {
        LoanPoolController { inner, pool }
    }
}

impl Controller for LoanPoolController {
    fn zone(&self) -> Zone {
        self.inner.zone()
    }

    fn send(&self, cmd: &Command, timeout: Option<Duration>) -> Result<Response, RcpError> {
        self.inner.send(cmd, timeout)
    }

    fn subscribe(&self) -> Result<Subscription, RcpError> {
        self.inner.subscribe()
    }

    fn close(&self) -> Result<(), RcpError> {
        self.inner.close()
    }
}

impl LoaningController for LoanPoolController {
    // fusa:req REQ-LOAN-006
    fn loan(&self, size: usize) -> Result<Loan, RcpError> {
        if size > self.pool.buffer_size() {
            return Err(RcpError::PayloadTooLarge);
        }
        Ok(self.pool.acquire())
    }

    // fusa:req REQ-LOAN-007
    fn send_loaned(
        &self,
        loan: Loan,
        mut cmd: Command,
        timeout: Option<Duration>,
    ) -> Result<Response, RcpError> {
        cmd.payload = Some(loan.payload.clone());
        // Buffer returned to pool on drop (loan's release fn fires).
        drop(loan);
        self.inner.send(&cmd, timeout)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockController;
    use crate::Zone;

    fn inner() -> Arc<dyn Controller> {
        MockController::new(Zone::FRONT_LEFT, None) as Arc<dyn Controller>
    }

    #[test]
    // fusa:test REQ-LOAN-001
    // fusa:test REQ-LOAN-002
    fn pool_created_with_correct_count() {
        let pool = LoanPool::new(3, 64);
        assert_eq!(pool.available(), 3);
        assert_eq!(pool.buffer_size(), 64);
    }

    #[test]
    // fusa:test REQ-LOAN-003
    fn acquire_reduces_available() {
        let pool = LoanPool::new(2, 64);
        let _loan = pool.acquire();
        assert_eq!(pool.available(), 1);
    }

    #[test]
    // fusa:test REQ-LOAN-003
    fn buffer_returned_on_drop() {
        let pool = LoanPool::new(1, 64);
        {
            let _loan = pool.acquire();
            assert_eq!(pool.available(), 0);
        }
        assert_eq!(pool.available(), 1, "buffer must be returned on drop");
    }

    #[test]
    // fusa:test REQ-LOAN-004
    fn try_acquire_returns_none_when_empty() {
        let pool = LoanPool::new(1, 64);
        let _l1 = pool.acquire();
        assert!(pool.try_acquire().is_none());
    }

    #[test]
    // fusa:test REQ-LOAN-006
    fn loan_rejects_oversized_request() {
        let pool = Arc::new(LoanPool::new(2, 64));
        let ctrl = LoanPoolController::new(inner(), Arc::clone(&pool));
        let err = ctrl.loan(65).unwrap_err();
        assert_eq!(err, RcpError::PayloadTooLarge);
    }

    #[test]
    // fusa:test REQ-LOAN-007
    fn send_loaned_forwards_payload() {
        let received = Arc::new(std::sync::Mutex::new(vec![]));
        let recv2 = Arc::clone(&received);
        let h: crate::mock::Handler = Box::new(move |cmd| {
            recv2
                .lock()
                .unwrap()
                .push(cmd.payload.clone().unwrap_or_default());
            crate::Response {
                command_id: cmd.id,
                zone: cmd.zone,
                status: crate::ResponseStatus::OK,
                payload: None,
            }
        });
        let inner = MockController::new(Zone::FRONT_LEFT, Some(h)) as Arc<dyn Controller>;
        let pool = Arc::new(LoanPool::new(1, 8));
        let ctrl = LoanPoolController::new(inner, Arc::clone(&pool));

        let mut loan = ctrl.loan(4).unwrap();
        loan.payload[..4].copy_from_slice(b"test");
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            ..Default::default()
        };
        ctrl.send_loaned(loan, cmd, None).unwrap();

        let got = received.lock().unwrap();
        assert!(got[0].starts_with(b"test"), "payload must be forwarded");
        // Buffer should now be returned (send_loaned drops the loan)
        assert_eq!(
            pool.available(),
            1,
            "buffer must be returned after send_loaned"
        );
    }

    #[test]
    // fusa:test REQ-LOAN-005
    fn loan_controller_zone_matches_inner() {
        let pool = Arc::new(LoanPool::new(1, 64));
        let ctrl = LoanPoolController::new(inner(), pool);
        assert_eq!(ctrl.zone(), Zone::FRONT_LEFT);
    }
}
