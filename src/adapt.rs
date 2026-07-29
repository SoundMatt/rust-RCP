// fusa:req REQ-ADAPT-001
// fusa:req REQ-ADAPT-002
// fusa:req REQ-ADAPT-003
// fusa:req REQ-ADAPT-004
// fusa:req REQ-ADAPT-005
// fusa:req REQ-ADAPT-006
// fusa:req REQ-ADAPT-007
// fusa:req REQ-ADAPT-008
// fusa:req REQ-ADAPT-009
// fusa:req REQ-ADAPT-010

//! Adapter layer — converts between RCP and external protocol representations.
//!
//! Provides bi-directional mapping between endpoint payload bytes and
//! arbitrary external message formats via the [`Adapter`] trait, and the
//! RELAY-spec `Adapt()` entry point (§10.3) that wraps a [`Controller`] as a
//! [`crate::relay::Caller`] using `to_message()`/`from_message()` (§15.7.5).
//!
//! `ROADMAP.md` Milestone 9 ("All ADAPT-disposition packages retargeted...")
//! cutover: per this module's own ADAPT disposition ("the RELAY
//! `Adapt()`/`to_message()`/`from_message()` pattern itself persists; the
//! mapping needs to be rebuilt against the new endpoint-addressed `Message`
//! shape (Milestone 10)"), this item's scope is deliberately split in two,
//! flagged here per Guiding Principle 5 rather than silently picked:
//!
//! - [`Adapter`]/[`AdaptEndpoint`] — the generic "convert an external
//!   message format to/from RCP" decorator layer, structurally the same
//!   kind of wrapper `ratelimit`/`proxy`/etc. are — is retargeted now, onto
//!   [`crate::mock::Endpoint`] in place of `Controller`. Since `Endpoint`
//!   has no single `send`-shaped call (only distinct `read`/`write`
//!   verbs), [`AdaptEndpoint::send_msg`] models one external "call" as a
//!   write-then-read round trip, converting `M` to write-payload bytes and
//!   converting the subsequent read's bytes back to `M` — this crate's own
//!   simplification, not a transcription of any real external protocol's
//!   actual semantics, which the deeper Milestone 10 rebuild below is
//!   still responsible for defining honestly.
//! - [`adapt()`]/`RcpAdapter`/[`to_message`]/[`from_message`]/
//!   [`response_to_message`] — the RELAY §10.3/§15.7.5 binding itself,
//!   which maps a zone *name* to/from `relay::Message.id` and forwards
//!   `Status`-keyed `Priority`/`CommandType` metadata neither of which
//!   `Endpoint` has an analog for — stay bound to the legacy `Controller`/
//!   `Zone`/`Command`/`Response`/`Status` surface unchanged. Retargeting
//!   this half honestly requires the endpoint-addressed `Message` shape
//!   this row's own disposition-table text reserves for Milestone 10 (a
//!   `StreamId`+`byte_bus_id` addressing scheme in place of a zone name,
//!   and a resolution for `crate::mock::RcServer`'s still-open "no live
//!   notification mechanism" gap before `Status`-style `subscribe`
//!   forwarding has anything real to forward) — inventing one here would
//!   guess at behavior this crate has not decided, rather than retarget
//!   existing behavior.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::{mpsc, Notify};

use crate::mock::Endpoint;
use crate::relay::{BackPressurePolicy, Context, Message, Protocol, SubscriberOptions, Version};
use crate::{zone_from_str, Command, CommandType, Controller, Priority, RcpError, Response};

// ── Adapter trait ─────────────────────────────────────────────────────────────

/// Converts between an external message format `M` and endpoint payload
/// bytes.
// fusa:req REQ-ADAPT-001
pub trait Adapter<M>: Send + Sync {
    /// Convert an external message to endpoint write-payload bytes.
    fn to_write_payload(&self, msg: M) -> Result<Vec<u8>, RcpError>;
    /// Convert endpoint read-response bytes to the external message type.
    fn adapt_read_bytes(&self, bytes: Vec<u8>) -> Result<M, RcpError>;
}

// ── AdaptEndpoint ────────────────────────────────────────────────────────────

/// Endpoint wrapper that adapts an external message type `M` to/from
/// endpoint payload bytes.
// fusa:req REQ-ADAPT-002
pub struct AdaptEndpoint<M> {
    inner: Arc<dyn Endpoint>,
    adapter: Arc<dyn Adapter<M>>,
}

impl<M: Send + Sync + 'static> AdaptEndpoint<M> {
    pub fn new(inner: Arc<dyn Endpoint>, adapter: Arc<dyn Adapter<M>>) -> Self {
        AdaptEndpoint { inner, adapter }
    }

    /// Write `msg` (adapted to endpoint payload bytes), then read back up
    /// to `read_size` bytes and adapt the response — see this module's doc
    /// comment for why this is a write-then-read round trip rather than a
    /// single `send`-shaped call.
    // fusa:req REQ-ADAPT-003
    pub fn send_msg(&self, msg: M, read_size: u8) -> Result<M, RcpError> {
        let payload = self.adapter.to_write_payload(msg)?;
        self.inner.write(&payload)?;
        let bytes = self.inner.read(read_size)?;
        self.adapter.adapt_read_bytes(bytes)
    }
}

// ── Passthrough adapter ───────────────────────────────────────────────────────

/// Identity adapter for `Vec<u8>` → `Vec<u8>` testing.
// fusa:req REQ-ADAPT-004
pub struct PassthroughAdapter;

impl Adapter<Vec<u8>> for PassthroughAdapter {
    fn to_write_payload(&self, msg: Vec<u8>) -> Result<Vec<u8>, RcpError> {
        Ok(msg)
    }
    fn adapt_read_bytes(&self, bytes: Vec<u8>) -> Result<Vec<u8>, RcpError> {
        Ok(bytes)
    }
}

// ---------------------------------------------------------------------------
// to_message / from_message — RELAY spec §15.7.5
// ---------------------------------------------------------------------------

/// Convert a [`Status`](crate::Status) to a `relay::Message` (Subscribe
/// direction) per RELAY spec §15.7.5.
// fusa:req REQ-ADAPT-006
pub fn to_message(status: &crate::Status) -> Message {
    let mut meta = std::collections::BTreeMap::new();
    meta.insert("rcp.healthy".to_string(), status.healthy.to_string());
    Message {
        protocol: Protocol::Rcp,
        version: Version::default(),
        id: status.zone.as_str().to_string(),
        payload: status.payload.clone().unwrap_or_default(),
        timestamp: Utc::now(),
        seq: status.seq as u64,
        meta,
    }
}

/// Convert a `relay::Message` to a [`Command`] (Caller.Call direction,
/// request half) per RELAY spec §15.7.5.
///
/// Returns `Err(RcpError::NotFound)` if `msg.id` is not a known zone name.
// fusa:req REQ-ADAPT-007
pub fn from_message(msg: &Message) -> Result<Command, RcpError> {
    let zone = zone_from_str(&msg.id)?;
    let priority = msg
        .meta
        .get("rcp.priority")
        .map(|v| parse_priority(v))
        .unwrap_or(Priority::NORMAL);
    let cmd_type = msg
        .meta
        .get("rcp.cmd_type")
        .map(|v| parse_cmd_type(v))
        .unwrap_or(CommandType::NOOP);
    let payload = if msg.payload.is_empty() {
        None
    } else {
        Some(msg.payload.clone())
    };
    Ok(Command {
        id: 0,
        zone,
        cmd_type,
        priority,
        payload,
    })
}

/// Convert a [`Response`] to a `relay::Message` (Caller.Call direction,
/// reply half) per RELAY spec §15.7.5.
// fusa:req REQ-ADAPT-008
pub fn response_to_message(resp: &Response) -> Message {
    let mut meta = std::collections::BTreeMap::new();
    meta.insert("rcp.status".to_string(), resp.status.0.to_string());
    Message {
        protocol: Protocol::Rcp,
        version: Version::default(),
        id: resp.zone.as_str().to_string(),
        payload: resp.payload.clone().unwrap_or_default(),
        timestamp: Utc::now(),
        seq: 0,
        meta,
    }
}

fn parse_priority(s: &str) -> Priority {
    match s {
        "high" => Priority::HIGH,
        "critical" => Priority::CRITICAL,
        _ => Priority::NORMAL,
    }
}

fn parse_cmd_type(s: &str) -> CommandType {
    match s {
        "set" => CommandType::SET,
        "get" => CommandType::GET,
        "reset" => CommandType::RESET,
        "watchdog" => CommandType::WATCHDOG,
        "sleep" => CommandType::SLEEP,
        "wake" => CommandType::WAKE,
        _ => CommandType::NOOP,
    }
}

// ---------------------------------------------------------------------------
// AdaptQueue — policy-aware buffer for the Adapt()-level relay.Message
// channel, per RELAY spec §10.5 rule 3.
// ---------------------------------------------------------------------------

/// A bounded `Message` queue implementing `DropNewest`/`DropOldest`/`Block`
/// back-pressure, sitting between the blocking [`Controller::subscribe`]
/// forwarding task and the async `relay::Node::subscribe` channel returned
/// to the caller.
struct AdaptQueue {
    queue: Mutex<VecDeque<Message>>,
    capacity: usize,
    policy: BackPressurePolicy,
    notify_push: Notify,
    notify_pop: Notify,
    closed: std::sync::atomic::AtomicBool,
}

impl AdaptQueue {
    fn new(capacity: usize, policy: BackPressurePolicy) -> Self {
        Self {
            queue: Mutex::new(VecDeque::with_capacity(capacity.min(256))),
            capacity: capacity.max(1),
            policy,
            notify_push: Notify::new(),
            notify_pop: Notify::new(),
            closed: std::sync::atomic::AtomicBool::new(false),
        }
    }

    async fn push(&self, msg: Message) {
        match self.policy {
            BackPressurePolicy::DropNewest => {
                let mut q = self.queue.lock().unwrap();
                if q.len() < self.capacity {
                    q.push_back(msg);
                    drop(q);
                    self.notify_push.notify_one();
                }
            }
            BackPressurePolicy::DropOldest => {
                let mut q = self.queue.lock().unwrap();
                if q.len() >= self.capacity {
                    q.pop_front();
                }
                q.push_back(msg);
                drop(q);
                self.notify_push.notify_one();
            }
            BackPressurePolicy::Block => loop {
                {
                    let mut q = self.queue.lock().unwrap();
                    if q.len() < self.capacity {
                        q.push_back(msg);
                        drop(q);
                        self.notify_push.notify_one();
                        return;
                    }
                }
                self.notify_pop.notified().await;
            },
        }
    }

    async fn pop(&self) -> Option<Message> {
        loop {
            {
                let mut q = self.queue.lock().unwrap();
                if let Some(m) = q.pop_front() {
                    drop(q);
                    self.notify_pop.notify_one();
                    return Some(m);
                }
            }
            if self.closed.load(std::sync::atomic::Ordering::SeqCst) {
                return self.queue.lock().unwrap().pop_front();
            }
            self.notify_push.notified().await;
        }
    }

    fn close(&self) {
        self.closed.store(true, std::sync::atomic::Ordering::SeqCst);
        self.notify_push.notify_waiters();
    }
}

// ---------------------------------------------------------------------------
// adapt() — RELAY spec §10.3: RCP `Adapt(c Controller) relay.Caller`
// ---------------------------------------------------------------------------

/// Wrap a [`Controller`] as a `relay::Caller` (which also satisfies
/// `relay::Node`) per RELAY spec §10.3.
///
/// `Controller`'s methods are blocking; each call here dispatches through
/// [`tokio::task::spawn_blocking`] so the async boundary required by §18.3
/// is genuine rather than a facade over a blocking call on the calling task.
// fusa:req REQ-ADAPT-009
pub fn adapt(ctrl: Arc<dyn Controller>) -> Box<dyn crate::relay::Caller> {
    Box::new(RcpAdapter { ctrl })
}

struct RcpAdapter {
    ctrl: Arc<dyn Controller>,
}

fn ctx_timeout(ctx: &Context) -> Option<Duration> {
    ctx.deadline.map(|d| {
        let now = std::time::Instant::now();
        if d > now {
            d - now
        } else {
            Duration::ZERO
        }
    })
}

fn map_err(e: RcpError) -> crate::relay::Error {
    if e.is_relay_closed() {
        crate::relay::Error::Closed
    } else if e.is_relay_timeout() {
        crate::relay::Error::Timeout
    } else if e.is_relay_payload_too_large() {
        crate::relay::Error::PayloadTooLarge
    } else {
        // NotConnected / NotFound / ZoneMismatch / wire / e2e / other — closest
        // available sentinel is NotConnected (§5.3: "no usable route" family).
        crate::relay::Error::NotConnected
    }
}

#[async_trait]
impl crate::relay::Node for RcpAdapter {
    fn protocol(&self) -> Protocol {
        Protocol::Rcp
    }

    /// Send a `relay::Message` by converting it to a [`Command`] and
    /// dispatching it; the [`Response`] is awaited (so delivery can be
    /// confirmed) but discarded, matching `Node::send`'s fire-and-forget
    /// contract (§10.1) — use [`crate::relay::Caller::call`] for the reply.
    async fn send(&self, ctx: Context, msg: Message) -> Result<(), crate::relay::Error> {
        let cmd = from_message(&msg).map_err(map_err)?;
        let ctrl = Arc::clone(&self.ctrl);
        let timeout = ctx_timeout(&ctx);
        tokio::task::spawn_blocking(move || ctrl.send(&cmd, timeout))
            .await
            .map_err(|_| crate::relay::Error::Closed)?
            .map_err(map_err)?;
        Ok(())
    }

    /// Subscribe to [`Status`](crate::Status) updates and forward them as
    /// `relay::Message`s, following the goroutine/task model of §10.5: one
    /// task per subscription, its own `Seq` counter starting at 0, and the
    /// caller-supplied `BackPressurePolicy` applied at the `relay.Message`
    /// layer via [`AdaptQueue`].
    async fn subscribe(
        &self,
        opts: SubscriberOptions,
    ) -> Result<mpsc::Receiver<Message>, crate::relay::Error> {
        let depth = opts.chan_depth(64);
        let policy = opts.back_pressure;
        let ctrl = Arc::clone(&self.ctrl);

        let sub = tokio::task::spawn_blocking(move || ctrl.subscribe())
            .await
            .map_err(|_| crate::relay::Error::Closed)?
            .map_err(map_err)?;

        let (tx, rx) = mpsc::channel::<Message>(1);
        let queue = Arc::new(AdaptQueue::new(depth, policy));

        // Producer: blocking thread draining the sync Subscription, applying
        // the back-pressure policy against `queue` (§10.5 rule 3). The
        // runtime `Handle` is captured here (on the async task, inside the
        // runtime) since `Handle::current()` panics if called from a plain
        // `std::thread::spawn` thread with no ambient runtime context.
        let producer_queue = Arc::clone(&queue);
        let rt = tokio::runtime::Handle::current();
        std::thread::spawn(move || {
            let mut seq: u64 = 0;
            while let Some(status) = sub.recv() {
                let mut m = to_message(&status);
                m.seq = seq;
                seq += 1;
                rt.block_on(producer_queue.push(m));
            }
            producer_queue.close();
        });

        // Forwarder: drains `queue` into the external channel one message at
        // a time. §10.5 rule 2: the channel closes when this task exits.
        tokio::spawn(async move {
            while let Some(msg) = queue.pop().await {
                if tx.send(msg).await.is_err() {
                    break; // receiver dropped
                }
            }
        });

        Ok(rx)
    }

    async fn close(&self) -> Result<(), crate::relay::Error> {
        let ctrl = Arc::clone(&self.ctrl);
        tokio::task::spawn_blocking(move || ctrl.close())
            .await
            .map_err(|_| crate::relay::Error::Closed)?
            .map_err(map_err)
    }
}

#[async_trait]
impl crate::relay::Caller for RcpAdapter {
    /// Dispatch `req` and return the zone controller's reply as a
    /// `relay::Message`, per RELAY spec §10.2/§15.7.5.
    // fusa:req REQ-ADAPT-010
    async fn call(&self, ctx: Context, req: Message) -> Result<Message, crate::relay::Error> {
        let cmd = from_message(&req).map_err(map_err)?;
        let ctrl = Arc::clone(&self.ctrl);
        let timeout = ctx_timeout(&ctx);
        let resp = tokio::task::spawn_blocking(move || ctrl.send(&cmd, timeout))
            .await
            .map_err(|_| crate::relay::Error::Closed)?
            .map_err(map_err)?;
        Ok(response_to_message(&resp))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::{MockController, MockEndpoint};
    use crate::regmap::EndpointType;
    use crate::{Response, ResponseStatus, Zone};

    fn ok_ctrl() -> Arc<dyn Controller> {
        let h: crate::mock::Handler = Box::new(|cmd| Response {
            command_id: cmd.id,
            zone: cmd.zone,
            status: ResponseStatus::OK,
            payload: cmd.payload.clone(),
        });
        MockController::new(Zone::FRONT_LEFT, Some(h)) as Arc<dyn Controller>
    }

    fn ok_endpoint() -> Arc<dyn Endpoint> {
        MockEndpoint::new(EndpointType::Gpio, vec![0u8; 8]) as Arc<dyn Endpoint>
    }

    #[test]
    // fusa:test REQ-ADAPT-001
    // fusa:test REQ-ADAPT-004
    fn passthrough_adapter_identity() {
        let ep = AdaptEndpoint::new(ok_endpoint(), Arc::new(PassthroughAdapter));
        let out = ep.send_msg(b"hi".to_vec(), 8).unwrap();
        assert!(out.starts_with(b"hi"));
    }

    #[test]
    // fusa:test REQ-ADAPT-002
    fn ep_type_forwarded() {
        let inner = ok_endpoint();
        let ep = AdaptEndpoint::new(Arc::clone(&inner), Arc::new(PassthroughAdapter));
        assert_eq!(ep.inner.ep_type(), EndpointType::Gpio);
    }

    #[test]
    // fusa:test REQ-ADAPT-003
    fn adapter_error_propagated() {
        struct FailAdapter;
        impl Adapter<Vec<u8>> for FailAdapter {
            fn to_write_payload(&self, _: Vec<u8>) -> Result<Vec<u8>, RcpError> {
                Err(RcpError::Other("bad msg".into()))
            }
            fn adapt_read_bytes(&self, _: Vec<u8>) -> Result<Vec<u8>, RcpError> {
                unreachable!()
            }
        }
        let ep = AdaptEndpoint::new(ok_endpoint(), Arc::new(FailAdapter));
        let err = ep.send_msg(Vec::new(), 8).unwrap_err();
        assert!(matches!(err, RcpError::Other(_)));
    }

    #[test]
    // fusa:test REQ-ADAPT-005
    fn passthrough_preserves_payload() {
        let ep = AdaptEndpoint::new(ok_endpoint(), Arc::new(PassthroughAdapter));
        let out = ep.send_msg(b"data".to_vec(), 8).unwrap();
        assert!(out.starts_with(b"data"));
    }

    // ── to_message / from_message / response_to_message (§15.7.5) ────────────

    #[test]
    // fusa:test REQ-ADAPT-006
    fn status_to_message_maps_zone_seq_healthy_payload() {
        let status = crate::Status {
            zone: Zone::FRONT_LEFT,
            seq: 3,
            healthy: true,
            payload: Some(vec![0x01]),
        };
        let msg = to_message(&status);
        assert_eq!(msg.protocol, Protocol::Rcp);
        assert_eq!(msg.id, "FrontLeft");
        assert_eq!(msg.seq, 3);
        assert_eq!(msg.payload, vec![0x01]);
        assert_eq!(msg.meta.get("rcp.healthy"), Some(&"true".to_string()));
    }

    #[test]
    // fusa:test REQ-ADAPT-007
    fn message_from_message_maps_zone_priority_cmd_type() {
        let mut meta = std::collections::BTreeMap::new();
        meta.insert("rcp.priority".to_string(), "critical".to_string());
        meta.insert("rcp.cmd_type".to_string(), "reset".to_string());
        let msg = Message {
            protocol: Protocol::Rcp,
            version: Version::default(),
            id: "RearRight".to_string(),
            payload: vec![0xAA],
            timestamp: Utc::now(),
            seq: 0,
            meta,
        };
        let cmd = from_message(&msg).unwrap();
        assert_eq!(cmd.zone, Zone::REAR_RIGHT);
        assert_eq!(cmd.priority, Priority::CRITICAL);
        assert_eq!(cmd.cmd_type, CommandType::RESET);
        assert_eq!(cmd.payload, Some(vec![0xAA]));
    }

    #[test]
    // fusa:test REQ-ADAPT-007
    fn message_from_message_unknown_zone_is_not_found() {
        let msg = Message::new(Protocol::Rcp, "NotAZone", vec![]);
        let err = from_message(&msg).unwrap_err();
        assert_eq!(err, RcpError::NotFound);
    }

    #[test]
    // fusa:test REQ-ADAPT-008
    fn response_to_message_maps_zone_status_payload() {
        let resp = Response {
            command_id: 9,
            zone: Zone::CENTRAL,
            status: ResponseStatus::ERROR,
            payload: Some(vec![0xFF]),
        };
        let msg = response_to_message(&resp);
        assert_eq!(msg.id, "Central");
        assert_eq!(msg.payload, vec![0xFF]);
        assert_eq!(msg.meta.get("rcp.status"), Some(&"1".to_string()));
    }

    // ── adapt() (§10.3) ────────────────────────────────────────────────────

    #[tokio::test]
    // fusa:test REQ-ADAPT-009
    // fusa:test REQ-ADAPT-010
    // fusa:test REQ-RELAY-008
    async fn adapt_call_dispatches_and_returns_message() {
        let node = adapt(ok_ctrl());
        let req = Message::new(Protocol::Rcp, "FrontLeft", vec![1, 2]);
        let reply = node.call(Context::background(), req).await.unwrap();
        assert_eq!(reply.id, "FrontLeft");
        assert_eq!(reply.payload, vec![1, 2]);
        assert_eq!(reply.meta.get("rcp.status"), Some(&"0".to_string()));
    }

    #[tokio::test]
    // fusa:test REQ-ADAPT-009
    async fn adapt_send_discards_response() {
        let node = adapt(ok_ctrl());
        let msg = Message::new(Protocol::Rcp, "FrontLeft", vec![]);
        crate::relay::Node::send(&*node, Context::background(), msg)
            .await
            .unwrap();
    }

    #[tokio::test]
    // fusa:test REQ-ADAPT-009
    async fn adapt_send_invalid_zone_is_not_connected() {
        let node = adapt(ok_ctrl());
        let msg = Message::new(Protocol::Rcp, "NoSuchZone", vec![]);
        let err = crate::relay::Node::send(&*node, Context::background(), msg)
            .await
            .unwrap_err();
        assert_eq!(err, crate::relay::Error::NotConnected);
    }

    #[tokio::test]
    // fusa:test REQ-ADAPT-009
    async fn adapt_subscribe_delivers_published_status() {
        let ctrl = MockController::new(Zone::FRONT_LEFT, None);
        let publishable = Arc::clone(&ctrl);
        let node = adapt(ctrl as Arc<dyn Controller>);

        let mut rx = crate::relay::Node::subscribe(&*node, SubscriberOptions::default())
            .await
            .unwrap();

        publishable.publish(Some(vec![0xAB]));
        let msg = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("no message received in time")
            .expect("channel closed");
        assert_eq!(msg.id, "FrontLeft");
        assert_eq!(msg.payload, vec![0xAB]);
    }

    #[tokio::test]
    // fusa:test REQ-ADAPT-009
    async fn adapt_close_closes_inner_controller() {
        let ctrl = MockController::new(Zone::FRONT_LEFT, None);
        let inner = Arc::clone(&ctrl);
        let node = adapt(ctrl as Arc<dyn Controller>);
        crate::relay::Node::close(&*node).await.unwrap();
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            ..Default::default()
        };
        assert_eq!(inner.send(&cmd, None).unwrap_err(), RcpError::Closed);
    }

    #[test]
    // fusa:test REQ-ADAPT-009
    // fusa:test REQ-RELAY-007
    fn adapt_protocol_is_rcp() {
        let node = adapt(ok_ctrl());
        assert_eq!(crate::relay::Node::protocol(&*node), Protocol::Rcp);
    }
}
