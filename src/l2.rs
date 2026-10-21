//fusa:req REQ-L2-001
//fusa:req REQ-L2-002
//fusa:req REQ-L2-003
//fusa:req REQ-L2-004
//fusa:req REQ-L2-005
//fusa:req REQ-L2-006
//fusa:req REQ-L2-007
//fusa:req REQ-L2-008
//fusa:req REQ-L2-009
//fusa:req REQ-L2-010

//! Layer-2 (raw Ethernet) transport for the TC18 AVTPDU/ACF wire format.
//!
//! TC18 §10.1: "[IEEE1722] can be used as a layer-2 protocol, which is
//! independent from the physical layer below... an AVTPDU is marked by an
//! EtherType value of 0x22F0." This is a second, wire-incompatible
//! transport option alongside [`crate::udp`]'s IEEE 1722-2016 Annex J
//! UDP/IP encapsulation — not an alternative socket API over the same
//! bytes. Frame layout: destination MAC (6 bytes) + source MAC (6 bytes) +
//! EtherType (2 bytes, big-endian, [`ETHERTYPE_AVTP`]) + the AVTPDU bytes
//! directly, with **no** 4-byte encapsulation sequence number —
//! [`crate::udp::encode_annex_j_udp_payload`]'s own field exists only for
//! the Annex J UDP/IP encapsulation and has no L2 counterpart. See
//! [`encode_ethernet_frame`]/[`decode_ethernet_frame`].
//!
//! Before this item, this crate had no layer-2 transport of any kind — the
//! same gap every other RCP-family repo (`go-RCP`, `cpp-RCP`, `c-RCP`) had
//! at the time this item was scoped.
//!
//! Mirrors [`crate::udp`]'s own `UdpSocket`/`UdpTransport` abstraction one
//! wire layer down: [`L2Socket`] is the `UdpSocket` analog (`SocketAddr`
//! replaced by a raw `[u8; 6]` MAC address), and [`L2Transport`] is the
//! `UdpTransport` analog (`send_acf_abb`/`send_acf_gbb`, the same
//! echo-back-verified request/response client shape). On `target_os =
//! "linux"`, [`RawEthernetSocket`] is a real, production [`L2Socket`] over
//! an `AF_PACKET`/`SOCK_RAW` socket; every other target gets a same-named
//! stub whose constructor always errors (see "Non-Linux platforms" below).
//!
//! Server-side dispatch — an `L2RcServer` mirroring
//! [`crate::udp::UdpRcServer`]'s register-map-driven request dispatch — is
//! intentionally out of scope for this item. Flagged here per Guiding
//! Principle 5 as a real, deliberate scope limit rather than an oversight:
//! this item's server-facing wiring is [`crate::udp::UdpRcServer`] run
//! over [`crate::udp::StdUdpSocket`] (see `src/bin/rcp.rs`'s `serve`
//! command) — building a second, independent copy of `UdpRcServer`'s
//! discovery/dispatch logic against `L2Socket` instead of duplicating it
//! is a follow-up, not bundled silently into this one.
//!
//! # Why `nix`, not raw `libc` `unsafe` syscalls — a flagged judgment call
//!
//! Per Guiding Principle 5: this crate is `#![forbid(unsafe_code)]`
//! crate-wide (`src/lib.rs`) — `src/capi.rs`'s own doc comment already
//! flags that this rule is the reason this crate has never built a real
//! raw-pointer FFI boundary. `forbid` cannot be locally overridden by an
//! inner `#[allow(unsafe_code)]` (attempting to do so is itself a compile
//! error, E0453), so a raw `socket()`/`bind()`/`sendto()`/`recvfrom()`
//! implementation directly against `libc` — which would require `unsafe
//! extern "C"` calls written in this crate's own source — is not an
//! option here at all, not merely a style preference this item chose
//! against.
//!
//! [`RawEthernetSocket`] is instead built on the [`nix`](https://docs.rs/nix)
//! crate (a new dependency — not already present in `Cargo.toml`, gated to
//! `target_os = "linux"` only), whose `socket`/`bind`/`sendto`/`recvfrom`/
//! `setsockopt`/`getifaddrs` functions are all safe Rust `fn`s, not
//! `unsafe fn`s — the `unsafe` needed to actually call into `libc` lives
//! inside `nix`'s own crate, never in this one. This was confirmed against
//! `nix` 0.31's own published API (docs.rs) before writing this module,
//! not assumed. `nix` is a narrowly-scoped POSIX-bindings crate, not a
//! heavyweight packet-crafting framework like `pnet`, matching this item's
//! own minimal-footprint intent for a Linux-only transport.
//!
//! # Runtime requirement
//!
//! [`RawEthernetSocket::bind`] opens an `AF_PACKET`/`SOCK_RAW` socket,
//! which the Linux kernel only permits to a process holding `CAP_NET_RAW`
//! (or running as root). This is a real operational requirement of raw
//! packet sockets themselves, not an artifact of this module's design —
//! every caller needs one of those two. The CI job that exercises this
//! module against a real interface (`.github/workflows/ci.yml`, the
//! `l2-veth` job) runs under `sudo` accordingly.
//!
//! # Non-Linux platforms
//!
//! [`RawEthernetSocket`] does not exist as a raw-socket implementation
//! outside `target_os = "linux"` — `AF_PACKET` is a Linux-specific
//! facility. A stub of the same name is compiled in for every other
//! target instead, whose `bind` always returns a clear `Err` explaining
//! why, rather than silently no-op-ing — so the rest of this crate (and
//! any downstream caller) can reference `crate::l2::RawEthernetSocket`
//! unconditionally, without its own `#[cfg(...)]` gate.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::acf::{self, AcfAbbMessage, AcfGbbMessage};
use crate::avtp::{self, StreamId};
use crate::RcpError;

// ── Ethernet II framing (pure functions — no socket) ───────────────────────────

/// IEEE 802 EtherType assigned to IEEE 1722 (AVTP) — TC18 §10.1: "an
/// AVTPDU is marked by an EtherType value of 0x22F0." Sent in place of,
/// never alongside, [`crate::udp::encode_annex_j_udp_payload`]'s 4-byte
/// encapsulation sequence number — see this module's own doc comment.
//fusa:req REQ-L2-001
pub const ETHERTYPE_AVTP: u16 = 0x22F0;

/// Ethernet II header length: 6-byte destination MAC + 6-byte source MAC +
/// 2-byte EtherType.
const ETHERNET_HEADER_LEN: usize = 14;

/// Encode a raw Ethernet II frame carrying `avtpdu`: `dest_mac` +
/// `src_mac` + [`ETHERTYPE_AVTP`] (big-endian) + `avtpdu` directly, with
/// no encapsulation sequence number — see this module's own doc comment.
//fusa:req REQ-L2-001
//fusa:req REQ-L2-002
pub fn encode_ethernet_frame(dest_mac: [u8; 6], src_mac: [u8; 6], avtpdu: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(ETHERNET_HEADER_LEN + avtpdu.len());
    frame.extend_from_slice(&dest_mac);
    frame.extend_from_slice(&src_mac);
    frame.extend_from_slice(&ETHERTYPE_AVTP.to_be_bytes());
    frame.extend_from_slice(avtpdu);
    frame
}

/// `(dest_mac, src_mac, avtpdu_bytes)` — [`decode_ethernet_frame`]'s
/// return shape, named so its signature stays readable.
pub type DecodedEthernetFrame<'a> = ([u8; 6], [u8; 6], &'a [u8]);

/// The inverse of [`encode_ethernet_frame`]: `(dest_mac, src_mac,
/// avtpdu_bytes)`.
///
/// `Err(RcpError::ShortFrame)` for fewer than 14 bytes. `Err(RcpError::
/// Other(_))` if the EtherType field is not [`ETHERTYPE_AVTP`] — such a
/// frame is real Ethernet traffic this transport is simply not addressed
/// to decode (e.g. ARP, IPv4/IPv6), not a malformed AVTPDU.
//fusa:req REQ-L2-001
//fusa:req REQ-L2-002
pub fn decode_ethernet_frame(frame: &[u8]) -> Result<DecodedEthernetFrame<'_>, RcpError> {
    if frame.len() < ETHERNET_HEADER_LEN {
        return Err(RcpError::ShortFrame);
    }
    let mut dest_mac = [0u8; 6];
    let mut src_mac = [0u8; 6];
    dest_mac.copy_from_slice(&frame[0..6]);
    src_mac.copy_from_slice(&frame[6..12]);
    let ethertype = u16::from_be_bytes([frame[12], frame[13]]);
    if ethertype != ETHERTYPE_AVTP {
        return Err(RcpError::Other(format!(
            "l2: unexpected EtherType 0x{ethertype:04x}, expected 0x{ETHERTYPE_AVTP:04x}"
        )));
    }
    Ok((dest_mac, src_mac, &frame[ETHERNET_HEADER_LEN..]))
}

/// `sockaddr_ll.sll_pkttype` value the Linux kernel stamps on the copy of
/// a frame this host itself transmitted that an `AF_PACKET` socket is
/// handed back on its own receive queue (`<linux/if_packet.h>`
/// `PACKET_OUTGOING`). Defined here, not taken from `libc`, so the pure
/// [`raw_frame_accepted`] filter is buildable and testable on every
/// target; a Linux-only test pins it to `nix::libc::PACKET_OUTGOING`.
const PACKET_OUTGOING: u8 = 4;

/// Receive-side admission filter applied by [`RawEthernetSocket`]'s
/// `recv` to every frame the kernel hands it, mirroring c-RCP
/// `src/l2.c:226` (`if (from.sll_pkttype == PACKET_OUTGOING) continue;`)
/// followed by `src/l2.c:228`'s `rcp_l2_frame_decode()` check: a frame is
/// dropped (and the caller re-polls) when the kernel reports it as this
/// socket's own looped-back transmission (`pkttype ==
/// Some(PACKET_OUTGOING)`), or when it is too short / not EtherType
/// [`ETHERTYPE_AVTP`]. `pkttype` is `None` when `recvfrom` returned no
/// link-layer address at all, in which case only the frame check applies.
//fusa:req REQ-L2-009
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn raw_frame_accepted(pkttype: Option<u8>, frame: &[u8]) -> bool {
    if pkttype == Some(PACKET_OUTGOING) {
        return false;
    }
    decode_ethernet_frame(frame).is_ok()
}

/// `true` when `mac`'s individual/group bit (bit 0 of octet 0) is clear —
/// the same test as c-RCP `src/l2.c:79` `rcp_l2_mac_is_unicast()`.
fn mac_is_unicast(mac: [u8; 6]) -> bool {
    mac[0] & 0x01 == 0
}

// ── L2Socket trait ───────────────────────────────────────────────────────────

/// Abstract raw-Ethernet socket for testability — the L2 analog of
/// [`crate::udp::UdpSocket`], addressed by a `[u8; 6]` MAC rather than a
/// `SocketAddr`. `send`/`recv` operate on already-framed
/// ([`encode_ethernet_frame`]/[`decode_ethernet_frame`]) wire bytes, the
/// same "callers see only already-framed bytes" contract
/// [`crate::udp::UdpSocket`] documents.
///
/// `Some(Duration::ZERO)` passed to [`Self::recv`] is not guaranteed to
/// mean "return immediately without blocking" — [`RawEthernetSocket`]'s
/// own implementation uses `SO_RCVTIMEO`, whose own POSIX-defined
/// zero-value means "block indefinitely," not "poll." A caller wanting a
/// true immediate-return poll should not rely on this method for that;
/// [`L2Transport::send_acf_abb`]/[`L2Transport::send_acf_gbb`] avoid the
/// ambiguity entirely by special-casing `Duration::ZERO` themselves before
/// ever reaching this method, the same discipline
/// [`crate::udp::UdpTransport::send_acf_abb`] uses.
//fusa:req REQ-L2-003
pub trait L2Socket: Send + Sync {
    /// Send `frame` (a full Ethernet frame — see [`encode_ethernet_frame`])
    /// out this socket's bound interface.
    fn send(&self, frame: &[u8]) -> Result<usize, RcpError>;

    /// Receive one Ethernet frame, waiting up to `timeout` (`None` blocks
    /// indefinitely) — see this trait's own doc comment for
    /// `Some(Duration::ZERO)`'s caveat.
    fn recv(&self, timeout: Option<Duration>) -> Result<Vec<u8>, RcpError>;
}

// ── L2Transport ──────────────────────────────────────────────────────────────

/// RCP-over-L2 transport, mirroring [`crate::udp::UdpTransport`] one wire
/// layer down: addressed by `local_stream` ([`StreamId`]) plus a
/// caller-supplied `dest_mac` (unicast or multicast — this crate does not
/// derive/allocate a multicast MAC of its own; that algorithm lives in the
/// base IEEE 1722 standard, not available to this crate, so it is always a
/// caller input, never computed here) and `src_mac` (this transport's own
/// address, used to build every outgoing frame's Ethernet header — see
/// [`RawEthernetSocket::bind`] for how a real caller obtains its
/// interface's own MAC without supplying one itself).
//fusa:req REQ-L2-004
pub struct L2Transport {
    local_stream: StreamId,
    socket: Arc<dyn L2Socket>,
    dest_mac: [u8; 6],
    src_mac: [u8; 6],
}

impl L2Transport {
    /// Construct a transport bound to `local_stream`, sending to
    /// `dest_mac` from `src_mac` over `socket`.
    pub fn new(
        local_stream: StreamId,
        socket: Arc<dyn L2Socket>,
        dest_mac: [u8; 6],
        src_mac: [u8; 6],
    ) -> Self {
        L2Transport {
            local_stream,
            socket,
            dest_mac,
            src_mac,
        }
    }

    /// This transport's local [`StreamId`].
    pub fn local_stream(&self) -> StreamId {
        self.local_stream
    }

    /// The destination MAC address every outgoing frame is addressed to.
    pub fn dest_mac(&self) -> [u8; 6] {
        self.dest_mac
    }

    /// The source MAC address every outgoing frame is sent from.
    pub fn src_mac(&self) -> [u8; 6] {
        self.src_mac
    }

    /// Send an ACF_ABB request wrapped in an NTSCF frame addressed under
    /// `local_stream`, framed as a raw Ethernet II frame
    /// ([`encode_ethernet_frame`]), and decode the ACF_ABB response,
    /// verifying it echoes the request's `byte_bus_id`
    /// ([`crate::acf::verify_echo_back`]) — the same request/response
    /// shape as [`crate::udp::UdpTransport::send_acf_abb`], one wire layer
    /// down.
    ///
    /// Returns `Err(RcpError::Timeout)` immediately for a zero `timeout`,
    /// matching [`crate::udp::UdpTransport::send_acf_abb`]'s own
    /// discipline.
    //fusa:req REQ-L2-005
    //fusa:req REQ-L2-006
    pub fn send_acf_abb(
        &self,
        msg: &AcfAbbMessage,
        sequence_num: u8,
        timeout: Option<Duration>,
    ) -> Result<AcfAbbMessage, RcpError> {
        if timeout == Some(Duration::ZERO) {
            return Err(RcpError::Timeout);
        }
        let payload = acf::encode_acf_abb(msg)?;
        let ntscf = avtp::encode_ntscf_frame(self.local_stream, sequence_num, &payload)?;
        let frame = encode_ethernet_frame(self.dest_mac, self.src_mac, &ntscf);
        self.socket.send(&frame)?;
        let resp_ntscf = self.recv_avtpdu(timeout)?;
        let (_, resp_payload) = avtp::decode_ntscf_frame(&resp_ntscf)?;
        let resp = acf::decode_acf_abb(resp_payload)?;
        acf::verify_echo_back(&msg.info, &resp.info)?;
        Ok(resp)
    }

    /// Same as [`Self::send_acf_abb`], for an ACF_GBB request/response
    /// pair.
    //fusa:req REQ-L2-005
    //fusa:req REQ-L2-006
    pub fn send_acf_gbb(
        &self,
        msg: &AcfGbbMessage,
        sequence_num: u8,
        timeout: Option<Duration>,
    ) -> Result<AcfGbbMessage, RcpError> {
        if timeout == Some(Duration::ZERO) {
            return Err(RcpError::Timeout);
        }
        let payload = acf::encode_acf_gbb(msg)?;
        let ntscf = avtp::encode_ntscf_frame(self.local_stream, sequence_num, &payload)?;
        let frame = encode_ethernet_frame(self.dest_mac, self.src_mac, &ntscf);
        self.socket.send(&frame)?;
        let resp_ntscf = self.recv_avtpdu(timeout)?;
        let (_, resp_payload) = avtp::decode_ntscf_frame(&resp_ntscf)?;
        let resp = acf::decode_acf_gbb(resp_payload)?;
        acf::verify_echo_back(&msg.info, &resp.info)?;
        Ok(resp)
    }

    /// Admit `frame` as a response addressed to this transport, returning
    /// its AVTPDU bytes, or `None` to drop it.
    ///
    /// Dropped: frames that are too short or not EtherType
    /// [`ETHERTYPE_AVTP`] (c-RCP `src/l2.c:228-237`: dropped and re-polled,
    /// not surfaced as an error); frames whose source MAC is this
    /// transport's own `src_mac` (our own transmission looped back — the
    /// transport-level counterpart of c-RCP `src/l2.c:226`'s
    /// `PACKET_OUTGOING` drop, which also covers sockets with no `pkttype`
    /// metadata, e.g. a mock or a multicast `dest_mac` the kernel loops
    /// back); and frames whose destination is neither this host's
    /// `src_mac` nor, when `dest_mac` is a group (multicast/broadcast)
    /// address, that configured `dest_mac`.
    ///
    /// c-RCP parity: c-RCP `src/l2.c` `l2_avtp_recv()` has no explicit
    /// destination-MAC check and relies on the NIC/kernel address filter
    /// alone (which already discards `PACKET_OTHERHOST` frames unless the
    /// interface is promiscuous); the explicit check here (rust-RCP#206)
    /// only drops frames c-RCP's kernel filter would also have dropped on
    /// a non-promiscuous interface, so wire interop is unchanged.
    fn accept_response<'a>(&self, frame: &'a [u8]) -> Option<&'a [u8]> {
        let (dest, src, avtpdu) = decode_ethernet_frame(frame).ok()?;
        if src == self.src_mac {
            return None;
        }
        let to_this_host = dest == self.src_mac;
        let to_configured_group = !mac_is_unicast(self.dest_mac) && dest == self.dest_mac;
        if to_this_host || to_configured_group {
            Some(avtpdu)
        } else {
            None
        }
    }

    /// Receive frames from the socket until one passes
    /// [`Self::accept_response`], returning its AVTPDU, or
    /// `Err(RcpError::Timeout)` once `timeout` (measured from this call,
    /// across every dropped frame — the same single overall deadline
    /// c-RCP `src/l2.c` `l2_avtp_recv()` re-checks each poll slice) has
    /// elapsed. Socket errors propagate unchanged.
    //fusa:req REQ-L2-010
    fn recv_avtpdu(&self, timeout: Option<Duration>) -> Result<Vec<u8>, RcpError> {
        let deadline = timeout.map(|d| Instant::now() + d);
        loop {
            let remaining = match deadline {
                Some(dl) => {
                    let r = dl.saturating_duration_since(Instant::now());
                    if r.is_zero() {
                        return Err(RcpError::Timeout);
                    }
                    Some(r)
                }
                None => None,
            };
            let frame = self.socket.recv(remaining)?;
            if let Some(avtpdu) = self.accept_response(&frame) {
                return Ok(avtpdu.to_vec());
            }
        }
    }

    /// No-op, matching [`crate::udp::UdpTransport::close`].
    pub fn close(&self) -> Result<(), RcpError> {
        Ok(())
    }
}

// ── RawEthernetSocket — real production L2Socket ────────────────────────────

#[cfg(target_os = "linux")]
mod raw_socket {
    use std::os::fd::{AsRawFd, OwnedFd};
    use std::time::{Duration, Instant};

    use nix::ifaddrs::getifaddrs;
    use nix::sys::socket::sockopt::ReceiveTimeout;
    use nix::sys::socket::{
        bind, recvfrom, sendto, setsockopt, socket, AddressFamily, LinkAddr, MsgFlags, SockFlag,
        SockProtocol, SockType,
    };
    use nix::sys::time::TimeVal;

    use super::L2Socket;
    use crate::RcpError;

    /// Real, production [`L2Socket`] over a Linux `AF_PACKET`/`SOCK_RAW`
    /// socket bound to one named network interface. See `l2`'s own module
    /// doc comment ("Why `nix`, not raw `libc` `unsafe` syscalls" and
    /// "Runtime requirement") for the design rationale and privilege
    /// requirement.
    //fusa:req REQ-L2-007
    //fusa:req REQ-L2-008
    #[derive(Debug)]
    pub struct RawEthernetSocket {
        fd: OwnedFd,
        bind_addr: LinkAddr,
        mac: [u8; 6],
    }

    impl RawEthernetSocket {
        /// Open a raw `AF_PACKET`/`SOCK_RAW` socket and bind it to
        /// `interface_name` (e.g. `"eth0"`). Requires `CAP_NET_RAW` (or
        /// root) — see this module's "Runtime requirement" doc note;
        /// `Err(RcpError::Other(_))` (not a panic) if that fails, or if
        /// `interface_name` does not exist or has no link-layer address.
        ///
        /// The interface's own MAC address ([`Self::mac`]) is read from
        /// the interface itself via `getifaddrs`, never supplied by the
        /// caller — mirroring how [`crate::udp::StdUdpSocket::bind`] never
        /// asks a caller for its own local IP address.
        //fusa:req REQ-L2-007
        pub fn bind(interface_name: &str) -> Result<Self, RcpError> {
            let addrs =
                getifaddrs().map_err(|e| RcpError::Other(format!("l2: getifaddrs: {e}")))?;
            let link_addr = addrs
                .filter(|ifa| ifa.interface_name == interface_name)
                .find_map(|ifa| ifa.address.and_then(|a| a.as_link_addr().copied()))
                .ok_or_else(|| {
                    RcpError::Other(format!(
                        "l2: interface {interface_name:?} not found, or has no AF_PACKET \
                         link-layer address"
                    ))
                })?;

            let mac = link_addr.addr().ok_or_else(|| {
                RcpError::Other(format!(
                    "l2: interface {interface_name:?} has no MAC address (halen != 6)"
                ))
            })?;

            // SockProtocol::EthAll (ETH_P_ALL, htons(0x0003)) registers
            // this socket for every EtherType, not just ETHERTYPE_AVTP —
            // AF_PACKET sockets otherwise receive nothing at all (the
            // `protocol` argument to the real socket(2) syscall is itself
            // a packet filter, not merely descriptive). This module's own
            // `recv` relies on `raw_frame_accepted`'s EtherType check
            // to reject anything that isn't ours, rather than filtering
            // at the socket layer. c-RCP (`src/l2.c` socket()/bind())
            // opens with protocol htons(0x22F0) instead; nix 0.31's
            // `SockProtocol` has no variant for an arbitrary EtherType, so
            // rust filters the same EtherType in safe code after recvfrom
            // — the set of frames handed to callers is identical.
            let fd = socket(
                AddressFamily::Packet,
                SockType::Raw,
                SockFlag::empty(),
                SockProtocol::EthAll,
            )
            .map_err(|e| {
                RcpError::Other(format!(
                    "l2: socket(AF_PACKET, SOCK_RAW): {e} (needs CAP_NET_RAW or root)"
                ))
            })?;

            bind(fd.as_raw_fd(), &link_addr)
                .map_err(|e| RcpError::Other(format!("l2: bind {interface_name:?}: {e}")))?;

            Ok(RawEthernetSocket {
                fd,
                bind_addr: link_addr,
                mac,
            })
        }

        /// This interface's own MAC address, read from the OS at
        /// [`Self::bind`] time.
        pub fn mac(&self) -> [u8; 6] {
            self.mac
        }

        fn set_recv_timeout(&self, timeout: Option<Duration>) -> Result<(), RcpError> {
            // A zero TimeVal means "block indefinitely" (POSIX SO_RCVTIMEO
            // semantics) — see L2Socket::recv's own doc comment for this
            // caveat on `Some(Duration::ZERO)`.
            // A non-zero remainder below 1 µs would truncate to the
            // zero TimeVal and block forever; clamp it up to 1 µs.
            let tv = match timeout {
                Some(d) => {
                    let d = d.max(Duration::from_micros(1));
                    TimeVal::new(d.as_secs() as i64, d.subsec_micros() as i64)
                }
                None => TimeVal::new(0, 0),
            };
            setsockopt(&self.fd, ReceiveTimeout, &tv)
                .map_err(|e| RcpError::Other(format!("l2: setsockopt(SO_RCVTIMEO): {e}")))
        }
    }

    impl L2Socket for RawEthernetSocket {
        /// Sends `frame` out this socket's bound interface. For an
        /// `AF_PACKET`/`SOCK_RAW` socket, the destination address the
        /// kernel actually transmits to is read from `frame`'s own
        /// Ethernet header (already built by [`super::encode_ethernet_frame`]),
        /// not from `sendto`'s own destination-address argument — only
        /// that argument's interface index matters for a raw send, so
        /// this reuses [`Self::bind_addr`], which already carries the
        /// correct one.
        //fusa:req REQ-L2-007
        fn send(&self, frame: &[u8]) -> Result<usize, RcpError> {
            sendto(
                self.fd.as_raw_fd(),
                frame,
                &self.bind_addr,
                MsgFlags::empty(),
            )
            .map_err(|e| RcpError::Other(format!("l2: sendto: {e}")))
        }

        /// Receives the next frame that passes
        /// [`super::raw_frame_accepted`]: this socket's own looped-back
        /// transmissions (`sll_pkttype == PACKET_OUTGOING`) and anything
        /// not EtherType [`super::ETHERTYPE_AVTP`] are dropped and the
        /// socket re-polled, mirroring c-RCP `src/l2.c:226-238`. `timeout`
        /// is one overall deadline across every dropped frame;
        /// `Some(Duration::ZERO)` keeps [`super::L2Socket`]'s documented
        /// `SO_RCVTIMEO` "block indefinitely" caveat.
        //fusa:req REQ-L2-007
        //fusa:req REQ-L2-009
        fn recv(&self, timeout: Option<Duration>) -> Result<Vec<u8>, RcpError> {
            let deadline = match timeout {
                Some(d) if !d.is_zero() => Some(Instant::now() + d),
                _ => None,
            };
            let mut buf = [0u8; 65535];
            loop {
                let remaining = match deadline {
                    Some(dl) => {
                        let r = dl.saturating_duration_since(Instant::now());
                        if r.is_zero() {
                            return Err(RcpError::Timeout);
                        }
                        Some(r)
                    }
                    None => None,
                };
                self.set_recv_timeout(remaining)?;
                let (n, peer) =
                    recvfrom::<LinkAddr>(self.fd.as_raw_fd(), &mut buf).map_err(|e| {
                        if e == nix::errno::Errno::EAGAIN {
                            RcpError::Timeout
                        } else {
                            RcpError::Other(format!("l2: recvfrom: {e}"))
                        }
                    })?;
                let pkttype = peer.map(|p| p.pkttype());
                if super::raw_frame_accepted(pkttype, &buf[..n]) {
                    return Ok(buf[..n].to_vec());
                }
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod raw_socket {
    use std::time::Duration;

    use super::L2Socket;
    use crate::RcpError;

    /// Non-Linux stub — see `l2`'s own module doc comment, "Non-Linux
    /// platforms". [`Self::bind`] always fails explicitly; this type
    /// exists at all only so `crate::l2::RawEthernetSocket` resolves on
    /// every target.
    //fusa:req REQ-L2-008
    #[derive(Debug)]
    pub struct RawEthernetSocket {
        _unconstructible: (),
    }

    impl RawEthernetSocket {
        /// Always returns `Err(RcpError::Other(_))` — `AF_PACKET` raw
        /// sockets are a Linux-specific facility this crate has no
        /// implementation of on this target.
        //fusa:req REQ-L2-008
        pub fn bind(_interface_name: &str) -> Result<Self, RcpError> {
            Err(RcpError::Other(
                "l2::RawEthernetSocket is only implemented on target_os = \"linux\" \
                 (AF_PACKET/SOCK_RAW raw sockets are a Linux-specific facility); this \
                 platform has no real L2Socket implementation"
                    .to_string(),
            ))
        }

        /// Never actually callable: [`Self::bind`] always errors on this
        /// platform, so no value of this type can exist to call it on.
        pub fn mac(&self) -> [u8; 6] {
            unreachable!("RawEthernetSocket::bind always errors on this platform")
        }
    }

    impl L2Socket for RawEthernetSocket {
        fn send(&self, _frame: &[u8]) -> Result<usize, RcpError> {
            unreachable!("RawEthernetSocket::bind always errors on this platform")
        }

        fn recv(&self, _timeout: Option<Duration>) -> Result<Vec<u8>, RcpError> {
            unreachable!("RawEthernetSocket::bind always errors on this platform")
        }
    }
}

pub use raw_socket::RawEthernetSocket;

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::acf::ByteMessageInfo;
    use std::sync::Mutex;

    fn local_stream() -> StreamId {
        StreamId::new([0x02, 0x11, 0x22, 0x33, 0x44, 0x55], 0x0001)
    }

    // ── Ethernet II framing (pure byte manipulation, no socket) ───────────

    #[test]
    //fusa:test REQ-L2-001
    //fusa:test REQ-L2-002
    fn ethernet_frame_encode_decode_round_trips() {
        let dest = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
        let src = [0x02, 0x11, 0x22, 0x33, 0x44, 0x55];
        let avtpdu = vec![0x01, 0x02, 0x03];
        let frame = encode_ethernet_frame(dest, src, &avtpdu);
        assert_eq!(frame.len(), 14 + avtpdu.len());
        // Byte layout: dest || src || ethertype (BE) || payload.
        assert_eq!(&frame[0..6], &dest);
        assert_eq!(&frame[6..12], &src);
        assert_eq!(&frame[12..14], &[0x22, 0xF0]);
        assert_eq!(&frame[14..], avtpdu.as_slice());

        let (d, s, payload) = decode_ethernet_frame(&frame).unwrap();
        assert_eq!(d, dest);
        assert_eq!(s, src);
        assert_eq!(payload, avtpdu.as_slice());
    }

    #[test]
    //fusa:test REQ-L2-002
    fn ethernet_frame_encode_handles_empty_avtpdu() {
        let frame = encode_ethernet_frame([0; 6], [0; 6], &[]);
        assert_eq!(frame.len(), 14);
        let (_, _, payload) = decode_ethernet_frame(&frame).unwrap();
        assert!(payload.is_empty());
    }

    #[test]
    //fusa:test REQ-L2-002
    fn ethernet_frame_decode_rejects_short_frames() {
        for len in 0..14 {
            let buf = vec![0u8; len];
            let err = decode_ethernet_frame(&buf).unwrap_err();
            assert_eq!(err, RcpError::ShortFrame);
        }
    }

    #[test]
    //fusa:test REQ-L2-002
    fn ethernet_frame_decode_rejects_wrong_ethertype() {
        let mut frame = encode_ethernet_frame([0; 6], [0; 6], &[0xAA]);
        // Corrupt the EtherType field to something real but not AVTP
        // (0x0800 = IPv4).
        frame[12] = 0x08;
        frame[13] = 0x00;
        let err = decode_ethernet_frame(&frame).unwrap_err();
        assert!(matches!(err, RcpError::Other(_)));
    }

    // ── L2Transport (mocked L2Socket — no real socket, no privileges) ─────

    /// A mock socket that echoes back a well-formed ACF_ABB response,
    /// copying `byte_bus_id` from whatever request it received unless
    /// `mismatch` is set — the L2 analog of `udp`'s own `EchoUdp`.
    struct EchoL2 {
        mismatch: bool,
    }

    impl L2Socket for EchoL2 {
        fn send(&self, _frame: &[u8]) -> Result<usize, RcpError> {
            Ok(0)
        }

        fn recv(&self, _timeout: Option<Duration>) -> Result<Vec<u8>, RcpError> {
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
            let ntscf = avtp::encode_ntscf_frame(local_stream(), 1, &payload).unwrap();
            Ok(encode_ethernet_frame(
                [0x02, 0x11, 0x22, 0x33, 0x44, 0x55],
                [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF],
                &ntscf,
            ))
        }
    }

    /// A test double recording every frame handed to `send`, replaying
    /// queued frames from `recv` — the L2 analog of `udp`'s own
    /// `QueuedUdpSocket`.
    struct QueuedL2 {
        inbound: Mutex<Vec<Vec<u8>>>,
        outbound: Mutex<Vec<Vec<u8>>>,
    }

    impl QueuedL2 {
        fn with_inbound(frames: Vec<Vec<u8>>) -> Arc<Self> {
            Arc::new(Self {
                inbound: Mutex::new(frames),
                outbound: Mutex::new(Vec::new()),
            })
        }
    }

    impl L2Socket for QueuedL2 {
        fn send(&self, frame: &[u8]) -> Result<usize, RcpError> {
            self.outbound.lock().unwrap().push(frame.to_vec());
            Ok(frame.len())
        }

        fn recv(&self, _timeout: Option<Duration>) -> Result<Vec<u8>, RcpError> {
            let mut inbound = self.inbound.lock().unwrap();
            if inbound.is_empty() {
                Err(RcpError::Timeout)
            } else {
                Ok(inbound.remove(0))
            }
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

    const DEST_MAC: [u8; 6] = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
    const SRC_MAC: [u8; 6] = [0x02, 0x11, 0x22, 0x33, 0x44, 0x55];

    #[test]
    //fusa:test REQ-L2-004
    //fusa:test REQ-L2-005
    //fusa:test REQ-L2-006
    fn l2_send_acf_abb_round_trips_over_socket() {
        let socket = Arc::new(EchoL2 { mismatch: false });
        let transport = L2Transport::new(local_stream(), socket, DEST_MAC, SRC_MAC);
        let resp = transport.send_acf_abb(&request(7), 0, None).unwrap();
        assert_eq!(resp.info.byte_bus_id, 7);
        assert!(resp.info.rsp);
    }

    #[test]
    //fusa:test REQ-L2-006
    fn l2_send_acf_abb_rejects_echo_back_mismatch() {
        let socket = Arc::new(EchoL2 { mismatch: true });
        let transport = L2Transport::new(local_stream(), socket, DEST_MAC, SRC_MAC);
        let err = transport.send_acf_abb(&request(7), 0, None).unwrap_err();
        assert_eq!(err, RcpError::EpError);
    }

    #[test]
    //fusa:test REQ-L2-005
    fn l2_send_acf_abb_rejects_zero_timeout() {
        let socket = Arc::new(EchoL2 { mismatch: false });
        let transport = L2Transport::new(local_stream(), socket, DEST_MAC, SRC_MAC);
        let err = transport
            .send_acf_abb(&request(7), 0, Some(Duration::ZERO))
            .unwrap_err();
        assert_eq!(err, RcpError::Timeout);
    }

    #[test]
    //fusa:test REQ-L2-004
    fn l2_transport_getters_match_constructor() {
        let socket = Arc::new(EchoL2 { mismatch: false });
        let sid = local_stream();
        let transport = L2Transport::new(sid, socket, DEST_MAC, SRC_MAC);
        assert_eq!(transport.local_stream(), sid);
        assert_eq!(transport.dest_mac(), DEST_MAC);
        assert_eq!(transport.src_mac(), SRC_MAC);
        assert!(transport.close().is_ok());
    }

    #[test]
    //fusa:test REQ-L2-004
    //fusa:test REQ-L2-005
    fn l2_send_acf_gbb_round_trips_over_socket() {
        struct EchoGbb;
        impl L2Socket for EchoGbb {
            fn send(&self, _frame: &[u8]) -> Result<usize, RcpError> {
                Ok(0)
            }
            fn recv(&self, _timeout: Option<Duration>) -> Result<Vec<u8>, RcpError> {
                let resp = AcfGbbMessage {
                    info: ByteMessageInfo {
                        byte_bus_id: 3,
                        rsp: true,
                        ..Default::default()
                    },
                    message_timestamp: 0,
                    payload: vec![0x55],
                };
                let payload = acf::encode_acf_gbb(&resp).unwrap();
                let ntscf = avtp::encode_ntscf_frame(local_stream(), 1, &payload).unwrap();
                // Response from the peer (DEST_MAC) addressed back to
                // this host (SRC_MAC).
                Ok(encode_ethernet_frame(SRC_MAC, DEST_MAC, &ntscf))
            }
        }
        let socket = Arc::new(EchoGbb);
        let transport = L2Transport::new(local_stream(), socket, DEST_MAC, SRC_MAC);
        let msg = AcfGbbMessage {
            info: ByteMessageInfo {
                byte_bus_id: 3,
                op: true,
                ..Default::default()
            },
            message_timestamp: 0,
            payload: vec![0x01],
        };
        let resp = transport.send_acf_gbb(&msg, 0, None).unwrap();
        assert_eq!(resp.info.byte_bus_id, 3);
        assert_eq!(resp.payload, vec![0x55]);
    }

    #[test]
    //fusa:test REQ-L2-003
    //fusa:test REQ-L2-004
    fn l2_transport_send_records_the_real_ethernet_frame() {
        let socket = QueuedL2::with_inbound(Vec::new());
        let transport = L2Transport::new(local_stream(), socket.clone(), DEST_MAC, SRC_MAC);
        // Uses recv's Timeout error (empty queue) just to exercise send();
        // don't care about the response here.
        let _ = transport.send_acf_abb(&request(7), 0, None);

        let sent = socket.outbound.lock().unwrap();
        assert_eq!(sent.len(), 1);
        let (dest, src, _payload) = decode_ethernet_frame(&sent[0]).unwrap();
        assert_eq!(dest, DEST_MAC);
        assert_eq!(src, SRC_MAC);
    }

    // ── Receive filtering (rust-RCP#206, c-RCP src/l2.c:226-238) ──────────

    /// A well-formed ACF_ABB response frame echoing `byte_bus_id`,
    /// addressed `dest` <- `src`.
    fn abb_response_frame(dest: [u8; 6], src: [u8; 6], byte_bus_id: u16) -> Vec<u8> {
        let resp = AcfAbbMessage {
            info: ByteMessageInfo {
                byte_bus_id,
                rsp: true,
                ..Default::default()
            },
            payload: vec![0xC3],
        };
        let payload = acf::encode_acf_abb(&resp).unwrap();
        let ntscf = avtp::encode_ntscf_frame(local_stream(), 1, &payload).unwrap();
        encode_ethernet_frame(dest, src, &ntscf)
    }

    /// Mock socket modelling the Linux kernel's AF_PACKET loopback: every
    /// frame handed to `send` is queued straight back onto this same
    /// socket's receive queue (ahead of any genuine reply), exactly as an
    /// unfiltered raw socket would see its own transmission.
    struct LoopbackL2 {
        queue: Mutex<Vec<Vec<u8>>>,
        replies: Mutex<Vec<Vec<u8>>>,
        recv_calls: Mutex<usize>,
    }

    impl LoopbackL2 {
        fn new(replies: Vec<Vec<u8>>) -> Arc<Self> {
            Arc::new(Self {
                queue: Mutex::new(Vec::new()),
                replies: Mutex::new(replies),
                recv_calls: Mutex::new(0),
            })
        }
    }

    impl L2Socket for LoopbackL2 {
        fn send(&self, frame: &[u8]) -> Result<usize, RcpError> {
            let mut q = self.queue.lock().unwrap();
            q.push(frame.to_vec());
            q.append(&mut self.replies.lock().unwrap());
            Ok(frame.len())
        }

        fn recv(&self, _timeout: Option<Duration>) -> Result<Vec<u8>, RcpError> {
            *self.recv_calls.lock().unwrap() += 1;
            let mut q = self.queue.lock().unwrap();
            if q.is_empty() {
                Err(RcpError::Timeout)
            } else {
                Ok(q.remove(0))
            }
        }
    }

    #[test]
    //fusa:test REQ-L2-010
    fn l2_transport_skips_its_own_looped_back_request() {
        // Mutation-sensitive: without the filter the transport decodes its
        // own request (op=1, rsp=0) as the "response".
        let reply = abb_response_frame(SRC_MAC, DEST_MAC, 7);
        let socket = LoopbackL2::new(vec![reply]);
        let transport = L2Transport::new(local_stream(), socket.clone(), DEST_MAC, SRC_MAC);
        let resp = transport
            .send_acf_abb(&request(7), 0, Some(Duration::from_secs(1)))
            .unwrap();
        assert!(resp.info.rsp, "must be the peer's response, not our echo");
        assert_eq!(resp.payload, vec![0xC3]);
        assert_eq!(*socket.recv_calls.lock().unwrap(), 2);
    }

    #[test]
    //fusa:test REQ-L2-010
    fn l2_transport_times_out_when_only_its_own_frame_comes_back() {
        let socket = LoopbackL2::new(Vec::new());
        let transport = L2Transport::new(local_stream(), socket, DEST_MAC, SRC_MAC);
        let err = transport
            .send_acf_abb(&request(7), 0, Some(Duration::from_secs(1)))
            .unwrap_err();
        assert_eq!(err, RcpError::Timeout);
    }

    #[test]
    //fusa:test REQ-L2-010
    fn l2_transport_skips_wrong_ethertype_short_and_misaddressed_frames() {
        let mut wrong_ethertype = abb_response_frame(SRC_MAC, DEST_MAC, 7);
        wrong_ethertype[12] = 0x08;
        wrong_ethertype[13] = 0x00;
        let short = vec![0u8; 13];
        let other_host = abb_response_frame([0x02, 0, 0, 0, 0, 0x99], DEST_MAC, 7);
        // Group-addressed, but not our configured dest_mac (which is
        // unicast here): dropped.
        let foreign_group = abb_response_frame([0x01, 0x00, 0x5E, 0, 0, 1], DEST_MAC, 7);
        let good = abb_response_frame(SRC_MAC, DEST_MAC, 7);
        let socket = QueuedL2::with_inbound(vec![
            wrong_ethertype,
            short,
            other_host,
            foreign_group,
            good,
        ]);
        let transport = L2Transport::new(local_stream(), socket.clone(), DEST_MAC, SRC_MAC);
        let resp = transport.send_acf_abb(&request(7), 0, None).unwrap();
        assert_eq!(resp.payload, vec![0xC3]);
        assert!(socket.inbound.lock().unwrap().is_empty());
    }

    #[test]
    //fusa:test REQ-L2-010
    fn l2_transport_accepts_responses_to_its_configured_multicast_group() {
        let group = [0x91, 0xE0, 0xF0, 0x00, 0x0E, 0x80];
        let peer = [0x02, 0xAA, 0, 0, 0, 0x01];
        let socket = QueuedL2::with_inbound(vec![
            // Our own multicast request looped back: same dest, own src.
            abb_response_frame(group, SRC_MAC, 7),
            abb_response_frame(group, peer, 7),
        ]);
        let transport = L2Transport::new(local_stream(), socket.clone(), group, SRC_MAC);
        let resp = transport.send_acf_abb(&request(7), 0, None).unwrap();
        assert!(resp.info.rsp);
        assert!(socket.inbound.lock().unwrap().is_empty());
    }

    #[test]
    //fusa:test REQ-L2-010
    fn l2_transport_deadline_spans_dropped_frames() {
        // A socket that only ever returns a misaddressed frame, slowly:
        // the transport's single overall deadline must still fire.
        struct Chatty;
        impl L2Socket for Chatty {
            fn send(&self, frame: &[u8]) -> Result<usize, RcpError> {
                Ok(frame.len())
            }
            fn recv(&self, timeout: Option<Duration>) -> Result<Vec<u8>, RcpError> {
                assert!(timeout.is_some_and(|t| !t.is_zero()));
                std::thread::sleep(Duration::from_millis(5));
                Ok(abb_response_frame(DEST_MAC, DEST_MAC, 7))
            }
        }
        let transport = L2Transport::new(local_stream(), Arc::new(Chatty), DEST_MAC, SRC_MAC);
        let err = transport
            .send_acf_abb(&request(7), 0, Some(Duration::from_millis(30)))
            .unwrap_err();
        assert_eq!(err, RcpError::Timeout);
    }

    #[test]
    //fusa:test REQ-L2-009
    fn raw_frame_filter_drops_packet_outgoing_and_non_avtp() {
        let good = abb_response_frame(SRC_MAC, DEST_MAC, 7);
        assert!(raw_frame_accepted(None, &good));
        assert!(raw_frame_accepted(Some(0), &good)); // PACKET_HOST
        assert!(raw_frame_accepted(Some(2), &good)); // PACKET_MULTICAST
        assert!(!raw_frame_accepted(Some(PACKET_OUTGOING), &good));
        let mut ipv4 = good.clone();
        ipv4[12] = 0x08;
        ipv4[13] = 0x00;
        assert!(!raw_frame_accepted(Some(0), &ipv4));
        assert!(!raw_frame_accepted(None, &good[..13]));
    }

    #[cfg(target_os = "linux")]
    #[test]
    //fusa:test REQ-L2-009
    fn packet_outgoing_matches_linux_uapi() {
        assert_eq!(PACKET_OUTGOING, nix::libc::PACKET_OUTGOING);
    }

    #[test]
    //fusa:test REQ-L2-010
    fn mac_is_unicast_tests_the_group_bit() {
        assert!(mac_is_unicast(SRC_MAC));
        assert!(mac_is_unicast(DEST_MAC));
        assert!(!mac_is_unicast([0xFF; 6]));
        assert!(!mac_is_unicast([0x01, 0x00, 0x5E, 0, 0, 1]));
    }

    // ── Non-Linux RawEthernetSocket stub ───────────────────────────────────

    #[cfg(not(target_os = "linux"))]
    #[test]
    //fusa:test REQ-L2-008
    fn raw_ethernet_socket_bind_fails_explicitly_off_linux() {
        let err = RawEthernetSocket::bind("eth0").unwrap_err();
        assert!(matches!(err, RcpError::Other(_)));
    }

    // ── Real raw socket over a veth pair (Linux only, requires root/
    //    CAP_NET_RAW; #[ignore]d by default — see .github/workflows/
    //    ci.yml's `l2-veth` job, which sets up veth0/veth1 and runs this
    //    with `-- --ignored`) ──────────────────────────────────────────────

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires root/CAP_NET_RAW and a pre-existing veth0/veth1 pair; see ci.yml's l2-veth job"]
    //fusa:test REQ-L2-007
    fn real_raw_ethernet_socket_round_trips_a_frame_over_a_veth_pair() {
        let tx = RawEthernetSocket::bind("veth0").expect("bind veth0 (needs sudo/CAP_NET_RAW)");
        let rx = RawEthernetSocket::bind("veth1").expect("bind veth1 (needs sudo/CAP_NET_RAW)");

        let src_mac = tx.mac();
        let dest_mac = rx.mac();
        let avtpdu = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01, 0x02, 0x03];
        let frame = encode_ethernet_frame(dest_mac, src_mac, &avtpdu);

        tx.send(&frame).expect("send over veth0");

        // veth1 may also see other link-local traffic (e.g. NDP/ARP) on a
        // freshly created interface; loop past anything that doesn't
        // decode as our own EtherType/AVTPDU rather than assuming the very
        // first received frame is ours.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            assert!(
                std::time::Instant::now() < deadline,
                "did not observe our frame on veth1 within 5s"
            );
            let received = rx
                .recv(Some(Duration::from_secs(5)))
                .expect("recv on veth1");
            if let Ok((d, s, payload)) = decode_ethernet_frame(&received) {
                if d == dest_mac && s == src_mac {
                    assert_eq!(
                        payload,
                        avtpdu.as_slice(),
                        "frame must round-trip byte-for-byte"
                    );
                    break;
                }
            }
        }

        // rust-RCP#206 / c-RCP src/l2.c:226: the sender's own socket must
        // not hand back its own transmission (PACKET_OUTGOING), and must
        // drop the non-AVTP link-local chatter (NDP/MLD) a fresh veth pair
        // emits — so with nothing else sent, tx.recv times out.
        let err = tx
            .recv(Some(Duration::from_millis(300)))
            .expect_err("tx must not receive its own looped-back frame");
        assert_eq!(err, RcpError::Timeout);
    }
}
