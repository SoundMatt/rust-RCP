//fusa:req REQ-CAN-001
//fusa:req REQ-CAN-002
//fusa:req REQ-CAN-003
//fusa:req REQ-CAN-004
//fusa:req REQ-CAN-005
//fusa:req REQ-CAN-006
//fusa:req REQ-CAN-007
//fusa:req REQ-CAN-008
//fusa:req REQ-CAN-009
//fusa:req REQ-CAN-010
//fusa:req REQ-CAN-011

//! The CAN controller endpoint type (`ep_type 0x0B`) — `ROADMAP.md`
//! Milestone 7 ("Remaining Endpoint Types"), second checklist bullet:
//! Classical/FD/XL `FrameFormat` selection (CBFF/CEFF/FBFF/FEFF/
//! XL-classical/XL-new); CAN XL's 6-byte sub-header plus up to 2048-byte
//! payload (flagged as needing multi-AVTPDU fragmentation, deferred to
//! `ROADMAP.md` Milestone 8); data frames only, with no remote-frame
//! support; and a note that the spec's own CAN trigger-signal table is
//! unpopulated in this revision.
//!
//! This follows directly on [`crate::lin`] (this milestone's first entry):
//! same milestone, same "additive standalone plumbing only" discipline, same
//! doc-comment provenance-note style for anything this crate has not yet
//! reconciled against confirmed wire behavior. Four named pieces are in
//! scope, all implemented here:
//!
//! - [`FrameFormat`] — the six named Classical/FD/XL frame-format variants
//!   this checklist bullet lists by name. See "Provenance note: `FrameFormat`
//!   wire encoding" below for why its byte values are this crate's own
//!   working interpretation.
//! - [`CanXlSubHeader`] / [`CanXlFrame`] — CAN XL's 6-byte sub-header plus
//!   its up-to-2048-byte payload. See "Provenance note: the CAN XL
//!   sub-header is carried opaque" and "CAN XL fragmentation interaction"
//!   below.
//! - [`CanDataFrame`] — the Classical/FD counterpart to [`CanXlFrame`]: a
//!   frame-format tag, an arbitration ID, and data bytes, with no
//!   remote-frame representation anywhere in its shape. See "Data frames
//!   only — no remote-frame representation" below.
//! - [`CanFunctionalConfig`] — this endpoint type's functional-config
//!   content, carrying the selected [`FrameFormat`]. See "Relationship to
//!   `crate::regmap`" below.
//!
//! Deliberately out of scope, for the same reasons every prior Milestone 4/7
//! entry's own doc comment already gives:
//!
//! - Any remote-frame (RTR) request/response modeling. `ROADMAP.md`'s CAN
//!   controller checklist bullet states "data frames only, no remote-frame
//!   support", so unlike some CAN controller hardware, no type in this
//!   module has an RTR field, an RTR enum variant, or any other way to
//!   represent a remote frame.
//! - The CAN trigger-signal table. `ROADMAP.md`'s checklist bullet states
//!   this table is unpopulated in the current spec revision and directs
//!   that this be tracked as a spec gap rather than guessed at — see
//!   "Provenance note: the unpopulated CAN trigger-signal table" below. No
//!   `CanTriggerSignal` type or anything resembling one exists in this
//!   module.
//! - Real multi-AVTPDU reassembly of a CAN XL payload that spans more than
//!   one AVTPDU. `ROADMAP.md`'s checklist bullet itself defers this to
//!   Milestone 8 — see "CAN XL fragmentation interaction" below.
//! - [`crate::regmap::CommonFunctionalConfig`]'s fields — unchanged here, as
//!   in every prior Milestone 1-4/7 entry.
//! - Wiring any of the below into an actual decoder, dispatch loop, or
//!   [`crate::avtp`]/[`crate::acf`]/[`crate::addressing`] caller — matching
//!   the discipline every prior Milestone 1-4/7 entry already established.
//!
//! ## Validation against `canbr.rs` (historical — see below for its outcome)
//!
//! Per `ROADMAP.md`'s Satellite Package Disposition table, the legacy
//! `canbr.rs` bridge became this endpoint type; at the time this module was
//! first written (Milestone 7, `v0.10.0-dev`), that REPLACE-disposition
//! cutover was still Milestone 9's job, so `canbr.rs` was examined then for
//! validation purposes only, not touched. Its `CanBridge` struct's
//! `can_id = zone_id << 8 | cmd_type` framing and its `CanSocket`
//! abstraction were read and explicitly not reused: both derived frame
//! identity from the old `Zone`/`Command` model, which has no equivalent in
//! the endpoint-addressed model this crate replaces it with. Its
//! `CAN_FD_MAX_PAYLOAD` constant *was* reused at that time — the same way
//! [`crate::lin::LIN_MAX_DATA`] originally reused the legacy
//! `linbr::LIN_MAX_DATA` (since deleted by Milestone 9's own `linbr`
//! REPLACE cutover, the same way this module's own canbr cutover deleted
//! `canbr.rs` below) — since it stated a genuine CAN FD physical ceiling
//! (64 data bytes per frame) rather than any `Zone`/`Command`-coupled
//! behavior.
//!
//! Milestone 9's own canbr REPLACE cutover has since deleted `canbr.rs`
//! outright (its `CanBridge`/`CanSocket`/`Zone`-keyed framing had no
//! surviving analog in this endpoint-addressed model, matching the `wire`/
//! `e2e` REPLACE cutovers immediately before it), leaving [`CAN_FD_MAX_PAYLOAD`]
//! below as this crate's one live external caller of the deleted module. Per
//! Guiding Principle 5, that cross-module dependency is resolved by inlining
//! the physical-fact literal directly here rather than leaving a stub
//! module behind purely to hold one constant — [`CAN_FD_MAX_PAYLOAD`] is now
//! stated fresh, the same way [`CLASSICAL_CAN_MAX_DATA`] already was (a
//! comparable physical fact about classical CAN 2.0's own 8-byte data
//! ceiling that `canbr.rs` never separately named, since it only ever
//! carried CAN FD payloads).
//!
//! ## Relationship to [`crate::regmap`]
//!
//! As with every Milestone 4/7 endpoint-type module, CAN's real
//! functional-config content gets its own dedicated type,
//! [`CanFunctionalConfig`], rather than adding CAN-specific fields directly
//! onto the still-shared, thirteen-endpoint-type
//! [`crate::regmap::PerEpTypeFunctionalConfig`] placeholder.
//! [`CanFunctionalConfig::layer_tag`] shows how a caller obtains the
//! matching generic-layer tag so the two compose through
//! [`crate::regmap::check_functional_config_matches_ep_type`] exactly as
//! that cross-layer rule already expects, without this module editing
//! [`crate::regmap`] itself. Unlike [`crate::lin::LinFunctionalConfig`]
//! (left an intentionally empty placeholder, since its checklist bullet
//! names no configuration content), this checklist bullet explicitly names
//! "`FrameFormat` selection" as configurable content, so
//! [`CanFunctionalConfig`] carries one [`FrameFormat`] field rather than
//! being left empty.
//!
//! ## `FrameFormat` wire encoding (confirmed against TC18 Table 54)
//!
//! `ROADMAP.md`'s checklist bullet names the six [`FrameFormat`] variants by
//! their standard CAN abbreviations but states no numeric byte value for any
//! of them, so [`FrameFormat::to_u8`]/[`FrameFormat::from_u8`] originally
//! assigned each variant a stable, sequential byte value (`0`..=`5`,
//! declaration order) as this crate's own working choice. TC18 §13.7.11.3
//! Table 54 (TC18.txt line 5447) has since been reconciled against that
//! mapping and confirms it exactly: `CBFF = 0`, `CEFF = 1`, `FBFF = 2`,
//! `FEFF = 3`, `XL (classic physical layer) = 4`, `XL (new physical
//! layer) = 5`, with `6` and `7` both Reserved. The two XL rows also settle
//! what `ROADMAP.md`'s "XL-classical"/"XL-new" naming meant: which physical
//! layer the XL frame uses, not two different XL frame shapes.
//! [`FrameFormat::from_u8`]'s rejection of `6`/`7` is therefore Table 54's
//! own Reserved rows, not merely an out-of-enum range check.
//!
//! [`FrameFormat::is_extended_id`] additionally distinguishes the base
//! (11-bit arbitration ID) vs. extended (29-bit arbitration ID) rows among
//! the four non-XL variants — [`FrameFormat::Cbff`]/[`FrameFormat::Fbff`]
//! vs. [`FrameFormat::Ceff`]/[`FrameFormat::Feff`] — which
//! [`CanDataFrame::decode`] uses to enforce the matching real CAN
//! arbitration-ID width ([`CAN_STANDARD_ID_MAX`]/[`CAN_EXTENDED_ID_MAX`]).
//! That base-vs-extended split is what the "B"/"E" letters in CBFF/CEFF/
//! FBFF/FEFF themselves already name, so this one piece is treated as a
//! genuine CAN physical fact rather than a further guessed encoding.
//!
//! ## TC18 reconciliation note (§13.7.11)
//!
//! Reconciling this module against TC18 §13.7.11 confirms three further
//! behaviors and records four gaps.
//!
//! Confirmed: TC18 §13.7.11.3 (TC18.txt line 5471) states "Sending remote
//! frames is not supported", matching this module's own "Data frames only"
//! section below; the same line states "In case the CAN ID is 11bits, then
//! it shall be right aligned in the CAN ID field", which
//! [`CanDataFrame::encode`]'s big-endian [`CanDataFrame::id`] field
//! satisfies; and line 5443 plus line 5472 together give CAN XL's total
//! "CAN data" field size as 2054 bytes — up to 2048 payload bytes plus the
//! 6 additional ISO 11898-1 bytes (RRS, SDT, VCID, AF) — matching
//! [`CAN_XL_MAX_PAYLOAD`] + [`CAN_XL_SUB_HEADER_LEN`].
//!
//! Not implemented, and recorded as explicit not-implemented requirement
//! entries rather than silently omitted:
//!
//! - The exact request-payload bit layout of TC18 Figure 39 (line 5428),
//!   which carries `FrameFormat` and the CAN ID together in the payload's
//!   first 32-bit word. This module instead emits a full format-tag byte
//!   followed by a 4-byte big-endian CAN ID (5 bytes), so a
//!   [`CanDataFrame::encode`] buffer is **not** byte-compatible with Figure
//!   39's own word layout; the figure's column rendering does not survive
//!   text extraction, so the exact bit positions are not transcribed here.
//! - Naming the 6 XL bytes as RRS/SDT/VCID/AF — [`CanXlSubHeader`] keeps
//!   them opaque.
//! - Segmentation of an over-long CAN XL payload via the `ms` and
//!   `segment_num` fields (line 5444); [`CanXlCombinedPayload::assemble`]
//!   takes caller-ordered segments and reads neither field.
//! - TC18 Table 53's functional-config register layout (§13.7.11.2, lines
//!   5363-5419: bit-time registers 1-3, TDCC, EP/FIFO status, acceptance
//!   filters 1-4, receive filters 1-4) and the six configuration
//!   capabilities §13.7.11.2 enumerates (lines 5351-5356) — see
//!   [`CanFunctionalConfig`], which carries a [`FrameFormat`] and nothing
//!   else.
//!
//! ## Provenance note: the CAN XL sub-header is carried opaque
//!
//! `ROADMAP.md`'s checklist bullet states CAN XL's sub-header is 6 bytes but
//! does not state that 6 bytes' internal field layout. Per Guiding
//! Principle 5, [`CanXlSubHeader`] does not attempt to split those 6 bytes
//! into named sub-fields — it carries them as an opaque `[u8; 6]` this
//! module does not interpret, matching [`crate::i2c`]'s own "address bytes
//! are carried inline, unparsed" precedent for content whose internal
//! framing this crate's spec-extraction pass does not record. A future item
//! that does recover the sub-header's real field layout (against this
//! crate's own spec-extraction pass, never against restated spec prose) can
//! add that decomposition later without this module having guessed at it.
//!
//! ## CAN XL fragmentation interaction (`ROADMAP.md` Milestone 8 forward
//! dependency)
//!
//! [`CanXlFrame`]'s payload is capped at [`CAN_XL_MAX_PAYLOAD`] (2048
//! bytes) — the ceiling this checklist bullet itself states — but at the
//! time this module was written, this crate had no live multi-AVTPDU
//! reassembly buffer to reconstruct a payload that arrives split across
//! more than one AVTPDU (`ROADMAP.md` Milestone 8's own then-undecided
//! go/no-go item). Matching [`crate::e2e::CombinedFragmentPayload`]'s own
//! precedent for the analogous forward dependency in Milestone 6's
//! "Fragmentation interaction" bullet, [`CanXlCombinedPayload::assemble`]
//! takes a fragment train's per-segment payloads as a caller-supplied,
//! already-ordered `&[&[u8]]` rather than reading any segment-ordering
//! field itself — this module models CAN XL's own sub-header/payload
//! framing only, and leaves real reassembly of a payload spanning more than
//! one AVTPDU out of scope, matching every prior Milestone 4/7 entry's own
//! "additive standalone plumbing only" discipline. `ROADMAP.md` Milestone 8
//! has since decided "go" and landed
//! [`crate::fragment::FragmentReassemblyBuffer`], a real reassembly buffer
//! a caller can drive with the same per-fragment `ByteMessageInfo`/payload
//! pairs a real CAN XL fragment train would carry; this module's own types
//! are unchanged by that — [`CanXlCombinedPayload::assemble`] keeps taking
//! its caller-supplied `&[&[u8]]`, and nothing here composes
//! `crate::fragment` directly, since wiring a decoded [`CanXlFrame`]'s
//! payload into a live per-stream `FragmentReassemblyBuffer` is dispatch
//! plumbing this module's own "additive standalone plumbing only" scope
//! still excludes.
//!
//! ## Provenance note: the unpopulated CAN trigger-signal table
//!
//! `ROADMAP.md`'s checklist bullet itself states the spec's own CAN
//! trigger-signal table is unpopulated in the current TC18 spec revision
//! (OPEN Alliance TC18 Remote Control Protocol Specification v0.5.1_RC),
//! and directs that this be recorded as a spec gap to track rather than
//! silently implemented around. Per Guiding Principle 5, this module
//! therefore builds no `CanTriggerSignal` type, no trigger-signal
//! enumeration, and no functional-config field referencing one — the same
//! "flag rather than guess" treatment this crate already gives
//! [`crate::regmap::EndpointType::Dac`]'s reserved status, MDIO's
//! scope-list omission (`ROADMAP.md` Milestone 7's MDIO bullet), the
//! `read_size`/`segment_num` field ambiguity
//! ([`crate::acf::ReadSizeOrSegment`]), and [`crate::i2c::I2cSpeedMode`]'s
//! own ambiguous high-speed rows. A later item that recovers this table's
//! real content from a future spec revision or OPEN Alliance clarification
//! is expected to add the corresponding trigger-signal type then, not now.
//!
//! ## Data frames only — no remote-frame representation
//!
//! `ROADMAP.md`'s checklist bullet states this endpoint type carries data
//! frames only, with no remote-frame (RTR) support. Both [`CanDataFrame`]
//! and [`CanXlFrame`] therefore have no field or variant that could
//! represent a remote frame — no `rtr: bool`, no `FrameKind::Remote`
//! variant, nothing comparable. A future item that needs to add
//! remote-frame support (should a later spec revision call for it) would
//! need to extend this module's shape rather than merely set an existing
//! flag, by design.

use crate::RcpError;

// ── Physical-fact constants ─────────────────────────────────────────────────

/// Maximum classical CAN 2.0 data length in bytes — a genuine physical
/// ceiling of the classical CAN bus, not a spec-defined or otherwise
/// interpreted value. See this module's doc comment "Validation against
/// `canbr.rs`" for why this is stated fresh here rather than imported from
/// anywhere else.
//fusa:req REQ-CAN-003
pub const CLASSICAL_CAN_MAX_DATA: usize = 8;

/// Maximum CAN FD payload in bytes — a genuine physical ceiling of the CAN
/// FD bus, not a spec-defined or otherwise interpreted value. Originally
/// reused from the legacy `canbr::CAN_FD_MAX_PAYLOAD` (see this module's doc
/// comment "Validation against `canbr.rs`"); stated directly as this
/// module's own constant since Milestone 9's canbr REPLACE cutover deleted
/// that module.
//fusa:req REQ-CAN-003
pub const CAN_FD_MAX_PAYLOAD: usize = 64;

/// Maximum CAN XL payload in bytes, per `ROADMAP.md`'s own stated ceiling
/// for this checklist bullet and confirmed by TC18 §13.7.11.3 (TC18.txt
/// line 5443): "For CAN XL this can be up to 2054 bytes (2048 + 6)".
//fusa:req REQ-CAN-006
//fusa:req REQ-CAN-016
pub const CAN_XL_MAX_PAYLOAD: usize = 2048;

/// CAN XL's sub-header length in bytes, per `ROADMAP.md`'s own stated
/// ceiling for this checklist bullet and confirmed by TC18 §13.7.11.3
/// (TC18.txt line 5472): the "CAN data" field includes 6 additional bytes
/// (RRS, SDT, VCID, AF — see ISO 11898-1) for either XL frame format. See
/// [`CanXlSubHeader`].
//fusa:req REQ-CAN-006
//fusa:req REQ-CAN-016
pub const CAN_XL_SUB_HEADER_LEN: usize = 6;

/// Maximum standard (base-format, 11-bit) CAN arbitration ID — a genuine
/// physical fact about classical/FD CAN's base frame formats, not a
/// spec-defined or otherwise interpreted value.
//fusa:req REQ-CAN-004
pub const CAN_STANDARD_ID_MAX: u32 = 0x7FF;

/// Maximum extended (extended-format, 29-bit) CAN arbitration ID — a
/// genuine physical fact about classical/FD CAN's extended frame formats,
/// not a spec-defined or otherwise interpreted value.
//fusa:req REQ-CAN-004
pub const CAN_EXTENDED_ID_MAX: u32 = 0x1FFF_FFFF;

// ── FrameFormat ──────────────────────────────────────────────────────────────

/// The six Classical/FD/XL CAN frame formats `ROADMAP.md`'s CAN controller
/// checklist bullet names by name.
///
/// See this module's doc comment "Provenance note: `FrameFormat` wire
/// encoding" for why [`FrameFormat::to_u8`]'s byte values are this crate's
/// own working interpretation rather than a confirmed spec encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
//fusa:req REQ-CAN-001
//fusa:req REQ-CAN-012
pub enum FrameFormat {
    /// Classic Base Frame Format: classical CAN 2.0, 11-bit (standard)
    /// arbitration ID.
    Cbff = 0,
    /// Classic Extended Frame Format: classical CAN 2.0, 29-bit (extended)
    /// arbitration ID.
    Ceff = 1,
    /// FD Base Frame Format: CAN FD, 11-bit (standard) arbitration ID.
    Fbff = 2,
    /// FD Extended Frame Format: CAN FD, 29-bit (extended) arbitration ID.
    Feff = 3,
    /// CAN XL, "classical" variant, per `ROADMAP.md`'s own naming. See this
    /// module's doc comment — this crate takes no further position on what
    /// distinguishes this from [`FrameFormat::XlNew`] beyond the name
    /// `ROADMAP.md` itself gives it.
    XlClassical = 4,
    /// CAN XL, "new" variant, per `ROADMAP.md`'s own naming. See
    /// [`FrameFormat::XlClassical`]'s doc comment.
    XlNew = 5,
}

impl FrameFormat {
    /// Encode this frame format as its wire byte value, per TC18 §13.7.11.3
    /// Table 54 (TC18.txt line 5447). See this module's doc comment
    /// "`FrameFormat` wire encoding (confirmed against TC18 Table 54)".
    //fusa:req REQ-CAN-001
    //fusa:req REQ-CAN-012
    pub fn to_u8(self) -> u8 {
        self as u8
    }

    /// Decode a wire byte value into a [`FrameFormat`].
    ///
    /// Returns `Err(RcpError::InvalidParameter)` for any byte outside
    /// `0..=5` — `6` and `7` are Table 54's own two Reserved rows (TC18
    /// §13.7.11.3, TC18.txt line 5454), and no `FrameFormat` value wider
    /// than 3 bits exists. Never panics for any input.
    //fusa:req REQ-CAN-002
    //fusa:req REQ-CAN-013
    pub fn from_u8(raw: u8) -> Result<Self, RcpError> {
        match raw {
            0 => Ok(Self::Cbff),
            1 => Ok(Self::Ceff),
            2 => Ok(Self::Fbff),
            3 => Ok(Self::Feff),
            4 => Ok(Self::XlClassical),
            5 => Ok(Self::XlNew),
            _ => Err(RcpError::InvalidParameter),
        }
    }

    /// Is this one of the two classical (non-FD, non-XL) formats?
    //fusa:req REQ-CAN-001
    pub fn is_classical(self) -> bool {
        matches!(self, Self::Cbff | Self::Ceff)
    }

    /// Is this one of the two CAN FD formats?
    //fusa:req REQ-CAN-001
    pub fn is_fd(self) -> bool {
        matches!(self, Self::Fbff | Self::Feff)
    }

    /// Is this one of the two CAN XL formats?
    //fusa:req REQ-CAN-001
    pub fn is_xl(self) -> bool {
        matches!(self, Self::XlClassical | Self::XlNew)
    }

    /// Does this format use a 29-bit extended arbitration ID (as opposed to
    /// an 11-bit standard one)? Meaningful only for the four non-XL
    /// variants — see this module's doc comment "Provenance note:
    /// `FrameFormat` wire encoding".
    //fusa:req REQ-CAN-004
    pub fn is_extended_id(self) -> bool {
        matches!(self, Self::Ceff | Self::Feff)
    }

    /// The real classical/FD physical data-length ceiling for this format:
    /// [`CLASSICAL_CAN_MAX_DATA`] for [`FrameFormat::Cbff`]/
    /// [`FrameFormat::Ceff`], [`CAN_FD_MAX_PAYLOAD`] for
    /// [`FrameFormat::Fbff`]/[`FrameFormat::Feff`], and `None` for either
    /// XL variant (which use [`CAN_XL_MAX_PAYLOAD`] via [`CanXlFrame`]
    /// instead — see [`CanDataFrame`]).
    //fusa:req REQ-CAN-004
    pub fn max_data_len(self) -> Option<usize> {
        if self.is_classical() {
            Some(CLASSICAL_CAN_MAX_DATA)
        } else if self.is_fd() {
            Some(CAN_FD_MAX_PAYLOAD)
        } else {
            None
        }
    }
}

// ── CanDataFrame ─────────────────────────────────────────────────────────────

/// A Classical or FD CAN data frame: a [`FrameFormat`] tag (restricted to
/// the four non-XL variants), an arbitration ID, and data bytes.
///
/// See this module's doc comment "Data frames only — no remote-frame
/// representation" — this type has no field or variant that could represent
/// a remote frame.
#[derive(Debug, Clone, PartialEq, Eq)]
//fusa:req REQ-CAN-004
//fusa:req REQ-CAN-015
pub struct CanDataFrame {
    /// This frame's format. Always one of [`FrameFormat::Cbff`],
    /// [`FrameFormat::Ceff`], [`FrameFormat::Fbff`], [`FrameFormat::Feff`]
    /// — [`CanDataFrame::decode`] rejects either XL variant; use
    /// [`CanXlFrame`] for those instead.
    pub format: FrameFormat,
    /// This frame's arbitration ID: up to [`CAN_STANDARD_ID_MAX`] for the
    /// two base formats, up to [`CAN_EXTENDED_ID_MAX`] for the two extended
    /// formats.
    pub id: u32,
    /// This frame's data bytes, carried unparsed. Never longer than
    /// `format`'s own [`FrameFormat::max_data_len`] ceiling.
    pub data: Vec<u8>,
}

impl CanDataFrame {
    /// Encode this frame to its raw wire representation: one format-tag
    /// byte, then `id` as 4 big-endian bytes, then `data` unmodified.
    ///
    /// Performs no validation of its own — mirrors
    /// [`crate::lin::LinFrameTransfer::encode`]'s own trust-the-caller
    /// discipline; [`CanDataFrame::decode`] is where this module's
    /// validation lives. Never panics.
    ///
    /// The big-endian `id` field right-aligns an 11-bit CAN ID within the
    /// CAN ID field, as TC18 §13.7.11.3 (TC18.txt line 5471) requires: "In
    /// case the CAN ID is 11bits, then it shall be right aligned in the CAN
    /// ID field." No remote-frame (RTR) indication is emitted anywhere in
    /// this encoding — the same line states remote frames are not supported.
    /// See this module's doc comment "TC18 reconciliation note (§13.7.11)"
    /// for how this 5-byte form relates to Figure 39's own 32-bit word.
    //fusa:req REQ-CAN-004
    //fusa:req REQ-CAN-014
    //fusa:req REQ-CAN-015
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(1 + 4 + self.data.len());
        buf.push(self.format.to_u8());
        buf.extend_from_slice(&self.id.to_be_bytes());
        buf.extend_from_slice(&self.data);
        buf
    }

    /// Decode a [`CanDataFrame`] from a byte slice: one format-tag byte,
    /// then 4 big-endian arbitration-ID bytes, then data bytes.
    ///
    /// Returns `Err(RcpError::ShortFrame)` for input shorter than 5 bytes.
    /// Returns `Err(RcpError::InvalidParameter)` when the decoded format is
    /// either XL variant (use [`CanXlFrame::decode`] instead) or when the
    /// decoded arbitration ID exceeds the width `format` allows
    /// ([`CAN_STANDARD_ID_MAX`]/[`CAN_EXTENDED_ID_MAX`], per
    /// [`FrameFormat::is_extended_id`]). Returns
    /// `Err(RcpError::PayloadTooLarge)` when `data` exceeds `format`'s own
    /// [`FrameFormat::max_data_len`] ceiling. Never panics for any input.
    //fusa:req REQ-CAN-005
    pub fn decode(b: &[u8]) -> Result<Self, RcpError> {
        if b.len() < 5 {
            return Err(RcpError::ShortFrame);
        }
        let format = FrameFormat::from_u8(b[0])?;
        if format.is_xl() {
            return Err(RcpError::InvalidParameter);
        }
        let id = u32::from_be_bytes([b[1], b[2], b[3], b[4]]);
        let id_max = if format.is_extended_id() {
            CAN_EXTENDED_ID_MAX
        } else {
            CAN_STANDARD_ID_MAX
        };
        if id > id_max {
            return Err(RcpError::InvalidParameter);
        }
        let data = &b[5..];
        // `format.is_xl()` was already rejected above, so `max_data_len`
        // is always `Some(_)` here.
        let max_len = format.max_data_len().unwrap_or(CLASSICAL_CAN_MAX_DATA);
        if data.len() > max_len {
            return Err(RcpError::PayloadTooLarge);
        }
        Ok(Self {
            format,
            id,
            data: data.to_vec(),
        })
    }
}

// ── CanXlSubHeader / CanXlFrame ──────────────────────────────────────────────

/// CAN XL's 6-byte sub-header, carried opaque — see this module's doc
/// comment "Provenance note: the CAN XL sub-header is carried opaque".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-CAN-006
pub struct CanXlSubHeader(pub [u8; CAN_XL_SUB_HEADER_LEN]);

impl CanXlSubHeader {
    /// Encode this sub-header to its raw 6-byte wire representation.
    //fusa:req REQ-CAN-006
    pub fn encode(&self) -> [u8; CAN_XL_SUB_HEADER_LEN] {
        self.0
    }

    /// Decode a [`CanXlSubHeader`] from a byte slice's first
    /// [`CAN_XL_SUB_HEADER_LEN`] bytes.
    ///
    /// Returns `Err(RcpError::ShortFrame)` for input shorter than
    /// [`CAN_XL_SUB_HEADER_LEN`] bytes. Never panics for any input.
    //fusa:req REQ-CAN-006
    pub fn decode(b: &[u8]) -> Result<Self, RcpError> {
        if b.len() < CAN_XL_SUB_HEADER_LEN {
            return Err(RcpError::ShortFrame);
        }
        let mut raw = [0u8; CAN_XL_SUB_HEADER_LEN];
        raw.copy_from_slice(&b[..CAN_XL_SUB_HEADER_LEN]);
        Ok(Self(raw))
    }
}

/// A CAN XL data frame: a [`FrameFormat`] tag (restricted to the two XL
/// variants), a [`CanXlSubHeader`], and up to [`CAN_XL_MAX_PAYLOAD`] bytes
/// of payload.
///
/// See this module's doc comment "CAN XL fragmentation interaction" — this
/// type models CAN XL's own single-AVTPDU sub-header/payload framing only;
/// reassembling a payload that spans more than one AVTPDU is
/// `ROADMAP.md` Milestone 8's job. See also "Data frames only — no
/// remote-frame representation" — this type has no field or variant that
/// could represent a remote frame.
#[derive(Debug, Clone, PartialEq, Eq)]
//fusa:req REQ-CAN-007
pub struct CanXlFrame {
    /// This frame's format. Always one of [`FrameFormat::XlClassical`] or
    /// [`FrameFormat::XlNew`] — [`CanXlFrame::decode`] rejects any other
    /// value; use [`CanDataFrame`] for the four non-XL formats instead.
    pub format: FrameFormat,
    /// This frame's 6-byte sub-header, carried opaque.
    pub sub_header: CanXlSubHeader,
    /// This frame's payload, carried unparsed. Never longer than
    /// [`CAN_XL_MAX_PAYLOAD`] bytes.
    pub payload: Vec<u8>,
}

impl CanXlFrame {
    /// Encode this frame to its raw wire representation: one format-tag
    /// byte, then the sub-header's 6 bytes, then `payload` unmodified.
    ///
    /// Performs no validation of its own, mirroring
    /// [`CanDataFrame::encode`]'s trust-the-caller discipline. Never
    /// panics.
    //fusa:req REQ-CAN-007
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(1 + CAN_XL_SUB_HEADER_LEN + self.payload.len());
        buf.push(self.format.to_u8());
        buf.extend_from_slice(&self.sub_header.encode());
        buf.extend_from_slice(&self.payload);
        buf
    }

    /// Decode a [`CanXlFrame`] from a byte slice: one format-tag byte, then
    /// [`CAN_XL_SUB_HEADER_LEN`] sub-header bytes, then payload bytes.
    ///
    /// Returns `Err(RcpError::ShortFrame)` for input too short to contain
    /// the format tag plus a full sub-header. Returns
    /// `Err(RcpError::InvalidParameter)` when the decoded format is not one
    /// of the two XL variants (use [`CanDataFrame::decode`] instead).
    /// Returns `Err(RcpError::PayloadTooLarge)` when the remaining payload
    /// exceeds [`CAN_XL_MAX_PAYLOAD`] bytes. Never panics for any input.
    //fusa:req REQ-CAN-008
    //fusa:req REQ-CAN-009
    pub fn decode(b: &[u8]) -> Result<Self, RcpError> {
        if b.len() < 1 + CAN_XL_SUB_HEADER_LEN {
            return Err(RcpError::ShortFrame);
        }
        let format = FrameFormat::from_u8(b[0])?;
        if !format.is_xl() {
            return Err(RcpError::InvalidParameter);
        }
        let sub_header = CanXlSubHeader::decode(&b[1..])?;
        let payload = &b[1 + CAN_XL_SUB_HEADER_LEN..];
        if payload.len() > CAN_XL_MAX_PAYLOAD {
            return Err(RcpError::PayloadTooLarge);
        }
        Ok(Self {
            format,
            sub_header,
            payload: payload.to_vec(),
        })
    }
}

// ── CanXlCombinedPayload ─────────────────────────────────────────────────────

/// The combined payload of a multi-segment CAN XL "fragment train",
/// assembled by concatenating each fragment's own payload in the order the
/// caller supplies them.
///
/// Mirrors [`crate::e2e::CombinedFragmentPayload`]'s own caller-supplied-
/// ordering discipline for the same forward dependency on Milestone 8's
/// not-yet-built multi-AVTPDU reassembly buffer — see this module's doc
/// comment "CAN XL fragmentation interaction".
#[derive(Debug, Clone, PartialEq, Eq, Default)]
//fusa:req REQ-CAN-010
pub struct CanXlCombinedPayload(pub Vec<u8>);

impl CanXlCombinedPayload {
    /// Assembles a CAN XL fragment train's combined payload by
    /// concatenating `segments` verbatim, in the order given. An empty
    /// `segments` slice yields an empty combined payload; this function
    /// never panics for any input, including empty per-segment payloads.
    //fusa:req REQ-CAN-010
    pub fn assemble(segments: &[&[u8]]) -> Self {
        let mut combined = Vec::new();
        for segment in segments {
            combined.extend_from_slice(segment);
        }
        CanXlCombinedPayload(combined)
    }
}

// ── CanFunctionalConfig ──────────────────────────────────────────────────────

/// CAN controller's own per-EP-type functional-config content: the
/// selected [`FrameFormat`].
///
/// See this module's doc comment "Relationship to `crate::regmap`" for why
/// this carries a field (unlike [`crate::lin::LinFunctionalConfig`]'s empty
/// placeholder).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
//fusa:req REQ-CAN-011
pub struct CanFunctionalConfig {
    /// The [`FrameFormat`] this CAN controller endpoint is configured to
    /// use.
    pub format: FrameFormat,
}

impl CanFunctionalConfig {
    /// The [`crate::regmap::PerEpTypeFunctionalConfig`] generic-layer tag
    /// that matches this CAN functional config, for use with
    /// [`crate::regmap::check_functional_config_matches_ep_type`].
    ///
    /// This module does not itself call that function — it only shows how a
    /// caller would obtain the matching tag, per this module's doc comment
    /// "Relationship to `crate::regmap`".
    //fusa:req REQ-CAN-011
    pub fn layer_tag(&self) -> crate::regmap::PerEpTypeFunctionalConfig {
        crate::regmap::PerEpTypeFunctionalConfig::new(crate::regmap::EndpointType::Can)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Physical-fact constants ──────────────────────────────────────────────

    #[test]
    //fusa:test REQ-CAN-003
    fn physical_fact_constants_match_real_can_ceilings() {
        assert_eq!(CLASSICAL_CAN_MAX_DATA, 8);
        assert_eq!(CAN_FD_MAX_PAYLOAD, 64);
        assert_eq!(CAN_XL_MAX_PAYLOAD, 2048);
        assert_eq!(CAN_XL_SUB_HEADER_LEN, 6);
        assert_eq!(CAN_STANDARD_ID_MAX, 0x7FF);
        assert_eq!(CAN_EXTENDED_ID_MAX, 0x1FFF_FFFF);
    }

    // ── FrameFormat: encoding / classification round-trip ───────────────────

    #[test]
    //fusa:test REQ-CAN-001
    //fusa:test REQ-CAN-002
    fn frame_format_round_trips_through_to_u8_from_u8() {
        for format in [
            FrameFormat::Cbff,
            FrameFormat::Ceff,
            FrameFormat::Fbff,
            FrameFormat::Feff,
            FrameFormat::XlClassical,
            FrameFormat::XlNew,
        ] {
            assert_eq!(FrameFormat::from_u8(format.to_u8()), Ok(format));
        }
    }

    #[test]
    //fusa:test REQ-CAN-002
    fn frame_format_from_u8_rejects_out_of_range_byte() {
        for raw in [6u8, 7, 255] {
            assert_eq!(FrameFormat::from_u8(raw), Err(RcpError::InvalidParameter));
        }
    }

    // ── TC18 Table 54: confirmed FrameFormat wire values ────────────────────

    #[test]
    //fusa:test REQ-CAN-012
    fn frame_format_wire_values_match_tc18_table_54() {
        // TC18 §13.7.11.3 Table 54 "can frame formats" (TC18.txt line 5447),
        // transcribed row by row:
        //   CBFF                          -> 0
        //   CEFF                          -> 1
        //   FBFF                          -> 2
        //   FEFF                          -> 3
        //   XL (classic physical layer)   -> 4
        //   XL (new physical layer)       -> 5
        assert_eq!(FrameFormat::Cbff.to_u8(), 0);
        assert_eq!(FrameFormat::Ceff.to_u8(), 1);
        assert_eq!(FrameFormat::Fbff.to_u8(), 2);
        assert_eq!(FrameFormat::Feff.to_u8(), 3);
        assert_eq!(FrameFormat::XlClassical.to_u8(), 4);
        assert_eq!(FrameFormat::XlNew.to_u8(), 5);

        assert_eq!(FrameFormat::from_u8(0), Ok(FrameFormat::Cbff));
        assert_eq!(FrameFormat::from_u8(1), Ok(FrameFormat::Ceff));
        assert_eq!(FrameFormat::from_u8(2), Ok(FrameFormat::Fbff));
        assert_eq!(FrameFormat::from_u8(3), Ok(FrameFormat::Feff));
        assert_eq!(FrameFormat::from_u8(4), Ok(FrameFormat::XlClassical));
        assert_eq!(FrameFormat::from_u8(5), Ok(FrameFormat::XlNew));

        // Table 54's rows 4 and 5 are the two physical-layer variants of the
        // same XL frame format, so both classify as XL and neither as FD.
        assert!(FrameFormat::from_u8(4).unwrap().is_xl());
        assert!(FrameFormat::from_u8(5).unwrap().is_xl());
    }

    #[test]
    //fusa:test REQ-CAN-013
    fn frame_format_from_u8_rejects_table_54_reserved_rows_6_and_7() {
        // TC18 §13.7.11.3 Table 54 (TC18.txt lines 5454-5455): FrameFormat 6
        // and 7 are both "Reserved" — the only two of the 3-bit field's eight
        // code points without an assigned frame format.
        for reserved in [6u8, 7] {
            assert_eq!(
                FrameFormat::from_u8(reserved),
                Err(RcpError::InvalidParameter),
                "Table 54 row {reserved} is Reserved"
            );
        }
    }

    #[test]
    //fusa:test REQ-CAN-001
    fn frame_format_classification_helpers_partition_all_six_variants() {
        assert!(FrameFormat::Cbff.is_classical());
        assert!(FrameFormat::Ceff.is_classical());
        assert!(FrameFormat::Fbff.is_fd());
        assert!(FrameFormat::Feff.is_fd());
        assert!(FrameFormat::XlClassical.is_xl());
        assert!(FrameFormat::XlNew.is_xl());

        for format in [
            FrameFormat::Cbff,
            FrameFormat::Ceff,
            FrameFormat::Fbff,
            FrameFormat::Feff,
            FrameFormat::XlClassical,
            FrameFormat::XlNew,
        ] {
            let count = [format.is_classical(), format.is_fd(), format.is_xl()]
                .iter()
                .filter(|b| **b)
                .count();
            assert_eq!(count, 1);
        }
    }

    #[test]
    //fusa:test REQ-CAN-004
    fn frame_format_is_extended_id_matches_ceff_feff_only() {
        assert!(!FrameFormat::Cbff.is_extended_id());
        assert!(FrameFormat::Ceff.is_extended_id());
        assert!(!FrameFormat::Fbff.is_extended_id());
        assert!(FrameFormat::Feff.is_extended_id());
    }

    #[test]
    //fusa:test REQ-CAN-004
    fn frame_format_max_data_len_matches_classical_fd_ceilings_and_none_for_xl() {
        assert_eq!(
            FrameFormat::Cbff.max_data_len(),
            Some(CLASSICAL_CAN_MAX_DATA)
        );
        assert_eq!(
            FrameFormat::Ceff.max_data_len(),
            Some(CLASSICAL_CAN_MAX_DATA)
        );
        assert_eq!(FrameFormat::Fbff.max_data_len(), Some(CAN_FD_MAX_PAYLOAD));
        assert_eq!(FrameFormat::Feff.max_data_len(), Some(CAN_FD_MAX_PAYLOAD));
        assert_eq!(FrameFormat::XlClassical.max_data_len(), None);
        assert_eq!(FrameFormat::XlNew.max_data_len(), None);
    }

    // ── CanDataFrame: round-trip / never-panic ───────────────────────────────

    #[test]
    //fusa:test REQ-CAN-004
    //fusa:test REQ-CAN-005
    fn can_data_frame_round_trips_through_encode_decode() {
        for (format, id, data) in [
            (FrameFormat::Cbff, 0x000u32, vec![]),
            (FrameFormat::Cbff, CAN_STANDARD_ID_MAX, vec![0xAAu8; 8]),
            (FrameFormat::Ceff, CAN_EXTENDED_ID_MAX, vec![0x55u8; 3]),
            (FrameFormat::Fbff, 0x123, vec![0xFFu8; 64]),
            (FrameFormat::Feff, 0x1FFFF, vec![0x00u8; 40]),
        ] {
            let frame = CanDataFrame {
                format,
                id,
                data: data.clone(),
            };
            let decoded = CanDataFrame::decode(&frame.encode()).unwrap();
            assert_eq!(decoded.format, format);
            assert_eq!(decoded.id, id);
            assert_eq!(decoded.data, data);
        }
    }

    #[test]
    //fusa:test REQ-CAN-014
    fn can_data_frame_encode_right_aligns_an_11_bit_can_id() {
        // TC18 §13.7.11.3 (TC18.txt line 5471): "In case the CAN ID is
        // 11bits, then it shall be right aligned in the CAN ID field."
        //
        // 11-bit ID 0x123 in a 4-byte big-endian CAN ID field is
        // 0x00 0x00 0x01 0x23 — the ID's least-significant bit sits in the
        // field's least-significant bit, and the 21 leading bits are zero.
        let frame = CanDataFrame {
            format: FrameFormat::Cbff,
            id: 0x123,
            data: vec![0xDE, 0xAD],
        };
        assert_eq!(
            frame.encode(),
            vec![0x00, 0x00, 0x00, 0x01, 0x23, 0xDE, 0xAD]
        );

        // The widest 11-bit ID, 0x7FF, likewise occupies the field's low
        // bits — not the left-aligned 0xFFE0_0000 a 29-bit-field
        // left-justification would produce.
        let widest = CanDataFrame {
            format: FrameFormat::Fbff,
            id: CAN_STANDARD_ID_MAX,
            data: vec![],
        };
        assert_eq!(widest.encode(), vec![0x02, 0x00, 0x00, 0x07, 0xFF]);
        assert_ne!(&widest.encode()[1..5], &0xFFE0_0000u32.to_be_bytes()[..]);
    }

    #[test]
    //fusa:test REQ-CAN-015
    fn can_data_frame_encoding_carries_no_remote_frame_indication() {
        // TC18 §13.7.11.3 (TC18.txt line 5471): "Sending remote frames is not
        // supported." The encoded form is exactly one Table 54 format byte,
        // four CAN ID bytes, and the data bytes — there is no RTR bit, byte,
        // or trailing flag anywhere in it, for any of the four data-frame
        // formats.
        for (tag, format) in [
            (0u8, FrameFormat::Cbff),
            (1, FrameFormat::Ceff),
            (2, FrameFormat::Fbff),
            (3, FrameFormat::Feff),
        ] {
            let frame = CanDataFrame {
                format,
                id: 0x001,
                data: vec![0x11],
            };
            assert_eq!(frame.encode(), vec![tag, 0x00, 0x00, 0x00, 0x01, 0x11]);
            assert_eq!(frame.encode().len(), 5 + frame.data.len());
        }
    }

    #[test]
    //fusa:test REQ-CAN-005
    fn can_data_frame_decode_rejects_short_input() {
        for len in [0usize, 1, 2, 3, 4] {
            assert_eq!(
                CanDataFrame::decode(&vec![0u8; len]),
                Err(RcpError::ShortFrame)
            );
        }
    }

    #[test]
    //fusa:test REQ-CAN-005
    fn can_data_frame_decode_rejects_xl_format_tags() {
        for format in [FrameFormat::XlClassical, FrameFormat::XlNew] {
            let mut buf = vec![format.to_u8()];
            buf.extend_from_slice(&0u32.to_be_bytes());
            assert_eq!(CanDataFrame::decode(&buf), Err(RcpError::InvalidParameter));
        }
    }

    #[test]
    //fusa:test REQ-CAN-005
    fn can_data_frame_decode_rejects_id_wider_than_format_allows() {
        let mut buf = vec![FrameFormat::Cbff.to_u8()];
        buf.extend_from_slice(&(CAN_STANDARD_ID_MAX + 1).to_be_bytes());
        assert_eq!(CanDataFrame::decode(&buf), Err(RcpError::InvalidParameter));

        let mut buf = vec![FrameFormat::Ceff.to_u8()];
        buf.extend_from_slice(&(CAN_EXTENDED_ID_MAX + 1).to_be_bytes());
        assert_eq!(CanDataFrame::decode(&buf), Err(RcpError::InvalidParameter));
    }

    #[test]
    //fusa:test REQ-CAN-005
    fn can_data_frame_decode_rejects_data_exceeding_format_ceiling() {
        let mut buf = vec![FrameFormat::Cbff.to_u8()];
        buf.extend_from_slice(&0u32.to_be_bytes());
        buf.extend(vec![0xAAu8; CLASSICAL_CAN_MAX_DATA + 1]);
        assert_eq!(CanDataFrame::decode(&buf), Err(RcpError::PayloadTooLarge));

        let mut buf = vec![FrameFormat::Fbff.to_u8()];
        buf.extend_from_slice(&0u32.to_be_bytes());
        buf.extend(vec![0xAAu8; CAN_FD_MAX_PAYLOAD + 1]);
        assert_eq!(CanDataFrame::decode(&buf), Err(RcpError::PayloadTooLarge));
    }

    #[test]
    //fusa:test REQ-CAN-005
    fn can_data_frame_decode_never_panics_for_any_sampled_input() {
        for len in [0usize, 1, 4, 5, 6, 13, 200] {
            let buf = vec![0x5Au8; len];
            let _ = CanDataFrame::decode(&buf);
        }
    }

    // ── CanXlSubHeader / CanXlFrame: round-trip / never-panic ───────────────

    #[test]
    //fusa:test REQ-CAN-006
    fn can_xl_sub_header_round_trips_through_encode_decode() {
        let header = CanXlSubHeader([0x01, 0x02, 0x03, 0x04, 0x05, 0x06]);
        assert_eq!(CanXlSubHeader::decode(&header.encode()).unwrap(), header);
    }

    #[test]
    //fusa:test REQ-CAN-006
    fn can_xl_sub_header_decode_rejects_short_input() {
        for len in [0usize, 1, 5] {
            assert_eq!(
                CanXlSubHeader::decode(&vec![0u8; len]),
                Err(RcpError::ShortFrame)
            );
        }
    }

    #[test]
    //fusa:test REQ-CAN-007
    //fusa:test REQ-CAN-008
    fn can_xl_frame_round_trips_through_encode_decode() {
        for (format, payload) in [
            (FrameFormat::XlClassical, vec![]),
            (FrameFormat::XlClassical, vec![0xAAu8; 2048]),
            (FrameFormat::XlNew, vec![0x11u8; 512]),
        ] {
            let frame = CanXlFrame {
                format,
                sub_header: CanXlSubHeader([9, 8, 7, 6, 5, 4]),
                payload: payload.clone(),
            };
            let decoded = CanXlFrame::decode(&frame.encode()).unwrap();
            assert_eq!(decoded.format, format);
            assert_eq!(decoded.sub_header, frame.sub_header);
            assert_eq!(decoded.payload, payload);
        }
    }

    #[test]
    //fusa:test REQ-CAN-008
    fn can_xl_frame_decode_rejects_non_xl_format_tags() {
        for format in [
            FrameFormat::Cbff,
            FrameFormat::Ceff,
            FrameFormat::Fbff,
            FrameFormat::Feff,
        ] {
            let mut buf = vec![format.to_u8()];
            buf.extend_from_slice(&[0u8; CAN_XL_SUB_HEADER_LEN]);
            assert_eq!(CanXlFrame::decode(&buf), Err(RcpError::InvalidParameter));
        }
    }

    #[test]
    //fusa:test REQ-CAN-008
    fn can_xl_frame_decode_rejects_short_input() {
        for len in [0usize, 1, 3, 6] {
            assert_eq!(
                CanXlFrame::decode(&vec![FrameFormat::XlNew.to_u8(); len.max(1)][..len],),
                Err(RcpError::ShortFrame)
            );
        }
    }

    #[test]
    //fusa:test REQ-CAN-009
    fn can_xl_frame_decode_rejects_payload_exceeding_2048_bytes() {
        let mut buf = vec![FrameFormat::XlClassical.to_u8()];
        buf.extend_from_slice(&[0u8; CAN_XL_SUB_HEADER_LEN]);
        buf.extend(vec![0xAAu8; CAN_XL_MAX_PAYLOAD + 1]);
        assert_eq!(CanXlFrame::decode(&buf), Err(RcpError::PayloadTooLarge));
    }

    #[test]
    //fusa:test REQ-CAN-009
    fn can_xl_frame_decode_accepts_payload_at_exactly_2048_bytes() {
        let mut buf = vec![FrameFormat::XlNew.to_u8()];
        buf.extend_from_slice(&[0u8; CAN_XL_SUB_HEADER_LEN]);
        buf.extend(vec![0xAAu8; CAN_XL_MAX_PAYLOAD]);
        let decoded = CanXlFrame::decode(&buf).unwrap();
        assert_eq!(decoded.payload.len(), CAN_XL_MAX_PAYLOAD);
    }

    #[test]
    //fusa:test REQ-CAN-016
    fn can_xl_can_data_field_totals_2054_bytes() {
        // TC18 §13.7.11.3 (TC18.txt line 5443): "For CAN XL this can be up to
        // 2054 bytes (2048 + 6, see below)", and line 5472: the "CAN data"
        // field includes 6 additional bytes (RRS, SDT, VCID, AF — see
        // ISO 11898-1) for either XL frame format.
        assert_eq!(CAN_XL_SUB_HEADER_LEN + CAN_XL_MAX_PAYLOAD, 2054);

        let frame = CanXlFrame {
            format: FrameFormat::XlNew,
            sub_header: CanXlSubHeader([0u8; CAN_XL_SUB_HEADER_LEN]),
            payload: vec![0xA5; CAN_XL_MAX_PAYLOAD],
        };
        // 2054 CAN-data bytes, plus this module's own leading format tag.
        assert_eq!(frame.encode().len(), 1 + 2054);
        let decoded = CanXlFrame::decode(&frame.encode()).unwrap();
        assert_eq!(
            decoded.sub_header.encode().len() + decoded.payload.len(),
            2054
        );
    }

    #[test]
    //fusa:test REQ-CAN-008
    fn can_xl_frame_decode_never_panics_for_any_sampled_input() {
        for len in [0usize, 1, 6, 7, 20, 300] {
            let buf = vec![0x5Au8; len];
            let _ = CanXlFrame::decode(&buf);
        }
    }

    // ── CanXlCombinedPayload ──────────────────────────────────────────────────

    #[test]
    //fusa:test REQ-CAN-010
    fn can_xl_combined_payload_concatenates_segments_in_caller_supplied_order() {
        let segments: Vec<&[u8]> = vec![&[1, 2, 3], &[], &[4, 5]];
        let combined = CanXlCombinedPayload::assemble(&segments);
        assert_eq!(combined.0, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    //fusa:test REQ-CAN-010
    fn can_xl_combined_payload_empty_segments_yields_empty_payload() {
        assert_eq!(
            CanXlCombinedPayload::assemble(&[]),
            CanXlCombinedPayload(vec![])
        );
    }

    // ── CanFunctionalConfig / layer_tag ──────────────────────────────────────

    #[test]
    //fusa:test REQ-CAN-011
    fn can_functional_config_layer_tag_matches_ep_type_can() {
        let functional = CanFunctionalConfig {
            format: FrameFormat::Fbff,
        };
        let generic = crate::regmap::PerEpConfigBlock::new(crate::regmap::EndpointType::Can);
        let tag = functional.layer_tag();
        assert_eq!(tag.ep_type, crate::regmap::EndpointType::Can);
        assert!(crate::regmap::functional_config_matches_ep_type(
            &generic, &tag
        ));
        assert_eq!(
            crate::regmap::check_functional_config_matches_ep_type(&generic, &tag),
            Ok(())
        );
    }

    #[test]
    //fusa:test REQ-CAN-011
    fn can_functional_config_layer_tag_rejects_mismatched_ep_type() {
        let functional = CanFunctionalConfig {
            format: FrameFormat::Cbff,
        };
        let generic = crate::regmap::PerEpConfigBlock::new(crate::regmap::EndpointType::Lin);
        let tag = functional.layer_tag();
        assert!(!crate::regmap::functional_config_matches_ep_type(
            &generic, &tag
        ));
    }
}
