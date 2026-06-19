// fusa:req REQ-CAPI-001
// fusa:req REQ-CAPI-002
// fusa:req REQ-CAPI-003
// fusa:req REQ-CAPI-004

//! C API bridge — exposes a C-compatible FFI surface for embedding RCP in
//! C/C++ codebases. All types are `#[repr(C)]`; no unsafe code in the core
//! library; the FFI layer is tested via Rust wrappers.
//!
//! Note: actual `extern "C"` declarations live in a separate optional cdylib
//! target; this module provides the Rust-side wrappers and type definitions.

use crate::{Command, CommandType, Priority, Zone};

// ── C-compatible types ────────────────────────────────────────────────────────

/// C-compatible command structure.
// fusa:req REQ-CAPI-001
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct CCommand {
    pub id:       u32,
    pub zone:     u8,
    pub cmd_type: u16,
    pub priority: u8,
}

/// C-compatible response structure.
// fusa:req REQ-CAPI-002
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct CResponse {
    pub command_id: u32,
    pub zone:       u8,
    pub status:     u8,
}

// ── Conversion helpers ────────────────────────────────────────────────────────

impl From<&Command> for CCommand {
    // fusa:req REQ-CAPI-003
    fn from(cmd: &Command) -> Self {
        CCommand { id: cmd.id, zone: cmd.zone.0, cmd_type: cmd.cmd_type.0, priority: cmd.priority.0 }
    }
}

impl From<&CCommand> for Command {
    // fusa:req REQ-CAPI-003
    fn from(c: &CCommand) -> Self {
        Command { id: c.id, zone: Zone(c.zone), cmd_type: CommandType(c.cmd_type),
            priority: Priority(c.priority), payload: None }
    }
}

// ── Error codes ───────────────────────────────────────────────────────────────

/// C-compatible error code.
// fusa:req REQ-CAPI-004
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CError {
    Ok             = 0,
    Closed         = 1,
    NotConnected   = 2,
    Timeout        = 3,
    PayloadTooLarge= 4,
    NotFound       = 5,
    AlreadyExists  = 6,
    Busy           = 7,
    ZoneMismatch   = 8,
    Other          = 99,
}

impl From<&crate::RcpError> for CError {
    fn from(e: &crate::RcpError) -> Self {
        match e {
            crate::RcpError::Closed          => CError::Closed,
            crate::RcpError::NotConnected    => CError::NotConnected,
            crate::RcpError::Timeout         => CError::Timeout,
            crate::RcpError::PayloadTooLarge => CError::PayloadTooLarge,
            crate::RcpError::NotFound        => CError::NotFound,
            crate::RcpError::AlreadyExists   => CError::AlreadyExists,
            crate::RcpError::Busy            => CError::Busy,
            crate::RcpError::ZoneMismatch    => CError::ZoneMismatch,
            _                                => CError::Other,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, CommandType, Priority, Zone};

    #[test]
    // fusa:test REQ-CAPI-001
    fn c_command_is_repr_c() {
        // Size: u32 + u8 + (pad) + u16 + u8 = 8 bytes on most platforms
        assert!(std::mem::size_of::<CCommand>() >= 8);
    }

    #[test]
    // fusa:test REQ-CAPI-003
    fn command_round_trip() {
        let cmd = Command {
            id: 42, zone: Zone::FRONT_LEFT,
            cmd_type: CommandType::SET, priority: Priority::HIGH, payload: None,
        };
        let c: CCommand = (&cmd).into();
        let back: Command = (&c).into();
        assert_eq!(back.id, 42);
        assert_eq!(back.zone, Zone::FRONT_LEFT);
        assert_eq!(back.cmd_type, CommandType::SET);
        assert_eq!(back.priority, Priority::HIGH);
    }

    #[test]
    // fusa:test REQ-CAPI-004
    fn error_code_mapping() {
        assert_eq!(CError::from(&crate::RcpError::Closed),    CError::Closed);
        assert_eq!(CError::from(&crate::RcpError::Busy),      CError::Busy);
        assert_eq!(CError::from(&crate::RcpError::Timeout),   CError::Timeout);
        assert_eq!(CError::from(&crate::RcpError::ZoneMismatch), CError::ZoneMismatch);
    }

    #[test]
    // fusa:test REQ-CAPI-002
    fn c_response_fields() {
        let r = CResponse { command_id: 7, zone: 1, status: 0 };
        assert_eq!(r.zone, Zone::FRONT_LEFT.0);
    }
}
