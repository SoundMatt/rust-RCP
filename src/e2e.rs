//fusa:req REQ-CRC-001
//fusa:req REQ-CRC-002
//fusa:req REQ-CRC-003
//fusa:req REQ-CRC-004
//fusa:req REQ-CRC-005
//fusa:req REQ-CRC-006
//fusa:req REQ-CRC-007
//fusa:req REQ-CRC-008
//fusa:req REQ-CRC-009
//fusa:req REQ-CRC-010

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
//! bytes in its place), then the full ACF header (`byte_message_info`,
//! which folds in the `acf_msg_type` discriminant, and — for ACF_GBB only
//! — `message_timestamp`), then the payload. It reuses
//! [`crate::avtp::HeaderVariant`] to decide which of `stream_id`/
//! `avtp_timestamp` apply, and [`AcfCoverageMessage`] to decide which ACF
//! header shape applies, rather than re-deriving either decision.
//!
//! ### Which length field gets the one-quadlet pre-adjustment
//! (working interpretation, since reconciled against TC18 §13.6)
//!
//! **Reconciled.** TC18 §13.6 (TC18.txt line 3798) names the field
//! outright: "Before CRC calculation it is essential to adapt the
//! acf_message_length by plus 1 quadlet for the CRC32 addition to the
//! payload and in the AVTPD to increase the ntscf_data_length or in an TSCF
//! header the stream_data_length by 4 octets per ACF type in the ACF
//! payload being E-2-E protected." The `acf_msg_length + 1 quadlet` half of
//! that clause is what [`build_crc32_coverage_buffer`] implements, and the
//! working interpretation recorded below turned out to be correct. The
//! **second** half — increasing the AVTPDU-level `ntscf_data_length` /
//! `stream_data_length` by 4 octets per E-2-E-protected ACF type — is *not*
//! implemented anywhere in this crate; see requirement `REQ-CRC-016`.
//!
//! The original reasoning, kept for the record: the Milestone 6 checklist
//! stated a length field is pre-adjusted by one quadlet (4 octets) before
//! the CRC is computed over it, but did not say which length field. Of
//! this crate's currently-decoded length fields,
//! `byte_message_info`'s `acf_msg_length` is the only one that lives inside
//! the region this coverage rule actually covers (`stream_id`/
//! `avtp_timestamp`/ACF-header/payload) — the AVTP-level
//! `ntscf_data_length`/`stream_data_length` fields sit entirely outside
//! that region, in [`crate::avtp`]'s NTSCF/TSCF header, which this
//! function does not re-encode. [`build_crc32_coverage_buffer`] therefore
//! treats `acf_msg_length` as the field being pre-adjusted: the value it
//! encodes into the coverage buffer's ACF header is the caller-supplied
//! `ByteMessageInfo::acf_msg_length` plus one quadlet (since
//! rust-RCP-W01/W02, `acf_msg_length` is itself counted in quadlets, so
//! this is a raw `+1`, not `+4`).
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
//! ## CRC trailer wire placement: `pad` comes before the CRC, not after
//! (correctness fix, post-Milestone 9)
//!
//! TC18's own two worked examples (Figure 19 for ACF_ABB, Figure 20 for
//! ACF_GBB) show a CRC-protected message's real wire byte order as header
//! (+ `message_timestamp`), real payload, `pad` zero octets, THEN the
//! 4-byte CRC32 trailer. [`acf::encode_acf_abb`]/[`acf::encode_acf_gbb`]
//! have no CRC-trailer concept of their own and always append their own
//! automatically-derived `pad` after the entire `payload` blob they are
//! given — so a caller that concatenates `real_payload + crc_bytes` into
//! one blob before calling either encoder gets the reversed, non-conformant
//! `payload, CRC, pad` order instead. This module's own now-fixed
//! `acf::acf_abb_matches_figure_19_worked_example`/
//! `acf::acf_gbb_matches_figure_20_worked_example` golden vectors
//! previously did exactly that (their own byte positions went
//! unchecked, only totals/counts did, which is how the bug hid) before
//! being moved here as [`finalize_crc_trailer_matches_figure_19_worked_example`]/
//! [`finalize_crc_trailer_matches_figure_20_worked_example`], now pinning
//! the real byte sequence. [`finalize_crc_trailer`]/[`split_crc_trailer`]
//! are the correct, composable encode/decode primitives — see their own
//! doc comments and this module's "CRC trailer wire placement:
//! encode/decode" section below.
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
//! ordered `&[&[u8]]` rather than reading `acf::ReadSizeOrSegment`
//! itself to determine ordering. That is a deliberate, additional instance
//! of Guiding Principle 5: `acf::ReadSizeOrSegment`'s own provenance
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
//! fragment header-combination rule the roadmap text does not state.
//!
//! **Reconciled against TC18 §13.6 — and this working interpretation turned
//! out to be WRONG.** TC18 §13.6 (TC18.txt line 3801) states: "For
//! fragmented requests or responses going through CRC calculation only the
//! *first* AVTPDU and ACF header data will be used and the payload of all
//! segments." rust-RCP uses the *final* fragment's AVTPDU/ACF header
//! instead. This is a real, known divergence from TC18. It is recorded
//! honestly as requirement `REQ-CRC-015` rather than papered over, and is
//! deliberately left unfixed here: correcting it changes what
//! [`build_crc32_coverage_buffer_for_fragment_train`]/
//! [`crc32_tc18_for_fragment_train`]/
//! [`crate::fragment::verify_reassembled_train_crc`] take as their header
//! argument, so it belongs in its own change rather than in a
//! requirements-coverage pass. Interop with a conformant TC18 peer will
//! fail for any multi-fragment E-2-E-protected message whose first and last
//! fragments' AVTPDU/ACF header fields differ.

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
///
/// TC18 §13.6 Table 31 "CRC32 Polynomial" (TC18.txt line 3792) names this
/// CRC "CRC32P4" and fixes its six parameters: Polynomial `0xF4ACFB13`,
/// Width 32 bit, Initial Value `0xFFFFFFFF`, Final XOR `0xFFFFFFFF`, Input
/// reflection TRUE, Output reflection TRUE.
//fusa:req REQ-CRC-014
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
//fusa:req REQ-CRC-001
//fusa:req REQ-CRC-002
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

/// Number of quadlets the "length-field pre-adjustment" in `ROADMAP.md`
/// Milestone 6's "Coverage rule" bullet adds to a length field before the
/// CRC is computed: one quadlet. See this module's doc comment for which
/// length field [`build_crc32_coverage_buffer`] applies this to, and why —
/// this crate's own working interpretation, flagged per Guiding Principle
/// 5. Since `acf::ByteMessageInfo::acf_msg_length` is itself counted in
/// quadlets (rust-RCP-W01/W02, TC18 §11.2.1 Table 4), this pre-adjustment
/// is a raw `+1` to that field, not `+4` — before that reconciliation,
/// this module treated `acf_msg_length` as an opaque, undefined-unit
/// field and added `4` directly to it; that was already just as much a
/// working interpretation as this one, but the wrong one now that the
/// field's real unit is confirmed.
const CRC32_COVERAGE_LENGTH_PREADJUST_QUADLETS: u16 = 1;

/// Which of the two Milestone 1 ACF message shapes a
/// [`build_crc32_coverage_buffer`] call's "full ACF header" is drawn from.
///
/// Mirrors [`AcfAbbMessage`]/[`AcfGbbMessage`] rather than adding a new
/// decoded representation of either: this type only selects which one
/// applies, it does not reinterpret either message's fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcfCoverageMessage<'a> {
    /// An ACF_ABB message (no `message_timestamp`; `ACF_ABB_HEADER_LEN` ==
    /// 8-byte header before payload — `acf_msg_type` is folded into that
    /// header, not a separate leading byte, since rust-RCP-W01).
    Abb(&'a AcfAbbMessage),
    /// An ACF_GBB message (carries `message_timestamp`;
    /// `ACF_GBB_HEADER_LEN` == 16-byte header before payload).
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
/// - The full ACF header is `acf`'s `byte_message_info` (via
///   [`acf::encode_byte_message_info`], which folds the `acf_msg_type`
///   discriminant into that same 8-byte header — see
///   [`crate::acf`]'s "Canonical wire layout" doc section), and (for
///   [`AcfCoverageMessage::Gbb`] only) the 8-byte `message_timestamp`, with
///   the payload appended after. Before encoding, `acf`'s `byte_message_info.
///   acf_msg_length` is increased by [`CRC32_COVERAGE_LENGTH_PREADJUST_QUADLETS`]
///   — see this module's doc comment for why that field specifically. This
///   deliberately calls [`acf::encode_byte_message_info`] directly rather
///   than [`acf::encode_acf_abb`]/[`acf::encode_acf_gbb`]: those two derive
///   `acf_msg_length`/`pad` from the real payload length for an actual wire
///   frame and would simply discard this function's pre-adjusted value —
///   this buffer is a CRC-coverage scratch
///   construction that is never itself transmitted, so it has no reason to
///   go through that real-frame derivation.
///
/// Returns `Err(RcpError::InvalidSize)` if the pre-adjusted `acf_msg_length`
/// (or any other `ByteMessageInfo` field) fails
/// [`acf::encode_byte_message_info`]'s field-width validation.
///
/// Additive standalone plumbing, matching every prior Milestone 1-6 entry's
/// discipline: not called from [`crc32_tc18`] or a decoder/dispatch loop —
/// this function only assembles the buffer a caller would pass to
/// `crc32_tc18` and `crate::request`'s `CRC_ERROR` dispatch path.
/// Two further TC18 §13.6 properties fall out of this function's shape
/// rather than needing code of their own:
///
/// - **The CRC is ACF-specific** (TC18.txt line 3789: "the CRC32 is ACF
///   specific, which means it is calculated for multiple ACF types in one
///   AVTPDU for each ACF type individually"). This function takes exactly
///   one [`AcfCoverageMessage`], so an AVTPDU carrying N E-2-E-protected
///   ACF messages needs N independent calls and produces N independent
///   CRCs; there is no way to fold two ACF messages into one coverage
///   buffer.
/// - **Requests and responses use the identical scheme** (TC18.txt line
///   3808: "The CRC calculation for request and response follows the
///   identical scheme"). Neither this function nor [`crc32_tc18`] has a
///   direction parameter; a response differs from a request only by the
///   `rsp`/`err`/`evt` bits already inside the covered `byte_message_info`.
//fusa:req REQ-CRC-004
//fusa:req REQ-CRC-005
//fusa:req REQ-CRC-006
//fusa:req REQ-CRC-007
//fusa:req REQ-CRC-017
//fusa:req REQ-CRC-019
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

    // This deliberately encodes via `acf::encode_byte_message_info` (which
    // serializes whatever `acf_msg_length` it is given verbatim) rather
    // than `acf::encode_acf_abb`/`acf::encode_acf_gbb` (which, since
    // rust-RCP-N2-05, derive `acf_msg_length` from the real payload length
    // and reject a caller-supplied value that disagrees). This buffer is a
    // CRC-coverage scratch construction, not a real wire frame — the
    // pre-adjusted length below is *never* actually transmitted on the
    // wire, so it must bypass that real-frame derivation/validation
    // entirely rather than trying to satisfy it.
    match acf {
        AcfCoverageMessage::Abb(msg) => {
            let mut adjusted_info = msg.info;
            adjusted_info.acf_msg_type = acf::ACF_ABB_MSG_TYPE;
            adjusted_info.acf_msg_length = adjusted_info
                .acf_msg_length
                .saturating_add(CRC32_COVERAGE_LENGTH_PREADJUST_QUADLETS);
            let info_bytes = acf::encode_byte_message_info(&adjusted_info)?;
            buf.extend_from_slice(&info_bytes);
            buf.extend_from_slice(&msg.payload);
        }
        AcfCoverageMessage::Gbb(msg) => {
            let mut adjusted_info = msg.info;
            adjusted_info.acf_msg_type = acf::ACF_GBB_MSG_TYPE;
            adjusted_info.acf_msg_length = adjusted_info
                .acf_msg_length
                .saturating_add(CRC32_COVERAGE_LENGTH_PREADJUST_QUADLETS);
            let info_bytes = acf::encode_byte_message_info(&adjusted_info)?;
            buf.extend_from_slice(&info_bytes);
            buf.extend_from_slice(&msg.message_timestamp.to_be_bytes());
            buf.extend_from_slice(&msg.payload);
        }
    }

    Ok(buf)
}

// ── CRC trailer wire placement: encode/decode ───────────────────────────────
//
// TC18's own two worked examples (Figure 19 for ACF_ABB, Figure 20 for
// ACF_GBB) show a CRC-protected ACF message's real wire byte order as:
// header (+ `message_timestamp` for ACF_GBB), real payload, `pad` zero
// octets (rounding header+payload up to a whole quadlet), THEN the 4-byte
// CRC32 trailer — pad comes *before* the CRC, not after. `acf::encode_acf_abb`/
// `acf::encode_acf_gbb` have no CRC-trailer concept of their own (see
// `acf`'s "acf_msg_length quadlet semantics" note) and always append their
// own automatically-derived `pad` after the *entire* `payload` they are
// given; a caller that concatenates `real_payload + crc_bytes` into one
// blob before calling either encoder therefore gets `pad` appended after
// the CRC instead of before it — the wrong wire order. [`finalize_crc_trailer`]/
// [`split_crc_trailer`] are the correct, composable way to get TC18's real
// order out of those two unmodified encoders: call `encode_acf_abb`/
// `encode_acf_gbb` with the real payload alone (so its own automatic `pad`
// already lands immediately after the real payload, before this section's
// functions ever run), then use these two functions to bump/un-bump
// `acf_msg_length` by the trailer's one quadlet and append/strip the CRC
// bytes themselves — matching the two-step "encode the pre-CRC frame, then
// append the trailer" pattern `cpp-RCP`'s `rcp::e2e::append_crc` and
// `c-RCP`'s equivalent already use.

/// Number of quadlets (equivalently, [`QUADLET_LEN`]-byte octets) TC18's
/// trailing CRC32 always occupies on the wire — exactly one quadlet, never
/// more or fewer. Shared with [`CRC32_COVERAGE_LENGTH_PREADJUST_QUADLETS`]
/// (the same one-quadlet bump [`build_crc32_coverage_buffer`] applies to
/// its own scratch coverage header) rather than a second, independently
/// named constant for the same value.
pub const CRC_TRAILER_QUADLETS: u16 = CRC32_COVERAGE_LENGTH_PREADJUST_QUADLETS;

/// Number of octets TC18's trailing CRC32 always occupies on the wire —
/// one quadlet ([`QUADLET_LEN`]).
pub const CRC_TRAILER_LEN: usize = acf::QUADLET_LEN;

/// Turns an already-encoded, CRC-free ACF_ABB/ACF_GBB `frame` (as returned
/// by [`acf::encode_acf_abb`]/[`acf::encode_acf_gbb`] called with the
/// message's real payload only — no CRC bytes mixed in) into its
/// CRC-protected wire form: bumps `frame`'s own `byte_message_info.
/// acf_msg_length` by [`CRC_TRAILER_QUADLETS`] (the one quadlet the
/// about-to-be-appended trailer adds to the message's total length) and
/// appends `crc`'s 4 big-endian octets.
///
/// Because `frame` was built from the real payload alone, the encoder's
/// own automatic `pad` octets already sit immediately after the real
/// payload; this function only ever appends bytes after that point, so the
/// result is always header (+ `message_timestamp`), real payload, `pad`
/// zero octets, CRC — TC18's real order (Figure 19 / Figure 20) — never
/// the reversed payload/CRC/pad order a caller gets by concatenating CRC
/// bytes into the payload before calling `encode_acf_abb`/`encode_acf_gbb`
/// (see this section's doc comment).
///
/// Returns `Err(RcpError::ShortFrame)` if `frame` is shorter than
/// [`acf::BYTE_MESSAGE_INFO_LEN`], and `Err(RcpError::InvalidSize)` if
/// bumping `acf_msg_length` would overflow its 9-bit field width.
//fusa:req REQ-CRC-012
pub fn finalize_crc_trailer(frame: &mut Vec<u8>, crc: u32) -> Result<(), RcpError> {
    if frame.len() < acf::BYTE_MESSAGE_INFO_LEN {
        return Err(RcpError::ShortFrame);
    }
    let mut info = acf::decode_byte_message_info(&frame[..acf::BYTE_MESSAGE_INFO_LEN])?;
    info.acf_msg_length = info
        .acf_msg_length
        .checked_add(CRC_TRAILER_QUADLETS)
        .filter(|&q| q <= acf::ACF_MSG_LENGTH_9BIT_MAX)
        .ok_or(RcpError::InvalidSize)?;
    let new_header = acf::encode_byte_message_info(&info)?;
    frame[..acf::BYTE_MESSAGE_INFO_LEN].copy_from_slice(&new_header);
    frame.extend_from_slice(&crc.to_be_bytes());
    Ok(())
}

/// The mirror-image decode-side operation: splits a CRC-protected ACF
/// message (as [`finalize_crc_trailer`] produces) into `(body, crc)`,
/// where `body` is byte-for-byte what [`acf::decode_acf_abb`]/
/// [`acf::decode_acf_gbb`] already know how to parse on their own — header
/// (+ `message_timestamp`), real payload, native `pad` octets, with no CRC
/// trailer involved at all — and `crc` is the real, wire-carried CRC32
/// value for the caller to check (e.g. via
/// [`crate::request::check_rx_enforce_e2e`]).
///
/// This exists because [`acf::decode_acf_abb`]/[`acf::decode_acf_gbb`]
/// strip exactly `byte_message_info.pad` octets *from the end of the
/// region `acf_msg_length` describes* — which, for a message whose
/// `acf_msg_length` still counts the trailing CRC quadlet, would strip the
/// CRC's own trailing bytes as if they were `pad` instead of the real
/// `pad` octets that actually precede the CRC on the wire. Un-adjusting
/// `acf_msg_length` by the same [`CRC_TRAILER_QUADLETS`]
/// [`finalize_crc_trailer`] added, and removing the CRC's own 4 octets
/// from the byte slice first, makes `body` a message
/// [`acf::decode_acf_abb`]/[`acf::decode_acf_gbb`]'s existing `pad`-stripping
/// logic parses correctly without any change to either decoder.
///
/// Returns `Err(RcpError::ShortFrame)` if `frame` is shorter than a header
/// plus a full CRC trailer ([`acf::BYTE_MESSAGE_INFO_LEN`] +
/// [`CRC_TRAILER_LEN`]), and `Err(RcpError::InvalidSize)` if the header's
/// `acf_msg_length` is smaller than [`CRC_TRAILER_QUADLETS`] (i.e. does not
/// actually describe a message with room for a CRC trailer at all).
//fusa:req REQ-CRC-013
pub fn split_crc_trailer(frame: &[u8]) -> Result<(Vec<u8>, u32), RcpError> {
    if frame.len() < acf::BYTE_MESSAGE_INFO_LEN + CRC_TRAILER_LEN {
        return Err(RcpError::ShortFrame);
    }
    let mut info = acf::decode_byte_message_info(&frame[..acf::BYTE_MESSAGE_INFO_LEN])?;
    info.acf_msg_length = info
        .acf_msg_length
        .checked_sub(CRC_TRAILER_QUADLETS)
        .ok_or(RcpError::InvalidSize)?;
    let new_header = acf::encode_byte_message_info(&info)?;

    let crc_start = frame.len() - CRC_TRAILER_LEN;
    let mut crc_bytes = [0u8; 4];
    crc_bytes.copy_from_slice(&frame[crc_start..]);
    let crc = u32::from_be_bytes(crc_bytes);

    let mut body = frame[..crc_start].to_vec();
    body[..acf::BYTE_MESSAGE_INFO_LEN].copy_from_slice(&new_header);
    Ok((body, crc))
}

// ── Fragmentation interaction ────────────────────────────────────────────────

/// The combined payload of a multi-segment "fragment train", assembled by
/// concatenating each fragment's own payload in the order the caller
/// supplies them.
///
/// This crate has no live multi-AVTPDU reassembly buffer to read segment
/// order from yet (`ROADMAP.md` Milestone 8), so segment order is a
/// caller-supplied fact rather than something derived from
/// `acf::ReadSizeOrSegment::as_segment_num` here — see this module's
/// doc comment "Fragmentation interaction" section for why.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CombinedFragmentPayload(pub Vec<u8>);

impl CombinedFragmentPayload {
    /// Assembles a fragment train's combined payload by concatenating
    /// `segments` verbatim, in the order given. An empty `segments` slice
    /// yields an empty combined payload; this function never panics for
    /// any input, including empty per-segment payloads.
    //fusa:req REQ-CRC-009
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
//fusa:req REQ-CRC-010
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
//fusa:req REQ-CRC-010
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
//fusa:req REQ-CRC-008
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
//fusa:req REQ-CRC-008
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
    //fusa:test REQ-CRC-001
    fn crc32_tc18_empty_input_matches_reference() {
        assert_eq!(crc32_tc18(&[]), crc32_tc18_reference(&[]));
        assert_eq!(crc32_tc18(&[]), 0x0000_0000);
    }

    #[test]
    //fusa:test REQ-CRC-001
    fn crc32_tc18_ascii_check_string_matches_reference() {
        // "123456789" is the conventional CRC-32 check corpus; the expected
        // constant below is this polynomial's own derived value (see this
        // module's provenance note), not an externally published one.
        let data = b"123456789";
        assert_eq!(crc32_tc18(data), crc32_tc18_reference(data));
        assert_eq!(crc32_tc18(data), 0x1697_d06a);
    }

    #[test]
    //fusa:test REQ-CRC-001
    fn crc32_tc18_all_zero_boundary_matches_reference() {
        let data = [0u8; 16];
        assert_eq!(crc32_tc18(&data), crc32_tc18_reference(&data));
        assert_eq!(crc32_tc18(&data), 0x0fa6_214b);
    }

    #[test]
    //fusa:test REQ-CRC-001
    fn crc32_tc18_all_0xff_boundary_matches_reference() {
        let data = [0xFFu8; 16];
        assert_eq!(crc32_tc18(&data), crc32_tc18_reference(&data));
        assert_eq!(crc32_tc18(&data), 0xb0f2_7ef5);
    }

    #[test]
    //fusa:test REQ-CRC-001
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
    //fusa:test REQ-CRC-002
    fn crc32_tc18_never_panics_across_arbitrary_lengths() {
        for len in [0usize, 1, 2, 3, 6, 17, 64, 257, 1000] {
            let data: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
            let _ = crc32_tc18(&data);
        }
    }

    #[test]
    //fusa:test REQ-CRC-003
    fn crc32_tc18_different_payload_produces_different_crc() {
        let a = crc32_tc18(b"payload-a");
        let b = crc32_tc18(b"payload-b");
        assert_ne!(a, b);
    }

    #[test]
    //fusa:test REQ-CRC-003
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

    // ── TC18 §13.6 Table 31 / ACF-specific / request-response symmetry ─────

    #[test]
    //fusa:test REQ-CRC-014
    fn poly_constant_is_the_bit_reversal_of_table_31_polynomial() {
        // TC18 §13.6 Table 31 "CRC32 Polynomial" (TC18.txt line 3792), read
        // row by row:
        //   Polynomial        0xF4ACFB13
        //   Width             32 bit
        //   Initial Value     0xFFFFFFFF
        //   Final XOR         0xFFFFFFFF
        //   Input reflection  TRUE
        //   Output reflection TRUE
        //
        // `crc32_tc18` runs a reflected (LSB-first) shift register, which
        // is only equivalent to that definition if the constant it shifts
        // against is the exact bit reversal of Table 31's polynomial.
        const TABLE_31_POLYNOMIAL: u32 = 0xF4AC_FB13;
        const TABLE_31_INITIAL_VALUE: u32 = 0xFFFF_FFFF;
        const TABLE_31_FINAL_XOR: u32 = 0xFFFF_FFFF;
        const TABLE_31_WIDTH_BITS: u32 = 32;

        assert_eq!(
            CRC32_TC18_POLY_REFLECTED,
            TABLE_31_POLYNOMIAL.reverse_bits(),
            "the reflected engine must shift against bitrev32(0xF4ACFB13)"
        );
        assert_eq!(CRC32_TC18_INIT_XOROUT, TABLE_31_INITIAL_VALUE);
        assert_eq!(CRC32_TC18_INIT_XOROUT, TABLE_31_FINAL_XOR);
        assert_eq!(u32::BITS, TABLE_31_WIDTH_BITS);
    }

    #[test]
    //fusa:test REQ-CRC-017
    fn crc32_is_computed_per_acf_type_individually() {
        // TC18 §13.6 (TC18.txt line 3789): "the CRC32 is ACF specific,
        // which means it is calculated for multiple ACF types in one AVTPDU
        // for each ACF type individually."
        //
        // Two ACF messages riding under one and the same AVTPDU header must
        // therefore yield two independent coverage buffers and two
        // independent CRCs — neither buffer may contain the other message's
        // payload bytes.
        let header = HeaderVariant::Ntscf(avtp::NtscfHeader {
            stream_id: 0x0011_2233_4455_6677,
            ..Default::default()
        });
        let first = sample_abb_message(3, b"first-acf");
        let second = sample_gbb_message(5, 0x1234, b"second-acf");

        let buf_first = build_crc32_coverage_buffer(&header, &AcfCoverageMessage::Abb(&first))
            .expect("first ACF message covers on its own");
        let buf_second = build_crc32_coverage_buffer(&header, &AcfCoverageMessage::Gbb(&second))
            .expect("second ACF message covers on its own");

        assert!(
            !buf_first.windows(10).any(|w| w == b"second-acf"),
            "the first ACF type's coverage must not include the second's payload"
        );
        assert!(
            !buf_second.windows(9).any(|w| w == b"first-acf"),
            "the second ACF type's coverage must not include the first's payload"
        );
        assert_ne!(
            crc32_tc18(&buf_first),
            crc32_tc18(&buf_second),
            "two ACF types in one AVTPDU get two distinct CRC32 values"
        );
    }

    #[test]
    //fusa:test REQ-CRC-019
    fn crc_scheme_is_identical_for_request_and_response() {
        // TC18 §13.6 (TC18.txt line 3808): "The CRC calculation for request
        // and response follows the identical scheme."
        //
        // TC18 §11.2.1 Table 4 gives a request rsp = 0b and §11.3 Table 15
        // gives a response rsp = 1b; that single covered header bit is the
        // *only* thing that may differ between the two directions' coverage
        // buffers. In particular, neither build_crc32_coverage_buffer nor
        // crc32_tc18 takes a direction argument.
        let header = HeaderVariant::Ntscf(avtp::NtscfHeader {
            stream_id: 0x0102_0304_0506_0708,
            ..Default::default()
        });
        let payload = b"same-bytes-both-ways";

        let mut request = sample_abb_message(4, payload);
        request.info.rsp = false;
        let mut response = request.clone();
        response.info.rsp = true;

        let buf_request =
            build_crc32_coverage_buffer(&header, &AcfCoverageMessage::Abb(&request)).unwrap();
        let buf_response =
            build_crc32_coverage_buffer(&header, &AcfCoverageMessage::Abb(&response)).unwrap();

        assert_eq!(
            buf_request.len(),
            buf_response.len(),
            "identical scheme: same coverage extent in both directions"
        );
        // rsp is octet 6 bit 6 of byte_message_info, which starts 12 bytes
        // into the coverage buffer (8-byte stream_id + 4-byte
        // avtp_timestamp position) — so index 12 + 6 == 18.
        let differing: Vec<usize> = (0..buf_request.len())
            .filter(|&i| buf_request[i] != buf_response[i])
            .collect();
        assert_eq!(
            differing,
            vec![18],
            "only the rsp bit's own octet may differ between the two directions"
        );
        assert_eq!(buf_request[18] ^ buf_response[18], 0x40, "rsp is bit 6");

        // And with rsp forced equal, the two directions produce byte-for-byte
        // identical coverage and therefore an identical CRC.
        response.info.rsp = false;
        let buf_response_same =
            build_crc32_coverage_buffer(&header, &AcfCoverageMessage::Abb(&response)).unwrap();
        assert_eq!(buf_request, buf_response_same);
        assert_eq!(crc32_tc18(&buf_request), crc32_tc18(&buf_response_same));
    }

    #[test]
    //fusa:test REQ-CRC-004
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
    //fusa:test REQ-CRC-005
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
    //fusa:test REQ-CRC-005
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
    //fusa:test REQ-CRC-007
    fn coverage_buffer_abb_header_has_no_message_timestamp_region() {
        let header = HeaderVariant::Ntscf(avtp::NtscfHeader::default());
        let payload = b"abb-payload";
        let acf = AcfCoverageMessage::Abb(&sample_abb_message(0, payload));
        let buf = build_crc32_coverage_buffer(&header, &acf).unwrap();
        // 8 (stream_id) + 4 (avtp_timestamp) + ACF_ABB_HEADER_LEN + payload.
        assert_eq!(buf.len(), 12 + acf::ACF_ABB_HEADER_LEN + payload.len());
        // `acf_msg_type` is folded into byte_message_info's first octet
        // (rust-RCP-W01), not a separate leading byte — decode it back out
        // rather than comparing a raw byte.
        let decoded_info = acf::decode_byte_message_info(&buf[12..]).unwrap();
        assert_eq!(decoded_info.acf_msg_type, acf::ACF_ABB_MSG_TYPE);
        assert_eq!(&buf[buf.len() - payload.len()..], payload);
    }

    #[test]
    //fusa:test REQ-CRC-007
    fn coverage_buffer_gbb_header_carries_message_timestamp() {
        let header = HeaderVariant::Ntscf(avtp::NtscfHeader::default());
        let payload = b"gbb-payload";
        let msg = sample_gbb_message(0, 0x0011_2233_4455_6677, payload);
        let acf = AcfCoverageMessage::Gbb(&msg);
        let buf = build_crc32_coverage_buffer(&header, &acf).unwrap();
        // 8 (stream_id) + 4 (avtp_timestamp) + ACF_GBB_HEADER_LEN + payload.
        assert_eq!(buf.len(), 12 + acf::ACF_GBB_HEADER_LEN + payload.len());
        // `acf_msg_type` is folded into byte_message_info's first octet
        // (rust-RCP-W01), not a separate leading byte — decode it back out
        // rather than comparing a raw byte.
        let decoded_info = acf::decode_byte_message_info(&buf[12..]).unwrap();
        assert_eq!(decoded_info.acf_msg_type, acf::ACF_GBB_MSG_TYPE);
        // message_timestamp occupies the 8 bytes just before the payload
        // (ACF_GBB_HEADER_LEN already accounts for byte_message_info's
        // width, so it starts right after that).
        let ts_start = 12 + acf::BYTE_MESSAGE_INFO_LEN;
        let ts_end = ts_start + 8;
        assert_eq!(
            &buf[ts_start..ts_end],
            &0x0011_2233_4455_6677u64.to_be_bytes()
        );
        assert_eq!(&buf[buf.len() - payload.len()..], payload);
    }

    #[test]
    //fusa:test REQ-CRC-006
    fn coverage_buffer_preadjusts_acf_msg_length_by_one_quadlet() {
        let header = HeaderVariant::Ntscf(avtp::NtscfHeader::default());
        let acf_msg_length = 0x0100u16;
        let acf = AcfCoverageMessage::Abb(&sample_abb_message(acf_msg_length, b"x"));
        let buf = build_crc32_coverage_buffer(&header, &acf).unwrap();
        // The ACF header region starts at byte 12 (after stream_id +
        // avtp_timestamp); byte_message_info starts immediately there —
        // `acf_msg_type` is folded into it, not a separate leading byte
        // (rust-RCP-W01).
        let info_start = 12;
        let decoded = acf::decode_byte_message_info(
            &buf[info_start..info_start + acf::BYTE_MESSAGE_INFO_LEN],
        )
        .unwrap();
        assert_eq!(
            decoded.acf_msg_length,
            acf_msg_length + CRC32_COVERAGE_LENGTH_PREADJUST_QUADLETS
        );
    }

    #[test]
    //fusa:test REQ-CRC-006
    fn coverage_buffer_rejects_length_that_overflows_9_bits_after_preadjustment() {
        let header = HeaderVariant::Ntscf(avtp::NtscfHeader::default());
        // Max legal 9-bit acf_msg_length; +1 pushes it past the field
        // width, so encoding must fail the same way
        // `acf::encode_byte_message_info` itself would.
        let acf = AcfCoverageMessage::Abb(&sample_abb_message(acf::ACF_MSG_LENGTH_9BIT_MAX, b"x"));
        assert_eq!(
            build_crc32_coverage_buffer(&header, &acf),
            Err(RcpError::InvalidSize)
        );
    }

    #[test]
    //fusa:test REQ-CRC-004
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
    //fusa:test REQ-CRC-004
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

    // ── CRC trailer wire placement: encode/decode ────────────────────────────
    //
    // Golden vectors: TC18 Figure 19 (ACF_ABB) / Figure 20 (ACF_GBB) worked
    // examples, moved here from `acf.rs` (see this module's "CRC trailer
    // wire placement" doc section) and strengthened to pin the ACTUAL byte
    // sequence — not just total length/quadlet-count/pad-count, which
    // stayed correct even under the old, reversed `payload, CRC, pad` byte
    // order and so never caught that bug.

    #[test]
    //fusa:test REQ-CRC-012
    //fusa:test REQ-CRC-013
    fn finalize_crc_trailer_matches_figure_19_worked_example() {
        // Figure 19: ACF_ABB, 8-byte header + 6 real payload bytes + 2 pad
        // bytes + 4-byte CRC32 trailer = 20 bytes total = 5 quadlets, wire
        // order header, payload, pad, THEN CRC.
        let real_payload: [u8; 6] = [0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6];
        let msg = AcfAbbMessage {
            info: acf::ByteMessageInfo::default(),
            payload: real_payload.to_vec(),
        };

        // encode_acf_abb, called with the real payload alone, already gets
        // the pre-CRC part exactly right: its own automatic pad lands
        // right after the real payload.
        let mut frame = acf::encode_acf_abb(&msg).unwrap();
        assert_eq!(frame.len(), 16, "header(8) + payload(6) + native pad(2)");
        let pre_crc_info =
            acf::decode_byte_message_info(&frame[..acf::BYTE_MESSAGE_INFO_LEN]).unwrap();
        assert_eq!(
            pre_crc_info.acf_msg_length, 4,
            "base length before the CRC trailer's own quadlet is counted"
        );
        assert_eq!(
            pre_crc_info.pad, 2,
            "Figure 19: 2 pad bytes, derived from the real payload alone"
        );
        assert_eq!(
            &frame[8..14],
            &real_payload,
            "payload right after the header"
        );
        assert_eq!(
            &frame[14..16],
            &[0x00, 0x00],
            "native pad right after the payload"
        );

        let header = HeaderVariant::Ntscf(avtp::NtscfHeader::default());
        let coverage_msg = AcfAbbMessage {
            info: pre_crc_info,
            payload: real_payload.to_vec(),
        };
        let coverage =
            build_crc32_coverage_buffer(&header, &AcfCoverageMessage::Abb(&coverage_msg)).unwrap();
        let crc = crc32_tc18(&coverage);

        finalize_crc_trailer(&mut frame, crc).unwrap();
        assert_eq!(frame.len(), 20, "Figure 19: total message is 20 bytes");

        let final_info =
            acf::decode_byte_message_info(&frame[..acf::BYTE_MESSAGE_INFO_LEN]).unwrap();
        assert_eq!(
            final_info.acf_msg_length, 0x05,
            "Figure 19: acf_msg_length must be 5 quadlets"
        );
        assert_eq!(final_info.acf_msg_type, acf::ACF_ABB_MSG_TYPE);
        assert_eq!(
            final_info.pad, 2,
            "Figure 19: 2 pad bytes — unchanged by finalize_crc_trailer"
        );

        // The exact TC18 wire byte sequence: header(8), payload(6), pad(2),
        // CRC(4) — pad strictly BEFORE the CRC, not after it.
        assert_eq!(
            &frame[0..8],
            &acf::encode_byte_message_info(&final_info).unwrap()
        );
        assert_eq!(
            &frame[8..14],
            &real_payload,
            "payload comes right after the header"
        );
        assert_eq!(
            &frame[14..16],
            &[0x00, 0x00],
            "pad comes right after the payload"
        );
        assert_eq!(
            &frame[16..20],
            &crc.to_be_bytes(),
            "CRC comes last, after pad — not before it"
        );

        // Decode-side mirror: split_crc_trailer + acf::decode_acf_abb
        // recover the real payload (pad already stripped by the acf
        // layer's own existing logic) and the same CRC value.
        let (body, decoded_crc) = split_crc_trailer(&frame).unwrap();
        assert_eq!(decoded_crc, crc);
        let decoded = acf::decode_acf_abb(&body).unwrap();
        assert_eq!(decoded.payload, real_payload.to_vec());
        assert_eq!(decoded.info.pad, 2);
    }

    #[test]
    //fusa:test REQ-CRC-012
    //fusa:test REQ-CRC-013
    fn finalize_crc_trailer_matches_figure_20_worked_example() {
        // Figure 20: ACF_GBB, 8-byte header + 8-byte timestamp + 7 real
        // payload bytes + 1 pad byte + 4-byte CRC32 trailer = 28 bytes
        // total = 7 quadlets, wire order header, timestamp, payload, pad,
        // THEN CRC.
        let real_payload: [u8; 7] = [0xB1, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7];
        let message_timestamp: u64 = 0x0102_0304_0506_0708;
        let msg = AcfGbbMessage {
            info: acf::ByteMessageInfo::default(),
            message_timestamp,
            payload: real_payload.to_vec(),
        };

        let mut frame = acf::encode_acf_gbb(&msg).unwrap();
        assert_eq!(
            frame.len(),
            24,
            "header(8) + timestamp(8) + payload(7) + native pad(1)"
        );
        let pre_crc_info =
            acf::decode_byte_message_info(&frame[..acf::BYTE_MESSAGE_INFO_LEN]).unwrap();
        assert_eq!(
            pre_crc_info.acf_msg_length, 6,
            "base length before the CRC trailer's own quadlet is counted"
        );
        assert_eq!(
            pre_crc_info.pad, 1,
            "Figure 20: 1 pad byte, derived from the real payload alone"
        );
        let ts_start = acf::BYTE_MESSAGE_INFO_LEN;
        assert_eq!(
            &frame[ts_start..ts_start + 8],
            &message_timestamp.to_be_bytes(),
            "timestamp right after the header"
        );
        assert_eq!(
            &frame[16..23],
            &real_payload,
            "payload right after the timestamp"
        );
        assert_eq!(
            &frame[23..24],
            &[0x00],
            "native pad right after the payload"
        );

        let header = HeaderVariant::Ntscf(avtp::NtscfHeader::default());
        let coverage_msg = AcfGbbMessage {
            info: pre_crc_info,
            message_timestamp,
            payload: real_payload.to_vec(),
        };
        let coverage =
            build_crc32_coverage_buffer(&header, &AcfCoverageMessage::Gbb(&coverage_msg)).unwrap();
        let crc = crc32_tc18(&coverage);

        finalize_crc_trailer(&mut frame, crc).unwrap();
        assert_eq!(frame.len(), 28, "Figure 20: total message is 28 bytes");

        let final_info =
            acf::decode_byte_message_info(&frame[..acf::BYTE_MESSAGE_INFO_LEN]).unwrap();
        assert_eq!(
            final_info.acf_msg_length, 0x07,
            "Figure 20: acf_msg_length must be 7 quadlets"
        );
        assert_eq!(final_info.acf_msg_type, acf::ACF_GBB_MSG_TYPE);
        assert_eq!(
            final_info.pad, 1,
            "Figure 20: 1 pad byte — unchanged by finalize_crc_trailer"
        );

        // The exact TC18 wire byte sequence: header(8), timestamp(8),
        // payload(7), pad(1), CRC(4) — pad strictly BEFORE the CRC.
        assert_eq!(
            &frame[ts_start..ts_start + 8],
            &message_timestamp.to_be_bytes()
        );
        assert_eq!(
            &frame[16..23],
            &real_payload,
            "payload comes right after the timestamp"
        );
        assert_eq!(&frame[23..24], &[0x00], "pad comes right after the payload");
        assert_eq!(
            &frame[24..28],
            &crc.to_be_bytes(),
            "CRC comes last, after pad — not before it"
        );

        let (body, decoded_crc) = split_crc_trailer(&frame).unwrap();
        assert_eq!(decoded_crc, crc);
        let decoded = acf::decode_acf_gbb(&body).unwrap();
        assert_eq!(decoded.payload, real_payload.to_vec());
        assert_eq!(decoded.message_timestamp, message_timestamp);
        assert_eq!(decoded.info.pad, 1);
    }

    #[test]
    //fusa:test REQ-CRC-012
    fn finalize_crc_trailer_never_places_pad_after_crc() {
        // Regression guard for the padding-order bug this module's own
        // "CRC trailer wire placement" doc section describes: naively
        // concatenating `real_payload + crc_bytes` into one blob and
        // handing THAT to `encode_acf_abb` (the old, buggy pattern) puts
        // the encoder's automatically-derived pad after the CRC instead of
        // before it. finalize_crc_trailer must never reproduce that order.
        let real_payload: [u8; 6] = [1, 2, 3, 4, 5, 6];
        let crc: u32 = 0x1122_3344;

        // The old, buggy construction this fix replaces.
        let buggy_payload: Vec<u8> = real_payload
            .iter()
            .copied()
            .chain(crc.to_be_bytes())
            .collect();
        let buggy_msg = AcfAbbMessage {
            info: acf::ByteMessageInfo::default(),
            payload: buggy_payload,
        };
        let buggy_frame = acf::encode_acf_abb(&buggy_msg).unwrap();
        // Buggy order: ...payload(6), CRC(4), pad(2) — CRC bytes land at
        // [14..18], pad lands at [18..20], both wrong relative to Figure 19.
        assert_eq!(&buggy_frame[14..18], &crc.to_be_bytes());
        assert_eq!(&buggy_frame[18..20], &[0x00, 0x00]);

        // The fixed construction.
        let msg = AcfAbbMessage {
            info: acf::ByteMessageInfo::default(),
            payload: real_payload.to_vec(),
        };
        let mut frame = acf::encode_acf_abb(&msg).unwrap();
        finalize_crc_trailer(&mut frame, crc).unwrap();
        // Correct order: pad(2) at [14..16], THEN CRC(4) at [16..20].
        assert_eq!(&frame[14..16], &[0x00, 0x00]);
        assert_eq!(&frame[16..20], &crc.to_be_bytes());
        assert_ne!(
            frame, buggy_frame,
            "the fixed byte order must differ from the old, buggy one"
        );
    }

    #[test]
    fn split_crc_trailer_rejects_short_frame() {
        let short = vec![0u8; acf::BYTE_MESSAGE_INFO_LEN + CRC_TRAILER_LEN - 1];
        assert_eq!(split_crc_trailer(&short), Err(RcpError::ShortFrame));
    }

    #[test]
    fn split_crc_trailer_rejects_length_without_room_for_crc_trailer() {
        // acf_msg_length describes a message shorter than the header
        // itself once the CRC trailer's quadlet is un-adjusted — must be
        // rejected, not silently underflow.
        let info = acf::ByteMessageInfo {
            acf_msg_type: acf::ACF_ABB_MSG_TYPE,
            acf_msg_length: 0, // < CRC_TRAILER_QUADLETS
            ..Default::default()
        };
        let header = acf::encode_byte_message_info(&info).unwrap();
        let mut frame = header.to_vec();
        frame.extend_from_slice(&[0u8; CRC_TRAILER_LEN]);
        assert_eq!(split_crc_trailer(&frame), Err(RcpError::InvalidSize));
    }

    #[test]
    fn finalize_crc_trailer_rejects_length_overflow() {
        let info = acf::ByteMessageInfo {
            acf_msg_type: acf::ACF_ABB_MSG_TYPE,
            acf_msg_length: acf::ACF_MSG_LENGTH_9BIT_MAX,
            ..Default::default()
        };
        let header = acf::encode_byte_message_info(&info).unwrap();
        let mut frame = header.to_vec();
        assert_eq!(
            finalize_crc_trailer(&mut frame, 0),
            Err(RcpError::InvalidSize)
        );
    }

    #[test]
    fn finalize_crc_trailer_rejects_short_frame() {
        let mut short = vec![0u8; acf::BYTE_MESSAGE_INFO_LEN - 1];
        assert_eq!(
            finalize_crc_trailer(&mut short, 0),
            Err(RcpError::ShortFrame)
        );
    }

    // ── Fragmentation interaction ────────────────────────────────────────────

    #[test]
    //fusa:test REQ-CRC-009
    fn combined_fragment_payload_concatenates_in_given_order() {
        let segments: [&[u8]; 3] = [b"ab", b"cd", b"ef"];
        let combined = CombinedFragmentPayload::assemble(&segments);
        assert_eq!(combined.0, b"abcdef".to_vec());
    }

    #[test]
    //fusa:test REQ-CRC-009
    fn combined_fragment_payload_empty_segments_yields_empty() {
        let segments: [&[u8]; 0] = [];
        let combined = CombinedFragmentPayload::assemble(&segments);
        assert!(combined.0.is_empty());
    }

    #[test]
    //fusa:test REQ-CRC-009
    fn combined_fragment_payload_single_segment_matches_it_verbatim() {
        let segments: [&[u8]; 1] = [b"solo"];
        let combined = CombinedFragmentPayload::assemble(&segments);
        assert_eq!(combined.0, b"solo".to_vec());
    }

    #[test]
    //fusa:test REQ-CRC-008
    fn fragment_crc_expectation_not_expected_when_more_segments_follow() {
        assert_eq!(
            fragment_crc_expectation(true),
            FragmentCrcExpectation::NotExpected
        );
    }

    #[test]
    //fusa:test REQ-CRC-008
    fn fragment_crc_expectation_expected_on_final_fragment() {
        assert_eq!(
            fragment_crc_expectation(false),
            FragmentCrcExpectation::Expected
        );
    }

    #[test]
    //fusa:test REQ-CRC-008
    fn check_fragment_crc_placement_accepts_no_crc_on_intermediate_fragment() {
        assert_eq!(check_fragment_crc_placement(true, false), Ok(()));
    }

    #[test]
    //fusa:test REQ-CRC-008
    fn check_fragment_crc_placement_accepts_crc_on_final_fragment() {
        assert_eq!(check_fragment_crc_placement(false, true), Ok(()));
    }

    #[test]
    //fusa:test REQ-CRC-008
    fn check_fragment_crc_placement_rejects_crc_on_intermediate_fragment() {
        assert_eq!(
            check_fragment_crc_placement(true, true),
            Err(RcpError::InvalidParameter)
        );
    }

    #[test]
    //fusa:test REQ-CRC-008
    fn check_fragment_crc_placement_rejects_missing_crc_on_final_fragment() {
        assert_eq!(
            check_fragment_crc_placement(false, false),
            Err(RcpError::InvalidParameter)
        );
    }

    #[test]
    //fusa:test REQ-CRC-010
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
    //fusa:test REQ-CRC-010
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
    //fusa:test REQ-CRC-010
    fn coverage_buffer_for_fragment_train_gbb_carries_final_fragment_timestamp() {
        let header = HeaderVariant::Ntscf(avtp::NtscfHeader::default());
        let final_fragment_msg = sample_gbb_message(0, 0x1122_3344_5566_7788, b"unused");
        let final_fragment = AcfCoverageMessage::Gbb(&final_fragment_msg);
        let segments: [&[u8]; 2] = [b"part-a", b"part-b"];

        let buf =
            build_crc32_coverage_buffer_for_fragment_train(&header, &final_fragment, &segments)
                .unwrap();

        // `acf_msg_type` is folded into byte_message_info's first octet
        // (rust-RCP-W01), not a separate leading byte.
        let decoded_info = acf::decode_byte_message_info(&buf[12..]).unwrap();
        assert_eq!(decoded_info.acf_msg_type, acf::ACF_GBB_MSG_TYPE);
        let ts_start = 12 + acf::BYTE_MESSAGE_INFO_LEN;
        let ts_end = ts_start + 8;
        assert_eq!(
            &buf[ts_start..ts_end],
            &0x1122_3344_5566_7788u64.to_be_bytes()
        );
        assert_eq!(&buf[buf.len() - 12..], b"part-apart-b");
    }

    #[test]
    //fusa:test REQ-CRC-010
    fn coverage_buffer_for_fragment_train_propagates_length_overflow_error() {
        let header = HeaderVariant::Ntscf(avtp::NtscfHeader::default());
        let final_fragment_msg = sample_abb_message(acf::ACF_MSG_LENGTH_9BIT_MAX, b"unused");
        let final_fragment = AcfCoverageMessage::Abb(&final_fragment_msg);
        let segments: [&[u8]; 1] = [b"x"];
        assert_eq!(
            build_crc32_coverage_buffer_for_fragment_train(&header, &final_fragment, &segments),
            Err(RcpError::InvalidSize)
        );
    }

    #[test]
    //fusa:test REQ-CRC-010
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
