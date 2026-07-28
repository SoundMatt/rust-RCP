// fusa:req REQ-NTSCF-001
// fusa:req REQ-NTSCF-002
// fusa:req REQ-NTSCF-003
// fusa:req REQ-NTSCF-004
// fusa:req REQ-NTSCF-005
// fusa:req REQ-NTSCF-006

//! IEEE 1722 AVTPDU framing — TC18 wire format core (`ROADMAP.md` Milestone 1).
//!
//! This module begins the replacement of the legacy 16-byte frame in
//! [`crate::wire`] with the real OPEN Alliance TC18 Remote Control Protocol
//! wire format. For now it models only the **NTSCF** AVTPDU header — per
//! this crate's own spec-extraction notes, the only header variant an RC
//! Server ever sends. The TSCF header variant, the two ACF message types,
//! `stream_id` construction/parsing (as opposed to carrying it as an opaque
//! field, which this module does), and timestamp semantics are separate,
//! later items on the same Milestone 1 checklist and are intentionally not
//! implemented here.
//!
//! Nothing else in the crate depends on this module yet: it coexists with
//! [`crate::wire`] rather than replacing it, so existing callers of the
//! legacy frame are unaffected until a later milestone cuts them over.
//!
//! ## Provenance note
//!
//! Field names (`ntscf_data_length`, `sequence_num`) are taken from this
//! crate's `ROADMAP.md`, which itself cites the OPEN Alliance TC18 Remote
//! Control Protocol Specification v0.5.1_RC by section number only. The
//! specific byte offsets and bit widths implemented below are this crate's
//! own working interpretation of IEEE 1722 AVTPDU control-format framing,
//! not a transcription of that (confidential, OPEN-Members-only) document's
//! text. Per Guiding Principle 5, this is flagged for reconciliation against
//! the specification's behavior (never its prose) before being relied on for
//! interop with a real TC18 RC Server.

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
}
