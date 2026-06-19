// fusa:req REQ-OBS-001
// fusa:req REQ-OBS-002
// fusa:req REQ-OBS-003
// fusa:req REQ-OBS-004
// fusa:req REQ-OBS-005
// fusa:req REQ-OBS-006

//! Observability hooks — latency histogram, error counters, and event callbacks.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::{Command, Controller, RcpError, Response, Subscription, Zone};

// ── Metrics ───────────────────────────────────────────────────────────────────

/// Aggregated call metrics for a controller.
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
        let calls = self.calls();
        if calls == 0 {
            0
        } else {
            self.latency_us() / calls
        }
    }
}

// ── ObserveController ─────────────────────────────────────────────────────────

type HookFn = Box<dyn Fn(&Command, &Result<Response, RcpError>, Duration) + Send + Sync>;

/// Observing wrapper that records metrics and fires post-send hooks.
// fusa:req REQ-OBS-002
pub struct ObserveController {
    inner: Arc<dyn Controller>,
    metrics: Arc<Metrics>,
    hooks: Mutex<Vec<HookFn>>,
}

impl ObserveController {
    pub fn new(inner: Arc<dyn Controller>) -> Self {
        ObserveController {
            inner,
            metrics: Arc::new(Metrics::default()),
            hooks: Mutex::new(Vec::new()),
        }
    }

    /// Snapshot of aggregated metrics.
    // fusa:req REQ-OBS-003
    pub fn metrics(&self) -> Arc<Metrics> {
        Arc::clone(&self.metrics)
    }

    /// Register a post-send hook.
    // fusa:req REQ-OBS-004
    pub fn add_hook(
        &self,
        f: impl Fn(&Command, &Result<Response, RcpError>, Duration) + Send + Sync + 'static,
    ) {
        self.hooks.lock().unwrap().push(Box::new(f));
    }
}

impl Controller for ObserveController {
    fn zone(&self) -> Zone {
        self.inner.zone()
    }

    // fusa:req REQ-OBS-005
    fn send(&self, cmd: &Command, timeout: Option<Duration>) -> Result<Response, RcpError> {
        let start = Instant::now();
        let result = self.inner.send(cmd, timeout);
        let elapsed = start.elapsed();

        self.metrics.total_calls.fetch_add(1, Ordering::Relaxed);
        self.metrics
            .total_latency_us
            .fetch_add(elapsed.as_micros() as u64, Ordering::Relaxed);
        if result.is_err() {
            self.metrics.total_errors.fetch_add(1, Ordering::Relaxed);
        }

        for hook in self.hooks.lock().unwrap().iter() {
            hook(cmd, &result, elapsed);
        }

        result
    }

    fn subscribe(&self) -> Result<Subscription, RcpError> {
        self.inner.subscribe()
    }

    // fusa:req REQ-OBS-006
    fn close(&self) -> Result<(), RcpError> {
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
    use crate::{Command, Response, ResponseStatus, Zone};

    fn ok_ctrl() -> Arc<dyn Controller> {
        let h: crate::mock::Handler = Box::new(|cmd| Response {
            command_id: cmd.id,
            zone: cmd.zone,
            status: ResponseStatus::OK,
            payload: None,
        });
        MockController::new(Zone::FRONT_LEFT, Some(h)) as Arc<dyn Controller>
    }

    #[test]
    // fusa:test REQ-OBS-002
    // fusa:test REQ-OBS-003
    fn call_count_increments() {
        let o = ObserveController::new(ok_ctrl());
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            ..Default::default()
        };
        for _ in 0..5 {
            o.send(&cmd, None).unwrap();
        }
        assert_eq!(o.metrics().calls(), 5);
    }

    #[test]
    // fusa:test REQ-OBS-003
    fn error_count_increments_on_failure() {
        let inner = MockController::new(Zone::FRONT_LEFT, None) as Arc<dyn Controller>;
        inner.close().unwrap();
        let o = ObserveController::new(inner);
        let _ = o.send(
            &Command {
                zone: Zone::FRONT_LEFT,
                ..Default::default()
            },
            None,
        );
        assert_eq!(o.metrics().errors(), 1);
    }

    #[test]
    // fusa:test REQ-OBS-004
    // fusa:test REQ-OBS-005
    fn hook_is_called_after_send() {
        let fired = Arc::new(AtomicU64::new(0));
        let f2 = Arc::clone(&fired);
        let o = ObserveController::new(ok_ctrl());
        o.add_hook(move |_cmd, _res, _lat| {
            f2.fetch_add(1, Ordering::SeqCst);
        });
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            ..Default::default()
        };
        o.send(&cmd, None).unwrap();
        assert_eq!(fired.load(Ordering::SeqCst), 1);
    }

    #[test]
    // fusa:test REQ-OBS-001
    fn mean_latency_is_non_negative() {
        let o = ObserveController::new(ok_ctrl());
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            ..Default::default()
        };
        o.send(&cmd, None).unwrap();
        // Mean latency may be 0 on fast machines, but must not panic
        let _ = o.metrics().mean_latency_us();
    }

    #[test]
    // fusa:test REQ-OBS-006
    fn close_forwarded() {
        let o = ObserveController::new(ok_ctrl());
        o.close().unwrap();
    }
}
