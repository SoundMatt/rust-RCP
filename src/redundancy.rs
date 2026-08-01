//fusa:req REQ-RED-001
//fusa:req REQ-RED-002
//fusa:req REQ-RED-003
//fusa:req REQ-RED-004
//fusa:req REQ-RED-005
//fusa:req REQ-RED-006
//fusa:req REQ-RED-007
//fusa:req REQ-RED-008

//! Redundant endpoint pair with automatic failover (1-of-2 hot standby).
//!
//! All calls are dispatched to the primary. On primary failure, the
//! secondary is promoted and becomes the new primary.
//!
//! `ROADMAP.md` Milestone 9 ("All ADAPT-disposition packages retargeted...")
//! cutover: per this module's own ADAPT disposition ("generic 1-of-2
//! failover decorator; retarget to the new trait once defined"),
//! [`RedundancyEndpoint`] replaces the legacy `RedundancyController`,
//! wrapping [`crate::mock::Endpoint`] instead of `Controller`. Unlike the
//! old type, this one has no `close()` to forward on failover (`Endpoint`
//! defines none — see `src/mock.rs`'s own doc comment for why), so the
//! demoted primary is simply dropped once replaced, rather than
//! explicitly closed first; since this type is a plain `Arc<dyn Endpoint>`
//! holder, if it held the sole reference, the demoted endpoint goes out of
//! scope (and is deallocated) at that point.

use std::sync::{Arc, Mutex};

use crate::mock::Endpoint;
use crate::regmap::EndpointType;
use crate::RcpError;

// ── RedundancyEndpoint ────────────────────────────────────────────────────────

struct Inner {
    primary: Arc<dyn Endpoint>,
    secondary: Option<Arc<dyn Endpoint>>,
    failovers: u32,
}

/// Hot-standby redundant endpoint.
//fusa:req REQ-RED-001
pub struct RedundancyEndpoint {
    ep_type: EndpointType,
    state: Mutex<Inner>,
}

impl RedundancyEndpoint {
    /// Create with a primary and a secondary endpoint.
    //fusa:req REQ-RED-002
    pub fn new(primary: Arc<dyn Endpoint>, secondary: Arc<dyn Endpoint>) -> Self {
        let ep_type = primary.ep_type();
        RedundancyEndpoint {
            ep_type,
            state: Mutex::new(Inner {
                primary,
                secondary: Some(secondary),
                failovers: 0,
            }),
        }
    }

    /// Number of times failover has occurred.
    //fusa:req REQ-RED-006
    pub fn failover_count(&self) -> u32 {
        self.state.lock().unwrap().failovers
    }

    /// True if a secondary is still available.
    //fusa:req REQ-RED-007
    pub fn has_secondary(&self) -> bool {
        self.state.lock().unwrap().secondary.is_some()
    }

    /// Run `op` against the primary; on error, promote the secondary (if
    /// any) and retry once.
    fn dispatch<T>(
        &self,
        op: impl Fn(&dyn Endpoint) -> Result<T, RcpError>,
    ) -> Result<T, RcpError> {
        let result = {
            let g = self.state.lock().unwrap();
            op(g.primary.as_ref())
        };

        match result {
            Ok(v) => Ok(v),
            Err(primary_err) => {
                let mut g = self.state.lock().unwrap();
                match g.secondary.take() {
                    None => Err(primary_err),
                    Some(sec) => {
                        // Promote secondary.
                        g.primary = sec;
                        g.failovers += 1;
                        let primary = Arc::clone(&g.primary);
                        drop(g);
                        // Retry on new primary.
                        op(primary.as_ref())
                    }
                }
            }
        }
    }
}

impl Endpoint for RedundancyEndpoint {
    fn ep_type(&self) -> EndpointType {
        self.ep_type
    }

    //fusa:req REQ-RED-003
    //fusa:req REQ-RED-004
    //fusa:req REQ-RED-005
    fn read(&self, read_size: u16) -> Result<Vec<u8>, RcpError> {
        self.dispatch(|ep| ep.read(read_size))
    }

    //fusa:req REQ-RED-008
    fn write(&self, payload: &[u8]) -> Result<(), RcpError> {
        self.dispatch(|ep| ep.write(payload))
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

    struct AlwaysFail;
    impl Endpoint for AlwaysFail {
        fn ep_type(&self) -> EndpointType {
            EndpointType::Gpio
        }
        fn read(&self, _read_size: u16) -> Result<Vec<u8>, RcpError> {
            Err(RcpError::Closed)
        }
        fn write(&self, _payload: &[u8]) -> Result<(), RcpError> {
            Err(RcpError::Closed)
        }
    }

    fn failing_ep() -> Arc<dyn Endpoint> {
        Arc::new(AlwaysFail) as Arc<dyn Endpoint>
    }

    #[test]
    //fusa:test REQ-RED-001
    //fusa:test REQ-RED-003
    fn primary_success_no_failover() {
        let r = RedundancyEndpoint::new(ok_ep(EndpointType::Gpio), ok_ep(EndpointType::Gpio));
        r.write(b"x").unwrap();
        assert_eq!(r.failover_count(), 0);
    }

    #[test]
    //fusa:test REQ-RED-004
    //fusa:test REQ-RED-005
    fn primary_failure_triggers_failover_to_secondary() {
        let r = RedundancyEndpoint::new(failing_ep(), ok_ep(EndpointType::Gpio));
        r.write(b"x").unwrap();
        assert_eq!(r.failover_count(), 1);
    }

    #[test]
    //fusa:test REQ-RED-006
    fn failover_count_increments() {
        let r = RedundancyEndpoint::new(failing_ep(), ok_ep(EndpointType::Gpio));
        r.write(b"x").unwrap(); // triggers failover
        assert_eq!(r.failover_count(), 1);
        // Secondary is now primary; no more secondary
        assert!(!r.has_secondary());
    }

    #[test]
    //fusa:test REQ-RED-007
    fn no_secondary_after_failover() {
        let r = RedundancyEndpoint::new(failing_ep(), ok_ep(EndpointType::Gpio));
        assert!(r.has_secondary());
        r.write(b"x").unwrap();
        assert!(!r.has_secondary());
    }

    #[test]
    //fusa:test REQ-RED-005
    fn both_failed_returns_error() {
        let r = RedundancyEndpoint::new(failing_ep(), failing_ep());
        let err = r.write(b"x").unwrap_err();
        assert_eq!(err, RcpError::Closed);
    }

    #[test]
    //fusa:test REQ-RED-002
    fn ep_type_matches_primary() {
        let r = RedundancyEndpoint::new(ok_ep(EndpointType::Adc), ok_ep(EndpointType::Adc));
        assert_eq!(r.ep_type(), EndpointType::Adc);
    }

    #[test]
    //fusa:test REQ-RED-008
    fn read_forwarded_to_primary() {
        let r = RedundancyEndpoint::new(ok_ep(EndpointType::Gpio), ok_ep(EndpointType::Gpio));
        r.read(4).unwrap();
    }
}
