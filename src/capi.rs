// fusa:req REQ-CAPI-001
// fusa:req REQ-CAPI-002
// fusa:req REQ-CAPI-003
// fusa:req REQ-CAPI-004

//! C API bridge — exposes a C-compatible FFI surface for embedding this
//! crate's OPEN Alliance TC18 Remote Control Protocol Specification
//! v0.5.1_RC core in C/C++ codebases. All types are `#[repr(C)]`; no
//! unsafe code in the core library; the FFI layer is tested via Rust
//! wrappers, matching this module's pre-Milestone-9 discipline.
//!
//! `ROADMAP.md` Milestone 9 (`capi` REPLACE-disposition cutover): the old
//! `CCommand`/`CResponse`/`CError` trio, 1:1 tied to the now-legacy
//! `Command`/`Zone` shapes (see the Satellite Package Disposition table's
//! own reason for this row), is REPLACEd outright — deleted, not adapted,
//! the same discipline `watchdog`/`powerstate`/`wire`/`e2e`/`mock`/`config`
//! used in the milestones and items immediately before this one — with a
//! new surface addressed by [`crate::avtp::StreamId`] and the ACF_ABB
//! `byte_message_info` header ([`crate::acf::ByteMessageInfo`]) instead of
//! `Zone`/`Command`:
//!
//! - [`CStreamId`] mirrors [`StreamId`]'s two fields verbatim.
//! - [`CByteMessageInfo`] mirrors every [`ByteMessageInfo`] field, flattened
//!   — [`crate::acf::Evt`]'s `ack`/`sub_opcode` pair becomes `evt_ack`/
//!   `evt_sub_opcode`, and [`crate::acf::ReadSizeOrSegment`]'s wrapped
//!   value becomes a plain `u16` — since neither of those two small wrapper
//!   types is itself `#[repr(C)]`. Field-width validation (`acf_msg_length`/
//!   `byte_bus_id` fitting 11 bits, `evt_sub_opcode` fitting 3 bits) is
//!   deliberately not re-checked by either direction's conversion below —
//!   that already happens in [`crate::acf::encode_byte_message_info`] at
//!   actual wire-encode time, the same "only checked once, where the
//!   encode/decode step already enforces it" discipline `src/config.rs`'s
//!   own `validate()` doc comment used for its own field-width bounds.
//! - [`CAbbRequest`]/[`CAbbResponse`] each pair a [`CStreamId`] with a
//!   [`CByteMessageInfo`], mirroring how a real request and its response
//!   are actually the *same* [`crate::acf::AcfAbbMessage`] Rust type today
//!   (direction is carried in [`ByteMessageInfo::op`]/[`ByteMessageInfo::
//!   rsp`], not by two different Rust types) — unlike the old `CCommand`/
//!   `CResponse`, which really were two structurally different shapes
//!   (`cmd_type`+`priority` vs. `status`). They are still kept as two
//!   distinct Rust types here rather than one, despite the identical
//!   field layout: a C caller's call sites read more clearly naming which
//!   direction they're building, and nothing stops a request-only or
//!   response-only field from being added to just one of them later
//!   without disturbing the other's ABI.
//! - [`CError`] is rebuilt against the current [`crate::RcpError`]'s real
//!   TC18 spec error codes (the eleven [`crate::RcpError::
//!   is_tc18_error_code`] members) plus every other still-live variant,
//!   instead of the old `Closed`/`NotFound`/`ZoneMismatch`/etc. set (see
//!   [`crate::RcpError`]'s own "General-purpose sentinels" doc-comment
//!   section for why `NotFound`/`AlreadyExists`/`Busy` specifically have
//!   no TC18 analog and collapse to [`CError::Other`] below, the same as
//!   `RcpError::Other(_)`; the fourth sibling variant that used to be
//!   here, `ZoneMismatch`, had no TC18 or general-purpose meaning at all
//!   and has been removed from `RcpError` outright, rust-RCP-FS-02).
//!
//! Neither `CAbbRequest` nor `CAbbResponse` carries the ACF_ABB message's
//! variable-length `payload: Vec<u8>` — same scope limit the old
//! `CCommand`/`Command` conversion had (it dropped `Command::payload`
//! outright, always reconstructing `payload: None`), because a `Vec<u8>`
//! has no fixed-size `#[repr(C)]` representation. Carrying payload bytes
//! across a real C boundary is a raw-pointer-plus-length concern for
//! whichever `extern "C"` cdylib target eventually wraps this module (see
//! this module's own note below) — this crate `#![forbid(unsafe_code)]`
//! and has no such target today, so that boundary is left unbuilt rather
//! than invented here.
//!
//! Note: actual `extern "C"` declarations still live in a separate
//! optional cdylib target, not built by this crate today; this module
//! provides the Rust-side wrappers and type definitions only, the same
//! scope limit this module had before this item.
//!
//! This module had zero callers anywhere else in `src/` before this item
//! (confirmed by inspection: `src/bin/rcp.rs` and every other module
//! construct neither `CCommand`/`CResponse` nor now `CStreamId`/
//! `CAbbRequest`/`CAbbResponse`) and still has none after it, so — like
//! `wire`/`e2e`/`config` before it — this is a self-contained cutover with
//! no other file's callers to fix.

use crate::acf::{ByteMessageInfo, Evt, ReadSizeOrSegment};
use crate::avtp::StreamId;
use crate::RcpError;

// ── CStreamId ─────────────────────────────────────────────────────────────────

/// C-compatible mirror of [`StreamId`]: a sender MAC address plus a
/// locally-assigned unique-id suffix.
// fusa:req REQ-CAPI-001
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CStreamId {
    pub sender_mac: [u8; 6],
    pub unique_id: u16,
}

impl From<StreamId> for CStreamId {
    // fusa:req REQ-CAPI-003
    fn from(id: StreamId) -> Self {
        CStreamId {
            sender_mac: id.sender_mac,
            unique_id: id.unique_id,
        }
    }
}

impl From<CStreamId> for StreamId {
    // fusa:req REQ-CAPI-003
    fn from(id: CStreamId) -> Self {
        StreamId::new(id.sender_mac, id.unique_id)
    }
}

// ── CByteMessageInfo ───────────────────────────────────────────────────────────

/// C-compatible, flattened mirror of [`ByteMessageInfo`]. See this
/// module's doc comment for why `evt`/`read_size_segment` are
/// flattened rather than nested, and why field-width validation is not
/// repeated here.
// fusa:req REQ-CAPI-002
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CByteMessageInfo {
    pub acf_msg_type: u8,
    pub acf_msg_length: u16,
    pub pad: u8,
    pub mtv: bool,
    pub byte_bus_id: u16,
    pub evt_ack: bool,
    pub evt_sub_opcode: u8,
    pub hs: bool,
    pub cs: bool,
    pub transaction_num: u8,
    pub op: bool,
    pub rsp: bool,
    pub err: bool,
    pub ms: bool,
    pub read_size_segment: u16,
}

impl From<ByteMessageInfo> for CByteMessageInfo {
    // fusa:req REQ-CAPI-003
    fn from(info: ByteMessageInfo) -> Self {
        CByteMessageInfo {
            acf_msg_type: info.acf_msg_type,
            acf_msg_length: info.acf_msg_length,
            pad: info.pad,
            mtv: info.mtv,
            byte_bus_id: info.byte_bus_id,
            evt_ack: info.evt.ack,
            evt_sub_opcode: info.evt.sub_opcode,
            hs: info.hs,
            cs: info.cs,
            transaction_num: info.transaction_num,
            op: info.op,
            rsp: info.rsp,
            err: info.err,
            ms: info.ms,
            read_size_segment: info.read_size_segment.0,
        }
    }
}

impl From<CByteMessageInfo> for ByteMessageInfo {
    // fusa:req REQ-CAPI-003
    fn from(c: CByteMessageInfo) -> Self {
        ByteMessageInfo {
            acf_msg_type: c.acf_msg_type,
            acf_msg_length: c.acf_msg_length,
            pad: c.pad,
            mtv: c.mtv,
            byte_bus_id: c.byte_bus_id,
            evt: Evt {
                ack: c.evt_ack,
                sub_opcode: c.evt_sub_opcode,
            },
            hs: c.hs,
            cs: c.cs,
            transaction_num: c.transaction_num,
            op: c.op,
            rsp: c.rsp,
            err: c.err,
            ms: c.ms,
            read_size_segment: ReadSizeOrSegment(c.read_size_segment),
        }
    }
}

// ── CAbbRequest / CAbbResponse ─────────────────────────────────────────────────

/// C-compatible ACF_ABB request header: a [`CStreamId`] plus the request's
/// [`CByteMessageInfo`]. See this module's doc comment for why `payload`
/// bytes are not part of this type.
// fusa:req REQ-CAPI-001
// fusa:req REQ-CAPI-002
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CAbbRequest {
    pub stream: CStreamId,
    pub info: CByteMessageInfo,
}

impl CAbbRequest {
    /// Build a request header from a [`StreamId`] and a [`ByteMessageInfo`].
    pub fn new(stream_id: StreamId, info: ByteMessageInfo) -> Self {
        CAbbRequest {
            stream: stream_id.into(),
            info: info.into(),
        }
    }

    /// This request's [`StreamId`].
    pub fn stream_id(&self) -> StreamId {
        self.stream.into()
    }

    /// This request's [`ByteMessageInfo`].
    pub fn info(&self) -> ByteMessageInfo {
        self.info.into()
    }
}

/// C-compatible ACF_ABB response header — same field layout as
/// [`CAbbRequest`] (see this module's doc comment for why they are still
/// kept as two distinct types), for a response's [`CStreamId`]/
/// [`CByteMessageInfo`].
// fusa:req REQ-CAPI-001
// fusa:req REQ-CAPI-002
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CAbbResponse {
    pub stream: CStreamId,
    pub info: CByteMessageInfo,
}

impl CAbbResponse {
    /// Build a response header from a [`StreamId`] and a [`ByteMessageInfo`].
    pub fn new(stream_id: StreamId, info: ByteMessageInfo) -> Self {
        CAbbResponse {
            stream: stream_id.into(),
            info: info.into(),
        }
    }

    /// This response's [`StreamId`].
    pub fn stream_id(&self) -> StreamId {
        self.stream.into()
    }

    /// This response's [`ByteMessageInfo`].
    pub fn info(&self) -> ByteMessageInfo {
        self.info.into()
    }
}

// ── Error codes ───────────────────────────────────────────────────────────────

/// C-compatible error code, rebuilt against every current
/// [`crate::RcpError`] variant. See this module's doc comment for the
/// full provenance/mapping note.
// fusa:req REQ-CAPI-004
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CError {
    Ok = 0,

    // ── RELAY mandatory sentinels (not Zone-specific; kept as-is) ──────────
    Closed = 1,
    NotConnected = 2,
    Timeout = 3,
    PayloadTooLarge = 4,

    // ── General, non-legacy wire error ──────────────────────────────────────
    ShortFrame = 5,

    // ── TC18 RCP spec error codes (RcpError::is_tc18_error_code) ────────────
    UnsupportedCmd = 10,
    SequencerNotKnown = 11,
    UnauthorizedAccess = 12,
    LockedMemAccess = 13,
    RequestCanceled = 14,
    RequestNotFound = 15,
    EpError = 16,
    EpNotFound = 17,
    ReqStorageOvfl = 18,
    RequestRejected = 19,
    InvalidParameter = 20,

    // ── Chained-request error codes ─────────────────────────────────────────
    ChainAborted = 21,
    ChainError = 22,

    // ── CRC_ERROR error code ─────────────────────────────────────────────────
    CrcError = 23,

    // ── General errors ──────────────────────────────────────────────────────
    InvalidSize = 24,

    // ── Remaining TC18 Table 27 error codes (rust-RCP-W05) ───────────────────
    PwmInNoSignal = 25,
    PociFailure = 26,
    PresentationTimeTooFar = 27,
    GptpFail = 28,

    /// Catch-all: the legacy `NotFound`/`AlreadyExists`/`Busy`
    /// Zone/Controller/Registry sentinels (no TC18 analog — see this
    /// module's doc comment) and `RcpError::Other(String)` (whose message
    /// this fieldless `#[repr(C)]` enum has no room to carry) both map
    /// here.
    Other = 99,
}

impl From<&RcpError> for CError {
    // fusa:req REQ-CAPI-004
    fn from(e: &RcpError) -> Self {
        match e {
            RcpError::Closed => CError::Closed,
            RcpError::NotConnected => CError::NotConnected,
            RcpError::Timeout => CError::Timeout,
            RcpError::PayloadTooLarge => CError::PayloadTooLarge,
            RcpError::ShortFrame => CError::ShortFrame,
            RcpError::UnsupportedCmd => CError::UnsupportedCmd,
            RcpError::SequencerNotKnown => CError::SequencerNotKnown,
            RcpError::UnauthorizedAccess => CError::UnauthorizedAccess,
            RcpError::LockedMemAccess => CError::LockedMemAccess,
            RcpError::RequestCanceled => CError::RequestCanceled,
            RcpError::RequestNotFound => CError::RequestNotFound,
            RcpError::EpError => CError::EpError,
            RcpError::EpNotFound => CError::EpNotFound,
            RcpError::ReqStorageOvfl => CError::ReqStorageOvfl,
            RcpError::RequestRejected => CError::RequestRejected,
            RcpError::InvalidParameter => CError::InvalidParameter,
            RcpError::ChainAborted => CError::ChainAborted,
            RcpError::ChainError => CError::ChainError,
            RcpError::CrcError => CError::CrcError,
            RcpError::InvalidSize => CError::InvalidSize,
            RcpError::PwmInNoSignal => CError::PwmInNoSignal,
            RcpError::PociFailure => CError::PociFailure,
            RcpError::PresentationTimeTooFar => CError::PresentationTimeTooFar,
            RcpError::GptpFail => CError::GptpFail,
            RcpError::NotFound | RcpError::AlreadyExists | RcpError::Busy | RcpError::Other(_) => {
                CError::Other
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::acf::{Evt, ReadSizeOrSegment};

    fn stream_id() -> StreamId {
        StreamId::new([0x02, 0x11, 0x22, 0x33, 0x44, 0x55], 0x0007)
    }

    fn info(byte_bus_id: u16, op: bool) -> ByteMessageInfo {
        ByteMessageInfo {
            acf_msg_type: crate::acf::ACF_ABB_MSG_TYPE,
            acf_msg_length: 12,
            pad: 0,
            mtv: true,
            byte_bus_id,
            evt: Evt {
                ack: true,
                sub_opcode: 0x5,
            },
            hs: false,
            cs: true,
            transaction_num: 9,
            op,
            rsp: !op,
            err: false,
            ms: false,
            read_size_segment: ReadSizeOrSegment(0x22),
        }
    }

    // ── repr(C) sanity ───────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-CAPI-001
    fn c_stream_id_is_repr_c() {
        // 6-byte MAC + u16 unique_id: at least 8 bytes on every platform.
        assert!(std::mem::size_of::<CStreamId>() >= 8);
    }

    #[test]
    // fusa:test REQ-CAPI-002
    fn c_byte_message_info_is_repr_c() {
        // 2x u16 + 9 bool/u8-sized fields: at least 13 bytes on every
        // platform (padding may add more, never less).
        assert!(std::mem::size_of::<CByteMessageInfo>() >= 13);
    }

    // ── CStreamId <-> StreamId ───────────────────────────────────────────────

    #[test]
    // fusa:test REQ-CAPI-003
    fn stream_id_round_trip() {
        let sid = stream_id();
        let c: CStreamId = sid.into();
        let back: StreamId = c.into();
        assert_eq!(back, sid);
    }

    // ── CByteMessageInfo <-> ByteMessageInfo ─────────────────────────────────

    #[test]
    // fusa:test REQ-CAPI-003
    fn byte_message_info_round_trip() {
        let bmi = info(7, true);
        let c: CByteMessageInfo = bmi.into();
        assert_eq!(c.byte_bus_id, 7);
        assert!(c.op);
        assert!(!c.rsp);
        assert!(c.evt_ack);
        assert_eq!(c.evt_sub_opcode, 0x5);
        assert_eq!(c.read_size_segment, 0x22);
        let back: ByteMessageInfo = c.into();
        assert_eq!(back, bmi);
    }

    // ── CAbbRequest / CAbbResponse ───────────────────────────────────────────

    #[test]
    // fusa:test REQ-CAPI-001
    // fusa:test REQ-CAPI-002
    // fusa:test REQ-CAPI-003
    fn abb_request_round_trip() {
        let sid = stream_id();
        let bmi = info(7, true);
        let req = CAbbRequest::new(sid, bmi);
        assert_eq!(req.stream_id(), sid);
        assert_eq!(req.info(), bmi);
    }

    #[test]
    // fusa:test REQ-CAPI-001
    // fusa:test REQ-CAPI-002
    // fusa:test REQ-CAPI-003
    fn abb_response_round_trip() {
        let sid = stream_id();
        let bmi = info(7, false);
        let resp = CAbbResponse::new(sid, bmi);
        assert_eq!(resp.stream_id(), sid);
        assert_eq!(resp.info(), bmi);
    }

    #[test]
    // fusa:test REQ-CAPI-003
    fn request_and_response_headers_are_distinct_types_same_layout() {
        // Same field layout by construction (see this module's doc
        // comment) — this test exists so a future divergence in either
        // struct's field list is a visible, deliberate size change here,
        // not a silent one.
        assert_eq!(
            std::mem::size_of::<CAbbRequest>(),
            std::mem::size_of::<CAbbResponse>()
        );
    }

    // ── CError mapping ───────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-CAPI-004
    fn error_code_mapping_covers_tc18_codes() {
        assert_eq!(
            CError::from(&RcpError::UnsupportedCmd),
            CError::UnsupportedCmd
        );
        assert_eq!(
            CError::from(&RcpError::SequencerNotKnown),
            CError::SequencerNotKnown
        );
        assert_eq!(
            CError::from(&RcpError::UnauthorizedAccess),
            CError::UnauthorizedAccess
        );
        assert_eq!(
            CError::from(&RcpError::LockedMemAccess),
            CError::LockedMemAccess
        );
        assert_eq!(
            CError::from(&RcpError::RequestCanceled),
            CError::RequestCanceled
        );
        assert_eq!(
            CError::from(&RcpError::RequestNotFound),
            CError::RequestNotFound
        );
        assert_eq!(CError::from(&RcpError::EpError), CError::EpError);
        assert_eq!(CError::from(&RcpError::EpNotFound), CError::EpNotFound);
        assert_eq!(
            CError::from(&RcpError::ReqStorageOvfl),
            CError::ReqStorageOvfl
        );
        assert_eq!(
            CError::from(&RcpError::RequestRejected),
            CError::RequestRejected
        );
        assert_eq!(
            CError::from(&RcpError::InvalidParameter),
            CError::InvalidParameter
        );
        // rust-RCP-W05: the four TC18 Table 27 codes with no prior
        // RcpError variant at all.
        assert_eq!(
            CError::from(&RcpError::PwmInNoSignal),
            CError::PwmInNoSignal
        );
        assert_eq!(CError::from(&RcpError::PociFailure), CError::PociFailure);
        assert_eq!(
            CError::from(&RcpError::PresentationTimeTooFar),
            CError::PresentationTimeTooFar
        );
        assert_eq!(CError::from(&RcpError::GptpFail), CError::GptpFail);
    }

    #[test]
    // fusa:test REQ-CAPI-004
    fn error_code_mapping_covers_relay_and_general_sentinels() {
        assert_eq!(CError::from(&RcpError::Closed), CError::Closed);
        assert_eq!(CError::from(&RcpError::NotConnected), CError::NotConnected);
        assert_eq!(CError::from(&RcpError::Timeout), CError::Timeout);
        assert_eq!(
            CError::from(&RcpError::PayloadTooLarge),
            CError::PayloadTooLarge
        );
        assert_eq!(CError::from(&RcpError::ShortFrame), CError::ShortFrame);
        assert_eq!(CError::from(&RcpError::ChainAborted), CError::ChainAborted);
        assert_eq!(CError::from(&RcpError::ChainError), CError::ChainError);
        assert_eq!(CError::from(&RcpError::CrcError), CError::CrcError);
        assert_eq!(CError::from(&RcpError::InvalidSize), CError::InvalidSize);
    }

    #[test]
    // fusa:test REQ-CAPI-004
    fn error_code_mapping_collapses_legacy_and_other_to_other() {
        assert_eq!(CError::from(&RcpError::NotFound), CError::Other);
        assert_eq!(CError::from(&RcpError::AlreadyExists), CError::Other);
        assert_eq!(CError::from(&RcpError::Busy), CError::Other);
        assert_eq!(
            CError::from(&RcpError::Other("x".to_string())),
            CError::Other
        );
    }
}
