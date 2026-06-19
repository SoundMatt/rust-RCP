// fusa:req REQ-MDNS-001
// fusa:req REQ-MDNS-002
// fusa:req REQ-MDNS-003
// fusa:req REQ-MDNS-004

//! mDNS/DNS-SD service discovery for zone controllers.
//!
//! Resolves RCP zone controllers by name on the local network using mDNS.
//! Service type: `_rcp._tcp.local.`

use std::collections::HashMap;
use std::sync::RwLock;

// ── ServiceRecord ─────────────────────────────────────────────────────────────

/// A discovered mDNS service record.
// fusa:req REQ-MDNS-001
#[derive(Debug, Clone)]
pub struct ServiceRecord {
    pub host:     String,
    pub port:     u16,
    pub zone:     u8,
    pub txt:      HashMap<String, String>,
}

// ── MdnsRegistry ─────────────────────────────────────────────────────────────

/// In-process mDNS registry for testing. Production implementations
/// integrate with OS mDNS APIs via the same interface.
// fusa:req REQ-MDNS-002
pub struct MdnsRegistry {
    records: RwLock<HashMap<String, ServiceRecord>>,
}

impl MdnsRegistry {
    pub fn new() -> Self {
        MdnsRegistry { records: RwLock::new(HashMap::new()) }
    }

    /// Announce a service (called by a controller on startup).
    // fusa:req REQ-MDNS-003
    pub fn announce(&self, name: impl Into<String>, record: ServiceRecord) {
        self.records.write().unwrap().insert(name.into(), record);
    }

    /// Withdraw a service announcement.
    pub fn withdraw(&self, name: &str) {
        self.records.write().unwrap().remove(name);
    }

    /// Resolve a service name to its record.
    // fusa:req REQ-MDNS-004
    pub fn resolve(&self, name: &str) -> Option<ServiceRecord> {
        self.records.read().unwrap().get(name).cloned()
    }

    /// All currently announced service names.
    pub fn names(&self) -> Vec<String> {
        self.records.read().unwrap().keys().cloned().collect()
    }
}

impl Default for MdnsRegistry {
    fn default() -> Self { Self::new() }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    fn record(zone: u8) -> ServiceRecord {
        ServiceRecord { host: "vehicle.local".into(), port: 9000, zone, txt: HashMap::new() }
    }

    #[test]
    // fusa:test REQ-MDNS-003
    // fusa:test REQ-MDNS-004
    fn announce_and_resolve() {
        let r = MdnsRegistry::new();
        r.announce("fl-ctrl._rcp._tcp.local.", record(1));
        let rec = r.resolve("fl-ctrl._rcp._tcp.local.").unwrap();
        assert_eq!(rec.zone, 1);
    }

    #[test]
    // fusa:test REQ-MDNS-004
    fn resolve_unknown_returns_none() {
        assert!(MdnsRegistry::new().resolve("unknown").is_none());
    }

    #[test]
    // fusa:test REQ-MDNS-002
    fn withdraw_removes_record() {
        let r = MdnsRegistry::new();
        r.announce("svc", record(2));
        r.withdraw("svc");
        assert!(r.resolve("svc").is_none());
    }

    #[test]
    // fusa:test REQ-MDNS-001
    fn service_record_fields() {
        let rec = ServiceRecord { host: "h".into(), port: 80, zone: 3, txt: HashMap::new() };
        assert_eq!(rec.zone, 3);
        assert_eq!(rec.port, 80);
    }
}
