// fusa:req REQ-RL-001
// fusa:req REQ-RL-002
// fusa:req REQ-RL-003
// fusa:req REQ-RL-004
// fusa:req REQ-RL-005
// fusa:req REQ-RL-006
// fusa:req REQ-RL-008

//! Token-bucket rate limiter endpoint decorator.
//!
//! Requests that cannot be immediately served return `Err(RcpError::Busy)`.
//!
//! `ROADMAP.md` Milestone 9 ("All ADAPT-disposition packages retargeted...")
//! cutover: per this module's own ADAPT disposition ("generic token-bucket
//! decorator; retarget to whatever new endpoint-request dispatch trait
//! replaces `Controller`"), [`RateLimitEndpoint`] replaces the legacy
//! `RateLimitController`, wrapping [`crate::mock::Endpoint`] instead of
//! `Controller`. The bucket is now consumed uniformly by every
//! [`crate::mock::Endpoint::read`]/[`crate::mock::Endpoint::write`] call:
//! the old `exempt_critical` carve-out has no surviving analog, since
//! `Endpoint` requests carry no `Priority` (that field lived on the
//! deleted `Command` shape a caller addressed a zone controller with, not
//! on anything the register-map/endpoint model defines) — a scope
//! narrowing flagged here per Guiding Principle 5 rather than silently
//! dropped. `REQ-RL-007` ("Critical exempt from rate limit"), which
//! described only that dropped carve-out with no surviving analog, is
//! retired in `.fusa-reqs.json` rather than force-retargeted, per this
//! bullet's own "retarget in place, or explicitly retire if no equivalent
//! behavior exists" instruction.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::mock::Endpoint;
use crate::regmap::EndpointType;
use crate::RcpError;

// ── Config ────────────────────────────────────────────────────────────────────

/// Token-bucket configuration.
// fusa:req REQ-RL-001
#[derive(Clone, Debug)]
pub struct Config {
    /// Sustained request rate (calls per second).
    pub rate: f64,
    /// Maximum burst capacity (number of calls).
    pub burst: f64,
}

/// Returns the default rate-limiter config: 100 calls/s, 20-call burst.
// fusa:req REQ-RL-002
pub fn default_config() -> Config {
    Config {
        rate: 100.0,
        burst: 20.0,
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

// ── RateLimitEndpoint ────────────────────────────────────────────────────────

/// Rate-limiting wrapper around an inner [`Endpoint`].
// fusa:req REQ-RL-003
pub struct RateLimitEndpoint {
    inner: Arc<dyn Endpoint>,
    bucket: Mutex<Bucket>,
}

impl RateLimitEndpoint {
    /// Create a new `RateLimitEndpoint` with the given configuration.
    // fusa:req REQ-RL-004
    pub fn new(inner: Arc<dyn Endpoint>, cfg: Config) -> Self {
        RateLimitEndpoint {
            inner,
            bucket: Mutex::new(Bucket::new(&cfg)),
        }
    }

    /// Create with the default configuration.
    pub fn new_default(inner: Arc<dyn Endpoint>) -> Self {
        Self::new(inner, default_config())
    }

    /// Consume one token, or return `Err(RcpError::Busy)` if the bucket is
    /// empty.
    fn consume(&self) -> Result<(), RcpError> {
        let mut bucket = self.bucket.lock().unwrap();
        if !bucket.try_consume() {
            return Err(RcpError::Busy);
        }
        Ok(())
    }
}

impl Endpoint for RateLimitEndpoint {
    fn ep_type(&self) -> EndpointType {
        self.inner.ep_type()
    }

    // fusa:req REQ-RL-005
    // fusa:req REQ-RL-006
    fn read(&self, read_size: u16) -> Result<Vec<u8>, RcpError> {
        self.consume()?;
        self.inner.read(read_size)
    }

    // fusa:req REQ-RL-005
    // fusa:req REQ-RL-006
    fn write(&self, payload: &[u8]) -> Result<(), RcpError> {
        self.consume()?;
        self.inner.write(payload)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockEndpoint;
    use std::time::Duration;

    fn ok_endpoint() -> Arc<dyn Endpoint> {
        MockEndpoint::new(EndpointType::Gpio, vec![0u8; 4]) as Arc<dyn Endpoint>
    }

    fn rl(rate: f64, burst: f64) -> RateLimitEndpoint {
        let cfg = Config { rate, burst };
        RateLimitEndpoint::new(ok_endpoint(), cfg)
    }

    // ── Default config ────────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-RL-002
    fn default_config_values() {
        let cfg = default_config();
        assert_eq!(cfg.rate, 100.0);
        assert_eq!(cfg.burst, 20.0);
    }

    // ── Burst allowed ─────────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-RL-001
    // fusa:test REQ-RL-005
    fn burst_capacity_is_honoured() {
        let rl = rl(1.0, 5.0); // 1 call/s, burst=5
        for _ in 0..5 {
            rl.write(b"x").unwrap();
        }
        // 6th should be rejected
        let err = rl.write(b"x").unwrap_err();
        assert_eq!(err, RcpError::Busy);
    }

    // ── Empty bucket returns Busy ─────────────────────────────────────────────

    #[test]
    // fusa:test REQ-RL-006
    fn bucket_exhaustion_returns_busy() {
        let rl = rl(0.0, 0.0); // zero tokens — always Busy
        let err = rl.write(b"x").unwrap_err();
        assert_eq!(err, RcpError::Busy);
    }

    #[test]
    // fusa:test REQ-RL-006
    fn burst_exhaustion_across_multiple_calls_returns_busy() {
        let rl = rl(0.0, 3.0); // 3 burst, no refill
        for _ in 0..3 {
            rl.read(1).unwrap();
        }
        // 4th must be rejected — bucket is empty
        assert_eq!(rl.read(1).unwrap_err(), RcpError::Busy);
    }

    // ── Read and write both obey bucket ───────────────────────────────────────

    #[test]
    // fusa:test REQ-RL-005
    fn read_obeys_bucket() {
        let rl = rl(0.0, 0.0);
        let err = rl.read(1).unwrap_err();
        assert_eq!(err, RcpError::Busy);
    }

    #[test]
    // fusa:test REQ-RL-005
    fn write_obeys_bucket() {
        let rl = rl(0.0, 0.0);
        let err = rl.write(b"x").unwrap_err();
        assert_eq!(err, RcpError::Busy);
    }

    // ── ep_type forwarded ────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-RL-003
    fn ep_type_matches_inner() {
        let inner = MockEndpoint::new(EndpointType::Adc, vec![]) as Arc<dyn Endpoint>;
        let rl = RateLimitEndpoint::new_default(inner);
        assert_eq!(rl.ep_type(), EndpointType::Adc);
    }

    // ── Token replenishment ───────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-RL-004
    fn tokens_replenish_over_time() {
        let rl = rl(1000.0, 1.0); // very fast replenishment, burst=1
        rl.write(b"x").unwrap(); // consume the one token
                                 // Wait for replenishment
        std::thread::sleep(Duration::from_millis(5));
        rl.write(b"x").unwrap(); // should succeed after replenishment
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
}
