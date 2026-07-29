// fusa:req REQ-ERR-001
// fusa:req REQ-ERR-002
// fusa:req REQ-ERR-003
// fusa:req REQ-ERR-004
// fusa:req REQ-ERR-005
// fusa:req REQ-ERR-006
// fusa:req REQ-ERR-007
// fusa:req REQ-ERR-008
// fusa:req REQ-ERR-009
// fusa:req REQ-ERR-010
// fusa:req REQ-ERR-011
// fusa:req REQ-ERR-012
// fusa:req REQ-ERR-013
// fusa:req REQ-ERR-014
// fusa:req REQ-ERR-015
// fusa:req REQ-ERR-016
// fusa:req REQ-ERR-017
// fusa:req REQ-ERR-018
// fusa:req REQ-ERR-019
// fusa:req REQ-ERR-020
// fusa:req REQ-ERR-021
// fusa:req REQ-ERRM-001
// fusa:req REQ-ERRM-002
// fusa:req REQ-ERRM-003
// fusa:req REQ-ERRM-004
// fusa:req REQ-ERRM-005
// fusa:req REQ-ERRM-006
// fusa:req REQ-ERRM-007
// fusa:req REQ-ERRM-008
// fusa:req REQ-ERRM-009
// fusa:req REQ-ERRM-010
// fusa:req REQ-ERRM-011
// fusa:req REQ-ERRM-012
// fusa:req REQ-ERRM-013
// fusa:req REQ-SPEC-001

//! Remote Control Protocol (RCP) — a Rust implementation of the OPEN
//! Alliance TC18 Remote Control Protocol Specification v0.5.1_RC for
//! automotive zonal architecture.
//!
//! An RC Client addresses a peer RC Server's device endpoints by
//! `(`[`avtp::StreamId`]`, byte_bus_id)`, exchanging AVTPDU/ACF-framed
//! requests ([`avtp`], [`acf`]) that this crate decodes, routes
//! ([`ep0`], [`addressing`]), and dispatches against the RC Server's
//! lifecycle/register-map model ([`lifecycle`], [`regmap`]). [`mock::RcServer`]
//! is this crate's in-process reference implementation of that dispatch
//! path; [`mod@adapt`] binds it to the RELAY specification's `Adapt()` /
//! `to_message()` / `from_message()` contract.
//!
//! This crate implements RELAY specification version [`SPEC_VERSION`].
//!
//! See `docs/SEMVER.md` for which parts of this surface carry a semver
//! stability guarantee as of Milestone 10 (`ROADMAP.md`) and which remain
//! explicitly unstable.
//!
//! ## Note on the pre-Milestone-10 API
//!
//! Earlier versions of this crate (pre-`v1.0.0`, before the OPEN Alliance
//! TC18 uplift `ROADMAP.md` Milestones 1-10 carried out) exposed a
//! different, `Zone`-keyed `Command`/`Response`/`Status`/`Controller`/
//! `Registry` model. That model has been removed outright — per
//! `ROADMAP.md`'s own breaking-change notice, it never had a compatibility
//! shim — and none of its identifiers survive in this crate. It is
//! mentioned here only so anyone consulting old external documentation
//! against a current checkout understands why those names no longer
//! resolve.

#![forbid(unsafe_code)]

pub mod acf;
pub mod adapt;
pub mod adc;
pub mod addressing;
pub mod admin;
pub mod authz;
pub mod avtp;
pub(crate) mod base64_serde;
pub mod can;
pub mod capi;
pub mod certgap;
pub mod codegen;
pub mod config;
pub mod deadline;
pub mod discovery;
pub mod dyndata;
pub mod e2e;
pub mod ep0;
pub mod evtgroup;
pub mod faultinject;
pub mod federation;
pub mod formal;
pub mod fragment;
pub mod gpio;
pub mod i2c;
pub mod iseled;
pub mod iso21434;
pub mod lifecycle;
pub mod lin;
pub mod loan;
pub mod mdio;
pub mod mdns;
pub mod mock;
pub mod observe;
pub mod powerstate;
pub mod proxy;
pub mod pwm;
pub mod ratelimit;
pub mod record;
pub mod redundancy;
pub mod regmap;
pub mod relay;
pub mod request;
pub mod shmem;
pub mod sim;
pub mod spi;
pub mod timestamp;
pub mod tlstransport;
pub mod uart;
pub mod udp;
pub mod wakeup;
pub mod watchdog;

pub use adapt::{adapt, from_message, to_message};

use std::fmt;

// ── Spec version ────────────────────────────────────────────────────────────

/// RELAY specification version this crate implements.
// fusa:req REQ-SPEC-001
pub const SPEC_VERSION: &str = "1.11";

/// Alias for [`SPEC_VERSION`], exported from the crate root per RELAY spec
/// §18.3 ("`RELAY_SPEC_VERSION` MUST be exported from the crate root").
// fusa:req REQ-SPEC-001
pub const RELAY_SPEC_VERSION: &str = SPEC_VERSION;

// ── Error types ───────────────────────────────────────────────────────────────

/// All errors produced by this crate.
///
/// Sentinel relationships (mirroring RELAY spec §5 `errors.Is` chains):
/// - `NotFound`     → `is_not_connected()` (wraps `NotConnected`)
/// - `ZoneMismatch` → `is_not_connected()` (wraps `NotConnected`)
/// - `Busy`         → `is_timeout()` (wraps `Timeout`)
///
/// ## TC18 spec error codes (`ROADMAP.md` Milestone 2, "Error Model")
///
/// The "TC18 RCP spec error codes" group below gives this crate the eleven
/// named error codes enumerated by that checklist item —
/// [`UnsupportedCmd`](Self::UnsupportedCmd) through
/// [`InvalidParameter`](Self::InvalidParameter), Rust-cased equivalents of
/// the specification's own `UPPER_SNAKE_CASE` names (cited here as the OPEN
/// Alliance TC18 Remote Control Protocol Specification v0.5.1_RC, by name
/// only, per Guiding Principle 4). The timing- and CRC-specific codes this
/// checklist item explicitly defers are not added here; they remain the
/// later milestones' job (Milestone 6, per that milestone's own `CRC_ERROR`
/// error-path bullet).
///
/// Every module built earlier in this milestone (`lifecycle`, `ep0`,
/// `regmap`, `addressing`, `avtp`) had already introduced its own
/// provisional `RcpError` sentinel for its guard/check functions, each
/// explicitly documented at the time as a placeholder pending this exact
/// item (see each module's own doc comment history). This item retires
/// every one of those provisional names in favor of the spec-named variant
/// group, per the following mapping — this crate's own working
/// interpretation of which provisional sentinel corresponds to which named
/// code, flagged per Guiding Principle 5 pending reconciliation against the
/// specification's actual behavior (never its prose):
///
/// - `TimeSyncUnsupported` (`avtp`: TSCF header requires server
///   time-sync support the server doesn't have) → `UnsupportedCmd`. The
///   requested feature is not one this RC Server supports.
/// - `RegisterUnreachable` (`lifecycle`/`ep0`: register category not
///   reachable in the RC Server's current lifecycle state) and
///   `RootClientRequired` (`ep0`: EP0 write attempted by a non-root-client
///   stream) both → `UnauthorizedAccess`. Both represent the same shape of
///   failure — the requesting context (lifecycle state, or requesting
///   stream identity) does not authorize the attempted access — just
///   gated on two different axes, so both collapse onto the same code
///   rather than inventing a code-level distinction the checklist's own
///   eleven names do not draw.
/// - `RegisterLocked` (`lifecycle`/`ep0`: register category reachable but
///   write-locked) → `LockedMemAccess`. A direct name correspondence.
/// - `HwCfgInconsistent`/`RcpCfgInconsistent` (`lifecycle`:
///   `HW_CFG_INCONSISTENT`/`RCP_CFG_INCONSISTENT` transition-guard
///   rejections) and `EndpointTypeMismatch` (`regmap`: functional
///   config's `EndpointType` does not match the owning endpoint's
///   `ep_type`) all three → `InvalidParameter`. Each represents caller-
///   supplied configuration data failing a consistency/shape check, which
///   this crate reads as the same "invalid parameter" failure mode at
///   three different call sites rather than three distinct spec codes —
///   the checklist's own list has no more specific code for any of them.
/// - `InvalidLifecycleTransition` (`lifecycle`: `(from, to)` pair outside
///   the three implemented transitions) → `RequestRejected`. The request
///   names a structurally undefined operation for this RC Server, as
///   opposed to `UnsupportedCmd`'s "this feature isn't supported at all" —
///   the distinction this crate draws between the two is that
///   `UnsupportedCmd` is capability-based (would fail identically no
///   matter the RC Server's current state) while `RequestRejected` here is
///   state/shape-based (fails because of *which* transition was named, not
///   because transitions in general are unsupported).
/// - `EndpointAlreadyRegistered` (`addressing`: `(stream_id, byte_bus_id)`
///   pair already registered) and `EchoBackMismatch` (`acf`: response
///   `byte_bus_id` does not match the echo-back rule) both → `EpError`.
///   Both are endpoint-addressing-level failures with no more specific
///   named code in the checklist's list; `EpError` is this crate's reading
///   of the intentionally generic catch-all the checklist provides for
///   exactly that case, distinct from the specific `EpNotFound`.
///
/// `SequencerNotKnown`, `RequestCanceled`, `RequestNotFound`, `EpNotFound`,
/// and `ReqStorageOvfl` are added per the checklist's own naming but are
/// not yet constructed anywhere in this crate — no sequencer-lookup,
/// cancelable-request, or endpoint-lookup path exists yet to return them
/// from. They are reserved for the later milestones (sequencer state,
/// conditional/safety requests, concrete endpoint dispatch) that introduce
/// those concepts, matching this same milestone's own practice of adding a
/// named placeholder ahead of the code that will use it (e.g.
/// `RegisterCategory` ahead of the concrete Register Map).
///
/// ## Chained-request error codes (`ROADMAP.md` Milestone 5, "Chained")
///
/// [`ChainAborted`](Self::ChainAborted) and [`ChainError`](Self::ChainError)
/// are the first of the "timing- and CRC-specific codes wired in by later
/// milestones" the section above defers, to actually land. Per Guiding
/// Principle 5, [`crate::request`] — the module that names both, and
/// consumes [`ChainAborted`](Self::ChainAborted) from
/// [`crate::request::check_chain_continuation`] — carries this pair's full
/// provenance note (including why neither collapses onto
/// [`RequestRejected`](Self::RequestRejected) or any other of the eleven
/// codes above, and this crate's working interpretation of what
/// distinguishes the two) rather than duplicating it here; see that
/// module's doc comment "Provenance note: `CHAIN_ABORTED`/`CHAIN_ERROR` as
/// new variants, and the distinction between them". Both follow this
/// enum's own `"rcp/error: <CODE> — ..."` message convention but are kept
/// in their own enum section below, separate from the eleven-member "TC18
/// RCP spec error codes" group [`is_tc18_error_code`](Self::is_tc18_error_code)
/// queries — that predicate is scoped specifically to the Milestone 2
/// "Error Model" item's own eleven named codes, and is left unchanged by
/// this addition rather than silently widened to a twelve-or-more-member
/// group the checklist text for that item never named.
///
/// ## `CRC_ERROR` error code (`ROADMAP.md` Milestone 6, "`CRC_ERROR` error
/// path")
///
/// [`CrcError`](Self::CrcError) is the second of the "timing- and
/// CRC-specific codes wired in by later milestones" the "TC18 spec error
/// codes" section above defers, and the first CRC-specific one of the two
/// to actually land ([`ChainAborted`](Self::ChainAborted)/
/// [`ChainError`](Self::ChainError) above are both chain/timing-specific,
/// not CRC-specific). [`crate::request::check_rx_enforce_e2e`] constructs
/// it on a TC18 safe-point CRC-32 mismatch, replacing that function's
/// earlier, explicitly-provisional reuse of `CrcMismatch` — the legacy
/// CRC-16 sentinel [`crate::e2e`]'s own `wrap`/`unwrap` path used to
/// return. `CrcMismatch` (along with `Replay`) has since been retired
/// entirely by Milestone 9's `e2e` REPLACE cutover, the same way
/// `BadMagic`/`BadVersion` were retired by the `wire` REPLACE cutover
/// immediately before it — see this enum's "Wire / E2E errors" section
/// below. See [`crate::request`]'s own doc comment "Provenance note:
/// `CrcError` as a new variant, distinct from the legacy `CrcMismatch`
/// sentinel" for the full reasoning behind adding a new variant here
/// rather than collapsing onto `CrcMismatch` or any of the eleven TC18
/// codes above. Per the same pattern as
/// [`ChainAborted`](Self::ChainAborted)/[`ChainError`](Self::ChainError),
/// [`CrcError`](Self::CrcError) is kept out of the eleven-member
/// [`is_tc18_error_code`](Self::is_tc18_error_code) group.
///
/// ## Stability (`ROADMAP.md` Milestone 10, "Public API stability
/// guarantees")
///
/// `#[non_exhaustive]`: every one of this milestone's predecessors added
/// new variants here (`ChainAborted`/`ChainError` in Milestone 5,
/// `CrcError` in Milestone 6), and several named-but-unconstructed
/// variants (`SequencerNotKnown`, `RequestCanceled`, `RequestNotFound`,
/// `EpNotFound`, `ReqStorageOvfl`) are already reserved for later call
/// sites — this enum is a live growth surface, not a closed set. Matching
/// on it from outside this crate MUST include a wildcard arm; see
/// `docs/SEMVER.md`.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum RcpError {
    // ── Mandatory RELAY sentinels ─────────────────────────────────────────
    #[error("rcp: controller closed")]
    Closed,

    #[error("rcp: not connected")]
    NotConnected,

    #[error("rcp: command timeout")]
    Timeout,

    #[error("rcp: payload too large")]
    PayloadTooLarge,

    // ── General-purpose sentinels ────────────────────────────────────────
    // These four originate from this crate's pre-Milestone-10
    // `Zone`/`Controller`/`Registry` API (removed outright by Milestone
    // 10's core-surface cutover, `ROADMAP.md` — no compatibility shim was
    // kept). They are retained here, unchanged, as general-purpose
    // `RcpError` variants: `capi`, `authz`, `federation`, and other
    // still-live modules construct and match on them for meanings that
    // have nothing to do with the removed `Zone` type (e.g. `NotFound` for
    // "no such federation peer", `Busy` for a full rate-limit token
    // bucket). Renaming them is out of scope for this item — see
    // `docs/SEMVER.md` for this crate's stability commitment around them
    // going forward.
    #[error("rcp: zone not found")]
    NotFound,

    #[error("rcp: zone already registered")]
    AlreadyExists,

    #[error("rcp: zone controller busy")]
    Busy,

    #[error("rcp: zone mismatch")]
    ZoneMismatch,

    // ── Wire / E2E errors ────────────────────────────────────────────────
    // `CrcMismatch`/`Replay` were likewise legacy 16-byte-frame-specific
    // (see `e2e`'s own REPLACE disposition in `ROADMAP.md`'s satellite
    // table) and were kept unchanged pending that item — this comment
    // previously said so. That item has now landed: `src/e2e.rs`'s
    // `wrap`/`unwrap` (the CRC-16 frame `CrcMismatch` reported a mismatch
    // for) and `ReplayGuard` (`Replay`'s only constructor) are deleted
    // outright, and no other module ever constructed or matched on either
    // variant, so — mirroring `BadMagic`/`BadVersion`'s own retirement by
    // the immediately preceding `wire` REPLACE cutover — both `CrcMismatch`
    // and `Replay` are removed here too, rather than left as inert,
    // never-constructed sentinels. `ShortFrame` is not legacy-only —
    // every TC18 AVTPDU/ACF decoder added in Milestone 1 (`avtp`, `acf`)
    // and the Register Map config-table decoders added earlier in this
    // milestone (`regmap`) also return it for undersized input, so it
    // stays as a general-purpose, non-spec-code sentinel rather than being
    // folded into the TC18 error-code group below.
    #[error("rcp/wire: frame too short")]
    ShortFrame,

    // ── TC18 RCP spec error codes ───────────────────────────────────────
    // See this enum's own doc comment for the full provenance/mapping
    // note. Message text follows this item's own `"rcp/error: <CODE> —
    // ..."` convention rather than the per-module prefixes above, since
    // these variants are shared across every module that can construct
    // them rather than owned by any one of them.
    #[error(
        "rcp/error: UNSUPPORTED_CMD — command or protocol feature not supported by this RC Server"
    )]
    UnsupportedCmd,

    #[error(
        "rcp/error: SEQUENCER_NOT_KNOWN — referenced sequencer is not known to this RC Server"
    )]
    SequencerNotKnown,

    #[error("rcp/error: UNAUTHORIZED_ACCESS — access not authorized by the RC Server's current lifecycle state or the requesting stream's privileges")]
    UnauthorizedAccess,

    #[error(
        "rcp/error: LOCKED_MEM_ACCESS — target register is reachable but locked against writes"
    )]
    LockedMemAccess,

    #[error("rcp/error: REQUEST_CANCELED — request was canceled before completion")]
    RequestCanceled,

    #[error("rcp/error: REQUEST_NOT_FOUND — referenced request id is not known to this RC Server")]
    RequestNotFound,

    #[error("rcp/error: EP_ERROR — endpoint-level error")]
    EpError,

    #[error("rcp/error: EP_NOT_FOUND — referenced endpoint is not known to this RC Server")]
    EpNotFound,

    #[error("rcp/error: REQ_STORAGE_OVFL — request storage capacity exceeded")]
    ReqStorageOvfl,

    #[error("rcp/error: REQUEST_REJECTED — request rejected")]
    RequestRejected,

    #[error("rcp/error: INVALID_PARAMETER — one or more supplied parameter values is invalid")]
    InvalidParameter,

    // ── Chained-request error codes (ROADMAP.md Milestone 5, "Chained") ────
    // See this enum's own doc comment "Chained-request error codes" section
    // and crate::request's "Provenance note: CHAIN_ABORTED/CHAIN_ERROR as
    // new variants, and the distinction between them" for the full
    // provenance note behind adding these as new variants rather than
    // collapsing them onto RequestRejected or another of the eleven codes
    // above, and this crate's working interpretation of what distinguishes
    // the two from each other.
    #[error("rcp/error: CHAIN_ABORTED — chained request link skipped because a preceding link in the same chain errored")]
    ChainAborted,

    #[error("rcp/error: CHAIN_ERROR — chained request link's own execution failed")]
    ChainError,

    // ── CRC_ERROR error code (ROADMAP.md Milestone 6, "CRC_ERROR error
    //    path") ─────────────────────────────────────────────────────────
    // See this enum's own doc comment "CRC_ERROR error code" section and
    // crate::request's "Provenance note: CrcError as a new variant,
    // distinct from the legacy CrcMismatch sentinel" for the full
    // provenance note behind adding this as a new variant rather than
    // continuing check_rx_enforce_e2e's earlier reuse of CrcMismatch, and
    // for why this stays separate from the eleven-member "TC18 RCP spec
    // error codes" group.
    // fusa:req REQ-CRC-011
    #[error("rcp/error: CRC_ERROR — end-to-end CRC-32 safe-point verification failed")]
    CrcError,

    // ── General errors ───────────────────────────────────────────────────
    #[error("rcp: invalid size")]
    InvalidSize,

    #[error("rcp: {0}")]
    Other(String),
}

impl RcpError {
    // ── RELAY sentinel membership queries ─────────────────────────────────

    /// True for the `Closed` sentinel (wraps `relay::ErrClosed`).
    // fusa:req REQ-ERR-007
    // fusa:req REQ-ERR-014
    pub fn is_relay_closed(&self) -> bool {
        matches!(self, Self::Closed)
    }

    /// True for `NotConnected`, `NotFound`, and `ZoneMismatch`
    /// (all wrap `relay::ErrNotConnected`).
    // fusa:req REQ-ERR-008
    // fusa:req REQ-ERR-015
    // fusa:req REQ-ERR-018
    // fusa:req REQ-ERR-021
    pub fn is_relay_not_connected(&self) -> bool {
        matches!(
            self,
            Self::NotConnected | Self::NotFound | Self::ZoneMismatch
        )
    }

    /// True for `Timeout` and `Busy` (both wrap `relay::ErrTimeout`).
    // fusa:req REQ-ERR-010
    // fusa:req REQ-ERR-016
    // fusa:req REQ-ERR-020
    pub fn is_relay_timeout(&self) -> bool {
        matches!(self, Self::Timeout | Self::Busy)
    }

    /// True for the `PayloadTooLarge` sentinel.
    // fusa:req REQ-ERR-013
    // fusa:req REQ-ERR-017
    pub fn is_relay_payload_too_large(&self) -> bool {
        matches!(self, Self::PayloadTooLarge)
    }

    /// True for `AlreadyExists` (standalone per RELAY spec §5.4).
    // fusa:req REQ-ERR-009
    // fusa:req REQ-ERR-019
    pub fn is_already_exists(&self) -> bool {
        matches!(self, Self::AlreadyExists)
    }

    /// True for the `ZoneMismatch` sentinel.
    // fusa:req REQ-ERR-011
    pub fn is_zone_mismatch(&self) -> bool {
        matches!(self, Self::ZoneMismatch)
    }

    /// True for any of the eleven TC18 RCP spec error codes this crate
    /// added per `ROADMAP.md` Milestone 2's "Error Model" checklist item
    /// (`UnsupportedCmd` through `InvalidParameter`), as opposed to the
    /// RELAY-sentinel, general-purpose, or wire/E2E variants above. See
    /// this enum's own doc comment for the full list and provenance/
    /// mapping note.
    // fusa:req REQ-ERRM-012
    pub fn is_tc18_error_code(&self) -> bool {
        matches!(
            self,
            Self::UnsupportedCmd
                | Self::SequencerNotKnown
                | Self::UnauthorizedAccess
                | Self::LockedMemAccess
                | Self::RequestCanceled
                | Self::RequestNotFound
                | Self::EpError
                | Self::EpNotFound
                | Self::ReqStorageOvfl
                | Self::RequestRejected
                | Self::InvalidParameter
        )
    }
}

// ── Loan ──────────────────────────────────────────────────────────────────────

/// A borrowed, pre-allocated payload buffer, returned to its owning pool
/// (via the `release` closure) on drop or explicit [`Loan::return_loan`].
///
/// Used by [`loan::LoanPool`] for zero-copy endpoint
/// writes; carries no dependency of its own on any particular endpoint or
/// dispatch model.
pub struct Loan {
    pub payload: Vec<u8>,
    pub(crate) release: Option<Box<dyn FnOnce(Vec<u8>) + Send>>,
}

impl Loan {
    pub fn new(payload: Vec<u8>, release: impl FnOnce(Vec<u8>) + Send + 'static) -> Self {
        Loan {
            payload,
            release: Some(Box::new(release)),
        }
    }

    /// Return the buffer to the pool without sending.
    pub fn return_loan(mut self) {
        if let Some(f) = self.release.take() {
            f(std::mem::take(&mut self.payload));
        }
    }
}

impl Drop for Loan {
    fn drop(&mut self) {
        if let Some(f) = self.release.take() {
            f(std::mem::take(&mut self.payload));
        }
    }
}

impl fmt::Debug for Loan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Loan")
            .field("payload_len", &self.payload.len())
            .finish()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    // ── Error sentinels ───────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-ERR-001
    fn err_closed_is_distinct() {
        // Non-nil equivalent: it's a valid discriminant value
        let e = RcpError::Closed;
        assert!(e.is_relay_closed());
    }

    #[test]
    // fusa:test REQ-ERR-002
    fn err_not_found_is_distinct() {
        let e = RcpError::NotFound;
        assert!(e.is_relay_not_connected());
    }

    #[test]
    // fusa:test REQ-ERR-003
    fn err_already_exists_is_distinct() {
        let e = RcpError::AlreadyExists;
        assert!(e.is_already_exists());
    }

    #[test]
    // fusa:test REQ-ERR-004
    fn err_timeout_is_distinct() {
        let e = RcpError::Timeout;
        assert!(e.is_relay_timeout());
    }

    #[test]
    // fusa:test REQ-ERR-005
    fn err_busy_is_distinct() {
        let e = RcpError::Busy;
        assert!(e.is_relay_timeout());
    }

    #[test]
    // fusa:test REQ-ERR-006
    fn all_sentinels_are_mutually_distinct() {
        let sentinels = [
            RcpError::Closed,
            RcpError::NotFound,
            RcpError::AlreadyExists,
            RcpError::Timeout,
            RcpError::Busy,
        ];
        for i in 0..sentinels.len() {
            for j in (i + 1)..sentinels.len() {
                assert_ne!(sentinels[i], sentinels[j], "sentinels must be distinct");
            }
        }
    }

    #[test]
    // fusa:test REQ-ERR-007
    // fusa:test REQ-ERR-014
    fn err_closed_is_relay_closed() {
        assert!(RcpError::Closed.is_relay_closed());
        assert!(!RcpError::Timeout.is_relay_closed());
        assert!(!RcpError::NotFound.is_relay_closed());
    }

    #[test]
    // fusa:test REQ-ERR-008
    // fusa:test REQ-ERR-018
    // fusa:test REQ-ERR-021
    fn err_not_found_and_zone_mismatch_are_relay_not_connected() {
        assert!(RcpError::NotConnected.is_relay_not_connected());
        assert!(RcpError::NotFound.is_relay_not_connected());
        assert!(RcpError::ZoneMismatch.is_relay_not_connected());
        assert!(!RcpError::Closed.is_relay_not_connected());
        assert!(!RcpError::Timeout.is_relay_not_connected());
    }

    #[test]
    // fusa:test REQ-ERR-009
    // fusa:test REQ-ERR-019
    fn err_already_exists_is_standalone() {
        assert!(RcpError::AlreadyExists.is_already_exists());
        assert!(!RcpError::AlreadyExists.is_relay_closed());
        assert!(!RcpError::AlreadyExists.is_relay_timeout());
        assert!(!RcpError::AlreadyExists.is_relay_not_connected());
    }

    #[test]
    // fusa:test REQ-ERR-010
    // fusa:test REQ-ERR-020
    fn err_busy_wraps_timeout() {
        assert!(RcpError::Busy.is_relay_timeout());
        assert!(RcpError::Timeout.is_relay_timeout());
        assert!(!RcpError::Busy.is_relay_closed());
    }

    #[test]
    // fusa:test REQ-ERR-011
    fn err_zone_mismatch_is_distinct() {
        let e = RcpError::ZoneMismatch;
        assert!(e.is_zone_mismatch());
        assert!(e.is_relay_not_connected());
        assert!(!e.is_relay_closed());
        assert!(!e.is_relay_timeout());
        assert!(!e.is_already_exists());
    }

    #[test]
    // fusa:test REQ-ERR-012
    // fusa:test REQ-ERR-015
    fn err_not_connected_is_relay_not_connected() {
        assert!(RcpError::NotConnected.is_relay_not_connected());
    }

    #[test]
    // fusa:test REQ-ERR-013
    // fusa:test REQ-ERR-017
    fn err_payload_too_large_is_relay_payload_too_large() {
        assert!(RcpError::PayloadTooLarge.is_relay_payload_too_large());
        assert!(!RcpError::Closed.is_relay_payload_too_large());
    }

    #[test]
    // fusa:test REQ-ERR-016
    fn err_timeout_is_relay_timeout() {
        assert!(RcpError::Timeout.is_relay_timeout());
    }

    // ── TC18 RCP spec error codes (Milestone 2 "Error Model") ────────────────

    #[test]
    // fusa:test REQ-ERRM-001
    fn err_unsupported_cmd_is_tc18_error_code() {
        assert!(RcpError::UnsupportedCmd.is_tc18_error_code());
    }

    #[test]
    // fusa:test REQ-ERRM-002
    fn err_sequencer_not_known_is_tc18_error_code() {
        assert!(RcpError::SequencerNotKnown.is_tc18_error_code());
    }

    #[test]
    // fusa:test REQ-ERRM-003
    fn err_unauthorized_access_is_tc18_error_code() {
        assert!(RcpError::UnauthorizedAccess.is_tc18_error_code());
    }

    #[test]
    // fusa:test REQ-ERRM-004
    fn err_locked_mem_access_is_tc18_error_code() {
        assert!(RcpError::LockedMemAccess.is_tc18_error_code());
    }

    #[test]
    // fusa:test REQ-ERRM-005
    fn err_request_canceled_is_tc18_error_code() {
        assert!(RcpError::RequestCanceled.is_tc18_error_code());
    }

    #[test]
    // fusa:test REQ-ERRM-006
    fn err_request_not_found_is_tc18_error_code() {
        assert!(RcpError::RequestNotFound.is_tc18_error_code());
    }

    #[test]
    // fusa:test REQ-ERRM-007
    fn err_ep_error_is_tc18_error_code() {
        assert!(RcpError::EpError.is_tc18_error_code());
    }

    #[test]
    // fusa:test REQ-ERRM-008
    fn err_ep_not_found_is_tc18_error_code() {
        assert!(RcpError::EpNotFound.is_tc18_error_code());
    }

    #[test]
    // fusa:test REQ-ERRM-009
    fn err_req_storage_ovfl_is_tc18_error_code() {
        assert!(RcpError::ReqStorageOvfl.is_tc18_error_code());
    }

    #[test]
    // fusa:test REQ-ERRM-010
    fn err_request_rejected_is_tc18_error_code() {
        assert!(RcpError::RequestRejected.is_tc18_error_code());
    }

    #[test]
    // fusa:test REQ-ERRM-011
    fn err_invalid_parameter_is_tc18_error_code() {
        assert!(RcpError::InvalidParameter.is_tc18_error_code());
    }

    #[test]
    // fusa:test REQ-ERRM-012
    fn tc18_error_codes_are_mutually_distinct_and_exclusive() {
        let codes = [
            RcpError::UnsupportedCmd,
            RcpError::SequencerNotKnown,
            RcpError::UnauthorizedAccess,
            RcpError::LockedMemAccess,
            RcpError::RequestCanceled,
            RcpError::RequestNotFound,
            RcpError::EpError,
            RcpError::EpNotFound,
            RcpError::ReqStorageOvfl,
            RcpError::RequestRejected,
            RcpError::InvalidParameter,
        ];
        for i in 0..codes.len() {
            assert!(codes[i].is_tc18_error_code());
            for j in (i + 1)..codes.len() {
                assert_ne!(codes[i], codes[j], "TC18 error codes must be distinct");
            }
        }
        // None of the legacy/RELAY/wire sentinels are TC18 error codes.
        assert!(!RcpError::Closed.is_tc18_error_code());
        assert!(!RcpError::NotFound.is_tc18_error_code());
        assert!(!RcpError::ShortFrame.is_tc18_error_code());
        assert!(!RcpError::InvalidSize.is_tc18_error_code());
        assert!(!RcpError::Other("x".into()).is_tc18_error_code());
    }

    #[test]
    // fusa:test REQ-ERRM-013
    fn tc18_error_code_messages_carry_spec_name() {
        assert!(RcpError::UnsupportedCmd
            .to_string()
            .contains("UNSUPPORTED_CMD"));
        assert!(RcpError::SequencerNotKnown
            .to_string()
            .contains("SEQUENCER_NOT_KNOWN"));
        assert!(RcpError::UnauthorizedAccess
            .to_string()
            .contains("UNAUTHORIZED_ACCESS"));
        assert!(RcpError::LockedMemAccess
            .to_string()
            .contains("LOCKED_MEM_ACCESS"));
        assert!(RcpError::RequestCanceled
            .to_string()
            .contains("REQUEST_CANCELED"));
        assert!(RcpError::RequestNotFound
            .to_string()
            .contains("REQUEST_NOT_FOUND"));
        assert!(RcpError::EpError.to_string().contains("EP_ERROR"));
        assert!(RcpError::EpNotFound.to_string().contains("EP_NOT_FOUND"));
        assert!(RcpError::ReqStorageOvfl
            .to_string()
            .contains("REQ_STORAGE_OVFL"));
        assert!(RcpError::RequestRejected
            .to_string()
            .contains("REQUEST_REJECTED"));
        assert!(RcpError::InvalidParameter
            .to_string()
            .contains("INVALID_PARAMETER"));
    }

    // ── Spec version ──────────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-SPEC-001
    fn spec_version_nonempty() {
        assert!(!SPEC_VERSION.is_empty());
    }

    #[test]
    // fusa:test REQ-SPEC-001
    fn relay_spec_version_is_exported_and_matches_spec_version() {
        assert!(!RELAY_SPEC_VERSION.is_empty());
        assert_eq!(RELAY_SPEC_VERSION, SPEC_VERSION);
    }
}
