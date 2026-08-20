//fusa:req REQ-ADAPT-001
//fusa:req REQ-ADAPT-002
//fusa:req REQ-ADAPT-003
//fusa:req REQ-ADAPT-004
//fusa:req REQ-ADAPT-005
//fusa:req REQ-ADAPT-006
//fusa:req REQ-ADAPT-007
//fusa:req REQ-ADAPT-008
//fusa:req REQ-ADAPT-009
//fusa:req REQ-ADAPT-010
//fusa:req REQ-ADAPT-011

//! Adapter layer — converts between RCP and external protocol representations.
//!
//! Provides bi-directional mapping between endpoint payload bytes and
//! arbitrary external message formats via the [`Adapter`] trait, and the
//! RELAY-spec `Adapt()` entry point (§10.3) that wraps an [`RcServer`] as a
//! [`crate::relay::Caller`] using `to_message()`/`from_message()` (§15.7.5).
//!
//! `ROADMAP.md` Milestone 9 ("All ADAPT-disposition packages retargeted...")
//! split this module's own cutover in two, per its own ADAPT disposition
//! ("the RELAY `Adapt()`/`to_message()`/`from_message()` pattern itself
//! persists; the mapping needs to be rebuilt against the new
//! endpoint-addressed `Message` shape (Milestone 10)"):
//!
//! - [`Adapter`]/[`AdaptEndpoint`] — the generic "convert an external
//!   message format to/from RCP" decorator layer, structurally the same
//!   kind of wrapper `ratelimit`/`proxy`/etc. are — was retargeted in
//!   Milestone 9 onto [`crate::mock::Endpoint`] in place of `Controller`.
//!   Since `Endpoint` has no single `send`-shaped call (only distinct
//!   `read`/`write` verbs), [`AdaptEndpoint::send_msg`] models one external
//!   "call" as a write-then-read round trip, converting `M` to
//!   write-payload bytes and converting the subsequent read's bytes back
//!   to `M` — this crate's own simplification, not a transcription of any
//!   real external protocol's actual semantics.
//! - [`adapt()`]/`RcpAdapter`/[`to_message`]/[`from_message`]/
//!   [`response_to_message`] — the RELAY §10.3/§15.7.5 binding itself — is
//!   rebuilt here (Milestone 10) against [`crate::mock::RcServer`]'s
//!   `(StreamId, byte_bus_id)` addressing in place of the retired
//!   zone-name-as-`id` convention. See "Provenance note" below for the
//!   design choices this rebuild had to make that neither the RELAY spec
//!   nor `ROADMAP.md` pin down, per Guiding Principle 5.
//!
//! ## Provenance note
//!
//! - **`Message.id` encoding.** The RELAY spec says `id` names a message's
//!   addressed target; it does not prescribe a string shape. This binding
//!   encodes `(stream_id, byte_bus_id)` as
//!   `"<16 lowercase hex digits><'.'><decimal byte_bus_id>"` — see
//!   [`format_endpoint_id`]/[`parse_endpoint_id`] for the exact grammar.
//!   This crate's own choice, not a spec requirement.
//! - **Read vs. write.** Unlike the retired `Command`/`CommandType` model,
//!   `RcServer::handle_abb` dispatches purely on a boolean `op` (write) /
//!   not-`op` (read) flag, with no third "no-op" case. `from_message`
//!   reads an optional `"rcp.op"` meta key (`"read"`/`"write"`) and, absent
//!   one, infers `op` from whether `msg.payload` is empty — a stand-in
//!   signal this binding chose since `relay::Message` has no field that
//!   states read/write intent directly.
//! - **Read size.** A read additionally needs a requested byte count that
//!   `Command` never carried. `from_message` reads an optional
//!   `"rcp.read_size"` meta key (decimal `u16`), defaulting to `u16::MAX` —
//!   "return everything held" — matching [`crate::mock::MockEndpoint::read`]'s
//!   own already-established "cap to whatever is actually held" behavior.
//! - **Response classification.** [`to_message`]/[`response_to_message`]
//!   also surface [`crate::acf::ByteMessageInfo::response_kind`]'s TC18
//!   §11.3 Table 15 classification (`Acknowledge`/`Write`/`Read`/`Error`) as
//!   an `"rcp.response_kind"` meta key, mirroring cpp-RCP's
//!   `response_to_message` (`include/rcp/adapt.hpp`), which sets the
//!   analogous `meta["rcp.response_kind"]`.
//! - **Subscribe.** [`crate::mock::RcServer`]'s own doc comment states it
//!   deliberately does not model live asynchronous notification — no TC18
//!   analog has been identified for one in this crate to date — so unlike
//!   the retired `Controller::subscribe`/`Status` forwarding this replaced,
//!   there is nothing today for a subscription to forward. The RELAY spec
//!   itself anticipates exactly this: §10.4's routing-rules table and
//!   §15.7.5 both state that RCP has no server-initiated push and that
//!   `Subscribe()` is expected to return "a well-behaved, permanently-empty
//!   stream" for this protocol — this is not a rust-RCP-specific gap so
//!   much as an inherent property of RCP being request/response-only.
//!   Rather than invent a notification source or overload one of the four
//!   RELAY error sentinels to mean "unsupported," [`RcpAdapter`]'s
//!   `subscribe` (`crate::relay::Node::subscribe`) returns a channel that
//!   is immediately, legitimately closed — an honest "no events, currently"
//!   answer within `Node::subscribe`'s existing typed contract, not a
//!   silently-invented notification stream. One detail the spec text above
//!   does not fully pin down, and this binding has not reconciled: whether
//!   "permanently-empty stream" calls for the channel to close immediately
//!   (this binding's current choice) or to stay open-but-silent until
//!   [`crate::relay::Node::close`] is called, matching `Node::subscribe`'s
//!   general "closed when the node closes" contract for every other
//!   protocol. Building a real live-notification path (as opposed to
//!   resolving this open question) is left to whichever later milestone
//!   gives `RcServer` a live-notification mechanism to forward, same as
//!   `crate::mock::RcServer`'s own doc comment already defers it. Until
//!   then, `rust-rcp capabilities`' `"features"` array carries a
//!   `"no-live-subscribe"` entry (see `src/bin/rcp.rs`) — a top-level
//!   `"subscribe_supported"` property was tried first but rejected by
//!   `relay conform --strict`'s §12.2 schema check, which does not allow
//!   unrecognized top-level properties — making this limitation
//!   machine-readable rather than leaving a caller to discover it only by
//!   observing an empty channel at runtime.
//! - **Close.** `RcServer` tracks an [`crate::lifecycle::RcServerState`]
//!   lifecycle position, not an open/closed connection boolean, so there is
//!   nothing on `RcServer` itself for `Node::close` to delegate to —
//!   mirroring [`crate::udp::UdpTransport::close`]'s own precedent of a
//!   locally-tracked close in this same endpoint-addressed model.
//!   [`RcpAdapter`] keeps its own `closed` flag so `close`'s "further calls
//!   fail" contract stays meaningful rather than becoming a pure no-op.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::mpsc;

use crate::acf::{AcfAbbMessage, ByteMessageInfo, ReadSizeOrSegment};
use crate::avtp::StreamId;
use crate::mock::{Endpoint, RcServer};
use crate::relay::{Context, Message, Protocol, SubscriberOptions, Version};
use crate::RcpError;

// ── Adapter trait ─────────────────────────────────────────────────────────────

/// Converts between an external message format `M` and endpoint payload
/// bytes.
//fusa:req REQ-ADAPT-001
pub trait Adapter<M>: Send + Sync {
    /// Convert an external message to endpoint write-payload bytes.
    fn to_write_payload(&self, msg: M) -> Result<Vec<u8>, RcpError>;
    /// Convert endpoint read-response bytes to the external message type.
    fn adapt_read_bytes(&self, bytes: Vec<u8>) -> Result<M, RcpError>;
}

// ── AdaptEndpoint ────────────────────────────────────────────────────────────

/// Endpoint wrapper that adapts an external message type `M` to/from
/// endpoint payload bytes.
//fusa:req REQ-ADAPT-002
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
    //fusa:req REQ-ADAPT-003
    pub fn send_msg(&self, msg: M, read_size: u16) -> Result<M, RcpError> {
        let payload = self.adapter.to_write_payload(msg)?;
        self.inner.write(&payload)?;
        let bytes = self.inner.read(read_size)?;
        self.adapter.adapt_read_bytes(bytes)
    }
}

// ── Passthrough adapter ───────────────────────────────────────────────────────

/// Identity adapter for `Vec<u8>` → `Vec<u8>` testing.
//fusa:req REQ-ADAPT-004
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
// Endpoint-address encoding — this binding's own `Message.id` mapping
// (ROADMAP.md Milestone 10), replacing the retired zone-name-as-`id`
// convention. See this module's provenance note for why this shape.
// ---------------------------------------------------------------------------

/// Separator between the hex `stream_id` half and the decimal `byte_bus_id`
/// half of an encoded [`Message::id`].
const ENDPOINT_ID_SEP: char = '.';

/// Encode a `(stream_id, byte_bus_id)` pair as a `relay::Message.id` string.
///
/// `stream_id` is rendered as [`StreamId::to_u64`]'s full 64-bit value in
/// 16 lowercase hex digits, zero-padded so [`parse_endpoint_id`] never has
/// to guess where it ends; `byte_bus_id` follows in plain decimal, needing
/// no padding since [`ENDPOINT_ID_SEP`] already marks where it starts.
//fusa:req REQ-ADAPT-011
pub fn format_endpoint_id(stream_id: StreamId, byte_bus_id: u16) -> String {
    format!(
        "{:016x}{}{}",
        stream_id.to_u64(),
        ENDPOINT_ID_SEP,
        byte_bus_id
    )
}

/// Decode a `relay::Message.id` string built by [`format_endpoint_id`] back
/// into its `(stream_id, byte_bus_id)` pair.
///
/// Returns `Err(RcpError::InvalidParameter)` if `id` is not exactly one
/// [`ENDPOINT_ID_SEP`]-separated pair of a 16-hex-digit `stream_id` and a
/// decimal `byte_bus_id` in `0..=u16::MAX`. Never panics on malformed
/// input.
//fusa:req REQ-ADAPT-011
pub fn parse_endpoint_id(id: &str) -> Result<(StreamId, u16), RcpError> {
    let (sid_hex, bus_dec) = id
        .split_once(ENDPOINT_ID_SEP)
        .ok_or(RcpError::InvalidParameter)?;
    let raw = u64::from_str_radix(sid_hex, 16).map_err(|_| RcpError::InvalidParameter)?;
    let byte_bus_id = bus_dec
        .parse::<u16>()
        .map_err(|_| RcpError::InvalidParameter)?;
    Ok((StreamId::from_u64(raw), byte_bus_id))
}

// ---------------------------------------------------------------------------
// to_message / from_message / response_to_message — RELAY spec §15.7.5,
// rebuilt against RcServer's (StreamId, byte_bus_id)-addressed ACF_ABB
// request/response shape.
// ---------------------------------------------------------------------------

/// Convert a `relay::Message` to an addressed ACF_ABB request per RELAY
/// spec §15.7.5 (Caller.Call direction, request half), ready to hand to
/// [`crate::mock::RcServer::handle_abb`].
///
/// `msg.id` is decoded via [`parse_endpoint_id`]. `msg.meta` supplies two
/// optional, this-binding-defined keys — see this module's provenance
/// note for why each defaults the way it does:
///
/// - `"rcp.op"` (`"read"` or `"write"`; any other value is
///   `Err(RcpError::InvalidParameter)`) — defaults to `"write"` if
///   `msg.payload` is non-empty, `"read"` otherwise.
/// - `"rcp.read_size"` (a decimal `u16`; malformed is
///   `Err(RcpError::InvalidParameter)`) — defaults to `u16::MAX`, meaningful
///   only for a read.
///
/// Every other `ByteMessageInfo` field this binding has no `Message`-level
/// analog for (`evt`, `hs`, `cs`, `transaction_num`, `ms`, `pad`, `mtv`) is
/// left at its zero default; [`crate::mock::RcServer::handle_abb`]'s
/// dispatch logic does not consult any of them.
//fusa:req REQ-ADAPT-007
pub fn from_message(msg: &Message) -> Result<(StreamId, AcfAbbMessage), RcpError> {
    let (stream_id, byte_bus_id) = parse_endpoint_id(&msg.id)?;
    let op = match msg.meta.get("rcp.op").map(String::as_str) {
        Some("write") => true,
        Some("read") => false,
        Some(_) => return Err(RcpError::InvalidParameter),
        None => !msg.payload.is_empty(),
    };
    let read_size = msg
        .meta
        .get("rcp.read_size")
        .map(|v| v.parse::<u16>().map_err(|_| RcpError::InvalidParameter))
        .transpose()?
        .unwrap_or(u16::MAX);
    Ok((
        stream_id,
        AcfAbbMessage {
            info: ByteMessageInfo {
                byte_bus_id,
                op,
                read_size_segment: ReadSizeOrSegment(read_size),
                ..Default::default()
            },
            payload: msg.payload.clone(),
        },
    ))
}

/// Convert an addressed ACF_ABB response to a `relay::Message` per RELAY
/// spec §15.7.5. Shared by both directions this module's provenance note
/// describes: the Caller.Call reply half (used by [`response_to_message`]
/// below) and, per that note, the same shape a future subscribe-forwarding
/// path would reuse once [`crate::mock::RcServer`] gains a live-notification
/// mechanism.
///
/// `resp.info.op` is surfaced back as the `"rcp.op"` meta key
/// (`"write"`/`"read"`), mirroring [`from_message`]'s own request-side key,
/// so a caller can confirm which operation the RC Server actually
/// performed. `resp.info` is additionally classified via
/// [`ByteMessageInfo::response_kind`] (TC18 §11.3 Table 15) and surfaced as
/// the `"rcp.response_kind"` meta key (one of
/// [`crate::acf::ResponseKind::as_str`]'s
/// `"acknowledge"`/`"write"`/`"read"`/`"error"` values), mirroring cpp-RCP's
/// `response_to_message` (`include/rcp/adapt.hpp`), which sets the analogous
/// `meta["rcp.response_kind"]`.
//fusa:req REQ-ADAPT-006
//fusa:req REQ-RESP-004
pub fn to_message(stream_id: StreamId, resp: &AcfAbbMessage) -> Message {
    let mut meta = std::collections::BTreeMap::new();
    meta.insert(
        "rcp.op".to_string(),
        if resp.info.op { "write" } else { "read" }.to_string(),
    );
    meta.insert(
        "rcp.response_kind".to_string(),
        resp.info.response_kind().as_str().to_string(),
    );
    Message {
        protocol: Protocol::Rcp,
        version: Version::default(),
        id: format_endpoint_id(stream_id, resp.info.byte_bus_id),
        payload: resp.payload.clone(),
        timestamp: Utc::now(),
        seq: 0,
        meta,
    }
}

/// Convert an addressed ACF_ABB response to a `relay::Message` (Caller.Call
/// direction, reply half) per RELAY spec §15.7.5.
///
/// A thin, separately-named entry point over [`to_message`] — kept for
/// symmetry with [`from_message`]'s request half, since both directions now
/// share one addressed-response conversion (see this module's provenance
/// note on why the retired `Status`/`Response` split collapsed to one
/// shape).
//fusa:req REQ-ADAPT-008
pub fn response_to_message(stream_id: StreamId, resp: &AcfAbbMessage) -> Message {
    to_message(stream_id, resp)
}

// ---------------------------------------------------------------------------
// adapt() — RELAY spec §10.3: RCP `Adapt(...) relay.Caller`, rebuilt against
// crate::mock::RcServer per ROADMAP.md Milestone 10.
//
// Note: the retired `Controller`-based binding kept its own `AdaptQueue` —
// a `BackPressurePolicy`-aware buffer per RELAY spec §10.5 rule 3 — sitting
// between a blocking notification-producer task and the async
// `relay::Node::subscribe` channel. Since `subscribe` below has no producer
// to buffer for (see this module's provenance note), that queue has no
// caller left and is removed rather than kept as unused scaffolding;
// whichever later milestone gives `RcServer` a live-notification mechanism
// can reintroduce the same `crate::relay::BackPressurePolicy`-driven shape
// once it has something real to buffer.
//
// Back-pressure verification (rust-RCP-15): RELAY spec §10.5's
// `BackPressurePolicy` machinery (rules 3/6) is scoped explicitly to the
// goroutine/task that forwards a live protocol subscription into the
// bounded `relay.Message` channel `Node::subscribe` returns — it says
// nothing about `Caller::call`'s request/response path. `RcpAdapter::call`
// below has no bounded queue or semaphore of its own; each call dispatches
// one `spawn_blocking` round trip and is bounded only by `ctx`'s deadline
// (`Context::done`), the same shape `Node::send` already uses. That is
// consistent with §10.5's own scope (it never mentions `Caller.Call`), not
// a demonstrated instance of the spec's back-pressure requirement — this
// crate has not found a §10.5-equivalent back-pressure requirement that
// actually applies to `Call`/`Send`, but has also not exhaustively
// searched the rest of the spec for one, so this remains flagged per
// Guiding Principle 5 rather than asserted as a closed question.
// ---------------------------------------------------------------------------

/// Wrap an [`RcServer`] as a `relay::Caller` (which also satisfies
/// `relay::Node`) per RELAY spec §10.3, addressed by
/// `(StreamId, byte_bus_id)` rather than by the retired `Zone` model.
///
/// `RcServer`'s dispatch methods are synchronous; each call here still
/// dispatches through [`tokio::task::spawn_blocking`], preserving this
/// binding's existing §18.3 discipline (a genuine async boundary, not a
/// facade over a blocking call on the calling task) — a real `Endpoint`
/// behind an `RcServer` may perform real, blocking device I/O even though
/// [`crate::mock::MockEndpoint`] does not.
pub fn adapt(server: Arc<RcServer>) -> Box<dyn crate::relay::Caller> {
    Box::new(RcpAdapter {
        server,
        closed: AtomicBool::new(false),
    })
}

struct RcpAdapter {
    server: Arc<RcServer>,
    /// This binding's own open/closed flag — see this module's provenance
    /// note on why `RcServer` itself has nothing for `Node::close` to
    /// delegate to, and why this flag is actually consulted by
    /// `send`/`call` below rather than being a pure no-op.
    closed: AtomicBool,
}

fn map_err(e: RcpError) -> crate::relay::Error {
    if e.is_relay_closed() {
        crate::relay::Error::Closed
    } else if e.is_relay_timeout() {
        crate::relay::Error::Timeout
    } else if e.is_relay_payload_too_large() {
        crate::relay::Error::PayloadTooLarge
    } else {
        // InvalidParameter / EpNotFound / EpError / UnauthorizedAccess /
        // LockedMemAccess / wire / other — closest available sentinel is
        // NotConnected (§5.3: "no usable route" family).
        crate::relay::Error::NotConnected
    }
}

#[async_trait]
impl crate::relay::Node for RcpAdapter {
    fn protocol(&self) -> Protocol {
        Protocol::Rcp
    }

    /// Send a `relay::Message` by converting it to an addressed ACF_ABB
    /// request and dispatching it; the response is awaited (so delivery
    /// can be confirmed) but discarded, matching `Node::send`'s
    /// fire-and-forget contract (§10.1) — use
    /// [`crate::relay::Caller::call`] for the reply.
    async fn send(&self, ctx: Context, msg: Message) -> Result<(), crate::relay::Error> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(crate::relay::Error::Closed);
        }
        if ctx.done() {
            return Err(crate::relay::Error::Timeout);
        }
        let (stream_id, request) = from_message(&msg).map_err(map_err)?;
        let server = Arc::clone(&self.server);
        tokio::task::spawn_blocking(move || server.handle_abb(stream_id, &request))
            .await
            .map_err(|_| crate::relay::Error::Closed)?
            .map_err(map_err)?;
        Ok(())
    }

    /// See this module's provenance note on `subscribe`: `RcServer` has no
    /// live-notification mechanism to forward yet, so this returns a
    /// channel that is immediately, legitimately closed (no sender is ever
    /// held past this call) rather than inventing one.
    async fn subscribe(
        &self,
        _opts: SubscriberOptions,
    ) -> Result<mpsc::Receiver<Message>, crate::relay::Error> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(crate::relay::Error::Closed);
        }
        let (_tx, rx) = mpsc::channel::<Message>(1);
        // `_tx` is dropped here, closing `rx` immediately — see this
        // module's provenance note on `subscribe`.
        Ok(rx)
    }

    async fn close(&self) -> Result<(), crate::relay::Error> {
        self.closed.store(true, Ordering::SeqCst);
        Ok(())
    }
}

#[async_trait]
impl crate::relay::Caller for RcpAdapter {
    /// Dispatch `req` against the wrapped [`RcServer`] and return its reply
    /// as a `relay::Message`, per RELAY spec §10.2/§15.7.5.
    ///
    /// No bounded queue or semaphore sits in front of the dispatch below —
    /// each call is one `spawn_blocking` round trip against the wrapped
    /// [`RcServer`], bounded only by `ctx`'s deadline. See this module's
    /// "Back-pressure verification" note (above `adapt()`) for why: RELAY
    /// spec §10.5's `BackPressurePolicy` machinery is scoped to the
    /// `Node::subscribe` delivery channel, which this crate has not found
    /// an equivalent, spec-stated requirement for on the `Caller::call`
    /// path — flagged per Guiding Principle 5 rather than asserted as a
    /// closed question, since that absence has not been exhaustively
    /// confirmed against the rest of the spec.
    //fusa:req REQ-ADAPT-010
    async fn call(&self, ctx: Context, req: Message) -> Result<Message, crate::relay::Error> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(crate::relay::Error::Closed);
        }
        if ctx.done() {
            return Err(crate::relay::Error::Timeout);
        }
        let (stream_id, request) = from_message(&req).map_err(map_err)?;
        let server = Arc::clone(&self.server);
        let response = tokio::task::spawn_blocking(move || server.handle_abb(stream_id, &request))
            .await
            .map_err(|_| crate::relay::Error::Closed)?
            .map_err(map_err)?;
        Ok(response_to_message(stream_id, &response))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockEndpoint;
    use crate::regmap::{EndpointType, GeneralRegisters};
    use std::time::Duration;

    fn ok_endpoint() -> Arc<dyn Endpoint> {
        MockEndpoint::new(EndpointType::Gpio, vec![0u8; 8]) as Arc<dyn Endpoint>
    }

    fn stream(unique_id: u16) -> StreamId {
        StreamId::new([0x02, 0x11, 0x22, 0x33, 0x44, 0x55], unique_id)
    }

    /// A fresh `RcServer` with one `MockEndpoint` (initially holding
    /// `initial`) registered under `(stream, byte_bus_id)`.
    fn server_with_endpoint(
        stream_id: StreamId,
        byte_bus_id: u16,
        initial: Vec<u8>,
    ) -> Arc<RcServer> {
        let srv = RcServer::new(GeneralRegisters::default());
        srv.register_endpoint(
            stream_id,
            byte_bus_id,
            MockEndpoint::new(EndpointType::Gpio, initial),
        )
        .unwrap();
        srv
    }

    #[test]
    //fusa:test REQ-ADAPT-001
    //fusa:test REQ-ADAPT-004
    fn passthrough_adapter_identity() {
        let ep = AdaptEndpoint::new(ok_endpoint(), Arc::new(PassthroughAdapter));
        let out = ep.send_msg(b"hi".to_vec(), 8).unwrap();
        assert!(out.starts_with(b"hi"));
    }

    #[test]
    //fusa:test REQ-ADAPT-002
    fn ep_type_forwarded() {
        let inner = ok_endpoint();
        let ep = AdaptEndpoint::new(Arc::clone(&inner), Arc::new(PassthroughAdapter));
        assert_eq!(ep.inner.ep_type(), EndpointType::Gpio);
    }

    #[test]
    //fusa:test REQ-ADAPT-003
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
    //fusa:test REQ-ADAPT-005
    fn passthrough_preserves_payload() {
        let ep = AdaptEndpoint::new(ok_endpoint(), Arc::new(PassthroughAdapter));
        let out = ep.send_msg(b"data".to_vec(), 8).unwrap();
        assert!(out.starts_with(b"data"));
    }

    // ── endpoint-id encoding ───────────────────────────────────────────────

    #[test]
    //fusa:test REQ-ADAPT-011
    fn endpoint_id_roundtrips() {
        let sid = stream(0x1234);
        let encoded = format_endpoint_id(sid, 0x07FF);
        let (decoded_sid, decoded_bus) = parse_endpoint_id(&encoded).unwrap();
        assert_eq!(decoded_sid, sid);
        assert_eq!(decoded_bus, 0x07FF);
    }

    #[test]
    //fusa:test REQ-ADAPT-011
    fn parse_endpoint_id_rejects_malformed_input() {
        assert_eq!(
            parse_endpoint_id("not-an-address").unwrap_err(),
            RcpError::InvalidParameter
        );
        assert_eq!(
            parse_endpoint_id("zzzz.3").unwrap_err(),
            RcpError::InvalidParameter
        );
        assert_eq!(
            parse_endpoint_id("00000000000001ff.notanumber").unwrap_err(),
            RcpError::InvalidParameter
        );
    }

    // ── to_message / from_message / response_to_message (§15.7.5) ────────────

    #[test]
    //fusa:test REQ-ADAPT-006
    fn to_message_maps_address_op_and_payload() {
        let sid = stream(7);
        let resp = AcfAbbMessage {
            info: ByteMessageInfo {
                byte_bus_id: 9,
                op: true,
                ..Default::default()
            },
            payload: vec![0xAA],
        };
        let msg = to_message(sid, &resp);
        assert_eq!(msg.protocol, Protocol::Rcp);
        assert_eq!(msg.id, format_endpoint_id(sid, 9));
        assert_eq!(msg.payload, vec![0xAA]);
        assert_eq!(msg.meta.get("rcp.op"), Some(&"write".to_string()));
        // Default `ByteMessageInfo::evt`/`err` (op=true) classifies as
        // ResponseKind::Write per TC18 §11.3 Table 15/§11.3.2.
        assert_eq!(
            msg.meta.get("rcp.response_kind"),
            Some(&"write".to_string())
        );
    }

    #[test]
    //fusa:test REQ-ADAPT-006
    //fusa:test REQ-RESP-004
    fn to_message_surfaces_response_kind_for_acknowledge_and_error() {
        use crate::acf::{Evt, EVT_RESPONSE_ACKNOWLEDGE};

        let sid = stream(21);
        let ack_resp = AcfAbbMessage {
            info: ByteMessageInfo {
                byte_bus_id: 1,
                evt: Evt {
                    ack: (EVT_RESPONSE_ACKNOWLEDGE >> 3) != 0,
                    sub_opcode: EVT_RESPONSE_ACKNOWLEDGE & 0x7,
                },
                ..Default::default()
            },
            payload: vec![],
        };
        assert_eq!(
            to_message(sid, &ack_resp).meta.get("rcp.response_kind"),
            Some(&"acknowledge".to_string())
        );

        let err_resp = AcfAbbMessage {
            info: ByteMessageInfo {
                byte_bus_id: 1,
                err: true,
                ..Default::default()
            },
            payload: vec![],
        };
        assert_eq!(
            to_message(sid, &err_resp).meta.get("rcp.response_kind"),
            Some(&"error".to_string())
        );

        let read_resp = AcfAbbMessage {
            info: ByteMessageInfo {
                byte_bus_id: 1,
                op: false,
                err: false,
                ..Default::default()
            },
            payload: vec![0x01],
        };
        assert_eq!(
            to_message(sid, &read_resp).meta.get("rcp.response_kind"),
            Some(&"read".to_string())
        );
    }

    #[test]
    //fusa:test REQ-ADAPT-007
    fn from_message_defaults_op_from_payload_emptiness() {
        let write_msg = Message::new(Protocol::Rcp, format_endpoint_id(stream(1), 3), vec![0xAA]);
        let (_, write_req) = from_message(&write_msg).unwrap();
        assert!(write_req.info.op);

        let read_msg = Message::new(Protocol::Rcp, format_endpoint_id(stream(1), 3), vec![]);
        let (_, read_req) = from_message(&read_msg).unwrap();
        assert!(!read_req.info.op);
    }

    #[test]
    //fusa:test REQ-ADAPT-007
    fn from_message_honors_explicit_op_and_read_size_meta() {
        let mut msg = Message::new(Protocol::Rcp, format_endpoint_id(stream(1), 3), vec![]);
        msg.meta.insert("rcp.op".to_string(), "read".to_string());
        msg.meta
            .insert("rcp.read_size".to_string(), "16".to_string());
        let (sid, req) = from_message(&msg).unwrap();
        assert_eq!(sid, stream(1));
        assert_eq!(req.info.byte_bus_id, 3);
        assert!(!req.info.op);
        assert_eq!(req.info.read_size_segment.as_read_size(), 16);
    }

    #[test]
    //fusa:test REQ-ADAPT-007
    fn from_message_rejects_unknown_op_value() {
        let mut msg = Message::new(Protocol::Rcp, format_endpoint_id(stream(1), 3), vec![]);
        msg.meta
            .insert("rcp.op".to_string(), "sideways".to_string());
        assert_eq!(from_message(&msg).unwrap_err(), RcpError::InvalidParameter);
    }

    #[test]
    //fusa:test REQ-ADAPT-007
    fn from_message_rejects_malformed_id() {
        let msg = Message::new(Protocol::Rcp, "not-an-address", vec![]);
        assert_eq!(from_message(&msg).unwrap_err(), RcpError::InvalidParameter);
    }

    #[test]
    //fusa:test REQ-ADAPT-008
    fn response_to_message_matches_to_message() {
        let sid = stream(2);
        let resp = AcfAbbMessage {
            info: ByteMessageInfo {
                byte_bus_id: 5,
                op: false,
                ..Default::default()
            },
            payload: vec![0x01, 0x02],
        };
        let via_response = response_to_message(sid, &resp);
        let via_to_message = to_message(sid, &resp);
        assert_eq!(via_response.id, via_to_message.id);
        assert_eq!(via_response.payload, via_to_message.payload);
        assert_eq!(via_response.meta, via_to_message.meta);
    }

    // ── adapt() (§10.3) ────────────────────────────────────────────────────

    #[tokio::test]
    //fusa:test REQ-ADAPT-009
    //fusa:test REQ-ADAPT-010
    //fusa:test REQ-RELAY-008
    async fn adapt_call_write_then_read_round_trips_payload() {
        let sid = stream(11);
        let server = server_with_endpoint(sid, 4, vec![0u8; 8]);
        let node = adapt(server);

        let write_req = Message::new(Protocol::Rcp, format_endpoint_id(sid, 4), vec![1, 2, 3]);
        let write_reply = node.call(Context::background(), write_req).await.unwrap();
        assert_eq!(write_reply.id, format_endpoint_id(sid, 4));
        assert!(write_reply.payload.is_empty());
        assert_eq!(write_reply.meta.get("rcp.op"), Some(&"write".to_string()));
        assert_eq!(
            write_reply.meta.get("rcp.response_kind"),
            Some(&"write".to_string())
        );

        let mut read_req = Message::new(Protocol::Rcp, format_endpoint_id(sid, 4), vec![]);
        read_req
            .meta
            .insert("rcp.read_size".to_string(), "3".to_string());
        let read_reply = node.call(Context::background(), read_req).await.unwrap();
        assert_eq!(read_reply.payload, vec![1, 2, 3]);
        assert_eq!(read_reply.meta.get("rcp.op"), Some(&"read".to_string()));
        assert_eq!(
            read_reply.meta.get("rcp.response_kind"),
            Some(&"read".to_string())
        );
    }

    #[tokio::test]
    //fusa:test REQ-ADAPT-009
    async fn adapt_send_discards_response() {
        let sid = stream(12);
        let server = server_with_endpoint(sid, 4, vec![0u8; 8]);
        let node = adapt(server);
        let msg = Message::new(Protocol::Rcp, format_endpoint_id(sid, 4), vec![9]);
        crate::relay::Node::send(&*node, Context::background(), msg)
            .await
            .unwrap();
    }

    #[tokio::test]
    //fusa:test REQ-ADAPT-009
    async fn adapt_send_invalid_address_is_not_connected() {
        let sid = stream(13);
        let server = server_with_endpoint(sid, 4, vec![0u8; 8]);
        let node = adapt(server);
        let msg = Message::new(Protocol::Rcp, "not-an-address", vec![]);
        let err = crate::relay::Node::send(&*node, Context::background(), msg)
            .await
            .unwrap_err();
        assert_eq!(err, crate::relay::Error::NotConnected);
    }

    #[tokio::test]
    //fusa:test REQ-ADAPT-009
    async fn adapt_call_unknown_endpoint_is_not_connected() {
        let sid = stream(14);
        let server = server_with_endpoint(sid, 4, vec![0u8; 8]);
        let node = adapt(server);
        // byte_bus_id 5 was never registered.
        let msg = Message::new(Protocol::Rcp, format_endpoint_id(sid, 5), vec![]);
        let err = node.call(Context::background(), msg).await.unwrap_err();
        assert_eq!(err, crate::relay::Error::NotConnected);
    }

    #[tokio::test]
    //fusa:test REQ-ADAPT-009
    async fn adapt_call_already_expired_context_is_timeout() {
        let sid = stream(15);
        let server = server_with_endpoint(sid, 4, vec![0u8; 8]);
        let node = adapt(server);
        let msg = Message::new(Protocol::Rcp, format_endpoint_id(sid, 4), vec![]);
        let ctx = Context::with_timeout(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
        let err = node.call(ctx, msg).await.unwrap_err();
        assert_eq!(err, crate::relay::Error::Timeout);
    }

    #[tokio::test]
    //fusa:test REQ-ADAPT-009
    async fn adapt_subscribe_returns_immediately_closed_channel() {
        let sid = stream(16);
        let server = server_with_endpoint(sid, 4, vec![0u8; 8]);
        let node = adapt(server);
        let mut rx = crate::relay::Node::subscribe(&*node, SubscriberOptions::default())
            .await
            .unwrap();
        assert!(rx.recv().await.is_none());
    }

    #[tokio::test]
    //fusa:test REQ-ADAPT-009
    async fn adapt_close_then_call_is_closed() {
        let sid = stream(17);
        let server = server_with_endpoint(sid, 4, vec![0u8; 8]);
        let node = adapt(server);
        crate::relay::Node::close(&*node).await.unwrap();

        let msg = Message::new(Protocol::Rcp, format_endpoint_id(sid, 4), vec![]);
        let err = node.call(Context::background(), msg).await.unwrap_err();
        assert_eq!(err, crate::relay::Error::Closed);
    }

    #[test]
    //fusa:test REQ-ADAPT-009
    //fusa:test REQ-RELAY-007
    fn adapt_protocol_is_rcp() {
        let sid = stream(18);
        let server = server_with_endpoint(sid, 4, vec![0u8; 8]);
        let node = adapt(server);
        assert_eq!(crate::relay::Node::protocol(&*node), Protocol::Rcp);
    }
}
