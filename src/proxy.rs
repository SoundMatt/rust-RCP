// fusa:req REQ-PROXY-001
// fusa:req REQ-PROXY-002
// fusa:req REQ-PROXY-003
// fusa:req REQ-PROXY-004
// fusa:req REQ-PROXY-005
// fusa:req REQ-PROXY-006

//! Transparent proxy controller — delegates to an interchangeable inner
//! controller, allowing hot-swap without changing the call site.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use crate::{Command, Controller, RcpError, Response, Subscription, Zone};

// ── ProxyController ───────────────────────────────────────────────────────────

/// A proxy that forwards all calls to a replaceable inner controller.
// fusa:req REQ-PROXY-001
pub struct ProxyController {
    zone:  Zone,
    inner: RwLock<Option<Arc<dyn Controller>>>,
}

impl ProxyController {
    /// Create a proxy backed by `inner`.
    // fusa:req REQ-PROXY-002
    pub fn new(inner: Arc<dyn Controller>) -> Self {
        let zone = inner.zone();
        ProxyController { zone, inner: RwLock::new(Some(inner)) }
    }

    /// Replace the inner controller atomically.
    // fusa:req REQ-PROXY-005
    pub fn swap(&self, new_inner: Arc<dyn Controller>) {
        *self.inner.write().unwrap() = Some(new_inner);
    }

    /// Detach the inner controller; subsequent calls return `Err(RcpError::NotConnected)`.
    // fusa:req REQ-PROXY-006
    pub fn detach(&self) {
        *self.inner.write().unwrap() = None;
    }
}

impl Controller for ProxyController {
    fn zone(&self) -> Zone { self.zone }

    // fusa:req REQ-PROXY-003
    fn send(&self, cmd: &Command, timeout: Option<Duration>) -> Result<Response, RcpError> {
        let guard = self.inner.read().unwrap();
        match guard.as_ref() {
            Some(ctrl) => ctrl.send(cmd, timeout),
            None       => Err(RcpError::NotConnected),
        }
    }

    // fusa:req REQ-PROXY-004
    fn subscribe(&self) -> Result<Subscription, RcpError> {
        let guard = self.inner.read().unwrap();
        match guard.as_ref() {
            Some(ctrl) => ctrl.subscribe(),
            None       => Err(RcpError::NotConnected),
        }
    }

    fn close(&self) -> Result<(), RcpError> {
        let guard = self.inner.read().unwrap();
        match guard.as_ref() {
            Some(ctrl) => ctrl.close(),
            None       => Ok(()),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockController;
    use crate::{Command, Response, ResponseStatus, Zone};

    fn ok_ctrl(zone: Zone) -> Arc<dyn Controller> {
        let h: crate::mock::Handler = Box::new(move |cmd| Response {
            command_id: cmd.id, zone: cmd.zone,
            status: ResponseStatus::OK, payload: None,
        });
        MockController::new(zone, Some(h)) as Arc<dyn Controller>
    }

    #[test]
    // fusa:test REQ-PROXY-001
    // fusa:test REQ-PROXY-003
    fn forwards_send_to_inner() {
        let proxy = ProxyController::new(ok_ctrl(Zone::FRONT_LEFT));
        let cmd = Command { zone: Zone::FRONT_LEFT, ..Default::default() };
        proxy.send(&cmd, None).unwrap();
    }

    #[test]
    // fusa:test REQ-PROXY-002
    fn zone_matches_original_inner() {
        let proxy = ProxyController::new(ok_ctrl(Zone::REAR_LEFT));
        assert_eq!(proxy.zone(), Zone::REAR_LEFT);
    }

    #[test]
    // fusa:test REQ-PROXY-005
    fn swap_replaces_inner() {
        let proxy = ProxyController::new(ok_ctrl(Zone::FRONT_LEFT));
        // Close the original inner so it would return Err(Closed)
        let closed = ok_ctrl(Zone::FRONT_LEFT);
        closed.close().unwrap();
        proxy.swap(Arc::clone(&closed));
        let cmd = Command { zone: Zone::FRONT_LEFT, ..Default::default() };
        let err = proxy.send(&cmd, None).unwrap_err();
        assert_eq!(err, RcpError::Closed);
    }

    #[test]
    // fusa:test REQ-PROXY-006
    fn detach_returns_not_connected() {
        let proxy = ProxyController::new(ok_ctrl(Zone::FRONT_LEFT));
        proxy.detach();
        let err = proxy.send(&Command { zone: Zone::FRONT_LEFT, ..Default::default() }, None).unwrap_err();
        assert_eq!(err, RcpError::NotConnected);
    }

    #[test]
    // fusa:test REQ-PROXY-004
    fn subscribe_forwarded() {
        let proxy = ProxyController::new(ok_ctrl(Zone::FRONT_LEFT));
        proxy.subscribe().unwrap();
    }

    #[test]
    // fusa:test REQ-PROXY-006
    fn subscribe_detached_returns_not_connected() {
        let proxy = ProxyController::new(ok_ctrl(Zone::FRONT_LEFT));
        proxy.detach();
        let err = proxy.subscribe().unwrap_err();
        assert_eq!(err, RcpError::NotConnected);
    }
}
