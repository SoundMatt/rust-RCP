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
// fusa:req REQ-MOCKSRV-001
// fusa:req REQ-MOCKSRV-002
// fusa:req REQ-MOCKSRV-003
// fusa:req REQ-MOCKSRV-004
// fusa:req REQ-MOCKSRV-005
// fusa:req REQ-MOCKSRV-006
// fusa:req REQ-MOCKSRV-007
// fusa:req REQ-MOCKSRV-008
// fusa:req REQ-MOCKSRV-009
// fusa:req REQ-MOCKSRV-010

//! In-process test doubles for both this crate's old and new server models.
//!
//! `ROADMAP.md` Milestone 9's Satellite Package Disposition table calls
//! `mock` a **REPLACE** package: it "must model an RC Server + Endpoints
//! for testing, not a `Zone`-keyed controller." [`RcServer`]/[`Endpoint`]/
//! [`MockEndpoint`], added by this item, are that replacement — an
//! in-memory OPEN Alliance TC18 Remote Control Protocol Specification
//! v0.5.1_RC RC Server, keyed by `(`[`crate::avtp::StreamId`]`,
//! byte_bus_id)` and gated by [`crate::lifecycle::RcServerState`] rather
//! than by [`Zone`]. [`RcServer::handle_ntscf_frame`] answers a whole
//! on-wire request by composing already-built Milestone 1-3 primitives —
//! [`crate::avtp::decode_ntscf_frame`]/[`crate::avtp::encode_ntscf_frame`],
//! [`crate::acf::decode_acf_abb`]/[`crate::acf::encode_acf_abb`],
//! [`crate::acf::build_response_info`]/[`crate::acf::verify_echo_back`],
//! [`crate::ep0::route_byte_bus_id`]/[`crate::ep0::check_ep0_access_for_stream`],
//! and [`crate::addressing::EndpointTable`] — into the one live decode ->
//! route -> dispatch -> encode path this crate did not have anywhere
//! before this item: every one of those modules' own "Done" notes flagged
//! itself as additive, standalone plumbing, not yet wired into any
//! decoder or dispatch loop. This module is the first place that changes.
//!
//! The pre-existing [`MockController`]/[`MockRegistry`]/[`Handler`] test
//! double for the *old* [`Controller`]/[`Registry`]/[`Zone`]/[`Command`]/
//! [`Response`]/[`Status`] API is kept below, unmodified, rather than
//! deleted outright. This is a deliberate, narrower scope than a clean
//! REPLACE, recorded here per Guiding Principle 5 rather than silently
//! done: seventeen other still-`ADAPT`-disposition satellite packages'
//! own unit tests (`ratelimit`, `deadline`, `faultinject`, `proxy`,
//! `redundancy`, `observe`, `authz`, `record`, `prioqueue`, `zonegroup`,
//! `adapt`, `loan`, `admin`, `federation`, `tsn`, `firmware`) plus
//! `src/bin/rcp.rs` all construct [`MockController`]/[`MockRegistry`]
//! today to test their own `Controller`/`Registry`-based decorators and
//! CLI plumbing, none of which has been retargeted onto a new trait yet —
//! that retargeting is `ROADMAP.md` Milestone 9's own *second* checklist
//! bullet ("All ADAPT-disposition packages retargeted..."), a distinct,
//! not-yet-started item, not this REPLACE bullet's job. Deleting
//! `MockController`/`MockRegistry` now, before that bullet lands, would
//! break every one of those seventeen files' builds for no corresponding
//! benefit, since the `Controller`/`Registry`/`Zone`/`Command`/`Response`/
//! `Status` types they exercise are themselves not removed by this item
//! either (that is Milestone 9's ADAPT bullet, package by package, and
//! Milestone 10's core-surface cutover for the types themselves). This
//! crate's `.fusa-reqs.json` `REQ-CTRL-*`/`REQ-REG-*`/`REQ-RESP-*`/
//! `REQ-STAT-*`/`REQ-ERR-011` requirements therefore stay exactly as they
//! were — describing still-live, still-tested code — rather than being
//! retargeted or retired, unlike `wire`/`e2e`'s own REPLACE cutovers
//! immediately before this one in the roadmap, both of which had no
//! remaining external caller to preserve.
//!
//! All operations in both halves of this module execute synchronously in
//! memory with no network I/O, and are safe for concurrent use.

use std::collections::HashMap;
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

// ─────────────────────────────────────────────────────────────────────────────
//  RC Server + Endpoints test double (ROADMAP.md Milestone 9, `mock` REPLACE)
// ─────────────────────────────────────────────────────────────────────────────
//
// See this module's doc comment for the full scope note. In short: this
// section models an in-memory OPEN Alliance TC18 RC Server good enough to
// drive a request through the whole decode -> route -> dispatch -> encode
// path this crate did not previously assemble anywhere, addressed by
// `(StreamId, byte_bus_id)` and gated by `RcServerState` rather than by
// `Zone`.

use crate::acf::{
    build_response_info, decode_acf_abb, encode_acf_abb, verify_echo_back, AcfAbbMessage,
};
use crate::addressing::{EndpointId, EndpointTable};
use crate::avtp::{decode_ntscf_frame, encode_ntscf_frame, StreamId};
use crate::ep0::{check_ep0_access_for_stream, route_byte_bus_id, RequestRoute};
use crate::lifecycle::{RcServerState, RegisterCategory};
use crate::regmap::{EndpointType, GeneralRegisters};

// ── Endpoint abstraction ──────────────────────────────────────────────────────

/// The minimal per-endpoint behavior [`RcServer`] dispatches a
/// device-endpoint-addressed request to, once [`crate::ep0::route_byte_bus_id`]
/// has decided the request is not EP0-addressed.
///
/// Concrete, device-facing endpoint types (`crate::gpio`, `crate::can`,
/// etc.) are each their own additive, standalone set of pure functions over
/// their own wire shapes today (per every one of Milestone 4/7's own "Done"
/// notes) — none of them implement this trait yet, and wiring any one of
/// them onto it is out of scope for this item; it belongs to whichever
/// later milestone item first needs a live endpoint dispatched through an
/// RC Server. `canbr`'s own REPLACE rebuild (Milestone 9) has since
/// completed without wiring `crate::can` onto this trait either, for the
/// same reason — that rebuild's scope was the legacy `CanBridge`/
/// `CanSocket` cutover itself, not new dispatch plumbing; `linbr`'s own
/// still-open REPLACE rebuild remains the most likely next caller, per
/// `ROADMAP.md`'s own Progress note for this bullet. [`MockEndpoint`] is
/// this item's only implementation, standing
/// in for a concrete endpoint the same way [`crate::addressing::EndpointId`]
/// itself stands in for one.
///
/// Takes `&self` rather than `&mut self` so an implementation can be shared
/// behind `Arc<dyn Endpoint>` inside [`RcServer`]'s endpoint map — the same
/// shared-behind-`Arc`, interior-mutability shape [`MockController`] above
/// already uses for the old model. Never required to panic for any input;
/// [`MockEndpoint`]'s own impl never does.
pub trait Endpoint: Send + Sync {
    /// This endpoint's register-map type discriminant.
    fn ep_type(&self) -> EndpointType;

    /// Answer a read addressed to this endpoint.
    ///
    /// `read_size` is the request's raw
    /// [`crate::acf::ReadSizeOrSegmentNum::as_read_size`] byte. This trait
    /// does not itself prescribe what an implementation does with it —
    /// [`MockEndpoint::read`] treats it as a requested byte count, capped
    /// to however much data is actually held.
    fn read(&self, read_size: u8) -> Result<Vec<u8>, RcpError>;

    /// Apply a write addressed to this endpoint, given the request's raw
    /// payload bytes.
    fn write(&self, payload: &[u8]) -> Result<(), RcpError>;
}

/// A trivial byte-buffer-backed [`Endpoint`] test double.
///
/// [`MockEndpoint::write`] replaces the held buffer wholesale with
/// `payload`; [`MockEndpoint::read`] returns up to `read_size` bytes from
/// the front of whatever is currently held, or the whole buffer if
/// `read_size` (as an unsigned byte count) is not smaller than it — this
/// crate's own simplification, not a transcription of any real per-endpoint
/// read-chunking rule (none of the concrete endpoint-type modules define
/// one uniformly; that is separate, later work, same as
/// [`crate::fragment`]'s own AVTPDU-size chunking already flags for
/// responses).
pub struct MockEndpoint {
    ep_type: EndpointType,
    buf: Mutex<Vec<u8>>,
}

impl MockEndpoint {
    /// Construct a mock endpoint of type `ep_type`, initially holding
    /// `initial` as its buffer.
    pub fn new(ep_type: EndpointType, initial: Vec<u8>) -> Arc<Self> {
        Arc::new(Self {
            ep_type,
            buf: Mutex::new(initial),
        })
    }
}

impl Endpoint for MockEndpoint {
    fn ep_type(&self) -> EndpointType {
        self.ep_type
    }

    // fusa:req REQ-MOCKSRV-010
    fn read(&self, read_size: u8) -> Result<Vec<u8>, RcpError> {
        let buf = self.buf.lock().unwrap();
        let n = (read_size as usize).min(buf.len());
        Ok(buf[..n].to_vec())
    }

    // fusa:req REQ-MOCKSRV-009
    fn write(&self, payload: &[u8]) -> Result<(), RcpError> {
        *self.buf.lock().unwrap() = payload.to_vec();
        Ok(())
    }
}

// ── RcServer ──────────────────────────────────────────────────────────────────

/// An in-memory OPEN Alliance TC18 RC Server test double.
///
/// Holds exactly the state this item's design calls for: a
/// [`RcServerState`] lifecycle position, a [`GeneralRegisters`] snapshot
/// (the only [`RegisterCategory::General`] register block this crate has
/// concretely defined so far — `HwConfig`/`RcpConfig` register I/O through
/// EP0 is not modeled by this item; see [`Self::handle_abb`]'s doc comment),
/// an optional root-client [`StreamId`] gating EP0 writes, and an
/// [`EndpointTable`] of registered device endpoints alongside their
/// [`Endpoint`] implementations.
///
/// Deliberately does not model the old [`MockController`]'s
/// publish/subscribe `Status` broadcast: this crate's new core has no live
/// asynchronous-notification mechanism yet (no TC18 analog has been
/// identified for it in this crate to date), so replicating that shape here
/// would invent behavior rather than model something real. Should a real
/// notification mechanism land in a later milestone, extending this type to
/// test-double it is that milestone's job, not this one's.
pub struct RcServer {
    state: Mutex<RcServerState>,
    general: Mutex<GeneralRegisters>,
    root_client: Mutex<Option<StreamId>>,
    endpoints: Mutex<EndpointTable>,
    endpoint_impls: Mutex<HashMap<EndpointId, Arc<dyn Endpoint>>>,
    next_endpoint_id: AtomicU32,
    /// Free-running NTSCF `sequence_num` counter shared across every
    /// stream this server answers. A per-stream counter would be more
    /// faithful, but this crate has not built a per-stream sequencer
    /// registry for responses yet; a single shared counter is this test
    /// double's own simplification, not a spec requirement.
    sequence_num: AtomicU32,
}

impl RcServer {
    /// Construct a fresh RC Server, starting at [`RcServerState::INITIAL`]
    /// with no root client and no registered endpoints, holding `general`
    /// as its initial [`GeneralRegisters`] snapshot.
    // fusa:req REQ-MOCKSRV-001
    pub fn new(general: GeneralRegisters) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(RcServerState::INITIAL),
            general: Mutex::new(general),
            root_client: Mutex::new(None),
            endpoints: Mutex::new(EndpointTable::new()),
            endpoint_impls: Mutex::new(HashMap::new()),
            next_endpoint_id: AtomicU32::new(0),
            sequence_num: AtomicU32::new(0),
        })
    }

    /// This server's current lifecycle state.
    // fusa:req REQ-MOCKSRV-002
    pub fn state(&self) -> RcServerState {
        *self.state.lock().unwrap()
    }

    /// A snapshot of this server's current [`GeneralRegisters`].
    pub fn general_registers(&self) -> GeneralRegisters {
        *self.general.lock().unwrap()
    }

    /// Attempt to move this server from its current state to `target`,
    /// delegating to [`RcServerState::try_transition`]. On success, this
    /// server's stored state is updated to `target`; on failure, it is left
    /// unchanged.
    // fusa:req REQ-MOCKSRV-002
    pub fn try_transition(
        &self,
        target: RcServerState,
        is_consistent: impl FnOnce() -> bool,
    ) -> Result<(), RcpError> {
        let mut state = self.state.lock().unwrap();
        let new_state = state.try_transition(target, is_consistent)?;
        *state = new_state;
        Ok(())
    }

    /// Designate `stream` (or nobody, if `None`) as this server's root
    /// client — the one stream permitted to write EP0, per
    /// [`crate::ep0::check_ep0_access_for_stream`].
    pub fn set_root_client(&self, stream: Option<StreamId>) {
        *self.root_client.lock().unwrap() = stream;
    }

    /// Register `endpoint` under `(stream_id, byte_bus_id)` and return the
    /// fresh [`EndpointId`] handle it was assigned.
    ///
    /// Returns whatever error [`EndpointTable::register`] returns —
    /// `Err(RcpError::InvalidSize)` for an oversized `byte_bus_id`, or
    /// `Err(RcpError::EpError)` for an already-registered pair — without
    /// allocating an endpoint id or storing `endpoint` in either case.
    // fusa:req REQ-MOCKSRV-003
    pub fn register_endpoint(
        &self,
        stream_id: StreamId,
        byte_bus_id: u16,
        endpoint: Arc<dyn Endpoint>,
    ) -> Result<EndpointId, RcpError> {
        let id = EndpointId(self.next_endpoint_id.load(Ordering::SeqCst));
        self.endpoints
            .lock()
            .unwrap()
            .register(stream_id, byte_bus_id, id)?;
        self.next_endpoint_id.fetch_add(1, Ordering::SeqCst);
        self.endpoint_impls.lock().unwrap().insert(id, endpoint);
        Ok(id)
    }

    /// Answer one already-decoded [`AcfAbbMessage`] request from
    /// `stream_id`, returning the response [`AcfAbbMessage`] to send back.
    ///
    /// Routing follows [`crate::ep0::route_byte_bus_id`]:
    ///
    /// - [`RequestRoute::Ep0`]: gated by
    ///   [`check_ep0_access_for_stream`] against
    ///   [`RegisterCategory::General`] only — this test double models no
    ///   other register category's storage, so an EP0 access this crate
    ///   would otherwise route to `HwConfig`/`RcpConfig` is answered as
    ///   `General` too rather than rejected outright, a scope-narrowing
    ///   simplification flagged here rather than silently assumed. A read
    ///   returns the current [`GeneralRegisters::encode`] snapshot
    ///   verbatim, for either the root client or any other stream (reads
    ///   are never root-client-gated, per [`check_ep0_access_for_stream`]'s
    ///   own doc comment). A write requires the payload to decode as a
    ///   complete [`GeneralRegisters`] block (`Err(RcpError::ShortFrame)`
    ///   otherwise) and would replace the snapshot wholesale — but
    ///   [`crate::lifecycle::lock_policy`] assigns
    ///   [`RegisterCategory::General`] no [`crate::lifecycle::LockPolicy`]
    ///   at all, which [`crate::lifecycle::is_register_writable`]'s own doc
    ///   comment states means "never writable regardless of lifecycle
    ///   state." This test double does not special-case that: an EP0 write
    ///   is therefore always rejected with `Err(RcpError::LockedMemAccess)`
    ///   once past the root-client check, for the root client exactly as
    ///   for anyone else, and the snapshot is never actually replaced by
    ///   this path today — an honest consequence of modeling only
    ///   `General`, not a bug, and not this item's to work around by
    ///   inventing a writable category this crate has not built storage
    ///   for.
    /// - [`RequestRoute::DeviceEndpoint`]: resolved through
    ///   [`EndpointTable::lookup`], `Err(RcpError::EpNotFound)` if nothing
    ///   is registered under the pair, otherwise dispatched to the
    ///   registered [`Endpoint::read`]/[`Endpoint::write`].
    ///
    /// Every response this function builds echoes `request.info.byte_bus_id`
    /// via [`build_response_info`], and is checked against
    /// [`verify_echo_back`] before being returned (never observably fails,
    /// since `build_response_info` always sets the field it echoes — this
    /// call exists so a future change to either function is caught by this
    /// module's own tests rather than by a caller).
    // fusa:req REQ-MOCKSRV-004
    // fusa:req REQ-MOCKSRV-005
    // fusa:req REQ-MOCKSRV-006
    // fusa:req REQ-MOCKSRV-007
    pub fn handle_abb(
        &self,
        stream_id: StreamId,
        request: &AcfAbbMessage,
    ) -> Result<AcfAbbMessage, RcpError> {
        let response_payload = match route_byte_bus_id(request.info.byte_bus_id) {
            RequestRoute::Ep0 => {
                let state = self.state();
                let root_client = *self.root_client.lock().unwrap();
                check_ep0_access_for_stream(
                    state,
                    RegisterCategory::General,
                    &request.info,
                    stream_id,
                    root_client,
                )?;
                if request.info.op {
                    let decoded = GeneralRegisters::decode(&request.payload)?;
                    *self.general.lock().unwrap() = decoded;
                    Vec::new()
                } else {
                    self.general.lock().unwrap().encode().to_vec()
                }
            }
            RequestRoute::DeviceEndpoint => {
                let endpoint_id = self
                    .endpoints
                    .lock()
                    .unwrap()
                    .lookup(stream_id, request.info.byte_bus_id)
                    .ok_or(RcpError::EpNotFound)?;
                let endpoint = self
                    .endpoint_impls
                    .lock()
                    .unwrap()
                    .get(&endpoint_id)
                    .cloned()
                    .ok_or(RcpError::EpNotFound)?;
                if request.info.op {
                    endpoint.write(&request.payload)?;
                    Vec::new()
                } else {
                    endpoint.read(request.info.read_size_segment_num.as_read_size())?
                }
            }
        };

        let response_info = build_response_info(&request.info, request.info);
        verify_echo_back(&request.info, &response_info)?;
        Ok(AcfAbbMessage {
            info: response_info,
            payload: response_payload,
        })
    }

    /// Answer one whole on-wire NTSCF-framed ACF_ABB request, given
    /// `stream_id` and the raw AVTPDU bytes a transport received.
    ///
    /// Composes [`decode_ntscf_frame`] -> [`decode_acf_abb`] ->
    /// [`Self::handle_abb`] -> [`encode_acf_abb`] -> [`encode_ntscf_frame`]
    /// so a caller never has to touch any intermediate decoded type — the
    /// same reuse of Milestone 1's already-built AVTPDU/ACF stack the
    /// `wire` REPLACE cutover established for [`crate::udp::UdpTransport`].
    /// The response frame's `sequence_num` is this server's own
    /// free-running counter (see [`Self`]'s doc comment), unrelated to the
    /// request frame's.
    // fusa:req REQ-MOCKSRV-008
    pub fn handle_ntscf_frame(
        &self,
        stream_id: StreamId,
        frame: &[u8],
    ) -> Result<Vec<u8>, RcpError> {
        let (_hdr, acf_bytes) = decode_ntscf_frame(frame)?;
        let request = decode_acf_abb(acf_bytes)?;
        let response = self.handle_abb(stream_id, &request)?;
        let response_bytes = encode_acf_abb(&response)?;
        let seq = self.sequence_num.fetch_add(1, Ordering::SeqCst) as u8;
        encode_ntscf_frame(stream_id, seq, &response_bytes)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod rc_server_tests {
    use super::*;
    use crate::acf::{ByteMessageInfo, Evt, ReadSizeOrSegmentNum};
    use crate::ep0::EP0_BYTE_BUS_ID;

    fn stream(unique_id: u16) -> StreamId {
        StreamId::new([0x02, 0x11, 0x22, 0x33, 0x44, 0x55], unique_id)
    }

    fn abb_request(byte_bus_id: u16, op: bool, payload: Vec<u8>) -> AcfAbbMessage {
        AcfAbbMessage {
            info: ByteMessageInfo {
                byte_bus_id,
                op,
                evt: Evt::default(),
                read_size_segment_num: ReadSizeOrSegmentNum(payload.len() as u8),
                ..Default::default()
            },
            payload,
        }
    }

    // ── Lifecycle ─────────────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-MOCKSRV-001
    fn new_server_starts_hw_unconfigured_with_no_root_client() {
        let srv = RcServer::new(GeneralRegisters::default());
        assert_eq!(srv.state(), RcServerState::HwUnconfigured);
    }

    #[test]
    // fusa:test REQ-MOCKSRV-002
    fn try_transition_updates_state_on_success_and_leaves_it_on_failure() {
        let srv = RcServer::new(GeneralRegisters::default());
        srv.try_transition(RcServerState::HwConfigured, || true)
            .unwrap();
        assert_eq!(srv.state(), RcServerState::HwConfigured);

        let err = srv
            .try_transition(RcServerState::RcpConfigured, || false)
            .unwrap_err();
        assert_eq!(err, RcpError::InvalidParameter);
        assert_eq!(srv.state(), RcServerState::HwConfigured);
    }

    // ── Endpoint registration ─────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-MOCKSRV-003
    fn register_endpoint_assigns_unique_ids_and_rejects_duplicate_pair() {
        let srv = RcServer::new(GeneralRegisters::default());
        let sid = stream(1);
        let ep_a = MockEndpoint::new(EndpointType::Gpio, vec![0; 4]);
        let ep_b = MockEndpoint::new(EndpointType::Gpio, vec![0; 4]);

        let id_a = srv.register_endpoint(sid, 7, ep_a).unwrap();
        let id_b = srv.register_endpoint(sid, 8, ep_b.clone()).unwrap();
        assert_ne!(id_a, id_b);

        let err = srv.register_endpoint(sid, 8, ep_b).unwrap_err();
        assert_eq!(err, RcpError::EpError);
    }

    // ── EP0 dispatch ──────────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-MOCKSRV-004
    // fusa:test REQ-MOCKSRV-005
    fn ep0_read_returns_general_registers_snapshot() {
        let regs = GeneralRegisters {
            svr_vendor_id: 0x1234,
            ..Default::default()
        };
        let srv = RcServer::new(regs);
        let sid = stream(1);

        let req = abb_request(EP0_BYTE_BUS_ID, false, Vec::new());
        let resp = srv.handle_abb(sid, &req).unwrap();
        assert_eq!(resp.payload, regs.encode().to_vec());
    }

    #[test]
    // fusa:test REQ-MOCKSRV-004
    // fusa:test REQ-MOCKSRV-005
    fn ep0_write_is_locked_even_for_the_root_client() {
        // RegisterCategory::General has no LockPolicy at all
        // (crate::lifecycle::lock_policy), meaning "never writable
        // regardless of lifecycle state" per is_register_writable's own
        // doc comment — this holds even for the designated root client,
        // who is otherwise the only stream ever permitted to write EP0 at
        // all. See handle_abb's own doc comment for why this test double
        // does not work around that.
        let srv = RcServer::new(GeneralRegisters::default());
        let sid = stream(1);
        srv.set_root_client(Some(sid));

        let new_regs = GeneralRegisters {
            svr_vendor_id: 0xBEEF,
            ..Default::default()
        };
        let req = abb_request(EP0_BYTE_BUS_ID, true, new_regs.encode().to_vec());
        let err = srv.handle_abb(sid, &req).unwrap_err();
        assert_eq!(err, RcpError::LockedMemAccess);

        // The snapshot must be unchanged.
        assert_eq!(
            srv.general_registers().svr_vendor_id,
            GeneralRegisters::default().svr_vendor_id
        );
    }

    #[test]
    // fusa:test REQ-MOCKSRV-004
    fn ep0_write_from_non_root_client_is_rejected() {
        let srv = RcServer::new(GeneralRegisters::default());
        let root = stream(1);
        let other = stream(2);
        srv.set_root_client(Some(root));

        let req = abb_request(
            EP0_BYTE_BUS_ID,
            true,
            GeneralRegisters::default().encode().to_vec(),
        );
        let err = srv.handle_abb(other, &req).unwrap_err();
        assert_eq!(err, RcpError::UnauthorizedAccess);
        // The snapshot must be unchanged.
        assert_eq!(
            srv.general_registers().svr_vendor_id,
            GeneralRegisters::default().svr_vendor_id
        );
    }

    #[test]
    // fusa:test REQ-MOCKSRV-004
    fn ep0_read_is_reachable_in_every_lifecycle_state() {
        let srv = RcServer::new(GeneralRegisters::default());
        let sid = stream(1);
        let req = abb_request(EP0_BYTE_BUS_ID, false, Vec::new());

        // HW_UNCONFIGURED (initial).
        assert!(srv.handle_abb(sid, &req).is_ok());

        srv.try_transition(RcServerState::HwConfigured, || true)
            .unwrap();
        assert!(srv.handle_abb(sid, &req).is_ok());

        srv.try_transition(RcServerState::RcpConfigured, || true)
            .unwrap();
        assert!(srv.handle_abb(sid, &req).is_ok());
    }

    // ── Device endpoint dispatch ──────────────────────────────────────────────

    #[test]
    // fusa:test REQ-MOCKSRV-006
    fn device_endpoint_write_then_read_round_trips_through_dispatch() {
        let srv = RcServer::new(GeneralRegisters::default());
        let sid = stream(1);
        let ep = MockEndpoint::new(EndpointType::Gpio, vec![0; 4]);
        srv.register_endpoint(sid, 5, ep).unwrap();

        let write_req = abb_request(5, true, vec![0xDE, 0xAD, 0xBE, 0xEF]);
        srv.handle_abb(sid, &write_req).unwrap();

        let mut read_req = abb_request(5, false, Vec::new());
        read_req.info.read_size_segment_num = ReadSizeOrSegmentNum(4);
        let resp = srv.handle_abb(sid, &read_req).unwrap();
        assert_eq!(resp.payload, vec![0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    // fusa:test REQ-MOCKSRV-006
    fn unregistered_device_endpoint_returns_ep_not_found() {
        let srv = RcServer::new(GeneralRegisters::default());
        let sid = stream(1);
        let req = abb_request(9, false, Vec::new());
        let err = srv.handle_abb(sid, &req).unwrap_err();
        assert_eq!(err, RcpError::EpNotFound);
    }

    #[test]
    // fusa:test REQ-MOCKSRV-006
    fn endpoint_registered_under_one_stream_is_not_visible_from_another() {
        let srv = RcServer::new(GeneralRegisters::default());
        let sid_a = stream(1);
        let sid_b = stream(2);
        let ep = MockEndpoint::new(EndpointType::Gpio, vec![0; 4]);
        srv.register_endpoint(sid_a, 5, ep).unwrap();

        let req = abb_request(5, false, Vec::new());
        assert!(srv.handle_abb(sid_a, &req).is_ok());
        let err = srv.handle_abb(sid_b, &req).unwrap_err();
        assert_eq!(err, RcpError::EpNotFound);
    }

    // ── Echo-back ─────────────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-MOCKSRV-007
    fn response_echoes_request_byte_bus_id() {
        let srv = RcServer::new(GeneralRegisters::default());
        let sid = stream(1);
        let ep = MockEndpoint::new(EndpointType::Gpio, vec![0; 4]);
        srv.register_endpoint(sid, 11, ep).unwrap();

        let req = abb_request(11, false, Vec::new());
        let resp = srv.handle_abb(sid, &req).unwrap();
        assert_eq!(resp.info.byte_bus_id, 11);
        assert!(resp.info.rsp);
    }

    // ── Whole on-wire round trip ──────────────────────────────────────────────

    #[test]
    // fusa:test REQ-MOCKSRV-008
    fn handle_ntscf_frame_round_trips_a_whole_on_wire_request() {
        let srv = RcServer::new(GeneralRegisters::default());
        let sid = stream(1);
        let ep = MockEndpoint::new(EndpointType::Gpio, vec![0xAA, 0xBB, 0xCC, 0xDD]);
        srv.register_endpoint(sid, 3, ep).unwrap();

        let mut req = abb_request(3, false, Vec::new());
        req.info.read_size_segment_num = ReadSizeOrSegmentNum(4);
        let req_bytes = encode_acf_abb(&req).unwrap();
        let frame = encode_ntscf_frame(sid, 0, &req_bytes).unwrap();

        let response_frame = srv.handle_ntscf_frame(sid, &frame).unwrap();
        let (_hdr, resp_acf_bytes) = decode_ntscf_frame(&response_frame).unwrap();
        let resp = decode_acf_abb(resp_acf_bytes).unwrap();
        assert_eq!(resp.payload, vec![0xAA, 0xBB, 0xCC, 0xDD]);
        assert_eq!(resp.info.byte_bus_id, 3);
    }

    #[test]
    // fusa:test REQ-MOCKSRV-008
    fn handle_ntscf_frame_never_panics_on_garbage_input() {
        let srv = RcServer::new(GeneralRegisters::default());
        let sid = stream(1);
        for garbage in [&b""[..], &b"\x00"[..], &[0xFFu8; 4][..], &[0u8; 40][..]] {
            let _ = srv.handle_ntscf_frame(sid, garbage);
        }
    }

    // ── MockEndpoint ──────────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-MOCKSRV-009
    fn mock_endpoint_read_returns_last_written_bytes() {
        let ep = MockEndpoint::new(EndpointType::Gpio, Vec::new());
        ep.write(&[1, 2, 3]).unwrap();
        assert_eq!(ep.read(3).unwrap(), vec![1, 2, 3]);
    }

    #[test]
    // fusa:test REQ-MOCKSRV-010
    fn mock_endpoint_read_size_exceeding_buffer_does_not_panic() {
        let ep = MockEndpoint::new(EndpointType::Gpio, vec![1, 2]);
        let out = ep.read(255).unwrap();
        assert_eq!(out, vec![1, 2]);
    }

    #[test]
    // fusa:test REQ-MOCKSRV-010
    fn mock_endpoint_read_on_empty_buffer_does_not_panic() {
        let ep = MockEndpoint::new(EndpointType::Gpio, Vec::new());
        assert_eq!(ep.read(10).unwrap(), Vec::<u8>::new());
    }
}
