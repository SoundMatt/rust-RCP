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
//! module does not itself frame that outer AVTPDU, and nothing here is
//! wired into [`crate::avtp`]'s decoders yet — that composition is later
//! work.
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
//!   the separate "Timestamp Semantics" checklist item, now implemented in
//!   [`crate::timestamp`] ([`crate::timestamp::MessageTimestamp`]) as a
//!   standalone newtype consuming this field's raw `u64` value, rather than
//!   a change to this field's own type.
//! - **`byte_message_info`** — [`ByteMessageInfo`] /
//!   [`encode_byte_message_info`] / [`decode_byte_message_info`]. The
//!   shared header both ACF_ABB and ACF_GBB carry immediately after their
//!   one-byte `acf_msg_type` discriminant. See "Provenance note" below for
//!   how this module resolves the `read_size`/`segment_num` dual-purpose
//!   field, per Guiding Principle 5.
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
//! Deliberately out of scope for this module (separate, later Milestone 1
//! bullets, or later milestones entirely):
//!
//! - `stream_id` construction/parsing and `(stream_id, byte_bus_id)`
//!   endpoint lookup (the rest of the "Addressing" subsection, in
//!   [`crate::addressing`]) — `byte_bus_id` is carried here as an opaque
//!   11-bit value only.
//! - `avtp_timestamp`/`message_timestamp` width/rollover semantics and the
//!   all-zero-timestamp fallback rule (the "Timestamp Semantics"
//!   subsection, in [`crate::timestamp`]) — moot for ACF_ABB, since it has
//!   no timestamp field to apply that rule to; for ACF_GBB,
//!   `message_timestamp` is carried as a raw `u64` only, not wired into
//!   [`crate::timestamp::MessageTimestamp`] at encode/decode time.
//! - Wiring either message type into [`crate::avtp`]'s AVTPDU decoders,
//!   or cutting over any caller of [`crate::wire`] — this module is
//!   additive only, matching the pattern [`crate::avtp`] itself
//!   established for NTSCF/TSCF.
//!
//! ## Provenance note
//!
//! `acf_msg_type`, `byte_message_info`, and their constituent field names
//! (`acf_msg_length`, `pad`, `mtv`, `byte_bus_id`, `evt`, `hs`, `cs`,
//! `transaction_num`, `op`, `rsp`, `err`, `ms`, `read_size`/`segment_num`,
//! `message_timestamp`) are taken from this crate's `ROADMAP.md`, which
//! itself cites the OPEN Alliance TC18 Remote Control Protocol
//! Specification v0.5.1_RC by section number only. Every byte offset, bit
//! width, and field-packing order implemented below is this crate's own
//! working interpretation — not a transcription of that (confidential,
//! OPEN-Members-only) document's text — and, per Guiding Principle 5, is
//! flagged here for reconciliation against the specification's *behavior*
//! (never its prose) before being relied on for interop with a real TC18 RC
//! Server. In particular:
//!
//! - `ByteMessageInfo`'s 8-byte length, its field ordering, and every
//!   individual bit position are placeholders sized only to fit the
//!   roadmap-named fields (an 11-bit `acf_msg_length`, an 11-bit
//!   `byte_bus_id`, and a 4-bit `evt`, by explicit roadmap width; the
//!   remaining `pad`/`mtv`/`hs`/`cs`/`op`/`rsp`/`err`/`ms` flags as 1 bit
//!   each; `transaction_num` and `read_size`/`segment_num` as 8 bits each)
//!   plus reserved zero-filled bits rounding the header to a whole number
//!   of bytes. None of these widths (besides the three the roadmap states
//!   explicitly) are confirmed against real TC18 framing.
//! - The `read_size`/`segment_num` dual-purpose field is, per the roadmap's
//!   own framing, one field whose meaning depends on message
//!   direction/type. This crate has not reconciled *which* bit(s) in
//!   `byte_message_info` (if any) select that interpretation, so rather
//!   than guess a discriminant bit that may not exist,
//!   [`ReadSizeOrSegmentNum`] models the field as a single raw byte with
//!   two same-bit accessor views (see its own doc comment) instead of
//!   silently committing to one interpretation or inventing an unconfirmed
//!   selector bit. `ROADMAP.md` Milestone 8's `crate::fragment` module
//!   narrows this — but does not resolve it — for its own single question
//!   of how a fragment train orders its segments: within a message whose
//!   `ms` flag marks it as part of a train, `crate::fragment` reads this
//!   field via [`ReadSizeOrSegmentNum::as_segment_num`] as a consistency
//!   check against gaps/duplicates/reordering. Whatever this field means
//!   outside a fragment train (i.e. the `read_size` interpretation) is
//!   untouched by that and stays exactly as ambiguous as stated above.
//! - Treating `acf_msg_type` as a standalone full leading byte (rather than
//!   bit-packing it alongside `pad`/part of `acf_msg_length`, the way real
//!   IEEE 1722 ACF common-header framing is understood to do) is a
//!   scope-narrowing simplification carried forward from this module's
//!   first ACF_ABB-only draft, not a claim about the real wire layout.
//! - This crate's `ROADMAP.md` states the echo-back rule itself (a
//!   response/ack must carry the same `byte_bus_id` it was received under)
//!   but not the mechanics of *when* it is checked against a live request/
//!   response exchange. [`build_response_info`]/[`verify_echo_back`] are
//!   deliberately plain functions over already-decoded [`ByteMessageInfo`]
//!   values, with no opinion on whether the real enforcement point ends up
//!   being at encode time (reject building a malformed response), at decode
//!   time (reject an inbound response that fails to echo), or purely as an
//!   application-level helper a later milestone's request/response dispatch
//!   calls explicitly. Wiring either function into an encoder, a decoder,
//!   or a dispatch loop is out of scope here and left to whichever later
//!   milestone actually builds that request/response lifecycle.

use crate::RcpError;

// ── byte_message_info ─────────────────────────────────────────────────────────

/// `acf_msg_length`/`byte_bus_id` are both 11-bit fields; this is the
/// maximum value representable in 11 bits.
pub const BYTE_MESSAGE_INFO_11BIT_MAX: u16 = 0x07FF;

/// `evt.sub_opcode` is a 3-bit field; this is its maximum representable
/// value.
pub const EVT_SUB_OPCODE_MAX: u8 = 0x07;

/// Length, in bytes, of the encoded `byte_message_info` header shared by
/// [`AcfAbbMessage`] and [`AcfGbbMessage`]. See this module's provenance
/// note for why 8 bytes specifically.
pub const BYTE_MESSAGE_INFO_LEN: usize = 8;

/// The `evt` field: a 1-bit ack flag + 3-bit sub-opcode pair, per the
/// roadmap's own description of `byte_message_info`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
// fusa:req REQ-BMI-001
pub struct Evt {
    /// The ack-flag bit of `evt`.
    pub ack: bool,
    /// The 3-bit sub-opcode of `evt`. Valid range is `0..=EVT_SUB_OPCODE_MAX`.
    pub sub_opcode: u8,
}

/// The dual-purpose 8-bit field the roadmap names `read_size`/`segment_num`.
///
/// Rather than pick one of the two meanings — or invent an unconfirmed
/// discriminant bit to switch between them — this type models the field as
/// a single raw byte with two same-bit accessor views. Which view a caller
/// should use is a message-direction/type convention this crate has not
/// yet reconciled against real TC18 behavior; see this module's provenance
/// note (Guiding Principle 5). Both accessors currently return the same
/// underlying byte unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
// fusa:req REQ-BMI-001
pub struct ReadSizeOrSegmentNum(pub u8);

impl ReadSizeOrSegmentNum {
    /// View this field's raw byte as a `read_size` value (a requested read
    /// byte count, by roadmap naming).
    pub fn as_read_size(self) -> u8 {
        self.0
    }

    /// View this field's raw byte as a `segment_num` value (a fragment
    /// index, by roadmap naming).
    pub fn as_segment_num(self) -> u8 {
        self.0
    }
}

/// Decoded `byte_message_info` header, shared by [`AcfAbbMessage`] and
/// [`AcfGbbMessage`].
///
/// `byte_bus_id` is carried here as an opaque 11-bit value only — this
/// module does not implement `(stream_id, byte_bus_id)` addressing or the
/// echo-back rule; those are the separate "Addressing" checklist item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
// fusa:req REQ-BMI-001
pub struct ByteMessageInfo {
    /// Length of the ACF message this header belongs to. 11 bits; valid
    /// range is `0..=BYTE_MESSAGE_INFO_11BIT_MAX`.
    pub acf_msg_length: u16,
    /// Padding-present flag.
    pub pad: bool,
    /// Message-timestamp-valid flag. Roadmap-shared across both ACF_ABB and
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
    /// The dual-purpose `read_size`/`segment_num` field. See
    /// [`ReadSizeOrSegmentNum`].
    pub read_size_segment_num: ReadSizeOrSegmentNum,
}

/// Encode a [`ByteMessageInfo`] to its 8-byte wire representation.
///
/// Returns `Err(RcpError::InvalidSize)` if `acf_msg_length` or
/// `byte_bus_id` exceeds the 11-bit field width, or `evt.sub_opcode`
/// exceeds the 3-bit field width.
// fusa:req REQ-BMI-002
// fusa:req REQ-BMI-003
pub fn encode_byte_message_info(
    info: &ByteMessageInfo,
) -> Result<[u8; BYTE_MESSAGE_INFO_LEN], RcpError> {
    if info.acf_msg_length > BYTE_MESSAGE_INFO_11BIT_MAX
        || info.byte_bus_id > BYTE_MESSAGE_INFO_11BIT_MAX
        || info.evt.sub_opcode > EVT_SUB_OPCODE_MAX
    {
        return Err(RcpError::InvalidSize);
    }

    let mut buf = [0u8; BYTE_MESSAGE_INFO_LEN];

    // byte 0: acf_msg_length[10:3] (top 8 bits of the 11-bit field).
    buf[0] = (info.acf_msg_length >> 3) as u8;
    // byte 1: acf_msg_length[2:0] | pad | mtv | reserved(3 bits, zero).
    buf[1] = (((info.acf_msg_length & 0x7) as u8) << 5)
        | ((info.pad as u8) << 4)
        | ((info.mtv as u8) << 3);

    // byte 2: byte_bus_id[10:3] (top 8 bits of the 11-bit field).
    buf[2] = (info.byte_bus_id >> 3) as u8;
    // byte 3: byte_bus_id[2:0] | evt (ack:1 + sub_opcode:3) | reserved(1 bit, zero).
    let evt_bits = ((info.evt.ack as u8) << 3) | (info.evt.sub_opcode & 0x7);
    buf[3] = (((info.byte_bus_id & 0x7) as u8) << 5) | (evt_bits << 1);

    // byte 4: hs | cs | op | rsp | err | ms | reserved(2 bits, zero).
    buf[4] = ((info.hs as u8) << 7)
        | ((info.cs as u8) << 6)
        | ((info.op as u8) << 5)
        | ((info.rsp as u8) << 4)
        | ((info.err as u8) << 3)
        | ((info.ms as u8) << 2);

    // byte 5: transaction_num, full byte.
    buf[5] = info.transaction_num;
    // byte 6: read_size/segment_num, full byte.
    buf[6] = info.read_size_segment_num.0;
    // byte 7: reserved, zero.

    Ok(buf)
}

/// Decode a [`ByteMessageInfo`] from a byte slice.
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

    let acf_msg_length = (u16::from(b[0]) << 3) | u16::from(b[1] >> 5);
    let pad = (b[1] >> 4) & 0x1 != 0;
    let mtv = (b[1] >> 3) & 0x1 != 0;

    let byte_bus_id = (u16::from(b[2]) << 3) | u16::from(b[3] >> 5);
    let evt_bits = (b[3] >> 1) & 0x0F;
    let evt = Evt {
        ack: (evt_bits >> 3) & 0x1 != 0,
        sub_opcode: evt_bits & 0x7,
    };

    let hs = (b[4] >> 7) & 0x1 != 0;
    let cs = (b[4] >> 6) & 0x1 != 0;
    let op = (b[4] >> 5) & 0x1 != 0;
    let rsp = (b[4] >> 4) & 0x1 != 0;
    let err = (b[4] >> 3) & 0x1 != 0;
    let ms = (b[4] >> 2) & 0x1 != 0;

    let transaction_num = b[5];
    let read_size_segment_num = ReadSizeOrSegmentNum(b[6]);

    Ok(ByteMessageInfo {
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
        read_size_segment_num,
    })
}

// ── Constants shared by both ACF message types ────────────────────────────────

/// `acf_msg_type` discriminant identifying an ACF_ABB message.
pub const ACF_ABB_MSG_TYPE: u8 = 0x0E;

/// `acf_msg_type` discriminant identifying an ACF_GBB message.
pub const ACF_GBB_MSG_TYPE: u8 = 0x0D;

/// Length, in bytes, of the ACF_ABB message header: the leading
/// `acf_msg_type` discriminant plus `byte_message_info`. Deliberately *not*
/// [`ACF_GBB_HEADER_LEN`]-wide: unlike ACF_GBB, ACF_ABB has no
/// `message_timestamp` region at all, so there is no reserved gap sized for
/// one.
pub const ACF_ABB_HEADER_LEN: usize = 1 + BYTE_MESSAGE_INFO_LEN;

/// Length, in bytes, of the ACF_GBB message header: the leading
/// `acf_msg_type` discriminant, `byte_message_info`, and the 8-byte
/// `message_timestamp`.
pub const ACF_GBB_HEADER_LEN: usize = 1 + BYTE_MESSAGE_INFO_LEN + 8;

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
/// The result is always exactly `ACF_ABB_HEADER_LEN + msg.payload.len()`
/// bytes: the discriminant byte, `byte_message_info`, and `payload`
/// verbatim, with no timestamp region of any width inserted. Returns
/// `Err(RcpError::InvalidSize)` if `msg.info` fails
/// [`encode_byte_message_info`]'s field-width validation.
// fusa:req REQ-ABB-002
// fusa:req REQ-ABB-003
pub fn encode_acf_abb(msg: &AcfAbbMessage) -> Result<Vec<u8>, RcpError> {
    let info_bytes = encode_byte_message_info(&msg.info)?;
    let mut buf = Vec::with_capacity(ACF_ABB_HEADER_LEN + msg.payload.len());
    buf.push(ACF_ABB_MSG_TYPE);
    buf.extend_from_slice(&info_bytes);
    buf.extend_from_slice(&msg.payload);
    Ok(buf)
}

/// Decode an [`AcfAbbMessage`] from a byte slice.
///
/// Never panics on short, truncated, or arbitrary input — always returns
/// `Err` instead. Any bytes after `byte_message_info`, including zero of
/// them, are accepted verbatim as `payload`.
// fusa:req REQ-ABB-002
// fusa:req REQ-ABB-004
// fusa:req REQ-ABB-005
pub fn decode_acf_abb(b: &[u8]) -> Result<AcfAbbMessage, RcpError> {
    let msg_type = *b.first().ok_or(RcpError::ShortFrame)?;
    if msg_type != ACF_ABB_MSG_TYPE {
        return Err(wrong_discriminant_error(
            "acf_abb",
            ACF_ABB_MSG_TYPE,
            msg_type,
            ACF_GBB_MSG_TYPE,
            "ACF_GBB",
        ));
    }
    if b.len() < ACF_ABB_HEADER_LEN {
        return Err(RcpError::ShortFrame);
    }
    let info = decode_byte_message_info(&b[1..ACF_ABB_HEADER_LEN])?;
    Ok(AcfAbbMessage {
        info,
        payload: b[ACF_ABB_HEADER_LEN..].to_vec(),
    })
}

// ── AcfGbbMessage ─────────────────────────────────────────────────────────────

/// Decoded ACF_GBB message.
///
/// `message_timestamp` is carried here as a raw 64-bit value only — this
/// module does not implement its rollover period or the all-zero-timestamp
/// "treat as untimed" fallback rule; those are the separate "Timestamp
/// Semantics" checklist item, implemented in
/// [`crate::timestamp::MessageTimestamp`] as a standalone wrapper over this
/// field's raw value.
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
/// The result is always exactly `ACF_GBB_HEADER_LEN + msg.payload.len()`
/// bytes: the discriminant byte, `byte_message_info`, the 8-byte
/// `message_timestamp`, and `payload` verbatim. Returns
/// `Err(RcpError::InvalidSize)` if `msg.info` fails
/// [`encode_byte_message_info`]'s field-width validation.
// fusa:req REQ-GBB-002
// fusa:req REQ-GBB-003
pub fn encode_acf_gbb(msg: &AcfGbbMessage) -> Result<Vec<u8>, RcpError> {
    let info_bytes = encode_byte_message_info(&msg.info)?;
    let mut buf = Vec::with_capacity(ACF_GBB_HEADER_LEN + msg.payload.len());
    buf.push(ACF_GBB_MSG_TYPE);
    buf.extend_from_slice(&info_bytes);
    buf.extend_from_slice(&msg.message_timestamp.to_be_bytes());
    buf.extend_from_slice(&msg.payload);
    Ok(buf)
}

/// Decode an [`AcfGbbMessage`] from a byte slice.
///
/// Never panics on short, truncated, or arbitrary input — always returns
/// `Err` instead. Any bytes after `message_timestamp`, including zero of
/// them, are accepted verbatim as `payload`.
// fusa:req REQ-GBB-002
// fusa:req REQ-GBB-004
// fusa:req REQ-GBB-005
pub fn decode_acf_gbb(b: &[u8]) -> Result<AcfGbbMessage, RcpError> {
    let msg_type = *b.first().ok_or(RcpError::ShortFrame)?;
    if msg_type != ACF_GBB_MSG_TYPE {
        return Err(wrong_discriminant_error(
            "acf_gbb",
            ACF_GBB_MSG_TYPE,
            msg_type,
            ACF_ABB_MSG_TYPE,
            "ACF_ABB",
        ));
    }
    if b.len() < ACF_GBB_HEADER_LEN {
        return Err(RcpError::ShortFrame);
    }
    let info_end = 1 + BYTE_MESSAGE_INFO_LEN;
    let info = decode_byte_message_info(&b[1..info_end])?;

    let mut ts_bytes = [0u8; 8];
    ts_bytes.copy_from_slice(&b[info_end..ACF_GBB_HEADER_LEN]);
    let message_timestamp = u64::from_be_bytes(ts_bytes);

    Ok(AcfGbbMessage {
        info,
        message_timestamp,
        payload: b[ACF_GBB_HEADER_LEN..].to_vec(),
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

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    fn sample_info() -> ByteMessageInfo {
        ByteMessageInfo {
            acf_msg_length: 0x0355,
            pad: true,
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
            read_size_segment_num: ReadSizeOrSegmentNum(0x99),
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
            acf_msg_length: BYTE_MESSAGE_INFO_11BIT_MAX,
            pad: true,
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
            read_size_segment_num: ReadSizeOrSegmentNum(0xFF),
        };
        let frame = encode_byte_message_info(&info).unwrap();
        let decoded = decode_byte_message_info(&frame).unwrap();
        assert_eq!(decoded, info);
    }

    #[test]
    // fusa:test REQ-BMI-003
    fn byte_message_info_encode_rejects_oversized_acf_msg_length() {
        let info = ByteMessageInfo {
            acf_msg_length: BYTE_MESSAGE_INFO_11BIT_MAX + 1,
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

    // ── ACF_ABB round-trip ─────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-ABB-001
    // fusa:test REQ-ABB-002
    fn acf_abb_round_trip() {
        let msg = AcfAbbMessage {
            info: sample_info(),
            payload: vec![0x11, 0x22, 0x33, 0x44, 0x55],
        };
        let frame = encode_acf_abb(&msg).unwrap();
        let decoded = decode_acf_abb(&frame).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    // fusa:test REQ-ABB-002
    fn acf_abb_round_trip_empty_payload() {
        let msg = AcfAbbMessage {
            info: ByteMessageInfo::default(),
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
        let msg = AcfAbbMessage {
            info: sample_info(),
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
        // timestamp at all — encoded length is exactly the header plus the
        // payload, never 8 bytes wider.
        for payload_len in [0usize, 1, 7, 8, 9, 16, 64] {
            let msg = AcfAbbMessage {
                info: ByteMessageInfo::default(),
                payload: vec![0x00; payload_len],
            };
            let frame = encode_acf_abb(&msg).unwrap();
            assert_eq!(frame.len(), ACF_ABB_HEADER_LEN + payload_len);
            assert_ne!(frame.len(), ACF_GBB_HEADER_LEN + payload_len);
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
        assert_eq!(frame[0], ACF_ABB_MSG_TYPE);
    }

    #[test]
    // fusa:test REQ-ABB-002
    fn acf_abb_encode_propagates_byte_message_info_validation_error() {
        let msg = AcfAbbMessage {
            info: ByteMessageInfo {
                acf_msg_length: BYTE_MESSAGE_INFO_11BIT_MAX + 1,
                ..Default::default()
            },
            payload: vec![],
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
        let err = decode_acf_abb(&[ACF_GBB_MSG_TYPE]).unwrap_err();
        match err {
            RcpError::Other(msg) => assert!(msg.contains("ACF_GBB")),
            other => panic!("expected RcpError::Other, got {other:?}"),
        }
    }

    #[test]
    // fusa:test REQ-ABB-004
    fn acf_abb_decode_rejects_truncated_byte_message_info() {
        // Correct discriminant, but too short to hold a full byte_message_info.
        let mut short = vec![ACF_ABB_MSG_TYPE];
        short.extend_from_slice(&[0u8; BYTE_MESSAGE_INFO_LEN - 1]);
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

    // ── ACF_GBB round-trip ──────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-GBB-001
    // fusa:test REQ-GBB-002
    fn acf_gbb_round_trip() {
        let msg = AcfGbbMessage {
            info: sample_info(),
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
                info: ByteMessageInfo::default(),
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
        let msg = AcfGbbMessage {
            info: sample_info(),
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
        assert_eq!(frame[0], ACF_GBB_MSG_TYPE);
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
        let err = decode_acf_gbb(&[ACF_ABB_MSG_TYPE]).unwrap_err();
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
        let mut short = vec![ACF_GBB_MSG_TYPE];
        short.extend_from_slice(&[0u8; BYTE_MESSAGE_INFO_LEN]);
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
}
