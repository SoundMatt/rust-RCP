// fusa:req REQ-CRC-001
// fusa:req REQ-CRC-002
// fusa:req REQ-CRC-003
// fusa:req REQ-CRC-004
// fusa:req REQ-CRC-005
// fusa:req REQ-CRC-006
// fusa:req REQ-CRC-007
// fusa:req REQ-CRC-008
// fusa:req REQ-CRC-009
// fusa:req REQ-CRC-010

//! End-to-end protection: the OPEN Alliance TC18 safe-point CRC-32
//! mechanism (polynomial `0xF4ACFB13`).
//!
//! ## History: REPLACEd from a CRC-16 + replay-guard model
//! (`ROADMAP.md` Milestone 9, Satellite Package Disposition, `e2e` row)
//!
//! Earlier revisions of this module additively grew the real
//! [`crc32_tc18`] machinery below (Milestone 6) alongside a pre-existing,
//! structurally unrelated legacy mechanism: a fixed `[seqNum(4) |
//! CRC-16(2) | payload]` frame (`wrap`/`unwrap`), a sliding-window
//! anti-replay guard (`ReplayGuard`), and a `Controller`-wrapping decorator
//! (`E2eController`) that applied `wrap` to every outgoing payload. That
//! legacy trio served a different model (a `Zone`/`Controller`-keyed
//! decorator stack) than the spec's own stream/endpoint-addressed one, and
//! implemented a different algorithm (CRC-16/CCITT-FALSE, not the spec's
//! CRC-32) — the Satellite Package Disposition table's own stated reason
//! for giving `e2e` a REPLACE (not ADAPT) disposition. This item removes
//! that legacy trio outright, per the same "delete, don't adapt"
//! discipline the `wire`/`watchdog`/`powerstate` REPLACE items already
//! established, leaving this module as the real TC18 safe-point mechanism
//! only. No other module constructed or matched on `E2eController`,
//! `e2e::wrap`, `e2e::unwrap`, or `ReplayGuard` (confirmed by inspection
//! before this removal), so no caller-side migration was needed.
//!
//! ## Coverage rule (`ROADMAP.md` Milestone 6, "Coverage rule" bullet)
//!
//! [`build_crc32_coverage_buffer`] assembles the exact byte sequence
//! [`crc32_tc18`] is meant to run over for a real safe-point AVTPDU/ACF
//! frame: `stream_id`, then `avtp_timestamp` (or, for an NTSCF-headed
//! frame — which carries no `avtp_timestamp` field at all — four zero
//! bytes in its place), then the full ACF header (the `acf_msg_type`
//! discriminant, `byte_message_info`, and — for ACF_GBB only —
//! `message_timestamp`), then the payload. It reuses
//! [`crate::avtp::HeaderVariant`] to decide which of `stream_id`/
//! `avtp_timestamp` apply, and [`AcfCoverageMessage`] to decide which ACF
//! header shape applies, rather than re-deriving either decision.
//!
//! ### Working interpretation: which length field gets the `+4` octet
//! pre-adjustment (Guiding Principle 5)
//!
//! The Milestone 6 checklist states a length field is pre-adjusted by one
//! quadlet (4 octets) before the CRC is computed over it, but does not say
//! which length field. Of this crate's currently-decoded length fields,
//! `byte_message_info`'s `acf_msg_length` is the only one that lives inside
//! the region this coverage rule actually covers (`stream_id`/
//! `avtp_timestamp`/ACF-header/payload) — the AVTP-level
//! `ntscf_data_length`/`stream_data_length` fields sit entirely outside
//! that region, in [`crate::avtp`]'s NTSCF/TSCF header, which this
//! function does not re-encode. [`build_crc32_coverage_buffer`] therefore
//! treats `acf_msg_length` as the field being pre-adjusted: the value it
//! encodes into the coverage buffer's ACF header is the caller-supplied
//! `ByteMessageInfo::acf_msg_length` plus 4, not the raw as-decoded value.
//! This is this crate's own working interpretation, not a spec-confirmed
//! fact, and is flagged here for reconciliation against real TC18 behavior
//! (never against spec prose) before being relied on for interop.
//!
//! ## Provenance note: `crc32_tc18` verified by cross-implementation, not
//! by a published check value
//!
//! The four parameters `ROADMAP.md`'s Milestone 6 checklist bullet states —
//! polynomial `0xF4ACFB13`, init `0xFFFFFFFF`, final XOR `0xFFFFFFFF`,
//! reflected input and output — fully and unambiguously determine one
//! specific CRC-32 variant mathematically, so (per Guiding Principle 5)
//! there is little genuine ambiguity left to flag about the algorithm
//! itself. But because `0xF4ACFB13` is not a named/published CRC-32
//! variant, there is no externally citable "check value" (the way the
//! standard CRC-32/ISO-HDLC check value for `"123456789"` is publicly
//! known) to test [`crc32_tc18`] against. This module's tests instead
//! cross-validate [`crc32_tc18`]'s reflected-engine implementation (which
//! runs the shift register LSB-first against the bit-reversed polynomial)
//! against a second, structurally independent implementation of the exact
//! same four parameters (which reflects each input byte and the final
//! register by hand around a plain MSB-first shift register using the
//! *unreversed* polynomial) across the classic `"123456789"` corpus,
//! all-zero/all-`0xFF` boundary inputs, and several other patterns — the
//! two agreeing is the correctness evidence, since both are direct,
//! independently-written renderings of the same stated definition.
//!
//! This entry does not take a position on which requests/streams actually
//! get CRC-protected in the first place — that is `ROADMAP.md` Milestone
//! 6's later "Per-stream safety config" bullet's job, not this one's.
//!
//! ## Fragmentation interaction (`ROADMAP.md` Milestone 6, "Fragmentation
//! interaction" bullet)
//!
//! [`CombinedFragmentPayload`], [`build_crc32_coverage_buffer_for_fragment_train`],
//! and [`crc32_tc18_for_fragment_train`] extend the single-message coverage
//! rule above to a message split across multiple ACF frames (a "fragment
//! train"): the CRC is computed once, across the concatenated payload of
//! every fragment in the train, and only the train's final fragment (the
//! one whose [`acf::ByteMessageInfo::ms`] is `false`) carries it on the
//! wire. [`fragment_crc_expectation`]/[`check_fragment_crc_placement`]
//! state and validate that placement rule itself, independent of how the
//! combined-payload CRC is computed.
//!
//! At the time this section was written, this crate had no live
//! multi-AVTPDU reassembly buffer (that was `ROADMAP.md` Milestone 8's
//! still-undecided job) — matching every Milestone 5/6 entry's precedent of
//! taking not-yet-built state as a caller-supplied fact (e.g.
//! `crate::request`'s `SequencerState`/`root_client`), this section takes a
//! fragment train's per-segment payloads as a caller-supplied, already-
//! ordered `&[&[u8]]` rather than reading `acf::ReadSizeOrSegmentNum`
//! itself to determine ordering. That is a deliberate, additional instance
//! of Guiding Principle 5: `acf::ReadSizeOrSegmentNum`'s own provenance
//! note already flags that this crate has not resolved which bit(s), if
//! any, select its `read_size` vs. `segment_num` interpretation, so this
//! module treats "this is a fragment train, and this is its segment order"
//! as a fact the caller establishes out of band, rather than silently
//! resolving that open ambiguity here.
//!
//! `ROADMAP.md` Milestone 8 has since decided "go" and landed
//! [`crate::fragment::FragmentReassemblyBuffer`], a real reassembly buffer
//! that derives segment order from wire-arrival order plus a validated
//! `segment_num` sequence rather than a caller-supplied slice.
//! [`crate::fragment::verify_reassembled_train_crc`] composes
//! [`crc32_tc18_for_fragment_train`] against that buffer's own
//! wire-collected segments, re-verifying this section's CRC-placement rule
//! against real reassembled state. The functions in this section keep their
//! own `&[&[u8]]`-based signatures unchanged — Milestone 8 composed with
//! them from `crate::fragment`, rather than editing them here.
//!
//! ### Working interpretation: non-payload coverage fields come from the
//! final fragment's own header (Guiding Principle 5)
//!
//! The roadmap states the CRC is "computed across the combined payload"
//! but does not say whether the coverage buffer's non-payload region
//! (`stream_id`, `avtp_timestamp`, and the ACF header's own fields other
//! than the payload) should likewise be drawn from each fragment
//! individually (and, if so, how they would be combined — concatenated,
//! required to match, or something else) or taken once from a single
//! fragment. Since only the final fragment carries the CRC at all, and an
//! intermediate fragment's own `byte_message_info`/`message_timestamp`
//! describe *that fragment*, not the reassembled message,
//! [`build_crc32_coverage_buffer_for_fragment_train`] takes the entire
//! non-payload region from the caller-supplied final fragment's own
//! [`AcfCoverageMessage`] — mirroring [`build_crc32_coverage_buffer`]'s
//! existing single-message behavior with the payload field alone replaced
//! by the train's combined payload — rather than inventing a multi-
//! fragment header-combination rule the roadmap text does not state. This
//! is this crate's own working interpretation, flagged here for
//! reconciliation against real TC18 behavior (never against spec prose)
//! before being relied on for interop.

use crate::acf::{self, AcfAbbMessage, AcfGbbMessage};
use crate::avtp::HeaderVariant;
use crate::RcpError;

// ── CRC-32 (TC18 safe-point) ────────────────────────────────────────────────

/// Bit-reversal of the OPEN Alliance TC18 safe-point CRC-32 polynomial
/// `0xF4ACFB13`, precomputed so [`crc32_tc18`] can shift its register right
/// (LSB-first) instead of reflecting every input byte and the final
/// register by hand — the standard technique for implementing a reflected
/// CRC without an explicit per-byte/per-register bit-reversal step. See
/// this module's provenance note above; the unreversed polynomial itself
/// reappears in `tests::crc32_tc18_reference`, the independent
/// cross-check.
const CRC32_TC18_POLY_REFLECTED: u32 = 0xC8DF_352F;

/// Initial and final-XOR value, `0xFFFFFFFF`, shared by both ends of the
/// algorithm per `ROADMAP.md` Milestone 6's stated parameters.
const CRC32_TC18_INIT_XOROUT: u32 = 0xFFFF_FFFF;

/// Computes the OPEN Alliance TC18 safe-point CRC-32 over `data`.
///
/// Polynomial `0xF4ACFB13`, init and final XOR both `0xFFFFFFFF`, reflected
/// input and output.
///
/// This function computes the CRC over exactly the bytes it is given; it
/// takes no position on which bytes of a safe-point frame belong in that
/// slice (see this module's provenance note above).
// fusa:req REQ-CRC-001
// fusa:req REQ-CRC-002
pub fn crc32_tc18(data: &[u8]) -> u32 {
    let mut crc = CRC32_TC18_INIT_XOROUT;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ CRC32_TC18_POLY_REFLECTED
            } else {
                crc >> 1
            };
        }
    }
    crc ^ CRC32_TC18_INIT_XOROUT
}

// ── CRC-32 coverage rule ─────────────────────────────────────────────────────

/// Number of octets the "length-field pre-adjustment" in `ROADMAP.md`
/// Milestone 6's "Coverage rule" bullet adds to a length field before the
/// CRC is computed: one quadlet (4 octets). See this module's doc comment
/// for which length field [`build_crc32_coverage_buffer`] applies this to,
/// and why — this crate's own working interpretation, flagged per Guiding
/// Principle 5.
const CRC32_COVERAGE_LENGTH_PREADJUST_OCTETS: u16 = 4;

/// Which of the two Milestone 1 ACF message shapes a
/// [`build_crc32_coverage_buffer`] call's "full ACF header" is drawn from.
///
/// Mirrors [`AcfAbbMessage`]/[`AcfGbbMessage`] rather than adding a new
/// decoded representation of either: this type only selects which one
/// applies, it does not reinterpret either message's fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcfCoverageMessage<'a> {
    /// An ACF_ABB message (no `message_timestamp`; `ACF_ABB_HEADER_LEN` ==
    /// 9-byte header before payload).
    Abb(&'a AcfAbbMessage),
    /// An ACF_GBB message (carries `message_timestamp`;
    /// `ACF_GBB_HEADER_LEN` == 17-byte header before payload).
    Gbb(&'a AcfGbbMessage),
}

/// Assembles the exact byte sequence [`crc32_tc18`] is meant to run over
/// for a real safe-point AVTPDU/ACF frame, per `ROADMAP.md` Milestone 6's
/// "Coverage rule" bullet: `stream_id`, then `avtp_timestamp` (zeroed under
/// NTSCF), then the full ACF header, then the payload.
///
/// - `stream_id` comes from whichever of [`HeaderVariant::Ntscf`]/
///   [`HeaderVariant::Tscf`] `header` is, encoded big-endian as 8 bytes.
/// - `avtp_timestamp` is the real 4-byte big-endian value from
///   [`HeaderVariant::Tscf`], or, for [`HeaderVariant::Ntscf`] (which has no
///   `avtp_timestamp` field to carry), four zero bytes occupying the same
///   position in the buffer rather than being omitted.
/// - The full ACF header is `acf`'s discriminant byte, `byte_message_info`,
///   and (for [`AcfCoverageMessage::Gbb`] only) the 8-byte
///   `message_timestamp` — reusing [`acf::encode_acf_abb`]/
///   [`acf::encode_acf_gbb`] (which also appends the payload, completing
///   the buffer in the same call) rather than re-deriving either message's
///   wire layout here. Before encoding, `acf`'s `byte_message_info.
///   acf_msg_length` is increased by [`CRC32_COVERAGE_LENGTH_PREADJUST_OCTETS`]
///   — see this module's doc comment for why that field specifically.
///
/// Returns `Err(RcpError::InvalidSize)` if the pre-adjusted `acf_msg_length`
/// (or any other `ByteMessageInfo` field) fails
/// [`acf::encode_byte_message_info`]'s field-width validation — the same
/// error [`acf::encode_acf_abb`]/[`acf::encode_acf_gbb`] would themselves
/// return for an out-of-range header.
///
/// Additive standalone plumbing, matching every prior Milestone 1-6 entry's
/// discipline: not called from [`crc32_tc18`] or a decoder/dispatch loop —
/// this function only assembles the buffer a caller would pass to
/// `crc32_tc18` and `crate::request`'s `CRC_ERROR` dispatch path.
// fusa:req REQ-CRC-004
// fusa:req REQ-CRC-005
// fusa:req REQ-CRC-006
// fusa:req REQ-CRC-007
pub fn build_crc32_coverage_buffer(
    header: &HeaderVariant,
    acf: &AcfCoverageMessage,
) -> Result<Vec<u8>, RcpError> {
    let (stream_id, avtp_timestamp_bytes) = match header {
        HeaderVariant::Ntscf(h) => (h.stream_id, [0u8; 4]),
        HeaderVariant::Tscf(h) => (h.stream_id, h.avtp_timestamp.to_be_bytes()),
    };

    let mut buf = Vec::new();
    buf.extend_from_slice(&stream_id.to_be_bytes());
    buf.extend_from_slice(&avtp_timestamp_bytes);

    match acf {
        AcfCoverageMessage::Abb(msg) => {
            let mut adjusted = (*msg).clone();
            adjusted.info.acf_msg_length = adjusted
                .info
                .acf_msg_length
                .saturating_add(CRC32_COVERAGE_LENGTH_PREADJUST_OCTETS);
            buf.extend_from_slice(&acf::encode_acf_abb(&adjusted)?);
        }
        AcfCoverageMessage::Gbb(msg) => {
            let mut adjusted = (*msg).clone();
            adjusted.info.acf_msg_length = adjusted
                .info
                .acf_msg_length
                .saturating_add(CRC32_COVERAGE_LENGTH_PREADJUST_OCTETS);
            buf.extend_from_slice(&acf::encode_acf_gbb(&adjusted)?);
        }
    }

    Ok(buf)
}

// ── Fragmentation interaction ────────────────────────────────────────────────

/// The combined payload of a multi-segment "fragment train", assembled by
/// concatenating each fragment's own payload in the order the caller
/// supplies them.
///
/// This crate has no live multi-AVTPDU reassembly buffer to read segment
/// order from yet (`ROADMAP.md` Milestone 8), so segment order is a
/// caller-supplied fact rather than something derived from
/// `acf::ReadSizeOrSegmentNum::as_segment_num` here — see this module's
/// doc comment "Fragmentation interaction" section for why.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CombinedFragmentPayload(pub Vec<u8>);

impl CombinedFragmentPayload {
    /// Assembles a fragment train's combined payload by concatenating
    /// `segments` verbatim, in the order given. An empty `segments` slice
    /// yields an empty combined payload; this function never panics for
    /// any input, including empty per-segment payloads.
    // fusa:req REQ-CRC-009
    pub fn assemble(segments: &[&[u8]]) -> Self {
        let mut combined = Vec::new();
        for segment in segments {
            combined.extend_from_slice(segment);
        }
        CombinedFragmentPayload(combined)
    }
}

/// Assembles the CRC coverage buffer for a fragment train: identical to
/// [`build_crc32_coverage_buffer`], except the payload region is the
/// train's [`CombinedFragmentPayload`] (assembled from `segments`) rather
/// than `final_fragment`'s own single-fragment payload.
///
/// `header` and `final_fragment` supply every non-payload field of the
/// coverage buffer — `stream_id`, `avtp_timestamp`, and the full ACF
/// header (including, for [`AcfCoverageMessage::Gbb`], `message_timestamp`)
/// — from the train's final fragment (the one whose `ms` is `false`); see
/// this module's doc comment for why the final fragment's own header is
/// used rather than combining every fragment's header fields. The same
/// length-field pre-adjustment [`build_crc32_coverage_buffer`] applies is
/// applied here too, and this function returns the same
/// `Err(RcpError::InvalidSize)` that call would return for an out-of-range
/// header.
///
/// Additive standalone plumbing, matching every prior Milestone 1-6 entry's
/// discipline: composes [`build_crc32_coverage_buffer`] rather than
/// re-deriving its buffer-assembly logic, and is not wired into
/// `crc32_tc18` or a decoder/dispatch loop.
// fusa:req REQ-CRC-010
pub fn build_crc32_coverage_buffer_for_fragment_train(
    header: &HeaderVariant,
    final_fragment: &AcfCoverageMessage,
    segments: &[&[u8]],
) -> Result<Vec<u8>, RcpError> {
    let combined = CombinedFragmentPayload::assemble(segments);
    match final_fragment {
        AcfCoverageMessage::Abb(msg) => {
            let combined_msg = AcfAbbMessage {
                info: msg.info,
                payload: combined.0,
            };
            build_crc32_coverage_buffer(header, &AcfCoverageMessage::Abb(&combined_msg))
        }
        AcfCoverageMessage::Gbb(msg) => {
            let combined_msg = AcfGbbMessage {
                info: msg.info,
                message_timestamp: msg.message_timestamp,
                payload: combined.0,
            };
            build_crc32_coverage_buffer(header, &AcfCoverageMessage::Gbb(&combined_msg))
        }
    }
}

/// Computes the safe-point CRC-32 for a fragment train: composes
/// [`build_crc32_coverage_buffer_for_fragment_train`] and [`crc32_tc18`]
/// rather than re-deriving either. This is the value the train's *final*
/// fragment carries on the wire per the "only the last fragment carries
/// the CRC" rule — see [`fragment_crc_expectation`]/
/// [`check_fragment_crc_placement`] for that placement rule itself.
// fusa:req REQ-CRC-010
pub fn crc32_tc18_for_fragment_train(
    header: &HeaderVariant,
    final_fragment: &AcfCoverageMessage,
    segments: &[&[u8]],
) -> Result<u32, RcpError> {
    let buf = build_crc32_coverage_buffer_for_fragment_train(header, final_fragment, segments)?;
    Ok(crc32_tc18(&buf))
}

/// Whether a CRC32 safe-point value is expected to accompany a given
/// fragment of a fragment train, keyed only on that fragment's own `ms`
/// ("more segments") flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FragmentCrcExpectation {
    /// `ms == true`: more segments follow this one. No CRC field is
    /// expected to accompany this fragment.
    NotExpected,
    /// `ms == false`: this is the fragment train's final fragment. A CRC
    /// field, computed across the train's combined payload (see
    /// [`crc32_tc18_for_fragment_train`]), is expected to accompany it.
    Expected,
}

/// Derives [`FragmentCrcExpectation`] from a fragment's `ms` flag alone,
/// per the "only the last fragment carries the CRC" rule.
// fusa:req REQ-CRC-008
pub fn fragment_crc_expectation(ms: bool) -> FragmentCrcExpectation {
    if ms {
        FragmentCrcExpectation::NotExpected
    } else {
        FragmentCrcExpectation::Expected
    }
}

/// Validates a fragment's actual CRC-presence state against
/// [`fragment_crc_expectation`]'s rule, rather than silently ignoring a
/// violation of it.
///
/// Returns `Ok(())` when `ms == true` and `crc_present == false` (an
/// intermediate fragment correctly carrying no CRC), or when `ms == false`
/// and `crc_present == true` (the final fragment correctly carrying one).
/// Returns `Err(RcpError::InvalidParameter)` for either invalid
/// combination: a CRC present on a non-final fragment, or absent on the
/// final one — mirroring this crate's existing convention (see
/// `crate::request::check_compound_bundle_claim`) of reporting caller-
/// supplied state that fails a shape/consistency check as
/// `InvalidParameter`, rather than inventing a new sentinel for this one
/// rule ahead of the later "`CRC_ERROR` error path" checklist item, which
/// is scoped to the wire-level error code a received `CRC_ERROR` produces,
/// not to this placement rule.
// fusa:req REQ-CRC-008
pub fn check_fragment_crc_placement(ms: bool, crc_present: bool) -> Result<(), RcpError> {
    match (fragment_crc_expectation(ms), crc_present) {
        (FragmentCrcExpectation::NotExpected, false) => Ok(()),
        (FragmentCrcExpectation::Expected, true) => Ok(()),
        _ => Err(RcpError::InvalidParameter),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::avtp;

    // ── crc32_tc18 ──────────────────────────────────────────────────────────

    /// Independent reference implementation of the same
    /// poly=`0xF4ACFB13`/init=`0xFFFFFFFF`/xorout=`0xFFFFFFFF`/reflect-in/
    /// reflect-out CRC-32 definition, structured differently from
    /// [`crc32_tc18`]: it reflects each input byte and the final register
    /// by hand around a plain MSB-first shift register using the
    /// *unreversed* polynomial, instead of [`crc32_tc18`]'s LSB-first
    /// shift against the pre-reversed polynomial. Exists only so the two
    /// can be cross-checked against each other — see this module's
    /// provenance note. Test-only; not part of the public API.
    fn crc32_tc18_reference(data: &[u8]) -> u32 {
        // The un-reversed OPEN Alliance TC18 safe-point CRC-32 polynomial;
        // deliberately re-declared here (rather than sharing
        // `CRC32_TC18_POLY_REFLECTED`) so this reference implementation
        // has no code in common with `crc32_tc18` beyond the four stated
        // parameters themselves.
        const POLY: u32 = 0xF4AC_FB13;
        let mut crc = CRC32_TC18_INIT_XOROUT;
        for &byte in data {
            crc ^= (byte.reverse_bits() as u32) << 24;
            for _ in 0..8 {
                crc = if crc & 0x8000_0000 != 0 {
                    (crc << 1) ^ POLY
                } else {
                    crc << 1
                };
            }
        }
        crc.reverse_bits() ^ CRC32_TC18_INIT_XOROUT
    }

    #[test]
    // fusa:test REQ-CRC-001
    fn crc32_tc18_empty_input_matches_reference() {
        assert_eq!(crc32_tc18(&[]), crc32_tc18_reference(&[]));
        assert_eq!(crc32_tc18(&[]), 0x0000_0000);
    }

    #[test]
    // fusa:test REQ-CRC-001
    fn crc32_tc18_ascii_check_string_matches_reference() {
        // "123456789" is the conventional CRC-32 check corpus; the expected
        // constant below is this polynomial's own derived value (see this
        // module's provenance note), not an externally published one.
        let data = b"123456789";
        assert_eq!(crc32_tc18(data), crc32_tc18_reference(data));
        assert_eq!(crc32_tc18(data), 0x1697_d06a);
    }

    #[test]
    // fusa:test REQ-CRC-001
    fn crc32_tc18_all_zero_boundary_matches_reference() {
        let data = [0u8; 16];
        assert_eq!(crc32_tc18(&data), crc32_tc18_reference(&data));
        assert_eq!(crc32_tc18(&data), 0x0fa6_214b);
    }

    #[test]
    // fusa:test REQ-CRC-001
    fn crc32_tc18_all_0xff_boundary_matches_reference() {
        let data = [0xFFu8; 16];
        assert_eq!(crc32_tc18(&data), crc32_tc18_reference(&data));
        assert_eq!(crc32_tc18(&data), 0xb0f2_7ef5);
    }

    #[test]
    // fusa:test REQ-CRC-001
    fn crc32_tc18_matches_reference_across_varied_inputs() {
        let vectors: [&[u8]; 4] = [
            b"OPEN Alliance TC18",
            &[0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07],
            b"a",
            b"safe-point",
        ];
        for v in vectors {
            assert_eq!(crc32_tc18(v), crc32_tc18_reference(v));
        }
        let pattern: Vec<u8> = (0..=255u8).collect();
        assert_eq!(crc32_tc18(&pattern), crc32_tc18_reference(&pattern));
    }

    #[test]
    // fusa:test REQ-CRC-002
    fn crc32_tc18_never_panics_across_arbitrary_lengths() {
        for len in [0usize, 1, 2, 3, 6, 17, 64, 257, 1000] {
            let data: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
            let _ = crc32_tc18(&data);
        }
    }

    #[test]
    // fusa:test REQ-CRC-003
    fn crc32_tc18_different_payload_produces_different_crc() {
        let a = crc32_tc18(b"payload-a");
        let b = crc32_tc18(b"payload-b");
        assert_ne!(a, b);
    }

    #[test]
    // fusa:test REQ-CRC-003
    fn crc32_tc18_single_bit_flip_changes_crc() {
        let mut data = b"integrity check data".to_vec();
        let baseline = crc32_tc18(&data);
        data[0] ^= 0x01;
        assert_ne!(crc32_tc18(&data), baseline);
    }

    // ── CRC-32 coverage rule ────────────────────────────────────────────────

    fn sample_abb_message(acf_msg_length: u16, payload: &[u8]) -> AcfAbbMessage {
        AcfAbbMessage {
            info: acf::ByteMessageInfo {
                acf_msg_length,
                ..Default::default()
            },
            payload: payload.to_vec(),
        }
    }

    fn sample_gbb_message(
        acf_msg_length: u16,
        message_timestamp: u64,
        payload: &[u8],
    ) -> AcfGbbMessage {
        AcfGbbMessage {
            info: acf::ByteMessageInfo {
                acf_msg_length,
                ..Default::default()
            },
            message_timestamp,
            payload: payload.to_vec(),
        }
    }

    #[test]
    // fusa:test REQ-CRC-004
    fn coverage_buffer_leads_with_stream_id_bytes() {
        let header = HeaderVariant::Ntscf(avtp::NtscfHeader {
            stream_id: 0x0102_0304_0506_0708,
            ..Default::default()
        });
        let acf = AcfCoverageMessage::Abb(&sample_abb_message(0, b"pl"));
        let buf = build_crc32_coverage_buffer(&header, &acf).unwrap();
        assert_eq!(&buf[0..8], &0x0102_0304_0506_0708u64.to_be_bytes());
    }

    #[test]
    // fusa:test REQ-CRC-005
    fn coverage_buffer_zeroes_avtp_timestamp_under_ntscf() {
        let header = HeaderVariant::Ntscf(avtp::NtscfHeader {
            stream_id: 0xAABB_CCDD_EEFF_0011,
            ..Default::default()
        });
        let acf = AcfCoverageMessage::Abb(&sample_abb_message(0, b"pl"));
        let buf = build_crc32_coverage_buffer(&header, &acf).unwrap();
        // Bytes [8..12] are the avtp_timestamp position, immediately after
        // the 8-byte stream_id — must be all-zero, not omitted, per the
        // "zeroed under NTSCF" rule.
        assert_eq!(&buf[8..12], &[0, 0, 0, 0]);
    }

    #[test]
    // fusa:test REQ-CRC-005
    fn coverage_buffer_uses_real_avtp_timestamp_under_tscf() {
        let header = HeaderVariant::Tscf(avtp::TscfHeader {
            stream_id: 0x1,
            avtp_timestamp: 0xDEAD_BEEF,
            ..Default::default()
        });
        let acf = AcfCoverageMessage::Abb(&sample_abb_message(0, b"pl"));
        let buf = build_crc32_coverage_buffer(&header, &acf).unwrap();
        assert_eq!(&buf[8..12], &0xDEAD_BEEFu32.to_be_bytes());
    }

    #[test]
    // fusa:test REQ-CRC-007
    fn coverage_buffer_abb_header_has_no_message_timestamp_region() {
        let header = HeaderVariant::Ntscf(avtp::NtscfHeader::default());
        let payload = b"abb-payload";
        let acf = AcfCoverageMessage::Abb(&sample_abb_message(0, payload));
        let buf = build_crc32_coverage_buffer(&header, &acf).unwrap();
        // 8 (stream_id) + 4 (avtp_timestamp) + ACF_ABB_HEADER_LEN + payload.
        assert_eq!(buf.len(), 12 + acf::ACF_ABB_HEADER_LEN + payload.len());
        // The ACF discriminant byte sits right after the 12-byte prefix.
        assert_eq!(buf[12], acf::ACF_ABB_MSG_TYPE);
        assert_eq!(&buf[buf.len() - payload.len()..], payload);
    }

    #[test]
    // fusa:test REQ-CRC-007
    fn coverage_buffer_gbb_header_carries_message_timestamp() {
        let header = HeaderVariant::Ntscf(avtp::NtscfHeader::default());
        let payload = b"gbb-payload";
        let msg = sample_gbb_message(0, 0x0011_2233_4455_6677, payload);
        let acf = AcfCoverageMessage::Gbb(&msg);
        let buf = build_crc32_coverage_buffer(&header, &acf).unwrap();
        // 8 (stream_id) + 4 (avtp_timestamp) + ACF_GBB_HEADER_LEN + payload.
        assert_eq!(buf.len(), 12 + acf::ACF_GBB_HEADER_LEN + payload.len());
        assert_eq!(buf[12], acf::ACF_GBB_MSG_TYPE);
        // message_timestamp occupies the 8 bytes just before the payload
        // (ACF_GBB_HEADER_LEN already accounts for byte_message_info's
        // width, so it starts right after that).
        let ts_start = 12 + 1 + acf::BYTE_MESSAGE_INFO_LEN;
        let ts_end = ts_start + 8;
        assert_eq!(
            &buf[ts_start..ts_end],
            &0x0011_2233_4455_6677u64.to_be_bytes()
        );
        assert_eq!(&buf[buf.len() - payload.len()..], payload);
    }

    #[test]
    // fusa:test REQ-CRC-006
    fn coverage_buffer_preadjusts_acf_msg_length_by_one_quadlet() {
        let header = HeaderVariant::Ntscf(avtp::NtscfHeader::default());
        let acf_msg_length = 0x0100u16;
        let acf = AcfCoverageMessage::Abb(&sample_abb_message(acf_msg_length, b"x"));
        let buf = build_crc32_coverage_buffer(&header, &acf).unwrap();
        // The ACF header region starts at byte 12 (after stream_id +
        // avtp_timestamp); byte_message_info follows the 1-byte
        // discriminant.
        let info_start = 12 + 1;
        let decoded = acf::decode_byte_message_info(
            &buf[info_start..info_start + acf::BYTE_MESSAGE_INFO_LEN],
        )
        .unwrap();
        assert_eq!(
            decoded.acf_msg_length,
            acf_msg_length + CRC32_COVERAGE_LENGTH_PREADJUST_OCTETS
        );
    }

    #[test]
    // fusa:test REQ-CRC-006
    fn coverage_buffer_rejects_length_that_overflows_11_bits_after_preadjustment() {
        let header = HeaderVariant::Ntscf(avtp::NtscfHeader::default());
        // Max legal 11-bit acf_msg_length; +4 pushes it past the field
        // width, so encoding must fail the same way
        // `acf::encode_byte_message_info` itself would.
        let acf =
            AcfCoverageMessage::Abb(&sample_abb_message(acf::BYTE_MESSAGE_INFO_11BIT_MAX, b"x"));
        assert_eq!(
            build_crc32_coverage_buffer(&header, &acf),
            Err(RcpError::InvalidSize)
        );
    }

    #[test]
    // fusa:test REQ-CRC-004
    fn coverage_buffer_feeds_crc32_tc18_without_panicking() {
        let header = HeaderVariant::Tscf(avtp::TscfHeader {
            stream_id: 0x0203_0405_0607_0809,
            avtp_timestamp: 0x1234_5678,
            ..Default::default()
        });
        let msg = sample_gbb_message(0x0010, 0x99, b"safe-point payload");
        let acf = AcfCoverageMessage::Gbb(&msg);
        let buf = build_crc32_coverage_buffer(&header, &acf).unwrap();
        let _ = crc32_tc18(&buf);
    }

    #[test]
    // fusa:test REQ-CRC-004
    fn coverage_buffer_changes_when_stream_id_differs() {
        let payload = b"same-payload";
        let acf = AcfCoverageMessage::Abb(&sample_abb_message(0, payload));
        let h1 = HeaderVariant::Ntscf(avtp::NtscfHeader {
            stream_id: 1,
            ..Default::default()
        });
        let h2 = HeaderVariant::Ntscf(avtp::NtscfHeader {
            stream_id: 2,
            ..Default::default()
        });
        let b1 = build_crc32_coverage_buffer(&h1, &acf).unwrap();
        let b2 = build_crc32_coverage_buffer(&h2, &acf).unwrap();
        assert_ne!(crc32_tc18(&b1), crc32_tc18(&b2));
    }

    // ── Fragmentation interaction ────────────────────────────────────────────

    #[test]
    // fusa:test REQ-CRC-009
    fn combined_fragment_payload_concatenates_in_given_order() {
        let segments: [&[u8]; 3] = [b"ab", b"cd", b"ef"];
        let combined = CombinedFragmentPayload::assemble(&segments);
        assert_eq!(combined.0, b"abcdef".to_vec());
    }

    #[test]
    // fusa:test REQ-CRC-009
    fn combined_fragment_payload_empty_segments_yields_empty() {
        let segments: [&[u8]; 0] = [];
        let combined = CombinedFragmentPayload::assemble(&segments);
        assert!(combined.0.is_empty());
    }

    #[test]
    // fusa:test REQ-CRC-009
    fn combined_fragment_payload_single_segment_matches_it_verbatim() {
        let segments: [&[u8]; 1] = [b"solo"];
        let combined = CombinedFragmentPayload::assemble(&segments);
        assert_eq!(combined.0, b"solo".to_vec());
    }

    #[test]
    // fusa:test REQ-CRC-008
    fn fragment_crc_expectation_not_expected_when_more_segments_follow() {
        assert_eq!(
            fragment_crc_expectation(true),
            FragmentCrcExpectation::NotExpected
        );
    }

    #[test]
    // fusa:test REQ-CRC-008
    fn fragment_crc_expectation_expected_on_final_fragment() {
        assert_eq!(
            fragment_crc_expectation(false),
            FragmentCrcExpectation::Expected
        );
    }

    #[test]
    // fusa:test REQ-CRC-008
    fn check_fragment_crc_placement_accepts_no_crc_on_intermediate_fragment() {
        assert_eq!(check_fragment_crc_placement(true, false), Ok(()));
    }

    #[test]
    // fusa:test REQ-CRC-008
    fn check_fragment_crc_placement_accepts_crc_on_final_fragment() {
        assert_eq!(check_fragment_crc_placement(false, true), Ok(()));
    }

    #[test]
    // fusa:test REQ-CRC-008
    fn check_fragment_crc_placement_rejects_crc_on_intermediate_fragment() {
        assert_eq!(
            check_fragment_crc_placement(true, true),
            Err(RcpError::InvalidParameter)
        );
    }

    #[test]
    // fusa:test REQ-CRC-008
    fn check_fragment_crc_placement_rejects_missing_crc_on_final_fragment() {
        assert_eq!(
            check_fragment_crc_placement(false, false),
            Err(RcpError::InvalidParameter)
        );
    }

    #[test]
    // fusa:test REQ-CRC-010
    fn coverage_buffer_for_fragment_train_matches_manual_concatenation() {
        let header = HeaderVariant::Ntscf(avtp::NtscfHeader {
            stream_id: 0x0102_0304_0506_0708,
            ..Default::default()
        });
        let segments: [&[u8]; 3] = [b"seg-one-", b"seg-two-", b"seg-three"];
        let final_info = acf::ByteMessageInfo {
            acf_msg_length: 0x20,
            ms: false,
            ..Default::default()
        };
        // The final fragment's own payload field is irrelevant to the
        // train buffer — only its header fields (info) are consulted; the
        // payload region comes from `segments` instead.
        let final_fragment_msg = AcfAbbMessage {
            info: final_info,
            payload: b"ignored-final-fragment-payload".to_vec(),
        };
        let final_fragment = AcfCoverageMessage::Abb(&final_fragment_msg);

        let via_train_fn =
            build_crc32_coverage_buffer_for_fragment_train(&header, &final_fragment, &segments)
                .unwrap();

        let manual_combined_msg = AcfAbbMessage {
            info: final_info,
            payload: b"seg-one-seg-two-seg-three".to_vec(),
        };
        let manual =
            build_crc32_coverage_buffer(&header, &AcfCoverageMessage::Abb(&manual_combined_msg))
                .unwrap();

        assert_eq!(via_train_fn, manual);
    }

    #[test]
    // fusa:test REQ-CRC-010
    fn coverage_buffer_for_fragment_train_changes_when_any_segment_differs() {
        let header = HeaderVariant::Ntscf(avtp::NtscfHeader::default());
        let final_fragment_msg = sample_abb_message(0, b"unused");
        let final_fragment = AcfCoverageMessage::Abb(&final_fragment_msg);

        let segments_a: [&[u8]; 2] = [b"aaaa", b"bbbb"];
        let segments_b: [&[u8]; 2] = [b"aaaa", b"cccc"];

        let buf_a =
            build_crc32_coverage_buffer_for_fragment_train(&header, &final_fragment, &segments_a)
                .unwrap();
        let buf_b =
            build_crc32_coverage_buffer_for_fragment_train(&header, &final_fragment, &segments_b)
                .unwrap();
        assert_ne!(buf_a, buf_b);
    }

    #[test]
    // fusa:test REQ-CRC-010
    fn coverage_buffer_for_fragment_train_gbb_carries_final_fragment_timestamp() {
        let header = HeaderVariant::Ntscf(avtp::NtscfHeader::default());
        let final_fragment_msg = sample_gbb_message(0, 0x1122_3344_5566_7788, b"unused");
        let final_fragment = AcfCoverageMessage::Gbb(&final_fragment_msg);
        let segments: [&[u8]; 2] = [b"part-a", b"part-b"];

        let buf =
            build_crc32_coverage_buffer_for_fragment_train(&header, &final_fragment, &segments)
                .unwrap();

        assert_eq!(buf[12], acf::ACF_GBB_MSG_TYPE);
        let ts_start = 12 + 1 + acf::BYTE_MESSAGE_INFO_LEN;
        let ts_end = ts_start + 8;
        assert_eq!(
            &buf[ts_start..ts_end],
            &0x1122_3344_5566_7788u64.to_be_bytes()
        );
        assert_eq!(&buf[buf.len() - 12..], b"part-apart-b");
    }

    #[test]
    // fusa:test REQ-CRC-010
    fn coverage_buffer_for_fragment_train_propagates_length_overflow_error() {
        let header = HeaderVariant::Ntscf(avtp::NtscfHeader::default());
        let final_fragment_msg = sample_abb_message(acf::BYTE_MESSAGE_INFO_11BIT_MAX, b"unused");
        let final_fragment = AcfCoverageMessage::Abb(&final_fragment_msg);
        let segments: [&[u8]; 1] = [b"x"];
        assert_eq!(
            build_crc32_coverage_buffer_for_fragment_train(&header, &final_fragment, &segments),
            Err(RcpError::InvalidSize)
        );
    }

    #[test]
    // fusa:test REQ-CRC-010
    fn crc32_tc18_for_fragment_train_matches_manual_computation() {
        let header = HeaderVariant::Tscf(avtp::TscfHeader {
            stream_id: 0x0203_0405_0607_0809,
            avtp_timestamp: 0x1234_5678,
            ..Default::default()
        });
        let final_fragment_msg = sample_abb_message(0, b"unused");
        let final_fragment = AcfCoverageMessage::Abb(&final_fragment_msg);
        let segments: [&[u8]; 2] = [b"hello-", b"world"];

        let via_helper =
            crc32_tc18_for_fragment_train(&header, &final_fragment, &segments).unwrap();
        let buf =
            build_crc32_coverage_buffer_for_fragment_train(&header, &final_fragment, &segments)
                .unwrap();
        assert_eq!(via_helper, crc32_tc18(&buf));
    }
}
