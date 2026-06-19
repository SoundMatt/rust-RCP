// fusa:req REQ-REST-001
// fusa:req REQ-REST-002
// fusa:req REQ-REST-003
// fusa:req REQ-REST-004

//! REST/HTTP bridge — sends RCP commands via HTTP POST and parses JSON responses.
//!
//! Endpoint: `POST /rcp/v1/zones/{zone}/commands`

use std::sync::Arc;
use std::time::Duration;

use crate::{Command, Controller, RcpError, Response, ResponseStatus, Subscription, Zone};

// ── HttpClient trait ──────────────────────────────────────────────────────────

/// Abstract HTTP client for bridge testability.
// fusa:req REQ-REST-001
pub trait HttpClient: Send + Sync {
    fn post(
        &self,
        url: &str,
        body: &[u8],
        timeout: Option<Duration>,
    ) -> Result<(u16, Vec<u8>), RcpError>;
}

// ── JSON helpers ──────────────────────────────────────────────────────────────

fn cmd_to_json(cmd: &Command) -> Vec<u8> {
    let payload_hex = cmd
        .payload
        .as_ref()
        .map(|p| p.iter().map(|b| format!("{:02x}", b)).collect::<String>())
        .unwrap_or_default();
    format!(
        r#"{{"id":{},"zone":{},"cmd_type":{},"priority":{},"payload":"{}"}}"#,
        cmd.id, cmd.zone.0, cmd.cmd_type.0, cmd.priority.0, payload_hex
    )
    .into_bytes()
}

// ── RestBridge ────────────────────────────────────────────────────────────────

/// REST HTTP bridge controller.
// fusa:req REQ-REST-002
pub struct RestBridge {
    zone: Zone,
    client: Arc<dyn HttpClient>,
    base_url: String,
}

impl RestBridge {
    pub fn new(zone: Zone, client: Arc<dyn HttpClient>, base_url: impl Into<String>) -> Self {
        RestBridge {
            zone,
            client,
            base_url: base_url.into(),
        }
    }

    fn endpoint(&self) -> String {
        format!("{}/rcp/v1/zones/{}/commands", self.base_url, self.zone.0)
    }
}

impl Controller for RestBridge {
    fn zone(&self) -> Zone {
        self.zone
    }

    // fusa:req REQ-REST-003
    fn send(&self, cmd: &Command, timeout: Option<Duration>) -> Result<Response, RcpError> {
        if timeout == Some(Duration::ZERO) {
            return Err(RcpError::Timeout);
        }
        if cmd.zone != self.zone {
            return Err(RcpError::ZoneMismatch);
        }
        let body = cmd_to_json(cmd);
        let (status_code, _body) = self.client.post(&self.endpoint(), &body, timeout)?;
        let status = match status_code {
            200 | 201 | 202 => ResponseStatus::OK,
            408 | 504 => ResponseStatus::TIMEOUT,
            429 => ResponseStatus::BUSY,
            _ => ResponseStatus::ERROR,
        };
        Ok(Response {
            command_id: cmd.id,
            zone: self.zone,
            status,
            payload: None,
        })
    }

    fn subscribe(&self) -> Result<Subscription, RcpError> {
        Err(RcpError::NotFound)
    }

    // fusa:req REQ-REST-004
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

    struct Mock200;
    impl HttpClient for Mock200 {
        fn post(&self, _: &str, _: &[u8], _: Option<Duration>) -> Result<(u16, Vec<u8>), RcpError> {
            Ok((200, vec![]))
        }
    }

    #[test]
    // fusa:test REQ-REST-001
    // fusa:test REQ-REST-002
    // fusa:test REQ-REST-003
    fn rest_bridge_send_ok() {
        let b = RestBridge::new(Zone::FRONT_LEFT, Arc::new(Mock200), "http://localhost:8080");
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
    // fusa:test REQ-REST-003
    fn zone_mismatch() {
        let b = RestBridge::new(Zone::FRONT_LEFT, Arc::new(Mock200), "http://localhost:8080");
        let err = b
            .send(
                &Command {
                    zone: Zone::REAR_LEFT,
                    ..Default::default()
                },
                None,
            )
            .unwrap_err();
        assert_eq!(err, RcpError::ZoneMismatch);
    }

    #[test]
    // fusa:test REQ-REST-003
    fn http_429_maps_to_busy() {
        struct Mock429;
        impl HttpClient for Mock429 {
            fn post(
                &self,
                _: &str,
                _: &[u8],
                _: Option<Duration>,
            ) -> Result<(u16, Vec<u8>), RcpError> {
                Ok((429, vec![]))
            }
        }
        let b = RestBridge::new(Zone::FRONT_LEFT, Arc::new(Mock429), "http://localhost");
        let resp = b
            .send(
                &Command {
                    zone: Zone::FRONT_LEFT,
                    ..Default::default()
                },
                None,
            )
            .unwrap();
        assert_eq!(resp.status, ResponseStatus::BUSY);
    }

    #[test]
    // fusa:test REQ-REST-004
    fn close_is_noop() {
        let b = RestBridge::new(Zone::FRONT_LEFT, Arc::new(Mock200), "http://localhost");
        assert!(b.close().is_ok());
        assert!(b.close().is_ok());
    }
}
