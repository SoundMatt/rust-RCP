//fusa:req REQ-DL-001
//fusa:req REQ-DL-002
//fusa:req REQ-DL-003
//fusa:req REQ-DL-004
//fusa:req REQ-DL-005
//fusa:req REQ-DL-006

//! Deadline monitor — enforces a maximum call latency on an [`Endpoint`].
//!
//! `ROADMAP.md` Milestone 9 ("All ADAPT-disposition packages retargeted...")
//! cutover: per this module's own ADAPT disposition ("generic client-side
//! call-timeout decorator; retarget to the new API, distinct from the
//! spec's own presentation-timestamp semantics"), [`DeadlineEndpoint`]
//! replaces the legacy `DeadlineController`.
//!
//! [`crate::mock::Endpoint::read`]/[`crate::mock::Endpoint::write`]'s own
//! fixed signatures carry no `timeout` parameter — unlike the deleted
//! `Controller::send`, which took one directly — so a deadline cannot be
//! enforced through the plain [`Endpoint`] trait impl this type also
//! provides (a pass-through, for use anywhere a bare `Arc<dyn Endpoint>` is
//! required). Deadline enforcement is instead exposed through the two
//! additional inherent methods [`DeadlineEndpoint::read_with_deadline`]/
//! [`DeadlineEndpoint::write_with_deadline`], the same "extend the base
//! trait with extra methods a caller must reach for explicitly" shape
//! [`crate::loan::LoanPoolEndpoint`] already established for
//! [`crate::Loan`]ed sends. A flagged consequence of this crate's `Endpoint`
//! model being purely synchronous, in-process dispatch with no real I/O
//! wait of its own (see `src/mock.rs`'s own doc comment): the zero-timeout-is-already-
//! expired check remains fully observable, but the `min(caller, deadline)`
//! effective-timeout computation has nothing left to bound once computed,
//! since neither `read` nor `write` can actually block past it. This is
//! recorded here per Guiding Principle 5 rather than silently pretending
//! the deadline is enforced against real latency.

use std::sync::Arc;
use std::time::Duration;

use crate::mock::Endpoint;
use crate::regmap::EndpointType;
use crate::RcpError;

// ── DeadlineEndpoint ──────────────────────────────────────────────────────────

/// Wraps an inner [`Endpoint`] with a configured deadline, honored by
/// [`Self::read_with_deadline`]/[`Self::write_with_deadline`].
//fusa:req REQ-DL-001
pub struct DeadlineEndpoint {
    inner: Arc<dyn Endpoint>,
    deadline: Duration,
}

impl DeadlineEndpoint {
    /// Create a new deadline endpoint.
    ///
    /// # Panics
    /// Panics if `deadline` is zero (use the `Timeout` sentinel instead).
    //fusa:req REQ-DL-002
    pub fn new(inner: Arc<dyn Endpoint>, deadline: Duration) -> Self {
        assert!(!deadline.is_zero(), "deadline must be non-zero");
        DeadlineEndpoint { inner, deadline }
    }

    /// The configured deadline.
    pub fn deadline(&self) -> Duration {
        self.deadline
    }

    /// Compute `min(timeout, self.deadline)`, or `Err(RcpError::Timeout)`
    /// immediately if `timeout` is already `Some(Duration::ZERO)`.
    fn effective(&self, timeout: Option<Duration>) -> Result<Duration, RcpError> {
        if timeout == Some(Duration::ZERO) {
            return Err(RcpError::Timeout);
        }
        Ok(match timeout {
            None => self.deadline,
            Some(t) => t.min(self.deadline),
        })
    }

    /// Read from the inner endpoint, enforcing this deadline against
    /// `timeout` — see this module's doc comment for what "enforcing"
    /// means on this crate's synchronous `Endpoint` model.
    //fusa:req REQ-DL-003
    //fusa:req REQ-DL-004
    //fusa:req REQ-DL-005
    pub fn read_with_deadline(
        &self,
        read_size: u16,
        timeout: Option<Duration>,
    ) -> Result<Vec<u8>, RcpError> {
        let _effective = self.effective(timeout)?;
        self.inner.read(read_size)
    }

    /// Same as [`Self::read_with_deadline`], for a write.
    //fusa:req REQ-DL-003
    //fusa:req REQ-DL-004
    //fusa:req REQ-DL-005
    pub fn write_with_deadline(
        &self,
        payload: &[u8],
        timeout: Option<Duration>,
    ) -> Result<(), RcpError> {
        let _effective = self.effective(timeout)?;
        self.inner.write(payload)
    }
}

impl Endpoint for DeadlineEndpoint {
    fn ep_type(&self) -> EndpointType {
        self.inner.ep_type()
    }

    //fusa:req REQ-DL-006
    fn read(&self, read_size: u16) -> Result<Vec<u8>, RcpError> {
        self.inner.read(read_size)
    }

    //fusa:req REQ-DL-006
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

    fn quick_endpoint() -> Arc<dyn Endpoint> {
        MockEndpoint::new(EndpointType::Gpio, vec![1, 2, 3, 4]) as Arc<dyn Endpoint>
    }

    #[test]
    //fusa:test REQ-DL-001
    //fusa:test REQ-DL-003
    fn passes_calls_to_inner() {
        let dl = DeadlineEndpoint::new(quick_endpoint(), Duration::from_secs(1));
        dl.write_with_deadline(b"hi", None).unwrap();
        dl.read_with_deadline(4, None).unwrap();
    }

    #[test]
    //fusa:test REQ-DL-002
    fn deadline_getter() {
        let d = Duration::from_millis(500);
        let dl = DeadlineEndpoint::new(quick_endpoint(), d);
        assert_eq!(dl.deadline(), d);
    }

    #[test]
    //fusa:test REQ-DL-004
    fn zero_timeout_returns_timeout_error() {
        let dl = DeadlineEndpoint::new(quick_endpoint(), Duration::from_secs(1));
        let err = dl
            .write_with_deadline(b"hi", Some(Duration::ZERO))
            .unwrap_err();
        assert_eq!(err, RcpError::Timeout);

        let err = dl.read_with_deadline(4, Some(Duration::ZERO)).unwrap_err();
        assert_eq!(err, RcpError::Timeout);
    }

    #[test]
    //fusa:test REQ-DL-005
    fn shorter_caller_timeout_wins() {
        let dl = DeadlineEndpoint::new(quick_endpoint(), Duration::from_secs(10));
        // If caller timeout is shorter, that is the effective timeout.
        dl.write_with_deadline(b"hi", Some(Duration::from_secs(5)))
            .unwrap();
    }

    #[test]
    //fusa:test REQ-DL-006
    fn plain_endpoint_impl_ignores_deadline() {
        let dl = DeadlineEndpoint::new(quick_endpoint(), Duration::from_secs(1));
        let as_endpoint: &dyn Endpoint = &dl;
        as_endpoint.write(b"hi").unwrap();
        as_endpoint.read(4).unwrap();
        assert_eq!(as_endpoint.ep_type(), EndpointType::Gpio);
    }
}
