//fusa:req REQ-CFG-001
//fusa:req REQ-CFG-005
//fusa:req REQ-CFG-006
//fusa:req REQ-CFG-007
//fusa:req REQ-CFG-008

//! Configuration loader and validator for the RC Server register-map/
//! lifecycle model (`ROADMAP.md` Milestone 9, Satellite Package Migration,
//! `config`'s own REPLACE row in the Satellite Package Disposition table).
//!
//! REPLACE of this crate's old ad-hoc config: the previous [`RcpConfig`]
//! (deleted by this item, per `ROADMAP.md`'s own disposition-table reason
//! for this file — "must represent the register-map/lifecycle
//! configuration model, not `RcpConfig{controllers, watchdog, rate_limit}`")
//! loaded a flat `controllers: HashMap<String, ControllerConfig>` keyed by
//! `Zone`, plus a standalone `WatchdogConfig`/`RateLimitConfig` pair, and
//! validated it against invented bounds (`zone <= 5`,
//! `max_payload_bytes <= 65491`) that have no referent in the real OPEN
//! Alliance TC18 Remote Control Protocol Specification v0.5.1_RC. None of
//! that shape survives here.
//!
//! This module instead gives the register-map/lifecycle pieces Milestone 2
//! already built — [`crate::regmap::GeneralRegisters`] (`§3.6`), the five
//! `§3.7`-`§3.11` child config-table row types, and
//! [`crate::lifecycle::RcServerState`] — a JSON/YAML-loadable,
//! human-authorable configuration surface, the same role [`RcpConfig`]
//! used to serve for the old `Controller`/`Registry` model. [`RcServerConfig`]
//! is the loadable/serializable unit: it *composes* those existing types
//! rather than duplicating their field lists in a parallel shape — see
//! `src/regmap.rs`'s and `src/lifecycle.rs`'s own "`Serialize`/`Deserialize`
//! derive" doc-comment sections for the purely-additive derives this item
//! added to them so that composition is possible without inventing a
//! second copy of every field.
//!
//! [`from_json`]/[`from_yaml`] keep the same two-format-loader shape
//! [`RcpConfig`]'s own loader established, now parsing [`RcServerConfig`]
//! instead. [`validate`] is rebuilt from scratch against real register-map/
//! lifecycle constraints rather than the old invented bounds:
//!
//! - Each of the four `TableDescriptor`-backed child tables
//!   ([`GeneralRegisters::svr_hw_cfg`], [`GeneralRegisters::
//!   svr_request_stream_cfg`], [`GeneralRegisters::svr_ep_bytebus_id_map`],
//!   [`GeneralRegisters::svr_response_stream_cfg`]) must have enough
//!   declared `capacity` to hold the config's own row `Vec` for that table
//!   — a real structural constraint the old model had no equivalent of.
//! - [`GeneralRegisters::svr_sequencers_max`] must be large enough to hold
//!   the config's `sequencer_state` row count, since
//!   [`GeneralRegisters::svr_sequencer_state_ptr`] is pointer-only (no
//!   paired capacity field of its own — see `src/regmap.rs`'s own "Config
//!   tables" doc-comment section) and this crate's own Milestone 5
//!   lifecycle work already uses `svr_sequencers_max` as that table's row-
//!   count bound.
//! - Every `§3.8`-category row table populated in the config must be
//!   reachable given the config's own `initial_state`, per
//!   [`crate::lifecycle::is_register_reachable`]: [`RequestStreamConfigEntry`],
//!   [`EpByteBusIdMapEntry`], [`ResponseStreamConfigEntry`], and
//!   [`SequencerStateEntry`] all carry [`RegisterCategory::RcpConfig`], which
//!   [`crate::lifecycle::is_register_reachable`] reports unreachable while
//!   `initial_state` is [`RcServerState::HwUnconfigured`] — configuring rows
//!   for a table the server would immediately reject as unreachable is
//!   caught here rather than left as a runtime surprise.
//!
//! Field-*width* bounds (e.g. a `u8` count never exceeding `255`) are
//! deliberately not re-checked here: [`GeneralRegisters::encode`]/
//! [`GeneralRegisters::decode`] and each row type's own `encode`/`decode`
//! already enforce them structurally, through Rust's own integer types,
//! the same way every prior Milestone 1-8 encode/decode pair in this crate
//! does. Only relationships *between* fields — a table row count against
//! its own declared capacity bound, a category against the state that
//! gates its reachability — need [`validate`]'s own logic, since nothing
//! else in this crate checks those relationships either.
//!
//! Like the old [`RcpConfig`]'s own loader, this module performs no
//! register I/O against a real RC Server and is not wired into
//! [`crate::ep0`], [`crate::lifecycle`], or any other existing caller
//! ([`config::`] had zero callers anywhere else in `src/` before this item,
//! confirmed by inspection, and still has none after it) — purely additive
//! standalone plumbing, matching the discipline this milestone's other
//! REPLACE cutovers already established.

use serde::{Deserialize, Serialize};

use crate::lifecycle::{is_register_reachable, RcServerState, RegisterCategory};
use crate::regmap::{
    EpByteBusIdMapEntry, GeneralRegisters, HwPinMappingEntry, RequestStreamConfigEntry,
    ResponseStreamConfigEntry, SequencerStateEntry,
};
use crate::RcpError;

// ── Config types ──────────────────────────────────────────────────────────────

/// Top-level, loadable RC Server register-map/lifecycle configuration.
///
/// See this module's doc comment for why this composes
/// [`crate::regmap::GeneralRegisters`], the five `§3.7`-`§3.11` child
/// config-table row types, and [`crate::lifecycle::RcServerState`] rather
/// than reinventing them.
//fusa:req REQ-CFG-001
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct RcServerConfig {
    /// Lifecycle state to bring a freshly loaded RC Server up in.
    ///
    /// Defaults to [`RcServerState::INITIAL`] (`HW_UNCONFIGURED`) via
    /// [`RcServerState`]'s own `Default` impl.
    #[serde(default)]
    pub initial_state: RcServerState,

    /// The `§3.6` general register-map block.
    #[serde(default)]
    pub general: GeneralRegisters,

    /// `§3.7` HW pin-mapping table rows.
    #[serde(default)]
    pub hw_pin_mapping: Vec<HwPinMappingEntry>,

    /// `§3.8` request-stream config table rows.
    #[serde(default)]
    pub request_streams: Vec<RequestStreamConfigEntry>,

    /// `§3.9` EP/`byte_bus_id` mapping table rows.
    #[serde(default)]
    pub ep_bytebus_id_map: Vec<EpByteBusIdMapEntry>,

    /// `§3.10` response/ack queue config table rows.
    #[serde(default)]
    pub response_streams: Vec<ResponseStreamConfigEntry>,

    /// `§3.11` sequencer-state rows.
    #[serde(default)]
    pub sequencer_state: Vec<SequencerStateEntry>,
}

// ── Loader ────────────────────────────────────────────────────────────────────

/// Parse a JSON string into [`RcServerConfig`].
///
/// Returns `Err(RcpError::Other(_))` wrapping the underlying `serde_json`
/// message on malformed input. Never panics.
//fusa:req REQ-CFG-005
pub fn from_json(s: &str) -> Result<RcServerConfig, RcpError> {
    serde_json::from_str(s).map_err(|e| RcpError::Other(format!("config: invalid JSON: {e}")))
}

/// Parse a YAML string into [`RcServerConfig`].
///
/// Returns `Err(RcpError::Other(_))` wrapping the underlying `serde_yaml`
/// message on malformed input. Never panics.
//fusa:req REQ-CFG-005
pub fn from_yaml(s: &str) -> Result<RcServerConfig, RcpError> {
    serde_yaml::from_str(s).map_err(|e| RcpError::Other(format!("config: invalid YAML: {e}")))
}

/// Validate a parsed [`RcServerConfig`] against real register-map/lifecycle
/// constraints.
///
/// See this module's doc comment for the full list of checks and why
/// field-width bounds are deliberately not repeated here. Returns
/// `Err(RcpError::InvalidParameter)` for a table-capacity/row-count
/// mismatch, `Err(RcpError::UnauthorizedAccess)` for an `RcpConfig`-category
/// table populated while unreachable given `initial_state` (mirroring
/// [`crate::lifecycle::check_register_reachable`]'s own error choice for
/// exactly this "not reachable in this state" shape), `Ok(())` otherwise.
/// Never panics for any input.
//fusa:req REQ-CFG-006
//fusa:req REQ-CFG-007
//fusa:req REQ-CFG-008
pub fn validate(cfg: &RcServerConfig) -> Result<(), RcpError> {
    // ── Table row counts vs. declared capacity ──────────────────────────────
    check_capacity(cfg.hw_pin_mapping.len(), cfg.general.svr_hw_cfg.capacity)?;
    check_capacity(
        cfg.request_streams.len(),
        cfg.general.svr_request_stream_cfg.capacity,
    )?;
    check_capacity(
        cfg.ep_bytebus_id_map.len(),
        cfg.general.svr_ep_bytebus_id_map.capacity,
    )?;
    check_capacity(
        cfg.response_streams.len(),
        cfg.general.svr_response_stream_cfg.capacity,
    )?;

    // `svr_sequencer_state_ptr` is pointer-only (no paired `capacity`
    // field), so `sequencer_state`'s bound comes from `svr_sequencers_max`
    // instead — see this module's doc comment.
    if cfg.sequencer_state.len() > cfg.general.svr_sequencers_max as usize {
        return Err(RcpError::InvalidParameter);
    }

    // ── RcpConfig-category tables must be reachable in initial_state ───────
    let rcp_config_populated = !cfg.request_streams.is_empty()
        || !cfg.ep_bytebus_id_map.is_empty()
        || !cfg.response_streams.is_empty()
        || !cfg.sequencer_state.is_empty();
    if rcp_config_populated
        && !is_register_reachable(cfg.initial_state, RegisterCategory::RcpConfig)
    {
        return Err(RcpError::UnauthorizedAccess);
    }

    Ok(())
}

/// Shared row-count-vs-capacity check for [`validate`]'s four
/// `TableDescriptor`-backed tables. Never panics for any input.
fn check_capacity(row_count: usize, capacity: u16) -> Result<(), RcpError> {
    if row_count > capacity as usize {
        Err(RcpError::InvalidParameter)
    } else {
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::regmap::TableDescriptor;

    #[test]
    //fusa:test REQ-CFG-001
    fn default_config_is_valid() {
        let cfg = RcServerConfig::default();
        validate(&cfg).unwrap();
        assert_eq!(cfg.initial_state, RcServerState::HwUnconfigured);
    }

    #[test]
    //fusa:test REQ-CFG-005
    fn parse_json_minimal() {
        let json = r#"{"initial_state": "HwConfigured"}"#;
        let cfg = from_json(json).unwrap();
        assert_eq!(cfg.initial_state, RcServerState::HwConfigured);
        assert!(cfg.hw_pin_mapping.is_empty());
    }

    #[test]
    //fusa:test REQ-CFG-005
    fn parse_yaml_minimal() {
        let yaml = "initial_state: RcpConfigured\n";
        let cfg = from_yaml(yaml).unwrap();
        assert_eq!(cfg.initial_state, RcServerState::RcpConfigured);
    }

    #[test]
    //fusa:test REQ-CFG-005
    fn invalid_json_returns_error() {
        assert!(from_json("{invalid}").is_err());
    }

    #[test]
    //fusa:test REQ-CFG-005
    fn invalid_yaml_returns_error() {
        assert!(from_yaml(":\n  - not: [valid").is_err());
    }

    #[test]
    //fusa:test REQ-CFG-005
    fn round_trips_through_json_with_table_rows() {
        let mut cfg = RcServerConfig::default();
        cfg.general.svr_hw_cfg = TableDescriptor {
            ptr: 0x100,
            capacity: 4,
        };
        cfg.hw_pin_mapping.push(HwPinMappingEntry {
            hw_ep_nr: 1,
            hw_ep_pin_nr: 2,
            hw_pin_props: 0,
        });
        let json = serde_json::to_string(&cfg).unwrap();
        let back = from_json(&json).unwrap();
        assert_eq!(back, cfg);
        validate(&back).unwrap();
    }

    #[test]
    //fusa:test REQ-CFG-006
    fn validate_rejects_hw_pin_mapping_exceeding_capacity() {
        let mut cfg = RcServerConfig::default();
        cfg.general.svr_hw_cfg = TableDescriptor {
            ptr: 0,
            capacity: 1,
        };
        cfg.hw_pin_mapping = vec![HwPinMappingEntry::default(), HwPinMappingEntry::default()];
        assert_eq!(validate(&cfg), Err(RcpError::InvalidParameter));
    }

    #[test]
    //fusa:test REQ-CFG-006
    fn validate_rejects_request_streams_exceeding_capacity() {
        let mut cfg = RcServerConfig::default();
        cfg.general.svr_request_stream_cfg = TableDescriptor {
            ptr: 0,
            capacity: 0,
        };
        cfg.initial_state = RcServerState::HwConfigured;
        cfg.request_streams = vec![RequestStreamConfigEntry::default()];
        assert_eq!(validate(&cfg), Err(RcpError::InvalidParameter));
    }

    #[test]
    //fusa:test REQ-CFG-006
    fn validate_rejects_ep_bytebus_id_map_exceeding_capacity() {
        let mut cfg = RcServerConfig::default();
        cfg.general.svr_ep_bytebus_id_map = TableDescriptor {
            ptr: 0,
            capacity: 0,
        };
        cfg.initial_state = RcServerState::HwConfigured;
        cfg.ep_bytebus_id_map = vec![EpByteBusIdMapEntry::default()];
        assert_eq!(validate(&cfg), Err(RcpError::InvalidParameter));
    }

    #[test]
    //fusa:test REQ-CFG-006
    fn validate_rejects_response_streams_exceeding_capacity() {
        let mut cfg = RcServerConfig::default();
        cfg.general.svr_response_stream_cfg = TableDescriptor {
            ptr: 0,
            capacity: 0,
        };
        cfg.initial_state = RcServerState::HwConfigured;
        cfg.response_streams = vec![ResponseStreamConfigEntry::default()];
        assert_eq!(validate(&cfg), Err(RcpError::InvalidParameter));
    }

    #[test]
    //fusa:test REQ-CFG-007
    fn validate_rejects_sequencer_state_exceeding_svr_sequencers_max() {
        let mut cfg = RcServerConfig {
            initial_state: RcServerState::HwConfigured,
            ..Default::default()
        };
        cfg.general.svr_sequencers_max = 1;
        cfg.sequencer_state = vec![
            SequencerStateEntry::power_on_default(),
            SequencerStateEntry::power_on_default(),
        ];
        assert_eq!(validate(&cfg), Err(RcpError::InvalidParameter));
    }

    #[test]
    //fusa:test REQ-CFG-007
    fn validate_accepts_sequencer_state_within_svr_sequencers_max() {
        let mut cfg = RcServerConfig {
            initial_state: RcServerState::HwConfigured,
            ..Default::default()
        };
        cfg.general.svr_sequencers_max = 2;
        cfg.sequencer_state = vec![SequencerStateEntry::power_on_default()];
        validate(&cfg).unwrap();
    }

    #[test]
    //fusa:test REQ-CFG-008
    fn validate_rejects_rcp_config_tables_while_hw_unconfigured() {
        let mut cfg = RcServerConfig::default();
        // initial_state defaults to HwUnconfigured, where RcpConfig-category
        // registers are not yet reachable.
        cfg.general.svr_sequencers_max = 1;
        cfg.sequencer_state = vec![SequencerStateEntry::power_on_default()];
        assert_eq!(validate(&cfg), Err(RcpError::UnauthorizedAccess));
    }

    #[test]
    //fusa:test REQ-CFG-008
    fn validate_accepts_rcp_config_tables_once_hw_configured() {
        let mut cfg = RcServerConfig {
            initial_state: RcServerState::HwConfigured,
            ..Default::default()
        };
        cfg.general.svr_sequencers_max = 1;
        cfg.sequencer_state = vec![SequencerStateEntry::power_on_default()];
        validate(&cfg).unwrap();
    }

    #[test]
    //fusa:test REQ-CFG-008
    fn validate_accepts_hw_config_tables_while_hw_unconfigured() {
        // HwConfig-category tables (hw_pin_mapping) are reachable from the
        // very first state onward, unlike RcpConfig-category tables.
        let mut cfg = RcServerConfig::default();
        cfg.general.svr_hw_cfg = TableDescriptor {
            ptr: 0,
            capacity: 1,
        };
        cfg.hw_pin_mapping = vec![HwPinMappingEntry::default()];
        validate(&cfg).unwrap();
    }
}
