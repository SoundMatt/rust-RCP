// fusa:req REQ-ZG-001
// fusa:req REQ-ZG-002
// fusa:req REQ-ZG-003
// fusa:req REQ-ZG-004
// fusa:req REQ-ZG-005
// fusa:req REQ-ZG-006
// fusa:req REQ-ZG-007

//! Zone group — broadcast a command to multiple zone controllers in parallel.
//!
//! All responses are collected; the call succeeds only if every member succeeds.

use std::sync::Arc;
use std::time::Duration;

use crate::{Command, Controller, RcpError, Response, Subscription, Zone};

// ── ZoneGroup ─────────────────────────────────────────────────────────────────

/// Broadcasts commands to a fixed set of zone controllers.
// fusa:req REQ-ZG-001
pub struct ZoneGroup {
    members: Vec<Arc<dyn Controller>>,
    zone: Zone,
}

impl ZoneGroup {
    /// Create a group from a list of controllers.
    ///
    /// `zone` is the logical zone identifier reported by this group.
    // fusa:req REQ-ZG-002
    pub fn new(zone: Zone, members: Vec<Arc<dyn Controller>>) -> Self {
        ZoneGroup { zone, members }
    }

    /// The member controllers in this group.
    pub fn members(&self) -> &[Arc<dyn Controller>] {
        &self.members
    }
}

impl Controller for ZoneGroup {
    fn zone(&self) -> Zone {
        self.zone
    }

    /// Send a command to all members in parallel (via scoped threads).
    ///
    /// Returns the first error encountered; all other threads still run to
    /// completion to avoid leaving members in divergent state.
    // fusa:req REQ-ZG-003
    // fusa:req REQ-ZG-004
    fn send(&self, cmd: &Command, timeout: Option<Duration>) -> Result<Response, RcpError> {
        let results: Vec<Result<Response, RcpError>> = std::thread::scope(|s| {
            let handles: Vec<_> = self
                .members
                .iter()
                .map(|m| {
                    let cmd = cmd.clone();
                    s.spawn(move || m.send(&cmd, timeout))
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });

        let mut first_err: Option<RcpError> = None;
        let mut last_ok: Option<Response> = None;
        for r in results {
            match r {
                Ok(resp) => {
                    last_ok = Some(resp);
                }
                Err(e) => {
                    if first_err.is_none() {
                        first_err = Some(e);
                    }
                }
            }
        }
        if let Some(e) = first_err {
            return Err(e);
        }
        Ok(last_ok.unwrap_or_default())
    }

    // fusa:req REQ-ZG-005
    fn subscribe(&self) -> Result<Subscription, RcpError> {
        // Subscribe to the first member for group-level status events.
        self.members.first().ok_or(RcpError::NotFound)?.subscribe()
    }

    // fusa:req REQ-ZG-006
    fn close(&self) -> Result<(), RcpError> {
        let mut first_err: Option<RcpError> = None;
        for m in &self.members {
            if let Err(e) = m.close() {
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
        }
        first_err.map_or(Ok(()), Err)
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
    use std::sync::atomic::{AtomicU32, Ordering};

    fn ok_ctrl(zone: Zone) -> Arc<dyn Controller> {
        let h: crate::mock::Handler = Box::new(move |cmd| Response {
            command_id: cmd.id,
            zone: cmd.zone,
            status: ResponseStatus::OK,
            payload: None,
        });
        MockController::new(zone, Some(h)) as Arc<dyn Controller>
    }

    #[test]
    // fusa:test REQ-ZG-001
    // fusa:test REQ-ZG-002
    fn zone_is_reported_correctly() {
        let g = ZoneGroup::new(Zone::CENTRAL, vec![ok_ctrl(Zone::FRONT_LEFT)]);
        assert_eq!(g.zone(), Zone::CENTRAL);
    }

    #[test]
    // fusa:test REQ-ZG-003
    fn send_broadcasts_to_all_members() {
        let count = Arc::new(AtomicU32::new(0));
        let c1 = count.clone();
        let c2 = count.clone();
        let h1: crate::mock::Handler = Box::new(move |cmd| {
            c1.fetch_add(1, Ordering::SeqCst);
            Response {
                command_id: cmd.id,
                zone: cmd.zone,
                status: ResponseStatus::OK,
                payload: None,
            }
        });
        let h2: crate::mock::Handler = Box::new(move |cmd| {
            c2.fetch_add(1, Ordering::SeqCst);
            Response {
                command_id: cmd.id,
                zone: cmd.zone,
                status: ResponseStatus::OK,
                payload: None,
            }
        });
        let m1 = MockController::new(Zone::FRONT_LEFT, Some(h1)) as Arc<dyn Controller>;
        let m2 = MockController::new(Zone::FRONT_RIGHT, Some(h2)) as Arc<dyn Controller>;
        let g = ZoneGroup::new(Zone::CENTRAL, vec![m1, m2]);
        g.send(
            &Command {
                zone: Zone::CENTRAL,
                ..Default::default()
            },
            None,
        )
        .unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 2);
    }

    #[test]
    // fusa:test REQ-ZG-004
    fn first_member_error_is_propagated() {
        let _err_ctrl: crate::mock::Handler = Box::new(|_cmd| Response {
            command_id: 0,
            zone: Zone::FRONT_LEFT,
            status: ResponseStatus::ERROR,
            payload: None,
        });
        // Inject via fault route: build a mock that returns Err
        let h: crate::mock::Handler = Box::new(|_| Response {
            command_id: 0,
            zone: Zone::FRONT_LEFT,
            status: ResponseStatus::OK,
            payload: None,
        });
        let m1 = MockController::new(
            Zone::FRONT_LEFT,
            Some(Box::new(|_cmd| Response {
                command_id: 0,
                zone: Zone::FRONT_LEFT,
                status: ResponseStatus::OK,
                payload: None,
            })),
        ) as Arc<dyn Controller>;
        // Close m1 so its send returns Err(Closed)
        m1.close().unwrap();
        let m2 = MockController::new(Zone::FRONT_RIGHT, Some(h)) as Arc<dyn Controller>;
        let g = ZoneGroup::new(Zone::CENTRAL, vec![m1, m2]);
        let err = g
            .send(
                &Command {
                    zone: Zone::CENTRAL,
                    ..Default::default()
                },
                None,
            )
            .unwrap_err();
        assert_eq!(err, RcpError::Closed);
    }

    #[test]
    // fusa:test REQ-ZG-006
    fn close_closes_all_members() {
        let m1 = ok_ctrl(Zone::FRONT_LEFT);
        let m2 = ok_ctrl(Zone::FRONT_RIGHT);
        let g = ZoneGroup::new(Zone::CENTRAL, vec![m1, m2]);
        g.close().unwrap();
    }

    #[test]
    // fusa:test REQ-ZG-005
    fn subscribe_delegates_to_first_member() {
        let m = ok_ctrl(Zone::FRONT_LEFT);
        let g = ZoneGroup::new(Zone::CENTRAL, vec![m]);
        // MockController supports subscribe; should succeed
        assert!(g.subscribe().is_ok());
    }

    #[test]
    // fusa:test REQ-ZG-007
    fn empty_group_send_returns_default_ok() {
        let g = ZoneGroup::new(Zone::CENTRAL, vec![]);
        let resp = g
            .send(
                &Command {
                    zone: Zone::CENTRAL,
                    ..Default::default()
                },
                None,
            )
            .unwrap();
        assert_eq!(resp.status, ResponseStatus::OK);
    }
}
