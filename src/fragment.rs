//fusa:req REQ-FRAG-001
//fusa:req REQ-FRAG-002
//fusa:req REQ-FRAG-003
//fusa:req REQ-FRAG-004
//fusa:req REQ-FRAG-005
//fusa:req REQ-FRAG-006
//fusa:req REQ-FRAG-007
//fusa:req REQ-FRAG-008

//! Multi-AVTPDU fragmentation: the encode-side planner/splitter and the
//! decode-side reassembly buffer, mirroring c-RCP's `src/fragment.c` /
//! `include/rcp/fragment.h`.
//!
//! ## Wire semantics (shared with c-RCP)
//!
//! A single logical ACF_ABB/ACF_GBB payload is split into an ordered
//! sequence of one or more ACF messages ("fragments") that share the
//! logical message's `acf_msg_type`, `byte_bus_id`, `op` and
//! `transaction_num`. Every fragment but the last sets `ms = 1` and carries
//! its own zero-based index in the 12-bit `read_size`/`segment_num` field;
//! the final fragment sets `ms = 0` and carries whatever value that field
//! ordinarily holds for the message kind involved (e.g. `read_size` on a
//! read response). A message that never needed fragmenting is a
//! one-fragment sequence (`ms = 0`), byte-identical to unfragmented traffic.
//!
//! Because `segment_num` is 12 bits wide, at most
//! [`MAX_INTERMEDIATE_SEGMENTS`] (4096) `ms = 1` fragments can precede the
//! final one.
//!
//! ## Encode side
//!
//! [`fragment_plan_count`]/[`fragment_plan`] are direct ports of c-RCP's
//! `rcp_fragment_plan_count()`/`rcp_fragment_plan()` (`c-RCP/src/fragment.c:26-82`):
//! a greedy, fixed-size split where every fragment but the last carries
//! exactly `max_fragment_payload` octets. [`fragment_abb_message`]/
//! [`fragment_gbb_message`] apply that plan to a whole
//! [`AcfAbbMessage`]/[`AcfGbbMessage`], the way c-RCP's per-endpoint
//! fragmented encoders do (e.g. `c-RCP/src/ep_can.c:442-466`).
//!
//! Safe-point CRC (TC18-13.6-007, "fragmentation happens after CRC
//! calculation ... the CRC ... will be transmitted in the last segment"):
//! like c-RCP (`include/rcp/fragment.h:113-127`), this module does not
//! compute or place the CRC itself. The caller computes the CRC over the
//! whole (pre-fragmentation) message via [`crate::e2e`] and appends it to
//! the final fragment only ([`crate::e2e::check_fragment_crc_placement`]).
//!
//! ## Decode side
//!
//! [`FragmentReassemblyBuffer`] mirrors `rcp_fragment_reassembler_feed()`
//! (`c-RCP/src/fragment.c:158-192`): it accepts fragments in wire-arrival
//! order, requires the first `ms = 1` fragment to carry `segment_num == 0`
//! and every subsequent `ms = 1` fragment to carry exactly one more, and
//! completes the train on any `ms = 0` fragment without consulting that
//! fragment's `read_size`/`segment_num` field.
//!
//! The reassembly bound is this crate's own (kept per issue #176): it is
//! [`crate::regmap::RequestStreamConfigEntry::rx_stream_max_request_size`],
//! applied to the running combined length, with `0` meaning "fragmentation
//! unsupported on this stream" ([`crate::RcpError::UnsupportedCmd`]), rather
//! than c-RCP's caller-chosen fixed `max_total_len`.

use std::fmt;

use crate::acf::READ_SIZE_SEGMENT_12BIT_MAX;
use crate::acf::{AcfAbbMessage, AcfGbbMessage, ByteMessageInfo, ReadSizeOrSegment};
use crate::avtp::HeaderVariant;
use crate::e2e::{self, AcfCoverageMessage, CombinedFragmentPayload};
use crate::RcpError;

// ─────────────────────────────────────────────────────────────────────────────
//  Encode side: planning
// ─────────────────────────────────────────────────────────────────────────────

/// The largest number of `ms = 1` (intermediate) fragments one message can
/// be split into: `segment_num` is 12 bits wide, giving 4096 distinct values
/// (`0..=4095`), every one usable by an intermediate fragment. Port of
/// c-RCP's `RCP_FRAGMENT_MAX_INTERMEDIATE_SEGMENTS`
/// (`c-RCP/include/rcp/fragment.h:202`).
//fusa:req REQ-FRAG-010
pub const MAX_INTERMEDIATE_SEGMENTS: usize = READ_SIZE_SEGMENT_12BIT_MAX as usize + 1;

/// Why [`fragment_plan`] (or the splitters built on it) could not produce a
/// plan. Port of c-RCP's `rcp_fragment_errc_t`
/// (`c-RCP/include/rcp/fragment.h:168-182`); [`FragmentPlanError::code`]
/// returns c-RCP's numeric value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
//fusa:req REQ-FRAG-013
//fusa:req REQ-FRAG-015
pub enum FragmentPlanError {
    /// `max_fragment_payload == 0` and the payload does not fit in a single
    /// fragment (`RCP_FRAGMENT_ERR_DISABLED`).
    Disabled,
    /// The split would need more than [`MAX_INTERMEDIATE_SEGMENTS`]
    /// intermediate fragments (`RCP_FRAGMENT_ERR_TOO_MANY_SEGMENTS`).
    TooManySegments,
    /// The caller-supplied output slice's length does not equal
    /// [`fragment_plan_count`]'s answer (`RCP_FRAGMENT_ERR_BAD_SEGMENT_COUNT`).
    BadSegmentCount,
}

impl FragmentPlanError {
    /// c-RCP's numeric `rcp_fragment_errc_t` value for this error
    /// (`RCP_FRAGMENT_OK` is 0 and has no variant here).
    //fusa:req REQ-FRAG-015
    pub fn code(self) -> u32 {
        match self {
            FragmentPlanError::Disabled => 1,
            FragmentPlanError::TooManySegments => 2,
            FragmentPlanError::BadSegmentCount => 3,
        }
    }
}

impl fmt::Display for FragmentPlanError {
    /// The same message text as c-RCP's `rcp_fragment_strerror()`
    /// (`c-RCP/src/fragment.c:11-20`).
    //fusa:req REQ-FRAG-015
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            FragmentPlanError::Disabled => {
                "rcp/fragment: fragmentation disabled for this max_fragment_payload"
            }
            FragmentPlanError::TooManySegments => {
                "rcp/fragment: payload needs more segments than segment_num can address"
            }
            FragmentPlanError::BadSegmentCount => {
                "rcp/fragment: segment_count does not match rcp_fragment_plan_count()"
            }
        })
    }
}

impl std::error::Error for FragmentPlanError {}

/// One planned fragment: the slice `[offset, offset + len)` of the original
/// payload it carries, whether it is intermediate (`ms == true`) or final
/// (`ms == false`), and — meaningful only when `ms` is `true` — its
/// `segment_num`. Port of c-RCP's `rcp_fragment_segment_t`
/// (`c-RCP/include/rcp/fragment.h:211-216`); a final fragment's
/// `segment_num` is left at `0`, since that wire slot means something else
/// once `ms == 0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-FRAG-012
pub struct FragmentSegment {
    /// Offset of this fragment's slice within the original payload.
    pub offset: usize,
    /// Length of this fragment's slice.
    pub len: usize,
    /// More-segments flag for this fragment.
    pub ms: bool,
    /// Zero-based index of this fragment (`0..=4095`); `0` on the final
    /// fragment.
    pub segment_num: u16,
}

/// The number of fragments [`fragment_plan`] would produce for
/// `payload_len` octets split into fragments of at most
/// `max_fragment_payload` octets each. Port of c-RCP's
/// `rcp_fragment_plan_count()` (`c-RCP/src/fragment.c:26-40`).
///
/// An empty payload, or one that already fits in `max_fragment_payload`,
/// always plans to exactly one fragment (even when `max_fragment_payload ==
/// 0` for the empty case). Returns `0` — a value no valid plan produces —
/// when `max_fragment_payload == 0` and the payload is non-empty, or when the
/// split would need more than [`MAX_INTERMEDIATE_SEGMENTS`] intermediate
/// fragments.
//fusa:req REQ-FRAG-011
pub fn fragment_plan_count(payload_len: usize, max_fragment_payload: usize) -> usize {
    if payload_len == 0 {
        return 1;
    }
    if max_fragment_payload == 0 {
        return 0;
    }
    if payload_len <= max_fragment_payload {
        return 1;
    }
    let count = payload_len.div_ceil(max_fragment_payload);
    // Every segment but the last is intermediate.
    if count - 1 > MAX_INTERMEDIATE_SEGMENTS {
        return 0;
    }
    count
}

/// Fills `out` with the greedy, fixed-size splitting plan for `payload_len`
/// octets in fragments of at most `max_fragment_payload` octets. Port of
/// c-RCP's `rcp_fragment_plan()` (`c-RCP/src/fragment.c:45-82`).
///
/// `out.len()` must equal [`fragment_plan_count`]`(payload_len,
/// max_fragment_payload)` exactly. Returns
/// [`FragmentPlanError::Disabled`]/[`FragmentPlanError::TooManySegments`]
/// under the conditions [`fragment_plan_count`] returns `0` for (checked
/// first), otherwise [`FragmentPlanError::BadSegmentCount`] on a length
/// mismatch; `out` is left untouched on every error. On success every
/// fragment but the last has `ms == true`, `len == max_fragment_payload` and
/// `segment_num` equal to its index; the last has `ms == false` and
/// `segment_num == 0`.
//fusa:req REQ-FRAG-012
//fusa:req REQ-FRAG-013
pub fn fragment_plan(
    payload_len: usize,
    max_fragment_payload: usize,
    out: &mut [FragmentSegment],
) -> Result<(), FragmentPlanError> {
    let expected = fragment_plan_count(payload_len, max_fragment_payload);
    if expected == 0 {
        return Err(if max_fragment_payload == 0 {
            FragmentPlanError::Disabled
        } else {
            FragmentPlanError::TooManySegments
        });
    }
    if out.len() != expected {
        return Err(FragmentPlanError::BadSegmentCount);
    }

    if expected == 1 {
        out[0] = FragmentSegment {
            offset: 0,
            len: payload_len,
            ms: false,
            segment_num: 0,
        };
        return Ok(());
    }

    let mut off = 0usize;
    for (i, seg) in out[..expected - 1].iter_mut().enumerate() {
        *seg = FragmentSegment {
            offset: off,
            len: max_fragment_payload,
            ms: true,
            // i < MAX_INTERMEDIATE_SEGMENTS (4096) by fragment_plan_count.
            segment_num: i as u16,
        };
        off += max_fragment_payload;
    }
    out[expected - 1] = FragmentSegment {
        offset: off,
        len: payload_len - off,
        ms: false,
        segment_num: 0,
    };
    Ok(())
}

/// Convenience wrapper: allocates and returns the [`fragment_plan`] for
/// `payload_len`/`max_fragment_payload`.
//fusa:req REQ-FRAG-012
//fusa:req REQ-FRAG-013
pub fn fragment_plan_vec(
    payload_len: usize,
    max_fragment_payload: usize,
) -> Result<Vec<FragmentSegment>, FragmentPlanError> {
    let count = fragment_plan_count(payload_len, max_fragment_payload);
    let mut segs = vec![FragmentSegment::default(); count];
    fragment_plan(payload_len, max_fragment_payload, &mut segs)?;
    Ok(segs)
}

/// The `byte_message_info` header for one planned fragment of a message
/// whose (unfragmented) header is `template`: identical except for `ms` and,
/// on intermediate fragments, `read_size`/`segment_num` (set to the
/// fragment's `segment_num`). The final fragment keeps `template`'s own
/// field value — the ordinary value that slot carries for the message kind
/// (c-RCP: `segs[i].ms ? segs[i].segment_num : <ordinary value>`,
/// `c-RCP/src/discovery.c:316`, `c-RCP/src/ep_can.c:455`).
fn fragment_info(template: &ByteMessageInfo, seg: &FragmentSegment) -> ByteMessageInfo {
    ByteMessageInfo {
        ms: seg.ms,
        read_size_segment: if seg.ms {
            ReadSizeOrSegment(seg.segment_num)
        } else {
            template.read_size_segment
        },
        ..*template
    }
}

/// Splits `msg` into the ACF_ABB fragment train [`fragment_plan`] describes
/// for `msg.payload.len()`/`max_fragment_payload` (`max_fragment_payload`
/// counts payload octets only, excluding the 8-octet header and pad).
///
/// Every fragment copies `msg.info` except `ms` and, on intermediate
/// fragments, `read_size`/`segment_num` (see the module doc comment); a
/// payload that fits yields exactly one fragment equal to `msg` with
/// `ms = false`. Encoding each result with [`crate::acf::encode_acf_abb`]
/// produces the same bytes c-RCP's fragmented encoders emit for the same
/// header and payload.
//fusa:req REQ-FRAG-014
pub fn fragment_abb_message(
    msg: &AcfAbbMessage,
    max_fragment_payload: usize,
) -> Result<Vec<AcfAbbMessage>, FragmentPlanError> {
    let plan = fragment_plan_vec(msg.payload.len(), max_fragment_payload)?;
    Ok(plan
        .iter()
        .map(|seg| AcfAbbMessage {
            info: fragment_info(&msg.info, seg),
            payload: msg.payload[seg.offset..seg.offset + seg.len].to_vec(),
        })
        .collect())
}

/// ACF_GBB counterpart of [`fragment_abb_message`]: every fragment also
/// carries `msg.message_timestamp` unchanged (as c-RCP's timed fragmented
/// response encoder does, `c-RCP/src/ep_can.c:654-666`).
//fusa:req REQ-FRAG-014
pub fn fragment_gbb_message(
    msg: &AcfGbbMessage,
    max_fragment_payload: usize,
) -> Result<Vec<AcfGbbMessage>, FragmentPlanError> {
    let plan = fragment_plan_vec(msg.payload.len(), max_fragment_payload)?;
    Ok(plan
        .iter()
        .map(|seg| AcfGbbMessage {
            info: fragment_info(&msg.info, seg),
            message_timestamp: msg.message_timestamp,
            payload: msg.payload[seg.offset..seg.offset + seg.len].to_vec(),
        })
        .collect())
}

// ─────────────────────────────────────────────────────────────────────────────
//  Decode side: reassembly
// ─────────────────────────────────────────────────────────────────────────────

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

/// A live, per-stream multi-AVTPDU reassembly buffer — the counterpart of
/// c-RCP's `rcp_fragment_reassembler_t` (`c-RCP/include/rcp/fragment.h:285-292`).
///
/// Bounded by a caller-supplied `rx_stream_max_request_size` (see
/// [`crate::regmap::RequestStreamConfigEntry::rx_stream_max_request_size`]);
/// see this module's doc comment for the ordering and bounding rules.
#[derive(Debug, Clone, PartialEq, Eq)]
//fusa:req REQ-FRAG-001
pub struct FragmentReassemblyBuffer {
    max_request_size: u16,
    segments: Vec<Vec<u8>>,
    total_len: usize,
    collecting: bool,
    expected_segment_num: u16,
    complete: bool,
}

impl FragmentReassemblyBuffer {
    /// Creates an empty reassembly buffer bounded by `rx_stream_max_request_size`
    /// bytes of *combined* payload. `0` is accepted here (it is the
    /// stream-config field's own "fragmentation unsupported" sentinel); see
    /// [`Self::fragmentation_supported`] and [`Self::accept_fragment`].
    //fusa:req REQ-FRAG-001
    pub fn new(rx_stream_max_request_size: u16) -> Self {
        FragmentReassemblyBuffer {
            max_request_size: rx_stream_max_request_size,
            segments: Vec::new(),
            total_len: 0,
            collecting: false,
            expected_segment_num: 0,
            complete: false,
        }
    }

    /// Whether this stream's configured bound allows fragmentation at all.
    /// `false` exactly when this buffer was constructed with
    /// `rx_stream_max_request_size == 0`, per that field's own documented
    /// sentinel meaning.
    //fusa:req REQ-FRAG-001
    pub fn fragmentation_supported(&self) -> bool {
        self.max_request_size != 0
    }

    /// Whether a train is currently mid-reassembly: at least one `ms = 1`
    /// fragment accepted since the last [`Self::reset`] (or construction)
    /// and no final `ms = 0` fragment yet. Mirrors c-RCP's
    /// `rcp_fragment_reassembler_is_collecting()`.
    //fusa:req REQ-FRAG-001
    pub fn is_in_progress(&self) -> bool {
        self.collecting
    }

    /// Feeds one wire-arrival-ordered fragment's already-decoded header and
    /// payload into the buffer. Mirrors c-RCP's
    /// `rcp_fragment_reassembler_feed()` (`c-RCP/src/fragment.c:158-192`),
    /// including its check order.
    ///
    /// - `Err(RcpError::UnsupportedCmd)` if this buffer was constructed with
    ///   `rx_stream_max_request_size == 0`.
    /// - `Err(RcpError::InvalidParameter)` (c-RCP:
    ///   `RCP_FRAGMENT_REASM_ERR_OUT_OF_ORDER`) if `info.ms` is `true` and
    ///   the raw `read_size`/`segment_num` field is not the next expected
    ///   zero-based index (`0` for a train's first fragment, then one more
    ///   per accepted intermediate fragment), or exceeds the field's 12-bit
    ///   width. So at most [`MAX_INTERMEDIATE_SEGMENTS`] intermediate
    ///   fragments can be accepted per train.
    /// - `Err(RcpError::InvalidParameter)` if the buffer already holds a
    ///   [`FragmentAcceptOutcome::Complete`] train and has not been
    ///   [`Self::reset`].
    /// - `Err(RcpError::PayloadTooLarge)` (c-RCP:
    ///   `RCP_FRAGMENT_REASM_ERR_TOO_LARGE`) if storing `payload` would push
    ///   the combined length past `rx_stream_max_request_size`.
    ///
    /// The buffer's state is unchanged on every `Err`.
    ///
    /// An `ms = 0` fragment completes the train — or, on an idle buffer, is a
    /// complete never-fragmented message on its own — **without consulting
    /// its `read_size`/`segment_num` field**.
    ///
    /// c-RCP parity: the final (`ms = 0`) fragment's field is ignored, as
    /// `rcp_fragment_reassembler_feed()` does (`c-RCP/src/fragment.c:180`,
    /// `include/rcp/fragment.h:325-326`); c-RCP's planner writes `0` there
    /// (`fragment.c:79`) and its encoders write the ordinary `read_size`
    /// (`discovery.c:316`). Requiring a continued `segment_num` there would
    /// reject c-RCP-encoded trains.
    ///
    /// c-RCP parity: whether the field is a `segment_num` is decided by `ms`
    /// alone, not by `op`. TC18 Table 4 (TC18-11.2.1-005) reads the field as
    /// `read_size` when `op = 0`, but c-RCP emits `op = 0` (read) response
    /// trains whose `ms = 1` fragments carry `segment_num`
    /// (`c-RCP/src/discovery.c:311-316`, `ep_can.c:657-664`) and its
    /// reassembler never looks at `op` (`fragment.c:158-192`). The op-gated
    /// accessors [`ByteMessageInfo::read_size`]/[`ByteMessageInfo::segment_num`]
    /// are unchanged for non-fragmentation use.
    //fusa:req REQ-FRAG-002
    //fusa:req REQ-FRAG-003
    //fusa:req REQ-FRAG-004
    //fusa:req REQ-FRAG-005
    //fusa:req REQ-FRAG-009
    //fusa:req REQ-FRAG-010
    pub fn accept_fragment(
        &mut self,
        info: &ByteMessageInfo,
        payload: &[u8],
    ) -> Result<FragmentAcceptOutcome, RcpError> {
        let segment_num = self.check_feed_preconditions(info)?;
        let prospective_total = self.total_len + payload.len();
        let too_large = prospective_total > self.max_request_size as usize;

        if !self.collecting {
            return self.accept_on_idle(
                info.ms,
                segment_num,
                payload,
                prospective_total,
                too_large,
            );
        }

        if info.ms && segment_num != self.expected_segment_num {
            return Err(RcpError::InvalidParameter);
        }
        if too_large {
            return Err(RcpError::PayloadTooLarge);
        }
        self.push(payload, prospective_total);

        if info.ms {
            // segment_num <= 4095 was checked above, so this is <= 4096.
            self.expected_segment_num += 1;
            return Ok(FragmentAcceptOutcome::Continuing);
        }
        self.collecting = false;
        self.complete = true;
        Ok(FragmentAcceptOutcome::Complete)
    }

    /// The state-independent checks [`Self::accept_fragment`] runs first, in
    /// check order (`c-RCP/src/fragment.c:158-192`). Returns the raw
    /// `read_size`/`segment_num` field on success.
    fn check_feed_preconditions(&self, info: &ByteMessageInfo) -> Result<u16, RcpError> {
        if !self.fragmentation_supported() {
            return Err(RcpError::UnsupportedCmd);
        }
        // Rust-only guard (not a wire behaviour): c-RCP appends a new
        // message onto a completed one unless the caller resets
        // (`fragment.c:163-169`); this buffer fails closed instead.
        if self.complete {
            return Err(RcpError::InvalidParameter);
        }
        let segment_num = info.read_size_segment.as_segment_num();
        if info.ms && segment_num > READ_SIZE_SEGMENT_12BIT_MAX {
            return Err(RcpError::InvalidParameter);
        }
        Ok(segment_num)
    }

    /// [`Self::accept_fragment`] on an idle buffer: either a complete
    /// never-fragmented message (`ms = 0`) or the first fragment of a train,
    /// which must carry `segment_num == 0`.
    fn accept_on_idle(
        &mut self,
        ms: bool,
        segment_num: u16,
        payload: &[u8],
        prospective_total: usize,
        too_large: bool,
    ) -> Result<FragmentAcceptOutcome, RcpError> {
        if too_large {
            return Err(RcpError::PayloadTooLarge);
        }
        if !ms {
            // Never-fragmented, single-segment message.
            self.push(payload, prospective_total);
            self.complete = true;
            return Ok(FragmentAcceptOutcome::Complete);
        }
        if segment_num != 0 {
            return Err(RcpError::InvalidParameter);
        }
        self.push(payload, prospective_total);
        self.collecting = true;
        self.expected_segment_num = 1;
        Ok(FragmentAcceptOutcome::Continuing)
    }

    fn push(&mut self, payload: &[u8], new_total: usize) {
        self.segments.push(payload.to_vec());
        self.total_len = new_total;
    }

    /// Borrowed views of every segment accepted so far, in arrival order —
    /// the same shape [`crate::e2e::build_crc32_coverage_buffer_for_fragment_train`]/
    /// [`crate::e2e::crc32_tc18_for_fragment_train`] take as their own
    /// `segments` parameter, and what [`verify_reassembled_train_crc`]
    /// passes them.
    //fusa:req REQ-FRAG-005
    pub fn segment_refs(&self) -> Vec<&[u8]> {
        self.segments.iter().map(Vec::as_slice).collect()
    }

    /// The combined payload of every segment accepted so far, concatenated
    /// in arrival order by composing (not re-deriving)
    /// [`crate::e2e::CombinedFragmentPayload::assemble`].
    //fusa:req REQ-FRAG-005
    pub fn combined_payload(&self) -> CombinedFragmentPayload {
        CombinedFragmentPayload::assemble(&self.segment_refs())
    }

    /// Clears all accumulated state, readying this buffer for a new train.
    /// Does not change the configured `rx_stream_max_request_size` bound.
    /// Mirrors c-RCP's `rcp_fragment_reassembler_reset()`.
    //fusa:req REQ-FRAG-006
    pub fn reset(&mut self) {
        *self = FragmentReassemblyBuffer::new(self.max_request_size);
    }
}

/// Re-verifies the "only the last fragment carries the CRC" rule and the
/// safe-point CRC-32 against a real [`FragmentReassemblyBuffer`]'s own
/// wire-collected segments.
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
//fusa:req REQ-FRAG-007
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
    use crate::acf::{self, Evt};
    use crate::avtp;

    fn fragment_info(ms: bool, segment_num: u16) -> ByteMessageInfo {
        ByteMessageInfo {
            ms,
            read_size_segment: ReadSizeOrSegment(segment_num),
            ..Default::default()
        }
    }

    // ── c-RCP golden vectors ─────────────────────────────────────────────────
    //
    // Generated by a C harness linked against c-RCP's real librcp.a
    // (c-RCP src/fragment.c rcp_fragment_plan_count()/rcp_fragment_plan()
    // + src/acf.c rcp_acf_encode_abb()), applying the plan exactly as
    // c-RCP's fragmented encoders do (`hdr.ms = segs[i].ms;
    // hdr.read_size_or_segment_num = segs[i].ms ? segs[i].segment_num :
    // <ordinary value>`, src/ep_can.c:454-455, src/discovery.c:315-316).
    // Payload octets are 0xA0, 0xA1, ... in every train.

    // Write request: byte_bus_id 0x123, op = RCP_ACF_OP_WRITE (wire 1),
    // transaction_num 0x5A, final field 0. payload_len=25,
    // max_fragment_payload=10 -> 3 frames.
    const C_RCP_WRITE_TRAIN_25_BY_10: &[&[u8]] = &[
        &[
            0x1C, 0x05, 0x81, 0x23, 0x00, 0x5A, 0x90, 0x00, 0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5,
            0xA6, 0xA7, 0xA8, 0xA9, 0x00, 0x00,
        ],
        &[
            0x1C, 0x05, 0x81, 0x23, 0x00, 0x5A, 0x90, 0x01, 0xAA, 0xAB, 0xAC, 0xAD, 0xAE, 0xAF,
            0xB0, 0xB1, 0xB2, 0xB3, 0x00, 0x00,
        ],
        &[
            0x1C, 0x04, 0xC1, 0x23, 0x00, 0x5A, 0x80, 0x00, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0x00,
            0x00, 0x00,
        ],
    ];

    // Read response: byte_bus_id 0x7FF, op = RCP_ACF_OP_READ (wire 0),
    // rsp = 1, transaction_num 0x07, final field = read_size 23 (0x17).
    // payload_len=23, max_fragment_payload=8 -> 3 frames. The ms=1 frames
    // carry op=0 AND a segment_num (byte 6 = 0x50).
    const C_RCP_READ_RESPONSE_TRAIN_23_BY_8: &[&[u8]] = &[
        &[
            0x1C, 0x04, 0x07, 0xFF, 0x00, 0x07, 0x50, 0x00, 0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5,
            0xA6, 0xA7,
        ],
        &[
            0x1C, 0x04, 0x07, 0xFF, 0x00, 0x07, 0x50, 0x01, 0xA8, 0xA9, 0xAA, 0xAB, 0xAC, 0xAD,
            0xAE, 0xAF,
        ],
        &[
            0x1C, 0x04, 0x47, 0xFF, 0x00, 0x07, 0x40, 0x17, 0xB0, 0xB1, 0xB2, 0xB3, 0xB4, 0xB5,
            0xB6, 0x00,
        ],
    ];

    // Same read-response header, payload_len=5 fits max 8 -> 1 frame,
    // ms=0, read_size 5.
    const C_RCP_SINGLE_READ_RESPONSE_5_BY_8: &[&[u8]] = &[&[
        0x1C, 0x04, 0xC7, 0xFF, 0x00, 0x07, 0x40, 0x05, 0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0x00, 0x00,
        0x00,
    ]];

    // (payload_len, max_fragment_payload, rcp_fragment_plan_count)
    const C_RCP_PLAN_COUNT: &[(usize, usize, usize)] = &[
        (0, 0, 1),
        (0, 10, 1),
        (5, 0, 0),
        (10, 10, 1),
        (11, 10, 2),
        (25, 10, 3),
        (30, 10, 3),
        (4097, 1, 4097),
        (4098, 1, 0),
        (40970, 10, 4097),
        (40971, 10, 0),
    ];

    // (payload_len, max, segment_count, rcp_fragment_plan errc)
    const C_RCP_PLAN_ERRC: &[(usize, usize, usize, u32)] = &[
        (5, 0, 1, 1),
        (4098, 1, 4098, 2),
        (25, 10, 2, 3),
        (25, 10, 3, 0),
    ];

    fn golden_payload(len: usize) -> Vec<u8> {
        (0..len).map(|i| 0xA0u8.wrapping_add(i as u8)).collect()
    }

    fn write_template(len: usize) -> AcfAbbMessage {
        AcfAbbMessage {
            info: ByteMessageInfo {
                byte_bus_id: 0x123,
                op: true,
                transaction_num: 0x5A,
                ..Default::default()
            },
            payload: golden_payload(len),
        }
    }

    fn read_response_template(len: usize, read_size: u16) -> AcfAbbMessage {
        AcfAbbMessage {
            info: ByteMessageInfo {
                byte_bus_id: 0x7FF,
                op: false,
                rsp: true,
                transaction_num: 0x07,
                read_size_segment: ReadSizeOrSegment(read_size),
                ..Default::default()
            },
            payload: golden_payload(len),
        }
    }

    fn encode_all(frags: &[AcfAbbMessage]) -> Vec<Vec<u8>> {
        frags
            .iter()
            .map(|m| acf::encode_acf_abb(m).unwrap())
            .collect()
    }

    fn reassemble_wire_train(frames: &[&[u8]], bound: u16) -> Result<Vec<u8>, RcpError> {
        let mut buf = FragmentReassemblyBuffer::new(bound);
        let mut last = None;
        for frame in frames {
            let msg = acf::decode_acf_abb(frame)?;
            last = Some(buf.accept_fragment(&msg.info, &msg.payload)?);
        }
        assert_eq!(last, Some(FragmentAcceptOutcome::Complete));
        Ok(buf.segment_refs().concat())
    }

    // ── planner: fragment_plan_count ─────────────────────────────────────────

    #[test]
    //fusa:test REQ-FRAG-011
    fn plan_count_matches_c_rcp_golden_table() {
        for &(len, max, want) in C_RCP_PLAN_COUNT {
            assert_eq!(fragment_plan_count(len, max), want, "({len}, {max})");
        }
    }

    #[test]
    //fusa:test REQ-FRAG-011
    //fusa:test REQ-FRAG-010
    fn plan_count_admits_exactly_4096_intermediate_segments() {
        assert_eq!(MAX_INTERMEDIATE_SEGMENTS, 4096);
        assert_eq!(fragment_plan_count(MAX_INTERMEDIATE_SEGMENTS + 1, 1), 4097);
        assert_eq!(fragment_plan_count(MAX_INTERMEDIATE_SEGMENTS + 2, 1), 0);
    }

    // ── planner: fragment_plan ───────────────────────────────────────────────

    #[test]
    //fusa:test REQ-FRAG-012
    fn plan_single_segment_when_payload_fits() {
        assert_eq!(
            fragment_plan_vec(7, 10).unwrap(),
            vec![FragmentSegment {
                offset: 0,
                len: 7,
                ms: false,
                segment_num: 0
            }]
        );
        assert_eq!(
            fragment_plan_vec(0, 0).unwrap(),
            vec![FragmentSegment::default()]
        );
    }

    #[test]
    //fusa:test REQ-FRAG-012
    fn plan_multi_segment_layout_and_numbering() {
        let segs = fragment_plan_vec(25, 10).unwrap();
        assert_eq!(
            segs,
            vec![
                FragmentSegment {
                    offset: 0,
                    len: 10,
                    ms: true,
                    segment_num: 0
                },
                FragmentSegment {
                    offset: 10,
                    len: 10,
                    ms: true,
                    segment_num: 1
                },
                FragmentSegment {
                    offset: 20,
                    len: 5,
                    ms: false,
                    segment_num: 0
                },
            ]
        );
    }

    #[test]
    //fusa:test REQ-FRAG-012
    //fusa:test REQ-FRAG-010
    fn plan_4097_segments_matches_c_rcp_boundary() {
        // c-RCP harness: segs[4095] = off 4095 len 1 ms 1 segnum 4095;
        //                segs[4096] = off 4096 len 1 ms 0 segnum 0.
        let segs = fragment_plan_vec(4097, 1).unwrap();
        assert_eq!(segs.len(), 4097);
        assert_eq!(
            segs[4095],
            FragmentSegment {
                offset: 4095,
                len: 1,
                ms: true,
                segment_num: 4095
            }
        );
        assert_eq!(
            segs[4096],
            FragmentSegment {
                offset: 4096,
                len: 1,
                ms: false,
                segment_num: 0
            }
        );
    }

    #[test]
    //fusa:test REQ-FRAG-012
    fn plan_covers_payload_contiguously() {
        for (len, max) in [(1usize, 1usize), (99, 7), (100, 10), (4000, 3)] {
            let segs = fragment_plan_vec(len, max).unwrap();
            let mut off = 0;
            for (i, s) in segs.iter().enumerate() {
                assert_eq!(s.offset, off);
                let last = i + 1 == segs.len();
                assert_eq!(s.ms, !last);
                if !last {
                    assert_eq!(s.len, max);
                    assert_eq!(s.segment_num as usize, i);
                }
                off += s.len;
            }
            assert_eq!(off, len);
        }
    }

    #[test]
    //fusa:test REQ-FRAG-013
    fn plan_error_codes_match_c_rcp_golden_table() {
        for &(len, max, count, want) in C_RCP_PLAN_ERRC {
            let mut out = vec![FragmentSegment::default(); count];
            let got = match fragment_plan(len, max, &mut out) {
                Ok(()) => 0,
                Err(e) => e.code(),
            };
            assert_eq!(got, want, "({len}, {max}, {count})");
        }
    }

    #[test]
    //fusa:test REQ-FRAG-013
    fn plan_errors_leave_output_untouched() {
        let sentinel = FragmentSegment {
            offset: 9,
            len: 9,
            ms: true,
            segment_num: 9,
        };
        let mut out = vec![sentinel; 2];
        assert_eq!(
            fragment_plan(25, 10, &mut out),
            Err(FragmentPlanError::BadSegmentCount)
        );
        assert_eq!(out, vec![sentinel; 2]);
        assert_eq!(fragment_plan_vec(5, 0), Err(FragmentPlanError::Disabled));
        assert_eq!(
            fragment_plan_vec(4098, 1),
            Err(FragmentPlanError::TooManySegments)
        );
    }

    #[test]
    //fusa:test REQ-FRAG-015
    fn plan_error_display_is_distinct_and_matches_c_rcp_strerror() {
        let all = [
            FragmentPlanError::Disabled,
            FragmentPlanError::TooManySegments,
            FragmentPlanError::BadSegmentCount,
        ];
        let msgs: Vec<String> = all.iter().map(|e| e.to_string()).collect();
        assert_eq!(
            msgs[0],
            "rcp/fragment: fragmentation disabled for this max_fragment_payload"
        );
        assert_eq!(
            msgs[1],
            "rcp/fragment: payload needs more segments than segment_num can address"
        );
        assert_eq!(
            msgs[2],
            "rcp/fragment: segment_count does not match rcp_fragment_plan_count()"
        );
        assert_eq!(
            all.iter().map(|e| e.code()).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        let e: &dyn std::error::Error = &all[0];
        assert!(e.source().is_none());
    }

    // ── encoder: fragment_abb_message / fragment_gbb_message ─────────────────

    #[test]
    //fusa:test REQ-FRAG-014
    fn abb_write_train_is_byte_identical_to_c_rcp() {
        let frags = fragment_abb_message(&write_template(25), 10).unwrap();
        assert_eq!(encode_all(&frags), C_RCP_WRITE_TRAIN_25_BY_10);
    }

    #[test]
    //fusa:test REQ-FRAG-014
    fn abb_read_response_train_is_byte_identical_to_c_rcp() {
        let frags = fragment_abb_message(&read_response_template(23, 23), 8).unwrap();
        assert_eq!(encode_all(&frags), C_RCP_READ_RESPONSE_TRAIN_23_BY_8);
        let single = fragment_abb_message(&read_response_template(5, 5), 8).unwrap();
        assert_eq!(encode_all(&single), C_RCP_SINGLE_READ_RESPONSE_5_BY_8);
    }

    #[test]
    //fusa:test REQ-FRAG-014
    fn abb_fragments_copy_header_except_ms_and_segment_num() {
        let mut msg = read_response_template(23, 23);
        msg.info.evt = Evt {
            ack: false,
            sub_opcode: 3,
        };
        msg.info.hs = true;
        let frags = fragment_abb_message(&msg, 8).unwrap();
        for (i, f) in frags.iter().enumerate() {
            let last = i + 1 == frags.len();
            assert_eq!(f.info.ms, !last);
            let want_field = if last { 23 } else { i as u16 };
            assert_eq!(f.info.read_size_segment, ReadSizeOrSegment(want_field));
            assert_eq!(
                ByteMessageInfo {
                    ms: msg.info.ms,
                    read_size_segment: msg.info.read_size_segment,
                    ..f.info
                },
                msg.info
            );
        }
    }

    #[test]
    //fusa:test REQ-FRAG-014
    fn abb_fragmenter_propagates_plan_errors() {
        assert_eq!(
            fragment_abb_message(&write_template(5), 0),
            Err(FragmentPlanError::Disabled)
        );
        assert_eq!(
            fragment_abb_message(&write_template(4098), 1),
            Err(FragmentPlanError::TooManySegments)
        );
    }

    #[test]
    //fusa:test REQ-FRAG-014
    fn gbb_fragments_carry_timestamp_and_round_trip() {
        let msg = AcfGbbMessage {
            info: ByteMessageInfo {
                byte_bus_id: 0x042,
                transaction_num: 9,
                op: true,
                ..Default::default()
            },
            message_timestamp: 0x0102_0304_0506_0708,
            payload: golden_payload(30),
        };
        let frags = fragment_gbb_message(&msg, 12).unwrap();
        assert_eq!(frags.len(), 3);
        let mut buf = FragmentReassemblyBuffer::new(64);
        for f in &frags {
            assert_eq!(f.message_timestamp, msg.message_timestamp);
            let wire = acf::encode_acf_gbb(f).unwrap();
            let back = acf::decode_acf_gbb(&wire).unwrap();
            buf.accept_fragment(&back.info, &back.payload).unwrap();
        }
        assert_eq!(buf.segment_refs().concat(), msg.payload);
        assert_eq!(
            fragment_gbb_message(&msg, 0),
            Err(FragmentPlanError::Disabled)
        );
    }

    #[test]
    //fusa:test REQ-FRAG-014
    //fusa:test REQ-FRAG-005
    fn encoder_round_trips_through_reassembly_buffer() {
        for (len, max) in [(0usize, 4usize), (1, 4), (64, 4), (100, 33), (500, 1)] {
            let msg = write_template(len);
            let frames = encode_all(&fragment_abb_message(&msg, max).unwrap());
            let refs: Vec<&[u8]> = frames.iter().map(Vec::as_slice).collect();
            assert_eq!(reassemble_wire_train(&refs, 1024).unwrap(), msg.payload);
        }
    }

    // ── interop: reassembling c-RCP-encoded trains ───────────────────────────

    #[test]
    //fusa:test REQ-FRAG-009
    //fusa:test REQ-FRAG-005
    fn reassembles_c_rcp_write_train() {
        assert_eq!(
            reassemble_wire_train(C_RCP_WRITE_TRAIN_25_BY_10, 64).unwrap(),
            golden_payload(25)
        );
    }

    #[test]
    //fusa:test REQ-FRAG-009
    //fusa:test REQ-FRAG-003
    fn reassembles_c_rcp_read_response_train_with_op_zero_and_read_size_on_final() {
        assert_eq!(
            reassemble_wire_train(C_RCP_READ_RESPONSE_TRAIN_23_BY_8, 64).unwrap(),
            golden_payload(23)
        );
    }

    #[test]
    //fusa:test REQ-FRAG-009
    fn unfragmented_read_with_nonzero_read_size_completes() {
        assert_eq!(
            reassemble_wire_train(C_RCP_SINGLE_READ_RESPONSE_5_BY_8, 64).unwrap(),
            golden_payload(5)
        );
    }

    #[test]
    //fusa:test REQ-FRAG-009
    fn final_fragment_segment_num_field_is_ignored() {
        for final_field in [0u16, 2, 7, READ_SIZE_SEGMENT_12BIT_MAX, 0xFFFF] {
            let mut buf = FragmentReassemblyBuffer::new(64);
            buf.accept_fragment(&fragment_info(true, 0), b"a").unwrap();
            buf.accept_fragment(&fragment_info(true, 1), b"b").unwrap();
            assert_eq!(
                buf.accept_fragment(&fragment_info(false, final_field), b"c"),
                Ok(FragmentAcceptOutcome::Complete)
            );
            assert_eq!(buf.segment_refs().concat(), b"abc");
        }
    }

    // ── fragmentation_supported / new ────────────────────────────────────────

    #[test]
    //fusa:test REQ-FRAG-001
    fn new_with_zero_bound_reports_fragmentation_unsupported() {
        let buf = FragmentReassemblyBuffer::new(0);
        assert!(!buf.fragmentation_supported());
    }

    #[test]
    //fusa:test REQ-FRAG-001
    fn new_with_nonzero_bound_reports_fragmentation_supported() {
        let buf = FragmentReassemblyBuffer::new(128);
        assert!(buf.fragmentation_supported());
    }

    #[test]
    //fusa:test REQ-FRAG-001
    fn new_buffer_is_not_in_progress() {
        let buf = FragmentReassemblyBuffer::new(128);
        assert!(!buf.is_in_progress());
        assert!(buf.segment_refs().is_empty());
    }

    #[test]
    //fusa:test REQ-FRAG-001
    fn in_progress_tracks_collecting_state() {
        let mut buf = FragmentReassemblyBuffer::new(128);
        buf.accept_fragment(&fragment_info(true, 0), b"a").unwrap();
        assert!(buf.is_in_progress());
        buf.accept_fragment(&fragment_info(false, 0), b"b").unwrap();
        assert!(!buf.is_in_progress());
        let mut single = FragmentReassemblyBuffer::new(128);
        single
            .accept_fragment(&fragment_info(false, 0), b"x")
            .unwrap();
        assert!(!single.is_in_progress());
    }

    // ── unsupported-stream rejection ─────────────────────────────────────────

    #[test]
    //fusa:test REQ-FRAG-002
    fn accept_fragment_rejects_on_zero_bound_stream() {
        let mut buf = FragmentReassemblyBuffer::new(0);
        for info in [fragment_info(true, 0), fragment_info(false, 0)] {
            assert_eq!(
                buf.accept_fragment(&info, b"x"),
                Err(RcpError::UnsupportedCmd)
            );
            assert!(!buf.is_in_progress());
        }
    }

    // ── segment_num ordering ─────────────────────────────────────────────────

    #[test]
    //fusa:test REQ-FRAG-003
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
    //fusa:test REQ-FRAG-003
    fn accept_fragment_rejects_gap_in_segment_num() {
        let mut buf = FragmentReassemblyBuffer::new(64);
        buf.accept_fragment(&fragment_info(true, 0), b"a").unwrap();
        assert_eq!(
            buf.accept_fragment(&fragment_info(true, 2), b"c"),
            Err(RcpError::InvalidParameter)
        );
    }

    #[test]
    //fusa:test REQ-FRAG-003
    fn accept_fragment_rejects_duplicate_segment_num() {
        let mut buf = FragmentReassemblyBuffer::new(64);
        buf.accept_fragment(&fragment_info(true, 0), b"a").unwrap();
        assert_eq!(
            buf.accept_fragment(&fragment_info(true, 0), b"a-again"),
            Err(RcpError::InvalidParameter)
        );
    }

    #[test]
    //fusa:test REQ-FRAG-003
    fn accept_fragment_rejects_out_of_order_segment_num() {
        let mut buf = FragmentReassemblyBuffer::new(64);
        buf.accept_fragment(&fragment_info(true, 0), b"a").unwrap();
        buf.accept_fragment(&fragment_info(true, 1), b"b").unwrap();
        assert_eq!(
            buf.accept_fragment(&fragment_info(true, 0), b"replay"),
            Err(RcpError::InvalidParameter)
        );
    }

    #[test]
    //fusa:test REQ-FRAG-003
    fn accept_fragment_first_ms_fragment_must_start_at_segment_num_zero() {
        let mut buf = FragmentReassemblyBuffer::new(64);
        assert_eq!(
            buf.accept_fragment(&fragment_info(true, 1), b"a"),
            Err(RcpError::InvalidParameter)
        );
        assert!(!buf.is_in_progress());
    }

    #[test]
    //fusa:test REQ-FRAG-003
    fn segment_num_is_read_regardless_of_op() {
        for op in [false, true] {
            let mut buf = FragmentReassemblyBuffer::new(64);
            let mut info = fragment_info(true, 0);
            info.op = op;
            buf.accept_fragment(&info, b"a").unwrap();
            info.read_size_segment = ReadSizeOrSegment(5);
            assert_eq!(
                buf.accept_fragment(&info, b"b"),
                Err(RcpError::InvalidParameter),
                "op={op}"
            );
        }
    }

    #[test]
    //fusa:test REQ-FRAG-003
    fn rejected_fragment_does_not_mutate_buffer_state() {
        let mut buf = FragmentReassemblyBuffer::new(64);
        buf.accept_fragment(&fragment_info(true, 0), b"a").unwrap();
        let before = buf.clone();
        let _ = buf.accept_fragment(&fragment_info(true, 5), b"bad");
        assert_eq!(buf, before);
    }

    // ── 12-bit segment_num width ─────────────────────────────────────────────

    #[test]
    //fusa:test REQ-FRAG-010
    fn accepts_4096_intermediate_fragments_then_rejects_a_4097th() {
        let mut buf = FragmentReassemblyBuffer::new(u16::MAX);
        for n in 0..MAX_INTERMEDIATE_SEGMENTS as u16 {
            assert_eq!(
                buf.accept_fragment(&fragment_info(true, n), b"x"),
                Ok(FragmentAcceptOutcome::Continuing)
            );
        }
        // No 12-bit value can be the 4097th index; neither 4096 (out of
        // field range) nor a wrapped 0 is accepted.
        for n in [4096u16, 0] {
            assert_eq!(
                buf.accept_fragment(&fragment_info(true, n), b"x"),
                Err(RcpError::InvalidParameter)
            );
        }
        assert_eq!(
            buf.accept_fragment(&fragment_info(false, 0), b"y"),
            Ok(FragmentAcceptOutcome::Complete)
        );
        assert_eq!(buf.segment_refs().len(), MAX_INTERMEDIATE_SEGMENTS + 1);
    }

    #[test]
    //fusa:test REQ-FRAG-010
    fn rejects_out_of_range_segment_num_on_intermediate_fragment() {
        let mut buf = FragmentReassemblyBuffer::new(64);
        assert_eq!(
            buf.accept_fragment(&fragment_info(true, 0x1000), b"x"),
            Err(RcpError::InvalidParameter)
        );
        assert!(!buf.is_in_progress());
    }

    // ── rx_stream_max_request_size bound ─────────────────────────────────────

    #[test]
    //fusa:test REQ-FRAG-004
    fn accept_fragment_rejects_when_combined_length_exceeds_bound() {
        let mut buf = FragmentReassemblyBuffer::new(3);
        buf.accept_fragment(&fragment_info(true, 0), b"ab").unwrap();
        assert_eq!(
            buf.accept_fragment(&fragment_info(false, 1), b"cd"),
            Err(RcpError::PayloadTooLarge)
        );
    }

    #[test]
    //fusa:test REQ-FRAG-004
    fn accept_fragment_rejects_oversized_first_fragment() {
        for ms in [true, false] {
            let mut buf = FragmentReassemblyBuffer::new(3);
            assert_eq!(
                buf.accept_fragment(&fragment_info(ms, 0), b"abcd"),
                Err(RcpError::PayloadTooLarge)
            );
            assert!(buf.segment_refs().is_empty());
        }
    }

    #[test]
    //fusa:test REQ-FRAG-004
    fn accept_fragment_accepts_combined_length_exactly_at_bound() {
        let mut buf = FragmentReassemblyBuffer::new(4);
        buf.accept_fragment(&fragment_info(true, 0), b"ab").unwrap();
        assert_eq!(
            buf.accept_fragment(&fragment_info(false, 1), b"cd"),
            Ok(FragmentAcceptOutcome::Complete)
        );
    }

    #[test]
    //fusa:test REQ-FRAG-004
    fn overflowing_fragment_does_not_mutate_buffer_state() {
        let mut buf = FragmentReassemblyBuffer::new(2);
        buf.accept_fragment(&fragment_info(true, 0), b"ab").unwrap();
        let before = buf.clone();
        let _ = buf.accept_fragment(&fragment_info(false, 1), b"too-big");
        assert_eq!(buf, before);
    }

    // ── Continuing / Complete / combined payload ─────────────────────────────

    #[test]
    //fusa:test REQ-FRAG-005
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
    //fusa:test REQ-FRAG-005
    fn combined_payload_matches_e2e_combined_fragment_payload_assemble() {
        let mut buf = FragmentReassemblyBuffer::new(64);
        let segments: [&[u8]; 3] = [b"seg-one-", b"seg-two-", b"seg-three"];
        for (i, seg) in segments.iter().enumerate() {
            let ms = i + 1 != segments.len();
            buf.accept_fragment(&fragment_info(ms, i as u16), seg)
                .unwrap();
        }
        assert_eq!(
            buf.combined_payload(),
            CombinedFragmentPayload::assemble(&segments)
        );
    }

    // ── reset ─────────────────────────────────────────────────────────────────

    #[test]
    //fusa:test REQ-FRAG-006
    fn reset_clears_state_for_a_new_train() {
        let mut buf = FragmentReassemblyBuffer::new(64);
        buf.accept_fragment(&fragment_info(false, 0), b"first-train")
            .unwrap();
        buf.reset();
        assert!(!buf.is_in_progress());
        assert_eq!(buf, FragmentReassemblyBuffer::new(64));
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
    //fusa:test REQ-FRAG-006
    fn accepting_a_fragment_after_complete_without_reset_is_rejected() {
        let mut buf = FragmentReassemblyBuffer::new(64);
        buf.accept_fragment(&fragment_info(false, 0), b"done")
            .unwrap();
        for info in [fragment_info(true, 0), fragment_info(false, 0)] {
            assert_eq!(
                buf.accept_fragment(&info, b"new-train"),
                Err(RcpError::InvalidParameter)
            );
        }
        assert_eq!(buf.segment_refs().concat(), b"done");
    }

    // ── never panics ──────────────────────────────────────────────────────────

    #[test]
    //fusa:test REQ-FRAG-008
    fn accept_fragment_never_panics_across_arbitrary_payload_lengths() {
        for len in [0usize, 1, 2, 3, 17, 64, 257] {
            let mut buf = FragmentReassemblyBuffer::new(u16::MAX);
            let payload: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
            let _ = buf.accept_fragment(&fragment_info(false, 0), &payload);
        }
    }

    #[test]
    //fusa:test REQ-FRAG-008
    fn accept_fragment_never_panics_across_every_raw_segment_num() {
        let mut buf = FragmentReassemblyBuffer::new(u16::MAX);
        for raw in 0..=u16::MAX {
            let _ = buf.accept_fragment(&fragment_info(true, raw), b"");
            let _ = buf.accept_fragment(&fragment_info(raw % 2 == 0, raw), b"");
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
    //fusa:test REQ-FRAG-007
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
    //fusa:test REQ-FRAG-007
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
    //fusa:test REQ-FRAG-007
    fn verify_reassembled_train_crc_propagates_length_overflow_error() {
        let mut buf = FragmentReassemblyBuffer::new(64);
        buf.accept_fragment(&fragment_info(false, 0), b"x").unwrap();
        let header = HeaderVariant::Ntscf(avtp::NtscfHeader::default());
        let final_msg = sample_final_fragment(acf::ACF_MSG_LENGTH_9BIT_MAX);
        let final_fragment = AcfCoverageMessage::Abb(&final_msg);
        assert_eq!(
            verify_reassembled_train_crc(&buf, &header, &final_fragment, true),
            Err(RcpError::InvalidSize)
        );
    }

    #[test]
    //fusa:test REQ-FRAG-007
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
