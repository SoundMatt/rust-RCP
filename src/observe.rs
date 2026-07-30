// fusa:req REQ-OBS-001
// fusa:req REQ-OBS-002
// fusa:req REQ-OBS-003
// fusa:req REQ-OBS-004
// fusa:req REQ-OBS-005
// fusa:req REQ-OBS-006

//! Observability hooks — latency histogram, error counters, and event
//! callbacks over an [`Endpoint`].
//!
//! `ROADMAP.md` Milestone 9 ("All ADAPT-disposition packages retargeted...")
//! cutover: per this module's own ADAPT disposition ("generic
//! metrics/latency decorator, entirely protocol-agnostic"),
//! [`ObserveEndpoint`] replaces the legacy `ObserveController`, wrapping
//! [`crate::mock::Endpoint`] instead of `Controller`. The old single
//! `add_hook`, keyed on `(&Command, &Result<Response, _>, Duration)`, is
//! split into [`Self::add_read_hook`]/[`Self::add_write_hook`], since
//! `Endpoint::read` and `Endpoint::write` return different `Result` types
//! (`Vec<u8>` vs `()`) with no common shape to give a single hook —
//! flagged here per Guiding Principle 5 rather than force-unifying them
//! behind an invented wrapper type.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::mock::Endpoint;
use crate::regmap::EndpointType;
use crate::RcpError;

// ── Metrics ───────────────────────────────────────────────────────────────────

/// Aggregated call metrics for an endpoint.
// fusa:req REQ-OBS-001
#[derive(Debug, Default)]
pub struct Metrics {
    pub total_calls: AtomicU64,
    pub total_errors: AtomicU64,
    pub total_latency_us: AtomicU64,
}

impl Metrics {
    pub fn calls(&self) -> u64 {
        self.total_calls.load(Ordering::Relaxed)
    }
    pub fn errors(&self) -> u64 {
        self.total_errors.load(Ordering::Relaxed)
    }
    pub fn latency_us(&self) -> u64 {
        self.total_latency_us.load(Ordering::Relaxed)
    }

    /// Mean latency in microseconds (0 if no calls).
    pub fn mean_latency_us(&self) -> u64 {
        self.latency_us().checked_div(self.calls()).unwrap_or(0)
    }

    fn record(&self, elapsed: Duration, is_err: bool) {
        self.total_calls.fetch_add(1, Ordering::Relaxed);
        self.total_latency_us
            .fetch_add(elapsed.as_micros() as u64, Ordering::Relaxed);
        if is_err {
            self.total_errors.fetch_add(1, Ordering::Relaxed);
        }
    }
}

// ── ObserveEndpoint ────────────────────────────────────────────────────────────

type ReadHookFn = Box<dyn Fn(u16, &Result<Vec<u8>, RcpError>, Duration) + Send + Sync>;
type WriteHookFn = Box<dyn Fn(&[u8], &Result<(), RcpError>, Duration) + Send + Sync>;

/// Observing wrapper that records metrics and fires post-call hooks.
// fusa:req REQ-OBS-002
pub struct ObserveEndpoint {
    inner: Arc<dyn Endpoint>,
    metrics: Arc<Metrics>,
    read_hooks: Mutex<Vec<ReadHookFn>>,
    write_hooks: Mutex<Vec<WriteHookFn>>,
}

impl ObserveEndpoint {
    pub fn new(inner: Arc<dyn Endpoint>) -> Self {
        ObserveEndpoint {
            inner,
            metrics: Arc::new(Metrics::default()),
            read_hooks: Mutex::new(Vec::new()),
            write_hooks: Mutex::new(Vec::new()),
        }
    }

    /// Snapshot of aggregated metrics (covers both reads and writes).
    // fusa:req REQ-OBS-003
    pub fn metrics(&self) -> Arc<Metrics> {
        Arc::clone(&self.metrics)
    }

    /// Register a post-read hook.
    // fusa:req REQ-OBS-004
    pub fn add_read_hook(
        &self,
        f: impl Fn(u16, &Result<Vec<u8>, RcpError>, Duration) + Send + Sync + 'static,
    ) {
        self.read_hooks.lock().unwrap().push(Box::new(f));
    }

    /// Register a post-write hook.
    // fusa:req REQ-OBS-004
    pub fn add_write_hook(
        &self,
        f: impl Fn(&[u8], &Result<(), RcpError>, Duration) + Send + Sync + 'static,
    ) {
        self.write_hooks.lock().unwrap().push(Box::new(f));
    }
}

impl Endpoint for ObserveEndpoint {
    fn ep_type(&self) -> EndpointType {
        self.inner.ep_type()
    }

    // fusa:req REQ-OBS-005
    fn read(&self, read_size: u16) -> Result<Vec<u8>, RcpError> {
        let start = Instant::now();
        let result = self.inner.read(read_size);
        let elapsed = start.elapsed();
        self.metrics.record(elapsed, result.is_err());
        for hook in self.read_hooks.lock().unwrap().iter() {
            hook(read_size, &result, elapsed);
        }
        result
    }

    // fusa:req REQ-OBS-006
    fn write(&self, payload: &[u8]) -> Result<(), RcpError> {
        let start = Instant::now();
        let result = self.inner.write(payload);
        let elapsed = start.elapsed();
        self.metrics.record(elapsed, result.is_err());
        for hook in self.write_hooks.lock().unwrap().iter() {
            hook(payload, &result, elapsed);
        }
        result
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockEndpoint;

    fn ok_ep() -> Arc<dyn Endpoint> {
        MockEndpoint::new(EndpointType::Gpio, vec![0u8; 4]) as Arc<dyn Endpoint>
    }

    struct AlwaysFail;
    impl Endpoint for AlwaysFail {
        fn ep_type(&self) -> EndpointType {
            EndpointType::Gpio
        }
        fn read(&self, _read_size: u16) -> Result<Vec<u8>, RcpError> {
            Err(RcpError::Closed)
        }
        fn write(&self, _payload: &[u8]) -> Result<(), RcpError> {
            Err(RcpError::Closed)
        }
    }

    #[test]
    // fusa:test REQ-OBS-002
    // fusa:test REQ-OBS-003
    fn call_count_increments() {
        let o = ObserveEndpoint::new(ok_ep());
        for _ in 0..5 {
            o.write(b"x").unwrap();
        }
        assert_eq!(o.metrics().calls(), 5);
    }

    #[test]
    // fusa:test REQ-OBS-003
    fn error_count_increments_on_failure() {
        let o = ObserveEndpoint::new(Arc::new(AlwaysFail) as Arc<dyn Endpoint>);
        let _ = o.write(b"x");
        assert_eq!(o.metrics().errors(), 1);
    }

    #[test]
    // fusa:test REQ-OBS-004
    // fusa:test REQ-OBS-005
    fn read_hook_is_called_after_read() {
        let fired = Arc::new(AtomicU64::new(0));
        let f2 = Arc::clone(&fired);
        let o = ObserveEndpoint::new(ok_ep());
        o.add_read_hook(move |_size, _res, _lat| {
            f2.fetch_add(1, Ordering::SeqCst);
        });
        o.read(4).unwrap();
        assert_eq!(fired.load(Ordering::SeqCst), 1);
    }

    #[test]
    // fusa:test REQ-OBS-006
    fn write_hook_is_called_after_write() {
        let fired = Arc::new(AtomicU64::new(0));
        let f2 = Arc::clone(&fired);
        let o = ObserveEndpoint::new(ok_ep());
        o.add_write_hook(move |_payload, _res, _lat| {
            f2.fetch_add(1, Ordering::SeqCst);
        });
        o.write(b"x").unwrap();
        assert_eq!(fired.load(Ordering::SeqCst), 1);
    }

    #[test]
    // fusa:test REQ-OBS-001
    fn mean_latency_is_non_negative() {
        let o = ObserveEndpoint::new(ok_ep());
        o.write(b"x").unwrap();
        // Mean latency may be 0 on fast machines, but must not panic
        let _ = o.metrics().mean_latency_us();
    }
}
