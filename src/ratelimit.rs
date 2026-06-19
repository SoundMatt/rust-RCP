// fusa:req REQ-RL-001
// fusa:req REQ-RL-002
// fusa:req REQ-RL-003
// fusa:req REQ-RL-004
// fusa:req REQ-RL-005
// fusa:req REQ-RL-006
// fusa:req REQ-RL-007
// fusa:req REQ-RL-008

//! Token-bucket rate limiter controller.
//!
//! Commands that cannot be immediately served return `Err(RcpError::Busy)`.
//! Critical commands bypass the bucket when `exempt_critical = true`.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::{Command, Controller, Priority, RcpError, Response, Subscription, Zone};

// ── Config ────────────────────────────────────────────────────────────────────

/// Token-bucket configuration.
// fusa:req REQ-RL-001
#[derive(Clone, Debug)]
pub struct Config {
    /// Sustained command rate (commands per second).
    pub rate: f64,
    /// Maximum burst capacity (number of commands).
    pub burst: f64,
    /// When `true`, Critical-priority commands bypass the rate limiter.
    pub exempt_critical: bool,
}

/// Returns the default rate-limiter config: 100 cmd/s, 20-command burst, Critical exempt.
// fusa:req REQ-RL-002
pub fn default_config() -> Config {
    Config {
        rate: 100.0,
        burst: 20.0,
        exempt_critical: true,
    }
}

// ── Bucket ────────────────────────────────────────────────────────────────────

struct Bucket {
    tokens: f64,
    last: Instant,
    rate: f64,
    burst: f64,
}

impl Bucket {
    fn new(cfg: &Config) -> Self {
        Bucket {
            tokens: cfg.burst,
            last: Instant::now(),
            rate: cfg.rate,
            burst: cfg.burst,
        }
    }

    /// Replenish tokens based on elapsed time, then attempt to consume one.
    /// Returns `false` if the bucket is empty.
    fn try_consume(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.rate).min(self.burst);
        self.last = now;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

// ── RateLimitController ───────────────────────────────────────────────────────

/// Rate-limiting wrapper around an inner [`Controller`].
// fusa:req REQ-RL-003
pub struct RateLimitController {
    inner: Arc<dyn Controller>,
    bucket: Mutex<Bucket>,
    exempt_critical: bool,
}

impl RateLimitController {
    /// Create a new `RateLimitController` with the given configuration.
    // fusa:req REQ-RL-004
    pub fn new(inner: Arc<dyn Controller>, cfg: Config) -> Self {
        let exempt = cfg.exempt_critical;
        RateLimitController {
            inner,
            bucket: Mutex::new(Bucket::new(&cfg)),
            exempt_critical: exempt,
        }
    }

    /// Create with the default configuration.
    pub fn new_default(inner: Arc<dyn Controller>) -> Self {
        Self::new(inner, default_config())
    }
}

impl Controller for RateLimitController {
    fn zone(&self) -> Zone {
        self.inner.zone()
    }

    // fusa:req REQ-RL-005
    // fusa:req REQ-RL-006
    // fusa:req REQ-RL-007
    fn send(&self, cmd: &Command, timeout: Option<Duration>) -> Result<Response, RcpError> {
        let is_critical = cmd.priority == Priority::CRITICAL;

        if !is_critical || !self.exempt_critical {
            let mut bucket = self.bucket.lock().unwrap();
            if !bucket.try_consume() {
                return Err(RcpError::Busy);
            }
        }

        self.inner.send(cmd, timeout)
    }

    fn subscribe(&self) -> Result<Subscription, RcpError> {
        self.inner.subscribe()
    }

    fn close(&self) -> Result<(), RcpError> {
        self.inner.close()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockController;
    use crate::{Command, Priority, Response, ResponseStatus, Zone};

    fn ok_controller() -> Arc<dyn Controller> {
        let h: crate::mock::Handler = Box::new(|cmd| Response {
            command_id: cmd.id,
            zone: cmd.zone,
            status: ResponseStatus::OK,
            payload: None,
        });
        MockController::new(Zone::FRONT_LEFT, Some(h)) as Arc<dyn Controller>
    }

    fn rl(rate: f64, burst: f64, exempt_critical: bool) -> RateLimitController {
        let cfg = Config {
            rate,
            burst,
            exempt_critical,
        };
        RateLimitController::new(ok_controller(), cfg)
    }

    // ── Default config ────────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-RL-002
    fn default_config_values() {
        let cfg = default_config();
        assert_eq!(cfg.rate, 100.0);
        assert_eq!(cfg.burst, 20.0);
        assert!(cfg.exempt_critical);
    }

    // ── Burst allowed ─────────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-RL-001
    // fusa:test REQ-RL-005
    fn burst_capacity_is_honoured() {
        let rl = rl(1.0, 5.0, false); // 1 cmd/s, burst=5
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            ..Default::default()
        };

        for _ in 0..5 {
            rl.send(&cmd, None).unwrap();
        }
        // 6th should be rejected
        let err = rl.send(&cmd, None).unwrap_err();
        assert_eq!(err, RcpError::Busy);
    }

    // ── Empty bucket returns Busy ─────────────────────────────────────────────

    #[test]
    // fusa:test REQ-RL-006
    fn bucket_exhaustion_returns_busy() {
        let rl = rl(0.0, 0.0, false); // zero tokens — always Busy
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            ..Default::default()
        };
        let err = rl.send(&cmd, None).unwrap_err();
        assert_eq!(err, RcpError::Busy);
    }

    // ── Critical exempt ───────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-RL-007
    fn critical_bypasses_empty_bucket() {
        let rl = rl(0.0, 0.0, true); // zero tokens, Critical exempt
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            priority: Priority::CRITICAL,
            ..Default::default()
        };
        rl.send(&cmd, None).unwrap(); // must succeed despite empty bucket
    }

    #[test]
    // fusa:test REQ-RL-007
    fn critical_not_exempt_when_disabled() {
        let rl = rl(0.0, 0.0, false); // zero tokens, Critical NOT exempt
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            priority: Priority::CRITICAL,
            ..Default::default()
        };
        let err = rl.send(&cmd, None).unwrap_err();
        assert_eq!(err, RcpError::Busy);
    }

    // ── Normal and High obey bucket ───────────────────────────────────────────

    #[test]
    // fusa:test REQ-RL-005
    fn normal_priority_obeys_bucket() {
        let rl = rl(0.0, 0.0, true); // Critical exempt, but normal is not
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            priority: Priority::NORMAL,
            ..Default::default()
        };
        let err = rl.send(&cmd, None).unwrap_err();
        assert_eq!(err, RcpError::Busy);
    }

    #[test]
    // fusa:test REQ-RL-005
    fn high_priority_obeys_bucket() {
        let rl = rl(0.0, 0.0, true);
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            priority: Priority::HIGH,
            ..Default::default()
        };
        let err = rl.send(&cmd, None).unwrap_err();
        assert_eq!(err, RcpError::Busy);
    }

    // ── Zone forwarded ────────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-RL-003
    fn zone_matches_inner() {
        let inner = MockController::new(Zone::REAR_LEFT, None) as Arc<dyn Controller>;
        let rl = RateLimitController::new_default(inner);
        assert_eq!(rl.zone(), Zone::REAR_LEFT);
    }

    // ── Token replenishment ───────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-RL-004
    fn tokens_replenish_over_time() {
        let rl = rl(1000.0, 1.0, false); // very fast replenishment, burst=1
        let cmd = Command {
            zone: Zone::FRONT_LEFT,
            ..Default::default()
        };
        rl.send(&cmd, None).unwrap(); // consume the one token
                                      // Wait for replenishment
        std::thread::sleep(Duration::from_millis(5));
        rl.send(&cmd, None).unwrap(); // should succeed after replenishment
    }

    // ── Busy is a relay timeout sentinel ─────────────────────────────────────

    #[test]
    // fusa:test REQ-RL-008
    fn busy_is_relay_timeout_sentinel() {
        let err = RcpError::Busy;
        assert!(
            err.is_relay_timeout(),
            "Busy must satisfy is_relay_timeout()"
        );
    }

    // ── Close forwarded ───────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-RL-003
    fn close_is_forwarded_to_inner() {
        let rl = rl(100.0, 10.0, true);
        rl.close().unwrap();
    }
}
