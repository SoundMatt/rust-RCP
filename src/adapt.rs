// fusa:req REQ-ADAPT-001
// fusa:req REQ-ADAPT-002
// fusa:req REQ-ADAPT-003
// fusa:req REQ-ADAPT-004
// fusa:req REQ-ADAPT-005

//! Adapter layer — converts between RCP and external protocol representations.
//!
//! Provides bi-directional mapping between `Command`/`Response` and
//! arbitrary external message formats via the [`Adapter`] trait.

use std::sync::Arc;
use std::time::Duration;

use crate::{Command, Controller, RcpError, Response, Subscription, Zone};

// ── Adapter trait ─────────────────────────────────────────────────────────────

/// Converts between RCP messages and an external format `M`.
// fusa:req REQ-ADAPT-001
pub trait Adapter<M>: Send + Sync {
    /// Convert an external message to an RCP command.
    fn to_command(&self, msg: M) -> Result<Command, RcpError>;
    /// Convert an RCP response to the external message type.
    fn to_message(&self, resp: Response) -> Result<M, RcpError>;
}

// ── AdaptController ───────────────────────────────────────────────────────────

/// Controller wrapper that adapts an external message type `M` to RCP.
// fusa:req REQ-ADAPT-002
pub struct AdaptController<M> {
    inner: Arc<dyn Controller>,
    adapter: Arc<dyn Adapter<M>>,
}

impl<M: Send + Sync + 'static> AdaptController<M> {
    pub fn new(inner: Arc<dyn Controller>, adapter: Arc<dyn Adapter<M>>) -> Self {
        AdaptController { inner, adapter }
    }

    /// Send using the external message type.
    // fusa:req REQ-ADAPT-003
    pub fn send_msg(&self, msg: M, timeout: Option<Duration>) -> Result<M, RcpError> {
        let cmd = self.adapter.to_command(msg)?;
        let resp = self.inner.send(&cmd, timeout)?;
        self.adapter.to_message(resp)
    }
}

// ── Passthrough adapter ───────────────────────────────────────────────────────

/// Identity adapter for `Command` → `Command` testing.
// fusa:req REQ-ADAPT-004
pub struct PassthroughAdapter;

impl Adapter<Command> for PassthroughAdapter {
    fn to_command(&self, msg: Command) -> Result<Command, RcpError> {
        Ok(msg)
    }
    fn to_message(&self, resp: Response) -> Result<Command, RcpError> {
        Ok(Command {
            id: resp.command_id,
            zone: resp.zone,
            payload: resp.payload,
            ..Default::default()
        })
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

    fn ok_ctrl() -> Arc<dyn Controller> {
        let h: crate::mock::Handler = Box::new(|cmd| Response {
            command_id: cmd.id,
            zone: cmd.zone,
            status: ResponseStatus::OK,
            payload: cmd.payload.clone(),
        });
        MockController::new(Zone::FRONT_LEFT, Some(h)) as Arc<dyn Controller>
    }

    #[test]
    // fusa:test REQ-ADAPT-001
    // fusa:test REQ-ADAPT-004
    fn passthrough_adapter_identity() {
        let ctrl = AdaptController::new(ok_ctrl(), Arc::new(PassthroughAdapter));
        let cmd = Command {
            id: 5,
            zone: Zone::FRONT_LEFT,
            payload: Some(b"hi".to_vec()),
            ..Default::default()
        };
        let out = ctrl.send_msg(cmd.clone(), None).unwrap();
        assert_eq!(out.id, 5);
    }

    #[test]
    // fusa:test REQ-ADAPT-002
    fn zone_forwarded() {
        let inner = ok_ctrl();
        let ctrl = AdaptController::new(Arc::clone(&inner), Arc::new(PassthroughAdapter));
        assert_eq!(ctrl.inner.zone(), Zone::FRONT_LEFT);
    }

    #[test]
    // fusa:test REQ-ADAPT-003
    fn adapter_error_propagated() {
        struct FailAdapter;
        impl Adapter<Command> for FailAdapter {
            fn to_command(&self, _: Command) -> Result<Command, RcpError> {
                Err(RcpError::Other("bad msg".into()))
            }
            fn to_message(&self, _: Response) -> Result<Command, RcpError> {
                unreachable!()
            }
        }
        let ctrl = AdaptController::new(ok_ctrl(), Arc::new(FailAdapter));
        let err = ctrl.send_msg(Command::default(), None).unwrap_err();
        assert!(matches!(err, RcpError::Other(_)));
    }

    #[test]
    // fusa:test REQ-ADAPT-005
    fn passthrough_preserves_payload() {
        let ctrl = AdaptController::new(ok_ctrl(), Arc::new(PassthroughAdapter));
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            payload: Some(b"data".to_vec()),
            ..Default::default()
        };
        let out = ctrl.send_msg(cmd, None).unwrap();
        assert_eq!(out.payload, Some(b"data".to_vec()));
    }
}
