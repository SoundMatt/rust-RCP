// fusa:req REQ-CFG-001
// fusa:req REQ-CFG-002
// fusa:req REQ-CFG-003
// fusa:req REQ-CFG-004
// fusa:req REQ-CFG-005
// fusa:req REQ-CFG-006

//! Configuration loader and validator.
//!
//! Supports JSON and YAML formats. All config values are validated against
//! schema constraints before use.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ── Config types ──────────────────────────────────────────────────────────────

/// Top-level RCP configuration.
// fusa:req REQ-CFG-001
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct RcpConfig {
    /// Named controller endpoint configurations.
    #[serde(default)]
    pub controllers: HashMap<String, ControllerConfig>,

    /// Watchdog configuration.
    #[serde(default)]
    pub watchdog: WatchdogConfig,

    /// Rate-limit configuration.
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
}

/// Per-controller configuration.
// fusa:req REQ-CFG-002
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ControllerConfig {
    pub zone:             u8,
    pub timeout_ms:       u64,
    pub max_payload_bytes: usize,
}

/// Watchdog timing configuration.
// fusa:req REQ-CFG-003
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WatchdogConfig {
    pub interval_ms: u64,
    pub window:      u32,
}

impl Default for WatchdogConfig {
    fn default() -> Self { WatchdogConfig { interval_ms: 1000, window: 3 } }
}

/// Rate-limit configuration (matches `ratelimit::Config` shape).
// fusa:req REQ-CFG-004
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RateLimitConfig {
    pub rate:             f64,
    pub burst:            f64,
    pub exempt_critical:  bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self { RateLimitConfig { rate: 100.0, burst: 20.0, exempt_critical: true } }
}

// ── Loader ────────────────────────────────────────────────────────────────────

/// Parse a JSON string into [`RcpConfig`].
// fusa:req REQ-CFG-005
pub fn from_json(s: &str) -> Result<RcpConfig, String> {
    serde_json::from_str(s).map_err(|e| e.to_string())
}

/// Parse a YAML string into [`RcpConfig`].
// fusa:req REQ-CFG-005
pub fn from_yaml(s: &str) -> Result<RcpConfig, String> {
    serde_yaml::from_str(s).map_err(|e| e.to_string())
}

/// Validate a parsed config against basic constraints.
// fusa:req REQ-CFG-006
pub fn validate(cfg: &RcpConfig) -> Result<(), String> {
    for (name, ctrl) in &cfg.controllers {
        if ctrl.zone > 5 {
            return Err(format!("controller '{}': zone {} out of range (0..=5)", name, ctrl.zone));
        }
        if ctrl.max_payload_bytes > 65491 {
            return Err(format!("controller '{}': max_payload_bytes exceeds protocol maximum", name));
        }
    }
    if cfg.rate_limit.rate <= 0.0 {
        return Err("rate_limit.rate must be positive".into());
    }
    if cfg.watchdog.window == 0 {
        return Err("watchdog.window must be > 0".into());
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    // fusa:test REQ-CFG-005
    fn parse_json_minimal() {
        let json = r#"{"controllers": {}, "watchdog": {"interval_ms": 500, "window": 2}, "rate_limit": {"rate": 50.0, "burst": 10.0, "exempt_critical": false}}"#;
        let cfg = from_json(json).unwrap();
        assert_eq!(cfg.watchdog.interval_ms, 500);
    }

    #[test]
    // fusa:test REQ-CFG-005
    fn parse_yaml_minimal() {
        let yaml = "controllers: {}\nwatchdog:\n  interval_ms: 200\n  window: 5\nrate_limit:\n  rate: 20.0\n  burst: 5.0\n  exempt_critical: true\n";
        let cfg = from_yaml(yaml).unwrap();
        assert_eq!(cfg.watchdog.window, 5);
    }

    #[test]
    // fusa:test REQ-CFG-005
    fn invalid_json_returns_error() {
        assert!(from_json("{invalid}").is_err());
    }

    #[test]
    // fusa:test REQ-CFG-006
    fn validate_bad_zone() {
        let mut cfg = RcpConfig::default();
        cfg.controllers.insert("ctrl".into(), ControllerConfig { zone: 9, timeout_ms: 100, max_payload_bytes: 512 });
        assert!(validate(&cfg).is_err());
    }

    #[test]
    // fusa:test REQ-CFG-006
    fn validate_bad_rate() {
        let mut cfg = RcpConfig::default();
        cfg.rate_limit.rate = -1.0;
        assert!(validate(&cfg).is_err());
    }

    #[test]
    // fusa:test REQ-CFG-006
    fn validate_zero_watchdog_window() {
        let mut cfg = RcpConfig::default();
        cfg.watchdog.window = 0;
        assert!(validate(&cfg).is_err());
    }

    #[test]
    // fusa:test REQ-CFG-001
    // fusa:test REQ-CFG-003
    // fusa:test REQ-CFG-004
    fn default_config_is_valid() {
        let cfg = RcpConfig::default();
        validate(&cfg).unwrap();
        assert_eq!(cfg.watchdog.interval_ms, 1000);
        assert_eq!(cfg.watchdog.window, 3);
        assert!((cfg.rate_limit.rate - 100.0).abs() < 1e-9);
        assert!((cfg.rate_limit.burst - 20.0).abs() < 1e-9);
        assert!(cfg.rate_limit.exempt_critical);
    }

    #[test]
    // fusa:test REQ-CFG-002
    fn controller_config_fields() {
        let cc = ControllerConfig { zone: 2, timeout_ms: 500, max_payload_bytes: 1024 };
        assert_eq!(cc.zone, 2);
        assert_eq!(cc.timeout_ms, 500);
        assert_eq!(cc.max_payload_bytes, 1024);
    }

    #[test]
    // fusa:test REQ-CFG-006
    fn payload_exceeds_proto_max() {
        let mut cfg = RcpConfig::default();
        cfg.controllers.insert("x".into(), ControllerConfig { zone: 1, timeout_ms: 100, max_payload_bytes: 99999 });
        assert!(validate(&cfg).is_err());
    }
}
