// fusa:req REQ-MDNS-001
// fusa:req REQ-MDNS-002
// fusa:req REQ-MDNS-003
// fusa:req REQ-MDNS-004

//! mDNS/DNS-SD service discovery — an optional pre-discovery rendezvous
//! helper.
//!
//! Resolves RC Server hosts by name on the local network using mDNS.
//! Service type: `_rcp._tcp.local.`
//!
//! `ROADMAP.md` Milestone 9 ("All ADAPT-disposition packages retargeted...")
//! cutover: per this module's own ADAPT disposition ("retained as an
//! optional pre-discovery network-rendezvous helper... does not replace
//! the mandatory spec discovery mechanism... Milestone 3"),
//! [`MdnsRegistry`] itself was never bound to the legacy `Controller`/
//! `Registry` traits (it is a same-named-but-unrelated host/port/txt-record
//! store, not an `impl` of either), so this item's only change is
//! [`ServiceRecord::stream_id`], replacing the old `zone: u8` field — the
//! one remaining piece of legacy `Zone`-shaped addressing in this module —
//! with a [`crate::avtp::StreamId`], matching every other ADAPT package's
//! address-key retarget in this bullet. Real mDNS lookups return only a
//! host/port to dial; the discovered peer's actual `StreamId` is learned
//! afterward via the real spec discovery exchange
//! ([`crate::discovery::build_discovery_request`]/
//! [`crate::discovery::is_discovery_request`]) run over that host/port —
//! this module does not itself decide what a resolved `ServiceRecord`'s
//! `stream_id` should be before that exchange completes; callers that
//! pre-seed it (as this module's own tests do) are doing so with a value
//! they already independently know or expect.

use std::collections::HashMap;
use std::sync::RwLock;

use crate::avtp::StreamId;

// ── ServiceRecord ─────────────────────────────────────────────────────────────

/// A discovered mDNS service record.
// fusa:req REQ-MDNS-001
#[derive(Debug, Clone)]
pub struct ServiceRecord {
    pub host: String,
    pub port: u16,
    pub stream_id: StreamId,
    pub txt: HashMap<String, String>,
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
        MdnsRegistry {
            records: RwLock::new(HashMap::new()),
        }
    }

    /// Announce a service (called by a server on startup).
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

    fn stream(unique_id: u16) -> StreamId {
        StreamId::new([0x02, 0x00, 0x00, 0x00, 0x00, 0x01], unique_id)
    }

    fn record(unique_id: u16) -> ServiceRecord {
        ServiceRecord {
            host: "vehicle.local".into(),
            port: 9000,
            stream_id: stream(unique_id),
            txt: HashMap::new(),
        }
    }

    #[test]
    // fusa:test REQ-MDNS-003
    // fusa:test REQ-MDNS-004
    fn announce_and_resolve() {
        let r = MdnsRegistry::new();
        r.announce("fl-svr._rcp._tcp.local.", record(1));
        let rec = r.resolve("fl-svr._rcp._tcp.local.").unwrap();
        assert_eq!(rec.stream_id, stream(1));
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
        let rec = ServiceRecord {
            host: "h".into(),
            port: 80,
            stream_id: stream(3),
            txt: HashMap::new(),
        };
        assert_eq!(rec.stream_id, stream(3));
        assert_eq!(rec.port, 80);
    }
}
