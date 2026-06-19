// fusa:req REQ-WIRE-001
// fusa:req REQ-WIRE-002
// fusa:req REQ-WIRE-003
// fusa:req REQ-WIRE-004
// fusa:req REQ-WIRE-005
// fusa:req REQ-WIRE-006
// fusa:req REQ-WIRE-007
// fusa:req REQ-WIRE-008
// fusa:req REQ-WIRE-009

//! Binary wire frame format shared by UDP and TLS transports.
//!
//! Frame layout:
//! ```text
//! [0]    Magic0  = 0x52 ('R')
//! [1]    Magic1  = 0x43 ('C')
//! [2]    ProtoVer = 0x01
//! [3]    MsgType  (Command=0x01, Response=0x02, Status=0x03, Subscribe=0x04, Unsubscribe=0x05)
//! [4]    Zone     (u8)
//! [5:7]  Type/Flags (u16 big-endian, interpretation depends on MsgType)
//! [7]    Priority/Status/Healthy (u8)
//! [8:12] ID/CommandID/Seq  (u32 big-endian)
//! [12:16] PayloadLen (u32 big-endian)
//! [16..] Payload (PayloadLen bytes)
//! ```

use crate::{Command, CommandType, Priority, RcpError, Response, ResponseStatus, Status, Zone};

// ── Constants ─────────────────────────────────────────────────────────────────

pub const MAGIC_BYTE_0: u8 = 0x52; // 'R'
pub const MAGIC_BYTE_1: u8 = 0x43; // 'C'
pub const PROTO_VER: u8 = 0x01;

pub const TYPE_COMMAND: u8 = 0x01;
pub const TYPE_RESPONSE: u8 = 0x02;
pub const TYPE_STATUS: u8 = 0x03;
pub const TYPE_SUBSCRIBE: u8 = 0x04;
pub const TYPE_UNSUBSCRIBE: u8 = 0x05;

pub const HEADER_LEN: usize = 16;

/// Maximum payload bytes per frame (UDP max datagram - header).
pub const MAX_PAYLOAD: usize = 65507 - HEADER_LEN;

// ── Header validation ─────────────────────────────────────────────────────────

/// Validate the 16-byte frame header. Returns `Err` on short, bad magic, or bad version.
// fusa:req REQ-WIRE-001
// fusa:req REQ-WIRE-002
// fusa:req REQ-WIRE-003
pub fn validate_header(b: &[u8]) -> Result<(), RcpError> {
    if b.len() < HEADER_LEN {
        return Err(RcpError::ShortFrame);
    }
    if b[0] != MAGIC_BYTE_0 || b[1] != MAGIC_BYTE_1 {
        return Err(RcpError::BadMagic);
    }
    if b[2] != PROTO_VER {
        return Err(RcpError::BadVersion);
    }
    Ok(())
}

// ── Encoding helpers ──────────────────────────────────────────────────────────

fn put_u16_be(buf: &mut [u8], offset: usize, v: u16) {
    buf[offset] = (v >> 8) as u8;
    buf[offset + 1] = v as u8;
}

fn put_u32_be(buf: &mut [u8], offset: usize, v: u32) {
    buf[offset] = (v >> 24) as u8;
    buf[offset + 1] = (v >> 16) as u8;
    buf[offset + 2] = (v >> 8) as u8;
    buf[offset + 3] = v as u8;
}

fn get_u16_be(b: &[u8], offset: usize) -> u16 {
    ((b[offset] as u16) << 8) | (b[offset + 1] as u16)
}

fn get_u32_be(b: &[u8], offset: usize) -> u32 {
    ((b[offset] as u32) << 24)
        | ((b[offset + 1] as u32) << 16)
        | ((b[offset + 2] as u32) << 8)
        | (b[offset + 3] as u32)
}

fn payload_slice(cmd_payload: &Option<Vec<u8>>) -> &[u8] {
    cmd_payload.as_deref().unwrap_or(&[])
}

// ── Command frame ─────────────────────────────────────────────────────────────

/// Encode a `Command` to a wire frame.
// fusa:req REQ-WIRE-004
pub fn encode_command(cmd: &Command) -> Vec<u8> {
    let pl = payload_slice(&cmd.payload);
    let mut buf = vec![0u8; HEADER_LEN + pl.len()];
    buf[0] = MAGIC_BYTE_0;
    buf[1] = MAGIC_BYTE_1;
    buf[2] = PROTO_VER;
    buf[3] = TYPE_COMMAND;
    buf[4] = cmd.zone.0;
    put_u16_be(&mut buf, 5, cmd.cmd_type.0);
    buf[7] = cmd.priority.0;
    put_u32_be(&mut buf, 8, cmd.id);
    put_u32_be(&mut buf, 12, pl.len() as u32);
    buf[HEADER_LEN..].copy_from_slice(pl);
    buf
}

/// Decode a `Command` from a wire frame.
// fusa:req REQ-WIRE-004
// fusa:req REQ-WIRE-007
// fusa:req REQ-WIRE-009
pub fn decode_command(b: &[u8]) -> Result<Command, RcpError> {
    validate_header(b)?;
    let body_len = get_u32_be(b, 12) as u64;
    if (b.len() as u64) < HEADER_LEN as u64 + body_len {
        return Err(RcpError::ShortFrame);
    }
    let payload = if body_len > 0 {
        let end = HEADER_LEN + body_len as usize;
        Some(b[HEADER_LEN..end].to_vec())
    } else {
        None
    };
    Ok(Command {
        zone: Zone(b[4]),
        cmd_type: CommandType(get_u16_be(b, 5)),
        priority: Priority(b[7]),
        id: get_u32_be(b, 8),
        payload,
    })
}

// ── Response frame ────────────────────────────────────────────────────────────

/// Encode a `Response` to a wire frame.
// fusa:req REQ-WIRE-005
pub fn encode_response(resp: &Response) -> Vec<u8> {
    let pl = payload_slice(&resp.payload);
    let mut buf = vec![0u8; HEADER_LEN + pl.len()];
    buf[0] = MAGIC_BYTE_0;
    buf[1] = MAGIC_BYTE_1;
    buf[2] = PROTO_VER;
    buf[3] = TYPE_RESPONSE;
    buf[4] = resp.zone.0;
    put_u16_be(&mut buf, 5, 0); // reserved
    buf[7] = resp.status.0;
    put_u32_be(&mut buf, 8, resp.command_id);
    put_u32_be(&mut buf, 12, pl.len() as u32);
    buf[HEADER_LEN..].copy_from_slice(pl);
    buf
}

/// Decode a `Response` from a wire frame.
// fusa:req REQ-WIRE-005
// fusa:req REQ-WIRE-007
// fusa:req REQ-WIRE-009
pub fn decode_response(b: &[u8]) -> Result<Response, RcpError> {
    validate_header(b)?;
    let body_len = get_u32_be(b, 12) as u64;
    if (b.len() as u64) < HEADER_LEN as u64 + body_len {
        return Err(RcpError::ShortFrame);
    }
    let payload = if body_len > 0 {
        Some(b[HEADER_LEN..HEADER_LEN + body_len as usize].to_vec())
    } else {
        None
    };
    Ok(Response {
        zone: Zone(b[4]),
        status: ResponseStatus(b[7]),
        command_id: get_u32_be(b, 8),
        payload,
    })
}

// ── Status frame ──────────────────────────────────────────────────────────────

/// Encode a `Status` to a wire frame.
// fusa:req REQ-WIRE-006
pub fn encode_status(st: &Status) -> Vec<u8> {
    let pl = payload_slice(&st.payload);
    let mut buf = vec![0u8; HEADER_LEN + pl.len()];
    buf[0] = MAGIC_BYTE_0;
    buf[1] = MAGIC_BYTE_1;
    buf[2] = PROTO_VER;
    buf[3] = TYPE_STATUS;
    buf[4] = st.zone.0;
    put_u16_be(&mut buf, 5, 0); // reserved
    buf[7] = if st.healthy { 1 } else { 0 };
    put_u32_be(&mut buf, 8, st.seq);
    put_u32_be(&mut buf, 12, pl.len() as u32);
    buf[HEADER_LEN..].copy_from_slice(pl);
    buf
}

/// Decode a `Status` from a wire frame.
// fusa:req REQ-WIRE-006
// fusa:req REQ-WIRE-007
// fusa:req REQ-WIRE-009
pub fn decode_status(b: &[u8]) -> Result<Status, RcpError> {
    validate_header(b)?;
    let body_len = get_u32_be(b, 12) as u64;
    if (b.len() as u64) < HEADER_LEN as u64 + body_len {
        return Err(RcpError::ShortFrame);
    }
    let payload = if body_len > 0 {
        Some(b[HEADER_LEN..HEADER_LEN + body_len as usize].to_vec())
    } else {
        None
    };
    Ok(Status {
        zone: Zone(b[4]),
        healthy: b[7] == 1,
        seq: get_u32_be(b, 8),
        payload,
    })
}

// ── Control frame ─────────────────────────────────────────────────────────────

/// Encode a control frame (Subscribe / Unsubscribe) with no payload.
// fusa:req REQ-WIRE-008
pub fn encode_control_frame(msg_type: u8, zone: Zone) -> Vec<u8> {
    let mut buf = vec![0u8; HEADER_LEN];
    buf[0] = MAGIC_BYTE_0;
    buf[1] = MAGIC_BYTE_1;
    buf[2] = PROTO_VER;
    buf[3] = msg_type;
    buf[4] = zone.0;
    buf
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    // ── validate_header ───────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-WIRE-001
    fn validate_header_rejects_short_frame() {
        let buf = [0u8; 10];
        assert_eq!(validate_header(&buf), Err(RcpError::ShortFrame));
    }

    #[test]
    // fusa:test REQ-WIRE-002
    fn validate_header_rejects_bad_magic() {
        let mut buf = [0u8; 16];
        buf[0] = 0xFF;
        buf[1] = 0xFF;
        buf[2] = PROTO_VER;
        assert_eq!(validate_header(&buf), Err(RcpError::BadMagic));
    }

    #[test]
    // fusa:test REQ-WIRE-003
    fn validate_header_rejects_bad_version() {
        let mut buf = [0u8; 16];
        buf[0] = MAGIC_BYTE_0;
        buf[1] = MAGIC_BYTE_1;
        buf[2] = 0xFF;
        assert_eq!(validate_header(&buf), Err(RcpError::BadVersion));
    }

    #[test]
    fn validate_header_accepts_valid() {
        let mut buf = [0u8; 16];
        buf[0] = MAGIC_BYTE_0;
        buf[1] = MAGIC_BYTE_1;
        buf[2] = PROTO_VER;
        assert!(validate_header(&buf).is_ok());
    }

    // ── Command round-trip ────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-WIRE-004
    fn command_round_trip_with_payload() {
        let cmd = Command {
            id: 0x0102_0304,
            zone: Zone::FRONT_LEFT,
            cmd_type: CommandType::SET,
            priority: Priority::HIGH,
            payload: Some(vec![0xAA, 0xBB, 0xCC]),
        };
        let frame = encode_command(&cmd);
        let decoded = decode_command(&frame).unwrap();
        assert_eq!(decoded.id, cmd.id);
        assert_eq!(decoded.zone, cmd.zone);
        assert_eq!(decoded.cmd_type, cmd.cmd_type);
        assert_eq!(decoded.priority, cmd.priority);
        assert_eq!(decoded.payload, cmd.payload);
    }

    #[test]
    // fusa:test REQ-WIRE-004
    fn command_round_trip_empty_payload() {
        let cmd = Command {
            id: 1,
            zone: Zone::CENTRAL,
            cmd_type: CommandType::NOOP,
            priority: Priority::NORMAL,
            payload: None,
        };
        let frame = encode_command(&cmd);
        let decoded = decode_command(&frame).unwrap();
        assert!(decoded.payload.is_none());
    }

    #[test]
    // fusa:test REQ-WIRE-007
    fn decode_command_rejects_truncated_body() {
        let cmd = Command {
            payload: Some(vec![1, 2, 3, 4, 5]),
            ..Default::default()
        };
        let mut frame = encode_command(&cmd);
        frame.truncate(HEADER_LEN + 2); // Truncate payload
        assert_eq!(decode_command(&frame), Err(RcpError::ShortFrame));
    }

    // ── Response round-trip ───────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-WIRE-005
    fn response_round_trip_with_payload() {
        let resp = Response {
            command_id: 0xDEAD_BEEF,
            zone: Zone::REAR_LEFT,
            status: ResponseStatus::ERROR,
            payload: Some(vec![1, 2]),
        };
        let frame = encode_response(&resp);
        let decoded = decode_response(&frame).unwrap();
        assert_eq!(decoded.command_id, resp.command_id);
        assert_eq!(decoded.zone, resp.zone);
        assert_eq!(decoded.status, resp.status);
        assert_eq!(decoded.payload, resp.payload);
    }

    #[test]
    // fusa:test REQ-WIRE-005
    fn response_round_trip_empty_payload() {
        let resp = Response {
            command_id: 1,
            zone: Zone::CENTRAL,
            status: ResponseStatus::OK,
            payload: None,
        };
        let frame = encode_response(&resp);
        let decoded = decode_response(&frame).unwrap();
        assert!(decoded.payload.is_none());
    }

    #[test]
    // fusa:test REQ-WIRE-007
    fn decode_response_rejects_truncated_body() {
        let resp = Response {
            payload: Some(vec![0; 10]),
            ..Default::default()
        };
        let mut frame = encode_response(&resp);
        frame.truncate(HEADER_LEN + 3);
        assert_eq!(decode_response(&frame), Err(RcpError::ShortFrame));
    }

    // ── Status round-trip ─────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-WIRE-006
    fn status_round_trip_healthy() {
        let st = Status {
            zone: Zone::FRONT_RIGHT,
            seq: 42,
            healthy: true,
            payload: Some(vec![0xDE, 0xAD]),
        };
        let frame = encode_status(&st);
        let decoded = decode_status(&frame).unwrap();
        assert_eq!(decoded.zone, st.zone);
        assert_eq!(decoded.seq, st.seq);
        assert_eq!(decoded.healthy, st.healthy);
        assert_eq!(decoded.payload, st.payload);
    }

    #[test]
    // fusa:test REQ-WIRE-006
    fn status_round_trip_unhealthy() {
        let st = Status {
            zone: Zone::REAR_RIGHT,
            seq: 99,
            healthy: false,
            payload: None,
        };
        let frame = encode_status(&st);
        let decoded = decode_status(&frame).unwrap();
        assert!(!decoded.healthy);
        assert!(decoded.payload.is_none());
    }

    #[test]
    // fusa:test REQ-WIRE-007
    fn decode_status_rejects_truncated_body() {
        let st = Status {
            payload: Some(vec![0; 20]),
            ..Default::default()
        };
        let mut frame = encode_status(&st);
        frame.truncate(HEADER_LEN + 1);
        assert_eq!(decode_status(&frame), Err(RcpError::ShortFrame));
    }

    // ── Control frame ─────────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-WIRE-008
    fn control_frame_passes_validate_header() {
        let frame = encode_control_frame(TYPE_SUBSCRIBE, Zone::FRONT_LEFT);
        assert_eq!(frame.len(), HEADER_LEN);
        assert!(validate_header(&frame).is_ok());
        assert_eq!(frame[3], TYPE_SUBSCRIBE);
        assert_eq!(frame[4], Zone::FRONT_LEFT.0);
    }

    #[test]
    // fusa:test REQ-WIRE-008
    fn unsubscribe_control_frame_passes_validate_header() {
        let frame = encode_control_frame(TYPE_UNSUBSCRIBE, Zone::REAR_RIGHT);
        assert!(validate_header(&frame).is_ok());
        assert_eq!(frame[3], TYPE_UNSUBSCRIBE);
    }

    // ── Fuzz-style: arbitrary bytes never panic ───────────────────────────────

    #[test]
    // fusa:test REQ-WIRE-009
    fn decode_functions_never_panic_on_arbitrary_input() {
        let inputs: &[&[u8]] = &[
            &[],
            &[
                0x52, 0x43, 0x01, 0x01, 0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                0xFF, 0xFF,
            ],
            // body_len near u32::MAX
            &[
                0x52, 0x43, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF,
                0xFF, 0xFF,
            ],
            &[0x52, 0x43, 0x01, 0x01],
        ];
        for input in inputs {
            let _ = decode_command(input);
            let _ = decode_response(input);
            let _ = decode_status(input);
        }
    }
}
