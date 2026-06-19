// fusa:req REQ-AUTHZ-001
// fusa:req REQ-AUTHZ-002
// fusa:req REQ-AUTHZ-003
// fusa:req REQ-AUTHZ-004
// fusa:req REQ-AUTHZ-005
// fusa:req REQ-AUTHZ-006
// fusa:req REQ-AUTHZ-007

//! Authorization policy enforcement on zone controllers.
//!
//! Implements an allowlist-based command-type ACL; disallowed commands return
//! `Err(RcpError::NotFound)` (maps to `relay::ErrNotConnected` per spec §5).

use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use crate::{Command, Controller, RcpError, Response, Subscription, Zone};

// ── Policy ────────────────────────────────────────────────────────────────────

/// Authorization policy for a zone controller.
// fusa:req REQ-AUTHZ-001
#[derive(Clone, Debug)]
pub struct Policy {
    /// Allowed command types. Empty = deny all.
    pub allowed_cmd_types: HashSet<u16>,
    /// Allowed priority range (min..=max by inner value).
    pub min_priority: u8,
    pub max_priority: u8,
}

impl Policy {
    /// Allow everything (open policy).
    // fusa:req REQ-AUTHZ-002
    pub fn allow_all() -> Self {
        let mut set = HashSet::new();
        for v in 0..=6u16 {
            set.insert(v);
        }
        Policy {
            allowed_cmd_types: set,
            min_priority: 0,
            max_priority: 2,
        }
    }

    /// Deny all commands (closed policy).
    // fusa:req REQ-AUTHZ-003
    pub fn deny_all() -> Self {
        Policy {
            allowed_cmd_types: HashSet::new(),
            min_priority: 0,
            max_priority: 2,
        }
    }

    pub fn is_allowed(&self, cmd: &Command) -> bool {
        self.allowed_cmd_types.contains(&cmd.cmd_type.0)
            && cmd.priority.0 >= self.min_priority
            && cmd.priority.0 <= self.max_priority
    }
}

// ── AuthzController ───────────────────────────────────────────────────────────

/// Policy-enforcing controller wrapper.
// fusa:req REQ-AUTHZ-004
pub struct AuthzController {
    inner: Arc<dyn Controller>,
    policy: RwLock<Policy>,
}

impl AuthzController {
    pub fn new(inner: Arc<dyn Controller>, policy: Policy) -> Self {
        AuthzController {
            inner,
            policy: RwLock::new(policy),
        }
    }

    /// Replace the active policy atomically.
    // fusa:req REQ-AUTHZ-006
    pub fn set_policy(&self, policy: Policy) {
        *self.policy.write().unwrap() = policy;
    }

    /// Snapshot of the current policy.
    pub fn policy(&self) -> Policy {
        self.policy.read().unwrap().clone()
    }
}

impl Controller for AuthzController {
    fn zone(&self) -> Zone {
        self.inner.zone()
    }

    // fusa:req REQ-AUTHZ-005
    fn send(&self, cmd: &Command, timeout: Option<Duration>) -> Result<Response, RcpError> {
        if !self.policy.read().unwrap().is_allowed(cmd) {
            return Err(RcpError::NotFound);
        }
        self.inner.send(cmd, timeout)
    }

    fn subscribe(&self) -> Result<Subscription, RcpError> {
        self.inner.subscribe()
    }

    // fusa:req REQ-AUTHZ-007
    fn close(&self) -> Result<(), RcpError> {
        self.inner.close()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockController;
    use crate::{Command, CommandType, Zone};

    fn inner() -> Arc<dyn Controller> {
        MockController::new(Zone::FRONT_LEFT, None) as Arc<dyn Controller>
    }

    #[test]
    // fusa:test REQ-AUTHZ-002
    // fusa:test REQ-AUTHZ-004
    // fusa:test REQ-AUTHZ-005
    fn allow_all_permits_any_command() {
        let a = AuthzController::new(inner(), Policy::allow_all());
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            cmd_type: CommandType::SET,
            ..Default::default()
        };
        a.send(&cmd, None).unwrap();
    }

    #[test]
    // fusa:test REQ-AUTHZ-003
    // fusa:test REQ-AUTHZ-005
    fn deny_all_blocks_every_command() {
        let a = AuthzController::new(inner(), Policy::deny_all());
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            cmd_type: CommandType::GET,
            ..Default::default()
        };
        let err = a.send(&cmd, None).unwrap_err();
        assert_eq!(err, RcpError::NotFound);
        assert!(err.is_relay_not_connected());
    }

    #[test]
    // fusa:test REQ-AUTHZ-001
    // fusa:test REQ-AUTHZ-005
    fn partial_allowlist_enforced() {
        let mut set = std::collections::HashSet::new();
        set.insert(CommandType::GET.0);
        let policy = Policy {
            allowed_cmd_types: set,
            min_priority: 0,
            max_priority: 2,
        };
        let a = AuthzController::new(inner(), policy);

        let get = Command {
            zone: Zone::FRONT_LEFT,
            cmd_type: CommandType::GET,
            ..Default::default()
        };
        let set = Command {
            zone: Zone::FRONT_LEFT,
            cmd_type: CommandType::SET,
            ..Default::default()
        };
        a.send(&get, None).unwrap();
        let err = a.send(&set, None).unwrap_err();
        assert_eq!(err, RcpError::NotFound);
    }

    #[test]
    // fusa:test REQ-AUTHZ-006
    fn set_policy_takes_effect_immediately() {
        let a = AuthzController::new(inner(), Policy::deny_all());
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            ..Default::default()
        };
        a.send(&cmd, None).unwrap_err();
        a.set_policy(Policy::allow_all());
        a.send(&cmd, None).unwrap();
    }

    #[test]
    // fusa:test REQ-AUTHZ-007
    fn close_forwarded() {
        let a = AuthzController::new(inner(), Policy::allow_all());
        a.close().unwrap();
    }
}
