//fusa:req REQ-AUTHZ-001
//fusa:req REQ-AUTHZ-002
//fusa:req REQ-AUTHZ-003
//fusa:req REQ-AUTHZ-004
//fusa:req REQ-AUTHZ-005
//fusa:req REQ-AUTHZ-006
//fusa:req REQ-AUTHZ-007

//! Authorization policy enforcement over an [`Endpoint`].
//!
//! Implements an allowlist-based (endpoint-type, request-type) ACL;
//! disallowed calls return `Err(RcpError::NotFound)` (maps to
//! `relay::ErrNotConnected` per spec §5).
//!
//! `ROADMAP.md` Milestone 9 ("All ADAPT-disposition packages retargeted...")
//! cutover: per this module's own ADAPT disposition ("generic ACL
//! decorator; retarget its key space from `CommandType` to
//! endpoint-type/request-type"), [`AuthzEndpoint`] replaces the legacy
//! `AuthzController`, wrapping [`crate::mock::Endpoint`] instead of
//! `Controller`. [`Policy::allowed`] is now keyed on `(`[`EndpointType`]
//! `as u8, is_write: bool)` pairs — `is_write` mirrors
//! [`crate::acf::ByteMessageInfo::op`]'s own true-is-write convention (see
//! `src/mock.rs`'s `RcServer::handle_abb`) — replacing the old
//! `allowed_cmd_types: HashSet<u16>` keyed on the deleted `CommandType`.
//! The old `min_priority`/`max_priority` range has no surviving analog:
//! `Endpoint` requests carry no `Priority` field at all (see
//! `ratelimit`'s own retargeting note for the same gap), so it is dropped
//! rather than force-mapped onto unrelated behavior.

use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use crate::mock::Endpoint;
use crate::regmap::EndpointType;
use crate::RcpError;

// ── Policy ────────────────────────────────────────────────────────────────────

/// Authorization policy for an endpoint: an allowlist of
/// `(ep_type, is_write)` pairs. Empty = deny all.
//fusa:req REQ-AUTHZ-001
#[derive(Clone, Debug, Default)]
pub struct Policy {
    pub allowed: HashSet<(u8, bool)>,
}

/// Every `ep_type` byte this crate's [`EndpointType::from_u8`] recognizes
/// (`0x01..=0x0D`), per that function's own doc comment.
const ALL_EP_TYPE_BYTES: std::ops::RangeInclusive<u8> = 0x01..=0x0D;

impl Policy {
    /// Allow every recognized endpoint type, for both reads and writes.
    //fusa:req REQ-AUTHZ-002
    pub fn allow_all() -> Self {
        let mut set = HashSet::new();
        for b in ALL_EP_TYPE_BYTES {
            set.insert((b, false));
            set.insert((b, true));
        }
        Policy { allowed: set }
    }

    /// Deny everything (closed policy).
    //fusa:req REQ-AUTHZ-003
    pub fn deny_all() -> Self {
        Policy::default()
    }

    pub fn is_allowed(&self, ep_type: EndpointType, is_write: bool) -> bool {
        self.allowed.contains(&(ep_type.to_u8(), is_write))
    }
}

// ── AuthzEndpoint ──────────────────────────────────────────────────────────────

/// Policy-enforcing endpoint wrapper.
//fusa:req REQ-AUTHZ-004
pub struct AuthzEndpoint {
    inner: Arc<dyn Endpoint>,
    policy: RwLock<Policy>,
}

impl AuthzEndpoint {
    pub fn new(inner: Arc<dyn Endpoint>, policy: Policy) -> Self {
        AuthzEndpoint {
            inner,
            policy: RwLock::new(policy),
        }
    }

    /// Replace the active policy atomically.
    //fusa:req REQ-AUTHZ-006
    pub fn set_policy(&self, policy: Policy) {
        *self.policy.write().unwrap() = policy;
    }

    /// Snapshot of the current policy.
    pub fn policy(&self) -> Policy {
        self.policy.read().unwrap().clone()
    }
}

impl Endpoint for AuthzEndpoint {
    fn ep_type(&self) -> EndpointType {
        self.inner.ep_type()
    }

    //fusa:req REQ-AUTHZ-005
    fn read(&self, read_size: u16) -> Result<Vec<u8>, RcpError> {
        let ep_type = self.inner.ep_type();
        if !self.policy.read().unwrap().is_allowed(ep_type, false) {
            return Err(RcpError::NotFound);
        }
        self.inner.read(read_size)
    }

    //fusa:req REQ-AUTHZ-005
    //fusa:req REQ-AUTHZ-007
    fn write(&self, payload: &[u8]) -> Result<(), RcpError> {
        let ep_type = self.inner.ep_type();
        if !self.policy.read().unwrap().is_allowed(ep_type, true) {
            return Err(RcpError::NotFound);
        }
        self.inner.write(payload)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockEndpoint;

    fn inner(ep_type: EndpointType) -> Arc<dyn Endpoint> {
        MockEndpoint::new(ep_type, vec![0u8; 4]) as Arc<dyn Endpoint>
    }

    #[test]
    //fusa:test REQ-AUTHZ-002
    //fusa:test REQ-AUTHZ-004
    //fusa:test REQ-AUTHZ-005
    fn allow_all_permits_any_call() {
        let a = AuthzEndpoint::new(inner(EndpointType::Gpio), Policy::allow_all());
        a.write(b"x").unwrap();
        a.read(4).unwrap();
    }

    #[test]
    //fusa:test REQ-AUTHZ-003
    //fusa:test REQ-AUTHZ-005
    fn deny_all_blocks_every_call() {
        let a = AuthzEndpoint::new(inner(EndpointType::Gpio), Policy::deny_all());
        let err = a.read(4).unwrap_err();
        assert_eq!(err, RcpError::NotFound);
        assert!(err.is_relay_not_connected());
    }

    #[test]
    //fusa:test REQ-AUTHZ-001
    //fusa:test REQ-AUTHZ-005
    fn partial_allowlist_enforced_by_request_type() {
        let mut set = HashSet::new();
        set.insert((EndpointType::Gpio.to_u8(), false)); // reads only
        let policy = Policy { allowed: set };
        let a = AuthzEndpoint::new(inner(EndpointType::Gpio), policy);

        a.read(4).unwrap();
        let err = a.write(b"x").unwrap_err();
        assert_eq!(err, RcpError::NotFound);
    }

    #[test]
    //fusa:test REQ-AUTHZ-001
    fn partial_allowlist_enforced_by_ep_type() {
        let mut set = HashSet::new();
        set.insert((EndpointType::Adc.to_u8(), false));
        let policy = Policy { allowed: set };
        let a = AuthzEndpoint::new(inner(EndpointType::Gpio), policy);

        let err = a.read(4).unwrap_err();
        assert_eq!(err, RcpError::NotFound);
    }

    #[test]
    //fusa:test REQ-AUTHZ-006
    fn set_policy_takes_effect_immediately() {
        let a = AuthzEndpoint::new(inner(EndpointType::Gpio), Policy::deny_all());
        a.write(b"x").unwrap_err();
        a.set_policy(Policy::allow_all());
        a.write(b"x").unwrap();
    }

    #[test]
    //fusa:test REQ-AUTHZ-007
    fn ep_type_forwarded() {
        let a = AuthzEndpoint::new(inner(EndpointType::Gpio), Policy::allow_all());
        assert_eq!(a.ep_type(), EndpointType::Gpio);
    }
}
