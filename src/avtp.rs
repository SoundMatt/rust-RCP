// fusa:req REQ-NTSCF-001
// fusa:req REQ-NTSCF-002
// fusa:req REQ-NTSCF-003
// fusa:req REQ-NTSCF-004
// fusa:req REQ-NTSCF-005
// fusa:req REQ-NTSCF-006
// fusa:req REQ-TSCF-001
// fusa:req REQ-TSCF-002
// fusa:req REQ-TSCF-003
// fusa:req REQ-TSCF-004
// fusa:req REQ-TSCF-005
// fusa:req REQ-TSCF-006
// fusa:req REQ-HVSEL-001
// fusa:req REQ-HVSEL-002
// fusa:req REQ-HVSEL-003
// fusa:req REQ-HVSEL-004
// fusa:req REQ-HVSEL-005
// fusa:req REQ-SID-001
// fusa:req REQ-SID-002
// fusa:req REQ-SID-003
// fusa:req REQ-SID-004
// fusa:req REQ-SID-005

//! IEEE 1722 AVTPDU framing — TC18 wire format core (`ROADMAP.md` Milestone 1).
//!
//! This module begins the replacement of the legacy 16-byte frame that used
//! to live in `crate::wire` (deleted by `ROADMAP.md` Milestone 9's `wire`
//! REPLACE cutover — see this module's "Frame composition" section below)
//! with the real OPEN Alliance TC18 Remote Control Protocol wire format. It
//! models the two AVTPDU header variants named on the
//! Milestone 1 checklist:
//!
//! - **NTSCF** — per this crate's own spec-extraction notes, the only header
//!   variant an RC Server ever sends.
//! - **TSCF** — the client-to-server counterpart, carrying an additional
//!   `avtp_timestamp` field. Per the same notes, a client is the only side
//!   that ever needs to *encode* one; [`encode_tscf_header`] is provided
//!   for symmetry (and so the round-trip tests below can exercise it) but
//!   an RC Server's own send path has no occasion to call it. Nothing in
//!   this module enforces that direction at the type level — it is a
//!   protocol-level convention, documented here rather than compiled in.
//!
//! [`select_header_variant`] layers the Milestone 1 "header-variant
//! selection/rejection rule" on top of the two decoders above: a receiving
//! server's [`TimeSyncCapability`] gates whether a TSCF-subtyped AVTPDU is
//! decoded at all. NTSCF carries no timing assumption and is accepted
//! unconditionally; TSCF's `avtp_timestamp` is only meaningful to a server
//! that participates in time synchronization, so a time-sync-incapable
//! server drops it outright rather than decoding the remainder of the
//! header.
//!
//! The two ACF message types (implemented separately in [`crate::acf`]) are
//! a separate item on the same Milestone 1 checklist and are intentionally
//! not implemented here. Full timestamp semantics — `avtp_timestamp`'s
//! width/rollover behavior and the invalid-timestamp fallback rule — are
//! also a separate item, now implemented in [`crate::timestamp`]
//! ([`crate::timestamp::AvtpTimestamp`]) as a standalone newtype rather
//! than a change to [`TscfHeader::avtp_timestamp`]'s own field type; see
//! that module's doc comment.
//!
//! `stream_id` construction/parsing — the first "Addressing" checklist
//! item — *is* implemented in this module: [`StreamId`] decomposes/composes
//! the opaque 64-bit value carried by [`NtscfHeader::stream_id`] and
//! [`TscfHeader::stream_id`] into a sender MAC address and a
//! locally-assigned unique-id suffix, via [`build_stream_id`]/
//! [`parse_stream_id`] (or the [`StreamId::to_u64`]/[`StreamId::from_u64`]
//! wrappers around them). This is additive, matching the discipline used by
//! every prior Milestone 1 entry: [`NtscfHeader::stream_id`] and
//! [`TscfHeader::stream_id`] remain plain `u64` fields rather than being cut
//! over to [`StreamId`] itself, so no existing caller of either header type
//! changes shape. The remaining two "Addressing" checklist items —
//! `(stream_id, byte_bus_id)` endpoint lookup (with `byte_bus_id`'s
//! stream-relative, not global, uniqueness) and the echo-back rule for
//! responses/acks — are still separate, later work.
//!
//! ## Frame composition (`ROADMAP.md` Milestone 9)
//!
//! Every item above stops at a decoded/encoded *header* — assembling a
//! whole on-wire AVTPDU (an NTSCF header followed by its ACF payload) was
//! explicitly left for "whichever later milestone actually builds that
//! request/response lifecycle." [`encode_ntscf_frame`]/
//! [`decode_ntscf_frame`] are that composition step, added as part of
//! Milestone 9's `wire` REPLACE-disposition cutover: they combine an
//! [`NtscfHeader`] with an already-encoded [`crate::acf::AcfAbbMessage`]/
//! [`crate::acf::AcfGbbMessage`] payload (or any other bytes a caller
//! supplies) into one transportable frame, and split one back apart. This
//! is the piece that lets a transport (`crate::udp`, `crate::tlstransport`)
//! stop calling the legacy `crate::wire` frame encoder/decoder — deleted by
//! this same milestone item, since nothing else in the crate constructed
//! its 16-byte frame — in favor of the real TC18 wire format built here and
//! in `crate::acf`. Per this module's own discipline, neither function
//! parses or interprets the ACF payload itself; that remains `crate::acf`'s
//! job.
//!
//! ## Provenance note
//!
//! Field names (`ntscf_data_length`, `sequence_num`, `avtp_timestamp`,
//! `stream_data_length`) are taken from this crate's `ROADMAP.md`, which
//! itself cites the OPEN Alliance TC18 Remote Control Protocol
//! Specification v0.5.1_RC by section number only. The specific byte
//! offsets and bit widths implemented below are this crate's own working
//! interpretation of IEEE 1722 AVTPDU control-format framing, not a
//! transcription of that (confidential, OPEN-Members-only) document's text.
//! Per Guiding Principle 5, this is flagged for reconciliation against the
//! specification's behavior (never its prose) before being relied on for
//! interop with a real TC18 RC Server. In particular, `TSCF_SUBTYPE`
//! (`0x83`) and the header's total length are this crate's own placeholder
//! values pending that reconciliation.
//!
//! [`StreamId`]'s split — sender MAC in the upper 48 bits, locally-assigned
//! unique-id suffix in the lower 16 bits — follows the widely used IEEE
//! 1722 AVTP convention of that name (talker MAC high, per-talker stream
//! discriminant low). It has not been independently reconciled against the
//! OPEN Alliance TC18 Remote Control Protocol Specification's own
//! `stream_id` construction rule, and per Guiding Principle 5 is flagged
//! here as this crate's own working interpretation, not a spec-confirmed
//! fact, pending that reconciliation.

use crate::RcpError;

// ── Constants ─────────────────────────────────────────────────────────────────

/// AVTPDU `subtype` value identifying an NTSCF-headed PDU.
pub const NTSCF_SUBTYPE: u8 = 0x82;

/// Total NTSCF header length in bytes (up to, but not including, the first
/// ACF message).
pub const NTSCF_HEADER_LEN: usize = 16;

/// `ntscf_data_length` is an 11-bit field; this is its maximum representable
/// value.
pub const NTSCF_DATA_LENGTH_MAX: u16 = 0x07FF;

// ── NtscfHeader ───────────────────────────────────────────────────────────────

/// Decoded NTSCF AVTPDU header.
///
/// `stream_id` is carried here as a plain 64-bit field — this struct itself
/// does not decompose it. Use [`StreamId::from_u64`]/[`StreamId::to_u64`]
/// (or the [`parse_stream_id`]/[`build_stream_id`] functions they wrap) to
/// interpret or construct its sender-MAC/unique-id-suffix structure; see
/// this module's doc comment for why the field remains untyped here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
// fusa:req REQ-NTSCF-001
pub struct NtscfHeader {
    /// Per-stream sequence number, incremented once per NTSCF AVTPDU sent.
    pub sequence_num: u8,
    /// Length, in bytes, of the ACF message(s) carried after this header.
    /// Valid range is `0..=NTSCF_DATA_LENGTH_MAX` (11 bits).
    pub ntscf_data_length: u16,
    /// AVTP `stream_id`, carried as a plain `u64`. See [`StreamId`] to
    /// decompose/compose its sender-MAC/unique-id-suffix structure.
    pub stream_id: u64,
}

// ── Encoding helpers (local to this module; wire.rs's are private there) ──────

fn put_u64_be(buf: &mut [u8], offset: usize, v: u64) {
    buf[offset..offset + 8].copy_from_slice(&v.to_be_bytes());
}

fn get_u64_be(b: &[u8]) -> u64 {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&b[..8]);
    u64::from_be_bytes(bytes)
}

// ── Encode / decode ───────────────────────────────────────────────────────────

/// Encode an [`NtscfHeader`] to its 16-byte wire representation.
///
/// Returns `Err(RcpError::InvalidSize)` if `ntscf_data_length` exceeds the
/// 11-bit field width.
// fusa:req REQ-NTSCF-002
pub fn encode_ntscf_header(hdr: &NtscfHeader) -> Result<[u8; NTSCF_HEADER_LEN], RcpError> {
    if hdr.ntscf_data_length > NTSCF_DATA_LENGTH_MAX {
        return Err(RcpError::InvalidSize);
    }

    let mut buf = [0u8; NTSCF_HEADER_LEN];
    buf[0] = NTSCF_SUBTYPE;
    buf[1] = 0x80; // sv=1 (stream_id valid), version=000, reserved=0000
    buf[2] = hdr.sequence_num;
    // ntscf_data_length (11 bits) = byte[3] (high 8 bits) + top 3 bits of byte[4].
    buf[3] = (hdr.ntscf_data_length >> 3) as u8;
    buf[4] = ((hdr.ntscf_data_length & 0x07) as u8) << 5;
    // bytes[5..8] reserved, left zeroed.
    put_u64_be(&mut buf, 8, hdr.stream_id);
    Ok(buf)
}

/// Decode an [`NtscfHeader`] from a byte slice.
///
/// Never panics on short, truncated, or arbitrary input — always returns
/// `Err` instead.
// fusa:req REQ-NTSCF-003
// fusa:req REQ-NTSCF-004
// fusa:req REQ-NTSCF-006
pub fn decode_ntscf_header(b: &[u8]) -> Result<NtscfHeader, RcpError> {
    if b.len() < NTSCF_HEADER_LEN {
        return Err(RcpError::ShortFrame);
    }
    if b[0] != NTSCF_SUBTYPE {
        return Err(RcpError::Other(format!(
            "ntscf: expected subtype 0x{:02X}, got 0x{:02X}",
            NTSCF_SUBTYPE, b[0]
        )));
    }
    let sv = (b[1] >> 7) & 0x1;
    if sv != 1 {
        return Err(RcpError::Other(
            "ntscf: sv bit must be 1 (stream_id always valid for NTSCF)".into(),
        ));
    }

    let sequence_num = b[2];
    let len_hi = u16::from(b[3]);
    let len_lo = u16::from(b[4] >> 5);
    let ntscf_data_length = (len_hi << 3) | len_lo;
    let stream_id = get_u64_be(&b[8..16]);

    Ok(NtscfHeader {
        sequence_num,
        ntscf_data_length,
        stream_id,
    })
}

// ── TscfHeader ────────────────────────────────────────────────────────────────

/// AVTPDU `subtype` value identifying a TSCF-headed PDU.
pub const TSCF_SUBTYPE: u8 = 0x83;

/// Total TSCF header length in bytes (up to, but not including, the first
/// ACF message). Wider than [`NTSCF_HEADER_LEN`] to carry `avtp_timestamp`.
pub const TSCF_HEADER_LEN: usize = 24;

/// `stream_data_length` is an 11-bit field; this is its maximum
/// representable value.
pub const TSCF_DATA_LENGTH_MAX: u16 = 0x07FF;

/// Decoded TSCF AVTPDU header.
///
/// TSCF is the client-to-server counterpart of [`NtscfHeader`]: per this
/// module's doc comment, an RC Server never needs to *encode* one, though
/// it must be able to decode one (an RC Client uses TSCF when it has
/// time-synchronized data to send). `stream_id` is carried here as a plain
/// 64-bit value, same as [`NtscfHeader::stream_id`] — see that field's doc
/// comment for how to decompose/compose it via [`StreamId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
// fusa:req REQ-TSCF-001
pub struct TscfHeader {
    /// Per-stream sequence number, incremented once per TSCF AVTPDU sent.
    pub sequence_num: u8,
    /// 32-bit AVTP presentation timestamp. TSCF-only. Carried here as a
    /// raw passthrough value; wrap it in [`crate::timestamp::AvtpTimestamp`]
    /// for its width/rollover semantics and the invalid-timestamp fallback
    /// rule, which distinguish it from ACF_GBB's 64-bit `message_timestamp`
    /// (see [`crate::timestamp`]).
    pub avtp_timestamp: u32,
    /// Length, in bytes, of the ACF message(s) carried after this header.
    /// Valid range is `0..=TSCF_DATA_LENGTH_MAX` (11 bits).
    pub stream_data_length: u16,
    /// AVTP `stream_id`, carried as a plain `u64`. See
    /// [`NtscfHeader::stream_id`].
    pub stream_id: u64,
}

/// Encode a [`TscfHeader`] to its 24-byte wire representation.
///
/// Returns `Err(RcpError::InvalidSize)` if `stream_data_length` exceeds the
/// 11-bit field width.
// fusa:req REQ-TSCF-002
pub fn encode_tscf_header(hdr: &TscfHeader) -> Result<[u8; TSCF_HEADER_LEN], RcpError> {
    if hdr.stream_data_length > TSCF_DATA_LENGTH_MAX {
        return Err(RcpError::InvalidSize);
    }

    let mut buf = [0u8; TSCF_HEADER_LEN];
    buf[0] = TSCF_SUBTYPE;
    buf[1] = 0x80; // sv=1 (stream_id valid), version=000, reserved=0000
    buf[2] = hdr.sequence_num;
    // stream_data_length (11 bits) = byte[3] (high 8 bits) + top 3 bits of byte[4].
    buf[3] = (hdr.stream_data_length >> 3) as u8;
    buf[4] = ((hdr.stream_data_length & 0x07) as u8) << 5;
    // bytes[5..8] reserved, left zeroed.
    put_u64_be(&mut buf, 8, hdr.stream_id);
    buf[16..20].copy_from_slice(&hdr.avtp_timestamp.to_be_bytes());
    // bytes[20..24] reserved, left zeroed.
    Ok(buf)
}

/// Decode a [`TscfHeader`] from a byte slice.
///
/// Never panics on short, truncated, or arbitrary input — always returns
/// `Err` instead.
// fusa:req REQ-TSCF-003
// fusa:req REQ-TSCF-004
// fusa:req REQ-TSCF-006
pub fn decode_tscf_header(b: &[u8]) -> Result<TscfHeader, RcpError> {
    if b.len() < TSCF_HEADER_LEN {
        return Err(RcpError::ShortFrame);
    }
    if b[0] != TSCF_SUBTYPE {
        return Err(RcpError::Other(format!(
            "tscf: expected subtype 0x{:02X}, got 0x{:02X}",
            TSCF_SUBTYPE, b[0]
        )));
    }
    let sv = (b[1] >> 7) & 0x1;
    if sv != 1 {
        return Err(RcpError::Other(
            "tscf: sv bit must be 1 (stream_id always valid for TSCF)".into(),
        ));
    }

    let sequence_num = b[2];
    let len_hi = u16::from(b[3]);
    let len_lo = u16::from(b[4] >> 5);
    let stream_data_length = (len_hi << 3) | len_lo;
    let stream_id = get_u64_be(&b[8..16]);
    let mut ts_bytes = [0u8; 4];
    ts_bytes.copy_from_slice(&b[16..20]);
    let avtp_timestamp = u32::from_be_bytes(ts_bytes);

    Ok(TscfHeader {
        sequence_num,
        avtp_timestamp,
        stream_data_length,
        stream_id,
    })
}

// ── Header-variant selection/rejection ───────────────────────────────────────

/// A receiving RC Server's time-synchronization capability.
///
/// TSCF's `avtp_timestamp` only carries meaning for a server that
/// participates in network time synchronization (e.g. gPTP/802.1AS). This
/// crate does not yet model *how* a server learns whether it has that
/// capability (that belongs to a later milestone's server-lifecycle work);
/// this type exists purely to make the header-variant selection rule below
/// testable against both outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// fusa:req REQ-HVSEL-001
pub enum TimeSyncCapability {
    /// The server participates in time synchronization; TSCF-headed
    /// AVTPDUs may be decoded.
    Capable,
    /// The server has no time-synchronization support; TSCF-headed AVTPDUs
    /// must be dropped outright, per this module's selection/rejection
    /// rule.
    Incapable,
}

impl TimeSyncCapability {
    fn accepts_tscf(self) -> bool {
        matches!(self, Self::Capable)
    }
}

/// A decoded AVTPDU header, tagged by which Milestone 1 header variant
/// produced it. Returned by [`select_header_variant`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// fusa:req REQ-HVSEL-001
pub enum HeaderVariant {
    /// Decoded via [`decode_ntscf_header`].
    Ntscf(NtscfHeader),
    /// Decoded via [`decode_tscf_header`].
    Tscf(TscfHeader),
}

/// Decode an AVTPDU header, applying the Milestone 1 header-variant
/// selection/rejection rule.
///
/// NTSCF-subtyped input is always decoded, regardless of `time_sync`. A
/// TSCF-subtyped input is decoded only when `time_sync` is
/// [`TimeSyncCapability::Capable`]; when the server is
/// [`TimeSyncCapability::Incapable`], the AVTPDU is dropped outright and
/// `Err(RcpError::UnsupportedCmd)` is returned without attempting to
/// decode the rest of the header — a time-sync-incapable server has no
/// meaningful use for `avtp_timestamp`, so there is nothing to gain by
/// reading further.
///
/// Never panics: returns `Err(RcpError::ShortFrame)` for input too short to
/// contain even the leading subtype byte, and `Err(RcpError::Other(_))` for
/// a subtype that is neither [`NTSCF_SUBTYPE`] nor [`TSCF_SUBTYPE`]. All
/// other decode-rejection paths (short frame past the subtype byte, sv bit,
/// ...) are delegated to [`decode_ntscf_header`]/[`decode_tscf_header`].
// fusa:req REQ-HVSEL-002
// fusa:req REQ-HVSEL-003
// fusa:req REQ-HVSEL-004
// fusa:req REQ-HVSEL-005
pub fn select_header_variant(
    b: &[u8],
    time_sync: TimeSyncCapability,
) -> Result<HeaderVariant, RcpError> {
    let subtype = *b.first().ok_or(RcpError::ShortFrame)?;
    match subtype {
        NTSCF_SUBTYPE => decode_ntscf_header(b).map(HeaderVariant::Ntscf),
        TSCF_SUBTYPE => {
            if !time_sync.accepts_tscf() {
                return Err(RcpError::UnsupportedCmd);
            }
            decode_tscf_header(b).map(HeaderVariant::Tscf)
        }
        other => Err(RcpError::Other(format!(
            "avtp: unrecognized subtype 0x{other:02X} (expected NTSCF 0x{NTSCF_SUBTYPE:02X} or TSCF 0x{TSCF_SUBTYPE:02X})"
        ))),
    }
}

// ── Addressing: stream_id construction/parsing ───────────────────────────────

/// A decomposed AVTP `stream_id`: a sender MAC address plus a
/// locally-assigned unique-id suffix.
///
/// [`NtscfHeader::stream_id`] and [`TscfHeader::stream_id`] both carry
/// `stream_id` as a plain opaque `u64` — this type is the typed view onto
/// that same 64 bits, produced by [`StreamId::from_u64`]/[`parse_stream_id`]
/// and turned back into the wire value by [`StreamId::to_u64`]/
/// [`build_stream_id`]. See the module's provenance note for the bit-layout
/// caveat that applies to both directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
// fusa:req REQ-SID-001
pub struct StreamId {
    /// The stream's sender's 48-bit MAC address, occupying the upper 48
    /// bits of the wire `stream_id` (see the module's provenance note).
    pub sender_mac: [u8; 6],
    /// A suffix the sender assigns locally to distinguish multiple
    /// concurrent streams it originates, occupying the lower 16 bits of the
    /// wire `stream_id`.
    pub unique_id: u16,
}

impl StreamId {
    /// Compose a [`StreamId`] from its sender-MAC and unique-id parts.
    // fusa:req REQ-SID-001
    pub fn new(sender_mac: [u8; 6], unique_id: u16) -> Self {
        Self {
            sender_mac,
            unique_id,
        }
    }

    /// Compose the opaque 64-bit wire `stream_id` value from this
    /// [`StreamId`]'s parts. Equivalent to [`build_stream_id`].
    // fusa:req REQ-SID-001
    pub fn to_u64(self) -> u64 {
        build_stream_id(self.sender_mac, self.unique_id)
    }

    /// Decompose an opaque 64-bit wire `stream_id` value into a
    /// [`StreamId`]. Equivalent to [`parse_stream_id`]. Infallible: every
    /// `u64` value — including all-zero and all-`0xFF` — maps to exactly
    /// one [`StreamId`].
    // fusa:req REQ-SID-001
    pub fn from_u64(raw: u64) -> Self {
        let (sender_mac, unique_id) = parse_stream_id(raw);
        Self::new(sender_mac, unique_id)
    }
}

impl From<StreamId> for u64 {
    fn from(id: StreamId) -> u64 {
        id.to_u64()
    }
}

impl From<u64> for StreamId {
    fn from(raw: u64) -> StreamId {
        StreamId::from_u64(raw)
    }
}

/// Compose an opaque 64-bit AVTP `stream_id` from a sender MAC address and a
/// locally-assigned unique-id suffix, per this module's stream_id bit-layout
/// convention (see the provenance note above): `sender_mac` occupies the
/// upper 48 bits, `unique_id` the lower 16.
///
/// Infallible — every `([u8; 6], u16)` pair maps to exactly one `u64`.
// fusa:req REQ-SID-001
pub fn build_stream_id(sender_mac: [u8; 6], unique_id: u16) -> u64 {
    let mac_bits = (u64::from(sender_mac[0]) << 56)
        | (u64::from(sender_mac[1]) << 48)
        | (u64::from(sender_mac[2]) << 40)
        | (u64::from(sender_mac[3]) << 32)
        | (u64::from(sender_mac[4]) << 24)
        | (u64::from(sender_mac[5]) << 16);
    mac_bits | u64::from(unique_id)
}

/// Decompose an opaque 64-bit AVTP `stream_id` into a sender MAC address and
/// a locally-assigned unique-id suffix — the inverse of [`build_stream_id`].
///
/// Never panics: a `u64` always carries exactly the 64 bits this function
/// reads, so there is no truncated-input case to reject the way the header
/// decoders above do.
// fusa:req REQ-SID-001
pub fn parse_stream_id(raw: u64) -> ([u8; 6], u16) {
    let sender_mac = [
        (raw >> 56) as u8,
        (raw >> 48) as u8,
        (raw >> 40) as u8,
        (raw >> 32) as u8,
        (raw >> 24) as u8,
        (raw >> 16) as u8,
    ];
    let unique_id = raw as u16;
    (sender_mac, unique_id)
}

// ── Frame composition (ROADMAP.md Milestone 9, `wire` REPLACE cutover) ───────

/// Combine an [`NtscfHeader`] (built from `stream_id`/`sequence_num`) with
/// an already-encoded ACF payload into one on-wire AVTPDU frame, ready to
/// hand to a transport.
///
/// `acf_payload` is opaque to this function — it is typically the output of
/// [`crate::acf::encode_acf_abb`] or [`crate::acf::encode_acf_gbb`], but
/// this function does not require that; it only measures the payload's
/// length to populate `ntscf_data_length`. Returns
/// `Err(RcpError::InvalidSize)` if that length exceeds
/// [`NTSCF_DATA_LENGTH_MAX`] (the same 11-bit field width
/// [`encode_ntscf_header`] itself enforces).
// fusa:req REQ-WIRE-001
// fusa:req REQ-WIRE-002
// fusa:req REQ-WIRE-004
// fusa:req REQ-WIRE-007
pub fn encode_ntscf_frame(
    stream_id: StreamId,
    sequence_num: u8,
    acf_payload: &[u8],
) -> Result<Vec<u8>, RcpError> {
    let ntscf_data_length = u16::try_from(acf_payload.len()).map_err(|_| RcpError::InvalidSize)?;
    let hdr = NtscfHeader {
        sequence_num,
        ntscf_data_length,
        stream_id: stream_id.to_u64(),
    };
    let hdr_bytes = encode_ntscf_header(&hdr)?;
    let mut buf = Vec::with_capacity(hdr_bytes.len() + acf_payload.len());
    buf.extend_from_slice(&hdr_bytes);
    buf.extend_from_slice(acf_payload);
    Ok(buf)
}

/// Split a decoded [`NtscfHeader`] from its trailing ACF payload bytes.
///
/// The returned payload slice is handed unparsed to whichever ACF decoder
/// the caller expects ([`crate::acf::decode_acf_abb`]/
/// [`crate::acf::decode_acf_gbb`]) — this function does not attempt to
/// distinguish between them. Never panics on short, truncated, or arbitrary
/// input: delegates directly to [`decode_ntscf_header`] for that case, then
/// returns whatever trailing bytes remain (including zero of them)
/// verbatim.
// fusa:req REQ-WIRE-003
// fusa:req REQ-WIRE-005
// fusa:req REQ-WIRE-008
// fusa:req REQ-WIRE-009
pub fn decode_ntscf_frame(b: &[u8]) -> Result<(NtscfHeader, &[u8]), RcpError> {
    let hdr = decode_ntscf_header(b)?;
    Ok((hdr, &b[NTSCF_HEADER_LEN..]))
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    // ── Round-trip ─────────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-NTSCF-001
    // fusa:test REQ-NTSCF-002
    // fusa:test REQ-NTSCF-003
    fn ntscf_header_round_trip() {
        let hdr = NtscfHeader {
            sequence_num: 0x42,
            ntscf_data_length: 0x0355,
            stream_id: 0x0011_2233_4455_6677,
        };
        let frame = encode_ntscf_header(&hdr).unwrap();
        assert_eq!(frame.len(), NTSCF_HEADER_LEN);
        let decoded = decode_ntscf_header(&frame).unwrap();
        assert_eq!(decoded, hdr);
    }

    #[test]
    // fusa:test REQ-NTSCF-002
    fn ntscf_header_round_trip_zero_values() {
        let hdr = NtscfHeader {
            sequence_num: 0,
            ntscf_data_length: 0,
            stream_id: 0,
        };
        let frame = encode_ntscf_header(&hdr).unwrap();
        let decoded = decode_ntscf_header(&frame).unwrap();
        assert_eq!(decoded, hdr);
    }

    #[test]
    // fusa:test REQ-NTSCF-002
    fn ntscf_header_round_trip_max_values() {
        let hdr = NtscfHeader {
            sequence_num: 0xFF,
            ntscf_data_length: NTSCF_DATA_LENGTH_MAX,
            stream_id: u64::MAX,
        };
        let frame = encode_ntscf_header(&hdr).unwrap();
        let decoded = decode_ntscf_header(&frame).unwrap();
        assert_eq!(decoded, hdr);
    }

    #[test]
    // fusa:test REQ-NTSCF-002
    fn encode_rejects_oversized_data_length() {
        let hdr = NtscfHeader {
            ntscf_data_length: NTSCF_DATA_LENGTH_MAX + 1,
            ..Default::default()
        };
        assert_eq!(encode_ntscf_header(&hdr), Err(RcpError::InvalidSize));
    }

    #[test]
    // fusa:test REQ-NTSCF-002
    fn encoded_header_has_expected_subtype_and_sv_bit() {
        let frame = encode_ntscf_header(&NtscfHeader::default()).unwrap();
        assert_eq!(frame[0], NTSCF_SUBTYPE);
        assert_eq!(frame[1] & 0x80, 0x80, "sv bit must be set for NTSCF");
    }

    // ── Decode rejection ──────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-NTSCF-004
    fn decode_rejects_short_input() {
        for len in 0..NTSCF_HEADER_LEN {
            let buf = vec![0u8; len];
            assert_eq!(decode_ntscf_header(&buf), Err(RcpError::ShortFrame));
        }
    }

    #[test]
    // fusa:test REQ-NTSCF-004
    fn decode_rejects_wrong_subtype() {
        let mut frame = encode_ntscf_header(&NtscfHeader::default()).unwrap();
        frame[0] = 0x83; // TSCF subtype, not NTSCF
        assert!(matches!(
            decode_ntscf_header(&frame),
            Err(RcpError::Other(_))
        ));
    }

    #[test]
    // fusa:test REQ-NTSCF-004
    fn decode_rejects_sv_bit_unset() {
        let mut frame = encode_ntscf_header(&NtscfHeader::default()).unwrap();
        frame[1] &= 0x7F; // clear sv
        assert!(matches!(
            decode_ntscf_header(&frame),
            Err(RcpError::Other(_))
        ));
    }

    #[test]
    // fusa:test REQ-NTSCF-004
    fn decode_ignores_reserved_bits() {
        let hdr = NtscfHeader {
            sequence_num: 7,
            ntscf_data_length: 0x123,
            stream_id: 0xABCD,
        };
        let mut frame = encode_ntscf_header(&hdr).unwrap();
        // Scribble over version/reserved bits and the reserved quadlet;
        // decode must still succeed and recover the same named fields.
        frame[1] |= 0x7F; // set everything but sv
        frame[4] |= 0x1F; // set the 5 reserved low bits of byte 4
        frame[5] = 0xFF;
        frame[6] = 0xFF;
        frame[7] = 0xFF;
        let decoded = decode_ntscf_header(&frame).unwrap();
        assert_eq!(decoded, hdr);
    }

    // ── Fuzz-style: arbitrary bytes never panic ───────────────────────────

    #[test]
    // fusa:test REQ-NTSCF-005
    // fusa:test REQ-NTSCF-006
    fn decode_never_panics_on_arbitrary_input() {
        let inputs: &[&[u8]] = &[
            &[],
            &[0x82],
            &[0x82, 0x80],
            &[0xFF; 16],
            &[0x00; 16],
            &[
                0x82, 0x80, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                0xFF,
            ],
            &[0x82; 32],
        ];
        for input in inputs {
            let _ = decode_ntscf_header(input);
        }
    }

    #[test]
    // fusa:test REQ-NTSCF-006
    fn decode_never_panics_on_random_lengths() {
        // Deterministic pseudo-random coverage across many lengths/contents,
        // matching wire.rs's fuzz-style discipline without adding a
        // dedicated cargo-fuzz target for this early a milestone item.
        let mut state: u32 = 0x1234_5678;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for len in 0..40 {
            let buf: Vec<u8> = (0..len).map(|_| (next() & 0xFF) as u8).collect();
            let _ = decode_ntscf_header(&buf);
        }
    }

    // ═══════════════════════════════════════════════════════════════════
    //  TscfHeader
    // ═══════════════════════════════════════════════════════════════════

    // ── Round-trip ─────────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-TSCF-001
    // fusa:test REQ-TSCF-002
    // fusa:test REQ-TSCF-003
    fn tscf_header_round_trip() {
        let hdr = TscfHeader {
            sequence_num: 0x42,
            avtp_timestamp: 0x1122_3344,
            stream_data_length: 0x0355,
            stream_id: 0x0011_2233_4455_6677,
        };
        let frame = encode_tscf_header(&hdr).unwrap();
        assert_eq!(frame.len(), TSCF_HEADER_LEN);
        let decoded = decode_tscf_header(&frame).unwrap();
        assert_eq!(decoded, hdr);
    }

    #[test]
    // fusa:test REQ-TSCF-002
    fn tscf_header_round_trip_zero_values() {
        let hdr = TscfHeader {
            sequence_num: 0,
            avtp_timestamp: 0,
            stream_data_length: 0,
            stream_id: 0,
        };
        let frame = encode_tscf_header(&hdr).unwrap();
        let decoded = decode_tscf_header(&frame).unwrap();
        assert_eq!(decoded, hdr);
    }

    #[test]
    // fusa:test REQ-TSCF-002
    fn tscf_header_round_trip_max_values() {
        let hdr = TscfHeader {
            sequence_num: 0xFF,
            avtp_timestamp: u32::MAX,
            stream_data_length: TSCF_DATA_LENGTH_MAX,
            stream_id: u64::MAX,
        };
        let frame = encode_tscf_header(&hdr).unwrap();
        let decoded = decode_tscf_header(&frame).unwrap();
        assert_eq!(decoded, hdr);
    }

    #[test]
    // fusa:test REQ-TSCF-002
    fn tscf_encode_rejects_oversized_data_length() {
        let hdr = TscfHeader {
            stream_data_length: TSCF_DATA_LENGTH_MAX + 1,
            ..Default::default()
        };
        assert_eq!(encode_tscf_header(&hdr), Err(RcpError::InvalidSize));
    }

    #[test]
    // fusa:test REQ-TSCF-002
    fn tscf_encoded_header_has_expected_subtype_and_sv_bit() {
        let frame = encode_tscf_header(&TscfHeader::default()).unwrap();
        assert_eq!(frame[0], TSCF_SUBTYPE);
        assert_eq!(frame[1] & 0x80, 0x80, "sv bit must be set for TSCF");
    }

    #[test]
    // fusa:test REQ-TSCF-002
    fn tscf_and_ntscf_headers_use_distinct_subtypes() {
        assert_ne!(TSCF_SUBTYPE, NTSCF_SUBTYPE);
    }

    // ── Decode rejection ──────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-TSCF-004
    fn tscf_decode_rejects_short_input() {
        for len in 0..TSCF_HEADER_LEN {
            let buf = vec![0u8; len];
            assert_eq!(decode_tscf_header(&buf), Err(RcpError::ShortFrame));
        }
    }

    #[test]
    // fusa:test REQ-TSCF-004
    fn tscf_decode_rejects_wrong_subtype() {
        let mut frame = encode_tscf_header(&TscfHeader::default()).unwrap();
        frame[0] = NTSCF_SUBTYPE; // NTSCF subtype, not TSCF
        assert!(matches!(
            decode_tscf_header(&frame),
            Err(RcpError::Other(_))
        ));
    }

    #[test]
    // fusa:test REQ-TSCF-004
    fn tscf_decode_rejects_sv_bit_unset() {
        let mut frame = encode_tscf_header(&TscfHeader::default()).unwrap();
        frame[1] &= 0x7F; // clear sv
        assert!(matches!(
            decode_tscf_header(&frame),
            Err(RcpError::Other(_))
        ));
    }

    #[test]
    // fusa:test REQ-TSCF-004
    fn tscf_decode_ignores_reserved_bits() {
        let hdr = TscfHeader {
            sequence_num: 7,
            avtp_timestamp: 0xDEAD_BEEF,
            stream_data_length: 0x123,
            stream_id: 0xABCD,
        };
        let mut frame = encode_tscf_header(&hdr).unwrap();
        // Scribble over version/reserved bits and the reserved bytes;
        // decode must still succeed and recover the same named fields.
        frame[1] |= 0x7F; // set everything but sv
        frame[4] |= 0x1F; // set the 5 reserved low bits of byte 4
        frame[5] = 0xFF;
        frame[6] = 0xFF;
        frame[7] = 0xFF;
        frame[20] = 0xFF;
        frame[21] = 0xFF;
        frame[22] = 0xFF;
        frame[23] = 0xFF;
        let decoded = decode_tscf_header(&frame).unwrap();
        assert_eq!(decoded, hdr);
    }

    // ── Fuzz-style: arbitrary bytes never panic ───────────────────────────

    #[test]
    // fusa:test REQ-TSCF-005
    // fusa:test REQ-TSCF-006
    fn tscf_decode_never_panics_on_arbitrary_input() {
        let inputs: &[&[u8]] = &[
            &[],
            &[0x83],
            &[0x83, 0x80],
            &[0xFF; 24],
            &[0x00; 24],
            &[
                0x83, 0x80, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            ],
            &[0x83; 40],
        ];
        for input in inputs {
            let _ = decode_tscf_header(input);
        }
    }

    #[test]
    // fusa:test REQ-TSCF-006
    fn tscf_decode_never_panics_on_random_lengths() {
        // Deterministic pseudo-random coverage across many lengths/contents,
        // matching wire.rs's fuzz-style discipline without adding a
        // dedicated cargo-fuzz target for this early a milestone item.
        let mut state: u32 = 0x8765_4321;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for len in 0..60 {
            let buf: Vec<u8> = (0..len).map(|_| (next() & 0xFF) as u8).collect();
            let _ = decode_tscf_header(&buf);
        }
    }

    // ═══════════════════════════════════════════════════════════════════
    //  Header-variant selection/rejection
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    // fusa:test REQ-HVSEL-001
    fn time_sync_capability_accepts_tscf_only_when_capable() {
        assert!(TimeSyncCapability::Capable.accepts_tscf());
        assert!(!TimeSyncCapability::Incapable.accepts_tscf());
    }

    #[test]
    // fusa:test REQ-HVSEL-004
    fn select_header_variant_accepts_ntscf_when_time_sync_capable() {
        let hdr = NtscfHeader {
            sequence_num: 3,
            ntscf_data_length: 12,
            stream_id: 0xAABB_CCDD,
        };
        let frame = encode_ntscf_header(&hdr).unwrap();
        let decoded = select_header_variant(&frame, TimeSyncCapability::Capable).unwrap();
        assert_eq!(decoded, HeaderVariant::Ntscf(hdr));
    }

    #[test]
    // fusa:test REQ-HVSEL-004
    fn select_header_variant_accepts_ntscf_when_time_sync_incapable() {
        // NTSCF carries no timing assumption: it is accepted regardless of
        // the receiving server's time-sync capability.
        let hdr = NtscfHeader {
            sequence_num: 3,
            ntscf_data_length: 12,
            stream_id: 0xAABB_CCDD,
        };
        let frame = encode_ntscf_header(&hdr).unwrap();
        let decoded = select_header_variant(&frame, TimeSyncCapability::Incapable).unwrap();
        assert_eq!(decoded, HeaderVariant::Ntscf(hdr));
    }

    #[test]
    // fusa:test REQ-HVSEL-003
    fn select_header_variant_accepts_tscf_when_time_sync_capable() {
        let hdr = TscfHeader {
            sequence_num: 9,
            avtp_timestamp: 0x1234_5678,
            stream_data_length: 40,
            stream_id: 0x0011_2233_4455_6677,
        };
        let frame = encode_tscf_header(&hdr).unwrap();
        let decoded = select_header_variant(&frame, TimeSyncCapability::Capable).unwrap();
        assert_eq!(decoded, HeaderVariant::Tscf(hdr));
    }

    #[test]
    // fusa:test REQ-HVSEL-002
    fn select_header_variant_drops_tscf_when_time_sync_incapable() {
        let hdr = TscfHeader {
            sequence_num: 9,
            avtp_timestamp: 0x1234_5678,
            stream_data_length: 40,
            stream_id: 0x0011_2233_4455_6677,
        };
        let frame = encode_tscf_header(&hdr).unwrap();
        assert_eq!(
            select_header_variant(&frame, TimeSyncCapability::Incapable),
            Err(RcpError::UnsupportedCmd)
        );
    }

    #[test]
    // fusa:test REQ-HVSEL-002
    fn select_header_variant_drops_tscf_before_decoding_body() {
        // Even a TSCF frame that would otherwise fail to decode (too short
        // past the subtype byte) must still be reported as the time-sync
        // rejection, not a short-frame/decode error, confirming the
        // rejection happens before the header body is inspected.
        let short_tscf_looking = [TSCF_SUBTYPE, 0x80];
        assert_eq!(
            select_header_variant(&short_tscf_looking, TimeSyncCapability::Incapable),
            Err(RcpError::UnsupportedCmd)
        );
    }

    #[test]
    // fusa:test REQ-HVSEL-005
    fn select_header_variant_rejects_empty_input() {
        assert_eq!(
            select_header_variant(&[], TimeSyncCapability::Capable),
            Err(RcpError::ShortFrame)
        );
        assert_eq!(
            select_header_variant(&[], TimeSyncCapability::Incapable),
            Err(RcpError::ShortFrame)
        );
    }

    #[test]
    // fusa:test REQ-HVSEL-005
    fn select_header_variant_rejects_unrecognized_subtype() {
        let frame = [0x01u8; TSCF_HEADER_LEN];
        assert!(matches!(
            select_header_variant(&frame, TimeSyncCapability::Capable),
            Err(RcpError::Other(_))
        ));
    }

    #[test]
    // fusa:test REQ-HVSEL-002
    // fusa:test REQ-HVSEL-003
    fn select_header_variant_propagates_tscf_decode_errors_when_capable() {
        // Wrong subtype byte inside an otherwise TSCF-length frame should
        // still surface as a genuine decode error (not the time-sync
        // rejection) once the server is time-sync capable — the peeked
        // dispatch byte and the header's internal subtype check are
        // consistent with each other.
        let hdr = TscfHeader::default();
        let mut frame = encode_tscf_header(&hdr).unwrap();
        frame[0] = TSCF_SUBTYPE;
        frame[1] &= 0x7F; // clear sv, forcing decode_tscf_header to reject
        assert!(matches!(
            select_header_variant(&frame, TimeSyncCapability::Capable),
            Err(RcpError::Other(_))
        ));
    }

    #[test]
    // fusa:test REQ-HVSEL-005
    fn select_header_variant_never_panics_on_arbitrary_input() {
        let mut state: u32 = 0x2468_ACE0;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for len in 0..40 {
            let buf: Vec<u8> = (0..len).map(|_| (next() & 0xFF) as u8).collect();
            let _ = select_header_variant(&buf, TimeSyncCapability::Capable);
            let _ = select_header_variant(&buf, TimeSyncCapability::Incapable);
        }
    }

    // ═══════════════════════════════════════════════════════════════════
    //  Addressing: stream_id construction/parsing
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    // fusa:test REQ-SID-002
    fn stream_id_round_trip() {
        let mac = [0x00, 0x11, 0x22, 0x33, 0x44, 0x55];
        let unique_id = 0xBEEF;
        let raw = build_stream_id(mac, unique_id);
        let (parsed_mac, parsed_unique_id) = parse_stream_id(raw);
        assert_eq!(parsed_mac, mac);
        assert_eq!(parsed_unique_id, unique_id);
    }

    #[test]
    // fusa:test REQ-SID-002
    fn stream_id_round_trip_zero_values() {
        let raw = build_stream_id([0; 6], 0);
        assert_eq!(raw, 0);
        assert_eq!(parse_stream_id(raw), ([0u8; 6], 0));
    }

    #[test]
    // fusa:test REQ-SID-002
    fn stream_id_round_trip_max_values() {
        let raw = build_stream_id([0xFF; 6], u16::MAX);
        assert_eq!(raw, u64::MAX);
        assert_eq!(parse_stream_id(raw), ([0xFF; 6], u16::MAX));
    }

    #[test]
    // fusa:test REQ-SID-001
    // fusa:test REQ-SID-002
    fn stream_id_type_round_trips_through_new_to_u64_from_u64() {
        let id = StreamId::new([0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF], 0x1234);
        let raw = id.to_u64();
        assert_eq!(StreamId::from_u64(raw), id);
        // `From`/`Into` conversions must agree with the named methods.
        assert_eq!(u64::from(id), raw);
        assert_eq!(StreamId::from(raw), id);
    }

    #[test]
    // fusa:test REQ-SID-003
    fn stream_id_places_sender_mac_in_upper_48_bits() {
        // Each MAC octet must land at a distinct, predictable byte position
        // (upper 48 bits, most-significant octet first), independent of
        // unique_id, so a single non-zero octet is recoverable in isolation.
        for (i, shift) in [56u32, 48, 40, 32, 24, 16].into_iter().enumerate() {
            let mut mac = [0u8; 6];
            mac[i] = 0xAB;
            let raw = build_stream_id(mac, 0);
            assert_eq!(
                raw,
                0xABu64 << shift,
                "octet {i} did not land at bit shift {shift}"
            );
        }
    }

    #[test]
    // fusa:test REQ-SID-003
    fn stream_id_places_unique_id_in_lower_16_bits() {
        let raw = build_stream_id([0; 6], 0x1234);
        assert_eq!(raw, 0x0000_0000_0000_1234);
    }

    #[test]
    // fusa:test REQ-SID-004
    fn parse_stream_id_never_panics_across_arbitrary_u64_values() {
        // parse_stream_id/build_stream_id operate on fixed-width integers
        // and fixed-size arrays only, so there is no truncated/malformed
        // input shape to panic on — this sweeps a deterministic spread of
        // values (including the extremes) to document and enforce that.
        let mut state: u64 = 0x1234_5678_9ABC_DEF0;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let mut values = vec![0u64, u64::MAX, 1, u64::MAX - 1];
        for _ in 0..64 {
            values.push(next());
        }
        for raw in values {
            let (mac, unique_id) = parse_stream_id(raw);
            assert_eq!(build_stream_id(mac, unique_id), raw);
        }
    }

    #[test]
    // fusa:test REQ-SID-005
    fn stream_id_interoperates_with_ntscf_header_opaque_field() {
        let id = StreamId::new([0x02, 0x42, 0xAC, 0x11, 0x00, 0x02], 0x0007);
        let hdr = NtscfHeader {
            sequence_num: 1,
            ntscf_data_length: 0,
            stream_id: id.to_u64(),
        };
        let frame = encode_ntscf_header(&hdr).unwrap();
        let decoded = decode_ntscf_header(&frame).unwrap();
        assert_eq!(StreamId::from_u64(decoded.stream_id), id);
    }

    #[test]
    // fusa:test REQ-SID-005
    fn stream_id_interoperates_with_tscf_header_opaque_field() {
        let id = StreamId::new([0x02, 0x42, 0xAC, 0x11, 0x00, 0x03], 0x0008);
        let hdr = TscfHeader {
            sequence_num: 1,
            avtp_timestamp: 0,
            stream_data_length: 0,
            stream_id: id.to_u64(),
        };
        let frame = encode_tscf_header(&hdr).unwrap();
        let decoded = decode_tscf_header(&frame).unwrap();
        assert_eq!(StreamId::from_u64(decoded.stream_id), id);
    }

    // ═══════════════════════════════════════════════════════════════════
    //  Frame composition (encode_ntscf_frame / decode_ntscf_frame)
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    // fusa:test REQ-WIRE-001
    // fusa:test REQ-WIRE-002
    // fusa:test REQ-WIRE-003
    // fusa:test REQ-WIRE-004
    // fusa:test REQ-WIRE-005
    fn ntscf_frame_round_trips_arbitrary_payload() {
        let sid = StreamId::new([0x02, 0x11, 0x22, 0x33, 0x44, 0x55], 0x0042);
        let payload = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02, 0x03];
        let frame = encode_ntscf_frame(sid, 7, &payload).unwrap();
        assert_eq!(frame.len(), NTSCF_HEADER_LEN + payload.len());
        let (hdr, decoded_payload) = decode_ntscf_frame(&frame).unwrap();
        assert_eq!(hdr.sequence_num, 7);
        assert_eq!(hdr.ntscf_data_length, payload.len() as u16);
        assert_eq!(StreamId::from_u64(hdr.stream_id), sid);
        assert_eq!(decoded_payload, payload.as_slice());
    }

    #[test]
    // fusa:test REQ-WIRE-005
    fn ntscf_frame_round_trips_empty_payload() {
        let sid = StreamId::default();
        let frame = encode_ntscf_frame(sid, 0, &[]).unwrap();
        assert_eq!(frame.len(), NTSCF_HEADER_LEN);
        let (hdr, decoded_payload) = decode_ntscf_frame(&frame).unwrap();
        assert_eq!(hdr.ntscf_data_length, 0);
        assert!(decoded_payload.is_empty());
    }

    #[test]
    // fusa:test REQ-WIRE-001
    // fusa:test REQ-WIRE-007
    fn ntscf_frame_rejects_oversized_payload() {
        let oversized = vec![0u8; NTSCF_DATA_LENGTH_MAX as usize + 1];
        assert_eq!(
            encode_ntscf_frame(StreamId::default(), 0, &oversized),
            Err(RcpError::InvalidSize)
        );
    }

    #[test]
    // fusa:test REQ-WIRE-008
    fn decode_ntscf_frame_propagates_wrong_subtype() {
        let mut frame = encode_ntscf_frame(StreamId::default(), 0, &[1, 2, 3]).unwrap();
        frame[0] = TSCF_SUBTYPE;
        assert!(matches!(
            decode_ntscf_frame(&frame),
            Err(RcpError::Other(_))
        ));
    }

    #[test]
    // fusa:test REQ-WIRE-009
    fn decode_ntscf_frame_rejects_short_input() {
        for len in 0..NTSCF_HEADER_LEN {
            let buf = vec![0u8; len];
            assert_eq!(decode_ntscf_frame(&buf), Err(RcpError::ShortFrame));
        }
    }

    #[test]
    // fusa:test REQ-WIRE-009
    fn ntscf_frame_functions_never_panic_on_arbitrary_input() {
        let mut state: u32 = 0x1357_9BDF;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for len in 0..NTSCF_HEADER_LEN + 20 {
            let buf: Vec<u8> = (0..len).map(|_| (next() & 0xFF) as u8).collect();
            let _ = decode_ntscf_frame(&buf);
        }
    }
}
