// fusa:req REQ-REC-001
// fusa:req REQ-REC-002
// fusa:req REQ-REC-003
// fusa:req REQ-REC-004
// fusa:req REQ-REC-005

//! Command/response recorder for replay, audit trails, and regression testing.

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use crate::{Command, Controller, RcpError, Response, Subscription, Zone};

// ── Record entry ──────────────────────────────────────────────────────────────

/// A single recorded interaction.
// fusa:req REQ-REC-001
#[derive(Clone, Debug)]
pub struct Entry {
    pub timestamp: SystemTime,
    pub command:   Command,
    pub result:    Result<Response, RcpError>,
}

// ── RecordController ──────────────────────────────────────────────────────────

/// Controller wrapper that records every command/response pair.
// fusa:req REQ-REC-002
pub struct RecordController {
    inner: Arc<dyn Controller>,
    log:   Mutex<Vec<Entry>>,
}

impl RecordController {
    pub fn new(inner: Arc<dyn Controller>) -> Self {
        RecordController { inner, log: Mutex::new(Vec::new()) }
    }

    /// All recorded entries in chronological order.
    // fusa:req REQ-REC-003
    pub fn entries(&self) -> Vec<Entry> {
        self.log.lock().unwrap().clone()
    }

    /// Clear the recorded log.
    // fusa:req REQ-REC-004
    pub fn clear(&self) {
        self.log.lock().unwrap().clear();
    }
}

impl Controller for RecordController {
    fn zone(&self) -> Zone { self.inner.zone() }

    // fusa:req REQ-REC-005
    fn send(&self, cmd: &Command, timeout: Option<Duration>) -> Result<Response, RcpError> {
        let result = self.inner.send(cmd, timeout);
        self.log.lock().unwrap().push(Entry {
            timestamp: SystemTime::now(),
            command:   cmd.clone(),
            result:    result.clone(),
        });
        result
    }

    fn subscribe(&self) -> Result<Subscription, RcpError> { self.inner.subscribe() }

    fn close(&self) -> Result<(), RcpError> { self.inner.close() }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockController;
    use crate::{Command, ResponseStatus, Zone};

    fn rec() -> RecordController {
        let inner = MockController::new(Zone::FRONT_LEFT, None) as Arc<dyn Controller>;
        RecordController::new(inner)
    }

    #[test]
    // fusa:test REQ-REC-002
    // fusa:test REQ-REC-005
    fn records_successful_sends() {
        let r = rec();
        for i in 1u32..=3 {
            r.send(&Command { id: i, zone: Zone::FRONT_LEFT, ..Default::default() }, None).unwrap();
        }
        let entries = r.entries();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].command.id, 1);
    }

    #[test]
    // fusa:test REQ-REC-005
    fn records_errors() {
        let inner = MockController::new(Zone::FRONT_LEFT, None) as Arc<dyn Controller>;
        inner.close().unwrap();
        let r = RecordController::new(inner);
        let _ = r.send(&Command { zone: Zone::FRONT_LEFT, ..Default::default() }, None);
        let entries = r.entries();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].result.is_err());
    }

    #[test]
    // fusa:test REQ-REC-004
    fn clear_empties_log() {
        let r = rec();
        r.send(&Command { zone: Zone::FRONT_LEFT, ..Default::default() }, None).unwrap();
        r.clear();
        assert!(r.entries().is_empty());
    }

    #[test]
    // fusa:test REQ-REC-001
    fn entry_timestamp_is_recent() {
        let r = rec();
        r.send(&Command { zone: Zone::FRONT_LEFT, ..Default::default() }, None).unwrap();
        let e = &r.entries()[0];
        let age = e.timestamp.elapsed().unwrap_or(Duration::ZERO);
        assert!(age < Duration::from_secs(5), "timestamp must be recent");
    }

    #[test]
    // fusa:test REQ-REC-003
    fn entries_in_order() {
        let r = rec();
        for i in 1u32..=5 {
            r.send(&Command { id: i, zone: Zone::FRONT_LEFT, ..Default::default() }, None).unwrap();
        }
        let ids: Vec<u32> = r.entries().iter().map(|e| e.command.id).collect();
        assert_eq!(ids, vec![1, 2, 3, 4, 5]);
    }
}
