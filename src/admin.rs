// fusa:req REQ-ADMIN-001
// fusa:req REQ-ADMIN-002
// fusa:req REQ-ADMIN-003
// fusa:req REQ-ADMIN-004
// fusa:req REQ-ADMIN-005

//! Administrative interface: health checks, graceful shutdown, and diagnostic info.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use crate::{RcpError, Registry};

// ── AdminServer ───────────────────────────────────────────────────────────────

/// Provides administrative diagnostics for an RCP registry.
// fusa:req REQ-ADMIN-001
pub struct AdminServer {
    registry: Arc<dyn Registry>,
    started: SystemTime,
    req_count: AtomicU64,
    shutdown: AtomicBool,
}

impl AdminServer {
    pub fn new(registry: Arc<dyn Registry>) -> Self {
        AdminServer {
            registry,
            started: SystemTime::now(),
            req_count: AtomicU64::new(0),
            shutdown: AtomicBool::new(false),
        }
    }

    /// Increment the admin request counter (call once per admin endpoint hit).
    // fusa:req REQ-ADMIN-002
    pub fn record_request(&self) {
        self.req_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Number of admin requests served since startup.
    pub fn request_count(&self) -> u64 {
        self.req_count.load(Ordering::Relaxed)
    }

    /// Uptime since the admin server was created.
    // fusa:req REQ-ADMIN-003
    pub fn uptime(&self) -> Duration {
        self.started.elapsed().unwrap_or(Duration::ZERO)
    }

    /// True if all zone controllers are reachable.
    // fusa:req REQ-ADMIN-004
    pub fn is_healthy(&self) -> bool {
        let controllers = self.registry.controllers();
        if controllers.is_empty() {
            return false;
        }
        controllers.iter().all(|c| {
            let cmd = crate::Command {
                zone: c.zone(),
                ..Default::default()
            };
            c.send(&cmd, Some(Duration::from_millis(100))).is_ok()
        })
    }

    /// Number of registered zone controllers.
    pub fn controller_count(&self) -> usize {
        self.registry.controllers().len()
    }

    /// Initiate graceful shutdown — closes the registry.
    // fusa:req REQ-ADMIN-005
    pub fn shutdown(&self) -> Result<(), RcpError> {
        self.shutdown.store(true, Ordering::SeqCst);
        self.registry.close()
    }

    /// True if shutdown has been initiated.
    pub fn is_shutting_down(&self) -> bool {
        self.shutdown.load(Ordering::SeqCst)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockRegistry;

    fn admin() -> AdminServer {
        AdminServer::new(Arc::new(MockRegistry::new()))
    }

    #[test]
    // fusa:test REQ-ADMIN-001
    // fusa:test REQ-ADMIN-004
    fn healthy_with_default_registry() {
        // MockRegistry pre-populates all zones, so is_healthy should be true
        assert!(admin().is_healthy());
    }

    #[test]
    // fusa:test REQ-ADMIN-002
    fn request_count_increments() {
        let a = admin();
        for _ in 0..5 {
            a.record_request();
        }
        assert_eq!(a.request_count(), 5);
    }

    #[test]
    // fusa:test REQ-ADMIN-003
    fn uptime_is_non_negative() {
        let a = admin();
        assert!(a.uptime() >= Duration::ZERO);
    }

    #[test]
    // fusa:test REQ-ADMIN-005
    fn shutdown_closes_registry() {
        let a = admin();
        a.shutdown().unwrap();
        assert!(a.is_shutting_down());
    }

    #[test]
    // fusa:test REQ-ADMIN-004
    fn controller_count_matches_registry() {
        let a = admin();
        assert_eq!(a.controller_count(), 5); // MockRegistry has 5 zones
    }
}
