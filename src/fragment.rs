// fusa:req REQ-FRAG-001
// fusa:req REQ-FRAG-002
// fusa:req REQ-FRAG-003
// fusa:req REQ-FRAG-004
// fusa:req REQ-FRAG-005
// fusa:req REQ-FRAG-006
// fusa:req REQ-FRAG-007
// fusa:req REQ-FRAG-008

//! Multi-AVTPDU fragmentation reassembly (`ROADMAP.md` Milestone 8,
//! "Fragmentation Go/No-Go").
//!
//! ## The decision
//!
//! **Go.** This crate's own `RequestStreamConfigEntry::rx_stream_max_request_size`
//! (`src/regmap.rs`, §3.8) already exists as a real per-stream byte bound
//! whose own doc comment states its purpose is reassembling "the largest
//! single fragmented request this stream will reassemble" — that field has
//! no purpose at all under a permanent single-AVTPDU-only limitation. More
//! concretely, [`crate::can::CanXlFrame`]'s up-to-2048-byte payload
//! (`ROADMAP.md` Milestone 7's CAN controller bullet) routinely exceeds any
//! practical single-AVTPDU MTU, and neither UART RX-FIFO sizing nor a
//! full-register-map discovery read (`ROADMAP.md` Milestone 2/3) is bounded
//! to fit inside one AVTPDU either. Shipping v1.0 with a silent
//! single-AVTPDU-only ceiling would quietly cap CAN XL, UART, and
//! whole-register-map discovery reads below their own already-modeled
//! sizes — not an accepted limitation, an unannounced regression. This
//! module is that "go" choice's implementation, closing Milestone 8's first
//! and second checklist bullets together.
//!
//! [`FragmentReassemblyBuffer`] is the reassembly buffer itself: it accepts
//! fragments in wire-arrival order, each carrying a decoded
//! [`crate::acf::ByteMessageInfo`], validates the dual-purpose
//! `read_size`/`segment_num` byte ([`crate::acf::ReadSizeOrSegmentNum`]) as
//! a `segment_num` consistency check, enforces the caller-supplied
//! `rx_stream_max_request_size` bound, and — composing (never
//! re-deriving) [`crate::e2e::CombinedFragmentPayload`] — assembles the
//! combined payload once the train's final fragment (`ms == false`)
//! arrives. [`verify_reassembled_train_crc`] re-verifies `ROADMAP.md`
//! Milestone 6's "only the last fragment carries the CRC" rule
//! ([`crate::e2e::check_fragment_crc_placement`]) and the safe-point CRC-32
//! itself ([`crate::e2e::crc32_tc18_for_fragment_train`]) against a real
//! buffer's own wire-collected segments, rather than the caller-supplied
//! `&[&[u8]]` those two functions' own doc comments flagged as a Milestone
//! 8 forward dependency when they first landed.
//!
//! Matching every prior Milestone 1-7 entry's own discipline, this module
//! is additive standalone plumbing only: nothing here is wired into
//! [`crate::avtp`]/[`crate::acf`]'s decoders, [`crate::request`]'s
//! lifecycle machinery, or any dispatch loop. [`FragmentReassemblyBuffer`]
//! is a pure state machine a caller drives explicitly, the same way
//! [`crate::request::SequencerBank`] is a live store a caller drives
//! explicitly rather than something this crate's own dispatch loop touches
//! yet.
//!
//! Deliberately out of scope:
//!
//! - Wiring this buffer into a live per-stream request-dispatch path, or
//!   into [`crate::regmap::RequestStreamConfigEntry`] itself (this module
//!   only *consumes* `rx_stream_max_request_size` as a caller-supplied
//!   `u16`, it does not read a live `RequestStreamConfigEntry`).
//! - The response-side counterpart,
//!   [`crate::regmap::ResponseStreamConfigEntry::resp_max_avtpdu_size`] —
//!   that field bounds a single *outgoing* AVTPDU's size (fragmenting a
//!   response that would exceed it), a distinct, unbuilt problem from this
//!   module's inbound-request reassembly, left for whichever later item
//!   builds response fragmentation.
//! - Resolving [`crate::acf::ReadSizeOrSegmentNum`]'s general
//!   direction/type-based ambiguity (see `acf.rs`'s own provenance note).
//!   This module only narrows that ambiguity for its own, narrower
//!   question — see "Provenance note: `segment_num` ordering" below.
//!
//! ## Provenance note: `segment_num` ordering
//!
//! `ROADMAP.md`'s Milestone 8 checklist bullet names `ms`/`segment_num`
//! reconstruction but does not state whether segment order is recovered
//! from `segment_num` itself, from wire-arrival order, or some combination
//! — nor whether `segment_num` starts at `0` or `1`. Per Guiding Principle
//! 5, [`FragmentReassemblyBuffer`] does not treat `segment_num` as the sole
//! source of ordering truth: it consumes fragments in wire-arrival order
//! (the same order [`crate::e2e::CombinedFragmentPayload::assemble`] already
//! concatenates caller-supplied segments in), and separately validates that
//! each arriving fragment's [`crate::acf::ReadSizeOrSegmentNum::as_segment_num`]
//! equals a strictly-incrementing counter starting at `0` — a consistency
//! check against gaps, duplicates, and reordering, not a re-sort. This is
//! this crate's own working interpretation of the one specific question
//! "which bit-pattern does a fragment train's `segment_num` use inside a
//! `ByteMessageInfo` whose `ms` flag marks it as part of a train" — it does
//! not resolve `acf.rs`'s own broader, still-open `read_size`-vs-
//! `segment_num` direction/type ambiguity, which stays flagged there.
//!
//! ## Provenance note: `rx_stream_max_request_size` as the combined-payload
//! bound
//!
//! [`crate::regmap::RequestStreamConfigEntry::rx_stream_max_request_size`]'s
//! own doc comment states it as "largest single fragmented request this
//! stream will reassemble, in bytes", so [`FragmentReassemblyBuffer`]
//! applies it to the running total of every segment's payload length
//! combined — not to any one fragment's individual payload length — and
//! rejects with [`crate::RcpError::PayloadTooLarge`] the moment that running
//! total would exceed it, before storing the offending fragment. A value of
//! `0` is that same field's own documented "fragmentation unsupported on
//! this stream" sentinel; [`FragmentReassemblyBuffer::new`] accepts it
//! without error, but [`FragmentReassemblyBuffer::accept_fragment`] rejects
//! every fragment on such a buffer with [`crate::RcpError::UnsupportedCmd`]
//! rather than silently reassembling anyway.

use crate::acf::ByteMessageInfo;
use crate::avtp::HeaderVariant;
use crate::e2e::{self, AcfCoverageMessage, CombinedFragmentPayload};
use crate::RcpError;

/// What [`FragmentReassemblyBuffer::accept_fragment`] learned from the
/// fragment it was just given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FragmentAcceptOutcome {
    /// `ms == true`: more segments are expected before the train is
    /// complete. The buffer has stored this fragment's payload and is
    /// ready for the next one.
    Continuing,
    /// `ms == false`: this was the train's final fragment. The combined
    /// payload is available via [`FragmentReassemblyBuffer::combined_payload`]
    /// (or [`FragmentReassemblyBuffer::segment_refs`] for CRC
    /// re-verification via [`verify_reassembled_train_crc`]). The caller
    /// must call [`FragmentReassemblyBuffer::reset`] before this buffer can
    /// accept a fragment belonging to a different train.
    Complete,
}

/// A live, per-stream multi-AVTPDU reassembly buffer.
///
/// Bounded by a caller-supplied `rx_stream_max_request_size` (see
/// [`crate::regmap::RequestStreamConfigEntry::rx_stream_max_request_size`]);
/// see this module's doc comment for the full provenance notes behind this
/// type's ordering and bounding rules.
#[derive(Debug, Clone, PartialEq, Eq)]
// fusa:req REQ-FRAG-001
pub struct FragmentReassemblyBuffer {
    max_request_size: u16,
    segments: Vec<Vec<u8>>,
    total_len: usize,
    next_expected_segment_num: u8,
}

impl FragmentReassemblyBuffer {
    /// Creates an empty reassembly buffer bounded by `rx_stream_max_request_size`
    /// bytes of *combined* payload. `0` is accepted here (it is the
    /// stream-config field's own "fragmentation unsupported" sentinel); see
    /// [`Self::fragmentation_supported`] and [`Self::accept_fragment`].
    // fusa:req REQ-FRAG-001
    pub fn new(rx_stream_max_request_size: u16) -> Self {
        FragmentReassemblyBuffer {
            max_request_size: rx_stream_max_request_size,
            segments: Vec::new(),
            total_len: 0,
            next_expected_segment_num: 0,
        }
    }

    /// Whether this stream's configured bound allows fragmentation at all.
    /// `false` exactly when this buffer was constructed with
    /// `rx_stream_max_request_size == 0`, per that field's own documented
    /// sentinel meaning.
    // fusa:req REQ-FRAG-001
    pub fn fragmentation_supported(&self) -> bool {
        self.max_request_size != 0
    }

    /// Whether a train is currently mid-reassembly (at least one fragment
    /// accepted since the last [`Self::reset`] or the buffer's own
    /// construction, and not yet completed).
    pub fn is_in_progress(&self) -> bool {
        !self.segments.is_empty()
    }

    /// Feeds one wire-arrival-ordered fragment's already-decoded header and
    /// payload into the buffer.
    ///
    /// Returns `Err(RcpError::UnsupportedCmd)` if this buffer was
    /// constructed with `rx_stream_max_request_size == 0` (see this
    /// module's doc comment).
    ///
    /// Returns `Err(RcpError::InvalidParameter)` if `info`'s
    /// `read_size_segment_num` (read as [`crate::acf::ReadSizeOrSegmentNum::as_segment_num`])
    /// does not equal the next expected value in a strictly-incrementing,
    /// zero-based sequence — see this module's "Provenance note:
    /// `segment_num` ordering". The buffer's state is left unchanged when
    /// this happens.
    ///
    /// Returns `Err(RcpError::PayloadTooLarge)` if storing `payload` would
    /// push the train's combined length past `rx_stream_max_request_size`.
    /// The buffer's state is left unchanged when this happens.
    ///
    /// On success, returns [`FragmentAcceptOutcome::Continuing`] when
    /// `info.ms` is `true`, or [`FragmentAcceptOutcome::Complete`] when it
    /// is `false` — the final-fragment signal this crate's Milestone 1 "ACF
    /// Messages" item already decodes.
    // fusa:req REQ-FRAG-002
    // fusa:req REQ-FRAG-003
    // fusa:req REQ-FRAG-004
    // fusa:req REQ-FRAG-005
    pub fn accept_fragment(
        &mut self,
        info: &ByteMessageInfo,
        payload: &[u8],
    ) -> Result<FragmentAcceptOutcome, RcpError> {
        if !self.fragmentation_supported() {
            return Err(RcpError::UnsupportedCmd);
        }

        let segment_num = info.read_size_segment_num.as_segment_num();
        if segment_num != self.next_expected_segment_num {
            return Err(RcpError::InvalidParameter);
        }

        let prospective_total = self.total_len + payload.len();
        if prospective_total > self.max_request_size as usize {
            return Err(RcpError::PayloadTooLarge);
        }

        self.segments.push(payload.to_vec());
        self.total_len = prospective_total;
        self.next_expected_segment_num = self.next_expected_segment_num.wrapping_add(1);

        if info.ms {
            Ok(FragmentAcceptOutcome::Continuing)
        } else {
            Ok(FragmentAcceptOutcome::Complete)
        }
    }

    /// Borrowed views of every segment accepted so far, in arrival order —
    /// the same shape [`crate::e2e::build_crc32_coverage_buffer_for_fragment_train`]/
    /// [`crate::e2e::crc32_tc18_for_fragment_train`] take as their own
    /// `segments` parameter, and what [`verify_reassembled_train_crc`]
    /// passes them.
    pub fn segment_refs(&self) -> Vec<&[u8]> {
        self.segments.iter().map(Vec::as_slice).collect()
    }

    /// The combined payload of every segment accepted so far, concatenated
    /// in arrival order by composing (not re-deriving)
    /// [`crate::e2e::CombinedFragmentPayload::assemble`].
    // fusa:req REQ-FRAG-005
    pub fn combined_payload(&self) -> CombinedFragmentPayload {
        CombinedFragmentPayload::assemble(&self.segment_refs())
    }

    /// Clears all accumulated state, readying this buffer for a new train.
    /// Does not change the configured `rx_stream_max_request_size` bound.
    // fusa:req REQ-FRAG-006
    pub fn reset(&mut self) {
        self.segments.clear();
        self.total_len = 0;
        self.next_expected_segment_num = 0;
    }
}

/// Re-verifies `ROADMAP.md` Milestone 6's "only the last fragment carries
/// the CRC" rule and its safe-point CRC-32 against a real
/// [`FragmentReassemblyBuffer`]'s own wire-collected segments, rather than
/// the caller-supplied `&[&[u8]]` [`crate::e2e::crc32_tc18_for_fragment_train`]
/// itself still takes (that function's own doc comment flags this exact
/// composition as a Milestone 8 forward dependency).
///
/// Composes [`crate::e2e::check_fragment_crc_placement`] (called with
/// `ms = false`, since only a buffer that has reached
/// [`FragmentAcceptOutcome::Complete`] is a meaningful CRC-verification
/// target) and [`crate::e2e::crc32_tc18_for_fragment_train`] in sequence,
/// re-deriving neither. Returns the same `Err` either of those would
/// return: `Err(RcpError::InvalidParameter)` if `crc_present` disagrees
/// with the "final fragment carries a CRC" rule, or
/// `Err(RcpError::InvalidSize)` if `final_fragment`'s own header fields
/// fail [`crate::acf::encode_byte_message_info`]'s field-width validation.
// fusa:req REQ-FRAG-007
pub fn verify_reassembled_train_crc(
    buffer: &FragmentReassemblyBuffer,
    header: &HeaderVariant,
    final_fragment: &AcfCoverageMessage<'_>,
    crc_present: bool,
) -> Result<u32, RcpError> {
    e2e::check_fragment_crc_placement(false, crc_present)?;
    e2e::crc32_tc18_for_fragment_train(header, final_fragment, &buffer.segment_refs())
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::acf::{self, ReadSizeOrSegmentNum};
    use crate::avtp;

    fn fragment_info(ms: bool, segment_num: u8) -> ByteMessageInfo {
        ByteMessageInfo {
            ms,
            read_size_segment_num: ReadSizeOrSegmentNum(segment_num),
            ..Default::default()
        }
    }

    // ── fragmentation_supported / new ────────────────────────────────────────

    #[test]
    // fusa:test REQ-FRAG-001
    fn new_with_zero_bound_reports_fragmentation_unsupported() {
        let buf = FragmentReassemblyBuffer::new(0);
        assert!(!buf.fragmentation_supported());
    }

    #[test]
    // fusa:test REQ-FRAG-001
    fn new_with_nonzero_bound_reports_fragmentation_supported() {
        let buf = FragmentReassemblyBuffer::new(128);
        assert!(buf.fragmentation_supported());
    }

    #[test]
    // fusa:test REQ-FRAG-001
    fn new_buffer_is_not_in_progress() {
        let buf = FragmentReassemblyBuffer::new(128);
        assert!(!buf.is_in_progress());
        assert!(buf.segment_refs().is_empty());
    }

    // ── unsupported-stream rejection ─────────────────────────────────────────

    #[test]
    // fusa:test REQ-FRAG-002
    fn accept_fragment_rejects_on_zero_bound_stream() {
        let mut buf = FragmentReassemblyBuffer::new(0);
        let info = fragment_info(true, 0);
        assert_eq!(
            buf.accept_fragment(&info, b"x"),
            Err(RcpError::UnsupportedCmd)
        );
        assert!(!buf.is_in_progress());
    }

    // ── segment_num ordering ─────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-FRAG-003
    fn accept_fragment_accepts_strictly_incrementing_zero_based_segment_nums() {
        let mut buf = FragmentReassemblyBuffer::new(64);
        assert_eq!(
            buf.accept_fragment(&fragment_info(true, 0), b"a"),
            Ok(FragmentAcceptOutcome::Continuing)
        );
        assert_eq!(
            buf.accept_fragment(&fragment_info(true, 1), b"b"),
            Ok(FragmentAcceptOutcome::Continuing)
        );
        assert_eq!(
            buf.accept_fragment(&fragment_info(false, 2), b"c"),
            Ok(FragmentAcceptOutcome::Complete)
        );
        assert_eq!(
            buf.combined_payload(),
            CombinedFragmentPayload::assemble(&[b"a", b"b", b"c"])
        );
    }

    #[test]
    // fusa:test REQ-FRAG-003
    fn accept_fragment_rejects_gap_in_segment_num() {
        let mut buf = FragmentReassemblyBuffer::new(64);
        buf.accept_fragment(&fragment_info(true, 0), b"a").unwrap();
        assert_eq!(
            buf.accept_fragment(&fragment_info(true, 2), b"c"),
            Err(RcpError::InvalidParameter)
        );
    }

    #[test]
    // fusa:test REQ-FRAG-003
    fn accept_fragment_rejects_duplicate_segment_num() {
        let mut buf = FragmentReassemblyBuffer::new(64);
        buf.accept_fragment(&fragment_info(true, 0), b"a").unwrap();
        assert_eq!(
            buf.accept_fragment(&fragment_info(true, 0), b"a-again"),
            Err(RcpError::InvalidParameter)
        );
    }

    #[test]
    // fusa:test REQ-FRAG-003
    fn accept_fragment_rejects_out_of_order_segment_num() {
        let mut buf = FragmentReassemblyBuffer::new(64);
        buf.accept_fragment(&fragment_info(true, 0), b"a").unwrap();
        buf.accept_fragment(&fragment_info(true, 1), b"b").unwrap();
        assert_eq!(
            buf.accept_fragment(&fragment_info(false, 0), b"replay"),
            Err(RcpError::InvalidParameter)
        );
    }

    #[test]
    // fusa:test REQ-FRAG-003
    fn accept_fragment_first_call_must_start_at_segment_num_zero() {
        let mut buf = FragmentReassemblyBuffer::new(64);
        assert_eq!(
            buf.accept_fragment(&fragment_info(true, 1), b"a"),
            Err(RcpError::InvalidParameter)
        );
    }

    #[test]
    // fusa:test REQ-FRAG-003
    fn rejected_fragment_does_not_mutate_buffer_state() {
        let mut buf = FragmentReassemblyBuffer::new(64);
        buf.accept_fragment(&fragment_info(true, 0), b"a").unwrap();
        let before = buf.clone();
        let _ = buf.accept_fragment(&fragment_info(true, 5), b"bad");
        assert_eq!(buf, before);
    }

    // ── rx_stream_max_request_size bound ─────────────────────────────────────

    #[test]
    // fusa:test REQ-FRAG-004
    fn accept_fragment_rejects_when_combined_length_exceeds_bound() {
        let mut buf = FragmentReassemblyBuffer::new(3);
        buf.accept_fragment(&fragment_info(true, 0), b"ab").unwrap();
        assert_eq!(
            buf.accept_fragment(&fragment_info(false, 1), b"cd"),
            Err(RcpError::PayloadTooLarge)
        );
    }

    #[test]
    // fusa:test REQ-FRAG-004
    fn accept_fragment_accepts_combined_length_exactly_at_bound() {
        let mut buf = FragmentReassemblyBuffer::new(4);
        buf.accept_fragment(&fragment_info(true, 0), b"ab").unwrap();
        assert_eq!(
            buf.accept_fragment(&fragment_info(false, 1), b"cd"),
            Ok(FragmentAcceptOutcome::Complete)
        );
    }

    #[test]
    // fusa:test REQ-FRAG-004
    fn overflowing_fragment_does_not_mutate_buffer_state() {
        let mut buf = FragmentReassemblyBuffer::new(2);
        buf.accept_fragment(&fragment_info(true, 0), b"ab").unwrap();
        let before = buf.clone();
        let _ = buf.accept_fragment(&fragment_info(false, 1), b"too-big");
        assert_eq!(buf, before);
    }

    // ── Continuing / Complete / combined payload ─────────────────────────────

    #[test]
    // fusa:test REQ-FRAG-005
    fn single_fragment_train_completes_immediately() {
        let mut buf = FragmentReassemblyBuffer::new(64);
        assert_eq!(
            buf.accept_fragment(&fragment_info(false, 0), b"solo"),
            Ok(FragmentAcceptOutcome::Complete)
        );
        assert_eq!(
            buf.combined_payload(),
            CombinedFragmentPayload::assemble(&[b"solo"])
        );
    }

    #[test]
    // fusa:test REQ-FRAG-005
    fn combined_payload_matches_e2e_combined_fragment_payload_assemble() {
        let mut buf = FragmentReassemblyBuffer::new(64);
        let segments: [&[u8]; 3] = [b"seg-one-", b"seg-two-", b"seg-three"];
        for (i, seg) in segments.iter().enumerate() {
            let ms = i + 1 != segments.len();
            buf.accept_fragment(&fragment_info(ms, i as u8), seg)
                .unwrap();
        }
        assert_eq!(
            buf.combined_payload(),
            CombinedFragmentPayload::assemble(&segments)
        );
    }

    // ── reset ─────────────────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-FRAG-006
    fn reset_clears_state_for_a_new_train() {
        let mut buf = FragmentReassemblyBuffer::new(64);
        buf.accept_fragment(&fragment_info(false, 0), b"first-train")
            .unwrap();
        buf.reset();
        assert!(!buf.is_in_progress());
        assert_eq!(
            buf.accept_fragment(&fragment_info(false, 0), b"second-train"),
            Ok(FragmentAcceptOutcome::Complete)
        );
        assert_eq!(
            buf.combined_payload(),
            CombinedFragmentPayload::assemble(&[b"second-train"])
        );
    }

    #[test]
    // fusa:test REQ-FRAG-006
    fn accepting_a_fragment_after_complete_without_reset_is_rejected() {
        let mut buf = FragmentReassemblyBuffer::new(64);
        buf.accept_fragment(&fragment_info(false, 0), b"done")
            .unwrap();
        // The completed train left next_expected_segment_num at 1; a
        // fresh train's first fragment (segment_num == 0) does not match,
        // so this is rejected until the caller calls `reset`.
        assert_eq!(
            buf.accept_fragment(&fragment_info(true, 0), b"new-train"),
            Err(RcpError::InvalidParameter)
        );
    }

    // ── never panics ──────────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-FRAG-008
    fn accept_fragment_never_panics_across_arbitrary_payload_lengths() {
        for len in [0usize, 1, 2, 3, 17, 64, 257] {
            let mut buf = FragmentReassemblyBuffer::new(u16::MAX);
            let payload: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
            let _ = buf.accept_fragment(&fragment_info(false, 0), &payload);
        }
    }

    #[test]
    // fusa:test REQ-FRAG-008
    fn accept_fragment_never_panics_across_segment_num_wraparound() {
        let mut buf = FragmentReassemblyBuffer::new(u16::MAX);
        // 300 fragments is well past the 8-bit segment_num field's own
        // 256-value range, so `next_expected_segment_num` wraps mid-loop.
        // `segment_num` is derived the same wrapping way the buffer itself
        // derives its expectation, so every call stays in lockstep and
        // succeeds — the property under test is solely that wraparound
        // itself never panics.
        for expected in 0u16..300 {
            let segment_num = (expected % 256) as u8;
            let ms = expected + 1 != 300;
            let result = buf.accept_fragment(&fragment_info(ms, segment_num), b"x");
            assert!(result.is_ok());
        }
    }

    // ── verify_reassembled_train_crc ─────────────────────────────────────────

    fn sample_final_fragment(acf_msg_length: u16) -> acf::AcfAbbMessage {
        acf::AcfAbbMessage {
            info: acf::ByteMessageInfo {
                acf_msg_length,
                ms: false,
                ..Default::default()
            },
            payload: b"ignored-final-fragment-payload".to_vec(),
        }
    }

    #[test]
    // fusa:test REQ-FRAG-007
    fn verify_reassembled_train_crc_matches_manual_e2e_computation() {
        let mut buf = FragmentReassemblyBuffer::new(64);
        buf.accept_fragment(&fragment_info(true, 0), b"hello-")
            .unwrap();
        buf.accept_fragment(&fragment_info(false, 1), b"world")
            .unwrap();

        let header = HeaderVariant::Ntscf(avtp::NtscfHeader {
            stream_id: 0x0102_0304_0506_0708,
            ..Default::default()
        });
        let final_msg = sample_final_fragment(0);
        let final_fragment = AcfCoverageMessage::Abb(&final_msg);

        let got = verify_reassembled_train_crc(&buf, &header, &final_fragment, true).unwrap();
        let want =
            e2e::crc32_tc18_for_fragment_train(&header, &final_fragment, &[b"hello-", b"world"])
                .unwrap();
        assert_eq!(got, want);
    }

    #[test]
    // fusa:test REQ-FRAG-007
    fn verify_reassembled_train_crc_rejects_missing_crc_on_completed_train() {
        let mut buf = FragmentReassemblyBuffer::new(64);
        buf.accept_fragment(&fragment_info(false, 0), b"solo")
            .unwrap();
        let header = HeaderVariant::Ntscf(avtp::NtscfHeader::default());
        let final_msg = sample_final_fragment(0);
        let final_fragment = AcfCoverageMessage::Abb(&final_msg);
        assert_eq!(
            verify_reassembled_train_crc(&buf, &header, &final_fragment, false),
            Err(RcpError::InvalidParameter)
        );
    }

    #[test]
    // fusa:test REQ-FRAG-007
    fn verify_reassembled_train_crc_propagates_length_overflow_error() {
        let mut buf = FragmentReassemblyBuffer::new(64);
        buf.accept_fragment(&fragment_info(false, 0), b"x").unwrap();
        let header = HeaderVariant::Ntscf(avtp::NtscfHeader::default());
        let final_msg = sample_final_fragment(acf::BYTE_MESSAGE_INFO_11BIT_MAX);
        let final_fragment = AcfCoverageMessage::Abb(&final_msg);
        assert_eq!(
            verify_reassembled_train_crc(&buf, &header, &final_fragment, true),
            Err(RcpError::InvalidSize)
        );
    }

    #[test]
    // fusa:test REQ-FRAG-007
    fn verify_reassembled_train_crc_on_empty_buffer_matches_empty_segments() {
        let buf = FragmentReassemblyBuffer::new(64);
        let header = HeaderVariant::Ntscf(avtp::NtscfHeader::default());
        let final_msg = sample_final_fragment(0);
        let final_fragment = AcfCoverageMessage::Abb(&final_msg);
        let got = verify_reassembled_train_crc(&buf, &header, &final_fragment, true).unwrap();
        let want = e2e::crc32_tc18_for_fragment_train(&header, &final_fragment, &[]).unwrap();
        assert_eq!(got, want);
    }
}
