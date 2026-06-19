// fusa:req REQ-FED-001
// fusa:req REQ-FED-002
// fusa:req REQ-FED-003
// fusa:req REQ-FED-004
// fusa:req REQ-FED-005

//! Multi-vehicle federation — routes commands to remote vehicle registries.
//!
//! A `FederationRouter` maps vehicle IDs to remote registries and dispatches
//! commands by prepending the vehicle-ID prefix to the zone.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::{RcpError, Registry, Zone};

// ── FederationRouter ──────────────────────────────────────────────────────────

/// Routes commands to one of several remote vehicle registries.
// fusa:req REQ-FED-001
pub struct FederationRouter {
    peers: RwLock<HashMap<String, Arc<dyn Registry>>>,
}

impl FederationRouter {
    pub fn new() -> Self {
        FederationRouter { peers: RwLock::new(HashMap::new()) }
    }

    /// Register a remote vehicle registry under `vehicle_id`.
    // fusa:req REQ-FED-002
    pub fn add_peer(&self, vehicle_id: impl Into<String>, registry: Arc<dyn Registry>) {
        self.peers.write().unwrap().insert(vehicle_id.into(), registry);
    }

    /// Remove a peer.
    // fusa:req REQ-FED-003
    pub fn remove_peer(&self, vehicle_id: &str) -> Option<Arc<dyn Registry>> {
        self.peers.write().unwrap().remove(vehicle_id)
    }

    /// List all registered vehicle IDs.
    // fusa:req REQ-FED-004
    pub fn peer_ids(&self) -> Vec<String> {
        self.peers.read().unwrap().keys().cloned().collect()
    }

    /// Look up a zone controller in a specific peer vehicle's registry.
    // fusa:req REQ-FED-005
    pub fn lookup_peer(&self, vehicle_id: &str, zone: Zone)
        -> Result<Arc<dyn crate::Controller>, RcpError>
    {
        let peers = self.peers.read().unwrap();
        let reg = peers.get(vehicle_id).ok_or(RcpError::NotFound)?;
        reg.lookup(zone)
    }
}

impl Default for FederationRouter {
    fn default() -> Self { Self::new() }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockRegistry;
    use crate::Zone;

    #[test]
    // fusa:test REQ-FED-001
    // fusa:test REQ-FED-002
    fn add_and_list_peers() {
        let r = FederationRouter::new();
        r.add_peer("VIN-001", Arc::new(MockRegistry::new()));
        r.add_peer("VIN-002", Arc::new(MockRegistry::new()));
        let mut ids = r.peer_ids();
        ids.sort();
        assert_eq!(ids, vec!["VIN-001", "VIN-002"]);
    }

    #[test]
    // fusa:test REQ-FED-003
    fn remove_peer() {
        let r = FederationRouter::new();
        r.add_peer("VIN-001", Arc::new(MockRegistry::new()));
        r.remove_peer("VIN-001");
        assert!(r.peer_ids().is_empty());
    }

    #[test]
    // fusa:test REQ-FED-005
    fn lookup_unknown_peer_returns_not_found() {
        let r = FederationRouter::new();
        let err = r.lookup_peer("VIN-999", Zone::FRONT_LEFT).unwrap_err();
        assert_eq!(err, RcpError::NotFound);
    }

    #[test]
    // fusa:test REQ-FED-005
    fn lookup_peer_zone() {
        let r = FederationRouter::new();
        r.add_peer("VIN-001", Arc::new(MockRegistry::new()));
        r.lookup_peer("VIN-001", Zone::FRONT_LEFT).unwrap();
    }

    #[test]
    // fusa:test REQ-FED-004
    fn peer_ids_empty_initially() {
        let r = FederationRouter::new();
        assert!(r.peer_ids().is_empty());
    }
}
