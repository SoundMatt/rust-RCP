//fusa:req REQ-PWM-001
//fusa:req REQ-PWM-002
//fusa:req REQ-PWM-003
//fusa:req REQ-PWM-004
//fusa:req REQ-PWM-005
//fusa:req REQ-PWM-006
//fusa:req REQ-PWM-007
//fusa:req REQ-PWM-008
//fusa:req REQ-PWM-009
//fusa:req REQ-PWMI-004
//fusa:req REQ-PWMI-005

//! The PWM_OUT / PWM_IN endpoint types (`ep_type 0x07`/`0x08`) —
//! `ROADMAP.md` Milestone 4 ("Basic Endpoint Types"), fifth checklist
//! bullet (following [`crate::uart`]'s `ep_type 0x05` item and preceding
//! this milestone's ADC (`ep_type 0x09`) and closing `evt[2:0]`
//! group-conventions items): "shared period+active-duration pair shape;
//! PWM_IN's `PWM_IN_NO_SIGNAL` timeout instead of hanging or returning
//! stale data."
//!
//! Three named pieces are in scope, all implemented here:
//!
//! - [`PwmDurationPair`] / [`PwmOutFunctionalConfig`] /
//!   [`PwmInFunctionalConfig`] — the shared period+active-duration pair
//!   *shape*, reused by both directions' functional-config content. See
//!   "Provenance note: one shared pair shape, two endpoint types" below for
//!   why this differs from [`crate::uart::UartFunctionalConfig`]'s single
//!   shared *block*.
//! - [`PwmInReadResolution`] / [`resolve_pwm_in_read`] — the
//!   `PWM_IN_NO_SIGNAL` timeout, turned into a pure, typed, testable
//!   function mirroring [`crate::uart::resolve_uart_read_completion`]'s own
//!   prose-rule-to-function discipline. See "Provenance note:
//!   `PWM_IN_NO_SIGNAL`, hanging, and stale data" below for how this avoids
//!   both failure modes the checklist bullet names.
//! - [`PwmInRequest`]/[`PwmInRequest::from_evt_sub_opcode`] — PWM_IN's own
//!   request-decode entry point, validating an incoming request's
//!   `evt.sub_opcode` against [`crate::evtgroup::evt_row2_kind_of`]'s TC18
//!   §13.5 Table 33 Row-2 rule. See "Provenance note: evt[2:0] request
//!   validation" below — this piece was added after this module's own
//!   original scope note below (which still accurately described why no
//!   `sub_opcode` reading existed here originally, before this addition) as
//!   this crate's third Row-2 endpoint-type module, following
//!   [`crate::i2c::I2cRequest`]/[`crate::i2c::I2cRequest::from_evt_sub_opcode`]'s
//!   pilot pattern and
//!   [`crate::adc::AdcRequest`]/[`crate::adc::AdcRequest::from_evt_sub_opcode`]'s
//!   second application of it. The remaining five Row-2 endpoint types
//!   (`LIN, CAN, UART, ISELED, MDIO`) are expected to follow the same
//!   pattern in their own later items.
//!
//! Deliberately out of scope, for the same reasons
//! [`crate::gpio`]'s/[`crate::spi`]'s/[`crate::i2c`]'s/[`crate::uart`]'s own
//! doc comments already give:
//!
//! - The "Groups A/B/C" `evt[2:0]` sub-opcode convention
//!   ([`crate::evtgroup::EvtGroup`]) as a general, cross-endpoint-type
//!   classification scheme — this milestone's own separate, still-open
//!   closing checklist bullet. [`crate::evtgroup`]'s own doc comment already
//!   flags that broader scheme as unresolved, independent of the narrower,
//!   unambiguous Table 33 Row-2 rule this module's [`PwmInRequest`] now
//!   implements (see "Provenance note: evt[2:0] request validation" below).
//! - [`crate::regmap::CommonFunctionalConfig`]'s fields — unchanged here.
//! - Any wire-level byte encoding for [`PwmDurationPair`]'s two fields, or
//!   for a PWM_IN measurement response. See "Provenance note: field widths
//!   and units" below — [`PwmInRequest::Plain`]'s own raw, undecoded payload
//!   (see "Provenance note: evt[2:0] request validation") does not resolve
//!   this either.
//! - Decoding [`PwmInRequest::ConfigWrite`]'s own TC18 §12.7.1 payload
//!   shape. [`PwmInRequest::from_evt_sub_opcode`] recognizes a config-write
//!   request as distinct from a [`Plain`](PwmInRequest::Plain) one, but does
//!   not itself interpret what the config-write payload contains — that is
//!   separate, later work, same as every Row-2 endpoint-type module this
//!   predicate lands in.
//! - Wiring [`PwmInRequest::from_evt_sub_opcode`] into an actual decoder,
//!   dispatch loop, or [`crate::mock::Endpoint`] implementation.
//!   [`crate::mock::Endpoint`]'s own trait signature still does not carry an
//!   `evt` value to any implementation at all — that gap is not specific to
//!   PWM_IN, it applies identically to
//!   [`crate::i2c::I2cRequest::from_evt_sub_opcode`]/
//!   [`crate::adc::AdcRequest::from_evt_sub_opcode`] (confirmed still
//!   unwired against [`crate::mock::Endpoint`]'s own doc comment) and to
//!   [`crate::gpio::GpioWriteSemantics::from_sub_opcode`]/
//!   [`crate::spi::SpiChannelSelect::from_sub_opcode`]. [`PwmInRequest`] is
//!   built to that same "additive standalone plumbing only" level.
//!
//! ## Relationship to [`crate::regmap`]
//!
//! Unlike every prior Milestone 4 entry, this checklist bullet names *two*
//! [`crate::regmap::EndpointType`] tags at once —
//! [`crate::regmap::EndpointType::PwmOut`] (`0x07`) and
//! [`crate::regmap::EndpointType::PwmIn`] (`0x08`) — rather than one. This
//! module therefore gives each direction its own dedicated
//! functional-config type, [`PwmOutFunctionalConfig`] and
//! [`PwmInFunctionalConfig`], each with its own `layer_tag` composing
//! against [`crate::regmap::check_functional_config_matches_ep_type`]'s
//! existing cross-layer rule independently — mirroring
//! [`crate::gpio::GpioFunctionalConfig`]'s/
//! [`crate::spi::SpiFunctionalConfig`]'s/
//! [`crate::i2c::I2cFunctionalConfig`]'s one-type-per-`EndpointType`
//! precedent, rather than [`crate::uart::UartFunctionalConfig`]'s
//! one-type-covering-two-directions precedent (UART's TX/RX split lives
//! inside a single `ep_type 0x05` endpoint; PWM_OUT/PWM_IN are two entirely
//! distinct register-map endpoints).
//!
//! ## Provenance note: one shared pair shape, two endpoint types
//!
//! `ROADMAP.md`'s checklist bullet calls the period+active-duration pair
//! "shared" but, unlike UART's "one ... functional-config block" wording,
//! does not say the two directions share one functional-config *type* —
//! only the pair's *shape*. Per the "Relationship to `crate::regmap`" note
//! above, [`crate::regmap::EndpointType`] already enumerates PWM_OUT and
//! PWM_IN as two distinct endpoint types, so this module reads "shared ...
//! shape" as: one common struct, [`PwmDurationPair`], reused as a field
//! inside each direction's own independent functional-config type —
//! [`PwmOutFunctionalConfig::target`] (the waveform PWM_OUT drives) and, for
//! PWM_IN, as [`resolve_pwm_in_read`]'s measured-value parameter and
//! [`PwmInReadResolution::Measured`]'s payload rather than a field stored on
//! [`PwmInFunctionalConfig`] itself (see the next provenance note for why).
//! This is this crate's own working interpretation of "shared shape" as
//! distinct from "shared block," flagged per Guiding Principle 5 pending
//! reconciliation against confirmed wire behavior.
//!
//! ## Provenance note: field widths and units
//!
//! Neither [`PwmDurationPair::period`]'s nor
//! [`PwmDurationPair::active_duration`]'s bit width or unit (raw hardware
//! tick count vs. microseconds vs. some other resolution) is stated by the
//! checklist text. Both are carried as plain `u32` values — this crate's
//! own unconfirmed-width/units placeholder, mirroring
//! [`crate::uart::UartRxQueueConfig::uart_timeout`]'s own precedent for an
//! unconfirmed tick-count field — rather than this crate guessing a
//! specific width or resolution. Whether
//! [`PwmDurationPair::active_duration`] may validly exceed
//! [`PwmDurationPair::period`] (a greater-than-100%-duty-cycle case) is
//! likewise unstated, so this module does not validate that relationship at
//! all; a future item reconciling this against confirmed wire behavior may
//! need to add such a check.
//!
//! ## Provenance note: `PWM_IN_NO_SIGNAL`, hanging, and stale data
//!
//! The checklist bullet names two failure modes a PWM_IN read must *not*
//! exhibit once no signal has been present for the configured timeout:
//! hanging (blocking indefinitely) and returning stale data (silently
//! re-reporting a previous measurement as though it were current).
//! [`resolve_pwm_in_read`] avoids both: it never blocks — like
//! [`crate::uart::resolve_uart_read_completion`], it is a pure function a
//! caller polls, returning `None` while a read is genuinely still awaiting
//! its first edge and the timeout has not yet elapsed (not a hang, since
//! the caller controls the polling loop, not this function) — and it never
//! re-reports a stale measurement past the timeout:
//! [`PwmInReadResolution::NoSignal`] is returned once
//! `elapsed_since_last_edge` reaches
//! [`PwmInFunctionalConfig::no_signal_timeout`] regardless of whether a
//! prior measurement exists, taking priority over any `last_measured` value
//! supplied. As with [`crate::uart::resolve_uart_read_completion`]'s own
//! zero-threshold discipline, a zero-valued `no_signal_timeout` is not
//! treated as "disabled" — [`PwmInFunctionalConfig::default`]'s zeroed
//! timeout resolves every read as `NoSignal` immediately, a consequence of
//! not inventing a disabling sentinel, not a claim about real RC Server
//! power-on behavior.
//!
//! Separately: the checklist's `PWM_IN_NO_SIGNAL` name has no candidate
//! among Milestone 2's Error Model's eleven TC18-spec-named
//! [`crate::RcpError`] variants (unlike UART's `UNKNOWN_CMD`, which this
//! crate read onto the existing [`crate::RcpError::UnsupportedCmd`] — see
//! [`crate::uart`]'s own provenance note). This crate reads
//! `PWM_IN_NO_SIGNAL` as describing a *measurement outcome* a PWM_IN read
//! response reports, not a request failure — so it is modeled as
//! [`PwmInReadResolution::NoSignal`], a resolved value alongside
//! [`PwmInReadResolution::Measured`], rather than as a [`crate::RcpError`]
//! variant at all.
//!
//! Also flagged: this module does not attempt to distinguish "a
//! freshly-measured pair whose period and active-duration both happen to be
//! `0`" from a genuine no-signal condition by inspecting
//! [`PwmDurationPair`]'s numeric contents. [`resolve_pwm_in_read`]'s
//! `NoSignal` outcome is driven purely by elapsed time against the
//! configured timeout, never by the measured pair's value — avoiding a
//! fragile "an all-zero reading means no signal" convention the checklist
//! text does not state.
//!
//! ## Provenance note: evt[2:0] request validation
//!
//! PWM_IN is one of the eight endpoint types TC18 §13.5 Table 33 groups
//! into one shared "Row 2" `evt[2:0]` rule — see [`crate::evtgroup`]'s own
//! doc comment "Provenance note: TC18 §13.5 Table 33's Row-2 rule
//! (`evt_row2_kind_of`)" for the full citation, including the literal-text
//! discrepancy that module's doc comment flags and resolves (Table 33's own
//! printed Row-2 cell reads "000b to 110b reserved", including 000b, which
//! this crate does not implement literally). [`PwmInRequest::from_evt_sub_opcode`]
//! is this module's own caller of that shared
//! [`crate::evtgroup::evt_row2_kind_of`] predicate.
//!
//! Unlike [`crate::adc::AdcRequest::Plain`], which carries no payload struct
//! at all because TC18 §13.7.9.3 states the ADC request has none,
//! [`PwmInRequest::Plain`] (`evt[2:0] == 000b`) does carry a payload: TC18
//! §13.7.6.3 states plainly "The PWM_IN request and response contain two 16
//! bit values in the payload." — the request side is explicitly named, not
//! silently absent the way ADC's is. However, §13.7.6.3's very next
//! sentence ("Responses include the measured active time of the PWM signal
//! as well as the period of the signal...") states what the *response's*
//! two values mean without ever stating what the *request's* two values
//! mean, and this module's own pre-existing "Provenance note: field widths
//! and units" above already flags [`PwmDurationPair`]'s exact wire-level
//! width, units, and byte framing as unconfirmed, with no
//! encode/decode pair defined for it at all. Per Guiding Principle 5, this
//! module does not bridge those two open questions to invent a decode of
//! the request payload into a [`PwmDurationPair`]: [`PwmInRequest::Plain`]
//! instead carries the payload as [`PwmInByteTransfer`], a raw,
//! byte-for-byte, uninterpreted transfer — mirroring
//! [`crate::i2c::I2cByteTransfer`]'s identical "framing not yet confirmed,
//! carry the bytes rather than guess a structure" discipline, rather than
//! [`crate::adc::AdcRequest`]'s "the spec states there is no payload, so
//! reject a non-empty one" discipline, since TC18 does not state PWM_IN's
//! request payload is absent the way it does for ADC. Every `Reserved`
//! sub_opcode is rejected with `Err(`[`RcpError::UnsupportedCmd`]`)`,
//! matching Table 33's own stated error code and
//! [`crate::i2c::I2cRequest::from_evt_sub_opcode`]'s/
//! [`crate::adc::AdcRequest::from_evt_sub_opcode`]'s identical refusal of
//! their own table's reserved code.

use crate::evtgroup::{evt_row2_kind_of, EvtRow2Kind};
use crate::regmap::{EndpointType, PerEpTypeFunctionalConfig};
use crate::RcpError;

// ── PwmDurationPair ──────────────────────────────────────────────────────────

/// The period+active-duration pair shape shared by both
/// [`PwmOutFunctionalConfig::target`] and [`resolve_pwm_in_read`]'s measured
/// output.
///
/// See this module's doc comment "Provenance note: field widths and units"
/// for why both fields are unconfirmed-width/units `u32` values rather than
/// a specific bit width or physical unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-PWM-001
pub struct PwmDurationPair {
    /// The full PWM cycle length.
    pub period: u32,
    /// The portion of `period` the signal spends in its active state.
    pub active_duration: u32,
}

// ── PwmOutFunctionalConfig ───────────────────────────────────────────────────

/// PWM_OUT's (`ep_type 0x07`) own per-EP-type functional-config content: the
/// target waveform this endpoint drives.
///
/// See this module's doc comment "Relationship to `crate::regmap`" for why
/// this is a dedicated type, and "Provenance note: one shared pair shape,
/// two endpoint types" for why it holds [`PwmDurationPair`] as a field
/// rather than PWM_OUT/PWM_IN sharing one functional-config type outright.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-PWM-002
pub struct PwmOutFunctionalConfig {
    /// The period+active-duration pair this endpoint drives onto the wire.
    pub target: PwmDurationPair,
}

impl PwmOutFunctionalConfig {
    /// The [`crate::regmap::PerEpTypeFunctionalConfig`] generic-layer tag
    /// that matches this PWM_OUT functional config, for use with
    /// [`crate::regmap::check_functional_config_matches_ep_type`].
    ///
    /// This module does not itself call that function — it only shows how a
    /// caller would obtain the matching tag, per
    /// [`crate::uart::UartFunctionalConfig::layer_tag`]'s own precedent.
    //fusa:req REQ-PWM-003
    pub fn layer_tag(&self) -> PerEpTypeFunctionalConfig {
        PerEpTypeFunctionalConfig::new(EndpointType::PwmOut)
    }
}

// ── PwmInFunctionalConfig ────────────────────────────────────────────────────

/// PWM_IN's (`ep_type 0x08`) own per-EP-type functional-config content: the
/// `PWM_IN_NO_SIGNAL` timeout threshold [`resolve_pwm_in_read`] measures
/// elapsed time against.
///
/// See this module's doc comment "Provenance note: `PWM_IN_NO_SIGNAL`,
/// hanging, and stale data" for why this holds only the timeout threshold —
/// not a stored last-measured [`PwmDurationPair`] — and for why a zero
/// threshold is not treated as "disabled."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-PWM-004
pub struct PwmInFunctionalConfig {
    /// How much time may elapse since the last observed signal edge before
    /// a read resolves to [`PwmInReadResolution::NoSignal`] instead of a
    /// measured value.
    pub no_signal_timeout: u32,
}

impl PwmInFunctionalConfig {
    /// The [`crate::regmap::PerEpTypeFunctionalConfig`] generic-layer tag
    /// that matches this PWM_IN functional config, for use with
    /// [`crate::regmap::check_functional_config_matches_ep_type`].
    //fusa:req REQ-PWM-005
    pub fn layer_tag(&self) -> PerEpTypeFunctionalConfig {
        PerEpTypeFunctionalConfig::new(EndpointType::PwmIn)
    }
}

// ── PWM_IN_NO_SIGNAL read resolution ─────────────────────────────────────────

/// The outcome of resolving one PWM_IN read against
/// [`PwmInFunctionalConfig::no_signal_timeout`], as computed by
/// [`resolve_pwm_in_read`].
///
/// See this module's doc comment "Provenance note: `PWM_IN_NO_SIGNAL`,
/// hanging, and stale data" for why this is a resolved measurement outcome
/// rather than a [`crate::RcpError`] variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
//fusa:req REQ-PWM-006
//fusa:req REQ-PWM-007
pub enum PwmInReadResolution {
    /// A signal edge was observed within the configured timeout; the inner
    /// value is the measured period+active-duration pair.
    Measured(PwmDurationPair),
    /// No signal edge has been observed for at least
    /// [`PwmInFunctionalConfig::no_signal_timeout`] — the `PWM_IN_NO_SIGNAL`
    /// condition. Takes priority over any previously measured value; see
    /// this module's doc comment for why.
    NoSignal,
}

/// Resolve one PWM_IN read's `PWM_IN_NO_SIGNAL`-or-measured outcome.
///
/// `last_measured` is the most recent measured pair, if any (`None` if no
/// edge has ever been observed on this endpoint). `elapsed_since_last_edge`
/// is how much time has passed since that measurement (or since the
/// endpoint became active, if `last_measured` is `None`).
///
/// Returns `None` only while genuinely still awaiting a first edge before
/// the timeout has elapsed — the caller polls rather than this function
/// blocking, so this is not a hang. Once `elapsed_since_last_edge` reaches
/// `config.no_signal_timeout`, this always returns
/// `Some(PwmInReadResolution::NoSignal)`, regardless of `last_measured` — a
/// stale measurement is never returned past the timeout. Never panics for
/// any input.
//fusa:req REQ-PWM-006
//fusa:req REQ-PWM-007
//fusa:req REQ-PWM-008
//fusa:req REQ-PWMI-003
pub fn resolve_pwm_in_read(
    config: &PwmInFunctionalConfig,
    last_measured: Option<PwmDurationPair>,
    elapsed_since_last_edge: u32,
) -> Option<PwmInReadResolution> {
    let timed_out = elapsed_since_last_edge >= config.no_signal_timeout;
    match (last_measured, timed_out) {
        (_, true) => Some(PwmInReadResolution::NoSignal),
        (Some(pair), false) => Some(PwmInReadResolution::Measured(pair)),
        (None, false) => None,
    }
}

// ── PwmInRequest: evt[2:0] request validation ───────────────────────────────

/// A raw, unstructured `byte_msg_payload` transfer carried by a
/// [`PwmInRequest::Plain`] request.
///
/// TC18 §13.7.6.3 states the PWM_IN request (like its response) contains a
/// `byte_msg_payload` of "two 16 bit values", but — unlike the response,
/// whose two values §13.7.6.3 names explicitly (measured active time,
/// period) — never states what the request's own two values mean. This
/// crate also has not resolved [`PwmDurationPair`]'s own wire-level width,
/// units, or byte framing (see this module's doc comment "Provenance note:
/// field widths and units"). Per this module's doc comment "Provenance
/// note: evt[2:0] request validation", this type therefore carries the
/// payload bytes exactly as received, unparsed, mirroring
/// [`crate::i2c::I2cByteTransfer`]'s identical "framing not yet confirmed"
/// discipline rather than guessing at a [`PwmDurationPair`] decode.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
//fusa:req REQ-PWMI-004
pub struct PwmInByteTransfer {
    /// The raw `byte_msg_payload` bytes, unmodified and unframed.
    pub bytes: Vec<u8>,
}

impl PwmInByteTransfer {
    /// Encode this transfer to its raw wire representation: `bytes`,
    /// unmodified and unframed.
    //fusa:req REQ-PWMI-004
    pub fn encode(&self) -> Vec<u8> {
        self.bytes.clone()
    }

    /// Decode a [`PwmInByteTransfer`] from a byte slice.
    ///
    /// Every possible byte slice, including an empty one, is a valid
    /// transfer, so this never fails and never panics for any input.
    //fusa:req REQ-PWMI-004
    pub fn decode(b: &[u8]) -> Self {
        Self { bytes: b.to_vec() }
    }
}

/// The decoded shape of an incoming PWM_IN request, after validating its
/// `evt[2:0]` sub-opcode against TC18 §13.5 Table 33's Row-2 rule (PWM_IN is
/// one of that row's eight endpoint types —
/// `{ADC, PWM_IN, I²C, LIN, CAN, UART, ISELED, MDIO}`).
///
/// See this module's doc comment "Provenance note: evt[2:0] request
/// validation" for the full citation, why [`PwmInRequest::Plain`] carries a
/// raw [`PwmInByteTransfer`] rather than either a decoded
/// [`PwmDurationPair`] or no payload at all, and
/// [`crate::evtgroup`]'s own doc comment for the literal-text discrepancy
/// this crate resolves `evt[2:0] == 000b` against.
#[derive(Debug, Clone, PartialEq, Eq)]
//fusa:req REQ-PWMI-004
pub enum PwmInRequest {
    /// `evt[2:0] == 000b`: an ordinary PWM_IN request. TC18 §13.7.6.3: "The
    /// PWM_IN request and response contain two 16 bit values in the
    /// payload." — carried here as a raw, undecoded [`PwmInByteTransfer`].
    Plain(PwmInByteTransfer),
    /// `evt[2:0] == 111b`: a functional-config write (TC18 §12.7.1) rather
    /// than an ordinary request. This crate does not yet decode the
    /// config-write payload shape itself — see this module's doc comment
    /// "Deliberately out of scope" — so a caller receiving this variant
    /// knows only that the request *is* a config-write, not its content.
    ConfigWrite,
}

impl PwmInRequest {
    /// Decode an incoming PWM_IN request from its `evt.sub_opcode`
    /// ([`crate::acf::Evt::sub_opcode`]) and raw `byte_msg_payload` bytes.
    ///
    /// Returns `Err(`[`RcpError::UnsupportedCmd`]`)` for every
    /// [`EvtRow2Kind::Reserved`] sub_opcode value — TC18 §13.5 Table 33's
    /// Row-2 rule requires the request be rejected with error code
    /// `UNSUPPORTED_CMD`, matching
    /// [`crate::i2c::I2cRequest::from_evt_sub_opcode`]'s/
    /// [`crate::adc::AdcRequest::from_evt_sub_opcode`]'s identical refusal
    /// of their own table's reserved code. Never panics for any
    /// `sub_opcode`/`payload` combination — [`PwmInByteTransfer::decode`] is
    /// itself infallible and total over every byte slice.
    //fusa:req REQ-PWMI-004
    //fusa:req REQ-PWMI-005
    pub fn from_evt_sub_opcode(sub_opcode: u8, payload: &[u8]) -> Result<Self, RcpError> {
        match evt_row2_kind_of(sub_opcode) {
            EvtRow2Kind::Plain => Ok(Self::Plain(PwmInByteTransfer::decode(payload))),
            EvtRow2Kind::ConfigWrite => Ok(Self::ConfigWrite),
            EvtRow2Kind::Reserved => Err(RcpError::UnsupportedCmd),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── PwmDurationPair ───────────────────────────────────────────────────

    #[test]
    //fusa:test REQ-PWM-001
    fn pwm_duration_pair_default_is_zeroed() {
        let pair = PwmDurationPair::default();
        assert_eq!(pair.period, 0);
        assert_eq!(pair.active_duration, 0);
    }

    #[test]
    //fusa:test REQ-PWM-001
    fn pwm_duration_pair_fields_are_independently_settable() {
        let pair = PwmDurationPair {
            period: 1000,
            active_duration: 250,
        };
        assert_eq!(pair.period, 1000);
        assert_eq!(pair.active_duration, 250);
    }

    // ── PwmOutFunctionalConfig / layer_tag ───────────────────────────────

    #[test]
    //fusa:test REQ-PWM-002
    fn pwm_out_functional_config_default_target_is_zeroed_pair() {
        let config = PwmOutFunctionalConfig::default();
        assert_eq!(config.target, PwmDurationPair::default());
    }

    #[test]
    //fusa:test REQ-PWM-003
    fn pwm_out_functional_config_layer_tag_matches_ep_type_pwm_out() {
        let functional = PwmOutFunctionalConfig::default();
        let generic = crate::regmap::PerEpConfigBlock::new(EndpointType::PwmOut);
        let tag = functional.layer_tag();
        assert_eq!(tag.ep_type, EndpointType::PwmOut);
        assert!(crate::regmap::functional_config_matches_ep_type(
            &generic, &tag
        ));
        assert_eq!(
            crate::regmap::check_functional_config_matches_ep_type(&generic, &tag),
            Ok(())
        );
    }

    #[test]
    //fusa:test REQ-PWM-003
    fn pwm_out_functional_config_layer_tag_rejects_mismatched_ep_type() {
        let functional = PwmOutFunctionalConfig::default();
        // In particular, PWM_OUT's tag must not silently match PWM_IN's
        // endpoint — the two are distinct register-map endpoints despite
        // sharing this module's pair shape.
        let generic = crate::regmap::PerEpConfigBlock::new(EndpointType::PwmIn);
        let tag = functional.layer_tag();
        assert!(!crate::regmap::functional_config_matches_ep_type(
            &generic, &tag
        ));
    }

    // ── PwmInFunctionalConfig / layer_tag ────────────────────────────────

    #[test]
    //fusa:test REQ-PWM-004
    fn pwm_in_functional_config_default_timeout_is_zero() {
        let config = PwmInFunctionalConfig::default();
        assert_eq!(config.no_signal_timeout, 0);
    }

    #[test]
    //fusa:test REQ-PWM-005
    fn pwm_in_functional_config_layer_tag_matches_ep_type_pwm_in() {
        let functional = PwmInFunctionalConfig::default();
        let generic = crate::regmap::PerEpConfigBlock::new(EndpointType::PwmIn);
        let tag = functional.layer_tag();
        assert_eq!(tag.ep_type, EndpointType::PwmIn);
        assert!(crate::regmap::functional_config_matches_ep_type(
            &generic, &tag
        ));
        assert_eq!(
            crate::regmap::check_functional_config_matches_ep_type(&generic, &tag),
            Ok(())
        );
    }

    #[test]
    //fusa:test REQ-PWM-005
    fn pwm_in_functional_config_layer_tag_rejects_mismatched_ep_type() {
        let functional = PwmInFunctionalConfig::default();
        let generic = crate::regmap::PerEpConfigBlock::new(EndpointType::PwmOut);
        let tag = functional.layer_tag();
        assert!(!crate::regmap::functional_config_matches_ep_type(
            &generic, &tag
        ));
    }

    // ── resolve_pwm_in_read: PWM_IN_NO_SIGNAL vs. measured vs. in-progress ──

    #[test]
    //fusa:test REQ-PWM-006
    fn resolve_pwm_in_read_reports_measured_within_timeout() {
        let config = PwmInFunctionalConfig {
            no_signal_timeout: 1000,
        };
        let pair = PwmDurationPair {
            period: 500,
            active_duration: 125,
        };
        assert_eq!(
            resolve_pwm_in_read(&config, Some(pair), 0),
            Some(PwmInReadResolution::Measured(pair))
        );
        assert_eq!(
            resolve_pwm_in_read(&config, Some(pair), 999),
            Some(PwmInReadResolution::Measured(pair))
        );
    }

    #[test]
    //fusa:test REQ-PWM-007
    fn resolve_pwm_in_read_reports_no_signal_once_timed_out() {
        let config = PwmInFunctionalConfig {
            no_signal_timeout: 1000,
        };
        assert_eq!(
            resolve_pwm_in_read(&config, None, 1000),
            Some(PwmInReadResolution::NoSignal)
        );
        assert_eq!(
            resolve_pwm_in_read(&config, None, 5000),
            Some(PwmInReadResolution::NoSignal)
        );
    }

    #[test]
    //fusa:test REQ-PWM-007
    fn resolve_pwm_in_read_never_returns_stale_measurement_past_timeout() {
        // A prior measurement exists, but the timeout has since elapsed —
        // the checklist's "instead of ... returning stale data" case.
        let config = PwmInFunctionalConfig {
            no_signal_timeout: 1000,
        };
        let stale = PwmDurationPair {
            period: 500,
            active_duration: 125,
        };
        assert_eq!(
            resolve_pwm_in_read(&config, Some(stale), 1000),
            Some(PwmInReadResolution::NoSignal)
        );
        assert_ne!(
            resolve_pwm_in_read(&config, Some(stale), 1000),
            Some(PwmInReadResolution::Measured(stale))
        );
    }

    #[test]
    //fusa:test REQ-PWM-007
    fn resolve_pwm_in_read_zeroed_config_resolves_no_signal_immediately() {
        // See this module's doc comment: zero is not treated as a
        // "disabled" sentinel for the timeout, mirroring
        // crate::uart::resolve_uart_read_completion's own zero-threshold
        // discipline.
        let config = PwmInFunctionalConfig::default();
        assert_eq!(
            resolve_pwm_in_read(&config, None, 0),
            Some(PwmInReadResolution::NoSignal)
        );
    }

    #[test]
    //fusa:test REQ-PWM-008
    fn resolve_pwm_in_read_returns_none_while_awaiting_first_edge_before_timeout() {
        // Genuinely in progress: no measurement yet, timeout not yet
        // elapsed — this is the "not a hang" case, resolved by returning
        // None for the caller to poll again rather than blocking.
        let config = PwmInFunctionalConfig {
            no_signal_timeout: 1000,
        };
        assert_eq!(resolve_pwm_in_read(&config, None, 0), None);
        assert_eq!(resolve_pwm_in_read(&config, None, 999), None);
    }

    #[test]
    //fusa:test REQ-PWMI-003
    fn resolve_pwm_in_read_invalidates_measurement_once_max_period_is_exceeded() {
        // TC18 §13.7.6.2 Table 45, pwmi_err_on_max_period = 0b (TC18.txt
        // line 4721): "if MAX PERIOD is exceeded, invalidate measurement and
        // wait for new active phase of signal". pwmi_max_period (Table 45,
        // relative address 0x000A, TC18.txt line 4735) is a 16-bit register,
        // so 0xFFFF is the largest MAX PERIOD a conforming RC Server can be
        // configured with.
        let config = PwmInFunctionalConfig {
            no_signal_timeout: 0xFFFF,
        };
        // TC18 §13.7.6.3 (TC18.txt line 4758): both measured values are
        // 16-bit, so a valid measurement fits 0x0000..=0xFFFF.
        let measured = PwmDurationPair {
            period: 0x8000,
            active_duration: 0x4000,
        };
        // One PWM_CLK cycle below MAX PERIOD the measurement is still valid.
        assert_eq!(
            resolve_pwm_in_read(&config, Some(measured), 0xFFFE),
            Some(PwmInReadResolution::Measured(measured))
        );
        // At and beyond MAX PERIOD the measurement is invalid and must never
        // be re-reported.
        for elapsed in [0xFFFFu32, 0x1_0000] {
            assert_eq!(
                resolve_pwm_in_read(&config, Some(measured), elapsed),
                Some(PwmInReadResolution::NoSignal)
            );
        }
    }

    #[test]
    //fusa:test REQ-PWM-009
    fn resolve_pwm_in_read_never_panics_for_any_sampled_input() {
        let configs = [
            PwmInFunctionalConfig {
                no_signal_timeout: 0,
            },
            PwmInFunctionalConfig {
                no_signal_timeout: u32::MAX,
            },
            PwmInFunctionalConfig {
                no_signal_timeout: 1000,
            },
        ];
        let measured_samples = [
            None,
            Some(PwmDurationPair::default()),
            Some(PwmDurationPair {
                period: u32::MAX,
                active_duration: u32::MAX,
            }),
        ];
        let elapsed_samples = [0u32, 1, 1000, u32::MAX];
        for config in &configs {
            for &measured in &measured_samples {
                for &elapsed in &elapsed_samples {
                    let _ = resolve_pwm_in_read(config, measured, elapsed);
                }
            }
        }
    }

    // ── PwmInRequest::from_evt_sub_opcode ────────────────────────────────

    #[test]
    //fusa:test REQ-PWMI-004
    //fusa:test REQ-PWMI-005
    fn pwm_in_request_plain_evt_decodes_payload_as_raw_byte_transfer() {
        // TC18 §13.7.6.3 Figure 29: two 16-bit values in the payload.
        let payload = [0x12, 0x34, 0x56, 0x78];
        let request = PwmInRequest::from_evt_sub_opcode(0b000, &payload).unwrap();
        assert_eq!(
            request,
            PwmInRequest::Plain(PwmInByteTransfer {
                bytes: payload.to_vec()
            })
        );
    }

    #[test]
    //fusa:test REQ-PWMI-004
    //fusa:test REQ-PWMI-005
    fn pwm_in_request_plain_evt_accepts_an_empty_payload() {
        let request = PwmInRequest::from_evt_sub_opcode(0b000, &[]).unwrap();
        assert_eq!(request, PwmInRequest::Plain(PwmInByteTransfer::default()));
    }

    #[test]
    //fusa:test REQ-PWMI-004
    //fusa:test REQ-PWMI-005
    fn pwm_in_request_config_write_evt_is_recognized_without_interpreting_payload() {
        // The payload is not decoded as a PwmInByteTransfer for a
        // config-write request -- the variant carries no payload at all, so
        // garbage bytes here cannot be silently misread as a transfer.
        let request = PwmInRequest::from_evt_sub_opcode(0b111, &[0xDE, 0xAD, 0xBE, 0xEF]).unwrap();
        assert_eq!(request, PwmInRequest::ConfigWrite);
    }

    #[test]
    //fusa:test REQ-PWMI-005
    fn pwm_in_request_reserved_evt_values_are_rejected_with_unsupported_cmd() {
        for sub_opcode in 0b001..=0b110u8 {
            assert_eq!(
                PwmInRequest::from_evt_sub_opcode(sub_opcode, &[]),
                Err(RcpError::UnsupportedCmd)
            );
            assert_eq!(
                PwmInRequest::from_evt_sub_opcode(sub_opcode, &[1, 2, 3]),
                Err(RcpError::UnsupportedCmd)
            );
        }
    }

    #[test]
    //fusa:test REQ-PWMI-005
    fn pwm_in_request_values_above_the_3_bit_field_are_also_rejected_with_unsupported_cmd() {
        for sub_opcode in (crate::acf::EVT_SUB_OPCODE_MAX + 1)..=u8::MAX {
            assert_eq!(
                PwmInRequest::from_evt_sub_opcode(sub_opcode, &[]),
                Err(RcpError::UnsupportedCmd)
            );
        }
    }

    #[test]
    //fusa:test REQ-PWMI-005
    fn pwm_in_request_from_evt_sub_opcode_never_panics_for_any_sampled_input() {
        let payloads: [&[u8]; 3] = [&[], &[0x00], &[0xAA; 32]];
        for sub_opcode in 0..=u8::MAX {
            for payload in payloads {
                let _ = PwmInRequest::from_evt_sub_opcode(sub_opcode, payload);
            }
        }
    }
}
