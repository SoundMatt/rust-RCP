//fusa:req REQ-UDP-001
//fusa:req REQ-UDP-002
//fusa:req REQ-UDP-003
//fusa:req REQ-UDP-004
//fusa:req REQ-UDP-005
//fusa:req REQ-UDP-006
//fusa:req REQ-UDP-007
//fusa:req REQ-UDP-008
//fusa:req REQ-UDP-009
//fusa:req REQ-UDP-010
//fusa:req REQ-UDP-011
//fusa:req REQ-UDP-012
//fusa:req REQ-UDP-013
//fusa:req REQ-UDP-014
//fusa:req REQ-UDP-015
//fusa:req REQ-UDP-016
//fusa:req REQ-UDP-017
//fusa:req REQ-UDP-018
//fusa:req REQ-UDP-019
//fusa:req REQ-UDP-020
//fusa:req REQ-UDP-021
//fusa:req REQ-UDP-022
//fusa:req REQ-UDP-023
//fusa:req REQ-UDP-024
//fusa:req REQ-UDP-025
//fusa:req REQ-UDP-026
//fusa:req REQ-UDP-027
//fusa:req REQ-UDP-028
//fusa:req REQ-UDP-029
//fusa:req REQ-UDP-030

//! UDP unicast transport for the TC18 AVTPDU/ACF wire format.
//!
//! `ROADMAP.md` Milestone 9 (`wire` REPLACE-disposition cutover): the old
//! `Zone`/`Controller`-based `UdpBridge`, which serialized `Command`/
//! `Response` through the now-deleted `crate::wire` 16-byte frame, is
//! REPLACEd outright — deleted, not adapted, the same discipline
//! `src/watchdog.rs`/`src/powerstate.rs` used in Milestones 6/7 — with
//! [`UdpTransport`]: a transport addressed by [`crate::avtp::StreamId`]
//! instead of `Zone`, sending/receiving NTSCF-headed AVTPDU frames
//! ([`crate::avtp::encode_ntscf_frame`]/[`crate::avtp::decode_ntscf_frame`])
//! that carry an ACF_ABB or ACF_GBB payload ([`crate::acf`]), with
//! `byte_bus_id`-addressed routing resolved through [`resolve_endpoint`]
//! ([`crate::ep0::route_byte_bus_id`]/[`crate::addressing::EndpointTable`])
//! rather than a `Zone` lookup.
//!
//! That cutover closed `wire`'s own row of the Satellite Package
//! Disposition table (`ROADMAP.md`), not `udp`'s own — `udp` is
//! REPLACE-dispositioned in its own right, per the disposition table's own
//! reason: "every framing call must be rebuilt against Milestone 1's AVTPDU
//! encode/decode instead of `wire::encode_command`" is the framing half only.
//! [`UdpRcServer`], added by this item, closes the remainder the Milestone 9
//! Progress note named explicitly: "a real RC-Server-endpoint-level rebuild
//! (register-map-driven dispatch, discovery integration)". It is
//! [`UdpTransport`]'s server-side counterpart — where [`UdpTransport`] is a
//! client sending one request/response pair to some other, unmodeled party,
//! [`UdpRcServer`] is that other party: it drives an actual
//! [`crate::mock::RcServer`] register-map/lifecycle-gated dispatch engine
//! from real inbound datagrams ([`UdpSocket::recv_from`]), and separately
//! recognizes and answers [`crate::discovery`] broadcast reads and
//! discovery-stream configure/claim attempts, in any lifecycle state, per
//! that module's own "Multi-client coexistence" rules. See
//! [`UdpRcServer`]'s own doc comment for the full design, including the
//! judgment calls this item had to make where the roadmap and Milestone 1-3
//! plumbing left a real wire-level choice unstated.
//!
//! [`UdpTransport::send_acf_abb`]/[`UdpTransport::send_acf_gbb`] and
//! [`resolve_endpoint`] are unchanged by this item — see their own doc
//! comments.
//!
//! # Real OS-socket transport and IEEE 1722-2016 Annex J encapsulation
//!
//! Before this item, this module's only [`UdpSocket`] implementations were
//! in-process test doubles (`EchoUdp`/`QueuedUdpSocket`, both in this
//! module's own `#[cfg(test)]` block) — there was no concrete
//! implementation over a real OS socket anywhere in this crate;
//! `src/bin/rcp.rs`'s own pre-this-item doc comment said so explicitly.
//! [`StdUdpSocket`] closes that gap: this is the first real network I/O
//! this crate has ever shipped for RCP.
//!
//! TC18 §10.1 states AVTPDUs can be carried over UDP/IP, "Encapsulation of
//! 1722 frames in IP/UDP and port usage is described in Annex J" (of the
//! base IEEE 1722-2016 standard, not TC18 itself). This crate does not
//! have access to the paywalled IEEE 1722-2016 standard text; the framing
//! [`StdUdpSocket`]/[`encode_annex_j_udp_payload`]/
//! [`decode_annex_j_udp_payload`] implement — a 4-byte big-endian
//! "encapsulation sequence number" prepended to every UDP payload before
//! the AVTPDU itself, and control-plane traffic (RCP requests/responses,
//! which this crate is exclusively concerned with) using destination port
//! [`ANNEX_J_CONTROL_PORT`] (17221), distinct from port 17220 for
//! "Continuous" streaming traffic ([`ANNEX_J_CONTINUOUS_PORT`]) — is taken
//! from two independent public secondary sources instead: a Wireshark
//! issue tracker discussion of the real Annex J framing, and the COVESA
//! Open1722 open-source reference implementation's `Avtp_Udp_t` header
//! struct (`include/avtp/Udp.h`, BSD-3-Clause,
//! <https://github.com/COVESA/Open1722>). This is flagged here per
//! Guiding Principle 5 as *not* independently verified against the
//! primary standard, rather than presented with false certainty.
//!
//! The encapsulation sequence number's exact intended receiver-side
//! semantics (e.g. loss detection) are not specified by either secondary
//! source consulted, and this crate does not invent any — [`StdUdpSocket`]
//! only guarantees it is monotonically increasing per sender, nothing
//! more. This field exists only for UDP/IP encapsulation (Annex J); it has
//! no counterpart when an AVTPDU is instead carried directly at layer 2
//! with EtherType `0x22F0` — see [`crate::l2`], added alongside this item
//! as the other, equally-supported transport option TC18 §10.1 names.
//!
//! # [`UdpAvtpTransport`] — the shared [`AvtpTransport`] over UDP (issue #205)
//!
//! [`UdpAvtpTransport`] is the port of c-RCP's
//! `rcp_udp_avtp_transport_dial()`/`_bind()` (`c-RCP/include/rcp/udp.h:
//! 204-226`, `c-RCP/src/udp.c:103-377`): an [`AvtpTransport`] that
//! applies/strips the Annex J sequence number transparently. Dial mode
//! connects to one fixed peer (the kernel discards datagrams from anyone
//! else); bind mode starts with no peer and learns it from every received
//! datagram (`udp.c:250`), returning [`RcpError::Busy`] from
//! [`AvtpTransport::send_frame`] until one has arrived. Its receive loop
//! polls in 20 ms slices so [`AvtpTransport::close`] from another thread
//! unblocks it, skips transient socket errors and datagrams too short to
//! carry the Annex J sequence number (`udp.c:235-245`), and drops
//! (returning [`RcpError::Busy`]) a datagram too large for the caller's
//! buffer. [`UdpTransport`]'s `send_acf_abb`/`send_acf_gbb` are now thin
//! wrappers over [`crate::avtp::request_acf_abb`]/
//! [`crate::avtp::request_acf_gbb`], and [`UdpRcServer`] receives and
//! replies over any [`AvtpTransport`] (normally a bind-mode
//! [`UdpAvtpTransport`]).

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::acf::{self, AcfAbbMessage, AcfGbbMessage, ByteMessageInfo};
use crate::addressing::{EndpointId, EndpointTable};
use crate::avtp::{self, AvtpTransport, StreamId, AVTP_MAX_FRAME_LEN};
use crate::discovery::{self, DiscoveryAccessKind, DiscoveryClaim};
use crate::ep0::{self, RequestRoute};
use crate::mock::RcServer;
use crate::RcpError;

// ── UdpSocket trait ───────────────────────────────────────────────────────────

/// Abstract UDP socket for testability. Unchanged in shape from this
/// module's pre-Milestone-9 version — only the bytes carried over it
/// changed.
//fusa:req REQ-UDP-001
pub trait UdpSocket: Send + Sync {
    fn send_to(&self, buf: &[u8], addr: SocketAddr) -> Result<usize, RcpError>;
    fn recv_from(&self, timeout: Option<Duration>) -> Result<(Vec<u8>, SocketAddr), RcpError>;
}

// ── Annex J UDP encapsulation ────────────────────────────────────────────────

/// Standard destination UDP port for IEEE 1722-2016 Annex J "Discrete"
/// (control-plane) traffic — RCP requests/responses/acknowledgements are
/// control-plane traffic, so this is the applicable port for RCP-over-UDP
/// and [`StdUdpSocket::new_default_port`]'s default. See this module's own
/// doc comment, "Real OS-socket transport and IEEE 1722-2016 Annex J
/// encapsulation", for this constant's provenance (public secondary
/// sources, not the paywalled primary standard).
//fusa:req REQ-UDP-012
pub const ANNEX_J_CONTROL_PORT: u16 = 17221;

/// Standard destination UDP port for IEEE 1722-2016 Annex J "Continuous"
/// (streaming/periodic) traffic — not RCP's traffic class, and not used by
/// any constructor in this module; named here only so both of Annex J's
/// two standard ports are documented rather than one left unstated. Same
/// provenance note as [`ANNEX_J_CONTROL_PORT`].
pub const ANNEX_J_CONTINUOUS_PORT: u16 = 17220;

/// Prepend a 4-byte big-endian encapsulation sequence number to `avtpdu`
/// — TC18 §10.1's IEEE 1722-2016 Annex J UDP/IP encapsulation, per this
/// module's own doc comment provenance note. Byte order matches this
/// crate's existing big-endian wire convention (e.g.
/// [`crate::avtp::encode_ntscf_frame`]'s `stream_id` field,
/// [`crate::acf`]'s `message_timestamp`).
//fusa:req REQ-UDP-012
pub fn encode_annex_j_udp_payload(seq: u32, avtpdu: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(4 + avtpdu.len());
    buf.extend_from_slice(&seq.to_be_bytes());
    buf.extend_from_slice(avtpdu);
    buf
}

/// The inverse of [`encode_annex_j_udp_payload`]: split a raw UDP payload
/// into its 4-byte encapsulation sequence number and the AVTPDU bytes that
/// follow it. `Err(RcpError::ShortFrame)` for fewer than 4 bytes — never
/// panics on truncated or empty input.
//fusa:req REQ-UDP-012
pub fn decode_annex_j_udp_payload(buf: &[u8]) -> Result<(u32, &[u8]), RcpError> {
    if buf.len() < 4 {
        return Err(RcpError::ShortFrame);
    }
    let mut seq_bytes = [0u8; 4];
    seq_bytes.copy_from_slice(&buf[..4]);
    Ok((u32::from_be_bytes(seq_bytes), &buf[4..]))
}

// ── StdUdpSocket ──────────────────────────────────────────────────────────────

/// Real, production [`UdpSocket`] implementation over a bound
/// `std::net::UdpSocket`. See this module's own doc comment, "Real
/// OS-socket transport and IEEE 1722-2016 Annex J encapsulation", for full
/// context — this is the first concrete implementation of [`UdpSocket`]
/// over a real OS socket this crate has ever shipped.
///
/// `send_to` prepends, and `recv_from` strips, the 4-byte encapsulation
/// sequence number [`encode_annex_j_udp_payload`]/
/// [`decode_annex_j_udp_payload`] implement — entirely transparent to
/// [`UdpSocket`] trait callers (e.g. [`UdpTransport`]), which
/// see only already-framed NTSCF/AVTPDU bytes, the same contract the
/// mock `EchoUdp`/`QueuedUdpSocket` test doubles already provide.
/// `send_to`'s sequence number is a per-`StdUdpSocket` monotonically
/// increasing counter, starting at 0 on construction; it is not exposed to
/// callers (see this module's own doc comment for why no receiver-side
/// semantics are attached to it).
//fusa:req REQ-UDP-013
//fusa:req REQ-UDP-014
pub struct StdUdpSocket {
    socket: std::net::UdpSocket,
    send_seq: AtomicU32,
}

impl StdUdpSocket {
    /// Bind a real UDP socket to `local_addr`.
    //fusa:req REQ-UDP-013
    pub fn bind(local_addr: SocketAddr) -> Result<Self, RcpError> {
        let socket = std::net::UdpSocket::bind(local_addr)
            .map_err(|e| RcpError::Other(format!("udp: bind {local_addr}: {e}")))?;
        Ok(StdUdpSocket {
            socket,
            send_seq: AtomicU32::new(0),
        })
    }

    /// Convenience constructor: bind to `bind_ip` on
    /// [`ANNEX_J_CONTROL_PORT`] — the sensible default for RCP's
    /// control-plane traffic. [`Self::bind`] remains available directly
    /// for an explicit port (testing, or a deployment that cannot use the
    /// standard port).
    //fusa:req REQ-UDP-013
    pub fn new_default_port(bind_ip: std::net::IpAddr) -> Result<Self, RcpError> {
        Self::bind(SocketAddr::new(bind_ip, ANNEX_J_CONTROL_PORT))
    }

    /// The local address this socket is actually bound to — useful when
    /// [`Self::bind`]'s `local_addr` used an ephemeral (`:0`) port.
    pub fn local_addr(&self) -> Result<SocketAddr, RcpError> {
        self.socket
            .local_addr()
            .map_err(|e| RcpError::Other(format!("udp: local_addr: {e}")))
    }
}

impl UdpSocket for StdUdpSocket {
    /// Returns the number of bytes of `buf` (the caller-supplied
    /// NTSCF/AVTPDU frame) sent — not the larger on-wire byte count
    /// including the prepended encapsulation sequence number — matching
    /// this trait's existing mock-implementation convention of echoing
    /// `buf.len()` back rather than any wire-framing overhead.
    //fusa:req REQ-UDP-013
    fn send_to(&self, buf: &[u8], addr: SocketAddr) -> Result<usize, RcpError> {
        let seq = self.send_seq.fetch_add(1, Ordering::Relaxed);
        let framed = encode_annex_j_udp_payload(seq, buf);
        let sent = self
            .socket
            .send_to(&framed, addr)
            .map_err(|e| RcpError::Other(format!("udp: send_to {addr}: {e}")))?;
        Ok(sent.saturating_sub(4))
    }

    /// `timeout` is applied via `SO_RCVTIMEO` on every call. `None` blocks
    /// indefinitely. A real OS-level timeout is mapped to
    /// `Err(RcpError::Timeout)`, matching every other timeout path in this
    /// crate.
    //fusa:req REQ-UDP-014
    fn recv_from(&self, timeout: Option<Duration>) -> Result<(Vec<u8>, SocketAddr), RcpError> {
        self.socket
            .set_read_timeout(timeout)
            .map_err(|e| RcpError::Other(format!("udp: set_read_timeout: {e}")))?;
        let mut buf = [0u8; 65535];
        let (n, addr) = self.socket.recv_from(&mut buf).map_err(|e| {
            if matches!(
                e.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) {
                RcpError::Timeout
            } else {
                RcpError::Other(format!("udp: recv_from: {e}"))
            }
        })?;
        let (_seq, avtpdu) = decode_annex_j_udp_payload(&buf[..n])?;
        Ok((avtpdu.to_vec(), addr))
    }
}

// ── UdpAvtpTransport ─────────────────────────────────────────────────────────

/// Mutable state guarded by one lock, as c-RCP's `mu` guards
/// `closed`/`peer`/`has_peer`/`send_seq`/`recv_seq` (`udp.c:108-122`).
struct UdpAvtpState {
    closed: bool,
    peer: Option<SocketAddr>,
    send_seq: u32,
    recv_seq: Option<u32>,
}

/// The shared [`AvtpTransport`] over a real UDP/IPv4 socket — port of
/// c-RCP's `rcp_udp_avtp_transport_t` (`c-RCP/src/udp.c:103-377`). See
/// this module's doc comment, "[`UdpAvtpTransport`] — the shared
/// [`AvtpTransport`] over UDP".
///
/// c-RCP returns a non-NULL transport whose `rcp_udp_avtp_transport_ok()`
/// is false on a socket/connect/bind failure; the constructors here return
/// `Err(RcpError::Other(_))` instead (the rust idiom for the same
/// distinction). Hosts are IPv4 dotted-quad strings, as c-RCP's
/// `inet_pton(AF_INET, ...)` requires.
//fusa:req REQ-UDP-015
//fusa:req REQ-UDP-016
pub struct UdpAvtpTransport {
    socket: std::net::UdpSocket,
    bound_mode: bool,
    time_sync_supported: bool,
    state: Mutex<UdpAvtpState>,
}

impl UdpAvtpTransport {
    fn from_socket(socket: std::net::UdpSocket, bound_mode: bool, tss: bool) -> Self {
        UdpAvtpTransport {
            socket,
            bound_mode,
            time_sync_supported: tss,
            state: Mutex::new(UdpAvtpState {
                closed: false,
                peer: None,
                send_seq: 0,
                recv_seq: None,
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, UdpAvtpState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Connect a UDP socket to `host:port` (`rcp_udp_avtp_transport_dial()`,
    /// `udp.c:296-324`). `host` must be an IPv4 address.
    //fusa:req REQ-UDP-015
    //fusa:req REQ-UDP-026
    pub fn dial(host: &str, port: u16, time_sync_supported: bool) -> Result<Self, RcpError> {
        let ip: Ipv4Addr = host
            .parse()
            .map_err(|_| RcpError::Other(format!("udp: dial: invalid IPv4 host {host:?}")))?;
        let socket = std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
            .map_err(|e| RcpError::Other(format!("udp: dial: socket: {e}")))?;
        socket
            .connect((ip, port))
            .map_err(|e| RcpError::Other(format!("udp: dial: connect {ip}:{port}: {e}")))?;
        Ok(Self::from_socket(socket, false, time_sync_supported))
    }

    /// Bind a UDP socket to `addr:port` (`rcp_udp_avtp_transport_bind()`,
    /// `udp.c:329-358`); an empty `addr` binds `INADDR_ANY`, port `0` lets
    /// the OS pick one (see [`Self::local_port`]).
    //fusa:req REQ-UDP-016
    //fusa:req REQ-UDP-026
    pub fn bind(addr: &str, port: u16, time_sync_supported: bool) -> Result<Self, RcpError> {
        let ip: Ipv4Addr = if addr.is_empty() {
            Ipv4Addr::UNSPECIFIED
        } else {
            addr.parse()
                .map_err(|_| RcpError::Other(format!("udp: bind: invalid IPv4 addr {addr:?}")))?
        };
        let socket = std::net::UdpSocket::bind((ip, port))
            .map_err(|e| RcpError::Other(format!("udp: bind {ip}:{port}: {e}")))?;
        Ok(Self::from_socket(socket, true, time_sync_supported))
    }

    /// [`Self::dial`] on [`ANNEX_J_CONTROL_PORT`] (`udp.c:360-366`).
    //fusa:req REQ-UDP-029
    pub fn dial_default_port(host: &str, time_sync_supported: bool) -> Result<Self, RcpError> {
        Self::dial(host, ANNEX_J_CONTROL_PORT, time_sync_supported)
    }

    /// [`Self::bind`] on [`ANNEX_J_CONTROL_PORT`] (`udp.c:368-374`).
    //fusa:req REQ-UDP-029
    pub fn bind_default_port(addr: &str, time_sync_supported: bool) -> Result<Self, RcpError> {
        Self::bind(addr, ANNEX_J_CONTROL_PORT, time_sync_supported)
    }

    /// The local address this transport's socket is bound to
    /// (`rcp_udp_avtp_transport_addr_string()`, `udp.c:393-410`).
    //fusa:req REQ-UDP-017
    pub fn local_addr(&self) -> Result<SocketAddr, RcpError> {
        self.socket
            .local_addr()
            .map_err(|e| RcpError::Other(format!("udp: local_addr: {e}")))
    }

    /// The local port this transport's socket is bound to, or `0` if it
    /// cannot be read (`rcp_udp_avtp_transport_port()`, `udp.c:383-391`).
    //fusa:req REQ-UDP-017
    pub fn local_port(&self) -> u16 {
        self.local_addr().map(|a| a.port()).unwrap_or(0)
    }

    /// The Annex J sequence number of the most recently received datagram,
    /// `0` before any (`rcp_udp_avtp_transport_last_recv_seq()`,
    /// `udp.c:412-421`).
    //fusa:req REQ-UDP-030
    pub fn last_recv_seq(&self) -> u32 {
        self.lock().recv_seq.unwrap_or(0)
    }

    /// A second handle to this transport's own socket (`try_clone`), e.g.
    /// for [`crate::tsn::TsnTransport`]'s `SO_PRIORITY` tagging — the rust
    /// counterpart of handing c-RCP's `rcp_tsn_avtp_transport_new()` the
    /// transport's socket fd.
    //fusa:req REQ-UDP-017
    pub fn try_clone_socket(&self) -> Result<std::net::UdpSocket, RcpError> {
        self.socket
            .try_clone()
            .map_err(|e| RcpError::Other(format!("udp: try_clone: {e}")))
    }
}

impl AvtpTransport for UdpAvtpTransport {
    /// Prepend this transport's own monotonically-incrementing Annex J
    /// sequence number and send (`udp_avtp_send()`, `udp.c:131-176`).
    /// Bind mode targets the learned peer and returns `Err(Busy)` before
    /// one is known; any socket error is `Err(Closed)`.
    ///
    /// c-RCP parity (`c-RCP/src/udp.c:154-157`): the sequence number is
    /// consumed even when the send then fails with `Busy` (no peer learned
    /// yet), so the first datagram a bound transport actually emits need
    /// not carry sequence number 0.
    //fusa:req REQ-UDP-018
    //fusa:req REQ-UDP-020
    //fusa:req REQ-UDP-021
    //fusa:req REQ-UDP-024
    //fusa:req REQ-UDP-027
    fn send_frame(&self, frame: &[u8]) -> Result<(), RcpError> {
        let (dest, seq) = {
            let mut st = self.lock();
            if st.closed {
                return Err(RcpError::Closed);
            }
            let seq = st.send_seq;
            st.send_seq = st.send_seq.wrapping_add(1);
            (st.peer, seq)
        };
        let sent = if self.bound_mode {
            let dest = dest.ok_or(RcpError::Busy)?;
            self.socket
                .send_to(&encode_annex_j_udp_payload(seq, frame), dest)
        } else {
            self.socket.send(&encode_annex_j_udp_payload(seq, frame))
        };
        sent.map(|_| ()).map_err(|_| RcpError::Closed)
    }

    /// Poll for one datagram until `deadline` (`udp_avtp_recv()`,
    /// `udp.c:184-271`): transient socket errors and datagrams shorter than
    /// the Annex J sequence number are skipped; in bind mode the sender
    /// becomes the peer; a datagram larger than `buf` is dropped and
    /// `Err(Busy)` returned.
    //fusa:req REQ-UDP-018
    //fusa:req REQ-UDP-019
    //fusa:req REQ-UDP-022
    //fusa:req REQ-UDP-023
    //fusa:req REQ-UDP-024
    //fusa:req REQ-UDP-025
    //fusa:req REQ-UDP-028
    //fusa:req REQ-UDP-030
    fn recv_frame(&self, deadline: Option<Instant>, buf: &mut [u8]) -> Result<usize, RcpError> {
        let mut tmp = vec![0u8; 4 + AVTP_MAX_FRAME_LEN];
        loop {
            if self.lock().closed {
                return Err(RcpError::Closed);
            }
            if avtp::deadline_done(deadline) {
                return Err(RcpError::Timeout);
            }
            if self
                .socket
                .set_read_timeout(Some(avtp::poll_slice(deadline)))
                .is_err()
            {
                continue;
            }
            // Poll slice elapsed, or a transient recv error (e.g. a
            // connected socket's ICMP port-unreachable): keep polling.
            let Ok((n, from)) = self.socket.recv_from(&mut tmp) else {
                continue;
            };
            // Too short to carry the Annex J sequence number: dropped.
            let Ok((seq, payload)) = decode_annex_j_udp_payload(&tmp[..n]) else {
                continue;
            };
            {
                let mut st = self.lock();
                if self.bound_mode {
                    st.peer = Some(from);
                }
                st.recv_seq = Some(seq);
            }
            let dst = buf.get_mut(..payload.len()).ok_or(RcpError::Busy)?;
            dst.copy_from_slice(payload);
            return Ok(payload.len());
        }
    }

    /// Raise the closed flag; the socket itself is closed on drop
    /// (`udp_avtp_close()`, `udp.c:275-283`).
    //fusa:req REQ-UDP-024
    //fusa:req REQ-UDP-025
    fn close(&self) -> Result<(), RcpError> {
        self.lock().closed = true;
        Ok(())
    }

    //fusa:req REQ-UDP-026
    fn time_sync_supported(&self) -> bool {
        self.time_sync_supported
    }
}

/// [`UdpTransport`]'s compatibility adapter: one [`UdpSocket`] plus a fixed
/// remote, as an [`AvtpTransport`]. A receive is a single
/// [`UdpSocket::recv_from`] call whose error is returned unchanged — this
/// wrapper's historical behavior.
struct UdpSocketLink<'a> {
    socket: &'a dyn UdpSocket,
    remote: SocketAddr,
}

impl AvtpTransport for UdpSocketLink<'_> {
    fn send_frame(&self, frame: &[u8]) -> Result<(), RcpError> {
        self.socket.send_to(frame, self.remote).map(|_| ())
    }

    fn recv_frame(&self, deadline: Option<Instant>, buf: &mut [u8]) -> Result<usize, RcpError> {
        // A deadline that has already passed is a timeout, not a zero
        // socket timeout (which `std::net::UdpSocket::set_read_timeout`
        // rejects, and which `SO_RCVTIMEO` reads as "block forever").
        let timeout = match avtp::remaining(deadline) {
            Some(r) if r.is_zero() => return Err(RcpError::Timeout),
            other => other,
        };
        let (frame, _) = self.socket.recv_from(timeout)?;
        let dst = buf.get_mut(..frame.len()).ok_or(RcpError::Busy)?;
        dst.copy_from_slice(&frame);
        Ok(frame.len())
    }

    fn close(&self) -> Result<(), RcpError> {
        Ok(())
    }

    fn time_sync_supported(&self) -> bool {
        false
    }
}

// ── UdpTransport ─────────────────────────────────────────────────────────────

/// RCP-over-UDP transport, addressed by `local_stream`
/// ([`crate::avtp::StreamId`]) rather than the legacy `Zone`.
///
/// `sequence_num` (the NTSCF header's per-stream sequence counter) is a
/// caller-supplied value on every send rather than state this struct owns —
/// matching this crate's existing discipline of taking such values as
/// explicit parameters (e.g. `crate::discovery`'s `now: Instant`) rather
/// than hiding a counter/clock behind an method that looks pure.
//fusa:req REQ-UDP-002
pub struct UdpTransport {
    local_stream: StreamId,
    socket: Arc<dyn UdpSocket>,
    remote: SocketAddr,
}

impl UdpTransport {
    /// Construct a transport bound to `local_stream`, sending to `remote`
    /// over `socket`.
    pub fn new(local_stream: StreamId, socket: Arc<dyn UdpSocket>, remote: SocketAddr) -> Self {
        UdpTransport {
            local_stream,
            socket,
            remote,
        }
    }

    /// This transport's local [`StreamId`].
    pub fn local_stream(&self) -> StreamId {
        self.local_stream
    }

    fn link(&self) -> UdpSocketLink<'_> {
        UdpSocketLink {
            socket: self.socket.as_ref(),
            remote: self.remote,
        }
    }

    /// Send an ACF_ABB request wrapped in an NTSCF frame addressed under
    /// `local_stream`, and decode the ACF_ABB response, verifying it
    /// echoes the request's `byte_bus_id`
    /// ([`crate::acf::verify_echo_back`]).
    ///
    /// Returns `Err(RcpError::Timeout)` immediately for a zero `timeout`,
    /// matching this module's pre-Milestone-9 behavior.
    //fusa:req REQ-UDP-003
    //fusa:req REQ-UDP-004
    //fusa:req REQ-WIRE-006
    pub fn send_acf_abb(
        &self,
        msg: &AcfAbbMessage,
        sequence_num: u8,
        timeout: Option<Duration>,
    ) -> Result<AcfAbbMessage, RcpError> {
        avtp::request_acf_abb(&self.link(), self.local_stream, msg, sequence_num, timeout)
    }

    /// Same as [`Self::send_acf_abb`], for an ACF_GBB request/response pair.
    //fusa:req REQ-UDP-003
    //fusa:req REQ-UDP-004
    //fusa:req REQ-WIRE-006
    pub fn send_acf_gbb(
        &self,
        msg: &AcfGbbMessage,
        sequence_num: u8,
        timeout: Option<Duration>,
    ) -> Result<AcfGbbMessage, RcpError> {
        avtp::request_acf_gbb(&self.link(), self.local_stream, msg, sequence_num, timeout)
    }

    /// No-op, matching this module's pre-Milestone-9 behavior.
    //fusa:req REQ-UDP-005
    pub fn close(&self) -> Result<(), RcpError> {
        Ok(())
    }
}

// ── Endpoint routing ──────────────────────────────────────────────────────────

/// Where a `(stream_id, byte_bus_id)`-addressed request actually resolves
/// to, once [`crate::addressing::EndpointTable`] has been consulted for the
/// `DeviceEndpoint` case — unlike [`crate::ep0::RequestRoute`], which stops
/// at the routing decision itself and never performs the lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
//fusa:req REQ-UDP-006
pub enum ResolvedEndpoint {
    /// `byte_bus_id` was the reserved EP0 address.
    Ep0,
    /// `byte_bus_id` resolved, via `endpoints`, to this device endpoint.
    Device(EndpointId),
}

/// Resolve `byte_bus_id`, under `stream_id`, to a [`ResolvedEndpoint`],
/// composing [`crate::ep0::route_byte_bus_id`] with `endpoints` — the
/// Milestone 9 "route addressing through `EndpointTable`/
/// `route_byte_bus_id` instead of the old `Zone` field" item, applied to
/// this transport's addressing.
///
/// Returns `Err(RcpError::EpNotFound)` if `byte_bus_id` is not the reserved
/// EP0 address and no endpoint is registered under `(stream_id,
/// byte_bus_id)`. Never panics for any input.
//fusa:req REQ-UDP-006
//fusa:req REQ-UDP-007
pub fn resolve_endpoint(
    endpoints: &EndpointTable,
    stream_id: StreamId,
    byte_bus_id: u16,
) -> Result<ResolvedEndpoint, RcpError> {
    match ep0::route_byte_bus_id(byte_bus_id) {
        RequestRoute::Ep0 => Ok(ResolvedEndpoint::Ep0),
        RequestRoute::DeviceEndpoint => endpoints
            .lookup(stream_id, byte_bus_id)
            .map(ResolvedEndpoint::Device)
            .ok_or(RcpError::EpNotFound),
    }
}

// ── UdpRcServer ──────────────────────────────────────────────────────────────

/// `UdpTransport`'s server-side counterpart: drives a
/// [`crate::mock::RcServer`] register-map/lifecycle-gated dispatch engine
/// from real inbound UDP datagrams, closing `udp`'s own still-open REPLACE
/// row (`ROADMAP.md` Milestone 9's "a real RC-Server-endpoint-level rebuild:
/// register-map-driven dispatch, discovery integration").
///
/// # Composing `mock::RcServer` from non-test code — a flagged judgment call
///
/// Per this crate's Guiding Principle 5, this is called out explicitly
/// rather than silently assumed: [`crate::mock::RcServer`] lives in a module
/// whose own doc comment describes it as "in-process test doubles," yet
/// [`UdpRcServer`] — real, non-test dispatch code — depends on it directly
/// rather than lifting/duplicating its `handle_abb` logic into a new home.
/// Three reasons this crate's own precedent favors reuse over duplication
/// here:
///
/// 1. `mock.rs`'s own doc comment already anticipated this exact caller —
///    "leaving `udp`'s own still-open deeper rebuild as the most likely next
///    caller, per `ROADMAP.md`'s own Progress note for this bullet" — so
///    this is not a new architectural decision this item is making
///    unilaterally; it is executing one `mock.rs` already flagged.
/// 2. `RcServer` is a plain `pub` item in a plain `pub mod mock` (not behind
///    `#[cfg(test)]`) — nothing about its compilation is test-only, only its
///    *name* and doc comment suggest that. Duplicating its `handle_abb`
///    routing (EP0 gating, `EndpointTable` lookup, echo-back verification)
///    into a second copy here would create two independently-maintained
///    implementations of the same dispatch rule, actively working against
///    this crate's own "reuse, don't duplicate" discipline (see
///    [`crate::discovery::is_discovery_configure_request`]'s own doc comment
///    for the same call made the other direction).
/// 3. `RcServer`'s only real "mock" characteristic is [`crate::mock::Endpoint`]
///    having exactly one implementation ([`crate::mock::MockEndpoint`]) —
///    a limitation `UdpRcServer` inherits unchanged, not one it introduces.
///
/// This item does not rename or relocate `RcServer`/`mock.rs` — that is a
/// separate, not-yet-scoped cleanup (renaming `mock` once it stops being
/// exclusively test-only is a naming question, not a dispatch-logic one) —
/// flagged here for whichever later item takes it up, rather than bundled
/// silently into this one.
///
/// # Discovery integration
///
/// [`Self::serve_one`] recognizes three request shapes, checked in this
/// order, before falling through to normal [`RcServer::handle_abb`]
/// dispatch:
///
/// 1. [`crate::discovery::is_discovery_request`] — a broadcast-or-direct
///    discovery read. Answered via [`crate::discovery::build_discovery_response`]
///    in any lifecycle state, gated (as a formality —
///    [`DiscoveryAccessKind::Read`] never actually rejects) through
///    [`crate::discovery::check_discovery_access`] so a future change to
///    either function's contract is caught by this module's own tests
///    rather than by a caller, mirroring [`RcServer::handle_abb`]'s own
///    `verify_echo_back` discipline.
/// 2. [`crate::discovery::is_discovery_configure_request`] (added by this
///    item — see its own doc comment) — a discovery-stream configure/claim
///    attempt. Gated via [`crate::discovery::check_discovery_access`]
///    with [`DiscoveryAccessKind::Configure`], then — only once that
///    succeeds — actually granted/refreshed via
///    [`crate::discovery::try_claim_discovery_stream`], whose result becomes
///    this server's newly held claim. Composing both rather than either
///    alone demonstrates they agree, the same discipline item 1 already
///    applies.
/// 3. Anything else arriving under
///    [`crate::discovery::DISCOVERY_BROADCAST_STREAM_ID`] is rejected with
///    `Err(RcpError::InvalidParameter)` without ever reaching
///    `RcServer::handle_abb` — the broadcast sentinel names no single real
///    client (per `crate::discovery`'s own Provenance note), so dispatching
///    it as if it were one (e.g. an EP0 root-client check, or an
///    `EndpointTable` lookup keyed by the sentinel) would misuse the
///    sentinel rather than honor its documented meaning.
///
/// Every other request — including a *direct* (non-broadcast-addressed)
/// discovery read, which is legitimate per `crate::discovery`'s own
/// "Multi-client coexistence" section — falls through to
/// [`RcServer::handle_abb`] unchanged.
///
/// # Response frame addressing — a flagged judgment call
///
/// Per Guiding Principle 5: neither `ROADMAP.md` nor any Milestone 1-3 item
/// states which [`StreamId`] a *response* frame's NTSCF header should carry.
/// [`Self::serve_one`] always addresses the response frame under this
/// server's own [`Self::local_stream`], never under the request frame's
/// stream_id (which, per this crate's `stream_id` convention — "always
/// identifying one specific sender" — identifies the *requesting client*,
/// not this server). Two consequences this item treats as intentional
/// rather than incidental:
///
/// - A discovery response's frame carries the server's real identity even
///   when the matching request arrived under
///   [`crate::discovery::DISCOVERY_BROADCAST_STREAM_ID`] — this is exactly
///   how a client is meant to *learn* a server's real `StreamId` from a
///   broadcast discovery exchange, to key its own
///   [`crate::discovery::DiscoveryCache`] entry by afterward.
/// - Every other response likewise carries this server's identity, not the
///   requester's — consistent with `stream_id` always naming a frame's
///   sender, on both the request and the response leg.
///
/// # Transport (issue #205)
///
/// The server receives and replies over any shared [`AvtpTransport`],
/// normally a bind-mode [`UdpAvtpTransport`]: that transport learns the
/// requester's address from each received datagram (`udp.c:250`) and
/// [`AvtpTransport::send_frame`] replies to it, so the response goes back
/// to whichever address the request arrived from.
pub struct UdpRcServer {
    local_stream: StreamId,
    transport: Arc<dyn AvtpTransport>,
    server: Arc<RcServer>,
    discovery_claim: Mutex<Option<DiscoveryClaim>>,
}

impl UdpRcServer {
    /// Construct a server addressed as `local_stream`, receiving/sending
    /// over `transport` (normally [`UdpAvtpTransport::bind`]), dispatching
    /// non-discovery requests through `server`.
    ///
    /// Starts with no discovery-stream claim held by anyone, matching
    /// [`crate::discovery::try_claim_discovery_stream`]'s own "`current` is
    /// `None`" unclaimed starting condition.
    //fusa:req REQ-UDP-008
    //fusa:req REQ-UDP-031
    pub fn new(
        local_stream: StreamId,
        transport: Arc<dyn AvtpTransport>,
        server: Arc<RcServer>,
    ) -> Self {
        UdpRcServer {
            local_stream,
            transport,
            server,
            discovery_claim: Mutex::new(None),
        }
    }

    /// This server's local [`StreamId`] — the identity every response frame
    /// this server sends is addressed under (see this type's own doc
    /// comment, "Response frame addressing").
    pub fn local_stream(&self) -> StreamId {
        self.local_stream
    }

    /// The [`crate::mock::RcServer`] this server dispatches non-discovery
    /// requests through.
    pub fn rc_server(&self) -> &Arc<RcServer> {
        &self.server
    }

    /// The discovery-stream claim this server currently holds, if any.
    pub fn discovery_claim(&self) -> Option<DiscoveryClaim> {
        *self.discovery_claim.lock().unwrap()
    }

    /// Receive one inbound frame (via [`AvtpTransport::recv_frame`]), decode
    /// it as an NTSCF-framed ACF_ABB request, dispatch it, and send the
    /// NTSCF-framed ACF_ABB response back over the same transport (for a
    /// bind-mode [`UdpAvtpTransport`], to whichever address the request
    /// arrived from).
    ///
    /// `recv_timeout` bounds the receive: it becomes the
    /// [`AvtpTransport::recv_frame`] deadline (`None` waits without limit),
    /// so the transport's own error handling applies (transient socket
    /// errors and short datagrams are polled past; `Err(Timeout)` once the
    /// deadline passes; `Err(Closed)` after close). `sequence_num` is this response frame's
    /// NTSCF `sequence_num` — a caller-supplied value, matching
    /// [`UdpTransport::send_acf_abb`]'s own "no hidden counter" discipline
    /// (this deliberately does not reuse [`crate::mock::RcServer`]'s own
    /// internal free-running counter, which that module's own doc comment
    /// already flags as "this test double's own simplification, not a spec
    /// requirement" — `UdpRcServer`, non-test code, holds this crate's
    /// stricter explicit-parameter line instead). `now`/`discovery_timeout`
    /// are threaded straight through to every `crate::discovery` call this
    /// method makes, matching that module's own "no real-clock read of its
    /// own" discipline.
    ///
    /// This method handles exactly one request-frame/response-frame cycle;
    /// it spawns no thread and runs no loop of its own, matching every
    /// other transport in this crate (`UdpTransport` included) — a caller
    /// composes it into whatever receive loop or async task fits its own
    /// runtime. Returns whatever error the receive or decode step first
    /// produces (there is no request context yet to answer those on the
    /// wire); a per-request dispatch failure with a TC18 Table 27 wire code
    /// is instead answered with a real `err=1` response frame
    /// (rust-RCP-W04, [`acf::build_error_response`]) rather than returned
    /// to this method's own caller — see [`Self::dispatch_request`]'s doc
    /// comment.
    ///
    /// A single inbound *frame* may carry more than one concatenated
    /// ACF_ABB request (TC18 §12.9.1.1, "Handling multiple requests in
    /// incoming messages" — rust-RCP-W03); this method uses
    /// [`acf::decode_acf_abb_messages`] and dispatches each decoded request
    /// through [`Self::dispatch_request`] in wire order, concatenating
    /// every response into the one outgoing frame in the same order. See
    /// [`crate::mock::RcServer::handle_ntscf_frame`]'s own doc comment for
    /// the same multi-request handling.
    //fusa:req REQ-UDP-008
    //fusa:req REQ-UDP-009
    //fusa:req REQ-UDP-010
    //fusa:req REQ-UDP-011
    //fusa:req REQ-UDP-031
    pub fn serve_one(
        &self,
        recv_timeout: Option<Duration>,
        sequence_num: u8,
        now: Instant,
        discovery_timeout: Duration,
    ) -> Result<(), RcpError> {
        let deadline = recv_timeout.map(|t| Instant::now() + t);
        let mut frame = vec![0u8; AVTP_MAX_FRAME_LEN];
        let n = self.transport.recv_frame(deadline, &mut frame)?;
        let (hdr, acf_bytes) = avtp::decode_ntscf_frame(&frame[..n])?;
        let requester_stream = StreamId::from_u64(hdr.stream_id);
        let requests = acf::decode_acf_abb_messages(acf_bytes)?;

        let mut response_bytes = Vec::new();
        for request in &requests {
            let response =
                match self.dispatch_request(requester_stream, request, now, discovery_timeout) {
                    Ok(response) => response,
                    Err(e) => match acf::build_error_response(&request.info, &e) {
                        Some(error_response) => error_response,
                        None => return Err(e),
                    },
                };
            response_bytes.extend_from_slice(&acf::encode_acf_abb(&response)?);
        }

        let response_frame =
            avtp::encode_ntscf_frame(self.local_stream, sequence_num, &response_bytes)?;
        self.transport.send_frame(&response_frame)?;
        Ok(())
    }

    /// The decoded-request half of [`Self::serve_one`] — see this type's own
    /// doc comment, "Discovery integration", for the three-case recognition
    /// order this implements.
    //fusa:req REQ-UDP-008
    //fusa:req REQ-UDP-009
    //fusa:req REQ-UDP-010
    //fusa:req REQ-UDP-011
    fn dispatch_request(
        &self,
        requester_stream: StreamId,
        request: &AcfAbbMessage,
        now: Instant,
        discovery_timeout: Duration,
    ) -> Result<AcfAbbMessage, RcpError> {
        if discovery::is_discovery_request(request) {
            discovery::check_discovery_access(
                self.discovery_claim(),
                requester_stream,
                DiscoveryAccessKind::Read,
                now,
                discovery_timeout,
            )?;
            let state = self.server.state();
            let general = self.server.general_registers();
            return discovery::build_discovery_response(&request.info, state, &general);
        }

        if discovery::is_discovery_configure_request(request) {
            let mut claim_guard = self.discovery_claim.lock().unwrap();
            discovery::check_discovery_access(
                *claim_guard,
                requester_stream,
                DiscoveryAccessKind::Configure,
                now,
                discovery_timeout,
            )?;
            let claim = discovery::try_claim_discovery_stream(
                *claim_guard,
                requester_stream,
                now,
                discovery_timeout,
            )?;
            *claim_guard = Some(claim);
            drop(claim_guard);

            let response_info = acf::build_response_info(&request.info, ByteMessageInfo::default());
            return Ok(AcfAbbMessage {
                info: response_info,
                payload: Vec::new(),
            });
        }

        if discovery::is_discovery_broadcast_stream_id(requester_stream) {
            return Err(RcpError::InvalidParameter);
        }

        self.server.handle_abb(requester_stream, request)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::acf::ByteMessageInfo;

    // ── Annex J encapsulation (pure byte manipulation, no socket) ─────────

    #[test]
    //fusa:test REQ-UDP-012
    fn annex_j_encode_decode_round_trips() {
        let avtpdu = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x01];
        let encoded = encode_annex_j_udp_payload(7, &avtpdu);
        assert_eq!(encoded.len(), 4 + avtpdu.len());
        // Big-endian, matching this crate's existing wire convention.
        assert_eq!(&encoded[..4], &[0x00, 0x00, 0x00, 0x07]);
        let (seq, decoded) = decode_annex_j_udp_payload(&encoded).unwrap();
        assert_eq!(seq, 7);
        assert_eq!(decoded, avtpdu.as_slice());
    }

    #[test]
    //fusa:test REQ-UDP-012
    fn annex_j_encode_handles_empty_avtpdu() {
        let encoded = encode_annex_j_udp_payload(0xFFFF_FFFF, &[]);
        assert_eq!(encoded, vec![0xFF, 0xFF, 0xFF, 0xFF]);
        let (seq, decoded) = decode_annex_j_udp_payload(&encoded).unwrap();
        assert_eq!(seq, 0xFFFF_FFFF);
        assert!(decoded.is_empty());
    }

    #[test]
    //fusa:test REQ-UDP-012
    fn annex_j_decode_rejects_short_buffers() {
        for len in 0..4 {
            let buf = vec![0u8; len];
            let err = decode_annex_j_udp_payload(&buf).unwrap_err();
            assert_eq!(err, RcpError::ShortFrame);
        }
    }

    #[test]
    fn annex_j_control_and_continuous_ports_are_distinct_and_documented() {
        assert_eq!(ANNEX_J_CONTROL_PORT, 17221);
        assert_eq!(ANNEX_J_CONTINUOUS_PORT, 17220);
        assert_ne!(ANNEX_J_CONTROL_PORT, ANNEX_J_CONTINUOUS_PORT);
    }

    // ── StdUdpSocket (real loopback sockets — no privileges required) ─────

    #[test]
    //fusa:test REQ-UDP-013
    //fusa:test REQ-UDP-014
    fn std_udp_socket_round_trips_over_real_loopback_socket() {
        let a = StdUdpSocket::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let b = StdUdpSocket::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let b_addr = b.local_addr().unwrap();

        let payload = vec![0x01, 0x02, 0x03, 0x04, 0x05];
        UdpSocket::send_to(&a, &payload, b_addr).unwrap();

        let (received, _from) = UdpSocket::recv_from(&b, Some(Duration::from_secs(5))).unwrap();
        assert_eq!(received, payload);
    }

    #[test]
    //fusa:test REQ-UDP-013
    //fusa:test REQ-UDP-014
    //fusa:test REQ-UDP-031
    fn std_udp_socket_and_udp_rc_server_serve_a_real_discovery_request_end_to_end() {
        // The same composition `src/bin/rcp.rs`'s `serve` command builds
        // (a bind-mode UdpAvtpTransport + UdpRcServer), with a real
        // StdUdpSocket client and the server talking over real loopback
        // sockets — proving both Annex J paths interoperate end-to-end
        // through UdpRcServer's own request dispatch, and that the reply
        // goes back to the learned requester address.
        use crate::regmap::GeneralRegisters;

        let server_stream = StreamId::new([0x02, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA], 0x00AA);
        let server_transport = UdpAvtpTransport::bind("127.0.0.1", 0, false).unwrap();
        let server_addr = server_transport.local_addr().unwrap();
        let general = GeneralRegisters {
            svr_vendor_id: 0x4242,
            ..Default::default()
        };
        let rc_server = RcServer::new(general);
        let server = UdpRcServer::new(server_stream, Arc::new(server_transport), rc_server);

        let client_stream = local_stream();
        let client_socket = StdUdpSocket::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let request = discovery::build_discovery_request(0x11);
        let payload = acf::encode_acf_abb(&request).unwrap();
        let frame = avtp::encode_ntscf_frame(client_stream, 0, &payload).unwrap();
        UdpSocket::send_to(&client_socket, &frame, server_addr).unwrap();

        server
            .serve_one(
                Some(Duration::from_secs(5)),
                0,
                Instant::now(),
                discovery::DISCOVERY_TIME_OUT,
            )
            .unwrap();

        let (resp_frame, _from) =
            UdpSocket::recv_from(&client_socket, Some(Duration::from_secs(5))).unwrap();
        let (hdr, acf_bytes) = avtp::decode_ntscf_frame(&resp_frame).unwrap();
        assert_eq!(StreamId::from_u64(hdr.stream_id), server_stream);
        let resp = acf::decode_acf_abb(acf_bytes).unwrap();
        let regs = GeneralRegisters::decode(&resp.payload).unwrap();
        assert_eq!(regs.svr_vendor_id, 0x4242);
    }

    #[test]
    //fusa:test REQ-UDP-031
    fn udp_rc_server_serve_one_returns_the_transports_timeout_and_closed() {
        use crate::regmap::GeneralRegisters;

        let server_stream = StreamId::new([0x02, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA], 0x00AA);
        let transport = Arc::new(UdpAvtpTransport::bind("127.0.0.1", 0, false).unwrap());
        let server = UdpRcServer::new(
            server_stream,
            transport.clone(),
            RcServer::new(GeneralRegisters::default()),
        );

        let start = Instant::now();
        let err = server
            .serve_one(
                Some(Duration::from_millis(50)),
                0,
                Instant::now(),
                discovery::DISCOVERY_TIME_OUT,
            )
            .unwrap_err();
        assert_eq!(err, RcpError::Timeout);
        assert!(start.elapsed() >= Duration::from_millis(50));

        transport.close().unwrap();
        let err = server
            .serve_one(None, 0, Instant::now(), discovery::DISCOVERY_TIME_OUT)
            .unwrap_err();
        assert_eq!(err, RcpError::Closed);
    }

    #[test]
    //fusa:test REQ-UDP-013
    fn std_udp_socket_send_seq_is_monotonically_increasing_on_the_wire() {
        // Inspect the real encapsulated bytes with a plain std socket
        // (bypassing StdUdpSocket's own recv_from, which strips the
        // sequence number) to prove send_to's sequence counter actually
        // increments on the wire, not just that decode_annex_j_udp_payload
        // can parse whatever value is there.
        let sender = StdUdpSocket::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let raw_receiver = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let receiver_addr = raw_receiver.local_addr().unwrap();

        for _ in 0..3 {
            UdpSocket::send_to(&sender, &[0xAA], receiver_addr).unwrap();
        }

        let mut seqs = Vec::new();
        let mut buf = [0u8; 64];
        for _ in 0..3 {
            let (n, _) = raw_receiver.recv_from(&mut buf).unwrap();
            let (seq, avtpdu) = decode_annex_j_udp_payload(&buf[..n]).unwrap();
            assert_eq!(avtpdu, &[0xAA]);
            seqs.push(seq);
        }
        assert_eq!(seqs, vec![0, 1, 2]);
    }

    #[test]
    fn std_udp_socket_recv_from_times_out_on_a_real_socket() {
        let socket = StdUdpSocket::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let err = socket
            .recv_from(Some(Duration::from_millis(50)))
            .unwrap_err();
        assert_eq!(err, RcpError::Timeout);
    }

    #[test]
    fn std_udp_socket_new_default_port_binds_annex_j_control_port() {
        // Bind to an ephemeral port instead of the real 17221 so this test
        // doesn't require exclusive access to a well-known port / root on
        // some platforms; the constructor logic under test is the address
        // construction itself, exercised via `bind` with an explicit
        // ANNEX_J_CONTROL_PORT.
        let addr: SocketAddr = format!("127.0.0.1:{ANNEX_J_CONTROL_PORT}").parse().unwrap();
        // A CI runner or developer machine may already have something
        // bound to the real control port, or lack permission; either is
        // an environment fact, not a bug in this constructor, so only the
        // success case is asserted on.
        if let Ok(socket) = StdUdpSocket::bind(addr) {
            assert_eq!(socket.local_addr().unwrap().port(), ANNEX_J_CONTROL_PORT);
        }
    }

    fn local_stream() -> StreamId {
        StreamId::new([0x02, 0x11, 0x22, 0x33, 0x44, 0x55], 0x0001)
    }

    /// A mock socket that echoes back a well-formed ACF_ABB response,
    /// copying `byte_bus_id` from whatever request it received (satisfying
    /// the echo-back rule) unless `mismatch` is set, in which case it
    /// deliberately returns a different `byte_bus_id`.
    struct EchoUdp {
        mismatch: bool,
    }

    impl UdpSocket for EchoUdp {
        fn send_to(&self, buf: &[u8], _addr: SocketAddr) -> Result<usize, RcpError> {
            Ok(buf.len())
        }

        fn recv_from(&self, _timeout: Option<Duration>) -> Result<(Vec<u8>, SocketAddr), RcpError> {
            let byte_bus_id = if self.mismatch { 99 } else { 7 };
            let resp = AcfAbbMessage {
                info: ByteMessageInfo {
                    byte_bus_id,
                    rsp: true,
                    ..Default::default()
                },
                payload: vec![0xAA],
            };
            let payload = acf::encode_acf_abb(&resp).unwrap();
            let frame = avtp::encode_ntscf_frame(local_stream(), 1, &payload).unwrap();
            let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
            Ok((frame, addr))
        }
    }

    fn request(byte_bus_id: u16) -> AcfAbbMessage {
        AcfAbbMessage {
            info: ByteMessageInfo {
                byte_bus_id,
                op: true,
                ..Default::default()
            },
            payload: vec![0x01, 0x02],
        }
    }

    // ── send_acf_abb ───────────────────────────────────────────────────────

    #[test]
    //fusa:test REQ-UDP-001
    //fusa:test REQ-UDP-002
    //fusa:test REQ-UDP-003
    //fusa:test REQ-WIRE-006
    fn send_acf_abb_round_trips_over_socket() {
        let addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();
        let socket = Arc::new(EchoUdp { mismatch: false });
        let transport = UdpTransport::new(local_stream(), socket, addr);
        let resp = transport.send_acf_abb(&request(7), 0, None).unwrap();
        assert_eq!(resp.info.byte_bus_id, 7);
        assert!(resp.info.rsp);
    }

    #[test]
    //fusa:test REQ-UDP-004
    fn send_acf_abb_rejects_echo_back_mismatch() {
        let addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();
        let socket = Arc::new(EchoUdp { mismatch: true });
        let transport = UdpTransport::new(local_stream(), socket, addr);
        let err = transport.send_acf_abb(&request(7), 0, None).unwrap_err();
        assert_eq!(err, RcpError::EpError);
    }

    #[test]
    //fusa:test REQ-UDP-003
    fn send_acf_abb_rejects_zero_timeout() {
        let addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();
        let socket = Arc::new(EchoUdp { mismatch: false });
        let transport = UdpTransport::new(local_stream(), socket, addr);
        let err = transport
            .send_acf_abb(&request(7), 0, Some(Duration::ZERO))
            .unwrap_err();
        assert_eq!(err, RcpError::Timeout);
    }

    /// A socket whose recv rejects a zero timeout the way
    /// `std::net::UdpSocket::set_read_timeout` does.
    struct ZeroTimeoutRejectingUdp;

    impl UdpSocket for ZeroTimeoutRejectingUdp {
        fn send_to(&self, buf: &[u8], _addr: SocketAddr) -> Result<usize, RcpError> {
            Ok(buf.len())
        }

        fn recv_from(&self, timeout: Option<Duration>) -> Result<(Vec<u8>, SocketAddr), RcpError> {
            if timeout == Some(Duration::ZERO) {
                return Err(RcpError::Other("zero read timeout".into()));
            }
            EchoUdp { mismatch: false }.recv_from(timeout)
        }
    }

    #[test]
    //fusa:test REQ-UDP-003
    fn udp_socket_link_maps_an_already_passed_deadline_to_timeout() {
        let socket = ZeroTimeoutRejectingUdp;
        let link = UdpSocketLink {
            socket: &socket,
            remote: "127.0.0.1:9000".parse().unwrap(),
        };
        let mut buf = vec![0u8; AVTP_MAX_FRAME_LEN];
        // The request deadline has already passed when the receive starts
        // (a tiny timeout can elapse between computing the deadline and
        // the receive).
        let err = link.recv_frame(Some(Instant::now()), &mut buf).unwrap_err();
        assert_eq!(err, RcpError::Timeout);
        // A live deadline still receives normally.
        let deadline = Some(Instant::now() + Duration::from_secs(5));
        assert!(link.recv_frame(deadline, &mut buf).unwrap() > 0);
    }

    #[test]
    //fusa:test REQ-UDP-002
    fn local_stream_getter_matches_constructor() {
        let addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();
        let socket = Arc::new(EchoUdp { mismatch: false });
        let sid = local_stream();
        let transport = UdpTransport::new(sid, socket, addr);
        assert_eq!(transport.local_stream(), sid);
    }

    #[test]
    //fusa:test REQ-UDP-005
    fn close_is_noop() {
        let addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();
        let socket = Arc::new(EchoUdp { mismatch: false });
        let transport = UdpTransport::new(local_stream(), socket, addr);
        assert!(transport.close().is_ok());
        assert!(transport.close().is_ok());
    }

    // ── resolve_endpoint ───────────────────────────────────────────────────

    #[test]
    //fusa:test REQ-UDP-006
    fn resolve_endpoint_routes_ep0() {
        let endpoints = EndpointTable::new();
        let resolved = resolve_endpoint(&endpoints, local_stream(), ep0::EP0_BYTE_BUS_ID).unwrap();
        assert_eq!(resolved, ResolvedEndpoint::Ep0);
    }

    #[test]
    //fusa:test REQ-UDP-006
    //fusa:test REQ-UDP-007
    fn resolve_endpoint_routes_registered_device_endpoint() {
        let mut endpoints = EndpointTable::new();
        let sid = local_stream();
        endpoints.register(sid, 7, EndpointId(42)).unwrap();
        let resolved = resolve_endpoint(&endpoints, sid, 7).unwrap();
        assert_eq!(resolved, ResolvedEndpoint::Device(EndpointId(42)));
    }

    #[test]
    //fusa:test REQ-UDP-007
    fn resolve_endpoint_rejects_unregistered_device_endpoint() {
        let endpoints = EndpointTable::new();
        let err = resolve_endpoint(&endpoints, local_stream(), 7).unwrap_err();
        assert_eq!(err, RcpError::EpNotFound);
    }

    #[test]
    //fusa:test REQ-UDP-007
    fn resolve_endpoint_does_not_leak_across_streams() {
        let mut endpoints = EndpointTable::new();
        let sid_a = local_stream();
        let sid_b = StreamId::new([0x02, 0x11, 0x22, 0x33, 0x44, 0x66], 0x0002);
        endpoints.register(sid_a, 7, EndpointId(1)).unwrap();
        let err = resolve_endpoint(&endpoints, sid_b, 7).unwrap_err();
        assert_eq!(err, RcpError::EpNotFound);
    }

    // ── UdpRcServer ──────────────────────────────────────────────────────────

    mod rc_server_tests {
        use super::*;
        use crate::mock::{Endpoint, MockEndpoint};
        use crate::regmap::{EndpointType, GeneralRegisters};
        use std::collections::VecDeque;

        /// A test double [`AvtpTransport`] for `UdpRcServer`, shaped like a
        /// bind-mode [`UdpAvtpTransport`]: `recv_frame` yields queued
        /// inbound `(frame, peer_addr)` pairs one at a time, learning
        /// `peer_addr` as the reply peer — `Err(RcpError::Timeout)` once the
        /// queue is empty, mirroring a real transport timing out with
        /// nothing to receive — and `send_frame` records every outbound
        /// `(frame, learned peer)` pair for a test's own later inspection.
        struct QueuedUdpSocket {
            inbound: Mutex<VecDeque<(Vec<u8>, SocketAddr)>>,
            outbound: Mutex<Vec<(Vec<u8>, SocketAddr)>>,
            peer: Mutex<Option<SocketAddr>>,
        }

        impl QueuedUdpSocket {
            fn with_inbound(frames: Vec<(Vec<u8>, SocketAddr)>) -> Arc<Self> {
                Arc::new(Self {
                    inbound: Mutex::new(frames.into()),
                    outbound: Mutex::new(Vec::new()),
                    peer: Mutex::new(None),
                })
            }

            fn sent(&self) -> Vec<(Vec<u8>, SocketAddr)> {
                self.outbound.lock().unwrap().clone()
            }
        }

        impl AvtpTransport for QueuedUdpSocket {
            fn send_frame(&self, frame: &[u8]) -> Result<(), RcpError> {
                let peer = (*self.peer.lock().unwrap()).ok_or(RcpError::Busy)?;
                self.outbound.lock().unwrap().push((frame.to_vec(), peer));
                Ok(())
            }

            fn recv_frame(
                &self,
                _deadline: Option<Instant>,
                buf: &mut [u8],
            ) -> Result<usize, RcpError> {
                let (frame, from) = self
                    .inbound
                    .lock()
                    .unwrap()
                    .pop_front()
                    .ok_or(RcpError::Timeout)?;
                *self.peer.lock().unwrap() = Some(from);
                buf[..frame.len()].copy_from_slice(&frame);
                Ok(frame.len())
            }

            fn close(&self) -> Result<(), RcpError> {
                Ok(())
            }

            fn time_sync_supported(&self) -> bool {
                false
            }
        }

        fn server_stream() -> StreamId {
            StreamId::new([0x02, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA], 0x00AA)
        }

        fn client_stream(unique_id: u16) -> StreamId {
            StreamId::new([0x02, 0x11, 0x22, 0x33, 0x44, 0x55], unique_id)
        }

        fn client_addr() -> SocketAddr {
            "127.0.0.1:9000".parse().unwrap()
        }

        fn frame_for(stream_id: StreamId, msg: &AcfAbbMessage) -> Vec<u8> {
            let payload = acf::encode_acf_abb(msg).unwrap();
            avtp::encode_ntscf_frame(stream_id, 0, &payload).unwrap()
        }

        fn decode_response(bytes: &[u8]) -> (StreamId, AcfAbbMessage) {
            let (hdr, acf_bytes) = avtp::decode_ntscf_frame(bytes).unwrap();
            let msg = acf::decode_acf_abb(acf_bytes).unwrap();
            (StreamId::from_u64(hdr.stream_id), msg)
        }

        fn abb(byte_bus_id: u16, op: bool, payload: Vec<u8>) -> AcfAbbMessage {
            AcfAbbMessage {
                info: ByteMessageInfo {
                    byte_bus_id,
                    op,
                    ..Default::default()
                },
                payload,
            }
        }

        // ── construction / accessors ────────────────────────────────────

        #[test]
        //fusa:test REQ-UDP-008
        fn new_server_holds_no_discovery_claim() {
            let socket = QueuedUdpSocket::with_inbound(Vec::new());
            let rc = RcServer::new(GeneralRegisters::default());
            let srv = UdpRcServer::new(server_stream(), socket, rc);
            assert_eq!(srv.local_stream(), server_stream());
            assert_eq!(srv.discovery_claim(), None);
        }

        // ── register-map-driven dispatch ─────────────────────────────────

        #[test]
        //fusa:test REQ-UDP-008
        fn serve_one_dispatches_ep0_read_through_rc_server() {
            let general = GeneralRegisters {
                svr_vendor_id: 0x1234,
                ..Default::default()
            };
            let rc = RcServer::new(general);
            let request = abb(ep0::EP0_BYTE_BUS_ID, false, Vec::new());
            let frame = frame_for(client_stream(1), &request);
            let socket = QueuedUdpSocket::with_inbound(vec![(frame, client_addr())]);
            let srv = UdpRcServer::new(server_stream(), socket.clone(), rc);

            srv.serve_one(None, 0, Instant::now(), discovery::DISCOVERY_TIME_OUT)
                .unwrap();

            let sent = socket.sent();
            assert_eq!(sent.len(), 1);
            let (bytes, addr) = &sent[0];
            assert_eq!(*addr, client_addr());
            let (resp_stream, resp) = decode_response(bytes);
            // Response frames are always addressed under the server's own
            // identity, never the requester's — see UdpRcServer's own doc
            // comment, "Response frame addressing".
            assert_eq!(resp_stream, server_stream());
            assert_eq!(resp.payload, general.encode().to_vec());
            assert!(resp.info.rsp);
        }

        #[test]
        //fusa:test REQ-UDP-008
        fn serve_one_dispatches_device_endpoint_write_through_rc_server() {
            let rc = RcServer::new(GeneralRegisters::default());
            let sid = client_stream(2);
            let endpoint = MockEndpoint::new(EndpointType::Gpio, vec![0; 4]);
            rc.register_endpoint(sid, 7, endpoint.clone()).unwrap();

            let request = abb(7, true, vec![0xDE, 0xAD, 0xBE, 0xEF]);
            let frame = frame_for(sid, &request);
            let socket = QueuedUdpSocket::with_inbound(vec![(frame, client_addr())]);
            let srv = UdpRcServer::new(server_stream(), socket.clone(), rc);

            srv.serve_one(None, 0, Instant::now(), discovery::DISCOVERY_TIME_OUT)
                .unwrap();

            assert_eq!(endpoint.read(4).unwrap(), vec![0xDE, 0xAD, 0xBE, 0xEF]);
            let sent = socket.sent();
            let (_, resp) = decode_response(&sent[0].0);
            assert!(resp.payload.is_empty());
        }

        #[test]
        //fusa:test REQ-UDP-008
        fn serve_one_dispatches_multiple_requests_concatenated_in_one_frame() {
            // TC18 §12.9.1.1: an RC Server must support multiple requests
            // concatenated in a single frame (rust-RCP-W03).
            let rc = RcServer::new(GeneralRegisters::default());
            let sid = client_stream(2);
            let ep_a = MockEndpoint::new(EndpointType::Gpio, vec![0xAA, 0xBB, 0xCC, 0xDD]);
            let ep_b = MockEndpoint::new(EndpointType::Gpio, vec![0x11, 0x22, 0x33, 0x44]);
            rc.register_endpoint(sid, 3, ep_a).unwrap();
            rc.register_endpoint(sid, 4, ep_b).unwrap();

            let mut req_a = abb(3, false, Vec::new());
            req_a.info.read_size_segment = acf::ReadSizeOrSegment(4);
            req_a.info.transaction_num = 0x01;
            let mut req_b = abb(4, false, Vec::new());
            req_b.info.read_size_segment = acf::ReadSizeOrSegment(4);
            req_b.info.transaction_num = 0x02;

            let mut body = acf::encode_acf_abb(&req_a).unwrap();
            body.extend_from_slice(&acf::encode_acf_abb(&req_b).unwrap());
            let frame = avtp::encode_ntscf_frame(sid, 0, &body).unwrap();
            let socket = QueuedUdpSocket::with_inbound(vec![(frame, client_addr())]);
            let srv = UdpRcServer::new(server_stream(), socket.clone(), rc);

            srv.serve_one(None, 0, Instant::now(), discovery::DISCOVERY_TIME_OUT)
                .unwrap();

            let sent = socket.sent();
            assert_eq!(sent.len(), 1);
            let (bytes, _addr) = &sent[0];
            let (_hdr, acf_bytes) = avtp::decode_ntscf_frame(bytes).unwrap();
            let responses = acf::decode_acf_abb_messages(acf_bytes).unwrap();
            assert_eq!(responses.len(), 2);
            assert_eq!(responses[0].info.transaction_num, 0x01);
            assert_eq!(responses[0].payload, vec![0xAA, 0xBB, 0xCC, 0xDD]);
            assert_eq!(responses[1].info.transaction_num, 0x02);
            assert_eq!(responses[1].payload, vec![0x11, 0x22, 0x33, 0x44]);
        }

        #[test]
        //fusa:test REQ-UDP-008
        fn serve_one_answers_unregistered_endpoint_with_a_wire_error_response() {
            // rust-RCP-W04: EpNotFound has a TC18 Table 27 wire code, so it
            // is answered with a real err=1 response frame, not just
            // propagated to serve_one's own caller as a local Result.
            let rc = RcServer::new(GeneralRegisters::default());
            let request = abb(9, false, Vec::new());
            let frame = frame_for(client_stream(3), &request);
            let socket = QueuedUdpSocket::with_inbound(vec![(frame, client_addr())]);
            let srv = UdpRcServer::new(server_stream(), socket.clone(), rc);

            srv.serve_one(None, 0, Instant::now(), discovery::DISCOVERY_TIME_OUT)
                .unwrap();

            let sent = socket.sent();
            let (_, resp) = decode_response(&sent[0].0);
            assert!(resp.info.err);
            assert_eq!(resp.payload, vec![8]); // EP_NOT_FOUND = 8
        }

        // ── discovery integration: broadcast read ────────────────────────

        #[test]
        //fusa:test REQ-UDP-009
        fn serve_one_answers_broadcast_discovery_request_in_any_lifecycle_state() {
            use crate::lifecycle::RcServerState;

            for state in [
                RcServerState::HwUnconfigured,
                RcServerState::HwConfigured,
                RcServerState::RcpConfigured,
            ] {
                let general = GeneralRegisters {
                    svr_vendor_id: 0x0102,
                    ..Default::default()
                };
                let rc = RcServer::new(general);
                // RcServerState::try_transition has no direct
                // HwUnconfigured -> RcpConfigured edge, so reaching
                // RcpConfigured needs an intermediate stop at HwConfigured.
                let snap = crate::lifecycle::PlausibilitySnapshot::default();
                let root = crate::lifecycle::WriterCtx {
                    via_root_client_ep0: true,
                    ..Default::default()
                };
                if state != RcServerState::HwUnconfigured {
                    rc.try_transition(RcServerState::HwConfigured, Some(&snap), root, true)
                        .unwrap();
                }
                if state == RcServerState::RcpConfigured {
                    rc.try_transition(RcServerState::RcpConfigured, Some(&snap), root, true)
                        .unwrap();
                }
                assert_eq!(rc.state(), state);

                let request = discovery::build_discovery_request(0x11);
                let frame = frame_for(discovery::DISCOVERY_BROADCAST_STREAM_ID, &request);
                let socket = QueuedUdpSocket::with_inbound(vec![(frame, client_addr())]);
                let srv = UdpRcServer::new(server_stream(), socket.clone(), rc);

                srv.serve_one(None, 0, Instant::now(), discovery::DISCOVERY_TIME_OUT)
                    .unwrap_or_else(|e| panic!("discovery must answer in {state:?}: {e:?}"));

                let sent = socket.sent();
                let (resp_stream, resp) = decode_response(&sent[0].0);
                // The client learns the server's real identity from the
                // response frame, even though its own request was
                // addressed under the broadcast sentinel.
                assert_eq!(resp_stream, server_stream());
                assert_eq!(resp.payload, general.encode().to_vec());
            }
        }

        #[test]
        //fusa:test REQ-UDP-009
        fn serve_one_answers_a_direct_non_broadcast_discovery_request_too() {
            let rc = RcServer::new(GeneralRegisters::default());
            let request = discovery::build_discovery_request(0x22);
            let frame = frame_for(client_stream(4), &request);
            let socket = QueuedUdpSocket::with_inbound(vec![(frame, client_addr())]);
            let srv = UdpRcServer::new(server_stream(), socket.clone(), rc);

            srv.serve_one(None, 0, Instant::now(), discovery::DISCOVERY_TIME_OUT)
                .unwrap();

            let sent = socket.sent();
            let (resp_stream, _resp) = decode_response(&sent[0].0);
            assert_eq!(resp_stream, server_stream());
        }

        // ── discovery integration: configure / claim ─────────────────────

        #[test]
        //fusa:test REQ-UDP-010
        fn serve_one_grants_a_discovery_configure_claim_to_the_first_requester() {
            let rc = RcServer::new(GeneralRegisters::default());
            let mut request = discovery::build_discovery_request(0);
            request.info.op = true; // discovery::is_discovery_configure_request shape
            let claimant = client_stream(5);
            let frame = frame_for(claimant, &request);
            let socket = QueuedUdpSocket::with_inbound(vec![(frame, client_addr())]);
            let srv = UdpRcServer::new(server_stream(), socket, rc);

            srv.serve_one(None, 0, Instant::now(), discovery::DISCOVERY_TIME_OUT)
                .unwrap();

            let claim = srv.discovery_claim().expect("claim must be recorded");
            assert_eq!(claim.claimant(), claimant);
        }

        #[test]
        //fusa:test REQ-UDP-010
        fn serve_one_rejects_a_different_live_claimant() {
            let rc = RcServer::new(GeneralRegisters::default());
            let now = Instant::now();

            let mut first = discovery::build_discovery_request(0);
            first.info.op = true;
            let first_frame = frame_for(client_stream(6), &first);

            let mut second = discovery::build_discovery_request(0);
            second.info.op = true;
            let second_frame = frame_for(client_stream(7), &second);

            let socket = QueuedUdpSocket::with_inbound(vec![
                (first_frame, client_addr()),
                (second_frame, client_addr()),
            ]);
            let srv = UdpRcServer::new(server_stream(), socket.clone(), rc);

            // client_stream(6) claims the discovery stream first.
            srv.serve_one(None, 0, now, discovery::DISCOVERY_TIME_OUT)
                .unwrap();
            assert_eq!(srv.discovery_claim().unwrap().claimant(), client_stream(6));

            // client_stream(7) attempts to configure while that claim is
            // still live and is rejected with a wire error response
            // (UnauthorizedAccess has a TC18 Table 27 wire code —
            // rust-RCP-W04); the existing claim is unaffected.
            let still_live = now + (discovery::DISCOVERY_TIME_OUT / 2);
            srv.serve_one(None, 0, still_live, discovery::DISCOVERY_TIME_OUT)
                .unwrap();
            let sent = socket.sent();
            let (_, resp) = decode_response(&sent[1].0);
            assert!(resp.info.err);
            assert_eq!(resp.payload, vec![3]); // UNAUTHORIZED_ACCESS = 3
            assert_eq!(srv.discovery_claim().unwrap().claimant(), client_stream(6));
        }

        // ── broadcast sentinel misuse ─────────────────────────────────────

        #[test]
        //fusa:test REQ-UDP-011
        fn serve_one_rejects_a_non_discovery_request_under_the_broadcast_sentinel() {
            let rc = RcServer::new(GeneralRegisters::default());
            let request = abb(7, false, Vec::new());
            let frame = frame_for(discovery::DISCOVERY_BROADCAST_STREAM_ID, &request);
            let socket = QueuedUdpSocket::with_inbound(vec![(frame, client_addr())]);
            let srv = UdpRcServer::new(server_stream(), socket.clone(), rc);

            // InvalidParameter has a TC18 Table 27 wire code, so this is
            // answered with a wire error response, not propagated as a
            // local Result (rust-RCP-W04).
            srv.serve_one(None, 0, Instant::now(), discovery::DISCOVERY_TIME_OUT)
                .unwrap();
            let sent = socket.sent();
            let (_, resp) = decode_response(&sent[0].0);
            assert!(resp.info.err);
            assert_eq!(resp.payload, vec![15]); // INVALID_PARAMETER = 15
        }
    }

    // ── UdpAvtpTransport (real loopback sockets; `udp_avtp_socket_` prefix
    //    is listed in scripts/miri-skips.txt — Miri has no SOCK_DGRAM) ────

    fn secs(n: u64) -> Option<Instant> {
        Some(Instant::now() + Duration::from_secs(n))
    }

    fn bound_and_dialed() -> (UdpAvtpTransport, UdpAvtpTransport) {
        let server = UdpAvtpTransport::bind("127.0.0.1", 0, false).unwrap();
        let client = UdpAvtpTransport::dial("127.0.0.1", server.local_port(), true).unwrap();
        (server, client)
    }

    #[test]
    //fusa:test REQ-UDP-015
    //fusa:test REQ-UDP-016
    //fusa:test REQ-UDP-017
    //fusa:test REQ-UDP-018
    //fusa:test REQ-UDP-019
    //fusa:test REQ-UDP-021
    //fusa:test REQ-UDP-026
    //fusa:test REQ-UDP-027
    fn udp_avtp_socket_dial_bind_round_trip_and_peer_learning() {
        let (server, client) = bound_and_dialed();
        assert!(client.time_sync_supported());
        assert!(!server.time_sync_supported());
        assert_ne!(server.local_port(), 0);
        assert_eq!(
            server.local_addr().unwrap().ip(),
            std::net::IpAddr::V4(Ipv4Addr::LOCALHOST)
        );

        client.send_frame(&[0x82, 1, 2, 3]).unwrap();
        let mut buf = [0u8; 64];
        let n = server.recv_frame(secs(5), &mut buf).unwrap();
        assert_eq!(&buf[..n], &[0x82, 1, 2, 3]);
        assert_eq!(server.last_recv_seq(), 0);

        // The bound side learned the client's address and replies to it.
        server.send_frame(&[0xAA]).unwrap();
        let n = client.recv_frame(secs(5), &mut buf).unwrap();
        assert_eq!(&buf[..n], &[0xAA]);

        // The client's own per-connection counter advances on the wire.
        client.send_frame(&[]).unwrap();
        assert_eq!(server.recv_frame(secs(5), &mut buf).unwrap(), 0);
        assert_eq!(server.last_recv_seq(), 1);
    }

    #[test]
    //fusa:test REQ-UDP-020
    //fusa:test REQ-UDP-027
    fn udp_avtp_socket_bound_send_before_peer_is_busy_and_consumes_a_seq() {
        let server = UdpAvtpTransport::bind("127.0.0.1", 0, false).unwrap();
        assert_eq!(server.send_frame(&[1]), Err(RcpError::Busy));
        let raw = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        raw.send_to(&[0, 0, 0, 9, 0x55], ("127.0.0.1", server.local_port()))
            .unwrap();
        let mut buf = [0u8; 8];
        assert_eq!(server.recv_frame(secs(5), &mut buf).unwrap(), 1);
        assert_eq!(server.last_recv_seq(), 9);
        server.send_frame(&[2]).unwrap();
        let mut wire = [0u8; 16];
        raw.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let (n, _) = raw.recv_from(&mut wire).unwrap();
        // c-RCP parity: the Busy send above consumed sequence number 0.
        assert_eq!(&wire[..n], &[0, 0, 0, 1, 2]);
    }

    #[test]
    //fusa:test REQ-UDP-022
    fn udp_avtp_socket_recv_times_out_at_the_deadline() {
        let server = UdpAvtpTransport::bind("", 0, false).unwrap();
        let mut buf = [0u8; 8];
        let start = Instant::now();
        let deadline = Some(Instant::now() + Duration::from_millis(50));
        assert_eq!(
            server.recv_frame(deadline, &mut buf),
            Err(RcpError::Timeout)
        );
        assert!(start.elapsed() >= Duration::from_millis(50));
    }

    #[test]
    //fusa:test REQ-UDP-023
    fn udp_avtp_socket_too_small_buffer_is_busy_and_drops_the_datagram() {
        let (server, client) = bound_and_dialed();
        client.send_frame(&[1, 2, 3, 4]).unwrap();
        let mut small = [0u8; 2];
        assert_eq!(server.recv_frame(secs(5), &mut small), Err(RcpError::Busy));
        // Dropped, not retrievable afterward.
        let mut buf = [0u8; 8];
        let soon = Some(Instant::now() + Duration::from_millis(50));
        assert_eq!(server.recv_frame(soon, &mut buf), Err(RcpError::Timeout));
    }

    #[test]
    //fusa:test REQ-UDP-024
    fn udp_avtp_socket_closed_send_and_recv_return_closed() {
        let (server, client) = bound_and_dialed();
        client.close().unwrap();
        client.close().unwrap();
        assert_eq!(client.send_frame(&[1]), Err(RcpError::Closed));
        let mut buf = [0u8; 8];
        assert_eq!(client.recv_frame(None, &mut buf), Err(RcpError::Closed));
        server.close().unwrap();
        assert_eq!(server.recv_frame(None, &mut buf), Err(RcpError::Closed));
    }

    #[test]
    //fusa:test REQ-UDP-025
    fn udp_avtp_socket_close_unblocks_a_concurrent_recv() {
        let server = Arc::new(UdpAvtpTransport::bind("127.0.0.1", 0, false).unwrap());
        let s2 = Arc::clone(&server);
        let h = std::thread::spawn(move || {
            let mut buf = [0u8; 8];
            s2.recv_frame(None, &mut buf)
        });
        std::thread::sleep(Duration::from_millis(50));
        server.close().unwrap();
        assert_eq!(h.join().unwrap(), Err(RcpError::Closed));
    }

    #[test]
    //fusa:test REQ-UDP-028
    //fusa:test REQ-UDP-030
    fn udp_avtp_socket_skips_datagrams_shorter_than_the_annex_j_seq() {
        let server = UdpAvtpTransport::bind("127.0.0.1", 0, false).unwrap();
        assert_eq!(server.last_recv_seq(), 0);
        let raw = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let dest = ("127.0.0.1", server.local_port());
        raw.send_to(&[1, 2, 3], dest).unwrap(); // not Annex J: skipped
        raw.send_to(&[0, 0, 1, 0, 0x82], dest).unwrap();
        let mut buf = [0u8; 8];
        assert_eq!(server.recv_frame(secs(5), &mut buf).unwrap(), 1);
        assert_eq!(buf[0], 0x82);
        assert_eq!(server.last_recv_seq(), 0x100);
    }

    #[test]
    //fusa:test REQ-UDP-028
    fn udp_avtp_socket_dial_skips_transient_connection_refused_errors() {
        // Dial a port nobody listens on: the send's ICMP port-unreachable
        // surfaces as a transient recv error on some platforms, which must
        // be skipped (udp.c:235) — the call still ends in Timeout.
        let unused = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = unused.local_addr().unwrap().port();
        drop(unused);
        let client = UdpAvtpTransport::dial("127.0.0.1", port, false).unwrap();
        client.send_frame(&[1]).unwrap();
        let mut buf = [0u8; 8];
        let soon = Some(Instant::now() + Duration::from_millis(80));
        assert_eq!(client.recv_frame(soon, &mut buf), Err(RcpError::Timeout));
    }

    #[test]
    //fusa:test REQ-UDP-015
    //fusa:test REQ-UDP-016
    //fusa:test REQ-UDP-017
    //fusa:test REQ-UDP-029
    fn udp_avtp_socket_constructors_reject_bad_addresses_and_default_to_17221() {
        assert!(matches!(
            UdpAvtpTransport::dial("not-an-ip", 1, false),
            Err(RcpError::Other(_))
        ));
        assert!(matches!(
            UdpAvtpTransport::dial("::1", 1, false),
            Err(RcpError::Other(_))
        ));
        assert!(matches!(
            UdpAvtpTransport::bind("bogus", 0, false),
            Err(RcpError::Other(_))
        ));
        // Binding an address this host does not own fails at bind().
        assert!(matches!(
            UdpAvtpTransport::bind("192.0.2.1", 0, false),
            Err(RcpError::Other(_))
        ));
        let d = UdpAvtpTransport::dial_default_port("127.0.0.1", false).unwrap();
        let peer = d.try_clone_socket().unwrap().peer_addr().unwrap();
        assert_eq!(peer.port(), ANNEX_J_CONTROL_PORT);
        // The control port may already be taken on a shared CI host; only
        // assert the port when the bind itself succeeds.
        if let Ok(b) = UdpAvtpTransport::bind_default_port("127.0.0.1", false) {
            assert_eq!(b.local_port(), ANNEX_J_CONTROL_PORT);
        }
    }

    #[test]
    //fusa:test REQ-UDP-003
    fn udp_transport_wrapper_maps_an_oversized_response_to_busy() {
        struct Huge;
        impl UdpSocket for Huge {
            fn send_to(&self, buf: &[u8], _a: SocketAddr) -> Result<usize, RcpError> {
                Ok(buf.len())
            }
            fn recv_from(&self, _t: Option<Duration>) -> Result<(Vec<u8>, SocketAddr), RcpError> {
                Ok((
                    vec![0u8; AVTP_MAX_FRAME_LEN + 1],
                    "127.0.0.1:1".parse().unwrap(),
                ))
            }
        }
        let t = UdpTransport::new(
            local_stream(),
            Arc::new(Huge),
            "127.0.0.1:1".parse().unwrap(),
        );
        assert_eq!(
            t.send_acf_abb(&request(1), 0, None).unwrap_err(),
            RcpError::Busy
        );
        let link = t.link();
        assert!(link.close().is_ok());
        assert!(!link.time_sync_supported());
    }
}
