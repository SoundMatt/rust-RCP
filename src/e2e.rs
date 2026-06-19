// fusa:req REQ-E2E-001
// fusa:req REQ-E2E-002
// fusa:req REQ-E2E-003
// fusa:req REQ-E2E-004
// fusa:req REQ-E2E-005
// fusa:req REQ-E2E-006
// fusa:req REQ-E2E-007
// fusa:req REQ-E2E-008

//! End-to-end protection: CRC-16/CCITT-FALSE header + replay guard.
//!
//! Frame layout:
//! ```text
//! [0:4]  seqNum  (u32 big-endian)
//! [4:6]  CRC-16  (u16 big-endian, over seqNum bytes ++ original payload)
//! [6..]  original payload
//! ```

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
        if window.contains(&seq_num) {
            return Err(RcpError::Replay);
        }
        if window.len() >= REPLAY_WINDOW {
            window.remove(0);
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
