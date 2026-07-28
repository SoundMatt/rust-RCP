// fusa:req REQ-ADC-001
// fusa:req REQ-ADC-002
// fusa:req REQ-ADC-003
// fusa:req REQ-ADC-004
// fusa:req REQ-ADC-005
// fusa:req REQ-ADC-006
// fusa:req REQ-ADC-007
// fusa:req REQ-ADC-008
// fusa:req REQ-ADC-009
// fusa:req REQ-ADC-010

//! The ADC endpoint type (`ep_type 0x09`) — `ROADMAP.md` Milestone 4
//! ("Basic Endpoint Types"), fifth checklist bullet: "≤16-bit resolution;
//! three-level averaging model (`adc_sample_interval` →
//! `adc_avg_intervals_per_request` → `adc_combine_avg_values`);
//! request-driven sampling only."
//!
//! This follows directly on [`crate::uart`] (Milestone 4's fourth item):
//! same milestone, same "additive standalone plumbing only" discipline, same
//! doc-comment provenance-note style for anything this crate has not yet
//! reconciled against confirmed wire behavior. Three named pieces are in
//! scope, all implemented here:
//!
//! - [`AdcResolutionBits`] / [`AdcSampleValue`] — a sample-resolution type
//!   that models "up to 16 bits" as an explicit, validated `1..=16` range
//!   rather than silently assuming every ADC endpoint samples at the full
//!   16-bit width, plus the sample value type that carries a raw reading
//!   together with the resolution it was taken at and refuses a raw value
//!   wider than that resolution allows. See "Provenance note: resolution as
//!   an explicit range, not a fixed width" below.
//! - [`AdcAveragingConfig`] / [`resolve_adc_sample_window_ticks`] /
//!   [`resolve_adc_averaged_value`] — the three-level averaging model,
//!   turned into two pure, typed, testable functions chaining
//!   `adc_sample_interval` → `adc_avg_intervals_per_request` →
//!   `adc_combine_avg_values`, mirroring
//!   [`crate::uart::resolve_uart_read_completion`]'s and
//!   [`crate::spi::truncate_spi_status_for_compound_wait`]'s own
//!   prose-rule-to-function discipline. See "Provenance note: the
//!   three-level averaging chain" below.
//! - [`AdcSamplingMode`] / [`validate_adc_sample_request`] — the
//!   request-driven-sampling-only rule, rejecting any request that asks for
//!   a free-running/continuous sampling mode, mirroring
//!   [`crate::uart::validate_uart_read_request`]'s payload-less-read-only
//!   refusal path. See "Provenance note: request-driven sampling only"
//!   below.
//!
//! Deliberately out of scope, for the same reasons
//! [`crate::gpio`]'s/[`crate::spi`]'s/[`crate::i2c`]'s/[`crate::uart`]'s own
//! doc comments already give:
//!
//! - PWM_OUT / PWM_IN (`ep_type 0x07`/`0x08`) — `ROADMAP.md`'s next
//!   Milestone 4 checklist bullet, not yet built.
//! - The "Groups A/B/C" `evt[2:0]` sub-opcode convention as a general,
//!   cross-endpoint-type classification scheme, and any use of
//!   `evt.sub_opcode` at all — `ROADMAP.md`'s ADC checklist bullet names no
//!   `sub_opcode`-keyed selection mechanism, so this module reads
//!   `sub_opcode` nowhere. That convention is this milestone's still-open
//!   final checklist bullet.
//! - [`crate::regmap::CommonFunctionalConfig`]'s fields — unchanged here, as
//!   in every prior Milestone 1-4 entry.
//! - Any actual sampling loop, timer, or scheduler that would *drive*
//!   `adc_sample_interval`-spaced sampling. This module models the
//!   averaging math and the request-driven-only validation rule as pure
//!   functions only; nothing here runs on a clock.
//! - Wiring any of the below into an actual decoder, dispatch loop, or
//!   [`crate::avtp`]/[`crate::acf`]/[`crate::addressing`] caller. This
//!   module remains additive standalone plumbing only, matching the
//!   discipline every prior Milestone 1-4 entry already established.
//!
//! ## Relationship to [`crate::regmap`]
//!
//! As with [`crate::gpio::GpioFunctionalConfig`],
//! [`crate::spi::SpiFunctionalConfig`], [`crate::i2c::I2cFunctionalConfig`],
//! and [`crate::uart::UartFunctionalConfig`], ADC's real functional-config
//! content gets its own dedicated type, [`AdcFunctionalConfig`], rather than
//! adding ADC-specific fields directly onto the still-shared,
//! thirteen-endpoint-type [`crate::regmap::PerEpTypeFunctionalConfig`]
//! placeholder. [`AdcFunctionalConfig::layer_tag`] shows how a caller
//! obtains the matching generic-layer tag so the two compose through
//! [`crate::regmap::check_functional_config_matches_ep_type`] exactly as
//! that cross-layer rule already expects, without this module editing
//! [`crate::regmap`] itself.
//!
//! ## Provenance note: resolution as an explicit range, not a fixed width
//!
//! `ROADMAP.md`'s ADC checklist bullet states "≤16-bit resolution" — an
//! upper bound, not a confirmed fixed width. Per Guiding Principle 5,
//! [`AdcResolutionBits`] does not silently model every ADC sample as a full
//! 16-bit `u16` the way a less careful reading might; it wraps a validated
//! `1..=16` bit-width value ([`AdcResolutionBits::new`] rejects `0` and
//! anything above `16` with [`crate::RcpError::InvalidParameter`]), and
//! [`AdcSampleValue::new`]/[`AdcSampleValue::decode`] reject a raw reading
//! that carries any bit set above what that resolution allows
//! ([`AdcResolutionBits::max_raw_value`]). The wire representation this
//! module carries a sample value in is nonetheless a fixed 2-byte
//! big-endian field regardless of the configured resolution — narrower
//! resolutions simply leave their unused high bits zero — since
//! `ROADMAP.md`'s checklist text does not state a narrower, resolution-
//! dependent wire width, and this crate does not invent one. This mirrors
//! [`crate::gpio::GpioBitmask`]'s own fixed-width, big-endian wire-form
//! discipline. [`AdcResolutionBits::default`] resolves to the maximum
//! modeled width, 16 bits — this crate's own reasonable placeholder for an
//! unconfirmed power-on default, not a transcription of a confirmed RC
//! Server default, mirroring [`crate::i2c::I2cSpeedMode::default`]'s own
//! explicitly-flagged choice.
//!
//! ## Provenance note: the three-level averaging chain
//!
//! `ROADMAP.md`'s ADC checklist bullet names three fields chained in a
//! stated order — `adc_sample_interval` → `adc_avg_intervals_per_request` →
//! `adc_combine_avg_values` — without stating either field's wire width or
//! units, or the exact arithmetic each stage performs. Per Guiding
//! Principle 5, [`AdcAveragingConfig`] carries all three as this crate's own
//! unconfirmed-width/units placeholders (`adc_sample_interval: u32`, an
//! elapsed-tick count mirroring [`crate::uart::UartRxQueueConfig::uart_timeout`]'s
//! own placeholder treatment; `adc_avg_intervals_per_request: u16` and
//! `adc_combine_avg_values: u16`, both raw counts) rather than transcribing
//! a confirmed wire encoding this crate's spec-extraction pass did not
//! pin down. This crate's own working interpretation of the chain, split
//! into two pure, never-panicking functions rather than one, is:
//!
//! - [`resolve_adc_averaged_value`] — the value pipeline. Every
//!   `adc_avg_intervals_per_request` raw samples are averaged into one
//!   intermediate value (level one); every `adc_combine_avg_values` of
//!   those intermediate values are then further averaged into the single
//!   final result a request returns (level two) — reading "combine" as a
//!   second, coarser averaging stage over the first stage's already-averaged
//!   outputs, since the checklist names no other combination operator.
//! - [`resolve_adc_sample_window_ticks`] — the timing pipeline.
//!   `adc_sample_interval` gives how often one raw sample is taken;
//!   multiplying it by the total raw-sample count
//!   `resolve_adc_averaged_value` consumes
//!   (`adc_avg_intervals_per_request * adc_combine_avg_values`) gives the
//!   total elapsed tick count one fully-combined result takes to produce.
//!
//! Both functions use checked arithmetic and return `None`/
//! [`crate::RcpError::InvalidParameter`] rather than panicking on overflow
//! or on a malformed (zero-count) config, matching the never-panics
//! discipline every prior Milestone 1-4 module already established.
//!
//! ## Provenance note: request-driven sampling only
//!
//! `ROADMAP.md`'s ADC checklist bullet states "request-driven sampling
//! only" but — unlike the UART checklist bullet immediately before it,
//! which named its payload-less-read violation's error code as
//! `UNKNOWN_CMD` — names no error code for what a request asking for a
//! free-running/continuous sampling mode should receive instead. Per
//! Guiding Principle 5, this module flags that gap explicitly rather than
//! silently assuming the same code applies: [`AdcSamplingMode`] models the
//! choice a sample request could in principle carry as an explicit two-
//! variant enum ([`AdcSamplingMode::RequestDriven`]/
//! [`AdcSamplingMode::Continuous`]), and [`validate_adc_sample_request`]
//! rejects [`AdcSamplingMode::Continuous`] with
//! [`crate::RcpError::UnsupportedCmd`] — this crate's own reasoned choice,
//! by the same "closest existing match" reasoning
//! [`crate::uart::validate_uart_read_request`]'s own provenance note already
//! gives for `UnsupportedCmd`, and consistent with
//! [`crate::gpio::apply_gpio_write`]'s and
//! [`crate::spi::resolve_spi_channel_index`]'s own use of the same variant
//! for a request naming an unsupported/unmodeled mode. This does not model
//! an actual continuous-sampling engine anywhere in this crate — there is
//! nothing to run continuously — only the refusal a request-driven-only RC
//! Server would need to give if asked to.

use crate::RcpError;

// ── AdcResolutionBits / AdcSampleValue ──────────────────────────────────────

/// An ADC sample resolution, in bits, validated to the `1..=16` range
/// `ROADMAP.md`'s "≤16-bit resolution" checklist text allows.
///
/// See this module's doc comment "Provenance note: resolution as an
/// explicit range, not a fixed width" for why this is a validated range
/// type rather than this module assuming every sample is a full 16-bit
/// value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// fusa:req REQ-ADC-001
pub struct AdcResolutionBits(u8);

impl AdcResolutionBits {
    /// Construct an [`AdcResolutionBits`] from a bit-width value.
    ///
    /// Returns `Err(RcpError::InvalidParameter)` for `0` (zero bits is not a
    /// meaningful resolution) or any value above `16` (wider than
    /// `ROADMAP.md`'s "≤16-bit resolution" upper bound). Never panics for
    /// any input.
    // fusa:req REQ-ADC-001
    // fusa:req REQ-ADC-002
    pub fn new(bits: u8) -> Result<Self, RcpError> {
        if (1..=16).contains(&bits) {
            Ok(Self(bits))
        } else {
            Err(RcpError::InvalidParameter)
        }
    }

    /// This resolution's bit width, `1..=16`.
    // fusa:req REQ-ADC-001
    pub fn to_u8(self) -> u8 {
        self.0
    }

    /// The largest raw sample value this resolution can represent:
    /// `2^bits - 1`.
    ///
    /// Never panics for any valid [`AdcResolutionBits`] — the widest
    /// modeled resolution, 16 bits, yields `u16::MAX` exactly.
    // fusa:req REQ-ADC-003
    pub fn max_raw_value(self) -> u16 {
        let bits = u32::from(self.0);
        ((1u32 << bits) - 1) as u16
    }
}

impl Default for AdcResolutionBits {
    /// Defaults to the widest modeled resolution, 16 bits — this crate's own
    /// reasonable placeholder for an unconfirmed power-on default, not a
    /// transcription of a confirmed RC Server default. See this module's
    /// doc comment.
    fn default() -> Self {
        Self(16)
    }
}

/// A single ADC sample reading: a raw value together with the resolution it
/// was taken at.
///
/// [`AdcSampleValue::new`]/[`AdcSampleValue::decode`] reject any raw value
/// that carries a bit set above what `resolution` allows, per this module's
/// doc comment "Provenance note: resolution as an explicit range, not a
/// fixed width". The wire form is a fixed 2-byte big-endian field
/// regardless of `resolution`, matching [`crate::gpio::GpioBitmask`]'s own
/// fixed-width, big-endian discipline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// fusa:req REQ-ADC-004
pub struct AdcSampleValue {
    /// The raw sample reading.
    pub raw: u16,
    /// The resolution `raw` was taken at.
    pub resolution: AdcResolutionBits,
}

impl AdcSampleValue {
    /// Construct an [`AdcSampleValue`], validating `raw` against
    /// `resolution`'s [`AdcResolutionBits::max_raw_value`].
    ///
    /// Returns `Err(RcpError::InvalidParameter)` if `raw` exceeds that
    /// maximum. Never panics for any input.
    // fusa:req REQ-ADC-004
    pub fn new(raw: u16, resolution: AdcResolutionBits) -> Result<Self, RcpError> {
        if raw > resolution.max_raw_value() {
            Err(RcpError::InvalidParameter)
        } else {
            Ok(Self { raw, resolution })
        }
    }

    /// Encode this sample to its 2-byte big-endian wire representation.
    // fusa:req REQ-ADC-004
    pub fn encode(&self) -> [u8; 2] {
        self.raw.to_be_bytes()
    }

    /// Decode an [`AdcSampleValue`] from its 2-byte big-endian wire
    /// representation at the given resolution.
    ///
    /// Returns `Err(RcpError::InvalidParameter)` if the decoded raw value
    /// exceeds `resolution`'s [`AdcResolutionBits::max_raw_value`]. Never
    /// panics for any input.
    // fusa:req REQ-ADC-004
    pub fn decode(bytes: [u8; 2], resolution: AdcResolutionBits) -> Result<Self, RcpError> {
        Self::new(u16::from_be_bytes(bytes), resolution)
    }
}

// ── AdcFunctionalConfig ──────────────────────────────────────────────────────

/// The three-level averaging model's own config content: the raw ingredients
/// [`resolve_adc_sample_window_ticks`] and [`resolve_adc_averaged_value`]
/// chain together.
///
/// See this module's doc comment "Provenance note: the three-level
/// averaging chain" for each field's unconfirmed width/units and for how
/// the two resolving functions connect them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
// fusa:req REQ-ADC-005
pub struct AdcAveragingConfig {
    /// How often one raw sample is taken, in this crate's own unconfirmed
    /// tick-count units.
    pub adc_sample_interval: u32,
    /// How many raw samples are averaged into one intermediate value per
    /// request (averaging level one).
    pub adc_avg_intervals_per_request: u16,
    /// How many of those intermediate averaged values are further combined
    /// into the final result (averaging level two).
    pub adc_combine_avg_values: u16,
}

/// ADC's own per-EP-type functional-config content: this endpoint's sample
/// [`AdcResolutionBits`] and its [`AdcAveragingConfig`].
///
/// See this module's doc comment "Relationship to `crate::regmap`" for why
/// this is a dedicated type rather than content added directly to
/// [`crate::regmap::PerEpTypeFunctionalConfig`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
// fusa:req REQ-ADC-005
pub struct AdcFunctionalConfig {
    /// This endpoint's configured sample resolution.
    pub resolution: AdcResolutionBits,
    /// This endpoint's configured three-level averaging model.
    pub averaging: AdcAveragingConfig,
}

impl AdcFunctionalConfig {
    /// The [`crate::regmap::PerEpTypeFunctionalConfig`] generic-layer tag
    /// that matches this ADC functional config, for use with
    /// [`crate::regmap::check_functional_config_matches_ep_type`].
    ///
    /// This module does not itself call that function — it only shows how a
    /// caller would obtain the matching tag, per this module's doc comment
    /// "Relationship to `crate::regmap`".
    // fusa:req REQ-ADC-006
    pub fn layer_tag(&self) -> crate::regmap::PerEpTypeFunctionalConfig {
        crate::regmap::PerEpTypeFunctionalConfig::new(crate::regmap::EndpointType::Adc)
    }
}

// ── Three-level averaging chain ──────────────────────────────────────────────

/// Resolve the timing half of the three-level averaging chain: the total
/// elapsed `adc_sample_interval` tick count one fully-combined
/// [`resolve_adc_averaged_value`] result takes to produce.
///
/// Uses checked, saturating-to-`None` arithmetic throughout rather than
/// panicking or silently wrapping on overflow. With
/// [`AdcAveragingConfig`]'s current field widths (`u16 * u16 * u32`) the
/// product always fits a `u64` — even the all-`MAX` combination stays just
/// under `u64::MAX` — so `None` is not reachable today; the checked
/// arithmetic is kept as this crate's standing discipline against silent
/// wraparound rather than a claim that overflow is currently possible, and
/// protects a future field-width revision from silently wrapping instead.
/// See this module's doc comment "Provenance note: the three-level
/// averaging chain".
// fusa:req REQ-ADC-007
pub fn resolve_adc_sample_window_ticks(averaging: &AdcAveragingConfig) -> Option<u64> {
    let raw_samples_needed = u64::from(averaging.adc_avg_intervals_per_request)
        .checked_mul(u64::from(averaging.adc_combine_avg_values))?;
    raw_samples_needed.checked_mul(u64::from(averaging.adc_sample_interval))
}

/// Resolve the value half of the three-level averaging chain: reduce a
/// slice of raw ADC samples to a single combined result via
/// `averaging`'s two averaging levels.
///
/// `raw_samples` is grouped into `adc_avg_intervals_per_request`-sized
/// groups (level one, each group averaged independently), then the first
/// `adc_combine_avg_values` of those group averages are themselves averaged
/// into the returned result (level two). Returns
/// `Err(RcpError::InvalidParameter)` if either configured count is zero, if
/// `raw_samples` does not carry at least
/// `adc_avg_intervals_per_request * adc_combine_avg_values` raw samples, or —
/// defensively, mirroring [`resolve_adc_sample_window_ticks`]'s own checked
/// arithmetic even though the `u16 * u16` product this function multiplies
/// cannot overflow `usize` on any platform this crate targets — if that
/// product would overflow. Never panics for any input.
// fusa:req REQ-ADC-008
// fusa:req REQ-ADC-009
pub fn resolve_adc_averaged_value(
    raw_samples: &[u16],
    averaging: &AdcAveragingConfig,
) -> Result<u32, RcpError> {
    let group_size = usize::from(averaging.adc_avg_intervals_per_request);
    let group_count = usize::from(averaging.adc_combine_avg_values);

    if group_size == 0 || group_count == 0 {
        return Err(RcpError::InvalidParameter);
    }

    let raw_samples_needed = group_size
        .checked_mul(group_count)
        .ok_or(RcpError::InvalidParameter)?;
    if raw_samples.len() < raw_samples_needed {
        return Err(RcpError::InvalidParameter);
    }

    let mut combined_sum: u64 = 0;
    for group in raw_samples[..raw_samples_needed].chunks_exact(group_size) {
        let group_sum: u64 = group.iter().map(|&v| u64::from(v)).sum();
        combined_sum += group_sum / group_size as u64;
    }
    // Each group average is itself bounded by u16::MAX (the widest raw
    // sample this module models), so combined_sum / group_count can never
    // exceed u16::MAX either — this cast is always exact, never truncating.
    let combined_avg = combined_sum / group_count as u64;
    Ok(combined_avg as u32)
}

// ── Request-driven sampling only ─────────────────────────────────────────────

/// The sampling mode a sample request could in principle ask for.
///
/// See this module's doc comment "Provenance note: request-driven sampling
/// only" for why [`AdcSamplingMode::Continuous`] exists here at all despite
/// this endpoint type never actually running one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// fusa:req REQ-ADC-010
pub enum AdcSamplingMode {
    /// One sample (or one fully-combined result) per request — the only
    /// mode `ROADMAP.md`'s ADC checklist bullet allows.
    RequestDriven,
    /// A free-running/continuous sampling mode. Always rejected by
    /// [`validate_adc_sample_request`].
    Continuous,
}

impl Default for AdcSamplingMode {
    /// Defaults to [`AdcSamplingMode::RequestDriven`] — the only mode
    /// `ROADMAP.md`'s ADC checklist bullet allows.
    fn default() -> Self {
        Self::RequestDriven
    }
}

/// Validate the request-driven-sampling-only rule: a sample request must not
/// ask for [`AdcSamplingMode::Continuous`].
///
/// Returns `Err(RcpError::UnsupportedCmd)` for
/// [`AdcSamplingMode::Continuous`] — see this module's doc comment
/// "Provenance note: request-driven sampling only" for why this crate reads
/// the checklist's unstated violation code onto this already-defined
/// variant. Never panics for any input.
// fusa:req REQ-ADC-010
pub fn validate_adc_sample_request(mode: AdcSamplingMode) -> Result<(), RcpError> {
    match mode {
        AdcSamplingMode::RequestDriven => Ok(()),
        AdcSamplingMode::Continuous => Err(RcpError::UnsupportedCmd),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── AdcResolutionBits: construction / round trip ─────────────────────────

    #[test]
    // fusa:test REQ-ADC-001
    fn adc_resolution_bits_round_trips_through_new_to_u8_for_the_full_1_to_16_range() {
        for bits in 1u8..=16 {
            let resolution = AdcResolutionBits::new(bits).unwrap();
            assert_eq!(resolution.to_u8(), bits);
        }
    }

    #[test]
    // fusa:test REQ-ADC-002
    fn adc_resolution_bits_new_rejects_zero_and_above_sixteen() {
        for bits in [0u8, 17, 32, 255] {
            assert_eq!(
                AdcResolutionBits::new(bits),
                Err(RcpError::InvalidParameter)
            );
        }
    }

    #[test]
    // fusa:test REQ-ADC-002
    fn adc_resolution_bits_new_never_panics_for_any_sampled_input() {
        for bits in [0u8, 1, 8, 16, 17, 128, 255] {
            let _ = AdcResolutionBits::new(bits);
        }
    }

    #[test]
    // fusa:test REQ-ADC-003
    fn adc_resolution_bits_max_raw_value_is_two_pow_bits_minus_one() {
        for bits in 1u8..=16 {
            let resolution = AdcResolutionBits::new(bits).unwrap();
            let expected = ((1u32 << u32::from(bits)) - 1) as u16;
            assert_eq!(resolution.max_raw_value(), expected);
        }
        assert_eq!(
            AdcResolutionBits::new(16).unwrap().max_raw_value(),
            u16::MAX
        );
        assert_eq!(AdcResolutionBits::new(1).unwrap().max_raw_value(), 1);
    }

    #[test]
    // fusa:test REQ-ADC-003
    fn adc_resolution_bits_default_is_sixteen_bits() {
        assert_eq!(AdcResolutionBits::default().to_u8(), 16);
        assert_eq!(AdcResolutionBits::default().max_raw_value(), u16::MAX);
    }

    // ── AdcSampleValue: construction / round trip ────────────────────────────

    #[test]
    // fusa:test REQ-ADC-004
    fn adc_sample_value_round_trips_through_encode_decode_within_resolution() {
        let resolution = AdcResolutionBits::new(12).unwrap();
        for raw in [0u16, 1, 2048, resolution.max_raw_value()] {
            let sample = AdcSampleValue::new(raw, resolution).unwrap();
            let decoded = AdcSampleValue::decode(sample.encode(), resolution).unwrap();
            assert_eq!(decoded, sample);
        }
    }

    #[test]
    // fusa:test REQ-ADC-004
    fn adc_sample_value_new_rejects_raw_wider_than_resolution() {
        let resolution = AdcResolutionBits::new(8).unwrap();
        assert_eq!(
            AdcSampleValue::new(256, resolution),
            Err(RcpError::InvalidParameter)
        );
        assert_eq!(
            AdcSampleValue::new(u16::MAX, resolution),
            Err(RcpError::InvalidParameter)
        );
        assert!(AdcSampleValue::new(255, resolution).is_ok());
    }

    #[test]
    // fusa:test REQ-ADC-004
    fn adc_sample_value_decode_never_panics_for_any_sampled_input() {
        for resolution_bits in [1u8, 8, 16] {
            let resolution = AdcResolutionBits::new(resolution_bits).unwrap();
            for raw in [0u16, 1, 255, 4095, u16::MAX] {
                let _ = AdcSampleValue::decode(raw.to_be_bytes(), resolution);
            }
        }
    }

    // ── AdcFunctionalConfig / layer_tag ──────────────────────────────────────

    #[test]
    // fusa:test REQ-ADC-005
    fn adc_functional_config_default_uses_default_resolution_and_zeroed_averaging() {
        let config = AdcFunctionalConfig::default();
        assert_eq!(config.resolution, AdcResolutionBits::default());
        assert_eq!(config.averaging, AdcAveragingConfig::default());
        assert_eq!(config.averaging.adc_sample_interval, 0);
        assert_eq!(config.averaging.adc_avg_intervals_per_request, 0);
        assert_eq!(config.averaging.adc_combine_avg_values, 0);
    }

    #[test]
    // fusa:test REQ-ADC-006
    fn adc_functional_config_layer_tag_matches_ep_type_adc() {
        let functional = AdcFunctionalConfig::default();
        let generic = crate::regmap::PerEpConfigBlock::new(crate::regmap::EndpointType::Adc);
        let tag = functional.layer_tag();
        assert_eq!(tag.ep_type, crate::regmap::EndpointType::Adc);
        assert!(crate::regmap::functional_config_matches_ep_type(
            &generic, &tag
        ));
        assert_eq!(
            crate::regmap::check_functional_config_matches_ep_type(&generic, &tag),
            Ok(())
        );
    }

    #[test]
    // fusa:test REQ-ADC-006
    fn adc_functional_config_layer_tag_rejects_mismatched_ep_type() {
        let functional = AdcFunctionalConfig::default();
        let generic = crate::regmap::PerEpConfigBlock::new(crate::regmap::EndpointType::Uart);
        let tag = functional.layer_tag();
        assert!(!crate::regmap::functional_config_matches_ep_type(
            &generic, &tag
        ));
    }

    // ── resolve_adc_sample_window_ticks: the timing chain ────────────────────

    #[test]
    // fusa:test REQ-ADC-007
    fn resolve_adc_sample_window_ticks_multiplies_all_three_fields() {
        let averaging = AdcAveragingConfig {
            adc_sample_interval: 10,
            adc_avg_intervals_per_request: 4,
            adc_combine_avg_values: 3,
        };
        // 4 * 3 raw samples, each 10 ticks apart.
        assert_eq!(resolve_adc_sample_window_ticks(&averaging), Some(120));
    }

    #[test]
    // fusa:test REQ-ADC-007
    fn resolve_adc_sample_window_ticks_does_not_overflow_at_max_field_widths() {
        // With AdcAveragingConfig's current u16/u16/u32 field widths, the
        // full product never exceeds u64::MAX (see this function's doc
        // comment), so the checked arithmetic resolves to Some rather than
        // None here.
        let averaging = AdcAveragingConfig {
            adc_sample_interval: u32::MAX,
            adc_avg_intervals_per_request: u16::MAX,
            adc_combine_avg_values: u16::MAX,
        };
        let expected = u64::from(u16::MAX) * u64::from(u16::MAX) * u64::from(u32::MAX);
        assert_eq!(resolve_adc_sample_window_ticks(&averaging), Some(expected));
    }

    #[test]
    // fusa:test REQ-ADC-007
    fn resolve_adc_sample_window_ticks_never_panics_for_any_sampled_input() {
        let intervals = [0u32, 1, 1000, u32::MAX];
        let counts = [0u16, 1, 255, u16::MAX];
        for &interval in &intervals {
            for &per_request in &counts {
                for &combine in &counts {
                    let averaging = AdcAveragingConfig {
                        adc_sample_interval: interval,
                        adc_avg_intervals_per_request: per_request,
                        adc_combine_avg_values: combine,
                    };
                    let _ = resolve_adc_sample_window_ticks(&averaging);
                }
            }
        }
    }

    // ── resolve_adc_averaged_value: the value chain ──────────────────────────

    #[test]
    // fusa:test REQ-ADC-008
    fn resolve_adc_averaged_value_computes_the_two_stage_average() {
        let averaging = AdcAveragingConfig {
            adc_sample_interval: 1,
            adc_avg_intervals_per_request: 2,
            adc_combine_avg_values: 2,
        };
        // Group 1: (10, 20) -> avg 15. Group 2: (30, 40) -> avg 35.
        // Combined: (15 + 35) / 2 = 25.
        let raw_samples = [10u16, 20, 30, 40];
        assert_eq!(resolve_adc_averaged_value(&raw_samples, &averaging), Ok(25));
    }

    #[test]
    // fusa:test REQ-ADC-008
    fn resolve_adc_averaged_value_ignores_trailing_samples_beyond_what_the_pipeline_needs() {
        let averaging = AdcAveragingConfig {
            adc_sample_interval: 1,
            adc_avg_intervals_per_request: 2,
            adc_combine_avg_values: 1,
        };
        // Only the first 2 samples (one group) are consumed.
        let raw_samples = [100u16, 200, 9999, 9999];
        assert_eq!(
            resolve_adc_averaged_value(&raw_samples, &averaging),
            Ok(150)
        );
    }

    #[test]
    // fusa:test REQ-ADC-008
    fn resolve_adc_averaged_value_single_sample_groups_return_the_samples_own_value() {
        let averaging = AdcAveragingConfig {
            adc_sample_interval: 1,
            adc_avg_intervals_per_request: 1,
            adc_combine_avg_values: 1,
        };
        assert_eq!(resolve_adc_averaged_value(&[42u16], &averaging), Ok(42));
    }

    #[test]
    // fusa:test REQ-ADC-009
    fn resolve_adc_averaged_value_rejects_zero_avg_intervals_per_request() {
        let averaging = AdcAveragingConfig {
            adc_sample_interval: 1,
            adc_avg_intervals_per_request: 0,
            adc_combine_avg_values: 1,
        };
        assert_eq!(
            resolve_adc_averaged_value(&[1u16, 2, 3], &averaging),
            Err(RcpError::InvalidParameter)
        );
    }

    #[test]
    // fusa:test REQ-ADC-009
    fn resolve_adc_averaged_value_rejects_zero_combine_avg_values() {
        let averaging = AdcAveragingConfig {
            adc_sample_interval: 1,
            adc_avg_intervals_per_request: 1,
            adc_combine_avg_values: 0,
        };
        assert_eq!(
            resolve_adc_averaged_value(&[1u16, 2, 3], &averaging),
            Err(RcpError::InvalidParameter)
        );
    }

    #[test]
    // fusa:test REQ-ADC-009
    fn resolve_adc_averaged_value_rejects_insufficient_raw_samples() {
        let averaging = AdcAveragingConfig {
            adc_sample_interval: 1,
            adc_avg_intervals_per_request: 4,
            adc_combine_avg_values: 4,
        };
        // Needs 16 raw samples; only 3 supplied.
        assert_eq!(
            resolve_adc_averaged_value(&[1u16, 2, 3], &averaging),
            Err(RcpError::InvalidParameter)
        );
    }

    #[test]
    // fusa:test REQ-ADC-009
    fn resolve_adc_averaged_value_rejects_insufficient_samples_at_max_field_widths() {
        // group_size * group_count at u16::MAX/u16::MAX does not actually
        // overflow usize (see this function's doc comment) — this exercises
        // the checked_mul path at its largest input without overflowing,
        // then falls through to the insufficient-raw-samples rejection.
        let averaging = AdcAveragingConfig {
            adc_sample_interval: 1,
            adc_avg_intervals_per_request: u16::MAX,
            adc_combine_avg_values: u16::MAX,
        };
        assert_eq!(
            resolve_adc_averaged_value(&[1u16, 2, 3], &averaging),
            Err(RcpError::InvalidParameter)
        );
    }

    #[test]
    // fusa:test REQ-ADC-009
    fn resolve_adc_averaged_value_never_panics_for_any_sampled_input() {
        let averagings = [
            AdcAveragingConfig {
                adc_sample_interval: 0,
                adc_avg_intervals_per_request: 0,
                adc_combine_avg_values: 0,
            },
            AdcAveragingConfig {
                adc_sample_interval: 1,
                adc_avg_intervals_per_request: 1,
                adc_combine_avg_values: 1,
            },
            AdcAveragingConfig {
                adc_sample_interval: u32::MAX,
                adc_avg_intervals_per_request: u16::MAX,
                adc_combine_avg_values: u16::MAX,
            },
        ];
        let sample_buffers: [&[u16]; 3] = [&[], &[0u16, 1, 2], &[u16::MAX; 8]];
        for averaging in &averagings {
            for buffer in &sample_buffers {
                let _ = resolve_adc_averaged_value(buffer, averaging);
            }
        }
    }

    // ── AdcSamplingMode / validate_adc_sample_request ────────────────────────

    #[test]
    // fusa:test REQ-ADC-010
    fn adc_sampling_mode_defaults_to_request_driven() {
        assert_eq!(AdcSamplingMode::default(), AdcSamplingMode::RequestDriven);
    }

    #[test]
    // fusa:test REQ-ADC-010
    fn validate_adc_sample_request_accepts_request_driven() {
        assert_eq!(
            validate_adc_sample_request(AdcSamplingMode::RequestDriven),
            Ok(())
        );
    }

    #[test]
    // fusa:test REQ-ADC-010
    fn validate_adc_sample_request_rejects_continuous() {
        assert_eq!(
            validate_adc_sample_request(AdcSamplingMode::Continuous),
            Err(RcpError::UnsupportedCmd)
        );
    }
}
