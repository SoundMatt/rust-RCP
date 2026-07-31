// fusa:req REQ-BMI-001
// fusa:req REQ-BMI-002
// fusa:req REQ-BMI-003
// fusa:req REQ-BMI-004
// fusa:req REQ-ABB-001
// fusa:req REQ-ABB-002
// fusa:req REQ-ABB-003
// fusa:req REQ-ABB-004
// fusa:req REQ-ABB-005
// fusa:req REQ-GBB-001
// fusa:req REQ-GBB-002
// fusa:req REQ-GBB-003
// fusa:req REQ-GBB-004
// fusa:req REQ-GBB-005
// fusa:req REQ-ECHO-001
// fusa:req REQ-ECHO-002
// fusa:req REQ-ECHO-003
// fusa:req REQ-ECHO-004

//! ACF (AVTP Control Format) messages — TC18 wire format core (`ROADMAP.md`
//! Milestone 1, "ACF Messages" subsection).
//!
//! This module is the second Milestone 1 subsection, picking up right after
//! [`crate::avtp`] finished "AVTPDU Framing". An ACF message is carried
//! inside the body of an NTSCF- or TSCF-headed AVTPDU (see
//! [`crate::avtp::NtscfHeader`]/[`crate::avtp::TscfHeader`]); this
//! module does not itself frame that outer AVTPDU.
//!
//! Three items are named on the Milestone 1 "ACF Messages" checklist, all
//! implemented here:
//!
//! - **ACF_ABB** (`acf_msg_type = 0x0E`) — [`AcfAbbMessage`] /
//!   [`encode_acf_abb`] / [`decode_acf_abb`]. Carries no timestamp of any
//!   kind. That absence is structural, not a value choice: the encoding has
//!   no reserved byte range sized to hold one, unlike a zeroed/reserved
//!   placeholder would — [`ACF_ABB_HEADER_LEN`] is 8 bytes narrower than
//!   [`ACF_GBB_HEADER_LEN`], exactly the width of ACF_GBB's
//!   `message_timestamp`, not merely a header with that field zeroed out.
//! - **ACF_GBB** (`acf_msg_type = 0x0D`) — [`AcfGbbMessage`] /
//!   [`encode_acf_gbb`] / [`decode_acf_gbb`]. The sibling type that *does*
//!   carry a 64-bit `message_timestamp`. Per this Milestone 1 item's own
//!   scope, `message_timestamp` here is a raw passthrough value only — its
//!   width/rollover behavior and the all-zero-timestamp fallback rule are
//!   the separate "Timestamp Semantics" checklist item, implemented in
//!   [`crate::timestamp`] ([`crate::timestamp::MessageTimestamp`]) as a
//!   standalone newtype consuming this field's raw `u64` value, rather than
//!   a change to this field's own type.
//! - **`byte_message_info`** — [`ByteMessageInfo`] /
//!   [`encode_byte_message_info`] / [`decode_byte_message_info`]. The
//!   8-byte header both ACF_ABB and ACF_GBB carry first, including
//!   `acf_msg_type` itself (see "Canonical wire layout" below) — this is
//!   *not* preceded by a separate one-byte discriminant.
//!
//! This module also implements one further item from the separate
//! "Addressing" subsection: the **echo-back rule** — [`build_response_info`]
//! / [`verify_echo_back`]. That rule (a response/ack must carry the same
//! `byte_bus_id` it was received under) is stated purely in terms of
//! `byte_bus_id`, which already lives on [`ByteMessageInfo`] here, so this
//! module is that rule's natural home even though the rest of "Addressing"
//! (`stream_id` and `(stream_id, byte_bus_id)` endpoint lookup) lives in
//! [`crate::addressing`]. See "Provenance note" below for what this module
//! does and does not claim about *when* in a request/response lifecycle the
//! rule is enforced.
//!
//! ## Canonical wire layout (TC18 v0.5.1_RC §11.2.1 Figure 7 / Table 4)
//!
//! `byte_message_info` is 8 octets, laid out as two 32-bit words:
//!
//! Row 1 (octets 0-3):
//! - `acf_msg_type` — 7 bits (octet 0 bits 7:1)
//! - `acf_msg_length` — 9 bits, in **quadlets** (octet 0 bit 0 = MSB,
//!   octet 1 bits 7:0 = low 8 bits)
//! - `pad` — 2 bits (octet 2 bits 7:6) — a *count* of padding octets
//!   appended after the payload to round the message up to a whole
//!   quadlet, not a presence flag
//! - `mtv` — 1 bit (octet 2 bit 5)
//! - `rsv` — 2 bits (octet 2 bits 4:3), always zero, not a meaningful field
//! - `byte_bus_id` — 11 bits (octet 2 bits 2:0 = top 3 bits, octet 3 bits
//!   7:0 = low 8 bits)
//!
//! Row 2 (octets 4-7):
//! - `evt` — 4 bits (octet 4 bits 7:4): a 1-bit `ack` flag + 3-bit
//!   `sub_opcode`, see [`Evt`]
//! - `rsv` — 2 bits (octet 4 bits 3:2), always zero
//! - `hs` — 1 bit (octet 4 bit 1)
//! - `cs` — 1 bit (octet 4 bit 0)
//! - `transaction_num` — 8 bits (octet 5, full byte) — comes *before* the
//!   `op`/`rsp`/`err`/`ms` group
//! - `op` — 1 bit (octet 6 bit 7)
//! - `rsp` — 1 bit (octet 6 bit 6)
//! - `err` — 1 bit (octet 6 bit 5)
//! - `ms` — 1 bit (octet 6 bit 4)
//! - `read_size_or_segment_num` — 12 bits (octet 6 bits 3:0 = top 4 bits,
//!   octet 7 bits 7:0 = low 8 bits)
//!
//! `acf_msg_type = 0x0E` is ACF_ABB (no timestamp field); `0x0D` is
//! ACF_GBB (carries a 64-bit `message_timestamp` immediately after this
//! 8-byte header).
//!
//! `acf_msg_length` counts **quadlets over the entire ACF message** —
//! header (+ `message_timestamp` for ACF_GBB) + payload + pad, rounded up
//! to whole quadlets — not payload-only and not octets. This is confirmed
//! by the specification's own two worked examples (Figure 19: a single
//! ACF_ABB with an 8-byte header + 6 payload bytes + 2 pad bytes + 4-byte
//! CRC32 trailer = 20 bytes = 5 quadlets, `acf_msg_length = 0x05`;
//! Figure 20: a single ACF_GBB with an 8-byte header + 8-byte timestamp +
//! 7 payload bytes + 1 pad byte + 4-byte CRC32 trailer = 28 bytes =
//! 7 quadlets, `acf_msg_length = 0x07`). Both figures' real wire byte
//! order — header (+ `message_timestamp`), payload, `pad`, THEN the CRC32
//! trailer, pad strictly *before* the CRC — is pinned byte-for-byte by
//! [`crate::e2e::finalize_crc_trailer`]'s own golden-vector tests
//! (`finalize_crc_trailer_matches_figure_19_worked_example`/
//! `finalize_crc_trailer_matches_figure_20_worked_example`), not here: this
//! module's own `encode_acf_abb`/`encode_acf_gbb` have no CRC-trailer
//! concept of their own (see the "acf_msg_length quadlet semantics" note
//! below), so a byte-for-byte CRC-inclusive worked-example test belongs
//! with the module that actually assembles a CRC-protected frame out of
//! them, not here. An earlier revision of this module pinned both figures
//! locally instead, by concatenating `payload + crc_bytes` into one blob
//! before calling `encode_acf_abb`/`encode_acf_gbb` — which put the
//! encoder's own automatic `pad` after the CRC instead of before it (the
//! reversed, non-conformant order) while still passing, since it only
//! checked total length/quadlet-count/pad-count, all of which stay
//! identical either way. See `crate::e2e`'s "CRC trailer wire placement"
//! doc section for the fix and the full explanation.
//!
//! ## Provenance note
//!
//! An earlier revision of this module used an invented layout — self-
//! admitted in this module's own comments at the time — that treated
//! `acf_msg_type` as a standalone leading byte outside `byte_message_info`,
//! modeled `acf_msg_length`/`byte_bus_id` as a pair of 11-bit fields, `pad`
//! as a 1-bit presence flag, `evt` as living in row 1 instead of row 2,
//! `transaction_num` as coming *after* the `op`/`rsp`/`err`/`ms` flag group,
//! and `read_size`/`segment_num` as a full 16 bits. That layout was this
//! crate's own placeholder interpretation, not a transcription of TC18's
//! text, and has been fully replaced by the canonical layout documented
//! above (pixel-verified against the real TC18 v0.5.1_RC PDF, Figure 7 /
//! Table 4, and cross-checked against the two worked examples). This is a
//! **breaking wire-format change** — see `CHANGELOG.md`.
//!
//! - The `ReadSizeOrSegment` dual-purpose field's *selecting condition*:
//!   `byte_message_info`'s own `op` flag is the documented selector (RELAY
//!   specification §15.5's canonical `Message.ReadSizeOrSegment` doc
//!   comment states the rule directly: read this field as `read_size` when
//!   `op` indicates a read, `segment_num` otherwise).
//!   [`ByteMessageInfo::read_size`]/[`ByteMessageInfo::segment_num`] apply
//!   that selection, returning `None` on the side that does not match `op`
//!   rather than returning a value under the wrong interpretation.
//!   [`ReadSizeOrSegment::as_read_size`]/[`ReadSizeOrSegment::as_segment_num`]
//!   remain as unconditional, op-independent accessors for the field's own
//!   raw value — still useful where a caller already knows which
//!   interpretation applies from context other than a live
//!   `ByteMessageInfo.op` bit (e.g. `crate::uart`'s own `read_size`
//!   configuration field, which reuses this same type for a UART-local
//!   value that is never anything but a read size).
//! - This crate's `ROADMAP.md` states the echo-back rule itself (a
//!   response/ack must carry the same `byte_bus_id` it was received under)
//!   but not the mechanics of *when* it is checked against a live request/
//!   response exchange. [`build_response_info`]/[`verify_echo_back`] are
//!   deliberately plain functions over already-decoded [`ByteMessageInfo`]
//!   values, with no opinion on whether the real enforcement point ends up
//!   being at encode time, decode time, or purely an application-level
//!   helper a dispatch loop calls explicitly.

use crate::RcpError;

// ── byte_message_info ─────────────────────────────────────────────────────────

/// `acf_msg_type` is a 7-bit field; this is the maximum representable
/// value.
pub const ACF_MSG_TYPE_7BIT_MAX: u8 = 0x7F;

/// `acf_msg_length` is a 9-bit field (counted in quadlets over the entire
/// ACF message); this is the maximum representable value.
pub const ACF_MSG_LENGTH_9BIT_MAX: u16 = 0x01FF;

/// `pad` is a 2-bit field (a count of padding octets, not a presence
/// flag); this is the maximum representable value.
pub const PAD_2BIT_MAX: u8 = 0x03;

/// `byte_bus_id` is an 11-bit field; this is the maximum representable
/// value.
pub const BYTE_MESSAGE_INFO_11BIT_MAX: u16 = 0x07FF;

/// `evt.sub_opcode` is a 3-bit field; this is its maximum representable
/// value.
pub const EVT_SUB_OPCODE_MAX: u8 = 0x07;

/// `read_size_or_segment_num` is a 12-bit field; this is the maximum
/// representable value.
pub const READ_SIZE_SEGMENT_12BIT_MAX: u16 = 0x0FFF;

/// Length, in bytes, of the encoded `byte_message_info` header shared by
/// [`AcfAbbMessage`] and [`AcfGbbMessage`]. `acf_msg_type` is folded into
/// this 8-byte header (row 1, octet 0) — there is no separate leading
/// discriminant byte. See this module's "Canonical wire layout" doc
/// section.
pub const BYTE_MESSAGE_INFO_LEN: usize = 8;

/// The `evt` field: a 1-bit ack flag + 3-bit sub-opcode pair, packed into
/// row 2's 4-bit `evt` nibble (octet 4 bits 7:4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
// fusa:req REQ-BMI-001
pub struct Evt {
    /// The ack-flag bit of `evt`.
    pub ack: bool,
    /// The 3-bit sub-opcode of `evt`. Valid range is `0..=EVT_SUB_OPCODE_MAX`.
    pub sub_opcode: u8,
}

/// The dual-purpose 12-bit field TC18 Table 4 calls
/// `read_size_or_segment_num` (row 2, octet 6 bits 3:0 + octet 7) — a
/// requested read byte count when the enclosing [`ByteMessageInfo::op`]
/// flag indicates a read, or a fragment train's segment index otherwise.
///
/// This type carries the field's raw value unconditionally;
/// [`ReadSizeOrSegment::as_read_size`]/[`ReadSizeOrSegment::as_segment_num`]
/// are plain, op-independent views of that same value, useful when a caller
/// already knows which interpretation applies from context other than a
/// live `ByteMessageInfo.op` bit (e.g. `crate::uart`'s own `read_size`
/// configuration field, which reuses this type for a UART-local value that
/// is never anything but a read size). A caller reading this field out of
/// an actual decoded [`ByteMessageInfo`] should prefer
/// [`ByteMessageInfo::read_size`]/[`ByteMessageInfo::segment_num`] instead,
/// which apply the `op`-bit selection this module's provenance note
/// describes rather than assuming one interpretation unconditionally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
// fusa:req REQ-BMI-001
pub struct ReadSizeOrSegment(pub u16);

impl ReadSizeOrSegment {
    /// View this field's raw value as a `read_size` value (a requested read
    /// byte count), unconditionally — see the struct doc comment for when
    /// this unconditional view is and is not the right one to reach for.
    pub fn as_read_size(self) -> u16 {
        self.0
    }

    /// View this field's raw value as a `segment_num` value (a fragment
    /// index), unconditionally — see the struct doc comment for when this
    /// unconditional view is and is not the right one to reach for.
    pub fn as_segment_num(self) -> u16 {
        self.0
    }
}

/// Decoded `byte_message_info` header, shared by [`AcfAbbMessage`] and
/// [`AcfGbbMessage`]. See this module's "Canonical wire layout" doc
/// section for the exact bit-for-bit packing.
///
/// `byte_bus_id` is carried here as an opaque 11-bit value only — this
/// module does not implement `(stream_id, byte_bus_id)` addressing or the
/// echo-back rule; those are the separate "Addressing" checklist item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
// fusa:req REQ-BMI-001
pub struct ByteMessageInfo {
    /// The ACF message-type discriminant (`ACF_ABB_MSG_TYPE`/
    /// `ACF_GBB_MSG_TYPE`). 7 bits; valid range is
    /// `0..=ACF_MSG_TYPE_7BIT_MAX`. [`encode_acf_abb`]/[`encode_acf_gbb`]
    /// always overwrite this with the correct discriminant for the message
    /// type being encoded, so a caller does not need to set it by hand for
    /// those two entry points.
    pub acf_msg_type: u8,
    /// Length, in quadlets, of the *entire* ACF message this header
    /// belongs to (header +, for ACF_GBB, `message_timestamp` + payload +
    /// `pad`). 9 bits; valid range is `0..=ACF_MSG_LENGTH_9BIT_MAX`. See
    /// this module's "Canonical wire layout" doc section for the Figure
    /// 19/20 worked-example derivation.
    pub acf_msg_length: u16,
    /// Count of padding octets appended after the payload to round the
    /// whole ACF message up to a quadlet boundary. 2 bits; valid range is
    /// `0..=PAD_2BIT_MAX`. Unlike the layout this module used before this
    /// item, this is a *count*, not a presence flag.
    pub pad: u8,
    /// Message-timestamp-valid flag. Shared across both ACF_ABB and
    /// ACF_GBB even though only ACF_GBB has a `message_timestamp` field to
    /// validate — see this module's provenance note.
    pub mtv: bool,
    /// Bus-relative endpoint id. 11 bits; valid range is
    /// `0..=BYTE_MESSAGE_INFO_11BIT_MAX`. Stream-relative, not global — see
    /// the "Addressing" checklist item this module does not implement.
    pub byte_bus_id: u16,
    /// Ack flag + 3-bit sub-opcode. See [`Evt`].
    pub evt: Evt,
    /// Handshake flag.
    pub hs: bool,
    /// Checksum-present flag.
    pub cs: bool,
    /// Per-transaction correlation id.
    pub transaction_num: u8,
    /// Operation flag.
    pub op: bool,
    /// Response flag.
    pub rsp: bool,
    /// Error flag.
    pub err: bool,
    /// More-segments flag.
    pub ms: bool,
    /// The dual-purpose `read_size`/`segment_num` field. 12 bits; valid
    /// range is `0..=READ_SIZE_SEGMENT_12BIT_MAX`. See
    /// [`ReadSizeOrSegment`].
    pub read_size_segment: ReadSizeOrSegment,
}

impl ByteMessageInfo {
    /// This header's [`ReadSizeOrSegment`] field, read as a `read_size`
    /// value — `Some` when [`ByteMessageInfo::op`] indicates a read,
    /// `None` when it indicates a write (in which case the same field is a
    /// `segment_num`, see [`ByteMessageInfo::segment_num`]).
    ///
    /// This is the op-bit-gated selection this module's provenance note
    /// describes: unlike [`ReadSizeOrSegment::as_read_size`]'s
    /// unconditional view of the raw field, this method refuses to hand
    /// back a value under the interpretation `op` says does not apply.
    // fusa:req REQ-BMI-005
    pub fn read_size(&self) -> Option<u16> {
        if self.op {
            None
        } else {
            Some(self.read_size_segment.as_read_size())
        }
    }

    /// This header's [`ReadSizeOrSegment`] field, read as a `segment_num`
    /// value — `Some` when [`ByteMessageInfo::op`] indicates a write,
    /// `None` when it indicates a read (in which case the same field is a
    /// `read_size`, see [`ByteMessageInfo::read_size`]).
    ///
    /// See [`ByteMessageInfo::read_size`]'s doc comment for why this is the
    /// preferred accessor over [`ReadSizeOrSegment::as_segment_num`]'s
    /// unconditional view.
    // fusa:req REQ-BMI-005
    pub fn segment_num(&self) -> Option<u16> {
        if self.op {
            Some(self.read_size_segment.as_segment_num())
        } else {
            None
        }
    }
}

/// Encode a [`ByteMessageInfo`] to its 8-byte wire representation, per this
/// module's "Canonical wire layout" doc section.
///
/// Returns `Err(RcpError::InvalidSize)` if any field exceeds its bit
/// width (`acf_msg_type`: 7 bits, `acf_msg_length`: 9 bits, `pad`: 2 bits,
/// `byte_bus_id`: 11 bits, `evt.sub_opcode`: 3 bits, `read_size_segment`:
/// 12 bits).
// fusa:req REQ-BMI-002
// fusa:req REQ-BMI-003
pub fn encode_byte_message_info(
    info: &ByteMessageInfo,
) -> Result<[u8; BYTE_MESSAGE_INFO_LEN], RcpError> {
    if info.acf_msg_type > ACF_MSG_TYPE_7BIT_MAX
        || info.acf_msg_length > ACF_MSG_LENGTH_9BIT_MAX
        || info.pad > PAD_2BIT_MAX
        || info.byte_bus_id > BYTE_MESSAGE_INFO_11BIT_MAX
        || info.evt.sub_opcode > EVT_SUB_OPCODE_MAX
        || info.read_size_segment.0 > READ_SIZE_SEGMENT_12BIT_MAX
    {
        return Err(RcpError::InvalidSize);
    }

    let mut buf = [0u8; BYTE_MESSAGE_INFO_LEN];

    // octet 0: acf_msg_type[6:0] in bits 7:1, acf_msg_length[8] (MSB) in bit 0.
    buf[0] = (info.acf_msg_type << 1) | (((info.acf_msg_length >> 8) & 0x1) as u8);
    // octet 1: acf_msg_length[7:0].
    buf[1] = (info.acf_msg_length & 0xFF) as u8;

    // octet 2: pad[1:0] in bits 7:6, mtv in bit 5, rsv(2 bits, zero) in
    // bits 4:3, byte_bus_id[10:8] in bits 2:0.
    buf[2] = (info.pad << 6) | ((info.mtv as u8) << 5) | (((info.byte_bus_id >> 8) & 0x7) as u8);
    // octet 3: byte_bus_id[7:0].
    buf[3] = (info.byte_bus_id & 0xFF) as u8;

    // octet 4: evt (ack:1 + sub_opcode:3) in bits 7:4, rsv(2 bits, zero) in
    // bits 3:2, hs in bit 1, cs in bit 0.
    let evt_bits = ((info.evt.ack as u8) << 3) | (info.evt.sub_opcode & 0x7);
    buf[4] = (evt_bits << 4) | ((info.hs as u8) << 1) | (info.cs as u8);

    // octet 5: transaction_num, full byte — comes before the op/rsp/err/ms
    // group, per Table 4's row-2 ordering.
    buf[5] = info.transaction_num;

    // octet 6: op in bit 7, rsp in bit 6, err in bit 5, ms in bit 4,
    // read_size_segment[11:8] in bits 3:0.
    buf[6] = ((info.op as u8) << 7)
        | ((info.rsp as u8) << 6)
        | ((info.err as u8) << 5)
        | ((info.ms as u8) << 4)
        | (((info.read_size_segment.0 >> 8) & 0xF) as u8);
    // octet 7: read_size_segment[7:0].
    buf[7] = (info.read_size_segment.0 & 0xFF) as u8;

    Ok(buf)
}

/// Decode a [`ByteMessageInfo`] from a byte slice, per this module's
/// "Canonical wire layout" doc section.
///
/// Never panics on short, truncated, or arbitrary input — always returns
/// `Err(RcpError::ShortFrame)` for input shorter than
/// [`BYTE_MESSAGE_INFO_LEN`] instead.
// fusa:req REQ-BMI-002
// fusa:req REQ-BMI-004
pub fn decode_byte_message_info(b: &[u8]) -> Result<ByteMessageInfo, RcpError> {
    if b.len() < BYTE_MESSAGE_INFO_LEN {
        return Err(RcpError::ShortFrame);
    }

    let acf_msg_type = b[0] >> 1;
    let acf_msg_length = (u16::from(b[0] & 0x1) << 8) | u16::from(b[1]);

    let pad = b[2] >> 6;
    let mtv = (b[2] >> 5) & 0x1 != 0;
    let byte_bus_id = (u16::from(b[2] & 0x7) << 8) | u16::from(b[3]);

    let evt_bits = b[4] >> 4;
    let evt = Evt {
        ack: (evt_bits >> 3) & 0x1 != 0,
        sub_opcode: evt_bits & 0x7,
    };
    let hs = (b[4] >> 1) & 0x1 != 0;
    let cs = b[4] & 0x1 != 0;

    let transaction_num = b[5];

    let op = (b[6] >> 7) & 0x1 != 0;
    let rsp = (b[6] >> 6) & 0x1 != 0;
    let err = (b[6] >> 5) & 0x1 != 0;
    let ms = (b[6] >> 4) & 0x1 != 0;
    let read_size_segment = ReadSizeOrSegment((u16::from(b[6] & 0xF) << 8) | u16::from(b[7]));

    Ok(ByteMessageInfo {
        acf_msg_type,
        acf_msg_length,
        pad,
        mtv,
        byte_bus_id,
        evt,
        hs,
        cs,
        transaction_num,
        op,
        rsp,
        err,
        ms,
        read_size_segment,
    })
}

// ── Constants shared by both ACF message types ────────────────────────────────

/// `acf_msg_type` discriminant identifying an ACF_ABB message.
pub const ACF_ABB_MSG_TYPE: u8 = 0x0E;

/// `acf_msg_type` discriminant identifying an ACF_GBB message.
pub const ACF_GBB_MSG_TYPE: u8 = 0x0D;

/// Length, in bytes, of the ACF_ABB message header: just
/// `byte_message_info` — `acf_msg_type` is folded into it, not a separate
/// leading byte (see this module's "Canonical wire layout" doc section).
/// Deliberately *not* [`ACF_GBB_HEADER_LEN`]-wide: unlike ACF_GBB, ACF_ABB
/// has no `message_timestamp` region at all, so there is no reserved gap
/// sized for one.
pub const ACF_ABB_HEADER_LEN: usize = BYTE_MESSAGE_INFO_LEN;

/// Length, in bytes, of the ACF_GBB message header: `byte_message_info`
/// plus the 8-byte `message_timestamp`.
pub const ACF_GBB_HEADER_LEN: usize = BYTE_MESSAGE_INFO_LEN + 8;

// ── acf_msg_length quadlet semantics ──────────────────────────────────────────
//
// TC18 §11.2.1 Table 4 describes `acf_msg_length` as a count of quadlets
// over the *entire* ACF message — header (+ message_timestamp for
// ACF_GBB) + payload + pad — confirmed by the specification's own Figure
// 19 (ACF_ABB: 8-byte header + 6 payload + 2 pad + 4-byte CRC32 = 20 bytes
// = 5 quadlets) and Figure 20 (ACF_GBB: 8-byte header + 8-byte timestamp +
// 7 payload + 1 pad + 4-byte CRC32 = 28 bytes = 7 quadlets) worked
// examples. `pad` is the number of zero octets appended after `payload` to
// round the message up to a whole quadlet; this crate's encoders compute
// and append it automatically, and its decoders strip it back off using
// the decoded `pad` count rather than assuming it is always zero.
//
// Note: if a caller wants a CRC32 trailer counted into `acf_msg_length`
// (as both worked examples do), it supplies those trailer bytes as part of
// `payload` itself — this module has no separate CRC-trailer field of its
// own (CRC computation itself lives in `crate::e2e`).

/// Number of bytes in one quadlet — the unit `acf_msg_length` counts in.
pub const QUADLET_LEN: usize = 4;

/// Derive the `(acf_msg_length, pad)` pair for an ACF message whose header
/// is `header_len` bytes and whose payload is `payload_len` bytes:
/// `acf_msg_length` is the total octet count (`header_len + payload_len`,
/// rounded up to the nearest whole quadlet) expressed in quadlets, and
/// `pad` is how many zero octets that rounding requires.
///
/// Returns `Err(RcpError::InvalidSize)` if the resulting `acf_msg_length`
/// would not fit the field's 9-bit width
/// ([`ACF_MSG_LENGTH_9BIT_MAX`]).
fn quadlets_and_pad_for_message(
    header_len: usize,
    payload_len: usize,
) -> Result<(u16, u8), RcpError> {
    let total = header_len + payload_len;
    let remainder = total % QUADLET_LEN;
    let pad = if remainder == 0 {
        0
    } else {
        QUADLET_LEN - remainder
    };
    let quadlets = (total + pad) / QUADLET_LEN;
    let acf_msg_length = u16::try_from(quadlets)
        .ok()
        .filter(|&q| q <= ACF_MSG_LENGTH_9BIT_MAX)
        .ok_or(RcpError::InvalidSize)?;
    Ok((acf_msg_length, pad as u8))
}

/// Given an already-decoded `info` (whose `acf_msg_length`/`pad` describe
/// the *whole* ACF message, header included) and the full byte slice `b`
/// starting at that message's first octet, return `(payload, consumed)`:
/// the message's real payload bytes (with the trailing `pad` octets
/// stripped back off) and the total number of bytes this one ACF message
/// occupies in `b` (`header_len + region_len`) — the offset a caller
/// splitting multiple concatenated ACF messages out of one frame (TC18
/// §12.9.1.1) should advance by to reach the next message, if any.
///
/// Returns `Err(RcpError::ShortFrame)` if `b` does not actually contain
/// `header_len + region_len` bytes, and `Err(RcpError::InvalidSize)` if
/// `info.acf_msg_length`/`info.pad` describe a message shorter than
/// `header_len` or shorter than `header_len + pad` — both indicate a
/// self-inconsistent header rather than merely a short read.
fn take_message_bytes<'a>(
    b: &'a [u8],
    header_len: usize,
    info: &ByteMessageInfo,
) -> Result<(&'a [u8], usize), RcpError> {
    let total_octets = info.acf_msg_length as usize * QUADLET_LEN;
    let region_len = total_octets
        .checked_sub(header_len)
        .ok_or(RcpError::InvalidSize)?;
    let payload_len = region_len
        .checked_sub(info.pad as usize)
        .ok_or(RcpError::InvalidSize)?;
    let consumed = header_len + region_len;
    if b.len() < consumed {
        return Err(RcpError::ShortFrame);
    }
    let payload = &b[header_len..header_len + payload_len];
    Ok((payload, consumed))
}

// ── AcfAbbMessage ─────────────────────────────────────────────────────────────

/// Decoded ACF_ABB message.
///
/// There is intentionally no `timestamp` field on this struct at all — see
/// the module doc comment's opening summary of ACF_ABB.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
// fusa:req REQ-ABB-001
pub struct AcfAbbMessage {
    /// The shared `byte_message_info` header. See [`ByteMessageInfo`].
    pub info: ByteMessageInfo,
    /// Opaque bytes following `byte_message_info`. This module does not
    /// parse any further internal structure of an ACF_ABB payload.
    pub payload: Vec<u8>,
}

/// Encode an [`AcfAbbMessage`] to its wire representation.
///
/// The result is always exactly `ACF_ABB_HEADER_LEN + msg.payload.len() +
/// pad` bytes: `byte_message_info` (with `acf_msg_type` forced to
/// [`ACF_ABB_MSG_TYPE`]), `payload` verbatim, then `pad` zero octets
/// rounding the whole message up to a quadlet boundary — never a
/// timestamp region of any width.
///
/// `msg.info.acf_msg_type`/`acf_msg_length`/`pad` are *not* trusted
/// verbatim: this function always derives and overwrites them
/// ([`ACF_ABB_MSG_TYPE`]; [`quadlets_and_pad_for_message`] over
/// `ACF_ABB_HEADER_LEN + msg.payload.len()`), so the emitted frame's
/// header is always self-consistent with what it actually describes.
/// Returns `Err(RcpError::InvalidSize)` if `msg.payload.len()` doesn't fit
/// the 9-bit quadlet-count range, or if `msg.info` (with those three
/// fields so overwritten) otherwise fails
/// [`encode_byte_message_info`]'s field-width validation.
// fusa:req REQ-ABB-002
// fusa:req REQ-ABB-003
pub fn encode_acf_abb(msg: &AcfAbbMessage) -> Result<Vec<u8>, RcpError> {
    let (acf_msg_length, pad) =
        quadlets_and_pad_for_message(ACF_ABB_HEADER_LEN, msg.payload.len())?;
    let info = ByteMessageInfo {
        acf_msg_type: ACF_ABB_MSG_TYPE,
        acf_msg_length,
        pad,
        ..msg.info
    };
    let info_bytes = encode_byte_message_info(&info)?;
    let mut buf = Vec::with_capacity(ACF_ABB_HEADER_LEN + msg.payload.len() + pad as usize);
    buf.extend_from_slice(&info_bytes);
    buf.extend_from_slice(&msg.payload);
    buf.extend(std::iter::repeat(0u8).take(pad as usize));
    Ok(buf)
}

/// Decode an [`AcfAbbMessage`] from a byte slice.
///
/// Never panics on short, truncated, or arbitrary input — always returns
/// `Err` instead. Only consumes the bytes this one ACF message's own
/// `acf_msg_length` describes (see [`take_message_bytes`]); any bytes in
/// `b` past that point (e.g. a second concatenated ACF message, TC18
/// §12.9.1.1) are left unread rather than folded into `payload` or
/// rejected — see [`decode_acf_abb_messages`] for splitting a frame that
/// carries more than one.
// fusa:req REQ-ABB-002
// fusa:req REQ-ABB-004
// fusa:req REQ-ABB-005
pub fn decode_acf_abb(b: &[u8]) -> Result<AcfAbbMessage, RcpError> {
    if b.len() < ACF_ABB_HEADER_LEN {
        return Err(RcpError::ShortFrame);
    }
    let info = decode_byte_message_info(&b[..ACF_ABB_HEADER_LEN])?;
    if info.acf_msg_type != ACF_ABB_MSG_TYPE {
        return Err(wrong_discriminant_error(
            "acf_abb",
            ACF_ABB_MSG_TYPE,
            info.acf_msg_type,
            ACF_GBB_MSG_TYPE,
            "ACF_GBB",
        ));
    }
    let (payload, _consumed) = take_message_bytes(b, ACF_ABB_HEADER_LEN, &info)?;
    Ok(AcfAbbMessage {
        info,
        payload: payload.to_vec(),
    })
}

/// Split zero or more concatenated ACF_ABB messages out of `b`, per TC18
/// §12.9.1.1 ("Handling multiple requests in incoming messages"): "An RC
/// Server shall support to handle multiple requests in one frame and check
/// each of them individually if to be processed or not."
///
/// Each message's own `acf_msg_length` (via [`take_message_bytes`]) is the
/// delimiter — there is no outer count or separator. Returns
/// `Err(RcpError::ShortFrame)` if `b` is empty or ends mid-message, and
/// propagates whatever error the first malformed message in the sequence
/// produces (a message after a malformed one is never reached). An empty
/// `b` is rejected the same way [`decode_acf_abb`] rejects it, rather than
/// silently returning zero messages, so a caller cannot mistake "nothing
/// to parse" for "one legitimately-empty-payload message".
// fusa:req REQ-ABB-004
pub fn decode_acf_abb_messages(b: &[u8]) -> Result<Vec<AcfAbbMessage>, RcpError> {
    if b.is_empty() {
        return Err(RcpError::ShortFrame);
    }
    let mut messages = Vec::new();
    let mut offset = 0usize;
    while offset < b.len() {
        let remaining = &b[offset..];
        if remaining.len() < ACF_ABB_HEADER_LEN {
            return Err(RcpError::ShortFrame);
        }
        let info = decode_byte_message_info(&remaining[..ACF_ABB_HEADER_LEN])?;
        if info.acf_msg_type != ACF_ABB_MSG_TYPE {
            return Err(wrong_discriminant_error(
                "acf_abb",
                ACF_ABB_MSG_TYPE,
                info.acf_msg_type,
                ACF_GBB_MSG_TYPE,
                "ACF_GBB",
            ));
        }
        let (payload, consumed) = take_message_bytes(remaining, ACF_ABB_HEADER_LEN, &info)?;
        messages.push(AcfAbbMessage {
            info,
            payload: payload.to_vec(),
        });
        offset += consumed;
    }
    Ok(messages)
}

// ── AcfGbbMessage ─────────────────────────────────────────────────────────────

/// Decoded ACF_GBB message.
///
/// `message_timestamp` is carried here as a raw 64-bit value only — this
/// module does not implement its rollover period or the all-zero-timestamp
/// "treat as untimed" fallback rule; those are the separate "Timestamp
/// Semantics" checklist item, implemented in
/// [`crate::timestamp::MessageTimestamp`] as a standalone wrapper over this
/// field's raw value. For a GBB *conditional* request specifically,
/// [`crate::request::RequestKind::from_gbb_message_timestamp`]/
/// [`crate::request::RequestKind::to_gbb_message_timestamp`] additionally
/// read/write this field's leading (most significant) byte as a
/// [`crate::request::RequestKind`] discriminant — see that pair's own doc
/// comment and [`crate::request`]'s module doc comment "Provenance note:
/// `RequestKind`'s wire placement" for the full reasoning. This module
/// itself stays unaware of that meaning: `message_timestamp` is still
/// encoded/decoded here as one opaque `u64`, with no `RequestKind`-specific
/// handling of its own.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
// fusa:req REQ-GBB-001
pub struct AcfGbbMessage {
    /// The shared `byte_message_info` header. See [`ByteMessageInfo`].
    pub info: ByteMessageInfo,
    /// Raw 64-bit message timestamp. See the struct-level doc comment for
    /// what semantics this module does not apply to it, and
    /// [`crate::timestamp::MessageTimestamp`] for where those semantics
    /// live.
    pub message_timestamp: u64,
    /// Opaque bytes following `message_timestamp`. This module does not
    /// parse any further internal structure of an ACF_GBB payload.
    pub payload: Vec<u8>,
}

/// Encode an [`AcfGbbMessage`] to its wire representation.
///
/// The result is always exactly `ACF_GBB_HEADER_LEN + msg.payload.len() +
/// pad` bytes: `byte_message_info` (with `acf_msg_type` forced to
/// [`ACF_GBB_MSG_TYPE`]), the 8-byte `message_timestamp`, `payload`
/// verbatim, then `pad` zero octets rounding the whole message up to a
/// quadlet boundary.
///
/// `msg.info.acf_msg_type`/`acf_msg_length`/`pad` are *not* trusted
/// verbatim — see [`encode_acf_abb`]'s doc comment for the same rule,
/// applied here over `ACF_GBB_HEADER_LEN + msg.payload.len()`. Returns
/// `Err(RcpError::InvalidSize)` if `msg.payload.len()` doesn't fit the
/// 9-bit quadlet-count range, or if `msg.info` (with those three fields so
/// overwritten) otherwise fails [`encode_byte_message_info`]'s
/// field-width validation.
// fusa:req REQ-GBB-002
// fusa:req REQ-GBB-003
pub fn encode_acf_gbb(msg: &AcfGbbMessage) -> Result<Vec<u8>, RcpError> {
    let (acf_msg_length, pad) =
        quadlets_and_pad_for_message(ACF_GBB_HEADER_LEN, msg.payload.len())?;
    let info = ByteMessageInfo {
        acf_msg_type: ACF_GBB_MSG_TYPE,
        acf_msg_length,
        pad,
        ..msg.info
    };
    let info_bytes = encode_byte_message_info(&info)?;
    let mut buf = Vec::with_capacity(ACF_GBB_HEADER_LEN + msg.payload.len() + pad as usize);
    buf.extend_from_slice(&info_bytes);
    buf.extend_from_slice(&msg.message_timestamp.to_be_bytes());
    buf.extend_from_slice(&msg.payload);
    buf.extend(std::iter::repeat(0u8).take(pad as usize));
    Ok(buf)
}

/// Decode an [`AcfGbbMessage`] from a byte slice.
///
/// Never panics on short, truncated, or arbitrary input — always returns
/// `Err` instead. Only consumes the bytes this one ACF message's own
/// `acf_msg_length` describes — see [`decode_acf_abb`]'s doc comment for
/// the same rule, applied here over the region following
/// `message_timestamp`.
// fusa:req REQ-GBB-002
// fusa:req REQ-GBB-004
// fusa:req REQ-GBB-005
pub fn decode_acf_gbb(b: &[u8]) -> Result<AcfGbbMessage, RcpError> {
    if b.len() < ACF_GBB_HEADER_LEN {
        return Err(RcpError::ShortFrame);
    }
    let info = decode_byte_message_info(&b[..BYTE_MESSAGE_INFO_LEN])?;
    if info.acf_msg_type != ACF_GBB_MSG_TYPE {
        return Err(wrong_discriminant_error(
            "acf_gbb",
            ACF_GBB_MSG_TYPE,
            info.acf_msg_type,
            ACF_ABB_MSG_TYPE,
            "ACF_ABB",
        ));
    }

    let mut ts_bytes = [0u8; 8];
    ts_bytes.copy_from_slice(&b[BYTE_MESSAGE_INFO_LEN..ACF_GBB_HEADER_LEN]);
    let message_timestamp = u64::from_be_bytes(ts_bytes);

    let (payload, _consumed) = take_message_bytes(b, ACF_GBB_HEADER_LEN, &info)?;

    Ok(AcfGbbMessage {
        info,
        message_timestamp,
        payload: payload.to_vec(),
    })
}

/// Shared "wrong discriminant, maybe it's the sibling type" error builder
/// for [`decode_acf_abb`]/[`decode_acf_gbb`].
fn wrong_discriminant_error(
    context: &str,
    expected: u8,
    got: u8,
    sibling_value: u8,
    sibling_name: &str,
) -> RcpError {
    let hint = if got == sibling_value {
        format!(" (that's {sibling_name}'s discriminant, not decodable by this function)")
    } else {
        String::new()
    };
    RcpError::Other(format!(
        "{context}: expected acf_msg_type 0x{expected:02X}, got 0x{got:02X}{hint}"
    ))
}

// ── Echo-back rule ────────────────────────────────────────────────────────────

/// Build a response/ack `byte_message_info` header that echoes `request`'s
/// `byte_bus_id`, per Milestone 1's "Addressing" echo-back rule.
///
/// `response` is a caller-populated header for the outgoing response/ack —
/// every field except `byte_bus_id` and `rsp` passes through unchanged. This
/// overwrites `response.byte_bus_id` with `request.byte_bus_id` and forces
/// `response.rsp = true` (a response/ack is, definitionally, a response),
/// then returns the result.
///
/// This does not validate field widths — that remains
/// [`encode_byte_message_info`]'s job at encode time — and does not encode
/// anything itself; it operates purely on already-decoded [`ByteMessageInfo`]
/// values. It also does not inspect `request.rsp`, so it is safe to call
/// even if `request` is itself already a decoded response; rejecting that
/// shape, if ever needed, is a separate concern for whichever later
/// milestone builds the full request/response dispatch.
// fusa:req REQ-ECHO-001
// fusa:req REQ-ECHO-002
pub fn build_response_info(
    request: &ByteMessageInfo,
    mut response: ByteMessageInfo,
) -> ByteMessageInfo {
    response.byte_bus_id = request.byte_bus_id;
    response.rsp = true;
    response
}

/// Verify that an already-built response/ack `byte_message_info` header
/// echoes the `byte_bus_id` it was received under, per Milestone 1's
/// "Addressing" echo-back rule.
///
/// Returns `Err(RcpError::EpError)` if `response.byte_bus_id !=
/// request.byte_bus_id`. Deliberately checks nothing else about either
/// header — in particular, it does not require `response.rsp` to be set,
/// since that is a separate concern from the byte_bus_id-echoing rule this
/// function checks. Never panics: both inputs are already-decoded values,
/// not raw bytes, so there is no truncated-input shape to reject.
// fusa:req REQ-ECHO-001
// fusa:req REQ-ECHO-003
// fusa:req REQ-ECHO-004
pub fn verify_echo_back(
    request: &ByteMessageInfo,
    response: &ByteMessageInfo,
) -> Result<(), RcpError> {
    if response.byte_bus_id != request.byte_bus_id {
        return Err(RcpError::EpError);
    }
    Ok(())
}

// ── Wire-level error responses (rust-RCP-W04) ─────────────────────────────────

/// Build a wire-level `err=1` ACF_ABB error response for `request`, given
/// the error that occurred processing it — TC18 §12.9.6 "Handling errors":
/// "The error response shall contain the byte_bus_id and transaction
/// number of the request. The error response shall contain a
/// byte_msg_payload with an error code."
///
/// Returns `None` if `error` has no TC18 Table 27 wire code
/// ([`RcpError::tc18_wire_code`]) — there is no meaningful `err=1` response
/// to build for an error TC18 itself does not name (e.g. a transport-level
/// [`RcpError::Timeout`] or [`RcpError::ShortFrame`] from a frame this
/// crate could not even decode far enough to learn a `byte_bus_id`/
/// `transaction_num` to echo). Callers (e.g.
/// [`crate::mock::RcServer::handle_ntscf_frame`]/
/// [`crate::udp::UdpRcServer::serve_one`]) are expected to fall back to
/// propagating the original error to their own caller when this returns
/// `None`, rather than silently dropping it.
///
/// `byte_bus_id`/`transaction_num` are echoed from `request` (via
/// [`build_response_info`], the same echo-back rule every other response
/// in this crate uses), `rsp`/`err` are forced to `true`/`true`, and the
/// response payload is the single Table 27 error-code octet. TC18 does not
/// spell out `byte_msg_payload`'s exact byte layout for an error response
/// beyond "contains an error code" — this one-octet encoding is this
/// crate's own working interpretation (Guiding Principle 5), matching the
/// simplest reading of that text, and flagged here for reconciliation
/// against real TC18 behavior before being relied on for interop.
pub fn build_error_response(request: &ByteMessageInfo, error: &RcpError) -> Option<AcfAbbMessage> {
    let code = error.tc18_wire_code()?;
    let mut info = build_response_info(request, *request);
    info.err = true;
    Some(AcfAbbMessage {
        info,
        payload: vec![code],
    })
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    fn sample_info() -> ByteMessageInfo {
        ByteMessageInfo {
            acf_msg_type: ACF_ABB_MSG_TYPE,
            acf_msg_length: 0x0155,
            pad: 0x2,
            mtv: false,
            byte_bus_id: 0x0123,
            evt: Evt {
                ack: true,
                sub_opcode: 0x5,
            },
            hs: false,
            cs: true,
            transaction_num: 0x42,
            op: true,
            rsp: false,
            err: true,
            ms: false,
            read_size_segment: ReadSizeOrSegment(0x099),
        }
    }

    // ── byte_message_info ──────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-BMI-001
    // fusa:test REQ-BMI-002
    fn byte_message_info_round_trip() {
        let info = sample_info();
        let frame = encode_byte_message_info(&info).unwrap();
        assert_eq!(frame.len(), BYTE_MESSAGE_INFO_LEN);
        let decoded = decode_byte_message_info(&frame).unwrap();
        assert_eq!(decoded, info);
    }

    #[test]
    // fusa:test REQ-BMI-002
    fn byte_message_info_round_trip_zero_values() {
        let info = ByteMessageInfo::default();
        let frame = encode_byte_message_info(&info).unwrap();
        let decoded = decode_byte_message_info(&frame).unwrap();
        assert_eq!(decoded, info);
    }

    #[test]
    // fusa:test REQ-BMI-002
    fn byte_message_info_round_trip_max_values() {
        let info = ByteMessageInfo {
            acf_msg_type: ACF_MSG_TYPE_7BIT_MAX,
            acf_msg_length: ACF_MSG_LENGTH_9BIT_MAX,
            pad: PAD_2BIT_MAX,
            mtv: true,
            byte_bus_id: BYTE_MESSAGE_INFO_11BIT_MAX,
            evt: Evt {
                ack: true,
                sub_opcode: EVT_SUB_OPCODE_MAX,
            },
            hs: true,
            cs: true,
            transaction_num: 0xFF,
            op: true,
            rsp: true,
            err: true,
            ms: true,
            read_size_segment: ReadSizeOrSegment(READ_SIZE_SEGMENT_12BIT_MAX),
        };
        let frame = encode_byte_message_info(&info).unwrap();
        let decoded = decode_byte_message_info(&frame).unwrap();
        assert_eq!(decoded, info);
    }

    #[test]
    // fusa:test REQ-BMI-003
    fn byte_message_info_encode_rejects_oversized_acf_msg_type() {
        let info = ByteMessageInfo {
            acf_msg_type: ACF_MSG_TYPE_7BIT_MAX + 1,
            ..Default::default()
        };
        assert_eq!(encode_byte_message_info(&info), Err(RcpError::InvalidSize));
    }

    #[test]
    // fusa:test REQ-BMI-003
    fn byte_message_info_encode_rejects_oversized_acf_msg_length() {
        let info = ByteMessageInfo {
            acf_msg_length: ACF_MSG_LENGTH_9BIT_MAX + 1,
            ..Default::default()
        };
        assert_eq!(encode_byte_message_info(&info), Err(RcpError::InvalidSize));
    }

    #[test]
    // fusa:test REQ-BMI-003
    fn byte_message_info_encode_rejects_oversized_pad() {
        let info = ByteMessageInfo {
            pad: PAD_2BIT_MAX + 1,
            ..Default::default()
        };
        assert_eq!(encode_byte_message_info(&info), Err(RcpError::InvalidSize));
    }

    #[test]
    // fusa:test REQ-BMI-003
    fn byte_message_info_encode_rejects_oversized_byte_bus_id() {
        let info = ByteMessageInfo {
            byte_bus_id: BYTE_MESSAGE_INFO_11BIT_MAX + 1,
            ..Default::default()
        };
        assert_eq!(encode_byte_message_info(&info), Err(RcpError::InvalidSize));
    }

    #[test]
    // fusa:test REQ-BMI-003
    fn byte_message_info_encode_rejects_oversized_sub_opcode() {
        let info = ByteMessageInfo {
            evt: Evt {
                ack: false,
                sub_opcode: EVT_SUB_OPCODE_MAX + 1,
            },
            ..Default::default()
        };
        assert_eq!(encode_byte_message_info(&info), Err(RcpError::InvalidSize));
    }

    #[test]
    // fusa:test REQ-BMI-003
    fn byte_message_info_encode_rejects_oversized_read_size_segment() {
        let info = ByteMessageInfo {
            read_size_segment: ReadSizeOrSegment(READ_SIZE_SEGMENT_12BIT_MAX + 1),
            ..Default::default()
        };
        assert_eq!(encode_byte_message_info(&info), Err(RcpError::InvalidSize));
    }

    #[test]
    // fusa:test REQ-BMI-004
    fn byte_message_info_decode_rejects_short_input() {
        assert_eq!(
            decode_byte_message_info(&[0u8; BYTE_MESSAGE_INFO_LEN - 1]),
            Err(RcpError::ShortFrame)
        );
    }

    #[test]
    // fusa:test REQ-BMI-004
    fn byte_message_info_decode_never_panics_across_lengths() {
        let mut state: u32 = 0xB111_5713;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for len in 0..32 {
            let buf: Vec<u8> = (0..len).map(|_| (next() & 0xFF) as u8).collect();
            let _ = decode_byte_message_info(&buf);
        }
    }

    // ── op-gated read_size/segment_num selection ────────────────────────────

    #[test]
    // fusa:test REQ-BMI-005
    fn byte_message_info_read_size_is_some_only_when_op_is_read() {
        let info = ByteMessageInfo {
            op: false,
            read_size_segment: ReadSizeOrSegment(42),
            ..Default::default()
        };
        assert_eq!(info.read_size(), Some(42));
        assert_eq!(info.segment_num(), None);
    }

    #[test]
    // fusa:test REQ-BMI-005
    fn byte_message_info_segment_num_is_some_only_when_op_is_write() {
        let info = ByteMessageInfo {
            op: true,
            read_size_segment: ReadSizeOrSegment(42),
            ..Default::default()
        };
        assert_eq!(info.segment_num(), Some(42));
        assert_eq!(info.read_size(), None);
    }

    #[test]
    // fusa:test REQ-BMI-005
    fn byte_message_info_read_size_segment_num_are_mutually_exclusive_across_op() {
        for op in [false, true] {
            let info = ByteMessageInfo {
                op,
                read_size_segment: ReadSizeOrSegment(0x0BEF),
                ..Default::default()
            };
            assert_ne!(info.read_size().is_some(), info.segment_num().is_some());
        }
    }

    // ── Canonical layout: bit-position pins ─────────────────────────────────
    //
    // Directly pins the octet-by-octet packing this module's "Canonical
    // wire layout" doc section describes, independent of round-trip tests
    // (which would still pass under a self-consistent but wrong layout).

    #[test]
    fn encode_pins_row1_octet_positions() {
        let info = ByteMessageInfo {
            acf_msg_type: 0x0E,     // 0b0001110
            acf_msg_length: 0x0155, // 0b0_0001_0101_0101 (9 bits: 1 0101 0101)
            pad: 0b10,
            mtv: true,
            byte_bus_id: 0x0123, // 0b010_0010_0011 (11 bits)
            ..Default::default()
        };
        let buf = encode_byte_message_info(&info).unwrap();
        // octet0: acf_msg_type<<1 | msg_length bit8. 0x0155 bit8 = 1.
        assert_eq!(buf[0], (0x0E << 1) | 0x1);
        // octet1: msg_length low 8 bits = 0x55.
        assert_eq!(buf[1], 0x55);
        // octet2: pad(2)<<6 | mtv<<5 | byte_bus_id[10:8].
        // 0x0123 = 0b1_0010_0011 -> bits[10:8] = 0b001.
        assert_eq!(buf[2], (0b10 << 6) | (1 << 5) | 0b001);
        // octet3: byte_bus_id[7:0] = 0x23.
        assert_eq!(buf[3], 0x23);
    }

    #[test]
    fn encode_pins_row2_octet_positions() {
        let info = ByteMessageInfo {
            evt: Evt {
                ack: true,
                sub_opcode: 0b101,
            },
            hs: true,
            cs: false,
            transaction_num: 0x77,
            op: true,
            rsp: false,
            err: true,
            ms: false,
            read_size_segment: ReadSizeOrSegment(0x0ABC),
            ..Default::default()
        };
        let buf = encode_byte_message_info(&info).unwrap();
        // octet4: evt(ack<<3|sub)<<4 | hs<<1 | cs.
        let evt_bits = (1 << 3) | 0b101;
        assert_eq!(buf[4], (evt_bits << 4) | (1 << 1));
        // octet5: transaction_num, BEFORE the op/rsp/err/ms group.
        assert_eq!(buf[5], 0x77);
        // octet6: op<<7|rsp<<6|err<<5|ms<<4 | read_size[11:8].
        assert_eq!(buf[6], (1 << 7) | (1 << 5) | 0x0A);
        // octet7: read_size[7:0].
        assert_eq!(buf[7], 0xBC);
    }

    // ── ACF_ABB round-trip ─────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-ABB-001
    // fusa:test REQ-ABB-002
    fn acf_abb_round_trip() {
        // acf_msg_type/acf_msg_length/pad are all derived/overwritten at
        // encode time (see encode_acf_abb's doc comment) — set them here to
        // the values that *will* be derived so this round-trip equality
        // holds meaningfully rather than by accident. header(8) + 5 payload
        // = 13 -> pad 3 -> 16 total -> 4 quadlets.
        let msg = AcfAbbMessage {
            info: ByteMessageInfo {
                acf_msg_type: ACF_ABB_MSG_TYPE,
                acf_msg_length: 4,
                pad: 3,
                ..sample_info()
            },
            payload: vec![0x11, 0x22, 0x33, 0x44, 0x55],
        };
        let frame = encode_acf_abb(&msg).unwrap();
        let decoded = decode_acf_abb(&frame).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    // fusa:test REQ-ABB-002
    fn acf_abb_round_trip_empty_payload() {
        // header(8) + 0 payload = 8 -> already quadlet-aligned -> pad 0 ->
        // 2 quadlets.
        let msg = AcfAbbMessage {
            info: ByteMessageInfo {
                acf_msg_type: ACF_ABB_MSG_TYPE,
                acf_msg_length: 2,
                pad: 0,
                ..Default::default()
            },
            payload: vec![],
        };
        let frame = encode_acf_abb(&msg).unwrap();
        assert_eq!(frame.len(), ACF_ABB_HEADER_LEN);
        let decoded = decode_acf_abb(&frame).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    // fusa:test REQ-ABB-002
    fn acf_abb_round_trip_large_payload() {
        // header(8) + 256 payload = 264 -> already quadlet-aligned -> pad 0
        // -> 66 quadlets.
        let msg = AcfAbbMessage {
            info: ByteMessageInfo {
                acf_msg_type: ACF_ABB_MSG_TYPE,
                acf_msg_length: 66,
                pad: 0,
                ..sample_info()
            },
            payload: (0..=255u16).map(|v| v as u8).collect(),
        };
        let frame = encode_acf_abb(&msg).unwrap();
        let decoded = decode_acf_abb(&frame).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    // fusa:test REQ-ABB-002
    // fusa:test REQ-ABB-003
    fn acf_abb_encoded_message_has_no_timestamp_region() {
        // The defining Milestone 1 constraint for ACF_ABB: unlike ACF_GBB's
        // 64-bit message_timestamp, there must be no reserved slot for a
        // timestamp at all.
        for payload_len in [0usize, 1, 7, 8, 9, 16, 64] {
            let msg = AcfAbbMessage {
                info: ByteMessageInfo::default(),
                payload: vec![0x00; payload_len],
            };
            let frame = encode_acf_abb(&msg).unwrap();
            assert_ne!(frame.len(), ACF_GBB_HEADER_LEN + payload_len);
            assert!(frame.len() >= ACF_ABB_HEADER_LEN + payload_len);
            assert!(frame.len() < ACF_ABB_HEADER_LEN + payload_len + QUADLET_LEN);
        }
    }

    #[test]
    // fusa:test REQ-ABB-003
    fn acf_abb_encoded_message_has_expected_discriminant() {
        let msg = AcfAbbMessage {
            info: ByteMessageInfo::default(),
            payload: vec![0xAA, 0xBB],
        };
        let frame = encode_acf_abb(&msg).unwrap();
        let decoded_info = decode_byte_message_info(&frame).unwrap();
        assert_eq!(decoded_info.acf_msg_type, ACF_ABB_MSG_TYPE);
    }

    #[test]
    // fusa:test REQ-ABB-002
    fn acf_abb_encode_propagates_byte_message_info_validation_error() {
        // acf_msg_type/acf_msg_length/pad are always overwritten by the
        // derived values before this validation runs, so this uses an
        // oversized byte_bus_id instead — a field encode_acf_abb does not
        // touch — to exercise the propagation of
        // encode_byte_message_info's own field-width validation.
        let msg = AcfAbbMessage {
            info: ByteMessageInfo {
                byte_bus_id: BYTE_MESSAGE_INFO_11BIT_MAX + 1,
                ..Default::default()
            },
            payload: vec![],
        };
        assert_eq!(encode_acf_abb(&msg), Err(RcpError::InvalidSize));
    }

    #[test]
    // fusa:test REQ-ABB-002
    fn acf_abb_encode_rejects_payload_too_large_for_the_quadlet_field() {
        // (ACF_MSG_LENGTH_9BIT_MAX + 1) quadlets' worth of bytes is one
        // quadlet past what the 9-bit acf_msg_length field can encode.
        let payload_len = (ACF_MSG_LENGTH_9BIT_MAX as usize + 1) * QUADLET_LEN;
        let msg = AcfAbbMessage {
            info: ByteMessageInfo::default(),
            payload: vec![0u8; payload_len],
        };
        assert_eq!(encode_acf_abb(&msg), Err(RcpError::InvalidSize));
    }

    // ── ACF_ABB decode rejection ────────────────────────────────────────────

    #[test]
    // fusa:test REQ-ABB-004
    fn acf_abb_decode_rejects_empty_input() {
        assert_eq!(decode_acf_abb(&[]), Err(RcpError::ShortFrame));
    }

    #[test]
    // fusa:test REQ-ABB-004
    fn acf_abb_decode_rejects_wrong_discriminant() {
        assert!(matches!(
            decode_acf_abb(&[0xFFu8; ACF_ABB_HEADER_LEN]),
            Err(RcpError::Other(_))
        ));
    }

    #[test]
    // fusa:test REQ-ABB-004
    fn acf_abb_decode_rejects_gbb_discriminant_with_specific_hint() {
        let msg = AcfGbbMessage {
            info: ByteMessageInfo::default(),
            message_timestamp: 0,
            payload: vec![],
        };
        let frame = encode_acf_gbb(&msg).unwrap();
        let err = decode_acf_abb(&frame).unwrap_err();
        match err {
            RcpError::Other(msg) => assert!(msg.contains("ACF_GBB")),
            other => panic!("expected RcpError::Other, got {other:?}"),
        }
    }

    #[test]
    // fusa:test REQ-ABB-004
    fn acf_abb_decode_rejects_truncated_byte_message_info() {
        let short = vec![0u8; BYTE_MESSAGE_INFO_LEN - 1];
        assert_eq!(decode_acf_abb(&short), Err(RcpError::ShortFrame));
    }

    // ── ACF_ABB fuzz-style: arbitrary bytes never panic ────────────────────

    #[test]
    // fusa:test REQ-ABB-005
    fn acf_abb_decode_never_panics_on_arbitrary_input() {
        let inputs: &[&[u8]] = &[&[], &[0x0E], &[0x0D], &[0xFF; 32], &[0x00; 32], &[0x0E; 64]];
        for input in inputs {
            let _ = decode_acf_abb(input);
        }
    }

    #[test]
    // fusa:test REQ-ABB-005
    fn acf_abb_decode_never_panics_on_random_lengths() {
        let mut state: u32 = 0x0E0D_0E0D;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for len in 0..64 {
            let buf: Vec<u8> = (0..len).map(|_| (next() & 0xFF) as u8).collect();
            let _ = decode_acf_abb(&buf);
        }
    }

    // ── decode_acf_abb_messages: splitting multiple ACF messages ───────────

    #[test]
    fn decode_acf_abb_messages_rejects_empty_input() {
        assert_eq!(decode_acf_abb_messages(&[]), Err(RcpError::ShortFrame));
    }

    #[test]
    fn decode_acf_abb_messages_splits_two_concatenated_messages() {
        let msg1 = AcfAbbMessage {
            info: ByteMessageInfo {
                transaction_num: 1,
                ..Default::default()
            },
            payload: vec![0xAA; 3],
        };
        let msg2 = AcfAbbMessage {
            info: ByteMessageInfo {
                transaction_num: 2,
                ..Default::default()
            },
            payload: vec![0xBB; 9],
        };
        let mut frame = encode_acf_abb(&msg1).unwrap();
        frame.extend_from_slice(&encode_acf_abb(&msg2).unwrap());

        let messages = decode_acf_abb_messages(&frame).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].info.transaction_num, 1);
        assert_eq!(messages[0].payload, vec![0xAA; 3]);
        assert_eq!(messages[1].info.transaction_num, 2);
        assert_eq!(messages[1].payload, vec![0xBB; 9]);
    }

    #[test]
    fn decode_acf_abb_messages_single_message_matches_decode_acf_abb() {
        let msg = AcfAbbMessage {
            info: ByteMessageInfo::default(),
            payload: vec![1, 2, 3, 4, 5],
        };
        let frame = encode_acf_abb(&msg).unwrap();
        let messages = decode_acf_abb_messages(&frame).unwrap();
        assert_eq!(messages, vec![decode_acf_abb(&frame).unwrap()]);
    }

    #[test]
    fn decode_acf_abb_messages_rejects_trailing_short_fragment() {
        let msg = AcfAbbMessage {
            info: ByteMessageInfo::default(),
            payload: vec![1, 2, 3],
        };
        let mut frame = encode_acf_abb(&msg).unwrap();
        frame.push(0xFF); // one dangling byte, not a full second header
        assert_eq!(decode_acf_abb_messages(&frame), Err(RcpError::ShortFrame));
    }

    // ── ACF_GBB round-trip ──────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-GBB-001
    // fusa:test REQ-GBB-002
    fn acf_gbb_round_trip() {
        // header(16) + 5 payload = 21 -> pad 3 -> 24 total -> 6 quadlets.
        let msg = AcfGbbMessage {
            info: ByteMessageInfo {
                acf_msg_type: ACF_GBB_MSG_TYPE,
                acf_msg_length: 6,
                pad: 3,
                ..sample_info()
            },
            message_timestamp: 0x0011_2233_4455_6677,
            payload: vec![0x11, 0x22, 0x33, 0x44, 0x55],
        };
        let frame = encode_acf_gbb(&msg).unwrap();
        let decoded = decode_acf_gbb(&frame).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    // fusa:test REQ-GBB-002
    fn acf_gbb_round_trip_zero_and_max_timestamp() {
        for message_timestamp in [0u64, u64::MAX] {
            let msg = AcfGbbMessage {
                info: ByteMessageInfo {
                    acf_msg_type: ACF_GBB_MSG_TYPE,
                    acf_msg_length: 4,
                    pad: 0,
                    ..Default::default()
                },
                message_timestamp,
                payload: vec![],
            };
            let frame = encode_acf_gbb(&msg).unwrap();
            assert_eq!(frame.len(), ACF_GBB_HEADER_LEN);
            let decoded = decode_acf_gbb(&frame).unwrap();
            assert_eq!(decoded, msg);
        }
    }

    #[test]
    // fusa:test REQ-GBB-002
    fn acf_gbb_round_trip_large_payload() {
        // header(16) + 256 payload = 272 -> already aligned -> pad 0 -> 68
        // quadlets.
        let msg = AcfGbbMessage {
            info: ByteMessageInfo {
                acf_msg_type: ACF_GBB_MSG_TYPE,
                acf_msg_length: 68,
                pad: 0,
                ..sample_info()
            },
            message_timestamp: 0xDEAD_BEEF_0000_0001,
            payload: (0..=255u16).map(|v| v as u8).collect(),
        };
        let frame = encode_acf_gbb(&msg).unwrap();
        let decoded = decode_acf_gbb(&frame).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    // fusa:test REQ-GBB-002
    fn acf_gbb_encoded_header_is_exactly_8_bytes_wider_than_acf_abb() {
        assert_eq!(ACF_GBB_HEADER_LEN, ACF_ABB_HEADER_LEN + 8);
    }

    #[test]
    // fusa:test REQ-GBB-003
    fn acf_gbb_encoded_message_has_expected_discriminant() {
        let msg = AcfGbbMessage {
            info: ByteMessageInfo::default(),
            message_timestamp: 0,
            payload: vec![0xAA, 0xBB],
        };
        let frame = encode_acf_gbb(&msg).unwrap();
        let decoded_info = decode_byte_message_info(&frame).unwrap();
        assert_eq!(decoded_info.acf_msg_type, ACF_GBB_MSG_TYPE);
    }

    #[test]
    // fusa:test REQ-GBB-002
    fn acf_gbb_encode_propagates_byte_message_info_validation_error() {
        let msg = AcfGbbMessage {
            info: ByteMessageInfo {
                byte_bus_id: BYTE_MESSAGE_INFO_11BIT_MAX + 1,
                ..Default::default()
            },
            message_timestamp: 0,
            payload: vec![],
        };
        assert_eq!(encode_acf_gbb(&msg), Err(RcpError::InvalidSize));
    }

    // ── ACF_GBB decode rejection ────────────────────────────────────────────

    #[test]
    // fusa:test REQ-GBB-004
    fn acf_gbb_decode_rejects_empty_input() {
        assert_eq!(decode_acf_gbb(&[]), Err(RcpError::ShortFrame));
    }

    #[test]
    // fusa:test REQ-GBB-004
    fn acf_gbb_decode_rejects_wrong_discriminant() {
        assert!(matches!(
            decode_acf_gbb(&[0xFFu8; ACF_GBB_HEADER_LEN]),
            Err(RcpError::Other(_))
        ));
    }

    #[test]
    // fusa:test REQ-GBB-004
    fn acf_gbb_decode_rejects_abb_discriminant_with_specific_hint() {
        let msg = AcfAbbMessage {
            info: ByteMessageInfo::default(),
            payload: vec![],
        };
        let frame = encode_acf_abb(&msg).unwrap();
        let mut padded = frame.clone();
        padded.extend_from_slice(&[0u8; 8]); // pad out to GBB header length
        let err = decode_acf_gbb(&padded).unwrap_err();
        match err {
            RcpError::Other(msg) => assert!(msg.contains("ACF_ABB")),
            other => panic!("expected RcpError::Other, got {other:?}"),
        }
    }

    #[test]
    // fusa:test REQ-GBB-004
    fn acf_gbb_decode_rejects_truncated_timestamp() {
        // Correct discriminant and full byte_message_info, but truncated
        // before the 8-byte message_timestamp is complete.
        let mut short = vec![0u8; BYTE_MESSAGE_INFO_LEN];
        short[0] = ACF_GBB_MSG_TYPE << 1;
        short.extend_from_slice(&[0u8; 3]); // only 3 of 8 timestamp bytes
        assert_eq!(decode_acf_gbb(&short), Err(RcpError::ShortFrame));
    }

    // ── ACF_GBB fuzz-style: arbitrary bytes never panic ────────────────────

    #[test]
    // fusa:test REQ-GBB-005
    fn acf_gbb_decode_never_panics_on_arbitrary_input() {
        let inputs: &[&[u8]] = &[&[], &[0x0E], &[0x0D], &[0xFF; 32], &[0x00; 32], &[0x0D; 64]];
        for input in inputs {
            let _ = decode_acf_gbb(input);
        }
    }

    #[test]
    // fusa:test REQ-GBB-005
    fn acf_gbb_decode_never_panics_on_random_lengths() {
        let mut state: u32 = 0x0D0E_0D0E;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for len in 0..64 {
            let buf: Vec<u8> = (0..len).map(|_| (next() & 0xFF) as u8).collect();
            let _ = decode_acf_gbb(&buf);
        }
    }

    // ── Echo-back rule ──────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-ECHO-001
    // fusa:test REQ-ECHO-002
    fn build_response_info_echoes_request_byte_bus_id_and_sets_rsp() {
        let request = ByteMessageInfo {
            byte_bus_id: 0x0123,
            rsp: false,
            ..sample_info()
        };
        let response_template = ByteMessageInfo {
            byte_bus_id: 0x0000,
            rsp: false,
            transaction_num: 0x77,
            ..Default::default()
        };
        let response = build_response_info(&request, response_template);
        assert_eq!(response.byte_bus_id, request.byte_bus_id);
        assert!(response.rsp);
        // Fields other than byte_bus_id/rsp pass through from the caller's
        // template unchanged.
        assert_eq!(response.transaction_num, 0x77);
    }

    #[test]
    // fusa:test REQ-ECHO-002
    // fusa:test REQ-ECHO-003
    fn build_response_info_output_passes_verify_echo_back() {
        let request = sample_info();
        let response = build_response_info(&request, ByteMessageInfo::default());
        assert_eq!(verify_echo_back(&request, &response), Ok(()));
    }

    #[test]
    // fusa:test REQ-ECHO-003
    fn verify_echo_back_accepts_matching_byte_bus_id() {
        let request = ByteMessageInfo {
            byte_bus_id: 0x0456,
            ..sample_info()
        };
        let response = ByteMessageInfo {
            byte_bus_id: 0x0456,
            rsp: true,
            ..ByteMessageInfo::default()
        };
        assert_eq!(verify_echo_back(&request, &response), Ok(()));
    }

    #[test]
    // fusa:test REQ-ECHO-003
    fn verify_echo_back_rejects_mismatched_byte_bus_id() {
        let request = ByteMessageInfo {
            byte_bus_id: 0x0001,
            ..sample_info()
        };
        let response = ByteMessageInfo {
            byte_bus_id: 0x0002,
            rsp: true,
            ..ByteMessageInfo::default()
        };
        assert_eq!(
            verify_echo_back(&request, &response),
            Err(RcpError::EpError)
        );
    }

    #[test]
    // fusa:test REQ-ECHO-003
    fn verify_echo_back_ignores_rsp_flag() {
        // The echo-back rule this function checks is scoped to byte_bus_id
        // only; it deliberately does not require response.rsp to be set.
        let request = ByteMessageInfo {
            byte_bus_id: 0x0007,
            ..sample_info()
        };
        let response = ByteMessageInfo {
            byte_bus_id: 0x0007,
            rsp: false,
            ..ByteMessageInfo::default()
        };
        assert_eq!(verify_echo_back(&request, &response), Ok(()));
    }

    #[test]
    // fusa:test REQ-ECHO-004
    fn echo_back_never_panics_across_arbitrary_field_combinations() {
        let mut state: u32 = 0xECC0_BACC;
        let mut next_u16 = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state & 0xFFFF) as u16
        };
        for _ in 0..64 {
            let request = ByteMessageInfo {
                byte_bus_id: next_u16(),
                ..ByteMessageInfo::default()
            };
            let response_template = ByteMessageInfo {
                byte_bus_id: next_u16(),
                ..ByteMessageInfo::default()
            };
            let response = build_response_info(&request, response_template);
            let _ = verify_echo_back(&request, &response);
            let _ = verify_echo_back(
                &request,
                &ByteMessageInfo {
                    byte_bus_id: next_u16(),
                    ..ByteMessageInfo::default()
                },
            );
        }
    }

    // ── Wire-level error responses ──────────────────────────────────────────

    #[test]
    fn build_error_response_echoes_request_and_sets_err_and_rsp() {
        let request = ByteMessageInfo {
            byte_bus_id: 0x0042,
            transaction_num: 0x07,
            ..sample_info()
        };
        let response = build_error_response(&request, &RcpError::InvalidParameter).unwrap();
        assert_eq!(response.info.byte_bus_id, 0x0042);
        assert_eq!(response.info.transaction_num, 0x07);
        assert!(response.info.err);
        assert!(response.info.rsp);
        assert_eq!(response.payload, vec![15]); // INVALID_PARAMETER = 15
    }

    #[test]
    fn build_error_response_covers_all_seventeen_table_27_codes() {
        let request = sample_info();
        let errors_and_codes: &[(RcpError, u8)] = &[
            (RcpError::UnsupportedCmd, 1),
            (RcpError::SequencerNotKnown, 2),
            (RcpError::UnauthorizedAccess, 3),
            (RcpError::LockedMemAccess, 4),
            (RcpError::RequestCanceled, 5),
            (RcpError::RequestNotFound, 6),
            (RcpError::EpError, 7),
            (RcpError::EpNotFound, 8),
            (RcpError::PwmInNoSignal, 9),
            (RcpError::ReqStorageOvfl, 10),
            (RcpError::RequestRejected, 11),
            (RcpError::PociFailure, 12),
            (RcpError::PresentationTimeTooFar, 13),
            (RcpError::GptpFail, 14),
            (RcpError::InvalidParameter, 15),
            (RcpError::ChainAborted, 16),
            (RcpError::ChainError, 17),
        ];
        for (error, code) in errors_and_codes {
            let response = build_error_response(&request, error).unwrap();
            assert_eq!(response.payload, vec![*code], "for {error:?}");
        }
    }

    #[test]
    fn build_error_response_returns_none_for_non_tc18_errors() {
        let request = sample_info();
        for error in [
            RcpError::Timeout,
            RcpError::ShortFrame,
            RcpError::InvalidSize,
            RcpError::Closed,
            RcpError::CrcError,
        ] {
            assert_eq!(
                build_error_response(&request, &error),
                None,
                "for {error:?}"
            );
        }
    }

    #[test]
    fn build_error_response_is_a_valid_encodable_acf_abb_frame() {
        let request = sample_info();
        let response = build_error_response(&request, &RcpError::EpNotFound).unwrap();
        let frame = encode_acf_abb(&response).unwrap();
        let decoded = decode_acf_abb(&frame).unwrap();
        assert!(decoded.info.err);
        assert_eq!(decoded.payload, vec![8]);
    }

    // ── Golden vectors: TC18 Figure 19 / Figure 20 worked examples ──────────
    //
    // Moved to `crate::e2e` (`finalize_crc_trailer_matches_figure_19_worked_example`/
    // `finalize_crc_trailer_matches_figure_20_worked_example`) and
    // strengthened to pin the ACTUAL byte sequence, not just totals. This
    // module's own `encode_acf_abb`/`encode_acf_gbb` have no CRC-trailer
    // concept of their own (see the "acf_msg_length quadlet semantics" note
    // above), so a worked example that includes the CRC trailer belongs
    // with the module that actually assembles a CRC-protected frame —
    // see `crate::e2e`'s "CRC trailer wire placement" doc section for why
    // the two tests that used to live here (concatenating `payload +
    // crc_bytes` into one blob before calling `encode_acf_abb`/
    // `encode_acf_gbb`) produced the wrong, reversed `payload, CRC, pad`
    // wire order while still passing — they only checked total length,
    // quadlet count, and pad count, all of which are identical either way.
}
