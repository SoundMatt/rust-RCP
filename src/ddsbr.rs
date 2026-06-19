// fusa:req REQ-DDS-001
// fusa:req REQ-DDS-002
// fusa:req REQ-DDS-003
// fusa:req REQ-DDS-004

//! DDS (Data Distribution Service) bridge — integrates RCP with AUTOSAR Adaptive
//! via DDS topics. Only the topic-mapping layer is implemented here; the DDS
//! middleware is injected via the [`DdsParticipant`] trait.

use std::sync::Arc;
use std::time::Duration;

use crate::{Command, Controller, RcpError, Response, ResponseStatus, Subscription, Zone};

// ── DdsParticipant trait ──────────────────────────────────────────────────────

/// Abstract DDS participant interface.
// fusa:req REQ-DDS-001
pub trait DdsParticipant: Send + Sync {
    fn write(&self, topic: &str, data: &[u8]) -> Result<(), RcpError>;
    fn take(&self, topic: &str, timeout: Option<Duration>) -> Result<Vec<u8>, RcpError>;
}

// ── DDsBridge ─────────────────────────────────────────────────────────────────

/// RCP-over-DDS bridge controller.
// fusa:req REQ-DDS-002
pub struct DdsBridge {
    zone: Zone,
    participant: Arc<dyn DdsParticipant>,
}

impl DdsBridge {
    pub fn new(zone: Zone, participant: Arc<dyn DdsParticipant>) -> Self {
        DdsBridge { zone, participant }
    }

    fn cmd_topic(&self) -> String {
        format!("rcp.zone{}.cmd", self.zone.0)
    }
    fn resp_topic(&self) -> String {
        format!("rcp.zone{}.resp", self.zone.0)
    }
}

impl Controller for DdsBridge {
    fn zone(&self) -> Zone {
        self.zone
    }

    // fusa:req REQ-DDS-003
    fn send(&self, cmd: &Command, timeout: Option<Duration>) -> Result<Response, RcpError> {
        if timeout == Some(Duration::ZERO) {
            return Err(RcpError::Timeout);
        }
        if cmd.zone != self.zone {
            return Err(RcpError::ZoneMismatch);
        }
        self.participant
            .write(&self.cmd_topic(), cmd.payload.as_deref().unwrap_or(&[]))?;
        let data = self.participant.take(&self.resp_topic(), timeout)?;
        Ok(Response {
            command_id: cmd.id,
            zone: self.zone,
            status: if data.first() == Some(&0) {
                ResponseStatus::OK
            } else {
                ResponseStatus::ERROR
            },
            payload: None,
        })
    }

    fn subscribe(&self) -> Result<Subscription, RcpError> {
        Err(RcpError::NotFound)
    }

    // fusa:req REQ-DDS-004
    fn close(&self) -> Result<(), RcpError> {
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Zone};

    struct MockDds;
    impl DdsParticipant for MockDds {
        fn write(&self, _: &str, _: &[u8]) -> Result<(), RcpError> {
            Ok(())
        }
        fn take(&self, _: &str, _: Option<Duration>) -> Result<Vec<u8>, RcpError> {
            Ok(vec![0])
        }
    }

    #[test]
    // fusa:test REQ-DDS-001
    // fusa:test REQ-DDS-002
    // fusa:test REQ-DDS-003
    fn dds_send_ok() {
        let b = DdsBridge::new(Zone::FRONT_LEFT, Arc::new(MockDds));
        let resp = b
            .send(
                &Command {
                    zone: Zone::FRONT_LEFT,
                    ..Default::default()
                },
                None,
            )
            .unwrap();
        assert_eq!(resp.status, ResponseStatus::OK);
    }

    #[test]
    // fusa:test REQ-DDS-003
    fn zone_mismatch() {
        let b = DdsBridge::new(Zone::FRONT_LEFT, Arc::new(MockDds));
        let err = b
            .send(
                &Command {
                    zone: Zone::REAR_RIGHT,
                    ..Default::default()
                },
                None,
            )
            .unwrap_err();
        assert_eq!(err, RcpError::ZoneMismatch);
    }

    #[test]
    // fusa:test REQ-DDS-004
    fn close_is_noop() {
        let b = DdsBridge::new(Zone::FRONT_LEFT, Arc::new(MockDds));
        assert!(b.close().is_ok());
        assert!(b.close().is_ok());
    }
}
