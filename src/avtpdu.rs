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

//! IEEE 1722 AVTPDU framing — TC18 wire format core (`ROADMAP.md` Milestone 1).
//!
//! This module begins the replacement of the legacy 16-byte frame in
//! [`crate::wire`] with the real OPEN Alliance TC18 Remote Control Protocol
//! wire format. It models the two AVTPDU header variants named on the
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
//! The two ACF message types, `stream_id` construction/parsing (as opposed
//! to carrying it as an opaque field, which this module does), and full
//! timestamp semantics (`message_timestamp`, invalid-timestamp fallback)
//! are separate, later items on the same Milestone 1 checklist and are
//! intentionally not implemented here.
//!
//! Nothing else in the crate depends on this module yet: it coexists with
//! [`crate::wire`] rather than replacing it, so existing callers of the
//! legacy frame are unaffected until a later milestone cuts them over.
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
/// `stream_id` is carried here as an opaque 64-bit value only — this module
/// does not construct or interpret its sender-MAC/unique-id-suffix internal
/// structure. That is the separate "Addressing" item on the Milestone 1
/// checklist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
// fusa:req REQ-NTSCF-001
pub struct NtscfHeader {
    /// Per-stream sequence number, incremented once per NTSCF AVTPDU sent.
    pub sequence_num: u8,
    /// Length, in bytes, of the ACF message(s) carried after this header.
    /// Valid range is `0..=NTSCF_DATA_LENGTH_MAX` (11 bits).
    pub ntscf_data_length: u16,
    /// Opaque AVTP `stream_id`. See the struct-level doc comment.
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
/// time-synchronized data to send). `stream_id` is carried here as an
/// opaque 64-bit value only, same as [`NtscfHeader::stream_id`] — see that
/// field's doc comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
// fusa:req REQ-TSCF-001
pub struct TscfHeader {
    /// Per-stream sequence number, incremented once per TSCF AVTPDU sent.
    pub sequence_num: u8,
    /// 32-bit AVTP presentation timestamp. TSCF-only — see
    /// `ROADMAP.md`'s "Timestamp Semantics" item for how this differs from
    /// ACF_GBB's 64-bit `message_timestamp`, which this module does not yet
    /// model.
    pub avtp_timestamp: u32,
    /// Length, in bytes, of the ACF message(s) carried after this header.
    /// Valid range is `0..=TSCF_DATA_LENGTH_MAX` (11 bits).
    pub stream_data_length: u16,
    /// Opaque AVTP `stream_id`. See [`NtscfHeader::stream_id`].
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
/// `Err(RcpError::TimeSyncUnsupported)` is returned without attempting to
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
                return Err(RcpError::TimeSyncUnsupported);
            }
            decode_tscf_header(b).map(HeaderVariant::Tscf)
        }
        other => Err(RcpError::Other(format!(
            "avtpdu: unrecognized subtype 0x{other:02X} (expected NTSCF 0x{NTSCF_SUBTYPE:02X} or TSCF 0x{TSCF_SUBTYPE:02X})"
        ))),
    }
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
            Err(RcpError::TimeSyncUnsupported)
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
            Err(RcpError::TimeSyncUnsupported)
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
}
