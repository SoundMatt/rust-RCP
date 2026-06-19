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

pub mod adapt;
pub mod admin;
pub mod authz;
pub mod canbr;
pub mod capi;
pub mod certgap;
pub mod codegen;
pub mod config;
pub mod ddsbr;
pub mod deadline;
pub mod doipbr;
pub mod dyndata;
pub mod e2e;
pub mod faultinject;
pub mod federation;
pub mod firmware;
pub mod formal;
pub mod grpcbridge;
pub mod iso21434;
pub mod linbr;
pub mod loan;
pub mod mdns;
pub mod mock;
pub mod mqttbr;
pub mod observe;
pub mod powerstate;
pub mod prioqueue;
pub mod proxy;
pub mod ratelimit;
pub mod record;
pub mod redundancy;
pub mod restbridge;
pub mod shmem;
pub mod sim;
pub mod someip;
pub mod tlstransport;
pub mod tsn;
pub mod udp;
pub mod udsbr;
pub mod watchdog;
pub mod wire;
pub mod zonegroup;

use std::fmt;
use std::sync::{mpsc, Arc};
use std::time::Duration;

// ── Spec version ────────────────────────────────────────────────────────────

/// RELAY specification version this crate implements.
// fusa:req REQ-SPEC-001
pub const SPEC_VERSION: &str = "1.6";

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

// ── Structs ───────────────────────────────────────────────────────────────────

/// Control message dispatched to a zone controller.
///
/// A zero-value `Command` (all fields default) is a safe no-op:
/// `Zone::UNKNOWN`, `CommandType::NOOP`, `Priority::NORMAL`, payload `None`.
// fusa:req REQ-CMDSTRUCT-001
// fusa:req REQ-CMDSTRUCT-002
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Command {
    pub id: u32,
    pub zone: Zone,
    pub cmd_type: CommandType,
    pub priority: Priority,
    pub payload: Option<Vec<u8>>,
}

/// Acknowledgement returned by a zone controller.
///
/// A zero-value `Response` has `status == ResponseStatus::OK`.
// fusa:req REQ-RESP-003
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Response {
    pub command_id: u32,
    pub zone: Zone,
    pub status: ResponseStatus,
    pub payload: Option<Vec<u8>>,
}

/// Periodic telemetry update published by a zone controller.
// fusa:req REQ-STAT-001
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Status {
    pub zone: Zone,
    pub seq: u32,
    pub healthy: bool,
    pub payload: Option<Vec<u8>>,
}

// ── Error types ───────────────────────────────────────────────────────────────

/// All errors produced by this crate.
///
/// Sentinel relationships (mirroring RELAY spec §5 `errors.Is` chains):
/// - `NotFound`     → `is_not_connected()` (wraps `NotConnected`)
/// - `ZoneMismatch` → `is_not_connected()` (wraps `NotConnected`)
/// - `Busy`         → `is_timeout()` (wraps `Timeout`)
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

    // ── Protocol-specific sentinels ──────────────────────────────────────
    #[error("rcp: zone not found")]
    NotFound,

    #[error("rcp: zone already registered")]
    AlreadyExists,

    #[error("rcp: zone controller busy")]
    Busy,

    #[error("rcp: zone mismatch")]
    ZoneMismatch,

    // ── Wire / E2E errors ────────────────────────────────────────────────
    #[error("rcp/wire: frame too short")]
    ShortFrame,

    #[error("rcp/wire: bad magic bytes")]
    BadMagic,

    #[error("rcp/wire: unsupported protocol version")]
    BadVersion,

    #[error("rcp/e2e: CRC mismatch")]
    CrcMismatch,

    #[error("rcp/e2e: replayed sequence number")]
    Replay,

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
            f(self.payload);
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
    fn zone_default_is_unknown() {
        assert_eq!(Zone::default(), Zone::UNKNOWN);
    }
}
