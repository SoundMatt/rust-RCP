// fusa:req REQ-RELAY-001
// fusa:req REQ-RELAY-002
// fusa:req REQ-RELAY-003
// fusa:req REQ-RELAY-004
// fusa:req REQ-RELAY-005
// fusa:req REQ-RELAY-006
// fusa:req REQ-RELAY-007
// fusa:req REQ-RELAY-008

//! RELAY protocol types, bundled locally until a published `relay-rs` crate
//! exists to depend on directly.
//!
//! These types mirror the RELAY spec v1.11 definitions for Rust (§18.3):
//! the universal message envelope, the four mandatory error sentinels, and
//! the [`Node`]/[`Caller`] application interfaces (§10.1/§10.2).

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;
use tokio::sync::mpsc;

// ── Protocol ──────────────────────────────────────────────────────────────────

/// Protocol identifier per RELAY spec §3. Serialises as its integer value.
// fusa:req REQ-RELAY-001
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    Can = 1,
    Dds = 2,
    Lin = 3,
    Mqtt = 4,
    Rcp = 5,
    Someip = 6,
}

impl From<Protocol> for i32 {
    fn from(p: Protocol) -> i32 {
        p as i32
    }
}

impl TryFrom<i32> for Protocol {
    type Error = String;
    fn try_from(v: i32) -> Result<Self, Self::Error> {
        match v {
            1 => Ok(Protocol::Can),
            2 => Ok(Protocol::Dds),
            3 => Ok(Protocol::Lin),
            4 => Ok(Protocol::Mqtt),
            5 => Ok(Protocol::Rcp),
            6 => Ok(Protocol::Someip),
            _ => Err(format!("unknown protocol: {v}")),
        }
    }
}

impl Serialize for Protocol {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_i32((*self).into())
    }
}

impl<'de> Deserialize<'de> for Protocol {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = i32::deserialize(d)?;
        Protocol::try_from(v).map_err(serde::de::Error::custom)
    }
}

impl std::fmt::Display for Protocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Protocol::Can => "CAN",
            Protocol::Dds => "DDS",
            Protocol::Lin => "LIN",
            Protocol::Mqtt => "MQTT",
            Protocol::Rcp => "RCP",
            Protocol::Someip => "SOMEIP",
        };
        f.write_str(s)
    }
}

// ── Version ───────────────────────────────────────────────────────────────────

/// Semantic version triplet per RELAY spec §4.1.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Version {
    pub major: i32,
    pub minor: i32,
    pub patch: i32,
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

// ── Message ───────────────────────────────────────────────────────────────────

/// Universal message envelope per RELAY spec §4.
// fusa:req REQ-RELAY-002
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub protocol: Protocol,
    pub version: Version,
    pub id: String,
    #[serde(with = "crate::base64_serde")]
    pub payload: Vec<u8>,
    /// RFC 3339 nanosecond timestamp.
    pub timestamp: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub seq: u64,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub meta: BTreeMap<String, String>,
}

fn is_zero_u64(v: &u64) -> bool {
    *v == 0
}

impl Message {
    /// Create a new message with the given protocol, id, and payload.
    pub fn new(protocol: Protocol, id: impl Into<String>, payload: Vec<u8>) -> Self {
        Self {
            protocol,
            version: Version::default(),
            id: id.into(),
            payload,
            timestamp: Utc::now(),
            seq: 0,
            meta: BTreeMap::new(),
        }
    }
}

// ── Back-pressure policy ──────────────────────────────────────────────────────

/// Back-pressure policy for subscriber channels per RELAY spec §14.
// fusa:req REQ-RELAY-003
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BackPressurePolicy {
    /// Drop the arriving message when the channel is full (default).
    #[default]
    DropNewest,
    /// Drop the oldest buffered message to make room.
    DropOldest,
    /// Block until space is available (use only with fast consumers).
    Block,
}

// ── SubscriberOptions ─────────────────────────────────────────────────────────

/// Subscriber channel configuration per RELAY spec §18.3.
// fusa:req REQ-RELAY-004
#[derive(Clone, Debug, Default)]
pub struct SubscriberOptions {
    /// Buffer depth; 0 means use the default (64).
    pub channel_depth: usize,
    /// Back-pressure policy applied when the channel is full.
    pub back_pressure: BackPressurePolicy,
}

impl SubscriberOptions {
    /// Return the effective channel depth, falling back to `default_depth`
    /// when `channel_depth` is zero.
    pub fn chan_depth(&self, default_depth: usize) -> usize {
        if self.channel_depth > 0 {
            self.channel_depth
        } else {
            default_depth
        }
    }
}

// ── Error ─────────────────────────────────────────────────────────────────────

/// The four mandatory RELAY error sentinels per §5.1.
// fusa:req REQ-RELAY-005
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum Error {
    #[error("relay: closed")]
    Closed,
    #[error("relay: not connected")]
    NotConnected,
    #[error("relay: timeout")]
    Timeout,
    #[error("relay: payload too large")]
    PayloadTooLarge,
}

// ── Context ───────────────────────────────────────────────────────────────────

/// Lightweight context carrying an optional deadline per RELAY spec §18.3.
// fusa:req REQ-RELAY-006
#[derive(Clone, Debug)]
pub struct Context {
    pub deadline: Option<Instant>,
}

impl Context {
    /// A background context with no deadline.
    pub fn background() -> Self {
        Self { deadline: None }
    }

    /// A context that expires after `d`.
    pub fn with_timeout(d: Duration) -> Self {
        Self {
            deadline: Some(Instant::now() + d),
        }
    }

    /// Returns true if the deadline has passed.
    pub fn done(&self) -> bool {
        self.deadline.is_some_and(|d| Instant::now() >= d)
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::background()
    }
}

// ── Node and Caller traits ────────────────────────────────────────────────────

/// Protocol-agnostic pub/sub interface per RELAY spec §10.1.
// fusa:req REQ-RELAY-007
#[async_trait]
pub trait Node: Send + Sync {
    fn protocol(&self) -> Protocol;

    async fn send(&self, ctx: Context, msg: Message) -> Result<(), Error>;

    async fn subscribe(&self, opts: SubscriberOptions) -> Result<mpsc::Receiver<Message>, Error>;

    async fn close(&self) -> Result<(), Error>;
}

/// Extends [`Node`] with request/response semantics per RELAY spec §10.2.
// fusa:req REQ-RELAY-008
#[async_trait]
pub trait Caller: Node {
    async fn call(&self, ctx: Context, req: Message) -> Result<Message, Error>;
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    // fusa:test REQ-RELAY-001
    fn protocol_display() {
        assert_eq!(Protocol::Rcp.to_string(), "RCP");
        assert_eq!(Protocol::Can.to_string(), "CAN");
    }

    #[test]
    // fusa:test REQ-RELAY-001
    fn protocol_serde_roundtrip() {
        let p = Protocol::Rcp;
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(json, "5");
        let p2: Protocol = serde_json::from_str(&json).unwrap();
        assert_eq!(p, p2);
    }

    #[test]
    // fusa:test REQ-RELAY-001
    fn protocol_deserialize_unknown_rejected() {
        let err = serde_json::from_str::<Protocol>("99");
        assert!(err.is_err());
    }

    #[test]
    // fusa:test REQ-RELAY-002
    fn message_new_defaults() {
        let m = Message::new(Protocol::Rcp, "FrontLeft", vec![1, 2, 3]);
        assert_eq!(m.id, "FrontLeft");
        assert_eq!(m.payload, vec![1, 2, 3]);
        assert_eq!(m.seq, 0);
        assert!(m.meta.is_empty());
    }

    #[test]
    // fusa:test REQ-RELAY-002
    fn message_serde_base64_payload() {
        let m = Message::new(Protocol::Rcp, "Central", vec![0xDE, 0xAD]);
        let json = serde_json::to_value(&m).unwrap();
        assert_eq!(json["payload"], "3q0=");
        assert_eq!(json["protocol"], 5);
    }

    #[test]
    // fusa:test REQ-RELAY-006
    fn context_background_not_done() {
        let ctx = Context::background();
        assert!(!ctx.done());
    }

    #[test]
    // fusa:test REQ-RELAY-006
    fn context_expired() {
        let ctx = Context::with_timeout(Duration::from_nanos(1));
        std::thread::sleep(Duration::from_millis(1));
        assert!(ctx.done());
    }

    #[test]
    // fusa:test REQ-RELAY-004
    fn subscriber_options_chan_depth() {
        let opts = SubscriberOptions::default();
        assert_eq!(opts.chan_depth(64), 64);
        let opts2 = SubscriberOptions {
            channel_depth: 128,
            ..Default::default()
        };
        assert_eq!(opts2.chan_depth(64), 128);
    }

    #[test]
    // fusa:test REQ-RELAY-003
    fn back_pressure_default_is_drop_newest() {
        assert_eq!(
            BackPressurePolicy::default(),
            BackPressurePolicy::DropNewest
        );
    }

    #[test]
    // fusa:test REQ-RELAY-005
    fn error_sentinels_are_distinct() {
        let sentinels = [
            Error::Closed,
            Error::NotConnected,
            Error::Timeout,
            Error::PayloadTooLarge,
        ];
        for i in 0..sentinels.len() {
            for j in (i + 1)..sentinels.len() {
                assert_ne!(sentinels[i], sentinels[j]);
            }
        }
    }
}
