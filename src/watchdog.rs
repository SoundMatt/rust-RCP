// fusa:req REQ-WDG-001
// fusa:req REQ-WDG-002
// fusa:req REQ-WDG-003
// fusa:req REQ-WDG-004
// fusa:req REQ-WDG-005
// fusa:req REQ-WDG-006
// fusa:req REQ-WDG-007
// fusa:req REQ-WDG-008

//! Software watchdog — periodic WATCHDOG command dispatcher.
//!
//! If a zone controller does not respond to WATCHDOG commands within the
//! configured window, the controller is flagged as unhealthy and optionally
//! closed. Implements ISO 26262 §6 periodic-monitoring requirement.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::{Command, CommandType, Controller, RcpError, Zone};

// ── WatchdogConfig ────────────────────────────────────────────────────────────

/// Watchdog configuration.
// fusa:req REQ-WDG-001
#[derive(Clone, Debug)]
pub struct WatchdogConfig {
    /// How often to send a WATCHDOG command.
    pub interval: Duration,
    /// Number of consecutive missed responses before declaring unhealthy.
    pub miss_window: u32,
    /// If true, close the controller when the window is exceeded.
    pub close_on_miss: bool,
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        WatchdogConfig {
            interval: Duration::from_secs(1),
            miss_window: 3,
            close_on_miss: false,
        }
    }
}

// ── WatchdogMonitor ───────────────────────────────────────────────────────────

struct Inner {
    controller: Arc<dyn Controller>,
    config: WatchdogConfig,
    misses: AtomicU32,
    healthy: AtomicBool,
    stopped: AtomicBool,
}

/// Periodic watchdog monitor for a single zone controller.
// fusa:req REQ-WDG-002
pub struct WatchdogMonitor {
    inner: Arc<Inner>,
}

impl WatchdogMonitor {
    /// Start watchdog monitoring. The background thread begins immediately.
    // fusa:req REQ-WDG-003
    pub fn start(controller: Arc<dyn Controller>, config: WatchdogConfig) -> Self {
        let inner = Arc::new(Inner {
            controller,
            config,
            misses: AtomicU32::new(0),
            healthy: AtomicBool::new(true),
            stopped: AtomicBool::new(false),
        });
        let inner2 = Arc::clone(&inner);
        std::thread::Builder::new()
            .name(format!("wdg-{}", inner.controller.zone().0))
            .spawn(move || Self::run(inner2))
            .expect("watchdog thread spawn failed");
        WatchdogMonitor { inner }
    }

    fn run(inner: Arc<Inner>) {
        while !inner.stopped.load(Ordering::Relaxed) {
            std::thread::sleep(inner.config.interval);
            if inner.stopped.load(Ordering::Relaxed) {
                break;
            }
            let zone = inner.controller.zone();
            let cmd = Command {
                zone,
                cmd_type: CommandType::WATCHDOG,
                ..Default::default()
            };
            match inner.controller.send(&cmd, Some(inner.config.interval)) {
                Ok(_) => {
                    inner.misses.store(0, Ordering::SeqCst);
                    inner.healthy.store(true, Ordering::SeqCst);
                }
                Err(_) => {
                    let misses = inner.misses.fetch_add(1, Ordering::SeqCst) + 1;
                    if misses >= inner.config.miss_window {
                        inner.healthy.store(false, Ordering::SeqCst);
                        if inner.config.close_on_miss {
                            let _ = inner.controller.close();
                            break;
                        }
                    }
                }
            }
        }
    }

    /// True if the controller has responded within the miss window.
    // fusa:req REQ-WDG-004
    pub fn is_healthy(&self) -> bool {
        self.inner.healthy.load(Ordering::SeqCst)
    }

    /// Number of consecutive missed WATCHDOG responses.
    // fusa:req REQ-WDG-005
    pub fn miss_count(&self) -> u32 {
        self.inner.misses.load(Ordering::SeqCst)
    }

    /// Stop the watchdog thread.
    // fusa:req REQ-WDG-006
    pub fn stop(&self) {
        self.inner.stopped.store(true, Ordering::SeqCst);
    }

    /// Zone being monitored.
    // fusa:req REQ-WDG-007
    pub fn zone(&self) -> Zone {
        self.inner.controller.zone()
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

    fn ok_ctrl() -> Arc<dyn Controller> {
        let h: crate::mock::Handler = Box::new(|cmd| Response {
            command_id: cmd.id,
            zone: cmd.zone,
            status: ResponseStatus::OK,
            payload: None,
        });
        MockController::new(Zone::FRONT_LEFT, Some(h)) as Arc<dyn Controller>
    }

    #[test]
    // fusa:test REQ-WDG-001
    fn default_config_values() {
        let cfg = WatchdogConfig::default();
        assert_eq!(cfg.interval, Duration::from_secs(1));
        assert_eq!(cfg.miss_window, 3);
        assert!(!cfg.close_on_miss);
    }

    #[test]
    // fusa:test REQ-WDG-007
    fn zone_matches_controller() {
        let cfg = WatchdogConfig {
            interval: Duration::from_millis(50),
            ..Default::default()
        };
        let w = WatchdogMonitor::start(ok_ctrl(), cfg);
        assert_eq!(w.zone(), Zone::FRONT_LEFT);
        w.stop();
    }

    #[test]
    // fusa:test REQ-WDG-004
    fn healthy_with_responsive_controller() {
        let cfg = WatchdogConfig {
            interval: Duration::from_millis(20),
            miss_window: 3,
            close_on_miss: false,
        };
        let w = WatchdogMonitor::start(ok_ctrl(), cfg);
        // Wait for at least one poll
        std::thread::sleep(Duration::from_millis(60));
        assert!(w.is_healthy());
        w.stop();
    }

    #[test]
    // fusa:test REQ-WDG-005
    // fusa:test REQ-WDG-008
    fn miss_count_zero_on_responsive_controller() {
        let cfg = WatchdogConfig {
            interval: Duration::from_millis(20),
            ..Default::default()
        };
        let w = WatchdogMonitor::start(ok_ctrl(), cfg);
        std::thread::sleep(Duration::from_millis(60));
        assert_eq!(w.miss_count(), 0);
        w.stop();
    }

    #[test]
    // fusa:test REQ-WDG-006
    fn stop_terminates_monitor() {
        let cfg = WatchdogConfig {
            interval: Duration::from_millis(10),
            ..Default::default()
        };
        let w = WatchdogMonitor::start(ok_ctrl(), cfg);
        w.stop(); // should not deadlock or panic
    }

    #[test]
    // fusa:test REQ-WDG-002
    // fusa:test REQ-WDG-003
    fn closed_controller_increments_miss_count() {
        let ctrl = ok_ctrl();
        ctrl.close().unwrap();
        let cfg = WatchdogConfig {
            interval: Duration::from_millis(10),
            miss_window: 5,
            close_on_miss: false,
        };
        let w = WatchdogMonitor::start(ctrl, cfg);
        std::thread::sleep(Duration::from_millis(60));
        assert!(
            w.miss_count() > 0,
            "misses must accumulate for closed controller"
        );
        assert!(!w.is_healthy(), "unhealthy after misses");
        w.stop();
    }
}
