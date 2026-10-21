//fusa:req REQ-TSN-001
//fusa:req REQ-TSN-002
//fusa:req REQ-TSN-003
//fusa:req REQ-TSN-004
//fusa:req REQ-TSN-005
//fusa:req REQ-TSN-006
//fusa:req REQ-TSN-007
//fusa:req REQ-TSN-008

//! IEEE 802.1p PCP tagging by request kind — a [`TsnTransport`] decorator
//! over any [`AvtpTransport`] (issue #208).
//!
//! Port of c-RCP's `tsn.h`/`tsn.c` (`c-RCP/include/rcp/tsn.h`,
//! `c-RCP/src/tsn.c:56-180`). [`TsnTransport::send_frame`] classifies the
//! outgoing AVTPDU by the request kind its own encoding carries
//! ([`classify_frame`]: an ACF_GBB message with `mtv = 0` repurposes the
//! first `message_timestamp` octet as its `request_type` opcode), maps that
//! kind to a PCP value ([`TsnPcpMap`]), sets `SO_PRIORITY` on the
//! configured socket so the egress qdisc places the frame in that 802.1p
//! traffic class, then delegates to the inner transport unchanged.
//! `recv_frame`/`close` are pure passthroughs — PCP tagging is egress-only.
//!
//! The request kinds and their order are TC18 §12.9.2's execution
//! priorities (cancellation > triggered > timed > compound > compound-wait >
//! chained > standard), the same ordering
//! [`crate::request::ExecutionPriorityTier`] models; the default map gives
//! each kind its c-RCP `rcp_sched_kind_rank()` value as its PCP (Standard
//! 0 .. Cancellation 6, PCP 7 left unused).
//!
//! `SO_PRIORITY` exists only on Linux. As in c-RCP (where the
//! `setsockopt()` call is compiled out elsewhere), the
//! [`SoPriority`] implementation for `std::net::UdpSocket` is a no-op on
//! every other target, and [`TsnTransport::send_frame`] still forwards to
//! the inner transport. Full 802.1Qbv gate scheduling (`vlan_id`/`cycle_ns`
//! in [`TsnConfig`]) needs a TSN-capable NIC; those fields are carried for
//! configuration parity with c-RCP and are not applied by this module,
//! exactly as in c-RCP.

use std::time::Instant;

use crate::acf::ACF_GBB_MSG_TYPE;
use crate::avtp::{self, AvtpTransport, NTSCF_SUBTYPE, TSCF_SUBTYPE};
use crate::request::{self, ExecutionPriorityTier, RequestKind};
use crate::RcpError;

/// The c-RCP `rcp_sched_kind_t` value of `kind` (`c-RCP/include/rcp/
/// scheduler.h:94-105`), which is also its `rcp_sched_kind_rank()`
/// (`c-RCP/src/scheduler.c:30-42`) and its index into [`TsnPcpMap::pcp`].
fn kind_index(kind: ExecutionPriorityTier) -> usize {
    match kind {
        ExecutionPriorityTier::Standard => 0,
        ExecutionPriorityTier::Chained => 1,
        ExecutionPriorityTier::CompoundWait => 2,
        ExecutionPriorityTier::Compound => 3,
        ExecutionPriorityTier::Timed => 4,
        ExecutionPriorityTier::Triggered => 5,
        ExecutionPriorityTier::Cancellation => 6,
    }
}

/// Maps each request kind to an IEEE 802.1p PCP value (0-7) — port of
/// `rcp_tsn_pcp_map_t` (`c-RCP/include/rcp/tsn.h:57-61`). `pcp` is indexed
/// by c-RCP's `rcp_sched_kind_t` order: `[Standard, Chained, CompoundWait,
/// Compound, Timed, Triggered, Cancellation]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
//fusa:req REQ-TSN-001
pub struct TsnPcpMap {
    /// PCP per kind, in c-RCP `rcp_sched_kind_t` order (see the type doc).
    pub pcp: [u8; 7],
}

impl Default for TsnPcpMap {
    //fusa:req REQ-TSN-001
    fn default() -> Self {
        default_pcp_map()
    }
}

/// `pcp[kind] = rank(kind)`: Standard → 0 … Cancellation → 6, PCP 7 unused
/// (`rcp_tsn_default_pcp_map()`, `c-RCP/src/tsn.c:22-32`).
//fusa:req REQ-TSN-001
pub fn default_pcp_map() -> TsnPcpMap {
    TsnPcpMap {
        pcp: [0, 1, 2, 3, 4, 5, 6],
    }
}

/// The PCP value `map` assigns to `kind` (`rcp_tsn_pcp_for()`,
/// `c-RCP/src/tsn.c:34-41`). c-RCP's fail-safe for an out-of-range kind
/// (fall back to the Standard entry) has no rust counterpart: an
/// [`ExecutionPriorityTier`] is always in range.
//fusa:req REQ-TSN-002
pub fn pcp_for(map: &TsnPcpMap, kind: ExecutionPriorityTier) -> u8 {
    map.pcp[kind_index(kind)]
}

/// TSN decorator configuration — port of `rcp_tsn_config_t`
/// (`c-RCP/include/rcp/tsn.h:76-80`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
//fusa:req REQ-TSN-008
pub struct TsnConfig {
    /// Request-kind → PCP map applied on every send.
    pub pcp_map: TsnPcpMap,
    /// 802.1Q VLAN id; `0` = untagged. Carried, not applied (see the
    /// module doc).
    pub vlan_id: i32,
    /// 802.1Qbv gate cycle in nanoseconds; `0` = disabled. Carried, not
    /// applied (see the module doc).
    pub cycle_ns: i32,
}

impl Default for TsnConfig {
    /// `{ pcp_map: default_pcp_map(), vlan_id: 0, cycle_ns: 0 }`
    /// (`rcp_tsn_default_config()`, `c-RCP/src/tsn.c:43-51`).
    //fusa:req REQ-TSN-008
    fn default() -> Self {
        TsnConfig {
            pcp_map: default_pcp_map(),
            vlan_id: 0,
            cycle_ns: 0,
        }
    }
}

/// Classify an already-encoded AVTPDU into its request kind, the way
/// [`TsnTransport::send_frame`] does before tagging
/// (`rcp_tsn_classify_frame()`, `c-RCP/src/tsn.c:55-100`).
///
/// An NTSCF/TSCF frame whose first ACF message is an ACF_GBB with `mtv = 0`
/// is classified by its repurposed `request_type` opcode
/// ([`crate::request::peek_request_type`]); everything else — ACF_ABB, an
/// ACF_GBB with `mtv = 1`, an unrecognized opcode, or any short, bad-subtype
/// or otherwise malformed frame — is [`ExecutionPriorityTier::Standard`],
/// the fail-safe default, never a fabricated higher class. Never panics.
//fusa:req REQ-TSN-003
pub fn classify_frame(frame: &[u8]) -> ExecutionPriorityTier {
    let payload = match avtp::peek_subtype(frame) {
        Ok(NTSCF_SUBTYPE) => avtp::decode_ntscf_frame(frame).map(|(_, p)| p),
        Ok(TSCF_SUBTYPE) => avtp::decode_tscf_frame(frame).map(|(_, p)| p),
        _ => return ExecutionPriorityTier::Standard,
    };
    let Ok(payload) = payload else {
        return ExecutionPriorityTier::Standard;
    };
    // ACF_ABB never carries the repurposing trick.
    if payload.first().map(|b| b >> 1) != Some(ACF_GBB_MSG_TYPE) {
        return ExecutionPriorityTier::Standard;
    }
    match request::peek_request_type(payload) {
        // rcp_sched_classify(true, request_type): an opcode RequestKind
        // does not name is Standard (c-RCP/src/scheduler.c:22-26).
        Ok(rt) => RequestKind::from_u8(rt)
            .map(request::execution_priority_tier)
            .unwrap_or(ExecutionPriorityTier::Standard),
        Err(_) => ExecutionPriorityTier::Standard,
    }
}

/// A socket whose `SO_PRIORITY` [`TsnTransport`] sets before each send —
/// the rust counterpart of the `socket_fd` handed to c-RCP's
/// `rcp_tsn_avtp_transport_new()`.
//fusa:req REQ-TSN-004
pub trait SoPriority: Send + Sync {
    /// `setsockopt(SOL_SOCKET, SO_PRIORITY, pcp)`.
    fn set_so_priority(&self, pcp: u8) -> Result<(), RcpError>;
}

impl SoPriority for std::net::UdpSocket {
    /// Linux: `setsockopt(SOL_SOCKET, SO_PRIORITY, pcp)`. Elsewhere a no-op
    /// returning `Ok(())`, as c-RCP compiles the call out
    /// (`c-RCP/src/tsn.c:16-19`).
    //fusa:req REQ-TSN-004
    fn set_so_priority(&self, pcp: u8) -> Result<(), RcpError> {
        #[cfg(target_os = "linux")]
        {
            nix::sys::socket::setsockopt(self, nix::sys::socket::sockopt::Priority, &i32::from(pcp))
                .map_err(|e| RcpError::Other(format!("tsn: setsockopt(SO_PRIORITY): {e}")))
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = pcp;
            Ok(())
        }
    }
}

/// 802.1p priority-tagging decorator over an inner [`AvtpTransport`] —
/// port of c-RCP's `rcp_tsn_avtp_transport_new()`
/// (`c-RCP/include/rcp/tsn.h:96-108`, `c-RCP/src/tsn.c:102-180`). See this
/// module's doc comment.
///
/// Pass `Arc<X>` as `T` to keep sharing the inner transport (c-RCP's
/// retain); `socket = None` skips the `SO_PRIORITY` call (c-RCP's
/// `socket_fd = -1`, e.g. over a [`crate::avtp::LoopbackTransport`] or a
/// [`crate::shmem::shmem_avtp_pair_new`] side) while still forwarding
/// every send.
//fusa:req REQ-TSN-007
pub struct TsnTransport<T: AvtpTransport> {
    inner: T,
    socket: Option<std::sync::Arc<dyn SoPriority>>,
    cfg: TsnConfig,
}

impl<T: AvtpTransport> TsnTransport<T> {
    /// Wrap `inner`, tagging `socket` (if any) per `cfg` before each send.
    /// The decorator's own [`AvtpTransport::time_sync_supported`] is
    /// `inner`'s.
    //fusa:req REQ-TSN-007
    pub fn new(inner: T, socket: Option<std::sync::Arc<dyn SoPriority>>, cfg: TsnConfig) -> Self {
        TsnTransport { inner, socket, cfg }
    }

    /// The wrapped transport.
    //fusa:req REQ-TSN-007
    pub fn inner(&self) -> &T {
        &self.inner
    }

    /// This decorator's configuration.
    //fusa:req REQ-TSN-008
    pub fn config(&self) -> &TsnConfig {
        &self.cfg
    }
}

impl<T: AvtpTransport> AvtpTransport for TsnTransport<T> {
    /// Set `SO_PRIORITY` to the PCP of `frame`'s classified kind (when a
    /// socket is configured; the `setsockopt` result is ignored, as in
    /// c-RCP `tsn.c:115`), then delegate to the inner transport
    /// (`tsn_send()`, `c-RCP/src/tsn.c:107-120`).
    //fusa:req REQ-TSN-004
    fn send_frame(&self, frame: &[u8]) -> Result<(), RcpError> {
        if let Some(socket) = &self.socket {
            let pcp = pcp_for(&self.cfg.pcp_map, classify_frame(frame));
            let _ = socket.set_so_priority(pcp);
        }
        self.inner.send_frame(frame)
    }

    /// Pure passthrough (`tsn_recv()`, `c-RCP/src/tsn.c:122-128`).
    //fusa:req REQ-TSN-005
    fn recv_frame(&self, deadline: Option<Instant>, buf: &mut [u8]) -> Result<usize, RcpError> {
        self.inner.recv_frame(deadline, buf)
    }

    /// Pure passthrough (`tsn_close()`, `c-RCP/src/tsn.c:130-135`).
    //fusa:req REQ-TSN-006
    fn close(&self) -> Result<(), RcpError> {
        self.inner.close()
    }

    /// Mirrors the inner transport's (`c-RCP/src/tsn.c:174`).
    //fusa:req REQ-TSN-007
    fn time_sync_supported(&self) -> bool {
        self.inner.time_sync_supported()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::acf::{self, AcfAbbMessage, AcfGbbMessage, ByteMessageInfo};
    use crate::avtp::{LoopbackTransport, StreamId, TscfHeader};
    use std::sync::{Arc, Mutex};

    fn sid() -> StreamId {
        StreamId::new([0x02, 0, 0, 0, 0, 7], 1)
    }

    fn gbb_frame(mtv: bool, request_type: u8) -> Vec<u8> {
        let msg = AcfGbbMessage {
            info: ByteMessageInfo {
                mtv,
                byte_bus_id: 3,
                ..Default::default()
            },
            message_timestamp: u64::from(request_type) << 56,
            payload: vec![0; 4],
        };
        avtp::encode_ntscf_frame(sid(), 0, &acf::encode_acf_gbb(&msg).unwrap()).unwrap()
    }

    #[test]
    //fusa:test REQ-TSN-001
    //fusa:test REQ-TSN-002
    fn default_map_is_the_rank_and_pcp_for_indexes_it() {
        let m = default_pcp_map();
        assert_eq!(m, TsnPcpMap::default());
        use ExecutionPriorityTier as K;
        let expect = [
            (K::Standard, 0),
            (K::Chained, 1),
            (K::CompoundWait, 2),
            (K::Compound, 3),
            (K::Timed, 4),
            (K::Triggered, 5),
            (K::Cancellation, 6),
        ];
        for (k, pcp) in expect {
            assert_eq!(pcp_for(&m, k), pcp, "{k:?}");
        }
        let custom = TsnPcpMap {
            pcp: [7, 6, 5, 4, 3, 2, 1],
        };
        assert_eq!(pcp_for(&custom, K::Standard), 7);
        assert_eq!(pcp_for(&custom, K::Cancellation), 1);
    }

    #[test]
    //fusa:test REQ-TSN-008
    fn default_config_matches_c_rcp() {
        let c = TsnConfig::default();
        assert_eq!(c.pcp_map, default_pcp_map());
        assert_eq!(c.vlan_id, 0);
        assert_eq!(c.cycle_ns, 0);
    }

    #[test]
    //fusa:test REQ-TSN-003
    fn classify_frame_reads_the_repurposed_opcode_and_fails_safe() {
        use ExecutionPriorityTier as K;
        for (rt, kind) in [
            (0x05, K::Cancellation),
            (0x06, K::Cancellation),
            (0x07, K::Cancellation),
            (0x0E, K::Triggered),
            (0x8E, K::Triggered),
            (0x0A, K::Timed),
            (0x0F, K::Compound),
            (0x8F, K::Compound),
            (0x0B, K::CompoundWait),
            (0x8B, K::CompoundWait),
            (0x01, K::Chained),
            (0x00, K::Standard),
            (0x42, K::Standard),
        ] {
            assert_eq!(classify_frame(&gbb_frame(false, rt)), kind, "rt={rt:#04x}");
        }
        // mtv = 1: a real timestamp, not repurposed.
        assert_eq!(classify_frame(&gbb_frame(true, 0x05)), K::Standard);
        // ACF_ABB.
        let abb = AcfAbbMessage {
            info: ByteMessageInfo::default(),
            payload: vec![0x05],
        };
        let f = avtp::encode_ntscf_frame(sid(), 0, &acf::encode_acf_abb(&abb).unwrap()).unwrap();
        assert_eq!(classify_frame(&f), K::Standard);
        // Malformed: empty, unknown subtype, truncated NTSCF, empty payload,
        // truncated GBB.
        assert_eq!(classify_frame(&[]), K::Standard);
        assert_eq!(classify_frame(&[0x00, 0, 0, 0]), K::Standard);
        let g = gbb_frame(false, 0x05);
        assert_eq!(classify_frame(&g[..g.len() - 1]), K::Standard);
        let empty = avtp::encode_ntscf_frame(sid(), 0, &[]).unwrap();
        assert_eq!(classify_frame(&empty), K::Standard);
        let short_gbb = avtp::encode_ntscf_frame(sid(), 0, &[ACF_GBB_MSG_TYPE << 1, 0]).unwrap();
        assert_eq!(classify_frame(&short_gbb), K::Standard);
        // A TSCF-headed frame is classified the same way.
        let gbb = &g[avtp::NTSCF_HEADER_LEN..];
        let t = avtp::encode_tscf_frame(&TscfHeader::default(), gbb).unwrap();
        assert_eq!(classify_frame(&t), K::Cancellation);
        assert_eq!(classify_frame(&t[..t.len() - 1]), K::Standard);
    }

    /// Golden vectors generated from c-RCP's real code
    /// (`rcp_tsn_classify_frame()` + `rcp_tsn_pcp_for()` over
    /// `rcp_tsn_default_pcp_map()`, frames built with c-RCP's own
    /// encoders; harness `tsn_golden.c` linked against c-RCP `librcp.a`).
    #[test]
    //fusa:test REQ-TSN-001
    //fusa:test REQ-TSN-003
    fn classify_frame_matches_c_rcp_golden_vectors() {
        for (hex, pcp) in GOLDEN {
            let frame: Vec<u8> = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                .collect();
            assert_eq!(
                pcp_for(&default_pcp_map(), classify_frame(&frame)),
                *pcp,
                "frame {hex}"
            );
        }
    }

    const GOLDEN: &[(&str, u8)] = &[
        ("82800c0302000000000700011c03410200008000a1b2c300", 0),
        ("0580000002000000000700010000000000000000000c00001c03410200008000a1b2c300", 0),
        ("8280100302000000000700011a040102000780000500000000000000", 6),
        ("8280100302000000000700011a040102000780000600000000000000", 6),
        ("8280100302000000000700011a040102000780000700000400000000", 6),
        ("8280140302000000000700011a054102000780000e00010200030001a1b2c300", 5),
        ("8280140302000000000700011a054102000780008e00010200030001a1b2c300", 5),
        ("8280140302000000000700011a054102000780000a00123456789abca1b2c300", 4),
        ("8280140302000000000700011a054102000780000f01020000050001a1b2c300", 3),
        ("8280140302000000000700011a054102000780008f01020000050001a1b2c300", 3),
        ("8280140302000000000700011a054102100780000b01020000050001a1b2c300", 2),
        ("8280140302000000000700011a054102100780008b01020000050001a1b2c300", 2),
        ("8280140302000000000700011a054102010780000100000000090000a1b2c300", 1),
        ("0580000002000000000700010000000000000000001000001a040102000780000500000000000000", 6),
        ("0580000002000000000700010000000000000000001400001a054102000780000f01020000050001a1b2c300", 3),
    ];

    /// Records every PCP it is asked to set.
    #[derive(Default)]
    struct Recorder(Mutex<Vec<u8>>);
    impl SoPriority for Recorder {
        fn set_so_priority(&self, pcp: u8) -> Result<(), RcpError> {
            self.0.lock().unwrap().push(pcp);
            Err(RcpError::Other("ignored by TsnTransport".into()))
        }
    }

    #[test]
    //fusa:test REQ-TSN-004
    //fusa:test REQ-TSN-005
    //fusa:test REQ-TSN-006
    //fusa:test REQ-TSN-007
    fn tsn_transport_tags_then_delegates_and_passes_recv_close_through() {
        let rec = Arc::new(Recorder::default());
        let inner = Arc::new(LoopbackTransport::new(true, 4));
        let t = TsnTransport::new(
            Arc::clone(&inner),
            Some(rec.clone() as Arc<dyn SoPriority>),
            TsnConfig::default(),
        );
        assert!(t.time_sync_supported());
        assert_eq!(t.config(), &TsnConfig::default());
        assert!(Arc::ptr_eq(t.inner(), &inner));

        let cancel = gbb_frame(false, 0x05);
        let timed = gbb_frame(false, 0x0A);
        t.send_frame(&cancel).unwrap(); // setsockopt error is ignored
        t.send_frame(&timed).unwrap();
        t.send_frame(&[0xFF]).unwrap();
        assert_eq!(*rec.0.lock().unwrap(), vec![6, 4, 0]);

        let mut buf = vec![0u8; 256];
        let n = t.recv_frame(None, &mut buf).unwrap();
        assert_eq!(&buf[..n], cancel.as_slice());
        t.close().unwrap();
        // close() reached the inner transport.
        assert_eq!(inner.send_frame(&[1]), Err(RcpError::Closed));
        assert_eq!(t.send_frame(&[1]), Err(RcpError::Closed));
        // recv drains what was queued, then reports the inner Closed.
        assert!(t.recv_frame(None, &mut buf).is_ok());
        assert!(t.recv_frame(None, &mut buf).is_ok());
        assert_eq!(t.recv_frame(None, &mut buf), Err(RcpError::Closed));
    }

    #[test]
    //fusa:test REQ-TSN-004
    //fusa:test REQ-TSN-007
    fn tsn_transport_without_a_socket_still_forwards() {
        let t = TsnTransport::new(LoopbackTransport::new(false, 1), None, TsnConfig::default());
        assert!(!t.time_sync_supported());
        t.send_frame(&gbb_frame(false, 0x05)).unwrap();
        let mut buf = vec![0u8; 256];
        assert!(t.recv_frame(Some(Instant::now()), &mut buf).is_ok());
    }

    // `tsn::tests::udp_socket_` is listed in scripts/miri-skips.txt.
    #[test]
    //fusa:test REQ-TSN-004
    fn udp_socket_so_priority_is_set_on_linux_and_a_no_op_elsewhere() {
        let s = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        s.set_so_priority(5).unwrap();
        #[cfg(target_os = "linux")]
        {
            let got =
                nix::sys::socket::getsockopt(&s, nix::sys::socket::sockopt::Priority).unwrap();
            assert_eq!(got, 5);
        }
    }
}
