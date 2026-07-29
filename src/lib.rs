// fusa:req REQ-ZONE-001
// fusa:req REQ-ZONE-002
// fusa:req REQ-ZONE-003
// fusa:req REQ-ZONE-004
// fusa:req REQ-ZONE-005
// fusa:req REQ-ZONE-006
// fusa:req REQ-ZONE-007
// fusa:req REQ-ZONE-008
// fusa:req REQ-PRI-001
// fusa:req REQ-PRI-002
// fusa:req REQ-PRI-003
// fusa:req REQ-CMD-001
// fusa:req REQ-CMD-002
// fusa:req REQ-CMD-003
// fusa:req REQ-CMD-004
// fusa:req REQ-CMD-005
// fusa:req REQ-CMD-006
// fusa:req REQ-STATUS-001
// fusa:req REQ-STATUS-002
// fusa:req REQ-STATUS-003
// fusa:req REQ-STATUS-004
// fusa:req REQ-STATUS-005
// fusa:req REQ-STATUS-006
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
// fusa:req REQ-CMDSTRUCT-001
// fusa:req REQ-CMDSTRUCT-002
// fusa:req REQ-RESP-001
// fusa:req REQ-RESP-002
// fusa:req REQ-RESP-003
// fusa:req REQ-STAT-001
// fusa:req REQ-STAT-002
// fusa:req REQ-STAT-003
// fusa:req REQ-STAT-004
// fusa:req REQ-STAT-005
// fusa:req REQ-SPEC-001
// fusa:req REQ-MSG-001
// fusa:req REQ-MSG-002

//! Remote Control Protocol (RCP) for automotive zonal architecture.
//!
//! A central HPC uses a [`Registry`] to discover zone controllers, dispatches
//! [`Command`]s to each [`Controller`], and receives [`Response`]s and periodic
//! [`Status`] telemetry in return.
//!
//! This crate implements RELAY specification version [`SPEC_VERSION`].

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
pub mod ddsbr;
pub mod deadline;
pub mod discovery;
pub mod doipbr;
pub mod dyndata;
pub mod e2e;
pub mod ep0;
pub mod evtgroup;
pub mod faultinject;
pub mod federation;
pub mod firmware;
pub mod formal;
pub mod fragment;
pub mod gpio;
pub mod grpcbridge;
pub mod i2c;
pub mod iseled;
pub mod iso21434;
pub mod lifecycle;
pub mod lin;
pub mod loan;
pub mod mdio;
pub mod mdns;
pub mod mock;
pub mod mqttbr;
pub mod observe;
pub mod powerstate;
pub mod prioqueue;
pub mod proxy;
pub mod pwm;
pub mod ratelimit;
pub mod record;
pub mod redundancy;
pub mod regmap;
pub mod relay;
pub mod request;
pub mod restbridge;
pub mod shmem;
pub mod sim;
pub mod someip;
pub mod spi;
pub mod timestamp;
pub mod tlstransport;
pub mod tsn;
pub mod uart;
pub mod udp;
pub mod udsbr;
pub mod wakeup;
pub mod watchdog;
pub mod zonegroup;

pub use adapt::{adapt, from_message, to_message};

use std::fmt;
use std::sync::{mpsc, Arc};
use std::time::Duration;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

// ── Spec version ────────────────────────────────────────────────────────────

/// RELAY specification version this crate implements.
// fusa:req REQ-SPEC-001
pub const SPEC_VERSION: &str = "1.11";

/// Alias for [`SPEC_VERSION`], exported from the crate root per RELAY spec
/// §18.3 ("`RELAY_SPEC_VERSION` MUST be exported from the crate root").
// fusa:req REQ-SPEC-001
pub const RELAY_SPEC_VERSION: &str = SPEC_VERSION;

// ── Zone ────────────────────────────────────────────────────────────────────

/// Physical zone identifier in the vehicle.
///
/// The inner `u8` value is stable and must remain fixed across versions.
// fusa:req REQ-ZONE-002
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Zone(pub u8);

impl Zone {
    pub const UNKNOWN: Zone = Zone(0);
    pub const FRONT_LEFT: Zone = Zone(1);
    pub const FRONT_RIGHT: Zone = Zone(2);
    pub const REAR_LEFT: Zone = Zone(3);
    pub const REAR_RIGHT: Zone = Zone(4);
    pub const CENTRAL: Zone = Zone(5);
}

impl fmt::Display for Zone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// Numeric serde per RELAY spec §15.5 (`Zone uint8` — a bare integer in JSON,
// not `#[derive(Serialize)]`'s default newtype-as-array encoding).
// fusa:req REQ-RELAY-010
impl Serialize for Zone {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(self.0)
    }
}

// fusa:req REQ-RELAY-010
impl<'de> Deserialize<'de> for Zone {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Zone(u8::deserialize(d)?))
    }
}

impl Zone {
    /// Canonical PascalCase name used as the RELAY message ID (spec §15.7.5).
    pub fn as_str(self) -> &'static str {
        match self {
            Zone::FRONT_LEFT => "FrontLeft",
            Zone::FRONT_RIGHT => "FrontRight",
            Zone::REAR_LEFT => "RearLeft",
            Zone::REAR_RIGHT => "RearRight",
            Zone::CENTRAL => "Central",
            _ => "Unknown",
        }
    }
}

/// Parse a zone from its canonical PascalCase name or legacy kebab-case alias.
///
/// Returns `Err(RcpError::NotFound)` for unrecognised strings.
// fusa:req REQ-MSG-001
// fusa:req REQ-MSG-002
pub fn zone_from_str(s: &str) -> Result<Zone, RcpError> {
    match s {
        "FrontLeft" | "front-left" => Ok(Zone::FRONT_LEFT),
        "FrontRight" | "front-right" => Ok(Zone::FRONT_RIGHT),
        "RearLeft" | "rear-left" => Ok(Zone::REAR_LEFT),
        "RearRight" | "rear-right" => Ok(Zone::REAR_RIGHT),
        "Central" | "central" => Ok(Zone::CENTRAL),
        _ => Err(RcpError::NotFound),
    }
}

// ── Priority ─────────────────────────────────────────────────────────────────

/// Command scheduling priority within a zone controller.
// fusa:req REQ-PRI-001
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Priority(pub u8);

impl Priority {
    pub const NORMAL: Priority = Priority(0);
    pub const HIGH: Priority = Priority(1);
    pub const CRITICAL: Priority = Priority(2);
}

impl fmt::Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            Priority::NORMAL => "normal",
            Priority::HIGH => "high",
            Priority::CRITICAL => "critical",
            _ => "unknown",
        };
        f.write_str(s)
    }
}

// fusa:req REQ-RELAY-010
impl Serialize for Priority {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(self.0)
    }
}

// fusa:req REQ-RELAY-010
impl<'de> Deserialize<'de> for Priority {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Priority(u8::deserialize(d)?))
    }
}

// ── CommandType ──────────────────────────────────────────────────────────────

/// Intent of a command dispatched to a zone controller.
// fusa:req REQ-CMD-001
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CommandType(pub u16);

impl CommandType {
    pub const NOOP: CommandType = CommandType(0);
    pub const SET: CommandType = CommandType(1);
    pub const GET: CommandType = CommandType(2);
    pub const RESET: CommandType = CommandType(3);
    pub const WATCHDOG: CommandType = CommandType(4);
    pub const SLEEP: CommandType = CommandType(5);
    pub const WAKE: CommandType = CommandType(6);
}

impl fmt::Display for CommandType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            CommandType::NOOP => "noop",
            CommandType::SET => "set",
            CommandType::GET => "get",
            CommandType::RESET => "reset",
            CommandType::WATCHDOG => "watchdog",
            CommandType::SLEEP => "sleep",
            CommandType::WAKE => "wake",
            _ => "unknown",
        };
        f.write_str(s)
    }
}

// fusa:req REQ-RELAY-010
impl Serialize for CommandType {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u16(self.0)
    }
}

// fusa:req REQ-RELAY-010
impl<'de> Deserialize<'de> for CommandType {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(CommandType(u16::deserialize(d)?))
    }
}

// ── ResponseStatus ────────────────────────────────────────────────────────────

/// Outcome of a command execution reported by a zone controller.
// fusa:req REQ-STATUS-002
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ResponseStatus(pub u8);

impl ResponseStatus {
    pub const OK: ResponseStatus = ResponseStatus(0);
    pub const ERROR: ResponseStatus = ResponseStatus(1);
    pub const TIMEOUT: ResponseStatus = ResponseStatus(2);
    pub const BUSY: ResponseStatus = ResponseStatus(3);
    pub const UNKNOWN: ResponseStatus = ResponseStatus(4);
}

impl fmt::Display for ResponseStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            ResponseStatus::OK => "OK",
            ResponseStatus::ERROR => "error",
            ResponseStatus::TIMEOUT => "timeout",
            ResponseStatus::BUSY => "busy",
            _ => "unknown",
        };
        f.write_str(s)
    }
}

// fusa:req REQ-RELAY-010
impl Serialize for ResponseStatus {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(self.0)
    }
}

// fusa:req REQ-RELAY-010
impl<'de> Deserialize<'de> for ResponseStatus {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(ResponseStatus(u8::deserialize(d)?))
    }
}

// ── Structs ───────────────────────────────────────────────────────────────────

/// Control message dispatched to a zone controller.
///
/// A zero-value `Command` (all fields default) is a safe no-op:
/// `Zone::UNKNOWN`, `CommandType::NOOP`, `Priority::NORMAL`, payload `None`.
// fusa:req REQ-CMDSTRUCT-001
// fusa:req REQ-CMDSTRUCT-002
// fusa:req REQ-RELAY-011
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Command {
    pub id: u32,
    pub zone: Zone,
    #[serde(rename = "type")]
    pub cmd_type: CommandType,
    pub priority: Priority,
    #[serde(
        default,
        skip_serializing_if = "base64_serde::opt::is_none",
        with = "base64_serde::opt"
    )]
    pub payload: Option<Vec<u8>>,
}

/// Acknowledgement returned by a zone controller.
///
/// A zero-value `Response` has `status == ResponseStatus::OK`.
// fusa:req REQ-RESP-003
// fusa:req REQ-RELAY-011
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub command_id: u32,
    pub zone: Zone,
    pub status: ResponseStatus,
    #[serde(
        default,
        skip_serializing_if = "base64_serde::opt::is_none",
        with = "base64_serde::opt"
    )]
    pub payload: Option<Vec<u8>>,
}

/// Periodic telemetry update published by a zone controller.
// fusa:req REQ-STAT-001
// fusa:req REQ-RELAY-011
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Status {
    pub zone: Zone,
    pub seq: u32,
    pub healthy: bool,
    #[serde(
        default,
        skip_serializing_if = "base64_serde::opt::is_none",
        with = "base64_serde::opt"
    )]
    pub payload: Option<Vec<u8>>,
}

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
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
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

    // ── Legacy Zone/Controller/Registry sentinels ──────────────────────────
    // Pending removal alongside the rest of the legacy `Zone`/`Controller`/
    // `Registry` surface (`ROADMAP.md` Milestone 9's satellite-package
    // migration and Milestone 10's core-surface cutover) — kept unchanged
    // by this item since dozens of still-live satellite packages (`mock`,
    // `capi`, and others) construct and match on them
    // today. `udp`'s own `wire` REPLACE cutover (Milestone 9) already
    // dropped its use of `ZoneMismatch` — see `src/udp.rs` — but the
    // variant itself stays for the packages that still construct it. Out
    // of scope for the "Error Model" checklist item, which names only the
    // TC18 spec's own error codes below.
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
    /// RELAY-sentinel, legacy Zone/Controller/Registry, or wire/E2E
    /// variants above. See this enum's own doc comment for the full list
    /// and provenance/mapping note.
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

// ── Subscription ─────────────────────────────────────────────────────────────

/// A live subscription to [`Status`] updates from a [`Controller`].
///
/// Dropping a `Subscription` unregisters it; subsequent publishes from the
/// controller will no longer attempt delivery to its channel.
pub struct Subscription {
    pub(crate) rx: mpsc::Receiver<Arc<Status>>,
}

impl Subscription {
    /// Block until the next [`Status`] arrives or the controller closes.
    pub fn recv(&self) -> Option<Arc<Status>> {
        self.rx.recv().ok()
    }

    /// Block for at most `timeout` waiting for the next [`Status`].
    pub fn recv_timeout(&self, timeout: Duration) -> Option<Arc<Status>> {
        self.rx.recv_timeout(timeout).ok()
    }

    /// Return the next [`Status`] if one is immediately available.
    pub fn try_recv(&self) -> Option<Arc<Status>> {
        self.rx.try_recv().ok()
    }
}

// ── Controller trait ──────────────────────────────────────────────────────────

/// Interface to a single zone controller endpoint.
// fusa:req REQ-CTRL-001
// fusa:req REQ-CTRL-003
// fusa:req REQ-CTRL-004
// fusa:req REQ-CTRL-005
// fusa:req REQ-CTRL-006
// fusa:req REQ-CTRL-007
// fusa:req REQ-CTRL-008
// fusa:req REQ-CTRL-009
// fusa:req REQ-CTRL-025
pub trait Controller: Send + Sync {
    /// Zone this controller manages.
    fn zone(&self) -> Zone;

    /// Dispatch a command and wait for the response.
    ///
    /// - Returns `Err(RcpError::Closed)` if already closed.
    /// - Returns `Err(RcpError::Timeout)` if `timeout` expires.
    /// - Returns `Err(RcpError::ZoneMismatch)` if `cmd.zone != self.zone()`.
    fn send(&self, cmd: &Command, timeout: Option<Duration>) -> Result<Response, RcpError>;

    /// Return a channel of periodic [`Status`] updates.
    ///
    /// The channel delivers updates until the `Subscription` is dropped or
    /// the controller is closed.
    fn subscribe(&self) -> Result<Subscription, RcpError>;

    /// Release all resources. Safe to call multiple times.
    fn close(&self) -> Result<(), RcpError>;
}

// ── LoaningController trait ───────────────────────────────────────────────────

/// A [`Controller`] that supports zero-copy payload loaning.
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

pub trait LoaningController: Controller {
    /// Obtain a zeroed buffer of `size` bytes.
    fn loan(&self, size: usize) -> Result<Loan, RcpError>;

    /// Send `cmd` using a previously loaned payload buffer.
    fn send_loaned(
        &self,
        loan: Loan,
        cmd: Command,
        timeout: Option<Duration>,
    ) -> Result<Response, RcpError>;
}

// ── Registry trait ────────────────────────────────────────────────────────────

/// Manages a collection of zone controllers.
// fusa:req REQ-REG-001
// fusa:req REQ-REG-002
// fusa:req REQ-REG-003
// fusa:req REQ-REG-004
// fusa:req REQ-REG-005
// fusa:req REQ-REG-006
// fusa:req REQ-REG-007
// fusa:req REQ-REG-008
// fusa:req REQ-REG-009
// fusa:req REQ-REG-010
// fusa:req REQ-REG-011
// fusa:req REQ-REG-012
// fusa:req REQ-REG-013
pub trait Registry: Send + Sync {
    /// Add a controller. Returns `Err(RcpError::AlreadyExists)` on duplicate zone.
    fn register(&self, ctrl: Arc<dyn Controller>) -> Result<(), RcpError>;

    /// Remove and close the controller for `zone`. Returns `Err(RcpError::NotFound)` if absent.
    fn deregister(&self, zone: Zone) -> Result<(), RcpError>;

    /// Retrieve the controller for `zone`.
    /// Returns `Err(RcpError::Closed)` if the registry is closed,
    /// `Err(RcpError::NotFound)` if the zone is not registered.
    fn lookup(&self, zone: Zone) -> Result<Arc<dyn Controller>, RcpError>;

    /// All currently registered controllers.
    fn controllers(&self) -> Vec<Arc<dyn Controller>>;

    /// Close all controllers and the registry. Safe to call multiple times.
    fn close(&self) -> Result<(), RcpError>;
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    // ── Zone constants ────────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-ZONE-002
    fn zone_unknown_is_zero() {
        assert_eq!(Zone::UNKNOWN.0, 0);
    }

    #[test]
    // fusa:test REQ-ZONE-003
    fn zone_front_left_is_one() {
        assert_eq!(Zone::FRONT_LEFT.0, 1);
    }

    #[test]
    // fusa:test REQ-ZONE-004
    fn zone_front_right_is_two() {
        assert_eq!(Zone::FRONT_RIGHT.0, 2);
    }

    #[test]
    // fusa:test REQ-ZONE-005
    fn zone_rear_left_is_three() {
        assert_eq!(Zone::REAR_LEFT.0, 3);
    }

    #[test]
    // fusa:test REQ-ZONE-006
    fn zone_rear_right_is_four() {
        assert_eq!(Zone::REAR_RIGHT.0, 4);
    }

    #[test]
    // fusa:test REQ-ZONE-007
    fn zone_central_is_five() {
        assert_eq!(Zone::CENTRAL.0, 5);
    }

    #[test]
    // fusa:test REQ-ZONE-008
    fn zone_constants_are_distinct() {
        let zones = [
            Zone::FRONT_LEFT,
            Zone::FRONT_RIGHT,
            Zone::REAR_LEFT,
            Zone::REAR_RIGHT,
            Zone::CENTRAL,
        ];
        for i in 0..zones.len() {
            for j in (i + 1)..zones.len() {
                assert_ne!(zones[i], zones[j], "duplicate zone value");
            }
        }
    }

    #[test]
    // fusa:test REQ-ZONE-001
    fn zone_string_unique_and_nonempty() {
        let zones = [
            Zone::UNKNOWN,
            Zone::FRONT_LEFT,
            Zone::FRONT_RIGHT,
            Zone::REAR_LEFT,
            Zone::REAR_RIGHT,
            Zone::CENTRAL,
        ];
        let mut seen = std::collections::HashSet::new();
        for z in &zones {
            let s = z.as_str();
            assert!(!s.is_empty(), "zone string must not be empty");
            assert!(seen.insert(s), "duplicate zone string: {s}");
        }
    }

    #[test]
    // fusa:test REQ-ZONE-001
    fn zone_display_matches_as_str() {
        assert_eq!(format!("{}", Zone::FRONT_LEFT), "FrontLeft");
        assert_eq!(format!("{}", Zone::CENTRAL), "Central");
    }

    // ── Priority constants ────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-PRI-001
    fn priority_normal_is_zero() {
        assert_eq!(Priority::NORMAL.0, 0);
    }

    #[test]
    // fusa:test REQ-PRI-002
    fn priority_high_greater_than_normal() {
        assert!(Priority::HIGH > Priority::NORMAL);
    }

    #[test]
    // fusa:test REQ-PRI-003
    fn priority_critical_greater_than_high() {
        assert!(Priority::CRITICAL > Priority::HIGH);
    }

    // ── CommandType constants ─────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-CMD-001
    fn cmd_noop_is_zero() {
        assert_eq!(CommandType::NOOP.0, 0);
    }

    #[test]
    // fusa:test REQ-CMD-002
    fn cmd_set_is_one() {
        assert_eq!(CommandType::SET.0, 1);
    }

    #[test]
    // fusa:test REQ-CMD-003
    fn cmd_get_is_two() {
        assert_eq!(CommandType::GET.0, 2);
    }

    #[test]
    // fusa:test REQ-CMD-004
    fn cmd_reset_is_three() {
        assert_eq!(CommandType::RESET.0, 3);
    }

    #[test]
    // fusa:test REQ-CMD-005
    fn cmd_watchdog_is_four() {
        assert_eq!(CommandType::WATCHDOG.0, 4);
    }

    #[test]
    // fusa:test REQ-CMD-006
    fn cmd_constants_are_distinct() {
        let types = [
            CommandType::NOOP,
            CommandType::SET,
            CommandType::GET,
            CommandType::RESET,
            CommandType::WATCHDOG,
            CommandType::SLEEP,
            CommandType::WAKE,
        ];
        for i in 0..types.len() {
            for j in (i + 1)..types.len() {
                assert_ne!(types[i], types[j], "duplicate command type value");
            }
        }
    }

    // ── ResponseStatus constants ──────────────────────────────────────────────

    #[test]
    // fusa:test REQ-STATUS-002
    fn status_ok_is_zero() {
        assert_eq!(ResponseStatus::OK.0, 0);
    }

    #[test]
    // fusa:test REQ-STATUS-003
    fn status_error_is_one() {
        assert_eq!(ResponseStatus::ERROR.0, 1);
    }

    #[test]
    // fusa:test REQ-STATUS-004
    fn status_timeout_is_two() {
        assert_eq!(ResponseStatus::TIMEOUT.0, 2);
    }

    #[test]
    // fusa:test REQ-STATUS-005
    fn status_busy_is_three() {
        assert_eq!(ResponseStatus::BUSY.0, 3);
    }

    #[test]
    // fusa:test REQ-STATUS-006
    fn status_constants_are_distinct() {
        let statuses = [
            ResponseStatus::OK,
            ResponseStatus::ERROR,
            ResponseStatus::TIMEOUT,
            ResponseStatus::BUSY,
            ResponseStatus::UNKNOWN,
        ];
        for i in 0..statuses.len() {
            for j in (i + 1)..statuses.len() {
                assert_ne!(statuses[i], statuses[j], "duplicate status value");
            }
        }
    }

    #[test]
    // fusa:test REQ-STATUS-001
    fn status_string_unique_and_nonempty() {
        let statuses = [
            ResponseStatus::OK,
            ResponseStatus::ERROR,
            ResponseStatus::TIMEOUT,
            ResponseStatus::BUSY,
            ResponseStatus::UNKNOWN,
        ];
        let mut seen = std::collections::HashSet::new();
        for s in &statuses {
            let txt = format!("{s}");
            assert!(!txt.is_empty());
            assert!(seen.insert(txt), "duplicate status string");
        }
    }

    // ── Zero-value struct safety ──────────────────────────────────────────────

    #[test]
    // fusa:test REQ-CMDSTRUCT-001
    fn zero_command_is_safe_noop() {
        let cmd = Command::default();
        assert_eq!(cmd.zone, Zone::UNKNOWN);
        assert_eq!(cmd.cmd_type, CommandType::NOOP);
        assert_eq!(cmd.priority, Priority::NORMAL);
        assert!(cmd.payload.is_none());
    }

    #[test]
    // fusa:test REQ-CMDSTRUCT-002
    fn command_payload_may_be_none() {
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            payload: None,
            ..Default::default()
        };
        assert!(cmd.payload.is_none());
    }

    #[test]
    // fusa:test REQ-RESP-003
    fn zero_response_has_status_ok() {
        let r = Response::default();
        assert_eq!(r.status, ResponseStatus::OK);
    }

    #[test]
    // fusa:test REQ-STAT-005
    fn status_payload_may_be_none() {
        let s = Status {
            zone: Zone::CENTRAL,
            seq: 1,
            healthy: true,
            payload: None,
        };
        assert!(s.payload.is_none());
    }

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

    // ── ZoneFromString ────────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-MSG-001
    fn zone_from_str_round_trip() {
        let zones = [
            Zone::FRONT_LEFT,
            Zone::FRONT_RIGHT,
            Zone::REAR_LEFT,
            Zone::REAR_RIGHT,
            Zone::CENTRAL,
        ];
        for z in zones {
            let s = z.as_str();
            let parsed = zone_from_str(s).expect("round-trip parse");
            assert_eq!(parsed, z);
        }
    }

    #[test]
    // fusa:test REQ-MSG-001
    fn zone_from_str_kebab_aliases() {
        assert_eq!(zone_from_str("front-left").unwrap(), Zone::FRONT_LEFT);
        assert_eq!(zone_from_str("front-right").unwrap(), Zone::FRONT_RIGHT);
        assert_eq!(zone_from_str("rear-left").unwrap(), Zone::REAR_LEFT);
        assert_eq!(zone_from_str("rear-right").unwrap(), Zone::REAR_RIGHT);
        assert_eq!(zone_from_str("central").unwrap(), Zone::CENTRAL);
    }

    #[test]
    // fusa:test REQ-MSG-002
    fn zone_from_str_unknown_returns_not_found() {
        let err = zone_from_str("bogus-zone").unwrap_err();
        assert_eq!(err, RcpError::NotFound);
        assert!(err.is_relay_not_connected());
    }

    // ── CmdSleep / CmdWake ────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-PWR-001
    fn cmd_sleep_and_wake_are_distinct() {
        assert_ne!(CommandType::SLEEP, CommandType::WAKE);
        assert_ne!(CommandType::SLEEP, CommandType::NOOP);
        assert_ne!(CommandType::SLEEP, CommandType::WATCHDOG);
        assert_ne!(CommandType::SLEEP, CommandType::RESET);
        assert_ne!(CommandType::WAKE, CommandType::NOOP);
        assert_ne!(CommandType::WAKE, CommandType::WATCHDOG);
        assert_ne!(CommandType::WAKE, CommandType::RESET);
    }

    // ── Default Zone is UNKNOWN ───────────────────────────────────────────────

    #[test]
    // fusa:test REQ-CMDSTRUCT-001
    fn zone_default_is_unknown() {
        assert_eq!(Zone::default(), Zone::UNKNOWN);
    }

    // ── RELAY serde (§18.3 / §15.5) ───────────────────────────────────────────

    #[test]
    // fusa:test REQ-RELAY-010
    fn zone_serializes_as_bare_integer() {
        let json = serde_json::to_string(&Zone::FRONT_LEFT).unwrap();
        assert_eq!(json, "1");
        let back: Zone = serde_json::from_str("1").unwrap();
        assert_eq!(back, Zone::FRONT_LEFT);
    }

    #[test]
    // fusa:test REQ-RELAY-010
    fn priority_cmdtype_responsestatus_serialize_as_bare_integers() {
        assert_eq!(serde_json::to_string(&Priority::HIGH).unwrap(), "1");
        assert_eq!(serde_json::to_string(&CommandType::RESET).unwrap(), "3");
        assert_eq!(serde_json::to_string(&ResponseStatus::ERROR).unwrap(), "1");
    }

    #[test]
    // fusa:test REQ-RELAY-011
    fn command_serializes_with_spec_field_names() {
        let cmd = Command {
            id: 7,
            zone: Zone::CENTRAL,
            cmd_type: CommandType::SET,
            priority: Priority::HIGH,
            payload: Some(vec![0xAA]),
        };
        let v: serde_json::Value = serde_json::to_value(&cmd).unwrap();
        assert_eq!(v["id"], 7);
        assert_eq!(v["zone"], 5);
        assert_eq!(v["type"], 1);
        assert_eq!(v["priority"], 1);
        assert_eq!(v["payload"], "qg==");
    }

    #[test]
    // fusa:test REQ-RELAY-011
    fn command_payload_omitted_when_none() {
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            ..Default::default()
        };
        let v: serde_json::Value = serde_json::to_value(&cmd).unwrap();
        assert!(v.get("payload").is_none());
    }

    #[test]
    // fusa:test REQ-RELAY-011
    fn response_and_status_round_trip_serde() {
        let resp = Response {
            command_id: 1,
            zone: Zone::REAR_LEFT,
            status: ResponseStatus::OK,
            payload: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: Response = serde_json::from_str(&json).unwrap();
        assert_eq!(back, resp);

        let st = Status {
            zone: Zone::REAR_RIGHT,
            seq: 4,
            healthy: true,
            payload: Some(vec![1]),
        };
        let json = serde_json::to_string(&st).unwrap();
        let back: Status = serde_json::from_str(&json).unwrap();
        assert_eq!(back, st);
    }

    #[test]
    // fusa:test REQ-SPEC-001
    fn relay_spec_version_is_exported_and_matches_spec_version() {
        assert!(!RELAY_SPEC_VERSION.is_empty());
        assert_eq!(RELAY_SPEC_VERSION, SPEC_VERSION);
    }
}
