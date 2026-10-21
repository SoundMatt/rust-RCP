//! The per-response/acknowledge-stream transmit queue (TC18 RC5 §12.7.9
//! Table 27 "Responder QUEUE_config", §12.9.4/§12.9.5): a port of
//! `c-RCP/src/respqueue.c` / `c-RCP/include/rcp/respqueue.h`.
//!
//! Responses and acknowledges an RC Server's endpoints produce are
//! collected into one [`ResponseQueue`] per configured response stream for
//! aggregated transmission. This is an *outbound* queue of framed ACF
//! messages awaiting transmission — structurally different from
//! [`crate::server::ServerEndpoint`]'s `ep_enable` pre-load queue, which
//! holds *inbound* requests awaiting execution.
//!
//! The queue decides only what its own contents and state can decide
//! without a clock or a transport:
//!
//! - storage and capacity against `queue_size` (REQ-RMAP-059),
//! - the per-message `Max_AVTPDUsize` ceiling and the config-time MTU check
//!   (REQ-RMAP-061),
//! - the fragmentation budget (REQ-RMAP-062),
//! - the `flush_on_count` trigger and the batch packing (REQ-RMAP-063),
//! - the `Flush_time` trigger, as a pure check over a caller-supplied
//!   elapsed time (REQ-RMAP-064/065),
//! - the queue-full eviction of the lowest-`sequence_num` entry and the
//!   latched overflow bit (REQ-RMAP-085).
//!
//! Units: like c-RCP, every function here takes **octets**. Converting a
//! [`ResponseStreamConfigEntry`]'s quadlet registers is the caller's job;
//! [`ResponseQueue::from_config`] does it for the two the queue stores.
//!
//! The per-response-stream heartbeat bookkeeping (`mock.c:483-534`) lives on
//! [`crate::server::RcServer::check_response_queue_heartbeat`], and
//! [`crate::udp::UdpRcServer`] drives these queues through its transport.

use crate::avtp::StreamId;
use crate::regmap::ResponseStreamConfigEntry;

/// The fixed slot-count ceiling on one queue: `RCP_RESPQUEUE_MAX_ENTRIES`
/// (`c-RCP/include/rcp/respqueue.h:218`). Enforced under every
/// configuration, in addition to the `queue_size` octet budget.
///
/// c-RCP parity: TC18 RC5 §12.7.9 defines only the `queue_size` octet
/// budget; this entry-count bound is c-RCP's own implementation ceiling
/// (`respqueue.h:200-217`) and is copied as-is.
//fusa:req REQ-RMAP-085
pub const RESPQUEUE_MAX_ENTRIES: usize = 64;

/// One response/acknowledge stream's transmit queue: `rcp_respqueue_t`
/// (`c-RCP/include/rcp/respqueue.h:220-278`). A FIFO of framed ACF
/// messages, each tagged with the `sequence_num` it was pushed under.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
//fusa:req REQ-RMAP-059
pub struct ResponseQueue {
    entries: Vec<(Vec<u8>, u8)>,
    octets: usize,
    capacity_octets: usize,
    max_avtpdu_size_octets: usize,
    next_sequence_num: u8,
    overflow: bool,
}

impl ResponseQueue {
    /// An empty queue with the given octet capacity (`queue_size` x 4) and
    /// per-message ceiling (`Max_AVTPDUsize` x 4); `0` for either means
    /// unbounded (`rcp_respqueue_init()`, `respqueue.c:8`).
    //fusa:req REQ-RMAP-059
    //fusa:req REQ-RMAP-061
    pub fn new(capacity_octets: usize, max_avtpdu_size_octets: usize) -> Self {
        Self {
            capacity_octets,
            max_avtpdu_size_octets,
            ..Self::default()
        }
    }

    /// [`Self::new`] from a Table 27 row, converting `resp_queue_size` and
    /// `resp_max_avtpdu_size` from quadlets to octets.
    //fusa:req REQ-RMAP-059
    //fusa:req REQ-RMAP-061
    pub fn from_config(cfg: &ResponseStreamConfigEntry) -> Self {
        Self::new(
            usize::from(cfg.resp_queue_size) * 4,
            usize::from(cfg.resp_max_avtpdu_size) * 4,
        )
    }

    /// The configured octet capacity (`0`: unbounded).
    //fusa:req REQ-RMAP-059
    pub fn capacity_octets(&self) -> usize {
        self.capacity_octets
    }

    /// The configured per-message ceiling in octets (`0`: unbounded).
    //fusa:req REQ-RMAP-061
    pub fn max_avtpdu_size_octets(&self) -> usize {
        self.max_avtpdu_size_octets
    }

    /// Drop every queued entry and reset the sequence counter and overflow
    /// bit, keeping the configuration (`rcp_respqueue_destroy()`,
    /// `respqueue.c:17`).
    //fusa:req REQ-RMAP-059
    pub fn clear(&mut self) {
        self.entries.clear();
        self.octets = 0;
        self.next_sequence_num = 0;
        self.overflow = false;
    }

    /// Push `frame` under the queue's own auto-incrementing `sequence_num`
    /// (wrapping mod 256), advancing it only on success
    /// (`rcp_respqueue_push()`, `respqueue.c:35`). See [`Self::push_seq`].
    #[must_use]
    //fusa:req REQ-RMAP-059
    //fusa:req REQ-RMAP-061
    //fusa:req REQ-RMAP-085
    pub fn push(&mut self, frame: &[u8]) -> bool {
        let ok = self.push_seq(frame, self.next_sequence_num);
        if ok {
            self.next_sequence_num = self.next_sequence_num.wrapping_add(1);
        }
        ok
    }

    /// Push `frame` tagged with an explicit `sequence_num`
    /// (`rcp_respqueue_push_seq()`, `respqueue.c:49`).
    ///
    /// Refused (`false`, queue untouched) when `frame` exceeds the
    /// per-message ceiling (REQ-RMAP-061) or the *entire* octet capacity
    /// (REQ-RMAP-059) — no eviction could make it fit. Otherwise the
    /// lowest-`sequence_num` entry is evicted, one at a time, while the
    /// frame does not fit the remaining octet budget or the queue holds
    /// [`RESPQUEUE_MAX_ENTRIES`] entries, latching [`Self::overflow`]
    /// (REQ-RMAP-085). The remaining entries keep FIFO order.
    ///
    /// c-RCP parity: "lowest" is the plain numeric minimum of the 8-bit
    /// `sequence_num` (`respqueue.c:99-103`), with no wrap-around
    /// awareness; TC18 RC5 §12.9.4/§12.9.5 speaks of the "lowest
    /// sequence_num" without defining serial-number arithmetic. Copied
    /// as-is.
    #[must_use]
    //fusa:req REQ-RMAP-059
    //fusa:req REQ-RMAP-061
    //fusa:req REQ-RMAP-085
    pub fn push_seq(&mut self, frame: &[u8], sequence_num: u8) -> bool {
        if self.max_avtpdu_size_octets != 0 && frame.len() > self.max_avtpdu_size_octets {
            return false;
        }
        if self.capacity_octets != 0 && frame.len() > self.capacity_octets {
            return false;
        }
        while (self.capacity_octets != 0 && frame.len() > self.capacity_octets - self.octets)
            || self.entries.len() == RESPQUEUE_MAX_ENTRIES
        {
            let lowest = self
                .entries
                .iter()
                .enumerate()
                .min_by_key(|(_, e)| e.1)
                .map_or(0, |(i, _)| i);
            let (evicted, _) = self.entries.remove(lowest);
            self.octets -= evicted.len();
            self.overflow = true;
        }
        self.entries.push((frame.to_vec(), sequence_num));
        self.octets += frame.len();
        true
    }

    /// The latched overflow bit: an eviction has happened since the queue
    /// was created, cleared or [`Self::clear_overflow`]'d
    /// (`rcp_respqueue_overflow()`).
    //fusa:req REQ-RMAP-085
    pub fn overflow(&self) -> bool {
        self.overflow
    }

    /// Clear the overflow bit (`rcp_respqueue_clear_overflow()`).
    //fusa:req REQ-RMAP-085
    pub fn clear_overflow(&mut self) {
        self.overflow = false;
    }

    /// Remove and return the oldest entry (`rcp_respqueue_pop()`).
    //fusa:req REQ-RMAP-059
    pub fn pop(&mut self) -> Option<Vec<u8>> {
        if self.entries.is_empty() {
            return None;
        }
        let (frame, _) = self.entries.remove(0);
        self.octets -= frame.len();
        Some(frame)
    }

    /// How many entries are queued (`rcp_respqueue_len()`).
    //fusa:req REQ-RMAP-059
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing is queued.
    //fusa:req REQ-RMAP-059
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The queued entries' total length in octets
    /// (`rcp_respqueue_octets()`).
    //fusa:req REQ-RMAP-059
    pub fn octets(&self) -> usize {
        self.octets
    }

    /// The `flush_on_count` trigger (`rcp_respqueue_should_flush()`,
    /// `respqueue.c:189`): never for an empty queue; always when
    /// `flush_on_count_octets` is `0`; otherwise once the queued octets
    /// reach it.
    ///
    /// c-RCP parity: TC18 RC5 §12.7.9 Table 27 encodes `flush_on_count` as
    /// "1: immediate / 2 to Queue_Size" and gives `0` no meaning; c-RCP
    /// treats `0` as "flush immediately" (`respqueue.c:192`). Copied as-is.
    //fusa:req REQ-RMAP-063
    pub fn should_flush(&self, flush_on_count_octets: usize) -> bool {
        if self.entries.is_empty() {
            return false;
        }
        flush_on_count_octets == 0 || self.octets >= flush_on_count_octets
    }

    /// How many leading (FIFO) entries pack into one AVTPDU of at most
    /// `max_avtpdu_size_octets` (`rcp_respqueue_plan_batch()`,
    /// `respqueue.c:198`): `0` for an empty queue, every entry when the
    /// ceiling is `0`, and always at least one entry otherwise.
    ///
    /// c-RCP parity: the ceiling is compared against the entries' summed
    /// ACF lengths alone, not counting the AVTPDU's own NTSCF header that
    /// TC18 RC5 §12.7.9 `Max_AVTPDUsize` ("maximum length of an AVTPDU")
    /// also covers (`respqueue.c:207-209`). Copied as-is.
    //fusa:req REQ-RMAP-063
    pub fn plan_batch(&self, max_avtpdu_size_octets: usize) -> usize {
        if self.entries.is_empty() {
            return 0;
        }
        if max_avtpdu_size_octets == 0 {
            return self.entries.len();
        }
        let mut total = 0usize;
        for (i, (frame, _)) in self.entries.iter().enumerate() {
            let next_total = total + frame.len();
            if i > 0 && next_total > max_avtpdu_size_octets {
                return i;
            }
            total = next_total;
        }
        self.entries.len()
    }
}

/// The config-time check that `Max_AVTPDUsize` keeps every frame within
/// the network MTU (`rcp_respqueue_max_avtpdu_size_within_mtu()`,
/// `respqueue.c:139`). A `0` (unbounded) ceiling passes only a `0`
/// budget.
//fusa:req REQ-RMAP-061
pub fn max_avtpdu_size_within_mtu(max_avtpdu_size_octets: usize, mtu_budget_octets: usize) -> bool {
    if max_avtpdu_size_octets == 0 {
        return mtu_budget_octets == 0;
    }
    max_avtpdu_size_octets <= mtu_budget_octets
}

/// The largest fragment payload that keeps one fragment within
/// `max_avtpdu_size_octets`, reserving `header_len` plus the worst-case 3
/// octets of trailing pad (`rcp_respqueue_max_fragment_payload()`,
/// `respqueue.c:176`); `0` when the ceiling is `0` or leaves no room.
//fusa:req REQ-RMAP-062
pub fn max_fragment_payload(max_avtpdu_size_octets: usize, header_len: usize) -> usize {
    if max_avtpdu_size_octets == 0 {
        return 0;
    }
    let reserved = header_len.saturating_add(3);
    max_avtpdu_size_octets.saturating_sub(reserved)
}

/// The `Flush_time` trigger (`rcp_respqueue_should_flush_by_time()`,
/// `respqueue.c:218`): never when `flush_time_us` is `0` ("flush only by
/// count"), otherwise once `elapsed_since_last_transmit_us` reaches it —
/// identically for an empty queue (the REQ-RMAP-065 heartbeat).
//fusa:req REQ-RMAP-064
//fusa:req REQ-RMAP-065
pub fn should_flush_by_time(elapsed_since_last_transmit_us: u64, flush_time_us: u64) -> bool {
    flush_time_us != 0 && elapsed_since_last_transmit_us >= flush_time_us
}

/// The [`StreamId`] a response queue transmits on, from the interface's
/// `mac` and the row's `STREAM_UID` (`rcp_regmap_response_queue_stream_id()`,
/// `c-RCP/src/regmap.c:2434`).
///
/// c-RCP parity: TC18 RC5 §12.7.9 Table 27 describes `STREAM_UID` as bits
/// `[63:48]` of the queue's stream identifier; c-RCP passes it as
/// `rcp_stream_id_make()`'s `unique_id`, i.e. bits `[15:0]` below the MAC
/// (`regmap.c:2437`). Copied as-is for interop.
//fusa:req REQ-RMAP-060
pub fn response_queue_stream_id(cfg: &ResponseStreamConfigEntry, mac: [u8; 6]) -> StreamId {
    StreamId::new(mac, cfg.resp_stream_uid)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(len: usize) -> Vec<u8> {
        (0..len).map(|i| i as u8).collect()
    }

    /// Golden scenario generated from c-RCP's real `respqueue.c`
    /// (`rcp_respqueue_init(&q, 20, 12)`, then the same pushes; harness
    /// output recorded in the assertions).
    #[test]
    //fusa:test REQ-RMAP-059
    //fusa:test REQ-RMAP-061
    //fusa:test REQ-RMAP-063
    //fusa:test REQ-RMAP-085
    fn golden_scenario_matches_c_rcp() {
        let mut q = ResponseQueue::new(20, 12);
        assert!(!q.push(&frame(13)), "over Max_AVTPDUsize: refused");
        assert!(q.is_empty());
        assert!(q.push(&frame(8)));
        assert!(q.push(&frame(8)));
        assert_eq!((q.len(), q.octets(), q.overflow()), (2, 16, false));
        assert!(q.push(&frame(12)), "evicts to fit the octet budget");
        assert_eq!((q.len(), q.octets(), q.overflow()), (2, 20, true));
        assert!(q.should_flush(0));
        assert!(q.should_flush(20));
        assert!(!q.should_flush(21));
        assert_eq!(q.plan_batch(12), 1);
        assert_eq!(q.plan_batch(20), 2);
        assert_eq!(q.plan_batch(0), 2);
        assert_eq!(q.plan_batch(4), 1, "always at least one entry");
        assert!(q.push_seq(&frame(4), 0));
        assert_eq!((q.len(), q.octets()), (2, 16));
        assert_eq!(q.pop().map(|f| f.len()), Some(12));
        assert_eq!(q.pop().map(|f| f.len()), Some(4));
        assert_eq!(q.pop(), None);
        assert_eq!(q.octets(), 0);
    }

    #[test]
    //fusa:test REQ-RMAP-059
    fn a_frame_larger_than_the_whole_capacity_is_refused_untouched() {
        let mut q = ResponseQueue::new(8, 0);
        assert!(q.push(&frame(4)));
        assert!(!q.push(&frame(9)));
        assert_eq!((q.len(), q.octets(), q.overflow()), (1, 4, false));
        assert_eq!(q.capacity_octets(), 8);
        assert_eq!(q.max_avtpdu_size_octets(), 0);
    }

    #[test]
    //fusa:test REQ-RMAP-085
    fn eviction_removes_the_numerically_lowest_sequence_num_and_keeps_fifo() {
        let mut q = ResponseQueue::new(12, 0);
        assert!(q.push_seq(&[0xA; 4], 9));
        assert!(q.push_seq(&[0xB; 4], 3));
        assert!(q.push_seq(&[0xC; 4], 7));
        assert!(q.push_seq(&[0xD; 4], 200));
        assert!(q.overflow());
        assert_eq!(q.pop(), Some(vec![0xA; 4]));
        assert_eq!(q.pop(), Some(vec![0xC; 4]));
        assert_eq!(q.pop(), Some(vec![0xD; 4]));
        q.clear_overflow();
        assert!(!q.overflow());
    }

    #[test]
    //fusa:test REQ-RMAP-085
    fn the_slot_count_bound_applies_even_when_unbounded_by_octets() {
        let mut q = ResponseQueue::new(0, 0);
        for _ in 0..RESPQUEUE_MAX_ENTRIES {
            assert!(q.push(&[1]));
        }
        assert!(!q.overflow());
        assert!(q.push(&[2]));
        assert_eq!(q.len(), RESPQUEUE_MAX_ENTRIES);
        assert!(q.overflow());
    }

    #[test]
    //fusa:test REQ-RMAP-059
    fn push_sequence_number_wraps_and_clear_resets_it() {
        let mut q = ResponseQueue::new(0, 0);
        for _ in 0..300 {
            assert!(q.push(&[]));
            let _ = q.pop();
        }
        assert_eq!(q.next_sequence_num, (300 % 256) as u8);
        assert!(q.push(&[0; 5]));
        q.clear();
        assert_eq!((q.len(), q.octets(), q.next_sequence_num), (0, 0, 0));
        assert!(!q.should_flush(0), "an empty queue never flushes by count");
        assert_eq!(q.plan_batch(0), 0);
    }

    #[test]
    //fusa:test REQ-RMAP-059
    //fusa:test REQ-RMAP-061
    fn from_config_converts_quadlets_to_octets() {
        let q = ResponseQueue::from_config(&ResponseStreamConfigEntry {
            resp_queue_size: 5,
            resp_max_avtpdu_size: 3,
            ..Default::default()
        });
        assert_eq!((q.capacity_octets(), q.max_avtpdu_size_octets()), (20, 12));
    }

    #[test]
    //fusa:test REQ-RMAP-061
    fn mtu_check_matches_c_rcp() {
        assert!(max_avtpdu_size_within_mtu(0, 0));
        assert!(!max_avtpdu_size_within_mtu(0, 5));
        assert!(max_avtpdu_size_within_mtu(5, 5));
        assert!(!max_avtpdu_size_within_mtu(6, 5));
    }

    #[test]
    //fusa:test REQ-RMAP-062
    fn fragment_payload_budget_matches_c_rcp() {
        assert_eq!(max_fragment_payload(0, 8), 0);
        assert_eq!(max_fragment_payload(11, 8), 0);
        assert_eq!(max_fragment_payload(12, 8), 1);
        assert_eq!(max_fragment_payload(64, 12), 49);
        assert_eq!(max_fragment_payload(64, usize::MAX), 0);
    }

    #[test]
    //fusa:test REQ-RMAP-064
    //fusa:test REQ-RMAP-065
    fn flush_by_time_fires_at_the_threshold_and_never_for_zero() {
        assert!(!should_flush_by_time(u64::MAX, 0));
        assert!(!should_flush_by_time(99, 100));
        assert!(should_flush_by_time(100, 100));
        assert!(should_flush_by_time(101, 100));
    }

    #[test]
    //fusa:test REQ-RMAP-060
    fn queue_stream_id_puts_stream_uid_below_the_mac_like_c_rcp() {
        let cfg = ResponseStreamConfigEntry {
            resp_stream_uid: 0x0B0C,
            ..Default::default()
        };
        let mac = [0x02, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE];
        assert_eq!(
            response_queue_stream_id(&cfg, mac).to_u64(),
            0x02AA_BBCC_DDEE_0B0C
        );
    }
}
