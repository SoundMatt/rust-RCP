// fusa:req REQ-FW-001
// fusa:req REQ-FW-002
// fusa:req REQ-FW-003
// fusa:req REQ-FW-004
// fusa:req REQ-FW-005
// fusa:req REQ-FW-006

//! Firmware update sequencer for zone controllers.
//!
//! Chunks a firmware image and dispatches it as a sequence of SET commands
//! with a configurable chunk size. Verifies completion via GET command.

use std::sync::Arc;
use std::time::Duration;

use crate::{Command, CommandType, Controller, RcpError, Response, ResponseStatus, Zone};

// ── FirmwareUpdater ───────────────────────────────────────────────────────────

/// Maximum firmware chunk size in bytes.
// fusa:req REQ-FW-001
pub const MAX_CHUNK: usize = 512;

/// Sequences a firmware update over an existing controller channel.
// fusa:req REQ-FW-002
pub struct FirmwareUpdater {
    controller: Arc<dyn Controller>,
    chunk_size: usize,
    timeout:    Option<Duration>,
}

impl FirmwareUpdater {
    pub fn new(controller: Arc<dyn Controller>, chunk_size: usize, timeout: Option<Duration>) -> Self {
        let chunk_size = chunk_size.min(MAX_CHUNK);
        FirmwareUpdater { controller, chunk_size, timeout }
    }

    /// Flash `image` to the zone controller.
    ///
    /// Chunks `image` into `chunk_size` SET commands. Returns the number of
    /// chunks sent on success, or the first error encountered.
    // fusa:req REQ-FW-003
    // fusa:req REQ-FW-004
    pub fn flash(&self, image: &[u8]) -> Result<usize, RcpError> {
        if image.is_empty() { return Err(RcpError::InvalidSize); }

        let zone = self.controller.zone();
        let chunks = image.chunks(self.chunk_size);
        let total = chunks.len();

        for (i, chunk) in image.chunks(self.chunk_size).enumerate() {
            let cmd = Command {
                id:       (i + 1) as u32,
                zone,
                cmd_type: CommandType::SET,
                payload:  Some(chunk.to_vec()),
                ..Default::default()
            };
            let resp = self.controller.send(&cmd, self.timeout)?;
            if resp.status != ResponseStatus::OK {
                return Err(RcpError::Other(format!("chunk {} rejected: {:?}", i, resp.status)));
            }
        }

        // Verify with a GET
        let verify_cmd = Command {
            id:       (total + 1) as u32,
            zone,
            cmd_type: CommandType::GET,
            ..Default::default()
        };
        let verify_resp = self.controller.send(&verify_cmd, self.timeout)?;
        if verify_resp.status != ResponseStatus::OK {
            return Err(RcpError::Other("firmware verify failed".into()));
        }

        Ok(total)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockController;
    use crate::{Command, Response, ResponseStatus, Zone};
    use std::sync::atomic::{AtomicU32, Ordering};

    fn ok_ctrl() -> Arc<dyn Controller> {
        let h: crate::mock::Handler = Box::new(|cmd| Response {
            command_id: cmd.id, zone: cmd.zone,
            status: ResponseStatus::OK, payload: None,
        });
        MockController::new(Zone::FRONT_LEFT, Some(h)) as Arc<dyn Controller>
    }

    #[test]
    // fusa:test REQ-FW-001
    fn max_chunk_constant() {
        assert_eq!(MAX_CHUNK, 512);
    }

    #[test]
    // fusa:test REQ-FW-003
    fn flash_single_chunk() {
        let upd = FirmwareUpdater::new(ok_ctrl(), 512, None);
        let image = vec![0xABu8; 100];
        let n = upd.flash(&image).unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    // fusa:test REQ-FW-004
    fn flash_multiple_chunks() {
        let upd = FirmwareUpdater::new(ok_ctrl(), 16, None);
        let image = vec![0u8; 48]; // 3 chunks of 16
        let n = upd.flash(&image).unwrap();
        assert_eq!(n, 3);
    }

    #[test]
    // fusa:test REQ-FW-002
    fn flash_empty_image_returns_invalid_size() {
        let upd = FirmwareUpdater::new(ok_ctrl(), 512, None);
        let err = upd.flash(&[]).unwrap_err();
        assert_eq!(err, RcpError::InvalidSize);
    }

    #[test]
    // fusa:test REQ-FW-005
    fn chunk_error_aborts_flash() {
        let call = Arc::new(AtomicU32::new(0));
        let c2 = Arc::clone(&call);
        let h: crate::mock::Handler = Box::new(move |cmd| {
            let n = c2.fetch_add(1, Ordering::SeqCst);
            let status = if n == 1 { ResponseStatus::ERROR } else { ResponseStatus::OK };
            Response { command_id: cmd.id, zone: cmd.zone, status, payload: None }
        });
        let ctrl = MockController::new(Zone::FRONT_LEFT, Some(h)) as Arc<dyn Controller>;
        let upd = FirmwareUpdater::new(ctrl, 1, None);
        let image = vec![0u8; 3];
        let err = upd.flash(&image).unwrap_err();
        assert!(matches!(err, RcpError::Other(_)));
    }

    #[test]
    // fusa:test REQ-FW-006
    fn chunk_size_capped_at_max() {
        let upd = FirmwareUpdater::new(ok_ctrl(), usize::MAX, None);
        assert_eq!(upd.chunk_size, MAX_CHUNK);
    }
}
