// fusa:req REQ-MQTT-001
// fusa:req REQ-MQTT-002
// fusa:req REQ-MQTT-003
// fusa:req REQ-MQTT-004
// fusa:req REQ-MQTT-005

//! MQTT bridge — publishes RCP commands as MQTT messages and receives responses.
//!
//! Topic scheme:
//! - Request:  `rcp/{zone}/cmd/{id}`
//! - Response: `rcp/{zone}/resp/{id}`
//! - Status:   `rcp/{zone}/status`

use std::sync::Arc;
use std::time::Duration;

use crate::{Command, Controller, RcpError, Response, ResponseStatus, Subscription, Zone};

// ── MqttClient trait ──────────────────────────────────────────────────────────

/// Abstract MQTT client for bridge testability.
// fusa:req REQ-MQTT-001
pub trait MqttClient: Send + Sync {
    fn publish(&self, topic: &str, payload: &[u8]) -> Result<(), RcpError>;
    fn subscribe_topic(&self, topic: &str) -> Result<(), RcpError>;
    fn recv_message(&self, timeout: Option<Duration>) -> Result<(String, Vec<u8>), RcpError>;
}

// ── Topic helpers ─────────────────────────────────────────────────────────────

fn cmd_topic(zone: Zone, id: u32) -> String {
    format!("rcp/{}/cmd/{}", zone.0, id)
}

fn resp_topic(zone: Zone, id: u32) -> String {
    format!("rcp/{}/resp/{}", zone.0, id)
}

// ── MqttBridge ────────────────────────────────────────────────────────────────

/// RCP-over-MQTT bridge controller.
// fusa:req REQ-MQTT-002
pub struct MqttBridge {
    zone:   Zone,
    client: Arc<dyn MqttClient>,
}

impl MqttBridge {
    pub fn new(zone: Zone, client: Arc<dyn MqttClient>) -> Self {
        MqttBridge { zone, client }
    }
}

impl Controller for MqttBridge {
    fn zone(&self) -> Zone { self.zone }

    // fusa:req REQ-MQTT-003
    // fusa:req REQ-MQTT-004
    fn send(&self, cmd: &Command, timeout: Option<Duration>) -> Result<Response, RcpError> {
        if timeout == Some(Duration::ZERO) { return Err(RcpError::Timeout); }
        if cmd.zone != self.zone { return Err(RcpError::ZoneMismatch); }

        let payload = cmd.payload.as_deref().unwrap_or(&[]);
        let topic = cmd_topic(self.zone, cmd.id);
        self.client.publish(&topic, payload)?;
        self.client.subscribe_topic(&resp_topic(self.zone, cmd.id))?;
        let (_resp_topic, resp_payload) = self.client.recv_message(timeout)?;

        Ok(Response {
            command_id: cmd.id,
            zone:       self.zone,
            status:     if resp_payload.first() == Some(&0) { ResponseStatus::OK } else { ResponseStatus::ERROR },
            payload:    if resp_payload.len() > 1 { Some(resp_payload[1..].to_vec()) } else { None },
        })
    }

    // fusa:req REQ-MQTT-005
    fn subscribe(&self) -> Result<Subscription, RcpError> { Err(RcpError::NotFound) }

    fn close(&self) -> Result<(), RcpError> { Ok(()) }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Zone};
    use std::sync::Mutex;

    struct MockMqtt { published: Mutex<Vec<(String, Vec<u8>)>> }
    impl MockMqtt {
        fn new() -> Arc<Self> { Arc::new(MockMqtt { published: Mutex::new(vec![]) }) }
    }
    impl MqttClient for MockMqtt {
        fn publish(&self, topic: &str, payload: &[u8]) -> Result<(), RcpError> {
            self.published.lock().unwrap().push((topic.to_string(), payload.to_vec()));
            Ok(())
        }
        fn subscribe_topic(&self, _: &str) -> Result<(), RcpError> { Ok(()) }
        fn recv_message(&self, _: Option<Duration>) -> Result<(String, Vec<u8>), RcpError> {
            Ok(("resp".into(), vec![0u8])) // OK response
        }
    }

    #[test]
    // fusa:test REQ-MQTT-001
    // fusa:test REQ-MQTT-002
    // fusa:test REQ-MQTT-003
    fn bridge_publishes_command() {
        let client = MockMqtt::new();
        let bridge = MqttBridge::new(Zone::FRONT_LEFT, Arc::clone(&client) as Arc<dyn MqttClient>);
        let cmd = Command { id: 42, zone: Zone::FRONT_LEFT, ..Default::default() };
        let resp = bridge.send(&cmd, None).unwrap();
        assert_eq!(resp.status, ResponseStatus::OK);
        let pubs = client.published.lock().unwrap();
        assert!(pubs[0].0.contains("42"), "topic must include cmd id");
    }

    #[test]
    // fusa:test REQ-MQTT-004
    fn zone_mismatch_rejected() {
        let client = MockMqtt::new();
        let bridge = MqttBridge::new(Zone::FRONT_LEFT, Arc::clone(&client) as Arc<dyn MqttClient>);
        let err = bridge.send(&Command { zone: Zone::REAR_RIGHT, ..Default::default() }, None).unwrap_err();
        assert_eq!(err, RcpError::ZoneMismatch);
    }

    #[test]
    // fusa:test REQ-MQTT-005
    fn subscribe_returns_not_found() {
        let client = MockMqtt::new();
        let bridge = MqttBridge::new(Zone::FRONT_LEFT, Arc::clone(&client) as Arc<dyn MqttClient>);
        let err = bridge.subscribe().unwrap_err();
        assert_eq!(err, RcpError::NotFound);
    }
}
