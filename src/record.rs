// fusa:req REQ-REC-001
// fusa:req REQ-REC-002
// fusa:req REQ-REC-003
// fusa:req REQ-REC-004
// fusa:req REQ-REC-005

//! Call recorder for replay, audit trails, and regression testing, wrapping
//! an [`Endpoint`].
//!
//! `ROADMAP.md` Milestone 9 ("All ADAPT-disposition packages retargeted...")
//! cutover: per this module's own ADAPT disposition ("generic audit-log
//! decorator, protocol-agnostic"), [`RecordEndpoint`] replaces the legacy
//! `RecordController`, wrapping [`crate::mock::Endpoint`] instead of
//! `Controller`. The old single `Entry { command, result }` shape, built
//! around one `Command`/`Response` pair, has no single-shape analog now
//! that the base trait has two distinct verbs with two distinct result
//! types (`Vec<u8>` vs `()`) — [`Entry`] becomes an enum with a `Read`/
//! `Write` variant instead, the same split [`crate::observe`]'s own
//! retargeting note already flagged for the identical reason.

use std::sync::Arc;
use std::sync::Mutex;
use std::time::SystemTime;

use crate::mock::Endpoint;
use crate::regmap::EndpointType;
use crate::RcpError;

// ── Record entry ──────────────────────────────────────────────────────────────

/// A single recorded interaction.
// fusa:req REQ-REC-001
#[derive(Clone, Debug)]
pub enum Entry {
    Read {
        timestamp: SystemTime,
        read_size: u8,
        result: Result<Vec<u8>, RcpError>,
    },
    Write {
        timestamp: SystemTime,
        payload: Vec<u8>,
        result: Result<(), RcpError>,
    },
}

impl Entry {
    /// The timestamp common to either variant.
    pub fn timestamp(&self) -> SystemTime {
        match self {
            Entry::Read { timestamp, .. } => *timestamp,
            Entry::Write { timestamp, .. } => *timestamp,
        }
    }

    /// True if this entry's recorded result was an error.
    pub fn is_err(&self) -> bool {
        match self {
            Entry::Read { result, .. } => result.is_err(),
            Entry::Write { result, .. } => result.is_err(),
        }
    }
}

// ── RecordEndpoint ─────────────────────────────────────────────────────────────

/// Endpoint wrapper that records every read/write call and its result.
// fusa:req REQ-REC-002
pub struct RecordEndpoint {
    inner: Arc<dyn Endpoint>,
    log: Mutex<Vec<Entry>>,
}

impl RecordEndpoint {
    pub fn new(inner: Arc<dyn Endpoint>) -> Self {
        RecordEndpoint {
            inner,
            log: Mutex::new(Vec::new()),
        }
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

impl Endpoint for RecordEndpoint {
    fn ep_type(&self) -> EndpointType {
        self.inner.ep_type()
    }

    // fusa:req REQ-REC-005
    fn read(&self, read_size: u8) -> Result<Vec<u8>, RcpError> {
        let result = self.inner.read(read_size);
        self.log.lock().unwrap().push(Entry::Read {
            timestamp: SystemTime::now(),
            read_size,
            result: result.clone(),
        });
        result
    }

    // fusa:req REQ-REC-005
    fn write(&self, payload: &[u8]) -> Result<(), RcpError> {
        let result = self.inner.write(payload);
        self.log.lock().unwrap().push(Entry::Write {
            timestamp: SystemTime::now(),
            payload: payload.to_vec(),
            result: result.clone(),
        });
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
    use std::time::Duration;

    fn rec() -> RecordEndpoint {
        let inner = MockEndpoint::new(EndpointType::Gpio, vec![0u8; 4]) as Arc<dyn Endpoint>;
        RecordEndpoint::new(inner)
    }

    #[test]
    // fusa:test REQ-REC-002
    // fusa:test REQ-REC-005
    fn records_successful_writes() {
        let r = rec();
        for i in 1u8..=3 {
            r.write(&[i]).unwrap();
        }
        let entries = r.entries();
        assert_eq!(entries.len(), 3);
        match &entries[0] {
            Entry::Write { payload, .. } => assert_eq!(payload, &vec![1u8]),
            _ => panic!("expected Write entry"),
        }
    }

    #[test]
    // fusa:test REQ-REC-005
    fn records_errors() {
        struct AlwaysFail;
        impl Endpoint for AlwaysFail {
            fn ep_type(&self) -> EndpointType {
                EndpointType::Gpio
            }
            fn read(&self, _read_size: u8) -> Result<Vec<u8>, RcpError> {
                Err(RcpError::Closed)
            }
            fn write(&self, _payload: &[u8]) -> Result<(), RcpError> {
                Err(RcpError::Closed)
            }
        }
        let r = RecordEndpoint::new(Arc::new(AlwaysFail) as Arc<dyn Endpoint>);
        let _ = r.write(b"x");
        let entries = r.entries();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_err());
    }

    #[test]
    // fusa:test REQ-REC-004
    fn clear_empties_log() {
        let r = rec();
        r.write(b"x").unwrap();
        r.clear();
        assert!(r.entries().is_empty());
    }

    #[test]
    // fusa:test REQ-REC-001
    fn entry_timestamp_is_recent() {
        let r = rec();
        r.write(b"x").unwrap();
        let e = &r.entries()[0];
        let age = e.timestamp().elapsed().unwrap_or(Duration::ZERO);
        assert!(age < Duration::from_secs(5), "timestamp must be recent");
    }

    #[test]
    // fusa:test REQ-REC-003
    fn entries_in_order() {
        let r = rec();
        for i in 1u8..=5 {
            r.write(&[i]).unwrap();
        }
        let payloads: Vec<Vec<u8>> = r
            .entries()
            .iter()
            .map(|e| match e {
                Entry::Write { payload, .. } => payload.clone(),
                _ => panic!("expected Write entry"),
            })
            .collect();
        assert_eq!(payloads, vec![vec![1], vec![2], vec![3], vec![4], vec![5]]);
    }
}
