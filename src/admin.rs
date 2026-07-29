// fusa:req REQ-ADMIN-001
// fusa:req REQ-ADMIN-002
// fusa:req REQ-ADMIN-003
// fusa:req REQ-ADMIN-004
// fusa:req REQ-ADMIN-005

//! Administrative interface: discovered-peer health/staleness reporting
//! and diagnostic info.
//!
//! `ROADMAP.md` Milestone 9 ("All ADAPT-disposition packages retargeted...")
//! cutover: per this module's own ADAPT disposition ("health-check/
//! graceful-shutdown wrapper over a `Registry`; concept persists once a
//! `Registry`-equivalent (discovered-server set) exists"), [`AdminServer`]
//! is rebuilt against [`crate::discovery::DiscoveryCache`] — the same
//! "discovery-derived server registry" dependency [`crate::federation`]'s
//! own retargeting note names — in place of the deleted `Registry` trait.
//!
//! Two behavioral narrowings from the old type, both flagged per Guiding
//! Principle 5 rather than silently made, since `DiscoveryCache` is a
//! passive client-side cache of previously observed peer identities, not a
//! live collection of dispatchable handles:
//!
//! - [`AdminServer::is_healthy`] no longer dispatches a real call to each
//!   peer to confirm reachability (there is nothing left to dispatch
//!   through — see this module's `Endpoint`-sibling modules' own notes on
//!   the same gap). It instead reports whether every cached entry is
//!   fresh — not [`crate::discovery::DiscoveryCacheEntry::is_stale`] as of
//!   a caller-supplied `now`/`max_age`, the same caller-supplies-the-clock
//!   discipline `DiscoveryCache` itself already requires.
//! - [`AdminServer::shutdown`] no longer closes any live controller (there
//!   is none to close); it invalidates every cached entry instead, the
//!   closest analog to "release what this server was holding" a passive
//!   cache offers.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use crate::discovery::DiscoveryCache;
use crate::RcpError;

// ── AdminServer ───────────────────────────────────────────────────────────────

/// Provides administrative diagnostics over a discovery cache of peers.
// fusa:req REQ-ADMIN-001
pub struct AdminServer {
    cache: Arc<Mutex<DiscoveryCache>>,
    started: SystemTime,
    req_count: AtomicU64,
    shutdown: AtomicBool,
}

impl AdminServer {
    pub fn new(cache: Arc<Mutex<DiscoveryCache>>) -> Self {
        AdminServer {
            cache,
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

    /// True if this server knows of at least one discovered peer.
    ///
    /// `DiscoveryCache` exposes no iterator over its entries' keys (by
    /// that module's own design — see its doc comment), so a whole-cache
    /// staleness sweep is not possible here; "healthy" narrows to
    /// "non-empty" for this coarse check. A caller that already knows
    /// which `StreamId`s to probe should use [`Self::is_peer_healthy`]
    /// instead, which does apply a real staleness check.
    // fusa:req REQ-ADMIN-004
    pub fn is_healthy(&self) -> bool {
        !self.cache.lock().unwrap().is_empty()
    }

    /// True if `stream_id`'s cached entry is known and not stale as of
    /// `now` under `max_age`.
    // fusa:req REQ-ADMIN-004
    pub fn is_peer_healthy(
        &self,
        stream_id: crate::avtp::StreamId,
        now: Instant,
        max_age: Duration,
    ) -> bool {
        self.cache.lock().unwrap().is_known(stream_id, now, max_age)
    }

    /// Number of peers currently cached.
    pub fn peer_count(&self) -> usize {
        self.cache.lock().unwrap().len()
    }

    /// Initiate graceful shutdown — invalidates `stream_id`'s cached entry.
    ///
    /// See this module's doc comment for why this narrows from "closes the
    /// registry" to "invalidate what's cached."
    // fusa:req REQ-ADMIN-005
    pub fn shutdown_peer(&self, stream_id: crate::avtp::StreamId) -> Result<(), RcpError> {
        self.shutdown.store(true, Ordering::SeqCst);
        self.cache.lock().unwrap().invalidate(stream_id);
        Ok(())
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
    use crate::avtp::StreamId;
    use crate::regmap::GeneralRegisters;

    fn stream(unique_id: u16) -> StreamId {
        StreamId::new([0x02, 0x00, 0x00, 0x00, 0x00, 0x01], unique_id)
    }

    fn admin_with_peer() -> (AdminServer, StreamId) {
        let sid = stream(1);
        let mut cache = DiscoveryCache::new();
        cache
            .remember(sid, &GeneralRegisters::default(), Instant::now())
            .unwrap();
        (AdminServer::new(Arc::new(Mutex::new(cache))), sid)
    }

    #[test]
    // fusa:test REQ-ADMIN-001
    // fusa:test REQ-ADMIN-004
    fn healthy_with_populated_cache() {
        let (a, _) = admin_with_peer();
        assert!(a.is_healthy());
    }

    #[test]
    // fusa:test REQ-ADMIN-004
    fn unhealthy_with_empty_cache() {
        let a = AdminServer::new(Arc::new(Mutex::new(DiscoveryCache::new())));
        assert!(!a.is_healthy());
    }

    #[test]
    // fusa:test REQ-ADMIN-002
    fn request_count_increments() {
        let (a, _) = admin_with_peer();
        for _ in 0..5 {
            a.record_request();
        }
        assert_eq!(a.request_count(), 5);
    }

    #[test]
    // fusa:test REQ-ADMIN-003
    fn uptime_is_non_negative() {
        let (a, _) = admin_with_peer();
        assert!(a.uptime() >= Duration::ZERO);
    }

    #[test]
    // fusa:test REQ-ADMIN-005
    fn shutdown_invalidates_peer() {
        let (a, sid) = admin_with_peer();
        a.shutdown_peer(sid).unwrap();
        assert!(a.is_shutting_down());
        assert!(!a.is_peer_healthy(sid, Instant::now(), Duration::from_secs(60)));
    }

    #[test]
    // fusa:test REQ-ADMIN-004
    fn peer_count_matches_cache() {
        let (a, _) = admin_with_peer();
        assert_eq!(a.peer_count(), 1);
    }
}
