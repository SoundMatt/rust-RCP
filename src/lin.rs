// fusa:req REQ-LIN-001
// fusa:req REQ-LIN-002
// fusa:req REQ-LIN-003
// fusa:req REQ-LIN-004
// fusa:req REQ-LIN-005
// fusa:req REQ-LIN-006

//! The LIN commander endpoint type (`ep_type 0x06`) — `ROADMAP.md`
//! Milestone 7 ("Remaining Endpoint Types"), opening checklist bullet: raw
//! byte pass-through only, with no PID, checksum, or schedule-table
//! intelligence performed at the protocol level, and that responsibility
//! pushed entirely onto the client.
//!
//! This is this crate's first Milestone 7 entry, following the same
//! "additive standalone plumbing only" discipline Milestone 4's six
//! endpoint-type modules ([`crate::gpio`], [`crate::spi`], [`crate::i2c`],
//! [`crate::uart`], [`crate::adc`], [`crate::pwm`]) already established. Two
//! named pieces are in scope, both implemented here:
//!
//! - [`LinFrameTransfer`] / [`LinFrameTransferResult`] — the raw PID+data
//!   bytes a LIN commander request sends onto the bus, and the raw data
//!   bytes a response returns from it, modeled as opaque byte content this
//!   module does not interpret, matching
//!   [`crate::spi::SpiByteTransfer`]/[`crate::i2c::I2cByteTransfer`]'s own
//!   "raw pass-through, no parsing" discipline. See "Provenance note: PID
//!   and checksum are entirely client-owned bytes" below for why this
//!   module computes neither.
//! - [`LinFunctionalConfig`] — this endpoint type's functional-config
//!   content. See "Relationship to `crate::regmap`" below.
//!
//! Deliberately out of scope, for the same reasons every prior Milestone 4
//! entry's own doc comment already gives:
//!
//! - Any PID computation, checksum computation, or LIN schedule-table
//!   management. `ROADMAP.md`'s LIN commander checklist bullet states the
//!   spec itself defines none of this at the protocol level, so this module
//!   builds none of it — see "Validation against `linbr.rs`" below.
//! - The "Groups A/B/C" `evt[2:0]` sub-opcode convention. Unlike
//!   [`crate::gpio`]'s write-semantics selection or [`crate::spi`]'s
//!   up-to-6 channel selection, `ROADMAP.md`'s LIN commander checklist
//!   bullet names no `sub_opcode`-keyed selection mechanism, so this module
//!   reads `sub_opcode` nowhere.
//! - [`crate::regmap::CommonFunctionalConfig`]'s fields — unchanged here, as
//!   in every prior Milestone 1-4 entry.
//! - Wiring any of the below into an actual decoder, dispatch loop, or
//!   [`crate::avtp`]/[`crate::acf`]/[`crate::addressing`] caller, and —
//!   distinct from every Milestone 4 entry — touching [`crate::linbr`]
//!   itself. `linbr.rs`'s own REPLACE-disposition cutover (deletion/rebuild
//!   against the new core) is `ROADMAP.md` Milestone 9's job, not this
//!   item's; this module is examined for validation purposes only, per
//!   "Validation against `linbr.rs`" below.
//!
//! ## Validation against `linbr.rs`
//!
//! Per this milestone's checklist bullet's own instruction, this module was
//! written only after reading [`crate::linbr::LinBridge::send`]'s existing
//! PID derivation
//! (`(self.zone.0 << 2) | (cmd.cmd_type.0 as u8 & 0x03)`) and confirming it
//! is not reusable logic: that formula derives a PID from the old
//! `Zone`/`Command` model's zone index and command-type discriminant, both
//! of which have no equivalent in the endpoint-addressed model this crate
//! is replacing them with, and — independent of that mismatch — deriving a
//! PID from anything other than a value the client itself supplies
//! contradicts the raw-byte-pass-through behavior this checklist bullet
//! describes: a real LIN commander must be free to place whatever PID
//! (parity bits included) and whatever data the client computed onto the
//! bus, not have the RC Server re-derive one from its own state. Nor is
//! `linbr::LinBridge::send`'s response-status inference (treating the
//! response's first returned byte as an implicit OK/ERROR flag) reused
//! here — that is itself a piece of protocol-level interpretation this
//! checklist bullet's pass-through behavior does not call for.
//! [`crate::linbr::LIN_MAX_DATA`] is reused, but only as the plain physical
//! fact it states (LIN 2.x's real per-frame data ceiling), imported below
//! as [`LIN_MAX_DATA`] rather than duplicated as a second literal `8`.
//!
//! ## Relationship to [`crate::regmap`]
//!
//! As with every Milestone 4 endpoint-type module, LIN's real
//! functional-config content gets its own dedicated type,
//! [`LinFunctionalConfig`], rather than adding LIN-specific fields directly
//! onto the still-shared, thirteen-endpoint-type
//! [`crate::regmap::PerEpTypeFunctionalConfig`] placeholder.
//! [`LinFunctionalConfig::layer_tag`] shows how a caller obtains the
//! matching generic-layer tag so the two compose through
//! [`crate::regmap::check_functional_config_matches_ep_type`] exactly as
//! that cross-layer rule already expects, without this module editing
//! [`crate::regmap`] itself.
//!
//! `ROADMAP.md`'s LIN commander checklist bullet names no functional-config
//! content beyond raw byte pass-through — no channel selection, no speed
//! preset, nothing comparable to [`crate::spi::SpiFunctionalConfig`]'s
//! channel slots or [`crate::i2c::I2cFunctionalConfig`]'s speed mode. Per
//! Guiding Principle 5, [`LinFunctionalConfig`] is therefore left an
//! intentionally empty placeholder, the same discipline
//! [`crate::spi::SpiChannelConfigSlot`] already applies to its own
//! still-unnamed per-channel fields, rather than this crate guessing at
//! plausible LIN configuration content on its own.
//!
//! ## Provenance note: PID and checksum are entirely client-owned bytes
//!
//! `ROADMAP.md`'s LIN commander checklist bullet states the spec defines no
//! PID/checksum/schedule-table intelligence at the protocol level and
//! expects that responsibility pushed to the client, without itself stating
//! the exact byte layout an RCP request/response carries. Per Guiding
//! Principle 5, this module's working interpretation is: a request carries
//! the client-computed PID as one leading byte followed by the frame's data
//! bytes ([`LinFrameTransfer::pid`] / [`LinFrameTransfer::data`]), and a
//! response carries only the data bytes actually read back off the bus
//! ([`LinFrameTransferResult::data`]) — no PID field, since a LIN response
//! frame is identified by the request's own PID, not a second one. Neither
//! field is validated for parity, and `data` is not scanned for a trailing
//! checksum byte the client may or may not have included in it — this
//! module carries both PID and data exactly as supplied, unparsed, matching
//! [`crate::spi::SpiByteTransfer`]'s own refusal to interpret its PICO
//! bytes. The one constraint this module does enforce —
//! [`LIN_MAX_DATA`]-byte data ceiling, returning
//! `Err(RcpError::PayloadTooLarge)` above it — is not new protocol-level
//! smarts; it is the same real LIN 2.x physical ceiling
//! [`crate::linbr::LinBridge::send`] already enforced, reused here as a
//! fact about the bus rather than re-derived.

use crate::RcpError;

// ── LIN_MAX_DATA ──────────────────────────────────────────────────────────────

/// Maximum LIN frame data length: LIN 2.x's real per-frame data ceiling,
/// reused from [`crate::linbr::LIN_MAX_DATA`] as a physical fact about the
/// bus — see this module's doc comment "Validation against `linbr.rs`" for
/// why this is the only piece of `linbr.rs` this module reuses.
// fusa:req REQ-LIN-001
pub const LIN_MAX_DATA: usize = crate::linbr::LIN_MAX_DATA;

// ── LinFrameTransfer ─────────────────────────────────────────────────────────

/// A raw LIN commander frame transfer: the client-computed PID byte plus
/// the frame's data bytes, sent from RC Server onto the LIN bus.
///
/// See this module's doc comment "Provenance note: PID and checksum are
/// entirely client-owned bytes" for this type's wire-layout interpretation
/// and for why neither field is parsed or validated beyond the
/// [`LIN_MAX_DATA`] data-length ceiling.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
// fusa:req REQ-LIN-002
pub struct LinFrameTransfer {
    /// The client-computed PID byte, carried unparsed and unvalidated —
    /// this module performs no parity check or PID derivation of its own.
    pub pid: u8,
    /// The frame's data bytes, carried unparsed — may or may not include a
    /// trailing checksum byte the client computed; this module does not
    /// distinguish either way. Never longer than [`LIN_MAX_DATA`] bytes.
    pub data: Vec<u8>,
}

impl LinFrameTransfer {
    /// Encode this transfer to its raw wire representation: the PID byte
    /// followed by `data`, unmodified and unframed.
    // fusa:req REQ-LIN-002
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(1 + self.data.len());
        buf.push(self.pid);
        buf.extend_from_slice(&self.data);
        buf
    }

    /// Decode a [`LinFrameTransfer`] from a byte slice: the first byte is
    /// `pid`, the remainder is `data`.
    ///
    /// Returns `Err(RcpError::ShortFrame)` for an empty slice (no PID byte
    /// present), matching [`crate::spi::SpiStatus::decode`]'s own
    /// too-short-input handling. Returns `Err(RcpError::PayloadTooLarge)`
    /// when the remaining data would exceed [`LIN_MAX_DATA`] bytes, the same
    /// error variant [`crate::linbr::LinBridge::send`] already used for the
    /// same physical ceiling. Never panics for any input.
    // fusa:req REQ-LIN-003
    // fusa:req REQ-LIN-004
    pub fn decode(b: &[u8]) -> Result<Self, RcpError> {
        let (pid, data) = b.split_first().ok_or(RcpError::ShortFrame)?;
        if data.len() > LIN_MAX_DATA {
            return Err(RcpError::PayloadTooLarge);
        }
        Ok(Self {
            pid: *pid,
            data: data.to_vec(),
        })
    }
}

/// A raw LIN commander frame transfer result: the data bytes a response
/// returns from the LIN bus back to the RC Server.
///
/// See [`LinFrameTransfer`]'s doc comment — this carries only `data`, since
/// a LIN response frame is identified by the request's own PID rather than
/// carrying a second one of its own.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
// fusa:req REQ-LIN-005
pub struct LinFrameTransferResult {
    /// The data bytes read back off the bus, carried unparsed. Never longer
    /// than [`LIN_MAX_DATA`] bytes.
    pub data: Vec<u8>,
}

impl LinFrameTransferResult {
    /// Encode this transfer result to its raw wire representation: `data`,
    /// unmodified and unframed.
    // fusa:req REQ-LIN-005
    pub fn encode(&self) -> Vec<u8> {
        self.data.clone()
    }

    /// Decode a [`LinFrameTransferResult`] from a byte slice.
    ///
    /// Returns `Err(RcpError::PayloadTooLarge)` for input longer than
    /// [`LIN_MAX_DATA`] bytes; an empty slice is a valid (zero-length) LIN
    /// response and decodes successfully. Never panics for any input.
    // fusa:req REQ-LIN-005
    pub fn decode(b: &[u8]) -> Result<Self, RcpError> {
        if b.len() > LIN_MAX_DATA {
            return Err(RcpError::PayloadTooLarge);
        }
        Ok(Self { data: b.to_vec() })
    }
}

// ── LinFunctionalConfig ──────────────────────────────────────────────────────

/// LIN commander's own per-EP-type functional-config content.
///
/// An intentionally empty placeholder — see this module's doc comment
/// "Relationship to `crate::regmap`" for why the checklist text names no
/// LIN-specific configuration content to model here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
// fusa:req REQ-LIN-006
pub struct LinFunctionalConfig;

impl LinFunctionalConfig {
    /// The [`crate::regmap::PerEpTypeFunctionalConfig`] generic-layer tag
    /// that matches this LIN functional config, for use with
    /// [`crate::regmap::check_functional_config_matches_ep_type`].
    ///
    /// This module does not itself call that function — it only shows how a
    /// caller would obtain the matching tag, per this module's doc comment
    /// "Relationship to `crate::regmap`".
    // fusa:req REQ-LIN-006
    pub fn layer_tag(&self) -> crate::regmap::PerEpTypeFunctionalConfig {
        crate::regmap::PerEpTypeFunctionalConfig::new(crate::regmap::EndpointType::Lin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── LIN_MAX_DATA ─────────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-LIN-001
    fn lin_max_data_is_eight_and_matches_linbr_fact() {
        assert_eq!(LIN_MAX_DATA, 8);
        assert_eq!(LIN_MAX_DATA, crate::linbr::LIN_MAX_DATA);
    }

    // ── LinFrameTransfer: round-trip / never-panic ──────────────────────────

    #[test]
    // fusa:test REQ-LIN-002
    fn lin_frame_transfer_round_trips_through_encode_decode() {
        for (pid, data) in [
            (0x00u8, vec![]),
            (0x53, vec![0x00]),
            (0xFF, vec![0xAA; 3]),
            (0x12, (0u8..8).collect::<Vec<_>>()),
        ] {
            let transfer = LinFrameTransfer {
                pid,
                data: data.clone(),
            };
            let decoded = LinFrameTransfer::decode(&transfer.encode()).unwrap();
            assert_eq!(decoded.pid, pid);
            assert_eq!(decoded.data, data);
        }
    }

    #[test]
    // fusa:test REQ-LIN-003
    fn lin_frame_transfer_decode_rejects_empty_input() {
        assert_eq!(LinFrameTransfer::decode(&[]), Err(RcpError::ShortFrame));
    }

    #[test]
    // fusa:test REQ-LIN-004
    fn lin_frame_transfer_decode_rejects_data_longer_than_lin_max_data() {
        let mut buf = vec![0x00u8]; // pid
        buf.extend(vec![0xAAu8; LIN_MAX_DATA + 1]);
        assert_eq!(
            LinFrameTransfer::decode(&buf),
            Err(RcpError::PayloadTooLarge)
        );
    }

    #[test]
    // fusa:test REQ-LIN-004
    fn lin_frame_transfer_decode_accepts_data_at_exactly_lin_max_data() {
        let mut buf = vec![0x00u8]; // pid
        buf.extend(vec![0xAAu8; LIN_MAX_DATA]);
        let decoded = LinFrameTransfer::decode(&buf).unwrap();
        assert_eq!(decoded.data.len(), LIN_MAX_DATA);
    }

    #[test]
    // fusa:test REQ-LIN-002
    fn lin_frame_transfer_decode_never_panics_for_any_sampled_input() {
        for len in [0usize, 1, 2, 9, 64] {
            let buf = vec![0x5Au8; len];
            let _ = LinFrameTransfer::decode(&buf);
        }
    }

    // ── LinFrameTransferResult: round-trip / never-panic ────────────────────

    #[test]
    // fusa:test REQ-LIN-005
    fn lin_frame_transfer_result_round_trips_through_encode_decode() {
        for data in [vec![], vec![0xFF], vec![0x01, 0x02, 0x03]] {
            let result = LinFrameTransferResult { data: data.clone() };
            assert_eq!(
                LinFrameTransferResult::decode(&result.encode())
                    .unwrap()
                    .data,
                data
            );
        }
    }

    #[test]
    // fusa:test REQ-LIN-005
    fn lin_frame_transfer_result_decode_rejects_longer_than_lin_max_data() {
        let buf = vec![0xAAu8; LIN_MAX_DATA + 1];
        assert_eq!(
            LinFrameTransferResult::decode(&buf),
            Err(RcpError::PayloadTooLarge)
        );
    }

    #[test]
    // fusa:test REQ-LIN-005
    fn lin_frame_transfer_result_decode_accepts_exactly_lin_max_data() {
        let buf = vec![0xAAu8; LIN_MAX_DATA];
        assert_eq!(
            LinFrameTransferResult::decode(&buf).unwrap().data.len(),
            LIN_MAX_DATA
        );
    }

    #[test]
    // fusa:test REQ-LIN-005
    fn lin_frame_transfer_result_decode_never_panics_for_any_sampled_input() {
        for len in [0usize, 1, 5, 32] {
            let buf = vec![0xA5u8; len];
            let _ = LinFrameTransferResult::decode(&buf);
        }
    }

    // ── LinFunctionalConfig / layer_tag ─────────────────────────────────────

    #[test]
    // fusa:test REQ-LIN-006
    fn lin_functional_config_layer_tag_matches_ep_type_lin() {
        let functional = LinFunctionalConfig;
        let generic = crate::regmap::PerEpConfigBlock::new(crate::regmap::EndpointType::Lin);
        let tag = functional.layer_tag();
        assert_eq!(tag.ep_type, crate::regmap::EndpointType::Lin);
        assert!(crate::regmap::functional_config_matches_ep_type(
            &generic, &tag
        ));
        assert_eq!(
            crate::regmap::check_functional_config_matches_ep_type(&generic, &tag),
            Ok(())
        );
    }

    #[test]
    // fusa:test REQ-LIN-006
    fn lin_functional_config_layer_tag_rejects_mismatched_ep_type() {
        let functional = LinFunctionalConfig;
        let generic = crate::regmap::PerEpConfigBlock::new(crate::regmap::EndpointType::Spi);
        let tag = functional.layer_tag();
        assert!(!crate::regmap::functional_config_matches_ep_type(
            &generic, &tag
        ));
    }
}
