// fusa:req REQ-FED-001
// fusa:req REQ-FED-002
// fusa:req REQ-FED-003
// fusa:req REQ-FED-004
// fusa:req REQ-FED-005

//! Multi-vehicle federation — routes lookups to remote vehicles' own
//! discovery caches.
//!
//! A [`FederationRouter`] maps vehicle IDs to remote
//! [`crate::discovery::DiscoveryCache`]s.
//!
//! `ROADMAP.md` Milestone 9 ("All ADAPT-disposition packages retargeted...")
//! cutover: per this module's own ADAPT disposition ("multi-vehicle
//! routing-by-name concept can be rebuilt once a discovery-derived server
//! registry exists (Milestone 3) — has a real dependency, so lands after
//! core discovery, not before"), [`FederationRouter`] is rebuilt against
//! [`crate::discovery::DiscoveryCache`] — the "discovery-derived server
//! registry" that dependency names, added by Milestone 3 — in place of the
//! deleted `Registry` trait. This is a narrower capability than the old
//! type had: the legacy `Registry::lookup` returned a live, dispatchable
//! `Arc<dyn Controller>` handle; `DiscoveryCache::lookup` returns only a
//! `Copy` snapshot of a discovered peer's identity
//! ([`crate::discovery::DiscoveryCacheEntry`]), since a client-side cache
//! is all `discovery.rs` itself models — there is no live "registry of
//! dispatch handles" anywhere in the new core to look one up from. Flagged
//! here per Guiding Principle 5 rather than silently narrowed:
//! [`Self::lookup_peer`]'s signature and return type both changed to match.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use crate::avtp::StreamId;
use crate::discovery::DiscoveryCache;
use crate::discovery::DiscoveryCacheEntry;
use crate::RcpError;

// ── FederationRouter ──────────────────────────────────────────────────────────

/// Routes lookups to one of several remote vehicles' own discovery caches.
// fusa:req REQ-FED-001
pub struct FederationRouter {
    peers: RwLock<HashMap<String, Arc<Mutex<DiscoveryCache>>>>,
}

impl FederationRouter {
    pub fn new() -> Self {
        FederationRouter {
            peers: RwLock::new(HashMap::new()),
        }
    }

    /// Register a remote vehicle's discovery cache under `vehicle_id`.
    // fusa:req REQ-FED-002
    pub fn add_peer(&self, vehicle_id: impl Into<String>, cache: Arc<Mutex<DiscoveryCache>>) {
        self.peers.write().unwrap().insert(vehicle_id.into(), cache);
    }

    /// Remove a peer.
    // fusa:req REQ-FED-003
    pub fn remove_peer(&self, vehicle_id: &str) -> Option<Arc<Mutex<DiscoveryCache>>> {
        self.peers.write().unwrap().remove(vehicle_id)
    }

    /// List all registered vehicle IDs.
    // fusa:req REQ-FED-004
    pub fn peer_ids(&self) -> Vec<String> {
        self.peers.read().unwrap().keys().cloned().collect()
    }

    /// Look up a discovered server's cached identity, by `stream_id`, in a
    /// specific peer vehicle's discovery cache.
    ///
    /// Returns `Err(RcpError::NotFound)` if `vehicle_id` names no
    /// registered peer, or if `stream_id` has no cached entry in that
    /// peer's cache.
    // fusa:req REQ-FED-005
    pub fn lookup_peer(
        &self,
        vehicle_id: &str,
        stream_id: StreamId,
    ) -> Result<DiscoveryCacheEntry, RcpError> {
        let cache = {
            let peers = self.peers.read().unwrap();
            let cache = peers.get(vehicle_id).ok_or(RcpError::NotFound)?;
            Arc::clone(cache)
        };
        let guard = cache.lock().unwrap();
        guard.lookup(stream_id).copied().ok_or(RcpError::NotFound)
    }
}

impl Default for FederationRouter {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::regmap::GeneralRegisters;
    use std::time::Instant;

    fn stream(unique_id: u16) -> StreamId {
        StreamId::new([0x02, 0x00, 0x00, 0x00, 0x00, 0x01], unique_id)
    }

    fn cache_with_entry(stream_id: StreamId) -> Arc<Mutex<DiscoveryCache>> {
        let mut cache = DiscoveryCache::new();
        cache
            .remember(stream_id, &GeneralRegisters::default(), Instant::now())
            .unwrap();
        Arc::new(Mutex::new(cache))
    }

    #[test]
    // fusa:test REQ-FED-001
    // fusa:test REQ-FED-002
    fn add_and_list_peers() {
        let r = FederationRouter::new();
        r.add_peer("VIN-001", Arc::new(Mutex::new(DiscoveryCache::new())));
        r.add_peer("VIN-002", Arc::new(Mutex::new(DiscoveryCache::new())));
        let mut ids = r.peer_ids();
        ids.sort();
        assert_eq!(ids, vec!["VIN-001", "VIN-002"]);
    }

    #[test]
    // fusa:test REQ-FED-003
    fn remove_peer() {
        let r = FederationRouter::new();
        r.add_peer("VIN-001", Arc::new(Mutex::new(DiscoveryCache::new())));
        r.remove_peer("VIN-001");
        assert!(r.peer_ids().is_empty());
    }

    #[test]
    // fusa:test REQ-FED-005
    fn lookup_unknown_peer_returns_not_found() {
        let r = FederationRouter::new();
        let err = r.lookup_peer("VIN-999", stream(1)).err().unwrap();
        assert_eq!(err, RcpError::NotFound);
    }

    #[test]
    // fusa:test REQ-FED-005
    fn lookup_unknown_stream_in_known_peer_returns_not_found() {
        let r = FederationRouter::new();
        r.add_peer("VIN-001", Arc::new(Mutex::new(DiscoveryCache::new())));
        let err = r.lookup_peer("VIN-001", stream(1)).err().unwrap();
        assert_eq!(err, RcpError::NotFound);
    }

    #[test]
    // fusa:test REQ-FED-005
    fn lookup_peer_stream_returns_cached_entry() {
        let r = FederationRouter::new();
        let sid = stream(7);
        r.add_peer("VIN-001", cache_with_entry(sid));
        let entry = r.lookup_peer("VIN-001", sid).unwrap();
        assert_eq!(
            entry.svr_ep_count(),
            GeneralRegisters::default().svr_ep_count
        );
    }

    #[test]
    // fusa:test REQ-FED-004
    fn peer_ids_empty_initially() {
        let r = FederationRouter::new();
        assert!(r.peer_ids().is_empty());
    }
}
