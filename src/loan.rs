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
//! [`crate::Loan`] from the pool, fill it, and pass it to
//! [`LoanPoolEndpoint::write_loaned`]. On completion the buffer is
//! automatically returned to the pool.
//!
//! `ROADMAP.md` Milestone 9 ("All ADAPT-disposition packages retargeted...")
//! cutover: per this module's own ADAPT disposition ("zero-copy
//! buffer-pool concept remains useful for endpoint payload buffers
//! (SPI/UART/CAN); retarget to the new API"), [`LoanPoolEndpoint`] replaces
//! the legacy `LoanPoolController`, wrapping [`crate::mock::Endpoint`]
//! instead of `Controller`. [`crate::Loan`]/[`LoanPool`] themselves are
//! unchanged — plain `Vec<u8>`-buffer pool machinery with no `Controller`
//! coupling of its own — only the endpoint being loaned into changes.
//! [`crate::LoaningController`] (the old trait `LoanPoolController`
//! implemented) is left in place, unused by this module now, the same way
//! `Controller`/`Registry` themselves are left in place by this bullet;
//! removing it is `lib.rs`'s own core-surface cutover, Milestone 10's job.
//! [`LoanPoolEndpoint`] instead exposes [`Self::loan`]/[`Self::write_loaned`]
//! as its own inherent methods rather than implementing that trait, since
//! `LoaningController: Controller` cannot be implemented by a type that no
//! longer implements `Controller`.

use std::sync::{Arc, Condvar, Mutex};

use crate::mock::Endpoint;
use crate::regmap::EndpointType;
use crate::{Loan, RcpError};

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

// ── LoanPoolEndpoint ─────────────────────────────────────────────────────────

/// An endpoint decorator backed by a `LoanPool` for zero-copy writes.
// fusa:req REQ-LOAN-005
pub struct LoanPoolEndpoint {
    inner: Arc<dyn Endpoint>,
    pool: Arc<LoanPool>,
}

impl LoanPoolEndpoint {
    pub fn new(inner: Arc<dyn Endpoint>, pool: Arc<LoanPool>) -> Self {
        LoanPoolEndpoint { inner, pool }
    }

    /// Obtain a loaned buffer of `size` bytes.
    ///
    /// Returns `Err(RcpError::PayloadTooLarge)` if `size` exceeds the
    /// pool's buffer size.
    // fusa:req REQ-LOAN-006
    pub fn loan(&self, size: usize) -> Result<Loan, RcpError> {
        if size > self.pool.buffer_size() {
            return Err(RcpError::PayloadTooLarge);
        }
        Ok(self.pool.acquire())
    }

    /// Write a previously loaned buffer's payload to the inner endpoint.
    ///
    /// The buffer is returned to the pool once this call completes (the
    /// `loan` is dropped either way).
    // fusa:req REQ-LOAN-007
    pub fn write_loaned(&self, loan: Loan) -> Result<(), RcpError> {
        let payload = loan.payload.clone();
        // Buffer returned to pool on drop (loan's release fn fires).
        drop(loan);
        self.inner.write(&payload)
    }
}

impl Endpoint for LoanPoolEndpoint {
    fn ep_type(&self) -> EndpointType {
        self.inner.ep_type()
    }

    fn read(&self, read_size: u8) -> Result<Vec<u8>, RcpError> {
        self.inner.read(read_size)
    }

    fn write(&self, payload: &[u8]) -> Result<(), RcpError> {
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

    fn inner() -> Arc<dyn Endpoint> {
        MockEndpoint::new(EndpointType::Spi, vec![]) as Arc<dyn Endpoint>
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
        let ep = LoanPoolEndpoint::new(inner(), Arc::clone(&pool));
        let err = ep.loan(65).unwrap_err();
        assert_eq!(err, RcpError::PayloadTooLarge);
    }

    #[test]
    // fusa:test REQ-LOAN-007
    fn write_loaned_forwards_payload() {
        let received = Arc::new(std::sync::Mutex::new(vec![]));
        let recv2 = Arc::clone(&received);
        let inner = crate::mock::MockEndpoint::new(EndpointType::Spi, vec![]);
        // Wrap in a small pass-through Endpoint that also records writes.
        struct Recording {
            inner: Arc<dyn Endpoint>,
            log: Arc<Mutex<Vec<Vec<u8>>>>,
        }
        impl Endpoint for Recording {
            fn ep_type(&self) -> EndpointType {
                self.inner.ep_type()
            }
            fn read(&self, read_size: u8) -> Result<Vec<u8>, RcpError> {
                self.inner.read(read_size)
            }
            fn write(&self, payload: &[u8]) -> Result<(), RcpError> {
                self.log.lock().unwrap().push(payload.to_vec());
                self.inner.write(payload)
            }
        }
        let recording = Arc::new(Recording {
            inner: inner as Arc<dyn Endpoint>,
            log: recv2,
        }) as Arc<dyn Endpoint>;

        let pool = Arc::new(LoanPool::new(1, 8));
        let ep = LoanPoolEndpoint::new(recording, Arc::clone(&pool));

        let mut loan = ep.loan(4).unwrap();
        loan.payload[..4].copy_from_slice(b"test");
        ep.write_loaned(loan).unwrap();

        let got = received.lock().unwrap();
        assert!(got[0].starts_with(b"test"), "payload must be forwarded");
        // Buffer should now be returned (write_loaned drops the loan)
        assert_eq!(
            pool.available(),
            1,
            "buffer must be returned after write_loaned"
        );
    }

    #[test]
    // fusa:test REQ-LOAN-005
    fn loan_endpoint_ep_type_matches_inner() {
        let pool = Arc::new(LoanPool::new(1, 64));
        let ep = LoanPoolEndpoint::new(inner(), pool);
        assert_eq!(ep.ep_type(), EndpointType::Spi);
    }
}
