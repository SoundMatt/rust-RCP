// fusa:req REQ-CTRL-001
// fusa:req REQ-CTRL-002
// fusa:req REQ-CTRL-003
// fusa:req REQ-CTRL-004
// fusa:req REQ-CTRL-005
// fusa:req REQ-CTRL-006
// fusa:req REQ-CTRL-007
// fusa:req REQ-CTRL-008
// fusa:req REQ-CTRL-009
// fusa:req REQ-CTRL-010
// fusa:req REQ-CTRL-011
// fusa:req REQ-CTRL-012
// fusa:req REQ-CTRL-013
// fusa:req REQ-CTRL-014
// fusa:req REQ-CTRL-015
// fusa:req REQ-CTRL-016
// fusa:req REQ-CTRL-017
// fusa:req REQ-CTRL-018
// fusa:req REQ-CTRL-019
// fusa:req REQ-CTRL-020
// fusa:req REQ-CTRL-021
// fusa:req REQ-CTRL-022
// fusa:req REQ-CTRL-023
// fusa:req REQ-CTRL-024
// fusa:req REQ-CTRL-025
// fusa:req REQ-CTRL-026
// fusa:req REQ-CTRL-027
// fusa:req REQ-REG-001
// fusa:req REQ-REG-002
// fusa:req REQ-REG-003
// fusa:req REQ-REG-004
// fusa:req REQ-REG-005
// fusa:req REQ-REG-006
// fusa:req REQ-REG-007
// fusa:req REQ-REG-008
// fusa:req REQ-REG-009
// fusa:req REQ-REG-010
// fusa:req REQ-REG-011
// fusa:req REQ-REG-012
// fusa:req REQ-REG-013
// fusa:req REQ-RESP-001
// fusa:req REQ-RESP-002
// fusa:req REQ-STAT-001
// fusa:req REQ-STAT-002
// fusa:req REQ-STAT-003
// fusa:req REQ-STAT-004
// fusa:req REQ-STAT-005
// fusa:req REQ-ERR-011

//! In-process mock [`Controller`] and [`Registry`] for unit tests.
//!
//! All operations execute synchronously in memory with no network I/O.
//! The mock is safe for concurrent use.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use crate::{
    Command, Controller, RcpError, Registry, Response, ResponseStatus, Status, Subscription, Zone,
};

// ── Subscription inner state ──────────────────────────────────────────────────

struct SubEntry {
    id: u64,
    tx: mpsc::SyncSender<Arc<Status>>,
}

struct Inner {
    subs: Vec<SubEntry>,
    next_sub_id: u64,
}

// ── Handler type ──────────────────────────────────────────────────────────────

/// User-supplied function that produces a [`Response`] for a [`Command`].
pub type Handler = Box<dyn Fn(&Command) -> Response + Send + Sync>;

// ── Controller ────────────────────────────────────────────────────────────────

/// Mock zone controller — in-process, zero-dependency, race-free.
pub struct MockController {
    zone: Zone,
    handler: Option<Handler>,
    closed: AtomicBool,
    seq: AtomicU32,
    inner: Arc<Mutex<Inner>>,
    #[allow(dead_code)]
    next_id: AtomicU64,
}

impl MockController {
    /// Create a mock controller. If `handler` is `None` every [`Command`] returns `StatusOK`.
    pub fn new(zone: Zone, handler: Option<Handler>) -> Arc<Self> {
        Arc::new(Self {
            zone,
            handler,
            closed: AtomicBool::new(false),
            seq: AtomicU32::new(0),
            inner: Arc::new(Mutex::new(Inner {
                subs: Vec::new(),
                next_sub_id: 0,
            })),
            next_id: AtomicU64::new(0),
        })
    }

    /// Push a [`Status`] to all active subscribers.
    // fusa:req REQ-CTRL-006
    // fusa:req REQ-CTRL-017
    pub fn publish(&self, payload: Option<Vec<u8>>) {
        if self.closed.load(Ordering::SeqCst) {
            return; // silent no-op after close
        }
        let seq = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        // Copy payload so caller mutation after publish cannot affect delivered Status.
        // fusa:req REQ-CTRL-027
        let p = payload.clone();
        let st = Arc::new(Status {
            zone: self.zone,
            seq,
            healthy: !self.closed.load(Ordering::SeqCst),
            payload: p,
        });
        let mut inner = self.inner.lock().unwrap();
        inner
            .subs
            .retain(|e| e.tx.try_send(Arc::clone(&st)).is_ok());
    }
}

impl Controller for MockController {
    fn zone(&self) -> Zone {
        self.zone
    }

    fn send(&self, cmd: &Command, timeout: Option<Duration>) -> Result<Response, RcpError> {
        // fusa:req REQ-CTRL-003
        if self.closed.load(Ordering::SeqCst) {
            return Err(RcpError::Closed);
        }
        // fusa:req REQ-CTRL-004 / REQ-CTRL-023: zero timeout = already-expired context
        if timeout == Some(Duration::ZERO) {
            return Err(RcpError::Timeout);
        }
        // fusa:req REQ-CTRL-025
        if cmd.zone != self.zone {
            return Err(RcpError::ZoneMismatch);
        }
        // fusa:req REQ-CTRL-026: copy payload before handler
        let mut safe = cmd.clone();
        safe.payload = cmd.payload.clone();

        if let Some(h) = &self.handler {
            // fusa:req REQ-CTRL-002 / REQ-CTRL-016
            Ok(h(&safe))
        } else {
            // fusa:req REQ-CTRL-001
            Ok(Response {
                command_id: cmd.id,
                zone: self.zone,
                status: ResponseStatus::OK,
                payload: None,
            })
        }
    }

    fn subscribe(&self) -> Result<Subscription, RcpError> {
        // fusa:req REQ-CTRL-008
        if self.closed.load(Ordering::SeqCst) {
            return Err(RcpError::Closed);
        }
        let (tx, rx) = mpsc::sync_channel(16);
        let id = {
            let mut inner = self.inner.lock().unwrap();
            let id = inner.next_sub_id;
            inner.next_sub_id += 1;
            inner.subs.push(SubEntry { id, tx });
            id
        };
        // Wrap receiver with cleanup so dropping the Subscription removes the sender.
        // fusa:req REQ-CTRL-011
        let inner_clone = Arc::clone(&self.inner);
        let rx = SubReceiver {
            rx,
            id,
            inner: inner_clone,
        };
        Ok(Subscription {
            rx: rx.into_std_receiver(),
        })
    }

    fn close(&self) -> Result<(), RcpError> {
        // fusa:req REQ-CTRL-005: idempotent
        if self.closed.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        // fusa:req REQ-CTRL-007: close all subscriber channels
        let mut inner = self.inner.lock().unwrap();
        inner.subs.clear(); // dropping senders closes receivers
        Ok(())
    }
}

// Helper: wraps the raw receiver so Drop cleans up the subscription entry.
struct SubReceiver {
    rx: mpsc::Receiver<Arc<Status>>,
    id: u64,
    inner: Arc<Mutex<Inner>>,
}

impl SubReceiver {
    fn into_std_receiver(self) -> mpsc::Receiver<Arc<Status>> {
        // We spawn a bridge to handle cleanup on drop via a wrapper channel.
        // For simplicity use a passthrough: create a forwarding channel.
        let (bridge_tx, bridge_rx) = mpsc::sync_channel::<Arc<Status>>(16);
        let rx = self.rx;
        let id = self.id;
        let inner = Arc::clone(&self.inner);
        std::thread::spawn(move || {
            // Forward until original sender closes.
            while let Ok(st) = rx.recv() {
                if bridge_tx.send(st).is_err() {
                    break;
                }
            }
            // Cleanup: remove subscription entry.
            let mut lock = inner.lock().unwrap();
            lock.subs.retain(|e| e.id != id);
        });
        bridge_rx
    }
}

// ── Registry ──────────────────────────────────────────────────────────────────

struct RegistryInner {
    controllers: std::collections::HashMap<Zone, Arc<dyn Controller>>,
    closed: bool,
}

/// In-process registry backed by mock controllers.
pub struct MockRegistry {
    inner: Mutex<RegistryInner>,
}

impl MockRegistry {
    /// Create a registry pre-populated with mock controllers for all five standard zones.
    // fusa:req REQ-REG-001
    pub fn new() -> Arc<Self> {
        let mut map = std::collections::HashMap::new();
        for z in [
            Zone::FRONT_LEFT,
            Zone::FRONT_RIGHT,
            Zone::REAR_LEFT,
            Zone::REAR_RIGHT,
            Zone::CENTRAL,
        ] {
            map.insert(z, MockController::new(z, None) as Arc<dyn Controller>);
        }
        Arc::new(Self {
            inner: Mutex::new(RegistryInner {
                controllers: map,
                closed: false,
            }),
        })
    }
}

impl Default for MockRegistry {
    fn default() -> Self {
        let mut map = std::collections::HashMap::new();
        for z in [
            Zone::FRONT_LEFT,
            Zone::FRONT_RIGHT,
            Zone::REAR_LEFT,
            Zone::REAR_RIGHT,
            Zone::CENTRAL,
        ] {
            map.insert(z, MockController::new(z, None) as Arc<dyn Controller>);
        }
        Self {
            inner: Mutex::new(RegistryInner {
                controllers: map,
                closed: false,
            }),
        }
    }
}

impl Registry for MockRegistry {
    fn register(&self, ctrl: Arc<dyn Controller>) -> Result<(), RcpError> {
        let mut inner = self.inner.lock().unwrap();
        // fusa:req REQ-REG-007
        if inner.closed {
            return Err(RcpError::Closed);
        }
        // fusa:req REQ-REG-002
        if inner.controllers.contains_key(&ctrl.zone()) {
            return Err(RcpError::AlreadyExists);
        }
        inner.controllers.insert(ctrl.zone(), ctrl);
        Ok(())
    }

    fn deregister(&self, zone: Zone) -> Result<(), RcpError> {
        let mut inner = self.inner.lock().unwrap();
        // fusa:req REQ-REG-004 / REQ-REG-008
        let ctrl = inner.controllers.remove(&zone).ok_or(RcpError::NotFound)?;
        let _ = ctrl.close();
        Ok(())
    }

    fn lookup(&self, zone: Zone) -> Result<Arc<dyn Controller>, RcpError> {
        let inner = self.inner.lock().unwrap();
        // fusa:req REQ-REG-013: return ErrClosed (not ErrNotFound) if registry is closed
        if inner.closed {
            return Err(RcpError::Closed);
        }
        // fusa:req REQ-REG-004 / REQ-REG-011
        inner
            .controllers
            .get(&zone)
            .cloned()
            .ok_or(RcpError::NotFound)
    }

    fn controllers(&self) -> Vec<Arc<dyn Controller>> {
        // fusa:req REQ-REG-006
        let inner = self.inner.lock().unwrap();
        inner.controllers.values().cloned().collect()
    }

    fn close(&self) -> Result<(), RcpError> {
        // fusa:req REQ-REG-005: idempotent
        let mut inner = self.inner.lock().unwrap();
        if inner.closed {
            return Ok(());
        }
        inner.closed = true;
        // fusa:req REQ-REG-010
        for ctrl in inner.controllers.values() {
            let _ = ctrl.close();
        }
        inner.controllers.clear();
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::CommandType;
    use std::sync::atomic::Ordering as AO;
    use std::time::Duration;

    // ── Controller.Zone ───────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-CTRL-009
    fn controller_zone_returns_declared_zone() {
        let c = MockController::new(Zone::FRONT_LEFT, None);
        assert_eq!(c.zone(), Zone::FRONT_LEFT);
    }

    // ── Controller.Send ───────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-CTRL-001
    fn send_no_handler_returns_status_ok() {
        let c = MockController::new(Zone::FRONT_LEFT, None);
        let cmd = Command {
            id: 1,
            zone: Zone::FRONT_LEFT,
            ..Default::default()
        };
        let resp = c.send(&cmd, None).unwrap();
        assert_eq!(resp.status, ResponseStatus::OK);
    }

    #[test]
    // fusa:test REQ-CTRL-002
    fn send_dispatches_to_handler() {
        let called = Arc::new(AtomicBool::new(false));
        let called2 = Arc::clone(&called);
        let h: Handler = Box::new(move |cmd| {
            called2.store(true, AO::SeqCst);
            Response {
                command_id: cmd.id,
                zone: cmd.zone,
                status: ResponseStatus::OK,
                payload: None,
            }
        });
        let c = MockController::new(Zone::FRONT_LEFT, Some(h));
        let cmd = Command {
            id: 7,
            zone: Zone::FRONT_LEFT,
            ..Default::default()
        };
        let _ = c.send(&cmd, None).unwrap();
        assert!(called.load(AO::SeqCst));
    }

    #[test]
    // fusa:test REQ-CTRL-003
    fn send_after_close_returns_err_closed() {
        let c = MockController::new(Zone::FRONT_LEFT, None);
        c.close().unwrap();
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            ..Default::default()
        };
        let err = c.send(&cmd, None).unwrap_err();
        assert_eq!(err, RcpError::Closed);
        assert!(err.is_relay_closed());
    }

    #[test]
    // fusa:test REQ-CTRL-004
    // fusa:test REQ-CTRL-023
    fn send_zero_timeout_returns_err_timeout_without_invoking_handler() {
        let called = Arc::new(AtomicBool::new(false));
        let called2 = Arc::clone(&called);
        let h: Handler = Box::new(move |_| {
            called2.store(true, AO::SeqCst);
            Response::default()
        });
        let c = MockController::new(Zone::FRONT_LEFT, Some(h));
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            ..Default::default()
        };
        let err = c.send(&cmd, Some(Duration::ZERO)).unwrap_err();
        assert_eq!(err, RcpError::Timeout);
        assert!(err.is_relay_timeout());
        assert!(!called.load(AO::SeqCst), "handler must not be invoked");
    }

    #[test]
    // fusa:test REQ-CTRL-005
    fn close_is_idempotent() {
        let c = MockController::new(Zone::FRONT_LEFT, None);
        assert!(c.close().is_ok());
        assert!(c.close().is_ok());
        assert!(c.close().is_ok());
    }

    #[test]
    // fusa:test REQ-CTRL-013
    fn cmd_noop_is_accepted_without_error() {
        let c = MockController::new(Zone::FRONT_LEFT, None);
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            cmd_type: CommandType::NOOP,
            ..Default::default()
        };
        let r = c.send(&cmd, None).unwrap();
        assert_eq!(r.status, ResponseStatus::OK);
    }

    #[test]
    // fusa:test REQ-CTRL-014
    fn cmd_watchdog_is_accepted_without_error() {
        let c = MockController::new(Zone::FRONT_LEFT, None);
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            cmd_type: CommandType::WATCHDOG,
            priority: crate::Priority::HIGH,
            ..Default::default()
        };
        let r = c.send(&cmd, None).unwrap();
        assert_eq!(r.status, ResponseStatus::OK);
    }

    #[test]
    // fusa:test REQ-CTRL-015
    fn cmd_reset_is_accepted_without_error() {
        let c = MockController::new(Zone::FRONT_LEFT, None);
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            cmd_type: CommandType::RESET,
            ..Default::default()
        };
        let r = c.send(&cmd, None).unwrap();
        assert_eq!(r.status, ResponseStatus::OK);
    }

    #[test]
    // fusa:test REQ-CTRL-016
    fn handler_response_returned_verbatim() {
        let custom = Response {
            command_id: 42,
            zone: Zone::FRONT_LEFT,
            status: ResponseStatus::ERROR,
            payload: Some(vec![0xAB]),
        };
        let custom2 = custom.clone();
        let h: Handler = Box::new(move |_| custom2.clone());
        let c = MockController::new(Zone::FRONT_LEFT, Some(h));
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            ..Default::default()
        };
        let resp = c.send(&cmd, None).unwrap();
        assert_eq!(resp.status, custom.status);
        assert_eq!(resp.command_id, custom.command_id);
        assert_eq!(resp.payload, custom.payload);
    }

    #[test]
    // fusa:test REQ-CTRL-025
    fn send_zone_mismatch_returns_err() {
        let c = MockController::new(Zone::FRONT_LEFT, None);
        let cmd = Command {
            zone: Zone::REAR_RIGHT,
            ..Default::default()
        };
        let err = c.send(&cmd, None).unwrap_err();
        assert_eq!(err, RcpError::ZoneMismatch);
        assert!(err.is_zone_mismatch());
        assert!(err.is_relay_not_connected());
    }

    #[test]
    // fusa:test REQ-CTRL-026
    fn send_copies_payload_before_handler() {
        let seen_payload = Arc::new(Mutex::new(vec![]));
        let seen2 = Arc::clone(&seen_payload);
        let h: Handler = Box::new(move |cmd| {
            *seen2.lock().unwrap() = cmd.payload.clone().unwrap_or_default();
            Response {
                command_id: cmd.id,
                zone: cmd.zone,
                status: ResponseStatus::OK,
                payload: None,
            }
        });
        let c = MockController::new(Zone::FRONT_LEFT, Some(h));
        let mut payload = vec![1u8, 2, 3];
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            payload: Some(payload.clone()),
            ..Default::default()
        };
        c.send(&cmd, None).unwrap();
        // Mutate original - handler copy must not change
        payload[0] = 0xFF;
        let handler_saw = seen_payload.lock().unwrap().clone();
        assert_eq!(handler_saw, vec![1u8, 2, 3]);
    }

    #[test]
    // fusa:test REQ-CTRL-024
    fn send_nil_payload_does_not_panic() {
        let c = MockController::new(Zone::FRONT_LEFT, None);
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            payload: None,
            ..Default::default()
        };
        assert!(c.send(&cmd, None).is_ok());
    }

    // ── Response field requirements ───────────────────────────────────────────

    #[test]
    // fusa:test REQ-RESP-001
    fn response_command_id_echoes_command_id() {
        let c = MockController::new(Zone::FRONT_LEFT, None);
        let cmd = Command {
            id: 0xDEAD_BEEF,
            zone: Zone::FRONT_LEFT,
            ..Default::default()
        };
        let resp = c.send(&cmd, None).unwrap();
        assert_eq!(resp.command_id, cmd.id);
    }

    #[test]
    // fusa:test REQ-RESP-002
    fn response_zone_matches_controller_zone() {
        let c = MockController::new(Zone::FRONT_LEFT, None);
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            ..Default::default()
        };
        let resp = c.send(&cmd, None).unwrap();
        assert_eq!(resp.zone, Zone::FRONT_LEFT);
    }

    // ── Subscribe / Publish ───────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-CTRL-006
    fn published_status_delivered_to_subscriber() {
        let c = MockController::new(Zone::FRONT_LEFT, None);
        let sub = c.subscribe().unwrap();
        c.publish(Some(vec![1, 2, 3]));
        let st = sub
            .recv_timeout(Duration::from_secs(1))
            .expect("expected status");
        assert_eq!(st.zone, Zone::FRONT_LEFT);
        assert_eq!(st.payload.as_deref(), Some([1u8, 2, 3].as_ref()));
    }

    #[test]
    // fusa:test REQ-CTRL-007
    fn close_closes_all_subscriber_channels() {
        let c = MockController::new(Zone::FRONT_LEFT, None);
        let sub = c.subscribe().unwrap();
        c.close().unwrap();
        // After close, channel should have closed - recv returns None
        let result = sub.recv_timeout(Duration::from_millis(200));
        assert!(
            result.is_none(),
            "channel should be closed after controller close"
        );
    }

    #[test]
    // fusa:test REQ-CTRL-008
    // fusa:test REQ-CTRL-011
    fn subscribe_after_close_returns_err_closed() {
        let c = MockController::new(Zone::FRONT_LEFT, None);
        c.close().unwrap();
        let err = c.subscribe().err().unwrap();
        assert_eq!(err, RcpError::Closed);
    }

    #[test]
    // fusa:test REQ-CTRL-010
    fn subscribe_seq_strictly_increasing() {
        let c = MockController::new(Zone::FRONT_LEFT, None);
        let sub = c.subscribe().unwrap();
        for _ in 0..5 {
            c.publish(None);
        }
        let mut last_seq = 0u32;
        for _ in 0..5 {
            let st = sub.recv_timeout(Duration::from_millis(500)).unwrap();
            assert!(st.seq > last_seq, "seq must be strictly increasing");
            last_seq = st.seq;
        }
    }

    #[test]
    // fusa:test REQ-CTRL-012
    fn multiple_concurrent_subscribers_each_receive_status() {
        let c = MockController::new(Zone::FRONT_LEFT, None);
        let sub1 = c.subscribe().unwrap();
        let sub2 = c.subscribe().unwrap();
        c.publish(Some(vec![0xAA]));
        let s1 = sub1.recv_timeout(Duration::from_secs(1)).unwrap();
        let s2 = sub2.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(s1.seq, s2.seq);
    }

    #[test]
    // fusa:test REQ-CTRL-017
    fn publish_on_closed_controller_does_not_panic() {
        let c = MockController::new(Zone::FRONT_LEFT, None);
        c.close().unwrap();
        // Must not panic
        c.publish(Some(vec![1, 2, 3]));
    }

    #[test]
    // fusa:test REQ-CTRL-018
    fn concurrent_sends_are_race_free() {
        let c = Arc::new(MockController::new(Zone::FRONT_LEFT, None));
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let c2 = Arc::clone(&c);
                std::thread::spawn(move || {
                    let cmd = Command {
                        id: i,
                        zone: Zone::FRONT_LEFT,
                        ..Default::default()
                    };
                    c2.send(&cmd, None).unwrap();
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
    }

    #[test]
    // fusa:test REQ-CTRL-019
    fn concurrent_publish_and_subscribe_are_race_free() {
        let c = Arc::new(MockController::new(Zone::FRONT_LEFT, None));
        let c2 = Arc::clone(&c);
        let publisher = std::thread::spawn(move || {
            for _ in 0..20 {
                c2.publish(None);
            }
        });
        let _sub = c.subscribe().unwrap();
        publisher.join().unwrap();
    }

    #[test]
    // fusa:test REQ-CTRL-020
    fn subscribe_status_carries_correct_zone() {
        let c = MockController::new(Zone::REAR_LEFT, None);
        let sub = c.subscribe().unwrap();
        c.publish(None);
        let st = sub.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(st.zone, Zone::REAR_LEFT);
    }

    #[test]
    // fusa:test REQ-CTRL-021
    fn subscribe_status_carries_correct_payload() {
        let c = MockController::new(Zone::FRONT_LEFT, None);
        let sub = c.subscribe().unwrap();
        c.publish(Some(vec![0xDE, 0xAD]));
        let st = sub.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(st.payload.as_deref(), Some([0xDEu8, 0xAD].as_ref()));
    }

    #[test]
    // fusa:test REQ-CTRL-022
    fn subscribe_status_healthy_is_true_while_open() {
        let c = MockController::new(Zone::FRONT_LEFT, None);
        let sub = c.subscribe().unwrap();
        c.publish(None);
        let st = sub.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(st.healthy);
    }

    #[test]
    // fusa:test REQ-CTRL-027
    fn publish_copies_payload_before_delivery() {
        let c = MockController::new(Zone::FRONT_LEFT, None);
        let sub = c.subscribe().unwrap();
        let mut payload = vec![0xAA, 0xBB];
        c.publish(Some(payload.clone()));
        let st = sub.recv_timeout(Duration::from_secs(1)).unwrap();
        // Mutate original after publish
        payload[0] = 0x00;
        // Subscriber sees original value
        assert_eq!(st.payload.as_deref(), Some([0xAAu8, 0xBB].as_ref()));
    }

    // ── Registry tests ────────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-REG-001
    fn new_registry_pre_populates_all_five_zones() {
        let r = MockRegistry::new();
        for z in [
            Zone::FRONT_LEFT,
            Zone::FRONT_RIGHT,
            Zone::REAR_LEFT,
            Zone::REAR_RIGHT,
            Zone::CENTRAL,
        ] {
            assert!(r.lookup(z).is_ok(), "zone {z:?} should be pre-populated");
        }
    }

    #[test]
    // fusa:test REQ-REG-002
    fn duplicate_zone_registration_returns_already_exists() {
        let r = MockRegistry::new();
        let ctrl = MockController::new(Zone::FRONT_LEFT, None);
        let err = r.register(ctrl).unwrap_err();
        assert_eq!(err, RcpError::AlreadyExists);
        assert!(err.is_already_exists());
    }

    #[test]
    // fusa:test REQ-REG-003
    fn deregister_removes_zone_and_closes_controller() {
        let r = MockRegistry::new();
        r.deregister(Zone::FRONT_LEFT).unwrap();
        let err = r.lookup(Zone::FRONT_LEFT).err().unwrap();
        assert_eq!(err, RcpError::NotFound);
    }

    #[test]
    // fusa:test REQ-REG-004
    fn lookup_not_registered_returns_not_found() {
        let r = MockRegistry::new();
        r.deregister(Zone::FRONT_LEFT).unwrap();
        let err = r.lookup(Zone::FRONT_LEFT).err().unwrap();
        assert_eq!(err, RcpError::NotFound);
        assert!(err.is_relay_not_connected());
    }

    #[test]
    // fusa:test REQ-REG-005
    fn registry_close_is_idempotent() {
        let r = MockRegistry::new();
        assert!(r.close().is_ok());
        assert!(r.close().is_ok());
    }

    #[test]
    // fusa:test REQ-REG-006
    fn controllers_returns_all_registered() {
        let r = MockRegistry::new();
        let ctrls = r.controllers();
        assert_eq!(ctrls.len(), 5);
    }

    #[test]
    // fusa:test REQ-REG-007
    fn register_after_close_returns_err_closed() {
        let r = MockRegistry::new();
        r.close().unwrap();
        let ctrl = MockController::new(Zone::UNKNOWN, None) as Arc<dyn Controller>;
        let err = r.register(ctrl).unwrap_err();
        assert_eq!(err, RcpError::Closed);
    }

    #[test]
    // fusa:test REQ-REG-008
    fn deregister_unregistered_zone_returns_not_found() {
        let r = MockRegistry::new();
        r.deregister(Zone::FRONT_LEFT).unwrap(); // first ok
        let err = r.deregister(Zone::FRONT_LEFT).unwrap_err();
        assert_eq!(err, RcpError::NotFound);
    }

    #[test]
    // fusa:test REQ-REG-009
    fn registered_controller_immediately_retrievable() {
        let r = MockRegistry::new();
        r.deregister(Zone::UNKNOWN).unwrap_or_default();
        let ctrl = MockController::new(Zone::UNKNOWN, None) as Arc<dyn Controller>;
        r.register(ctrl).unwrap();
        assert!(r.lookup(Zone::UNKNOWN).is_ok());
    }

    #[test]
    // fusa:test REQ-REG-010
    fn registry_close_closes_all_controllers() {
        let ctrl = MockController::new(Zone::UNKNOWN, None);
        let ctrl_arc = Arc::clone(&ctrl) as Arc<dyn Controller>;
        let r = MockRegistry::new();
        r.deregister(Zone::UNKNOWN).unwrap_or_default();
        r.register(ctrl_arc).unwrap();
        r.close().unwrap();
        // After close, controller should be closed — send returns Err
        let cmd = Command {
            zone: Zone::UNKNOWN,
            ..Default::default()
        };
        let err = ctrl.send(&cmd, None).unwrap_err();
        assert_eq!(err, RcpError::Closed);
    }

    #[test]
    // fusa:test REQ-REG-011
    // fusa:test REQ-REG-013
    fn lookup_on_closed_registry_returns_err_closed() {
        let r = MockRegistry::new();
        r.close().unwrap();
        let err = r.lookup(Zone::FRONT_LEFT).err().unwrap();
        assert_eq!(
            err,
            RcpError::Closed,
            "must return ErrClosed, not ErrNotFound"
        );
    }

    #[test]
    // fusa:test REQ-REG-012
    fn deregister_twice_returns_not_found_second_time() {
        let r = MockRegistry::new();
        r.deregister(Zone::FRONT_LEFT).unwrap();
        let err = r.deregister(Zone::FRONT_LEFT).unwrap_err();
        assert_eq!(err, RcpError::NotFound);
    }

    // ── Status requirements ───────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-STAT-001
    fn status_zone_identifies_publisher() {
        let c = MockController::new(Zone::REAR_RIGHT, None);
        let sub = c.subscribe().unwrap();
        c.publish(None);
        let st = sub.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(st.zone, Zone::REAR_RIGHT);
    }

    #[test]
    // fusa:test REQ-STAT-002
    fn status_seq_monotonically_increasing() {
        let c = MockController::new(Zone::FRONT_LEFT, None);
        let sub = c.subscribe().unwrap();
        c.publish(None);
        c.publish(None);
        let s1 = sub.recv_timeout(Duration::from_secs(1)).unwrap();
        let s2 = sub.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(s2.seq > s1.seq);
    }

    #[test]
    // fusa:test REQ-STAT-003
    fn status_healthy_is_true_while_open() {
        let c = MockController::new(Zone::FRONT_LEFT, None);
        let sub = c.subscribe().unwrap();
        c.publish(None);
        let st = sub.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(st.healthy);
    }

    #[test]
    // fusa:test REQ-STAT-004
    fn status_payload_carries_published_bytes() {
        let c = MockController::new(Zone::FRONT_LEFT, None);
        let sub = c.subscribe().unwrap();
        c.publish(Some(vec![0xFF, 0x00]));
        let st = sub.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(st.payload.as_deref(), Some([0xFFu8, 0x00].as_ref()));
    }

    #[test]
    // fusa:test REQ-STAT-005
    fn status_nil_payload_accepted() {
        let c = MockController::new(Zone::FRONT_LEFT, None);
        let sub = c.subscribe().unwrap();
        c.publish(None);
        let st = sub.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(st.payload.is_none());
    }
}
