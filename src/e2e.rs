// fusa:req REQ-E2E-001
// fusa:req REQ-E2E-002
// fusa:req REQ-E2E-003
// fusa:req REQ-E2E-004
// fusa:req REQ-E2E-005
// fusa:req REQ-E2E-006
// fusa:req REQ-E2E-007
// fusa:req REQ-E2E-008
// fusa:req REQ-CRC-001
// fusa:req REQ-CRC-002
// fusa:req REQ-CRC-003

//! End-to-end protection: CRC-16/CCITT-FALSE header + replay guard, plus
//! (as of `ROADMAP.md` Milestone 6) the standalone OPEN Alliance TC18
//! safe-point CRC-32 algorithm this module is migrating toward.
//!
//! Frame layout (current `wrap`/`unwrap`, unchanged by this milestone entry):
//! ```text
//! [0:4]  seqNum  (u32 big-endian)
//! [4:6]  CRC-16  (u16 big-endian, over seqNum bytes ++ original payload)
//! [6..]  original payload
//! ```
//!
//! [`crc32_tc18`] is added additively alongside the above: it is not yet
//! called from `wrap`/`unwrap` or from [`E2eController`]. Deciding exactly
//! which bytes of a real safe-point frame the CRC-32 covers (`stream_id`,
//! `avtp_timestamp`, the ACF header, and payload, per `ROADMAP.md`
//! Milestone 6's "Coverage rule" bullet) requires the AVTPDU/ACF framing
//! types from [`crate::avtp`]/[`crate::acf`], and is deliberately deferred
//! to that bullet rather than guessed at here.
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

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::{Command, Controller, RcpError, Response, Subscription, Zone};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Fixed E2E header length in bytes (4 byte seqNum + 2 byte CRC-16).
// fusa:req REQ-E2E-006
pub const HEADER_LEN: usize = 6;

/// Anti-replay window size.
const REPLAY_WINDOW: usize = 32;

// ── CRC-16/CCITT-FALSE ────────────────────────────────────────────────────────

fn crc16_ccitt_false(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

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
/// input and output. A genuinely different algorithm from
/// [`crc16_ccitt_false`] above — different width, different polynomial,
/// different (reflected vs. non-reflected) bit order — not a width-widened
/// variant of it.
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

// ── Wrap / Unwrap ─────────────────────────────────────────────────────────────

/// Prepend a 6-byte E2E header (seqNum + CRC-16) to `payload`.
// fusa:req REQ-E2E-001
// fusa:req REQ-E2E-002
// fusa:req REQ-E2E-006
pub fn wrap(seq_num: u32, payload: &[u8]) -> Vec<u8> {
    let mut covered = Vec::with_capacity(4 + payload.len());
    covered.extend_from_slice(&seq_num.to_be_bytes());
    covered.extend_from_slice(payload);
    let crc = crc16_ccitt_false(&covered);

    let mut frame = Vec::with_capacity(HEADER_LEN + payload.len());
    frame.extend_from_slice(&seq_num.to_be_bytes());
    frame.extend_from_slice(&crc.to_be_bytes());
    frame.extend_from_slice(payload);
    frame
}

/// Unwrap an E2E-protected frame, validating the CRC.
///
/// Returns `(seq_num, payload)` on success.
/// Returns `Err(RcpError::ShortFrame)` if frame < 6 bytes.
/// Returns `Err(RcpError::CrcMismatch)` on CRC failure.
// fusa:req REQ-E2E-003
// fusa:req REQ-E2E-007
pub fn unwrap(frame: &[u8]) -> Result<(u32, &[u8]), RcpError> {
    if frame.len() < HEADER_LEN {
        return Err(RcpError::ShortFrame);
    }
    let seq_num = u32::from_be_bytes(frame[0..4].try_into().unwrap());
    let stored_crc = u16::from_be_bytes(frame[4..6].try_into().unwrap());
    let payload = &frame[HEADER_LEN..];

    let mut covered = Vec::with_capacity(4 + payload.len());
    covered.extend_from_slice(&seq_num.to_be_bytes());
    covered.extend_from_slice(payload);
    let computed = crc16_ccitt_false(&covered);

    if computed != stored_crc {
        return Err(RcpError::CrcMismatch);
    }
    Ok((seq_num, payload))
}

// ── ReplayGuard ───────────────────────────────────────────────────────────────

/// Sliding-window anti-replay guard. Safe for concurrent use.
// fusa:req REQ-E2E-005
pub struct ReplayGuard {
    window: Mutex<Vec<u32>>,
}

impl ReplayGuard {
    pub fn new() -> Self {
        ReplayGuard {
            window: Mutex::new(Vec::with_capacity(REPLAY_WINDOW)),
        }
    }

    /// Returns `Err(RcpError::Replay)` if `seq_num` was already seen in the window.
    /// Records `seq_num` on success.
    pub fn check(&self, seq_num: u32) -> Result<(), RcpError> {
        let mut window = self.window.lock().unwrap();
        if window.len() >= REPLAY_WINDOW {
            window.remove(0);
        }
        if window.contains(&seq_num) {
            return Err(RcpError::Replay);
        }
        window.push(seq_num);
        Ok(())
    }
}

impl Default for ReplayGuard {
    fn default() -> Self {
        Self::new()
    }
}

// ── E2E Controller ────────────────────────────────────────────────────────────

/// Wraps a [`Controller`], adding an E2E header to every outgoing payload.
// fusa:req REQ-E2E-004
pub struct E2eController {
    inner: Arc<dyn Controller>,
    seq: AtomicU32,
}

impl E2eController {
    pub fn new(inner: Arc<dyn Controller>) -> Self {
        E2eController {
            inner,
            seq: AtomicU32::new(0),
        }
    }
}

impl Controller for E2eController {
    fn zone(&self) -> Zone {
        self.inner.zone()
    }

    fn send(&self, cmd: &Command, timeout: Option<Duration>) -> Result<Response, RcpError> {
        let seq_num = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        let raw_payload = cmd.payload.as_deref().unwrap_or(&[]);
        let protected = wrap(seq_num, raw_payload);
        let mut wrapped_cmd = cmd.clone();
        wrapped_cmd.payload = Some(protected);
        self.inner.send(&wrapped_cmd, timeout)
    }

    fn subscribe(&self) -> Result<Subscription, RcpError> {
        self.inner.subscribe()
    }

    fn close(&self) -> Result<(), RcpError> {
        self.inner.close()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockController;
    use crate::Zone;

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

    // ── Header length ─────────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-E2E-006
    fn header_len_is_six() {
        assert_eq!(HEADER_LEN, 6);
    }

    #[test]
    // fusa:test REQ-E2E-006
    fn wrapped_frame_length_equals_header_plus_payload() {
        let payload = b"hello";
        let frame = wrap(1, payload);
        assert_eq!(frame.len(), HEADER_LEN + payload.len());
    }

    // ── Wrap encodes seq and CRC ──────────────────────────────────────────────

    #[test]
    // fusa:test REQ-E2E-001
    fn wrap_prepends_seq_and_crc() {
        let seq = 42u32;
        let payload = b"test";
        let frame = wrap(seq, payload);
        assert_eq!(u32::from_be_bytes(frame[0..4].try_into().unwrap()), seq);
        // CRC stored in bytes 4:6 — we just verify Unwrap agrees
        let (got_seq, got_payload) = unwrap(&frame).unwrap();
        assert_eq!(got_seq, seq);
        assert_eq!(got_payload, payload);
    }

    // ── Unwrap validates CRC ──────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-E2E-003
    fn unwrap_rejects_short_frame() {
        let frame = [0u8; 4]; // less than HEADER_LEN
        assert_eq!(unwrap(&frame), Err(RcpError::ShortFrame));
    }

    #[test]
    // fusa:test REQ-E2E-003
    fn unwrap_rejects_crc_mismatch() {
        let mut frame = wrap(1, b"payload");
        frame[6] ^= 0xFF; // corrupt payload byte
        assert_eq!(unwrap(&frame), Err(RcpError::CrcMismatch));
    }

    // ── Round-trip ────────────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-E2E-007
    fn wrap_unwrap_are_inverse() {
        let seq = 0xDEAD_BEEF;
        let payload = b"roundtrip test";
        let frame = wrap(seq, payload);
        let (got_seq, got_payload) = unwrap(&frame).unwrap();
        assert_eq!(got_seq, seq);
        assert_eq!(got_payload, payload);
    }

    #[test]
    // fusa:test REQ-E2E-007
    fn wrap_unwrap_empty_payload() {
        let frame = wrap(0, &[]);
        let (seq, payload) = unwrap(&frame).unwrap();
        assert_eq!(seq, 0);
        assert!(payload.is_empty());
    }

    // ── CRC covers seqNum and payload ─────────────────────────────────────────

    #[test]
    // fusa:test REQ-E2E-002
    fn different_seq_produces_different_crc() {
        let payload = b"same";
        let f1 = wrap(1, payload);
        let f2 = wrap(2, payload);
        let crc1 = u16::from_be_bytes(f1[4..6].try_into().unwrap());
        let crc2 = u16::from_be_bytes(f2[4..6].try_into().unwrap());
        assert_ne!(crc1, crc2, "different seqNum should produce different CRC");
    }

    #[test]
    // fusa:test REQ-E2E-002
    fn different_payload_produces_different_crc() {
        let f1 = wrap(1, b"aaa");
        let f2 = wrap(1, b"bbb");
        let crc1 = u16::from_be_bytes(f1[4..6].try_into().unwrap());
        let crc2 = u16::from_be_bytes(f2[4..6].try_into().unwrap());
        assert_ne!(crc1, crc2, "different payload should produce different CRC");
    }

    // ── Single-bit corruption ─────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-E2E-008
    fn single_bit_payload_corruption_detected() {
        let payload = b"integrity test data";
        let mut frame = wrap(1, payload);
        // Flip a bit in the payload section
        frame[HEADER_LEN] ^= 0x01;
        assert_eq!(unwrap(&frame), Err(RcpError::CrcMismatch));
    }

    #[test]
    // fusa:test REQ-E2E-008
    fn single_bit_seq_corruption_detected() {
        let payload = b"data";
        let mut frame = wrap(1, payload);
        // Flip a bit in the seqNum field
        frame[0] ^= 0x01;
        assert_eq!(unwrap(&frame), Err(RcpError::CrcMismatch));
    }

    // ── ReplayGuard ───────────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-E2E-005
    fn replay_guard_accepts_new_seq_num() {
        let guard = ReplayGuard::new();
        assert!(guard.check(1).is_ok());
        assert!(guard.check(2).is_ok());
        assert!(guard.check(3).is_ok());
    }

    #[test]
    // fusa:test REQ-E2E-005
    fn replay_guard_rejects_seen_seq_num() {
        let guard = ReplayGuard::new();
        guard.check(42).unwrap();
        let err = guard.check(42).unwrap_err();
        assert_eq!(err, RcpError::Replay);
    }

    #[test]
    // fusa:test REQ-E2E-005
    fn replay_guard_window_evicts_old_entries() {
        let guard = ReplayGuard::new();
        // Fill window
        for i in 0..REPLAY_WINDOW as u32 {
            guard.check(i).unwrap();
        }
        // Seq 0 should be evicted
        assert!(
            guard.check(0).is_ok(),
            "seq 0 should be accepted after eviction"
        );
    }

    #[test]
    // fusa:test REQ-E2E-005
    fn replay_guard_concurrent_safe() {
        let guard = Arc::new(ReplayGuard::new());
        let handles: Vec<_> = (0..16)
            .map(|i| {
                let g = Arc::clone(&guard);
                std::thread::spawn(move || {
                    let _ = g.check(i as u32);
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
    }

    // ── E2E Controller ────────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-E2E-004
    fn e2e_controller_increments_seq_on_every_send() {
        let received_payloads = Arc::new(Mutex::new(vec![]));
        let rp2 = Arc::clone(&received_payloads);
        let h: crate::mock::Handler = Box::new(move |cmd| {
            let pl = cmd.payload.clone().unwrap_or_default();
            rp2.lock().unwrap().push(pl);
            crate::Response {
                command_id: cmd.id,
                zone: cmd.zone,
                status: crate::ResponseStatus::OK,
                payload: None,
            }
        });
        let inner = MockController::new(Zone::FRONT_LEFT, Some(h)) as Arc<dyn Controller>;
        let e2e = E2eController::new(inner);

        let cmd = crate::Command {
            zone: Zone::FRONT_LEFT,
            ..Default::default()
        };
        e2e.send(&cmd, None).unwrap();
        e2e.send(&cmd, None).unwrap();

        let payloads = received_payloads.lock().unwrap();
        assert_eq!(payloads.len(), 2);
        let seq1 = u32::from_be_bytes(payloads[0][0..4].try_into().unwrap());
        let seq2 = u32::from_be_bytes(payloads[1][0..4].try_into().unwrap());
        assert!(seq2 > seq1, "sequence must strictly increase");
    }
}
