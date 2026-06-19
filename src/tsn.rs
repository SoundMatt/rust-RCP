// fusa:req REQ-TSN-001
// fusa:req REQ-TSN-002
// fusa:req REQ-TSN-003
// fusa:req REQ-TSN-004
// fusa:req REQ-TSN-005

//! TSN (Time-Sensitive Networking) credit shaper and traffic class tagging.
//!
//! Adds IEEE 802.1Qav credit-based shaper metadata to outgoing commands so
//! that the underlying transport can schedule them in the appropriate traffic
//! class.

use std::sync::Arc;
use std::time::Duration;

use crate::{Command, Controller, Priority, RcpError, Response, Subscription, Zone};

// ── Traffic class mapping ─────────────────────────────────────────────────────

/// IEEE 802.1Q traffic class (0 = best effort, 7 = highest).
// fusa:req REQ-TSN-001
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TrafficClass(pub u8);

impl TrafficClass {
    pub const BEST_EFFORT: TrafficClass = TrafficClass(0);
    pub const CONTROL:     TrafficClass = TrafficClass(5);
    pub const CRITICAL:    TrafficClass = TrafficClass(7);

    /// Map an RCP [`Priority`] to the corresponding TSN traffic class.
    // fusa:req REQ-TSN-002
    pub fn from_priority(p: Priority) -> Self {
        match p {
            Priority::CRITICAL => TrafficClass::CRITICAL,
            Priority::HIGH     => TrafficClass::CONTROL,
            _                  => TrafficClass::BEST_EFFORT,
        }
    }
}

// ── TSN Controller ────────────────────────────────────────────────────────────

/// Wraps an inner controller, stamping TSN traffic-class metadata onto payloads.
///
/// The traffic class is prepended as a single byte to the payload.
// fusa:req REQ-TSN-003
pub struct TsnController {
    inner: Arc<dyn Controller>,
}

impl TsnController {
    pub fn new(inner: Arc<dyn Controller>) -> Self {
        TsnController { inner }
    }
}

impl Controller for TsnController {
    fn zone(&self) -> Zone { self.inner.zone() }

    // fusa:req REQ-TSN-004
    fn send(&self, cmd: &Command, timeout: Option<Duration>) -> Result<Response, RcpError> {
        let tc = TrafficClass::from_priority(cmd.priority);
        let raw = cmd.payload.as_deref().unwrap_or(&[]);
        let mut tagged = Vec::with_capacity(1 + raw.len());
        tagged.push(tc.0);
        tagged.extend_from_slice(raw);
        let mut tagged_cmd = cmd.clone();
        tagged_cmd.payload = Some(tagged);
        self.inner.send(&tagged_cmd, timeout)
    }

    fn subscribe(&self) -> Result<Subscription, RcpError> { self.inner.subscribe() }

    // fusa:req REQ-TSN-005
    fn close(&self) -> Result<(), RcpError> { self.inner.close() }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockController;
    use crate::{Command, Priority, Response, ResponseStatus, Zone};

    #[test]
    // fusa:test REQ-TSN-001
    // fusa:test REQ-TSN-002
    fn traffic_class_mapping() {
        assert_eq!(TrafficClass::from_priority(Priority::NORMAL),   TrafficClass::BEST_EFFORT);
        assert_eq!(TrafficClass::from_priority(Priority::HIGH),     TrafficClass::CONTROL);
        assert_eq!(TrafficClass::from_priority(Priority::CRITICAL), TrafficClass::CRITICAL);
    }

    #[test]
    // fusa:test REQ-TSN-004
    fn tsn_prepends_traffic_class_byte() {
        let received = Arc::new(std::sync::Mutex::new(vec![]));
        let r2 = Arc::clone(&received);
        let h: crate::mock::Handler = Box::new(move |cmd| {
            r2.lock().unwrap().push(cmd.payload.clone().unwrap_or_default());
            Response { command_id: cmd.id, zone: cmd.zone, status: ResponseStatus::OK, payload: None }
        });
        let inner = MockController::new(Zone::FRONT_LEFT, Some(h)) as Arc<dyn Controller>;
        let tsn = TsnController::new(inner);

        tsn.send(&Command { zone: Zone::FRONT_LEFT, priority: Priority::CRITICAL, ..Default::default() }, None).unwrap();
        let payloads = received.lock().unwrap();
        assert_eq!(payloads[0][0], TrafficClass::CRITICAL.0);
    }

    #[test]
    // fusa:test REQ-TSN-003
    fn zone_forwarded() {
        let inner = MockController::new(Zone::REAR_RIGHT, None) as Arc<dyn Controller>;
        let tsn = TsnController::new(inner);
        assert_eq!(tsn.zone(), Zone::REAR_RIGHT);
    }

    #[test]
    // fusa:test REQ-TSN-005
    fn close_forwarded() {
        let inner = MockController::new(Zone::FRONT_LEFT, None) as Arc<dyn Controller>;
        TsnController::new(inner).close().unwrap();
    }
}
