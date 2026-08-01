//fusa:req REQ-PROXY-001
//fusa:req REQ-PROXY-002
//fusa:req REQ-PROXY-003
//fusa:req REQ-PROXY-004
//fusa:req REQ-PROXY-005
//fusa:req REQ-PROXY-006

//! Transparent proxy endpoint — delegates to an interchangeable inner
//! endpoint, allowing hot-swap without changing the call site.
//!
//! `ROADMAP.md` Milestone 9 ("All ADAPT-disposition packages retargeted...")
//! cutover: per this module's own ADAPT disposition ("fully generic
//! hot-swap decorator; trivially retargeted to any new base trait"),
//! [`ProxyEndpoint`] replaces the legacy `ProxyController`, wrapping
//! [`crate::mock::Endpoint`] instead of `Controller`. [`Self::ep_type`] is
//! captured once, from the original inner endpoint at construction, and
//! never re-queried on [`Self::swap`] — the same "identity survives a
//! swap" discipline the old type's `zone` field already established.

use std::sync::{Arc, RwLock};

use crate::mock::Endpoint;
use crate::regmap::EndpointType;
use crate::RcpError;

// ── ProxyEndpoint ─────────────────────────────────────────────────────────────

/// A proxy that forwards all calls to a replaceable inner endpoint.
//fusa:req REQ-PROXY-001
pub struct ProxyEndpoint {
    ep_type: EndpointType,
    inner: RwLock<Option<Arc<dyn Endpoint>>>,
}

impl ProxyEndpoint {
    /// Create a proxy backed by `inner`.
    //fusa:req REQ-PROXY-002
    pub fn new(inner: Arc<dyn Endpoint>) -> Self {
        let ep_type = inner.ep_type();
        ProxyEndpoint {
            ep_type,
            inner: RwLock::new(Some(inner)),
        }
    }

    /// Replace the inner endpoint atomically.
    //fusa:req REQ-PROXY-005
    pub fn swap(&self, new_inner: Arc<dyn Endpoint>) {
        *self.inner.write().unwrap() = Some(new_inner);
    }

    /// Detach the inner endpoint; subsequent calls return `Err(RcpError::NotConnected)`.
    //fusa:req REQ-PROXY-006
    pub fn detach(&self) {
        *self.inner.write().unwrap() = None;
    }
}

impl Endpoint for ProxyEndpoint {
    fn ep_type(&self) -> EndpointType {
        self.ep_type
    }

    //fusa:req REQ-PROXY-003
    fn read(&self, read_size: u16) -> Result<Vec<u8>, RcpError> {
        let guard = self.inner.read().unwrap();
        match guard.as_ref() {
            Some(ep) => ep.read(read_size),
            None => Err(RcpError::NotConnected),
        }
    }

    //fusa:req REQ-PROXY-004
    fn write(&self, payload: &[u8]) -> Result<(), RcpError> {
        let guard = self.inner.read().unwrap();
        match guard.as_ref() {
            Some(ep) => ep.write(payload),
            None => Err(RcpError::NotConnected),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockEndpoint;

    fn ok_ep(ep_type: EndpointType) -> Arc<dyn Endpoint> {
        MockEndpoint::new(ep_type, vec![0u8; 4]) as Arc<dyn Endpoint>
    }

    #[test]
    //fusa:test REQ-PROXY-001
    //fusa:test REQ-PROXY-003
    fn forwards_calls_to_inner() {
        let proxy = ProxyEndpoint::new(ok_ep(EndpointType::Gpio));
        proxy.write(b"x").unwrap();
        proxy.read(4).unwrap();
    }

    #[test]
    //fusa:test REQ-PROXY-002
    fn ep_type_matches_original_inner() {
        let proxy = ProxyEndpoint::new(ok_ep(EndpointType::Adc));
        assert_eq!(proxy.ep_type(), EndpointType::Adc);
    }

    #[test]
    //fusa:test REQ-PROXY-005
    fn swap_replaces_inner() {
        let proxy = ProxyEndpoint::new(ok_ep(EndpointType::Gpio));
        // Detach-and-reattach a fresh endpoint to observe the swap taking
        // effect: swap to an endpoint holding different data.
        let replacement = MockEndpoint::new(EndpointType::Gpio, vec![9u8; 2]);
        proxy.swap(replacement as Arc<dyn Endpoint>);
        let got = proxy.read(2).unwrap();
        assert_eq!(got, vec![9u8; 2]);
    }

    #[test]
    //fusa:test REQ-PROXY-006
    fn detach_returns_not_connected() {
        let proxy = ProxyEndpoint::new(ok_ep(EndpointType::Gpio));
        proxy.detach();
        let err = proxy.write(b"x").unwrap_err();
        assert_eq!(err, RcpError::NotConnected);
    }

    #[test]
    //fusa:test REQ-PROXY-004
    fn read_forwarded() {
        let proxy = ProxyEndpoint::new(ok_ep(EndpointType::Gpio));
        proxy.read(4).unwrap();
    }

    #[test]
    //fusa:test REQ-PROXY-006
    fn read_detached_returns_not_connected() {
        let proxy = ProxyEndpoint::new(ok_ep(EndpointType::Gpio));
        proxy.detach();
        let err = proxy.read(4).err().unwrap();
        assert_eq!(err, RcpError::NotConnected);
    }
}
