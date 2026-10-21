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
//! mechanism (TC18 RC5 §13.6, polynomial `0xF4ACFB13`).
//!
//! ## c-RCP port
//!
//! This module is a port of c-RCP's E2E CRC layer (`c-RCP/src/e2e.c`,
//! `c-RCP/include/rcp/e2e.h`). Byte-for-byte interop with c-RCP is the
//! goal, so where c-RCP departs from the TC18 RC5 text this module copies
//! c-RCP and says so in a `c-RCP parity:` note.
//!
//! | rust                              | c-RCP                                   |
//! |-----------------------------------|-----------------------------------------|
//! | [`crc32_tc18`]                    | `rcp_e2e_crc32` (`e2e.c:64-71`)         |
//! | [`compute_crc`]                   | `rcp_e2e_compute_crc` (`e2e.c:124-150`) |
//! | [`length_with_crc`]               | `rcp_e2e_length_with_crc` (`e2e.c:183-187`) |
//! | [`data_length_for_protected_members`] | `rcp_e2e_data_length_for_protected_members` (`e2e.c:190-194`) |
//! | [`wrap`] / [`unwrap`]             | `rcp_e2e_wrap` / `rcp_e2e_unwrap` (`e2e.c:200-339`) |
//! | [`wrap_framed`] / [`unwrap_framed`] | `rcp_e2e_wrap_framed` / `_unwrap_framed` (`e2e.c:344-368`) |
//! | [`fragment_carries_crc`]          | `rcp_e2e_fragment_carries_crc` (`e2e.c:373-376`) |
//! | [`compute_fragmented_crc`]        | `rcp_e2e_compute_fragmented_crc` (`e2e.c:379-406`) |
//! | [`E2eError`]                      | `rcp_e2e_errc_t`, `rcp_e2e_strerror`, `rcp_e2e_wire_error` (`e2e.c:11-42`) |
//!
//! [`wrap_for_header`]/[`unwrap_for_header`] are rust-only conveniences
//! that read every CRC input from a decoded [`HeaderVariant`] and then call
//! the `_framed` functions.
//!
//! ## Wire layout (TC18 RC5 §13.6 Figures 20/21)
//!
//! An E2E-protected ACF message is `[header][real payload][CRC32][pad]`:
//! the CRC32 sits right after the real (unpadded) payload and the pad
//! octets come after it. The header's `acf_msg_length` counts the CRC
//! quadlet (+1); the `pad` field is unchanged. [`wrap`] finds the real/pad
//! boundary by reading the `pad` field (octet 2, bits 7:6) of the frame
//! it is given, as c-RCP does (`e2e.c:108-112`), and does the +1/-1 length
//! adjustment itself.
//!
//! ## CRC coverage
//!
//! [`compute_crc`] runs one CRC over, in order: `avtp_subtype` (1 octet),
//! `header_octet1` (1 octet), a `tu` octet (`0x01`/`0x00`), `stream_id`
//! (8 octets, big-endian), `avtp_timestamp` (4 octets, big-endian), then
//! the ACF header and real payload with `acf_msg_length` already
//! incremented (padding excluded).
//!
//! c-RCP parity: under NTSCF framing c-RCP still feeds a zero `tu` octet
//! and four zero `avtp_timestamp` octets (`c-RCP/src/e2e.c:140-148` and
//! `:344-368`). TC18 RC5 Figure 21 (ACF_GBB under NTSCF) has no
//! `avtp_timestamp` row and no `tu` column, and c-RCP's own header says so
//! (`c-RCP/include/rcp/e2e.h:155-160`). This module copies c-RCP so its
//! CRCs match c-RCP's (issue #152).
//!
//! ## Fragment trains
//!
//! Only the final fragment (`ms = 0`) carries the CRC
//! ([`fragment_carries_crc`], [`check_fragment_crc_placement`]). Its value
//! comes from [`compute_fragmented_crc`]: the same 15-octet header prefix,
//! then the *first* fragment's encoded ACF header exactly as sent, then
//! the concatenated payload of every segment. The final fragment carries
//! that CRC in the same position [`wrap`] uses, with its own
//! `acf_msg_length` incremented ([`finalize_crc_trailer`]).
//!
//! c-RCP parity: c-RCP feeds the first fragment's header as-is
//! (`c-RCP/src/e2e.c:379-406`, called from `c-RCP/src/mock.c:2370`), with
//! that fragment's own `acf_msg_length`, `pad`, `ms = 1` and
//! `segment_num = 0`, and without the +1 quadlet its single-frame path
//! applies (`e2e.c:250`). TC18-13.6-007 says the CRC is calculated once,
//! before fragmentation, which reads as a header for the whole message
//! (length and pad recomputed from the combined payload, then +1). This
//! module copies c-RCP (issues #169, #170).
//!
//! ## Legacy coverage-buffer API
//!
//! [`build_crc32_coverage_buffer`] and
//! [`build_crc32_coverage_buffer_for_fragment_train`] return the bytes
//! [`compute_crc`]/[`compute_fragmented_crc`] run over, so
//! `crc32_tc18(&buffer)` gives the same value. They are deprecated in
//! favour of the functions above.
//!
//! ## `crc32_tc18` provenance
//!
//! TC18 §13.6 Table 34 fixes the CRC: polynomial `0xF4ACFB13`, init and
//! final XOR `0xFFFFFFFF`, reflected input and output (CRC-32/AUTOSAR,
//! check value `0x1697D06A` over `"123456789"`). The tests cross-check
//! [`crc32_tc18`] against a separately written MSB-first reference.

use std::fmt;

use crate::acf::{self, AcfAbbMessage, AcfGbbMessage};
use crate::avtp::{self, HeaderVariant};
use crate::RcpError;

// ── CRC-32 (TC18 safe-point) ────────────────────────────────────────────────

/// Bit-reversal of the TC18 safe-point CRC-32 polynomial `0xF4ACFB13`
/// (TC18 §13.6 Table 34 "CRC32P4"), so [`crc32_tc18`] can shift its
/// register right (LSB-first) instead of reflecting every byte. Same
/// constant as c-RCP's `RCP_E2E_CRC32_RPOLY` (`c-RCP/src/e2e.c:50`).
//fusa:req REQ-CRC-014
const CRC32_TC18_POLY_REFLECTED: u32 = 0xC8DF_352F;

/// Initial and final-XOR value, `0xFFFFFFFF` (TC18 §13.6 Table 34).
const CRC32_TC18_INIT_XOROUT: u32 = 0xFFFF_FFFF;

/// Feeds `data` into a running (not yet final-XORed) CRC register. Port
/// of c-RCP's `crc32_update` (`c-RCP/src/e2e.c:52-61`), applied per byte.
fn crc32_tc18_update(mut crc: u32, data: &[u8]) -> u32 {
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ CRC32_TC18_POLY_REFLECTED
            } else {
                crc >> 1
            };
        }
    }
    crc
}

/// Computes the OPEN Alliance TC18 safe-point CRC-32 over `data`.
///
/// Polynomial `0xF4ACFB13`, init and final XOR both `0xFFFFFFFF`, reflected
/// input and output. Port of c-RCP's `rcp_e2e_crc32`
/// (`c-RCP/src/e2e.c:64-71`). Covers exactly the bytes it is given; see
/// [`compute_crc`] for the safe-point coverage span.
//fusa:req REQ-CRC-001
//fusa:req REQ-CRC-002
pub fn crc32_tc18(data: &[u8]) -> u32 {
    crc32_tc18_update(CRC32_TC18_INIT_XOROUT, data) ^ CRC32_TC18_INIT_XOROUT
}

// ── Coverage span ────────────────────────────────────────────────────────────

/// Quadlets the CRC32 trailer adds to `acf_msg_length`: exactly one.
pub const CRC_TRAILER_QUADLETS: u16 = 1;

/// Octets the CRC32 trailer occupies on the wire: one quadlet. c-RCP's
/// `RCP_E2E_CRC_LEN` (`c-RCP/include/rcp/e2e.h:447`).
pub const CRC_TRAILER_LEN: usize = acf::QUADLET_LEN;

/// The one-quadlet `acf_msg_length` pre-adjustment applied before the CRC
/// is computed (TC18-13.6-006).
const CRC32_COVERAGE_LENGTH_PREADJUST_QUADLETS: u16 = CRC_TRAILER_QUADLETS;

/// Length of the header-CRC prefix [`compute_crc`] feeds ahead of the ACF
/// bytes: subtype (1) + octet 1 (1) + tu (1) + stream_id (8) +
/// avtp_timestamp (4).
const COVERAGE_PREFIX_LEN: usize = 15;

/// The header-CRC bytes in the exact order c-RCP feeds them
/// (`c-RCP/src/e2e.c:140-148`).
fn coverage_prefix(
    avtp_subtype: u8,
    header_octet1: u8,
    tu: bool,
    stream_id: u64,
    avtp_timestamp: u32,
) -> [u8; COVERAGE_PREFIX_LEN] {
    let mut p = [0u8; COVERAGE_PREFIX_LEN];
    p[0] = avtp_subtype;
    p[1] = header_octet1;
    p[2] = u8::from(tu);
    p[3..11].copy_from_slice(&stream_id.to_be_bytes());
    p[11..15].copy_from_slice(&avtp_timestamp.to_be_bytes());
    p
}

/// The CRC inputs `_framed` derives from the framing choice: subtype from
/// `is_ntscf_framed`, and `tu`/`avtp_timestamp` forced to `false`/`0`
/// under NTSCF (`c-RCP/src/e2e.c:344-368`).
fn framed_inputs(is_ntscf_framed: bool, tu: bool, avtp_timestamp: u32) -> (u8, bool, u32) {
    if is_ntscf_framed {
        (avtp::NTSCF_SUBTYPE, false, 0)
    } else {
        (avtp::TSCF_SUBTYPE, tu, avtp_timestamp)
    }
}

/// `(is_ntscf_framed, header_octet1, tu, stream_id, avtp_timestamp)` read
/// from a decoded AVTPDU header.
fn header_inputs(header: &HeaderVariant) -> (bool, u8, bool, u64, u32) {
    let (is_ntscf, ts) = match header {
        HeaderVariant::Ntscf(_) => (true, 0),
        HeaderVariant::Tscf(h) => (false, h.avtp_timestamp),
    };
    (
        is_ntscf,
        header.octet1(),
        header.tu(),
        header.stream_id(),
        ts,
    )
}

/// The 15-octet header-CRC prefix for `header`, with the `_framed` NTSCF
/// forcing applied.
fn header_coverage_prefix(header: &HeaderVariant) -> [u8; COVERAGE_PREFIX_LEN] {
    let (is_ntscf, octet1, tu, stream_id, ts) = header_inputs(header);
    let (subtype, tu, ts) = framed_inputs(is_ntscf, tu, ts);
    coverage_prefix(subtype, octet1, tu, stream_id, ts)
}

/// CRC32 over `avtp_subtype` + `header_octet1` + a `tu` octet +
/// `stream_id` (8 octets, big-endian) + `avtp_timestamp` (4 octets,
/// big-endian) + `acf_frame`, as one continuous CRC.
///
/// Port of c-RCP's `rcp_e2e_compute_crc` (`c-RCP/src/e2e.c:124-150`).
/// Running the TC18 two-stage "header CRC seeds the ACF CRC" description
/// as one continuous register gives the same value (Table 34: no
/// intermediate final XOR). Pass `0`/`false` for `avtp_timestamp`/`tu`
/// under NTSCF, or use [`wrap_framed`], which forces them.
//fusa:req REQ-E2E-003
pub fn compute_crc(
    avtp_subtype: u8,
    header_octet1: u8,
    tu: bool,
    stream_id: u64,
    avtp_timestamp: u32,
    acf_frame: &[u8],
) -> u32 {
    let prefix = coverage_prefix(avtp_subtype, header_octet1, tu, stream_id, avtp_timestamp);
    let crc = crc32_tc18_update(CRC32_TC18_INIT_XOROUT, &prefix);
    crc32_tc18_update(crc, acf_frame) ^ CRC32_TC18_INIT_XOROUT
}

/// `payload_len + CRC_TRAILER_LEN`, saturating at `usize::MAX`. Port of
/// c-RCP's `rcp_e2e_length_with_crc` (`c-RCP/src/e2e.c:183-187`).
//fusa:req REQ-E2E-004
pub fn length_with_crc(payload_len: usize) -> usize {
    payload_len.saturating_add(CRC_TRAILER_LEN)
}

/// How much an AVTPDU's `ntscf_data_length`/`stream_data_length` grows
/// for `protected_member_count` E2E-protected ACF messages:
/// `protected_member_count * CRC_TRAILER_LEN`, saturating. Port of c-RCP's
/// `rcp_e2e_data_length_for_protected_members` (`c-RCP/src/e2e.c:190-194`).
/// [`crate::avtp::encode_ntscf_frame`] already derives the length from the
/// payload it is given, so this is for sizing and cross-checking only.
//fusa:req REQ-E2E-037
pub fn data_length_for_protected_members(protected_member_count: usize) -> usize {
    protected_member_count.saturating_mul(CRC_TRAILER_LEN)
}

// ── wrap / unwrap ────────────────────────────────────────────────────────────

/// The `pad` field (octet 2, bits 7:6) of an encoded ACF frame, or 0 if
/// the frame is too short to hold octet 2. Port of c-RCP's
/// `acf_pad_octets` (`c-RCP/src/e2e.c:108-112`).
fn acf_pad_octets(frame: &[u8]) -> usize {
    match frame.get(2) {
        Some(&octet2) => usize::from((octet2 >> 6) & 0x03),
        None => 0,
    }
}

/// Adjusts the 9-bit `acf_msg_length` (octet 0 bit 0, octet 1) of
/// `frame` by `delta` quadlets in place, keeping `acf_msg_type`. Returns
/// `false` and leaves `frame` unchanged if it is shorter than 2 octets or
/// the result would leave `0..=0x1FF`. Port of c-RCP's
/// `adapt_acf_msg_length` (`c-RCP/src/e2e.c:164-180`).
fn adapt_acf_msg_length(frame: &mut [u8], delta: i32) -> bool {
    if frame.len() < 2 {
        return false;
    }
    let type_bits = frame[0] & 0xFE;
    let len_field = (u16::from(frame[0] & 0x01) << 8) | u16::from(frame[1]);
    let adapted = i32::from(len_field) + delta;
    if !(0..=i32::from(acf::ACF_MSG_LENGTH_9BIT_MAX)).contains(&adapted) {
        return false;
    }
    frame[0] = type_bits | (((adapted >> 8) & 0x01) as u8);
    frame[1] = (adapted & 0xFF) as u8;
    true
}

/// Copies `acf_frame` with `acf_msg_length` +1 and a CRC32 trailer placed
/// between the real payload and the pad octets; `crc_of` receives the
/// adjusted real prefix. Shared by [`wrap`] and [`finalize_crc_trailer`]
/// (`c-RCP/src/e2e.c:226-271`).
fn insert_trailer(
    acf_frame: &[u8],
    crc_of: impl FnOnce(&[u8]) -> u32,
) -> Result<Vec<u8>, RcpError> {
    let pad = acf_pad_octets(acf_frame);
    // pad <= 3 <= len whenever pad != 0 (octet 2 exists), so no underflow.
    let real_len = acf_frame.len() - pad;
    let mut out = Vec::with_capacity(acf_frame.len() + CRC_TRAILER_LEN);
    out.extend_from_slice(&acf_frame[..real_len]);
    if !adapt_acf_msg_length(&mut out, i32::from(CRC_TRAILER_QUADLETS)) {
        return Err(if real_len < 2 {
            RcpError::ShortFrame
        } else {
            RcpError::InvalidSize
        });
    }
    let crc = crc_of(&out);
    out.extend_from_slice(&crc.to_be_bytes());
    out.extend_from_slice(&acf_frame[real_len..]);
    Ok(out)
}

/// A received frame split around its CRC32 trailer: `(body, real_len,
/// crc)`, where `body` is the frame with the 4 trailer octets removed
/// (`acf_msg_length` not yet adjusted). `None` if the frame is shorter
/// than the trailer plus the pad octets its `pad` field claims
/// (`c-RCP/src/e2e.c:292-321`).
fn split_trailer(frame: &[u8]) -> Option<(Vec<u8>, usize, u32)> {
    if frame.len() < CRC_TRAILER_LEN {
        return None;
    }
    let pad = acf_pad_octets(frame);
    if pad + CRC_TRAILER_LEN > frame.len() {
        return None;
    }
    let real_len = frame.len() - CRC_TRAILER_LEN - pad;
    let mut crc_bytes = [0u8; CRC_TRAILER_LEN];
    crc_bytes.copy_from_slice(&frame[real_len..real_len + CRC_TRAILER_LEN]);
    let mut body = Vec::with_capacity(frame.len() - CRC_TRAILER_LEN);
    body.extend_from_slice(&frame[..real_len]);
    body.extend_from_slice(&frame[real_len + CRC_TRAILER_LEN..]);
    Some((body, real_len, u32::from_be_bytes(crc_bytes)))
}

/// Protects an encoded ACF_ABB/ACF_GBB frame (as [`acf::encode_acf_abb`]/
/// [`acf::encode_acf_gbb`] produce it, pad octets included): returns a
/// copy with `acf_msg_length` +1 and the big-endian CRC32 from
/// [`compute_crc`] placed right after the real payload, followed by the
/// frame's own pad octets — `[header][real payload][CRC32][pad]` (TC18 RC5
/// §13.6 Figures 20/21). The CRC covers the adjusted header and real
/// payload, not the pad. `acf_frame` is never modified.
///
/// Port of c-RCP's `rcp_e2e_wrap` (`c-RCP/src/e2e.c:200-272`). Where
/// c-RCP returns an empty buffer, this returns:
/// `Err(RcpError::InvalidSize)` if `acf_frame.len()` is not a whole number
/// of quadlets or `acf_msg_length` is already `0x1FF`;
/// `Err(RcpError::ShortFrame)` if the real prefix is shorter than the
/// 2-octet length field (including an empty frame).
///
/// [`unwrap`] reverses this whenever the real prefix still holds the `pad`
/// field (octet 2), which any frame with a full `byte_message_info` does.
/// As in c-RCP, a shorter real prefix puts a CRC octet where [`unwrap`]
/// reads `pad`.
//fusa:req REQ-E2E-005
//fusa:req REQ-E2E-006
pub fn wrap(
    avtp_subtype: u8,
    header_octet1: u8,
    tu: bool,
    stream_id: u64,
    avtp_timestamp: u32,
    acf_frame: &[u8],
) -> Result<Vec<u8>, RcpError> {
    if acf_frame.len() % acf::QUADLET_LEN != 0 {
        return Err(RcpError::InvalidSize);
    }
    insert_trailer(acf_frame, |real| {
        compute_crc(
            avtp_subtype,
            header_octet1,
            tu,
            stream_id,
            avtp_timestamp,
            real,
        )
    })
}

/// Why [`unwrap`] rejected a frame. Mirrors c-RCP's `rcp_e2e_errc_t`
/// (`c-RCP/include/rcp/e2e.h:449-454`) minus `RCP_E2E_OK`, which is `Ok`.
#[derive(Debug, Clone, PartialEq, Eq)]
//fusa:req REQ-E2E-001
pub enum E2eError {
    /// The frame is too short for a CRC32 trailer plus the pad octets its
    /// `pad` field claims (`RCP_E2E_ERR_SHORT_FRAME`).
    ShortFrame,
    /// The CRC32 does not match (`RCP_E2E_ERR_CRC_MISMATCH`). The request
    /// must not be executed. `acf_frame` is the frame with the trailer
    /// removed and `acf_msg_length` restored, for diagnostics only.
    CrcMismatch {
        /// The unverified frame body.
        acf_frame: Vec<u8>,
    },
}

impl E2eError {
    /// c-RCP's numeric `rcp_e2e_errc_t` value: `ShortFrame` = 1,
    /// `CrcMismatch` = 2.
    //fusa:req REQ-E2E-001
    pub fn code(&self) -> u8 {
        match self {
            Self::ShortFrame => 1,
            Self::CrcMismatch { .. } => 2,
        }
    }

    /// c-RCP's `rcp_e2e_strerror` text (`c-RCP/src/e2e.c:11-25`).
    //fusa:req REQ-E2E-001
    pub fn strerror(&self) -> &'static str {
        match self {
            Self::ShortFrame => "e2e: frame too short for a CRC32 trailer",
            Self::CrcMismatch { .. } => "e2e: POCI failure -- CRC32 mismatch, execution skipped",
        }
    }

    /// The wire error code for a Response: `Some(RcpError::PociFailure)`
    /// (code 12, TC18 §12.9.6 Table 30) for a CRC mismatch, `None` for a
    /// short frame, which has no wire code. Port of c-RCP's
    /// `rcp_e2e_wire_error` (`c-RCP/src/e2e.c:28-42`).
    //fusa:req REQ-WIREERR-003
    pub fn wire_error(&self) -> Option<RcpError> {
        match self {
            Self::ShortFrame => None,
            Self::CrcMismatch { .. } => Some(RcpError::PociFailure),
        }
    }
}

impl fmt::Display for E2eError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.strerror())
    }
}

impl std::error::Error for E2eError {}

/// Reverses [`wrap`]: reads the CRC32 sitting after the real payload
/// (located via the frame's `pad` field), checks it against
/// [`compute_crc`] over the frame's real prefix (its `acf_msg_length`
/// still +1), and returns the frame with the trailer removed and
/// `acf_msg_length` -1 — ready for [`acf::decode_acf_abb`]/
/// [`acf::decode_acf_gbb`].
///
/// Port of c-RCP's `rcp_e2e_unwrap` (`c-RCP/src/e2e.c:277-339`). Returns
/// `Err(E2eError::ShortFrame)` if `frame` cannot hold the trailer and the
/// claimed pad octets, and `Err(E2eError::CrcMismatch { .. })` if the CRC
/// does not match. As in c-RCP, the length is left unadjusted if it
/// cannot be decremented (real prefix under 2 octets, or length 0).
//fusa:req REQ-E2E-007
//fusa:req REQ-E2E-008
//fusa:req REQ-E2E-009
pub fn unwrap(
    avtp_subtype: u8,
    header_octet1: u8,
    tu: bool,
    stream_id: u64,
    avtp_timestamp: u32,
    frame: &[u8],
) -> Result<Vec<u8>, E2eError> {
    let (mut body, real_len, got) = split_trailer(frame).ok_or(E2eError::ShortFrame)?;
    let want = compute_crc(
        avtp_subtype,
        header_octet1,
        tu,
        stream_id,
        avtp_timestamp,
        &frame[..real_len],
    );
    let _ = adapt_acf_msg_length(&mut body[..real_len], -i32::from(CRC_TRAILER_QUADLETS));
    if got == want {
        Ok(body)
    } else {
        Err(E2eError::CrcMismatch { acf_frame: body })
    }
}

/// [`wrap`] with `avtp_subtype` derived from `is_ntscf_framed` (`0x82`
/// NTSCF / `0x05` TSCF) and, under NTSCF, `tu` forced to `false` and
/// `avtp_timestamp` to `0` whatever the caller passed. `header_octet1` is
/// always used as given. Port of c-RCP's `rcp_e2e_wrap_framed`
/// (`c-RCP/src/e2e.c:344-354`).
///
/// c-RCP parity: the forced NTSCF zeros are still fed to the CRC (one `tu`
/// octet, four timestamp octets), although TC18 RC5 Figure 21 has neither
/// field (`c-RCP/include/rcp/e2e.h:155-160`).
//fusa:req REQ-E2E-035
pub fn wrap_framed(
    stream_id: u64,
    is_ntscf_framed: bool,
    header_octet1: u8,
    tu: bool,
    avtp_timestamp: u32,
    acf_frame: &[u8],
) -> Result<Vec<u8>, RcpError> {
    let (subtype, tu, ts) = framed_inputs(is_ntscf_framed, tu, avtp_timestamp);
    wrap(subtype, header_octet1, tu, stream_id, ts, acf_frame)
}

/// [`unwrap`] with the same derivation and NTSCF forcing as
/// [`wrap_framed`]. Port of c-RCP's `rcp_e2e_unwrap_framed`
/// (`c-RCP/src/e2e.c:357-368`).
//fusa:req REQ-E2E-035
pub fn unwrap_framed(
    stream_id: u64,
    is_ntscf_framed: bool,
    header_octet1: u8,
    tu: bool,
    avtp_timestamp: u32,
    frame: &[u8],
) -> Result<Vec<u8>, E2eError> {
    let (subtype, tu, ts) = framed_inputs(is_ntscf_framed, tu, avtp_timestamp);
    unwrap(subtype, header_octet1, tu, stream_id, ts, frame)
}

/// [`wrap_framed`] with every CRC input read from `header`: framing from
/// its variant, `header_octet1` from [`HeaderVariant::octet1`], `tu` and
/// `avtp_timestamp` from a TSCF header (forced to `false`/`0` for NTSCF),
/// and `stream_id`.
//fusa:req REQ-CRC-020
pub fn wrap_for_header(header: &HeaderVariant, acf_frame: &[u8]) -> Result<Vec<u8>, RcpError> {
    let (is_ntscf, octet1, tu, stream_id, ts) = header_inputs(header);
    wrap_framed(stream_id, is_ntscf, octet1, tu, ts, acf_frame)
}

/// [`unwrap_framed`] with every CRC input read from `header`, as in
/// [`wrap_for_header`].
//fusa:req REQ-CRC-020
pub fn unwrap_for_header(header: &HeaderVariant, frame: &[u8]) -> Result<Vec<u8>, E2eError> {
    let (is_ntscf, octet1, tu, stream_id, ts) = header_inputs(header);
    unwrap_framed(stream_id, is_ntscf, octet1, tu, ts, frame)
}

// ── Fragmentation ────────────────────────────────────────────────────────────

/// Whether a fragment carries the CRC: exactly `is_last_fragment`. Port of
/// c-RCP's `rcp_e2e_fragment_carries_crc` (`c-RCP/src/e2e.c:373-376`).
//fusa:req REQ-E2E-010
pub fn fragment_carries_crc(is_last_fragment: bool) -> bool {
    is_last_fragment
}

/// CRC32 for a fragment train: the [`compute_crc`] header prefix, then
/// `first_fragment_header` (the first fragment's encoded ACF header, 8
/// octets for ACF_ABB or 16 for ACF_GBB), then `reassembled_payload` (every
/// segment's payload, in order), as one continuous CRC. Port of c-RCP's
/// `rcp_e2e_compute_fragmented_crc` (`c-RCP/src/e2e.c:379-406`). There is
/// no `_framed` form: under NTSCF pass `tu = false` and
/// `avtp_timestamp = 0`, as c-RCP's caller does (`c-RCP/src/mock.c:2307`).
///
/// c-RCP parity: the first fragment's header is fed as sent, with its own
/// `acf_msg_length`, `pad`, `ms = 1` and `segment_num = 0`, and without the
/// +1 quadlet. TC18-13.6-007 ("Fragmentation ... happens after CRC
/// calculation") reads as a header for the whole message instead.
//fusa:req REQ-E2E-038
pub fn compute_fragmented_crc(
    avtp_subtype: u8,
    header_octet1: u8,
    tu: bool,
    stream_id: u64,
    avtp_timestamp: u32,
    first_fragment_header: &[u8],
    reassembled_payload: &[u8],
) -> u32 {
    let prefix = coverage_prefix(avtp_subtype, header_octet1, tu, stream_id, avtp_timestamp);
    let mut crc = crc32_tc18_update(CRC32_TC18_INIT_XOROUT, &prefix);
    crc = crc32_tc18_update(crc, first_fragment_header);
    crc32_tc18_update(crc, reassembled_payload) ^ CRC32_TC18_INIT_XOROUT
}

// ── Typed-message helpers (legacy surface, re-implemented on the above) ─────

/// Which ACF message shape a coverage helper reads its header from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcfCoverageMessage<'a> {
    /// An ACF_ABB message (8-octet header, no `message_timestamp`).
    Abb(&'a AcfAbbMessage),
    /// An ACF_GBB message (16-octet header including `message_timestamp`).
    Gbb(&'a AcfGbbMessage),
}

/// The CRC coverage bytes for one ACF message: the 15-octet header prefix
/// (subtype, octet 1, `tu`, `stream_id`, `avtp_timestamp`; NTSCF forcing
/// as in [`wrap_framed`]), then `byte_message_info` with `acf_msg_type` forced
/// and `acf_msg_length` +1, then (ACF_GBB only) `message_timestamp`, then
/// the payload. `crc32_tc18` of the result equals the CRC [`wrap`] places
/// when `acf`'s `acf_msg_length`/`pad` match its payload (as
/// [`acf::decode_acf_abb`]/[`acf::decode_acf_gbb`] return them); unlike
/// [`wrap`], the caller's `acf_msg_length`/`pad` are used verbatim.
///
/// c-RCP parity: under NTSCF the prefix still carries a zero `tu` octet and
/// four zero timestamp octets (`c-RCP/src/e2e.c:140-148`), although TC18
/// RC5 Figure 21 has neither field.
///
/// Returns `Err(RcpError::InvalidSize)` if the adjusted header fails
/// [`acf::encode_byte_message_info`]'s field-width checks.
//fusa:req REQ-CRC-004
//fusa:req REQ-CRC-005
//fusa:req REQ-CRC-006
//fusa:req REQ-CRC-007
//fusa:req REQ-CRC-017
//fusa:req REQ-CRC-019
#[deprecated(
    since = "6.0.0",
    note = "use e2e::wrap_for_header / e2e::compute_crc (c-RCP rcp_e2e_wrap)"
)]
pub fn build_crc32_coverage_buffer(
    header: &HeaderVariant,
    acf: &AcfCoverageMessage,
) -> Result<Vec<u8>, RcpError> {
    let mut buf = header_coverage_prefix(header).to_vec();
    let (info, msg_type, timestamp, payload) = match acf {
        AcfCoverageMessage::Abb(m) => (m.info, acf::ACF_ABB_MSG_TYPE, None, &m.payload),
        AcfCoverageMessage::Gbb(m) => (
            m.info,
            acf::ACF_GBB_MSG_TYPE,
            Some(m.message_timestamp),
            &m.payload,
        ),
    };
    let adjusted = acf::ByteMessageInfo {
        acf_msg_type: msg_type,
        acf_msg_length: info
            .acf_msg_length
            .saturating_add(CRC32_COVERAGE_LENGTH_PREADJUST_QUADLETS),
        ..info
    };
    buf.extend_from_slice(&acf::encode_byte_message_info(&adjusted)?);
    if let Some(ts) = timestamp {
        buf.extend_from_slice(&ts.to_be_bytes());
    }
    buf.extend_from_slice(payload);
    Ok(buf)
}

/// Protects an already-encoded ACF frame with a caller-computed `crc`:
/// `acf_msg_length` +1 and `crc`'s 4 big-endian octets inserted between
/// the real payload and the pad octets (located via the frame's `pad`
/// field) — the same `[header][real payload][CRC32][pad]` layout as
/// [`wrap`]. Use it to place [`compute_fragmented_crc`]'s value on a
/// fragment train's final fragment; for a single message use [`wrap`].
///
/// Returns `Err(RcpError::ShortFrame)` if `frame` is shorter than
/// [`acf::BYTE_MESSAGE_INFO_LEN`] and `Err(RcpError::InvalidSize)` if
/// `acf_msg_length` is already `0x1FF`; `frame` is unchanged on error.
//fusa:req REQ-CRC-012
pub fn finalize_crc_trailer(frame: &mut Vec<u8>, crc: u32) -> Result<(), RcpError> {
    if frame.len() < acf::BYTE_MESSAGE_INFO_LEN {
        return Err(RcpError::ShortFrame);
    }
    *frame = insert_trailer(frame, |_| crc)?;
    Ok(())
}

/// Reverses [`finalize_crc_trailer`] without checking the CRC: returns
/// `(body, crc)`, where `crc` is read from right after the real payload
/// (located via the frame's `pad` field) and `body` is the frame with
/// those 4 octets removed and `acf_msg_length` -1, ready for
/// [`acf::decode_acf_abb`]/[`acf::decode_acf_gbb`]. Same split as
/// [`unwrap`] and as c-RCP's final-fragment handling
/// (`c-RCP/src/mock.c:2323-2341`).
///
/// Returns `Err(RcpError::ShortFrame)` if `frame` is shorter than
/// [`acf::BYTE_MESSAGE_INFO_LEN`] + [`CRC_TRAILER_LEN`], and
/// `Err(RcpError::InvalidSize)` if `acf_msg_length` is 0.
//fusa:req REQ-CRC-013
pub fn split_crc_trailer(frame: &[u8]) -> Result<(Vec<u8>, u32), RcpError> {
    if frame.len() < acf::BYTE_MESSAGE_INFO_LEN + CRC_TRAILER_LEN {
        return Err(RcpError::ShortFrame);
    }
    let (mut body, real_len, crc) = split_trailer(frame).ok_or(RcpError::ShortFrame)?;
    if !adapt_acf_msg_length(&mut body[..real_len], -i32::from(CRC_TRAILER_QUADLETS)) {
        return Err(RcpError::InvalidSize);
    }
    Ok((body, crc))
}

/// The combined payload of a fragment train: each segment's payload
/// concatenated in the order given.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CombinedFragmentPayload(pub Vec<u8>);

impl CombinedFragmentPayload {
    /// Concatenates `segments` verbatim, in order. Never panics; an empty
    /// slice gives an empty payload.
    //fusa:req REQ-CRC-009
    pub fn assemble(segments: &[&[u8]]) -> Self {
        CombinedFragmentPayload(segments.concat())
    }
}

/// The first fragment's encoded ACF header (8 octets ACF_ABB, 16 ACF_GBB)
/// exactly as [`acf::encode_acf_abb`]/[`acf::encode_acf_gbb`] put it on
/// the wire: `acf_msg_length`/`pad` derived from that fragment's own
/// payload.
fn encoded_first_fragment_header(first: &AcfCoverageMessage) -> Result<Vec<u8>, RcpError> {
    let (mut frame, header_len) = match first {
        AcfCoverageMessage::Abb(m) => (acf::encode_acf_abb(m)?, acf::ACF_ABB_HEADER_LEN),
        AcfCoverageMessage::Gbb(m) => (acf::encode_acf_gbb(m)?, acf::ACF_GBB_HEADER_LEN),
    };
    frame.truncate(header_len);
    Ok(frame)
}

/// The CRC coverage bytes for a fragment train: the 15-octet header
/// prefix (as [`build_crc32_coverage_buffer`]), then `first_fragment`'s
/// encoded ACF header as sent (no +1, its own length and pad), then the
/// concatenated `segments`. `crc32_tc18` of the result equals
/// [`crc32_tc18_for_fragment_train`].
///
/// c-RCP parity: first-fragment header fed as-is, as
/// `rcp_e2e_compute_fragmented_crc` (`c-RCP/src/e2e.c:379-406`); see
/// [`compute_fragmented_crc`].
///
/// Returns `Err(RcpError::InvalidSize)` if `first_fragment` cannot be
/// encoded.
//fusa:req REQ-CRC-010
#[deprecated(
    since = "6.0.0",
    note = "use e2e::crc32_tc18_for_fragment_train / e2e::compute_fragmented_crc"
)]
pub fn build_crc32_coverage_buffer_for_fragment_train(
    header: &HeaderVariant,
    first_fragment: &AcfCoverageMessage,
    segments: &[&[u8]],
) -> Result<Vec<u8>, RcpError> {
    let mut buf = header_coverage_prefix(header).to_vec();
    buf.extend_from_slice(&encoded_first_fragment_header(first_fragment)?);
    buf.extend_from_slice(&CombinedFragmentPayload::assemble(segments).0);
    Ok(buf)
}

/// The fragment-train CRC for a decoded AVTPDU header: [`compute_fragmented_crc`]
/// over `header`'s CRC inputs (NTSCF forcing as in [`wrap_framed`]),
/// `first_fragment`'s encoded ACF header and the concatenated `segments`.
/// The train's final fragment carries the result
/// ([`finalize_crc_trailer`]).
///
/// c-RCP parity: first-fragment header as sent, see
/// [`compute_fragmented_crc`].
///
/// Returns `Err(RcpError::InvalidSize)` if `first_fragment` cannot be
/// encoded.
//fusa:req REQ-CRC-010
pub fn crc32_tc18_for_fragment_train(
    header: &HeaderVariant,
    first_fragment: &AcfCoverageMessage,
    segments: &[&[u8]],
) -> Result<u32, RcpError> {
    let first_header = encoded_first_fragment_header(first_fragment)?;
    let combined = CombinedFragmentPayload::assemble(segments);
    let (is_ntscf, octet1, tu, stream_id, ts) = header_inputs(header);
    let (subtype, tu, ts) = framed_inputs(is_ntscf, tu, ts);
    Ok(compute_fragmented_crc(
        subtype,
        octet1,
        tu,
        stream_id,
        ts,
        &first_header,
        &combined.0,
    ))
}

/// Whether a CRC is expected on a fragment, from its `ms` flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FragmentCrcExpectation {
    /// `ms == true`: more segments follow; no CRC on this fragment.
    NotExpected,
    /// `ms == false`: the final fragment; it carries the train's CRC.
    Expected,
}

/// [`FragmentCrcExpectation`] from `ms` alone: only the last fragment
/// carries the CRC (TC18-13.6-007; [`fragment_carries_crc`] with
/// `is_last_fragment = !ms`).
//fusa:req REQ-CRC-008
pub fn fragment_crc_expectation(ms: bool) -> FragmentCrcExpectation {
    if fragment_carries_crc(!ms) {
        FragmentCrcExpectation::Expected
    } else {
        FragmentCrcExpectation::NotExpected
    }
}

/// Checks a fragment's CRC presence against [`fragment_crc_expectation`]:
/// `Ok(())` for no CRC with `ms == true` or a CRC with `ms == false`,
/// otherwise `Err(RcpError::InvalidParameter)`.
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
#[allow(deprecated)]
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

    // ── Coverage-buffer fixtures ────────────────────────────────────────────

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

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// Offset of the ACF header inside a coverage buffer: the 15-octet
    /// header-CRC prefix.
    const ACF_AT: usize = COVERAGE_PREFIX_LEN;

    // ── c-RCP golden vectors (#185) ─────────────────────────────────────────
    //
    // Generated from c-RCP's real code (c-RCP a4f3a1f, built out of tree):
    // a C harness calling rcp_acf_encode_abb/_gbb, rcp_e2e_wrap_framed,
    // rcp_e2e_unwrap_framed, rcp_e2e_compute_crc and
    // rcp_e2e_compute_fragmented_crc. Each const is that harness's output.

    /// Vector 1, NTSCF + ACF_ABB: byte_bus_id 0x123, transaction_num 0x5A,
    /// op = write, hs = 1, payload A1..A6 (pad 2); stream_id
    /// 0x0011223344556677, header_octet1 0x80. Wrapped with
    /// `rcp_e2e_wrap_framed(sid, true, 0x80, tu = true, ts = 0xDEADBEEF, ..)`
    /// — the NTSCF forcing must ignore that tu/ts.
    const V1_PLAIN: &str = "1c048123025a8000a1a2a3a4a5a60000";
    const V1_WRAPPED: &str = "1c058123025a8000a1a2a3a4a5a6627bd68e0000";
    const V1_SID: u64 = 0x0011_2233_4455_6677;

    /// Vector 2, TSCF + ACF_GBB: byte_bus_id 0x7FF, transaction_num 0x33,
    /// op = read, read_size 4, mtv = 1, cs = 1, message_timestamp
    /// 0x0102030405060708, payload B1..B5 (pad 3); stream_id
    /// 0x8899AABBCCDDEEFF, header_octet1 0x89 (sv, mr, tv), tu = 1,
    /// avtp_timestamp 0xCAFEBABE.
    const V2_PLAIN: &str = "1a06e7ff013300040102030405060708b1b2b3b4b5000000";
    const V2_WRAPPED: &str = "1a07e7ff013300040102030405060708b1b2b3b4b53ea965a3000000";
    const V2_SID: u64 = 0x8899_AABB_CCDD_EEFF;
    const V2_TS: u32 = 0xCAFE_BABE;

    /// Vector 3, NTSCF fragment train, ACF_ABB: byte_bus_id 0x042,
    /// transaction_num 0x77, op = write, payload C0..CC (13 octets) split
    /// 5/5/3; stream_id 0x0102030405060708, header_octet1 0x80.
    /// `V3_CRC` = rcp_e2e_compute_fragmented_crc(0x82, 0x80, false, sid, 0,
    /// frag0[..8], C0..CC); `V3_FRAG2_WIRE` is the final fragment as
    /// rcp_e2e_wrap_framed lays it out, with `V3_CRC` in the trailer slot.
    const V3_FRAG0: &str = "1c04c04200779000c0c1c2c3c4000000";
    const V3_FRAG1: &str = "1c04c04200779001c5c6c7c8c9000000";
    const V3_FRAG2: &str = "1c03404200778000cacbcc00";
    const V3_FRAG2_WIRE: &str = "1c04404200778000cacbcc81850db700";
    const V3_CRC: u32 = 0x8185_0db7;
    const V3_SID: u64 = 0x0102_0304_0506_0708;

    fn v1_info() -> acf::ByteMessageInfo {
        acf::ByteMessageInfo {
            byte_bus_id: 0x123,
            transaction_num: 0x5A,
            op: true,
            hs: true,
            ..Default::default()
        }
    }

    fn v2_message() -> AcfGbbMessage {
        AcfGbbMessage {
            info: acf::ByteMessageInfo {
                byte_bus_id: 0x7FF,
                transaction_num: 0x33,
                op: false,
                read_size_segment: acf::ReadSizeOrSegment(4),
                mtv: true,
                cs: true,
                ..Default::default()
            },
            message_timestamp: 0x0102_0304_0506_0708,
            payload: vec![0xB1, 0xB2, 0xB3, 0xB4, 0xB5],
        }
    }

    fn v1_header() -> HeaderVariant {
        HeaderVariant::Ntscf(avtp::NtscfHeader {
            stream_id: V1_SID,
            ntscf_data_length: 20,
            ..Default::default()
        })
    }

    fn v2_header() -> HeaderVariant {
        HeaderVariant::Tscf(avtp::TscfHeader {
            stream_id: V2_SID,
            version: 0,
            mr: true,
            tv: true,
            tu: true,
            avtp_timestamp: V2_TS,
            ..Default::default()
        })
    }

    fn v3_message() -> AcfAbbMessage {
        AcfAbbMessage {
            info: acf::ByteMessageInfo {
                byte_bus_id: 0x042,
                transaction_num: 0x77,
                op: true,
                ..Default::default()
            },
            payload: (0xC0..=0xCC).collect(),
        }
    }

    #[test]
    //fusa:test REQ-E2E-005
    //fusa:test REQ-E2E-038
    fn embedded_golden_vectors_match_the_c_rcp_harness_output() {
        // testdata/c_rcp_e2e_golden.txt is the harness's raw output
        // (testdata/c_rcp_e2e_golden.c); the consts above must not drift.
        let golden = include_str!("../testdata/c_rcp_e2e_golden.txt");
        let field = |key: &str| -> &str {
            golden
                .lines()
                .find(|l| l.starts_with(key))
                .and_then(|l| l.rsplit([' ', '=']).next())
                .unwrap_or_else(|| panic!("{key} missing from golden file"))
        };
        assert_eq!(field("v1 plain"), V1_PLAIN);
        assert_eq!(field("v1 wrapped"), V1_WRAPPED);
        assert_eq!(field("v2 plain"), V2_PLAIN);
        assert_eq!(field("v2 wrapped"), V2_WRAPPED);
        assert_eq!(field("v2 unwrap wrong tu"), "2");
        assert_eq!(field("v3 frag0 plain"), V3_FRAG0);
        assert_eq!(field("v3 frag1 plain"), V3_FRAG1);
        assert_eq!(field("v3 frag2 plain"), V3_FRAG2);
        assert_eq!(field("v3 frag2 wire"), V3_FRAG2_WIRE);
        assert_eq!(field("v3 fragmented crc"), format!("{V3_CRC:08x}"));
        assert_eq!(field("kat compute_crc="), "454c3806");
        assert_eq!(field("kat compute_crc_empty"), "f8670df6");
    }

    #[test]
    //fusa:test REQ-E2E-005
    //fusa:test REQ-E2E-035
    fn wrap_framed_matches_c_rcp_ntscf_abb_golden_vector() {
        let plain = acf::encode_acf_abb(&AcfAbbMessage {
            info: v1_info(),
            payload: vec![0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6],
        })
        .unwrap();
        assert_eq!(plain, hex(V1_PLAIN), "rust ACF_ABB encoding == c-RCP's");
        let wrapped = wrap_framed(V1_SID, true, 0x80, true, 0xDEAD_BEEF, &plain).unwrap();
        assert_eq!(wrapped, hex(V1_WRAPPED));
        // Wire order payload, CRC, pad (RC5 Figure 20/21).
        assert_eq!(&wrapped[8..14], &[0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6]);
        assert_eq!(&wrapped[14..18], &0x627b_d68eu32.to_be_bytes());
        assert_eq!(&wrapped[18..20], &[0, 0]);
    }

    #[test]
    //fusa:test REQ-E2E-005
    //fusa:test REQ-E2E-035
    fn wrap_framed_matches_c_rcp_tscf_gbb_golden_vector() {
        let plain = acf::encode_acf_gbb(&v2_message()).unwrap();
        assert_eq!(plain, hex(V2_PLAIN), "rust ACF_GBB encoding == c-RCP's");
        let wrapped = wrap_framed(V2_SID, false, 0x89, true, V2_TS, &plain).unwrap();
        assert_eq!(wrapped, hex(V2_WRAPPED));
    }

    #[test]
    //fusa:test REQ-CRC-020
    fn wrap_for_header_matches_c_rcp_golden_vectors() {
        assert_eq!(v1_header().octet1(), 0x80);
        assert_eq!(v2_header().octet1(), 0x89);
        assert_eq!(
            wrap_for_header(&v1_header(), &hex(V1_PLAIN)).unwrap(),
            hex(V1_WRAPPED)
        );
        assert_eq!(
            wrap_for_header(&v2_header(), &hex(V2_PLAIN)).unwrap(),
            hex(V2_WRAPPED)
        );
        assert_eq!(
            unwrap_for_header(&v1_header(), &hex(V1_WRAPPED)),
            Ok(hex(V1_PLAIN))
        );
        assert_eq!(
            unwrap_for_header(&v2_header(), &hex(V2_WRAPPED)),
            Ok(hex(V2_PLAIN))
        );
    }

    #[test]
    //fusa:test REQ-CRC-020
    fn unwrap_for_header_rejects_a_frame_wrapped_for_another_header() {
        let wrapped = wrap_for_header(&v2_header(), &hex(V2_PLAIN)).unwrap();
        let mut other = v2_header();
        if let HeaderVariant::Tscf(h) = &mut other {
            h.mr = false; // header_octet1 changes
        }
        assert!(matches!(
            unwrap_for_header(&other, &wrapped),
            Err(E2eError::CrcMismatch { .. })
        ));
    }

    #[test]
    //fusa:test REQ-E2E-009
    fn unwrap_framed_recovers_c_rcp_golden_vectors() {
        assert_eq!(
            unwrap_framed(V1_SID, true, 0x80, false, 0, &hex(V1_WRAPPED)),
            Ok(hex(V1_PLAIN))
        );
        assert_eq!(
            unwrap_framed(V2_SID, false, 0x89, true, V2_TS, &hex(V2_WRAPPED)),
            Ok(hex(V2_PLAIN))
        );
        // The decoded body is a valid plain ACF message again.
        let body = unwrap_framed(V2_SID, false, 0x89, true, V2_TS, &hex(V2_WRAPPED)).unwrap();
        assert_eq!(
            acf::decode_acf_gbb(&body).unwrap().payload,
            v2_message().payload
        );
    }

    #[test]
    //fusa:test REQ-E2E-008
    fn unwrap_framed_rejects_c_rcp_vector_with_wrong_tu() {
        // c-RCP harness: rcp_e2e_unwrap_framed(.., tu = false, ..) on V2
        // returns RCP_E2E_ERR_CRC_MISMATCH (2).
        let err = unwrap_framed(V2_SID, false, 0x89, false, V2_TS, &hex(V2_WRAPPED)).unwrap_err();
        assert_eq!(err.code(), 2);
        assert_eq!(
            err,
            E2eError::CrcMismatch {
                acf_frame: hex(V2_PLAIN)
            }
        );
    }

    #[test]
    //fusa:test REQ-E2E-003
    fn compute_crc_matches_c_rcp_known_answers() {
        assert_eq!(
            compute_crc(
                0x05,
                0x81,
                true,
                0x0102_0304_0506_0708,
                0x1122_3344,
                &[1, 2, 3, 4]
            ),
            0x454c_3806
        );
        assert_eq!(compute_crc(0x82, 0x80, false, 0, 0, &[]), 0xf867_0df6);
    }

    #[test]
    //fusa:test REQ-E2E-003
    fn compute_crc_equals_crc32_over_the_concatenated_regions() {
        let acf_frame = b"acf-bytes";
        let mut concat = vec![0x05, 0x89, 0x01];
        concat.extend_from_slice(&0x0102_0304_0506_0708u64.to_be_bytes());
        concat.extend_from_slice(&0x1122_3344u32.to_be_bytes());
        concat.extend_from_slice(acf_frame);
        assert_eq!(
            compute_crc(
                0x05,
                0x89,
                true,
                0x0102_0304_0506_0708,
                0x1122_3344,
                acf_frame
            ),
            crc32_tc18(&concat)
        );
        // Every input moves the CRC.
        let base = compute_crc(0x05, 0x89, true, 1, 2, acf_frame);
        assert_ne!(base, compute_crc(0x82, 0x89, true, 1, 2, acf_frame));
        assert_ne!(base, compute_crc(0x05, 0x88, true, 1, 2, acf_frame));
        assert_ne!(base, compute_crc(0x05, 0x89, false, 1, 2, acf_frame));
        assert_ne!(base, compute_crc(0x05, 0x89, true, 3, 2, acf_frame));
        assert_ne!(base, compute_crc(0x05, 0x89, true, 1, 4, acf_frame));
        assert_ne!(base, compute_crc(0x05, 0x89, true, 1, 2, b"acf-bytez"));
    }

    #[test]
    //fusa:test REQ-E2E-004
    fn length_with_crc_adds_one_quadlet_and_saturates() {
        assert_eq!(length_with_crc(0), 4);
        assert_eq!(length_with_crc(16), 20);
        assert_eq!(length_with_crc(usize::MAX - 4), usize::MAX);
        assert_eq!(length_with_crc(usize::MAX - 3), usize::MAX);
        assert_eq!(length_with_crc(usize::MAX), usize::MAX);
    }

    #[test]
    //fusa:test REQ-E2E-037
    fn data_length_for_protected_members_is_four_per_member_and_saturates() {
        assert_eq!(data_length_for_protected_members(0), 0);
        assert_eq!(data_length_for_protected_members(1), 4);
        assert_eq!(data_length_for_protected_members(5), 20);
        assert_eq!(data_length_for_protected_members(usize::MAX), usize::MAX);
    }

    #[test]
    //fusa:test REQ-E2E-005
    fn wrap_places_crc_after_real_payload_and_bumps_length() {
        for payload_len in 0..8usize {
            let payload: Vec<u8> = (1..=payload_len as u8).collect();
            let plain = acf::encode_acf_abb(&AcfAbbMessage {
                info: acf::ByteMessageInfo::default(),
                payload: payload.clone(),
            })
            .unwrap();
            let pad = (4 - (payload_len % 4)) % 4;
            let real_len = 8 + payload_len;
            let wrapped = wrap(0x82, 0x80, false, 7, 0, &plain).unwrap();
            assert_eq!(wrapped.len(), plain.len() + CRC_TRAILER_LEN);
            let mut adjusted = plain[..real_len].to_vec();
            adjusted[1] += 1;
            let crc = compute_crc(0x82, 0x80, false, 7, 0, &adjusted);
            assert_eq!(&wrapped[..real_len], &adjusted[..]);
            assert_eq!(&wrapped[real_len..real_len + 4], &crc.to_be_bytes());
            assert_eq!(&wrapped[real_len + 4..], &vec![0u8; pad][..]);
            assert_eq!(unwrap(0x82, 0x80, false, 7, 0, &wrapped), Ok(plain));
        }
    }

    #[test]
    //fusa:test REQ-E2E-005
    fn wrap_carries_pad_octets_through_verbatim() {
        // c-RCP copies the caller's pad octets after the trailer rather
        // than re-zeroing them, and unwrap returns them unchanged.
        let mut plain = hex(V1_PLAIN);
        plain[14] = 0xEE;
        plain[15] = 0xFF;
        let wrapped = wrap_framed(V1_SID, true, 0x80, false, 0, &plain).unwrap();
        assert_eq!(&wrapped[18..20], &[0xEE, 0xFF]);
        assert_eq!(&wrapped[..18], &hex(V1_WRAPPED)[..18]);
        assert_eq!(
            unwrap_framed(V1_SID, true, 0x80, false, 0, &wrapped),
            Ok(plain)
        );
    }

    #[test]
    //fusa:test REQ-E2E-006
    fn wrap_fails_safe_on_invalid_input() {
        // Not a whole number of quadlets.
        assert_eq!(
            wrap(0x82, 0, false, 0, 0, &[0u8; 9]),
            Err(RcpError::InvalidSize)
        );
        // Empty: no length field to adjust.
        assert_eq!(wrap(0x82, 0, false, 0, 0, &[]), Err(RcpError::ShortFrame));
        // Four octets with pad = 3: real prefix of 1 octet.
        assert_eq!(
            wrap(0x82, 0, false, 0, 0, &[0x1C, 0x01, 0xC0, 0x00]),
            Err(RcpError::ShortFrame)
        );
        // acf_msg_length already 0x1FF.
        let mut full = vec![0u8; 8];
        full[0] = (acf::ACF_ABB_MSG_TYPE << 1) | 1;
        full[1] = 0xFF;
        assert_eq!(
            wrap(0x82, 0, false, 0, 0, &full),
            Err(RcpError::InvalidSize)
        );
        // 0x1FE still wraps, to 0x1FF, keeping acf_msg_type.
        full[1] = 0xFE;
        let w = wrap(0x82, 0, false, 0, 0, &full).unwrap();
        assert_eq!((w[0], w[1]), ((acf::ACF_ABB_MSG_TYPE << 1) | 1, 0xFF));
        // Length 0x0FF carries into bit 8.
        full[0] = acf::ACF_ABB_MSG_TYPE << 1;
        full[1] = 0xFF;
        let w = wrap(0x82, 0, false, 0, 0, &full).unwrap();
        assert_eq!((w[0], w[1]), ((acf::ACF_ABB_MSG_TYPE << 1) | 1, 0x00));
    }

    #[test]
    //fusa:test REQ-E2E-007
    fn unwrap_rejects_short_frames() {
        assert_eq!(unwrap(0x82, 0, false, 0, 0, &[]), Err(E2eError::ShortFrame));
        assert_eq!(
            unwrap(0x82, 0, false, 0, 0, &[0u8; 3]),
            Err(E2eError::ShortFrame)
        );
        // pad field claims 3 octets but only the trailer fits.
        assert_eq!(
            unwrap(0x82, 0, false, 0, 0, &[0x00, 0x00, 0xC0, 0x00, 0x00, 0x00]),
            Err(E2eError::ShortFrame)
        );
        // Exactly trailer + claimed pad: accepted as far as the split goes.
        assert!(matches!(
            unwrap(0x82, 0, false, 0, 0, &[0x00, 0x00, 0x40, 0x00, 0x00]),
            Err(E2eError::CrcMismatch { .. }) | Ok(_)
        ));
    }

    #[test]
    //fusa:test REQ-E2E-008
    fn unwrap_reports_crc_mismatch_on_any_corruption() {
        let wrapped = hex(V1_WRAPPED);
        for i in 0..18 {
            let mut bad = wrapped.clone();
            bad[i] ^= 0x01;
            let got = unwrap_framed(V1_SID, true, 0x80, false, 0, &bad);
            assert!(
                matches!(got, Err(E2eError::CrcMismatch { .. })),
                "bit flip at octet {i} must be a CRC mismatch, got {got:?}"
            );
        }
        // Pad octets are not covered.
        let mut pad_changed = wrapped.clone();
        pad_changed[19] = 0x55;
        assert!(unwrap_framed(V1_SID, true, 0x80, false, 0, &pad_changed).is_ok());
        // Wrong stream_id.
        assert!(matches!(
            unwrap_framed(V1_SID + 1, true, 0x80, false, 0, &wrapped),
            Err(E2eError::CrcMismatch { .. })
        ));
    }

    #[test]
    //fusa:test REQ-E2E-009
    fn unwrap_round_trips_wrap_for_gbb_and_abb() {
        let gbb = acf::encode_acf_gbb(&sample_gbb_message(0, 0x55, b"gbb")).unwrap();
        let abb = acf::encode_acf_abb(&sample_abb_message(0, b"abcd")).unwrap();
        for frame in [gbb, abb] {
            let w = wrap(0x05, 0x81, true, 9, 10, &frame).unwrap();
            assert_eq!(unwrap(0x05, 0x81, true, 9, 10, &w), Ok(frame));
        }
    }

    #[test]
    //fusa:test REQ-E2E-007
    fn unwrap_leaves_length_unadjusted_when_it_cannot_decrement() {
        // acf_msg_length 0 and a CRC computed over that: c-RCP leaves the
        // length untouched (adapt fails) and still reports OK.
        let real = [0x1Cu8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let crc = compute_crc(0x82, 0, false, 0, 0, &real);
        let mut frame = real.to_vec();
        frame.extend_from_slice(&crc.to_be_bytes());
        assert_eq!(unwrap(0x82, 0, false, 0, 0, &frame), Ok(real.to_vec()));
    }

    #[test]
    //fusa:test REQ-E2E-035
    fn framed_variants_force_ntscf_tu_and_timestamp_only() {
        let plain = hex(V1_PLAIN);
        let a = wrap_framed(5, true, 0x80, true, 0x1234, &plain).unwrap();
        let b = wrap_framed(5, true, 0x80, false, 0, &plain).unwrap();
        assert_eq!(a, b, "NTSCF ignores tu/avtp_timestamp");
        assert_eq!(
            a,
            wrap(avtp::NTSCF_SUBTYPE, 0x80, false, 5, 0, &plain).unwrap()
        );
        let t1 = wrap_framed(5, false, 0x81, true, 0x1234, &plain).unwrap();
        let t2 = wrap_framed(5, false, 0x81, false, 0x1234, &plain).unwrap();
        let t3 = wrap_framed(5, false, 0x81, true, 0x1235, &plain).unwrap();
        assert_ne!(t1, t2, "TSCF tu is covered");
        assert_ne!(t1, t3, "TSCF avtp_timestamp is covered");
        assert_eq!(
            t1,
            wrap(avtp::TSCF_SUBTYPE, 0x81, true, 5, 0x1234, &plain).unwrap()
        );
        assert_eq!(
            unwrap_framed(5, true, 0x80, true, 0x9999, &a),
            Ok(plain.clone())
        );
        assert_eq!(unwrap_framed(5, false, 0x81, true, 0x1234, &t1), Ok(plain));
    }

    #[test]
    //fusa:test REQ-E2E-001
    fn e2e_error_codes_and_messages_are_distinct() {
        let short = E2eError::ShortFrame;
        let mismatch = E2eError::CrcMismatch { acf_frame: vec![] };
        assert_eq!(short.code(), 1);
        assert_eq!(mismatch.code(), 2);
        assert_ne!(short.strerror(), mismatch.strerror());
        assert_eq!(
            short.to_string(),
            "e2e: frame too short for a CRC32 trailer"
        );
        assert_eq!(
            mismatch.to_string(),
            "e2e: POCI failure -- CRC32 mismatch, execution skipped"
        );
        let as_error: &dyn std::error::Error = &short;
        assert!(as_error.source().is_none());
    }

    #[test]
    //fusa:test REQ-WIREERR-003
    fn e2e_wire_error_maps_crc_mismatch_to_poci_failure() {
        let mismatch = E2eError::CrcMismatch { acf_frame: vec![1] };
        assert_eq!(mismatch.wire_error(), Some(RcpError::PociFailure));
        assert_eq!(RcpError::PociFailure.tc18_wire_code(), Some(12));
        assert_eq!(E2eError::ShortFrame.wire_error(), None);
    }

    // ── TC18 §13.6 Table 34 / ACF-specific / request-response symmetry ─────

    #[test]
    //fusa:test REQ-CRC-014
    fn poly_constant_is_the_bit_reversal_of_table_34_polynomial() {
        // TC18 §13.6 Table 34 "CRC32 Polynomial": 0xF4ACFB13, 32 bit,
        // init 0xFFFFFFFF, final XOR 0xFFFFFFFF, reflected in and out.
        const TABLE_34_POLYNOMIAL: u32 = 0xF4AC_FB13;
        assert_eq!(
            CRC32_TC18_POLY_REFLECTED,
            TABLE_34_POLYNOMIAL.reverse_bits()
        );
        assert_eq!(CRC32_TC18_INIT_XOROUT, 0xFFFF_FFFF);
        assert_eq!(u32::BITS, 32);
    }

    #[test]
    //fusa:test REQ-CRC-017
    fn crc32_is_computed_per_acf_type_individually() {
        // TC18 §13.6: "a CRC32 is calculated per ACF type". Two ACF
        // messages under one AVTPDU get two independent coverage buffers.
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

        assert!(!buf_first.windows(10).any(|w| w == b"second-acf"));
        assert!(!buf_second.windows(9).any(|w| w == b"first-acf"));
        assert_ne!(crc32_tc18(&buf_first), crc32_tc18(&buf_second));
    }

    #[test]
    //fusa:test REQ-CRC-019
    fn crc_scheme_is_identical_for_request_and_response() {
        // TC18 §13.6: "The CRC calculation for requests and responses
        // follows the identical scheme." Only the covered rsp bit differs.
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

        assert_eq!(buf_request.len(), buf_response.len());
        // rsp is octet 6 bit 6 of byte_message_info, which starts after the
        // 15-octet header prefix.
        let rsp_at = ACF_AT + 6;
        let differing: Vec<usize> = (0..buf_request.len())
            .filter(|&i| buf_request[i] != buf_response[i])
            .collect();
        assert_eq!(differing, vec![rsp_at]);
        assert_eq!(buf_request[rsp_at] ^ buf_response[rsp_at], 0x40);

        // The same holds through wrap: a request and a response differ in
        // the rsp bit and the CRC only.
        let req_frame = acf::encode_acf_abb(&request).unwrap();
        let rsp_frame = acf::encode_acf_abb(&response).unwrap();
        let wq = wrap_for_header(&header, &req_frame).unwrap();
        let wr = wrap_for_header(&header, &rsp_frame).unwrap();
        assert_eq!(unwrap_for_header(&header, &wq), Ok(req_frame));
        assert_eq!(unwrap_for_header(&header, &wr), Ok(rsp_frame));
    }

    // ── Legacy coverage buffer ──────────────────────────────────────────────

    #[test]
    //fusa:test REQ-CRC-004
    fn coverage_buffer_leads_with_the_c_rcp_header_prefix() {
        let header = HeaderVariant::Tscf(avtp::TscfHeader {
            stream_id: 0x0102_0304_0506_0708,
            avtp_timestamp: 0x1122_3344,
            tu: true,
            mr: true,
            tv: true,
            ..Default::default()
        });
        let acf = AcfCoverageMessage::Abb(&sample_abb_message(0, b"pl"));
        let buf = build_crc32_coverage_buffer(&header, &acf).unwrap();
        assert_eq!(buf[0], avtp::TSCF_SUBTYPE);
        assert_eq!(buf[1], 0x89);
        assert_eq!(buf[2], 0x01, "tu octet");
        assert_eq!(&buf[3..11], &0x0102_0304_0506_0708u64.to_be_bytes());
        assert_eq!(&buf[11..15], &0x1122_3344u32.to_be_bytes());
    }

    #[test]
    //fusa:test REQ-CRC-004
    fn coverage_buffer_crc_equals_the_crc_wrap_places() {
        // For a self-consistent message, crc32_tc18(coverage) is exactly
        // the trailer wrap_for_header writes.
        let msg = v2_message();
        let plain = acf::encode_acf_gbb(&msg).unwrap();
        let decoded = acf::decode_acf_gbb(&plain).unwrap();
        let coverage =
            build_crc32_coverage_buffer(&v2_header(), &AcfCoverageMessage::Gbb(&decoded)).unwrap();
        let wrapped = wrap_for_header(&v2_header(), &plain).unwrap();
        assert_eq!(&wrapped[21..25], &crc32_tc18(&coverage).to_be_bytes());

        let abb = AcfAbbMessage {
            info: v1_info(),
            payload: vec![0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6],
        };
        let decoded = acf::decode_acf_abb(&acf::encode_acf_abb(&abb).unwrap()).unwrap();
        let coverage =
            build_crc32_coverage_buffer(&v1_header(), &AcfCoverageMessage::Abb(&decoded)).unwrap();
        assert_eq!(
            &hex(V1_WRAPPED)[14..18],
            &crc32_tc18(&coverage).to_be_bytes()
        );
    }

    #[test]
    //fusa:test REQ-CRC-005
    fn coverage_buffer_feeds_zero_tu_and_timestamp_under_ntscf() {
        // c-RCP parity: the zero tu octet and the four zero timestamp
        // octets are present, not omitted.
        let header = HeaderVariant::Ntscf(avtp::NtscfHeader {
            stream_id: 0xAABB_CCDD_EEFF_0011,
            ..Default::default()
        });
        let acf = AcfCoverageMessage::Abb(&sample_abb_message(0, b"pl"));
        let buf = build_crc32_coverage_buffer(&header, &acf).unwrap();
        assert_eq!(buf[0], avtp::NTSCF_SUBTYPE);
        assert_eq!(buf[1], 0x80);
        assert_eq!(buf[2], 0x00);
        assert_eq!(&buf[11..15], &[0, 0, 0, 0]);
        assert_eq!(buf.len(), ACF_AT + acf::ACF_ABB_HEADER_LEN + 2);
    }

    #[test]
    //fusa:test REQ-CRC-005
    fn coverage_buffer_uses_real_tu_and_avtp_timestamp_under_tscf() {
        let header = HeaderVariant::Tscf(avtp::TscfHeader {
            stream_id: 0x1,
            avtp_timestamp: 0xDEAD_BEEF,
            tu: false,
            ..Default::default()
        });
        let acf = AcfCoverageMessage::Abb(&sample_abb_message(0, b"pl"));
        let buf = build_crc32_coverage_buffer(&header, &acf).unwrap();
        assert_eq!(buf[2], 0x00);
        assert_eq!(&buf[11..15], &0xDEAD_BEEFu32.to_be_bytes());
        let header_tu = HeaderVariant::Tscf(avtp::TscfHeader {
            stream_id: 0x1,
            avtp_timestamp: 0xDEAD_BEEF,
            tu: true,
            ..Default::default()
        });
        assert_eq!(
            build_crc32_coverage_buffer(&header_tu, &acf).unwrap()[2],
            0x01
        );
    }

    #[test]
    //fusa:test REQ-CRC-007
    fn coverage_buffer_abb_header_has_no_message_timestamp_region() {
        let header = HeaderVariant::Ntscf(avtp::NtscfHeader::default());
        let payload = b"abb-payload";
        let acf = AcfCoverageMessage::Abb(&sample_abb_message(0, payload));
        let buf = build_crc32_coverage_buffer(&header, &acf).unwrap();
        assert_eq!(buf.len(), ACF_AT + acf::ACF_ABB_HEADER_LEN + payload.len());
        let decoded_info = acf::decode_byte_message_info(&buf[ACF_AT..]).unwrap();
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
        assert_eq!(buf.len(), ACF_AT + acf::ACF_GBB_HEADER_LEN + payload.len());
        let decoded_info = acf::decode_byte_message_info(&buf[ACF_AT..]).unwrap();
        assert_eq!(decoded_info.acf_msg_type, acf::ACF_GBB_MSG_TYPE);
        let ts_start = ACF_AT + acf::BYTE_MESSAGE_INFO_LEN;
        assert_eq!(
            &buf[ts_start..ts_start + 8],
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
        let decoded =
            acf::decode_byte_message_info(&buf[ACF_AT..ACF_AT + acf::BYTE_MESSAGE_INFO_LEN])
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
        let acf = AcfCoverageMessage::Abb(&sample_abb_message(acf::ACF_MSG_LENGTH_9BIT_MAX, b"x"));
        assert_eq!(
            build_crc32_coverage_buffer(&header, &acf),
            Err(RcpError::InvalidSize)
        );
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

    // ── CRC trailer wire placement (TC18 RC5 Figures 20/21) ─────────────────

    #[test]
    //fusa:test REQ-CRC-012
    //fusa:test REQ-CRC-013
    fn finalize_crc_trailer_matches_figure_20_layout() {
        // Figure 20, ACF_ABB under TSCF: header(8), 6 payload octets,
        // CRC32(4), 2 pad octets = 20 octets = 5 quadlets.
        let real_payload: [u8; 6] = [0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6];
        let msg = AcfAbbMessage {
            info: acf::ByteMessageInfo::default(),
            payload: real_payload.to_vec(),
        };
        let mut frame = acf::encode_acf_abb(&msg).unwrap();
        assert_eq!(frame.len(), 16);
        let crc = 0x1122_3344;
        finalize_crc_trailer(&mut frame, crc).unwrap();
        assert_eq!(frame.len(), 20);
        let info = acf::decode_byte_message_info(&frame[..8]).unwrap();
        assert_eq!(info.acf_msg_length, 5);
        assert_eq!(info.pad, 2);
        assert_eq!(&frame[8..14], &real_payload);
        assert_eq!(
            &frame[14..18],
            &crc.to_be_bytes(),
            "CRC right after payload"
        );
        assert_eq!(&frame[18..20], &[0, 0], "pad after the CRC");

        let (body, decoded_crc) = split_crc_trailer(&frame).unwrap();
        assert_eq!(decoded_crc, crc);
        let decoded = acf::decode_acf_abb(&body).unwrap();
        assert_eq!(decoded.payload, real_payload.to_vec());
    }

    #[test]
    //fusa:test REQ-CRC-012
    //fusa:test REQ-CRC-013
    fn finalize_crc_trailer_matches_figure_21_layout() {
        // Figure 21 (RC5), ACF_GBB under NTSCF: header(8), timestamp(8),
        // 5 payload octets, CRC32(4), 3 pad octets = 28 octets = 7 quadlets.
        let real_payload: [u8; 5] = [0xB1, 0xB2, 0xB3, 0xB4, 0xB5];
        let message_timestamp: u64 = 0x0102_0304_0506_0708;
        let msg = AcfGbbMessage {
            // mtv=1: encode_acf_gbb zeroes an untimed timestamp (REQ-ACF-011).
            info: acf::ByteMessageInfo {
                mtv: true,
                ..acf::ByteMessageInfo::default()
            },
            message_timestamp,
            payload: real_payload.to_vec(),
        };
        let mut frame = acf::encode_acf_gbb(&msg).unwrap();
        assert_eq!(frame.len(), 24);
        let crc = 0xA1B2_C3D4;
        finalize_crc_trailer(&mut frame, crc).unwrap();
        assert_eq!(frame.len(), 28);
        let info = acf::decode_byte_message_info(&frame[..8]).unwrap();
        assert_eq!(info.acf_msg_length, 7);
        assert_eq!(info.pad, 3);
        assert_eq!(&frame[8..16], &message_timestamp.to_be_bytes());
        assert_eq!(&frame[16..21], &real_payload);
        assert_eq!(&frame[21..25], &crc.to_be_bytes());
        assert_eq!(&frame[25..28], &[0, 0, 0]);

        let (body, decoded_crc) = split_crc_trailer(&frame).unwrap();
        assert_eq!(decoded_crc, crc);
        let decoded = acf::decode_acf_gbb(&body).unwrap();
        assert_eq!(decoded.payload, real_payload.to_vec());
        assert_eq!(decoded.message_timestamp, message_timestamp);
    }

    #[test]
    //fusa:test REQ-CRC-012
    fn finalize_crc_trailer_with_wrap_crc_equals_wrap() {
        let plain = hex(V2_PLAIN);
        let wrapped = hex(V2_WRAPPED);
        let mut frame = plain.clone();
        finalize_crc_trailer(&mut frame, 0x3ea9_65a3).unwrap();
        assert_eq!(frame, wrapped);
    }

    #[test]
    //fusa:test REQ-CRC-013
    fn split_crc_trailer_rejects_short_frame() {
        let short = vec![0u8; acf::BYTE_MESSAGE_INFO_LEN + CRC_TRAILER_LEN - 1];
        assert_eq!(split_crc_trailer(&short), Err(RcpError::ShortFrame));
    }

    #[test]
    //fusa:test REQ-CRC-013
    fn split_crc_trailer_rejects_length_without_room_for_crc_trailer() {
        let info = acf::ByteMessageInfo {
            acf_msg_type: acf::ACF_ABB_MSG_TYPE,
            acf_msg_length: 0,
            ..Default::default()
        };
        let header = acf::encode_byte_message_info(&info).unwrap();
        let mut frame = header.to_vec();
        frame.extend_from_slice(&[0u8; CRC_TRAILER_LEN]);
        assert_eq!(split_crc_trailer(&frame), Err(RcpError::InvalidSize));
    }

    #[test]
    //fusa:test REQ-CRC-012
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
        assert_eq!(frame, header.to_vec(), "frame unchanged on error");
    }

    #[test]
    //fusa:test REQ-CRC-012
    fn finalize_crc_trailer_rejects_short_frame() {
        let mut short = vec![0u8; acf::BYTE_MESSAGE_INFO_LEN - 1];
        assert_eq!(
            finalize_crc_trailer(&mut short, 0),
            Err(RcpError::ShortFrame)
        );
    }

    // ── Fragmentation ───────────────────────────────────────────────────────

    #[test]
    //fusa:test REQ-E2E-010
    fn fragment_carries_crc_is_is_last_fragment() {
        assert!(fragment_carries_crc(true));
        assert!(!fragment_carries_crc(false));
    }

    #[test]
    //fusa:test REQ-E2E-038
    fn compute_fragmented_crc_matches_c_rcp_golden_vector() {
        let frag0 = hex(V3_FRAG0);
        let payload: Vec<u8> = (0xC0..=0xCC).collect();
        assert_eq!(
            compute_fragmented_crc(
                avtp::NTSCF_SUBTYPE,
                0x80,
                false,
                V3_SID,
                0,
                &frag0[..acf::ACF_ABB_HEADER_LEN],
                &payload
            ),
            V3_CRC
        );
    }

    #[test]
    //fusa:test REQ-E2E-038
    fn compute_fragmented_crc_equals_crc32_over_the_concatenation() {
        let hdr = [1u8, 2, 3, 4, 5, 6, 7, 8];
        let payload = b"reassembled";
        let mut concat = vec![0x05, 0x81, 0x01];
        concat.extend_from_slice(&9u64.to_be_bytes());
        concat.extend_from_slice(&10u32.to_be_bytes());
        concat.extend_from_slice(&hdr);
        concat.extend_from_slice(payload);
        assert_eq!(
            compute_fragmented_crc(0x05, 0x81, true, 9, 10, &hdr, payload),
            crc32_tc18(&concat)
        );
        // Same as compute_crc over header ++ payload.
        let mut acf_bytes = hdr.to_vec();
        acf_bytes.extend_from_slice(payload);
        assert_eq!(
            compute_fragmented_crc(0x05, 0x81, true, 9, 10, &hdr, payload),
            compute_crc(0x05, 0x81, true, 9, 10, &acf_bytes)
        );
    }

    #[test]
    //fusa:test REQ-CRC-010
    //fusa:test REQ-CRC-012
    //fusa:test REQ-CRC-013
    fn fragment_train_matches_c_rcp_golden_vector_end_to_end() {
        let fragments = crate::fragment::fragment_abb_message(&v3_message(), 5).unwrap();
        let encoded: Vec<Vec<u8>> = fragments
            .iter()
            .map(|f| acf::encode_acf_abb(f).unwrap())
            .collect();
        assert_eq!(encoded, vec![hex(V3_FRAG0), hex(V3_FRAG1), hex(V3_FRAG2)]);

        let header = HeaderVariant::Ntscf(avtp::NtscfHeader {
            stream_id: V3_SID,
            ..Default::default()
        });
        let segments: Vec<&[u8]> = fragments.iter().map(|f| f.payload.as_slice()).collect();
        let crc = crc32_tc18_for_fragment_train(
            &header,
            &AcfCoverageMessage::Abb(&fragments[0]),
            &segments,
        )
        .unwrap();
        assert_eq!(crc, V3_CRC);

        // The final fragment carries it where wrap would put its own CRC.
        let mut last = encoded[2].clone();
        finalize_crc_trailer(&mut last, crc).unwrap();
        assert_eq!(last, hex(V3_FRAG2_WIRE));

        // Receive side: split the final fragment, recompute, compare.
        let (body, got) = split_crc_trailer(&hex(V3_FRAG2_WIRE)).unwrap();
        assert_eq!(body, hex(V3_FRAG2));
        assert_eq!(got, crc);
    }

    #[test]
    //fusa:test REQ-CRC-010
    fn fragment_train_crc_uses_first_fragment_header_without_length_adjustment() {
        // c-RCP parity (#169/#170): the first fragment's header is fed as
        // sent — its own acf_msg_length (4), pad (3), ms = 1, segment 0 —
        // not the final fragment's, and without the +1 quadlet.
        let fragments = crate::fragment::fragment_abb_message(&v3_message(), 5).unwrap();
        let header = HeaderVariant::Ntscf(avtp::NtscfHeader {
            stream_id: V3_SID,
            ..Default::default()
        });
        let segments: Vec<&[u8]> = fragments.iter().map(|f| f.payload.as_slice()).collect();
        let buf = build_crc32_coverage_buffer_for_fragment_train(
            &header,
            &AcfCoverageMessage::Abb(&fragments[0]),
            &segments,
        )
        .unwrap();
        assert_eq!(&buf[ACF_AT..ACF_AT + 8], &hex(V3_FRAG0)[..8]);
        assert_eq!(&buf[ACF_AT + 8..], &v3_message().payload[..]);
        assert_eq!(crc32_tc18(&buf), V3_CRC);

        let with_final = crc32_tc18_for_fragment_train(
            &header,
            &AcfCoverageMessage::Abb(&fragments[2]),
            &segments,
        )
        .unwrap();
        assert_ne!(
            with_final, V3_CRC,
            "final-fragment header gives another CRC"
        );
    }

    #[test]
    //fusa:test REQ-CRC-010
    fn coverage_buffer_for_fragment_train_changes_when_any_segment_differs() {
        let header = HeaderVariant::Ntscf(avtp::NtscfHeader::default());
        let first_msg = sample_abb_message(0, b"aaaa");
        let first = AcfCoverageMessage::Abb(&first_msg);
        let segments_a: [&[u8]; 2] = [b"aaaa", b"bbbb"];
        let segments_b: [&[u8]; 2] = [b"aaaa", b"cccc"];
        let buf_a =
            build_crc32_coverage_buffer_for_fragment_train(&header, &first, &segments_a).unwrap();
        let buf_b =
            build_crc32_coverage_buffer_for_fragment_train(&header, &first, &segments_b).unwrap();
        assert_ne!(buf_a, buf_b);
    }

    #[test]
    //fusa:test REQ-CRC-010
    fn coverage_buffer_for_fragment_train_gbb_carries_first_fragment_timestamp() {
        let header = HeaderVariant::Ntscf(avtp::NtscfHeader::default());
        let mut first_msg = sample_gbb_message(0, 0x1122_3344_5566_7788, b"part-a");
        // mtv=1: encode_acf_gbb zeroes an untimed timestamp (REQ-ACF-011).
        first_msg.info.mtv = true;
        let first = AcfCoverageMessage::Gbb(&first_msg);
        let segments: [&[u8]; 2] = [b"part-a", b"part-b"];
        let buf =
            build_crc32_coverage_buffer_for_fragment_train(&header, &first, &segments).unwrap();
        assert_eq!(
            &buf[ACF_AT..ACF_AT + acf::ACF_GBB_HEADER_LEN],
            &acf::encode_acf_gbb(&first_msg).unwrap()[..acf::ACF_GBB_HEADER_LEN]
        );
        let ts_start = ACF_AT + acf::BYTE_MESSAGE_INFO_LEN;
        assert_eq!(
            &buf[ts_start..ts_start + 8],
            &0x1122_3344_5566_7788u64.to_be_bytes()
        );
        assert_eq!(&buf[buf.len() - 12..], b"part-apart-b");
    }

    #[test]
    //fusa:test REQ-CRC-010
    fn coverage_buffer_for_fragment_train_propagates_encode_error() {
        let header = HeaderVariant::Ntscf(avtp::NtscfHeader::default());
        let mut first_msg = sample_abb_message(0, b"x");
        first_msg.info.byte_bus_id = 0x800; // wider than 11 bits
        let first = AcfCoverageMessage::Abb(&first_msg);
        let mut gbb = sample_gbb_message(0, 0, b"x");
        gbb.info.byte_bus_id = 0x800;
        let segments: [&[u8]; 1] = [b"x"];
        assert_eq!(
            build_crc32_coverage_buffer_for_fragment_train(&header, &first, &segments),
            Err(RcpError::InvalidSize)
        );
        assert_eq!(
            crc32_tc18_for_fragment_train(&header, &AcfCoverageMessage::Gbb(&gbb), &segments),
            Err(RcpError::InvalidSize)
        );
    }

    #[test]
    //fusa:test REQ-CRC-010
    fn crc32_tc18_for_fragment_train_matches_coverage_buffer_under_tscf() {
        let header = HeaderVariant::Tscf(avtp::TscfHeader {
            stream_id: 0x0203_0405_0607_0809,
            avtp_timestamp: 0x1234_5678,
            tu: true,
            ..Default::default()
        });
        let first_msg = sample_abb_message(0, b"hello-");
        let first = AcfCoverageMessage::Abb(&first_msg);
        let segments: [&[u8]; 2] = [b"hello-", b"world"];
        let via_helper = crc32_tc18_for_fragment_train(&header, &first, &segments).unwrap();
        let buf =
            build_crc32_coverage_buffer_for_fragment_train(&header, &first, &segments).unwrap();
        assert_eq!(via_helper, crc32_tc18(&buf));
        assert_eq!(buf[2], 0x01, "TSCF tu covered");
        assert_eq!(&buf[11..15], &0x1234_5678u32.to_be_bytes());
    }
}
