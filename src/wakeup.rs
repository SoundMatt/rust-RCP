//fusa:req REQ-WAKE-001
//fusa:req REQ-WAKE-002
//fusa:req REQ-WAKE-003
//fusa:req REQ-WAKE-004
//fusa:req REQ-WAKE-005
//fusa:req REQ-WAKE-006
//fusa:req REQ-WAKE-007
//fusa:req REQ-WAKE-008

//! The Wakeup control endpoint type (`ep_type 0x01`) — `ROADMAP.md`
//! Milestone 7 ("Remaining Endpoint Types"), fifth checklist bullet: a
//! fixed `SleepCMD` (`0xA5`) request distinct from the generic request
//! taxonomy, wake-source pin monitoring, and wiring into the
//! Normal/StandBy/Sleep/Unpowered power-mode model [`crate::powerstate`]
//! (Milestone 6) already built.
//!
//! This follows directly on [`crate::lin`], [`crate::can`],
//! [`crate::iseled`], and [`crate::mdio`] (this milestone's first four
//! entries): same milestone, same "additive standalone plumbing only"
//! discipline, same doc-comment provenance-note style for anything this
//! crate has not yet reconciled against confirmed wire behavior. Like
//! ISELED and MDIO, Wakeup control has no old-protocol satellite bridge
//! module in this crate to validate against or migrate away from, so every
//! piece below is new modeling. Unlike every prior Milestone 7 entry, this
//! one has a real, already-built dependency to compose with rather than
//! only [`crate::regmap`]'s functional-config taxonomy:
//! [`crate::powerstate`]'s own doc comment names this exact checklist
//! bullet, verbatim, as the item its `WakeUpHandshakeState` machinery
//! exists to unblock but does not itself implement. Three named pieces are
//! in scope, all implemented here:
//!
//! - [`SleepCmdRequest`] — the fixed `SleepCMD` (`0xA5`) request. See
//!   "Provenance note: `SleepCmdRequest` as its own fixed discriminant, not
//!   a `RequestKind` member" below for why this is a dedicated type rather
//!   than a tenth [`crate::request::RequestKind`] variant.
//! - [`WakeSourcePinMask`] / [`WakeupTriggerConfig`] / [`WakeSourceSignals`]
//!   / [`evaluate_wake_source_signals`] — wake-source pin monitoring,
//!   mirroring [`crate::gpio::GpioBitmask`]/[`crate::gpio::GpioTriggerConfig`]
//!   /[`crate::gpio::GpioTriggerSignals`]/[`crate::gpio::evaluate_gpio_triggers`]'s
//!   own config-plus-evaluator shape for "which pin(s) actually fired."
//!   See "Provenance note: wake-source pin count, width, and push-vs-poll"
//!   below for what this crate does not yet know about the real mechanism.
//! - [`WakeupFunctionalConfig`] — this endpoint type's own per-EP-type
//!   functional-config content, carrying [`WakeupTriggerConfig`] and
//!   composing against [`crate::regmap::check_functional_config_matches_ep_type`]
//!   via [`WakeupFunctionalConfig::layer_tag`], tagged the already-reserved
//!   [`crate::regmap::EndpointType::Wakeup`], exactly as every prior
//!   Milestone 4/7 entry's own config type already does.
//!
//! The wiring into [`crate::powerstate`]'s Normal/StandBy/Sleep/Unpowered
//! model this checklist bullet's own closing clause calls for is
//! [`request_sleep_via_sleep_cmd`] and
//! [`wake_source_signals_trigger_handshake`] — see "Provenance note: two
//! directions of wiring into `crate::powerstate`" below.
//!
//! Deliberately out of scope, for the same reasons every prior Milestone
//! 4/7 entry's own doc comment already gives:
//!
//! - Any ACF/AVTPDU-level framing for [`SleepCmdRequest`], or any byte
//!   offset locating it within a larger request frame. See "Provenance
//!   note: `SleepCmdRequest`'s own framing" below.
//! - Any interpretation of which physical pin a given [`WakeSourcePinMask`]
//!   bit identifies. That is the generic, endpoint-agnostic job of
//!   [`crate::regmap::HwPinMappingEntry`] (§3.7), not this module's —
//!   [`WakeSourcePinMask`] only carries which bit positions fired, exactly
//!   as [`crate::gpio::GpioBitmask`] carries GPIO's own per-pin bits
//!   without naming a physical pin itself.
//! - A real WakeUp message encoder/decoder, or any composition of
//!   [`crate::powerstate::WakeUpHandshakeState`] with a live transport. Both
//!   remain out of scope for the same reason [`crate::powerstate`]'s own
//!   doc comment already gives.
//! - Wiring any of the below into an actual decoder, dispatch loop, or
//!   [`crate::avtp`]/[`crate::acf`]/[`crate::addressing`] caller — matching
//!   the discipline every prior Milestone 1-4/7 entry already established.
//!
//! ## Provenance note: `SleepCmdRequest` as its own fixed discriminant, not
//! a `RequestKind` member
//!
//! `ROADMAP.md`'s checklist bullet states `SleepCMD` (`0xA5`) is "distinct
//! from the generic request taxonomy" — [`crate::request::RequestKind`],
//! the nine-member Standard/Chained/ClearAll/ClearNonSafestate/
//! ClearSingle/Timed/CompoundWait/Triggered/Compound set Milestone 5 built,
//! plus the three Milestone 6 safety-tagged `0x8x` variants layered on top
//! of it. [`SleepCmdRequest`] is accordingly not added to that enum at all:
//! it is this endpoint type's own zero-field marker type, carrying no
//! discriminant range that could collide with or be confused for a
//! `RequestKind` value. Its single named constant,
//! [`SleepCmdRequest::DISCRIMINANT`], is this crate's own reading of the
//! checklist bullet's `0xA5` byte value as the entire content this fixed
//! request carries — there being, per the bullet's own wording, nothing
//! else to it.
//!
//! ## Provenance note: `SleepCmdRequest`'s own framing
//!
//! `ROADMAP.md`'s checklist bullet names the `0xA5` value itself but states
//! no byte offset for where it sits within a larger request frame (an ACF
//! payload, a `RequestKind`-tagged envelope, or something else entirely).
//! Per Guiding Principle 5, [`SleepCmdRequest::encode`]/
//! [`SleepCmdRequest::decode`] do not guess at any such framing: they treat
//! the entire encoded form as exactly the one discriminant byte, leaving
//! any real frame this byte is later found to sit inside for a future
//! transport-level item to add, mirroring how [`crate::mdio::MdioTransfer`]
//! carries its own bytes with no framing assumption beyond what
//! `ROADMAP.md` states.
//!
//! ## Provenance note: wake-source pin count, width, and push-vs-poll
//!
//! `ROADMAP.md`'s checklist bullet names "wake-source pin monitoring" but
//! states neither how many wake-source pins exist, what bit width
//! identifies them, nor whether the mechanism is the endpoint pushing an
//! unsolicited report to the client or the client polling the endpoint for
//! one. Per Guiding Principle 5, three separate working choices are
//! flagged here rather than asserted as spec fact:
//!
//! - [`WakeSourcePinMask`] reuses [`crate::gpio::GpioBitmask`]'s own
//!   4-byte/32-bit width, this crate's own consistency choice rather than a
//!   transcription of a confirmed wake-source pin count.
//! - [`evaluate_wake_source_signals`] reports a pin as fired whenever it is
//!   both armed in [`WakeupTriggerConfig::wake_enable`] and set in a single
//!   caller-supplied observed sample — a level check, not
//!   [`crate::gpio::evaluate_gpio_triggers`]'s own three-way
//!   changed/rising/falling edge detection across a previous/current pair.
//!   `ROADMAP.md`'s checklist bullet states no edge-vs-level semantics for
//!   wake sources the way Milestone 4's GPIO bullet stated GPIO's own
//!   edge-triggered arming, so this module does not transcribe GPIO's
//!   three-way edge model onto a mechanism this crate has no confirmed
//!   basis for reading the same way.
//! - Neither [`WakeupTriggerConfig`] nor [`evaluate_wake_source_signals`]
//!   assumes push or poll: the latter is a pure function over a
//!   caller-supplied "observed" sample, so it composes equally well with a
//!   future item that polls a register or one that decodes an unsolicited
//!   push message — this module commits to neither, matching
//!   [`crate::watchdog`]'s own "opaque tick, no live clock" stance on parts
//!   of its model this crate cannot yet source from a decoder.
//!
//! ## Provenance note: two directions of wiring into `crate::powerstate`
//!
//! `ROADMAP.md`'s checklist bullet's closing clause, "wired into the
//! Normal/StandBy/Sleep/Unpowered model from Milestone 6," names no further
//! detail about which of [`crate::powerstate`]'s pieces either
//! [`SleepCmdRequest`] or wake-source pin monitoring is supposed to drive.
//! Per Guiding Principle 5, this module reads the endpoint's own two named
//! pieces as driving [`crate::powerstate`]'s two opposite-direction moves,
//! rather than inventing a third, endpoint-specific power-mode machine of
//! its own:
//!
//! - [`request_sleep_via_sleep_cmd`] reads a decoded [`SleepCmdRequest`] as
//!   the endpoint-level event that requests a power-down move, composing
//!   directly with [`crate::powerstate::try_enter_power_mode`] rather than
//!   duplicating its transition-shape or gate logic. The caller still
//!   supplies the target [`crate::powerstate::PowerMode`] and
//!   [`crate::powerstate::PowerModeGateInput`] — `SleepCMD` names no target
//!   mode of its own, so this module does not guess between `StandBy` and
//!   `Sleep` on the caller's behalf.
//! - [`wake_source_signals_trigger_handshake`] reads a fired
//!   [`WakeSourceSignals`] as the endpoint-level event that begins the
//!   hot-start-from-Sleep WakeUp handshake, composing directly with
//!   [`crate::powerstate::send_wakeup_request`] rather than duplicating its
//!   `Idle` -> `RequestSent` state check. This is this module's own reading
//!   of [`crate::powerstate`]'s own doc comment, which names this
//!   checklist bullet by name as the item its abstract handshake state
//!   machine exists to be driven from.
//!
//! Neither function owns a `WakeUpHandshakeState`/`PowerMode` field of its
//! own, or advances the acknowledgment half of the handshake
//! ([`crate::powerstate::acknowledge_wakeup_request`]) — this checklist
//! bullet names wake-source pin monitoring as the mechanism that begins a
//! wake, not one that itself carries an acknowledgment. Composing the
//! acknowledgment half remains a future transport-level item's job, per
//! [`crate::powerstate`]'s own doc comment on the handshake's still-unknown
//! wire encoding.
//!
//! ## Relationship to [`crate::regmap`]
//!
//! As with every Milestone 4/7 endpoint-type module, Wakeup control's real
//! functional-config content gets its own dedicated type,
//! [`WakeupFunctionalConfig`], rather than adding Wakeup-specific fields
//! directly onto the still-shared, thirteen-endpoint-type
//! [`crate::regmap::PerEpTypeFunctionalConfig`] placeholder.
//! [`WakeupFunctionalConfig::layer_tag`] shows how a caller obtains the
//! matching generic-layer tag so the two compose through
//! [`crate::regmap::check_functional_config_matches_ep_type`] exactly as
//! that cross-layer rule already expects, without this module editing
//! [`crate::regmap`] itself.

use crate::RcpError;

// ── SleepCmdRequest ──────────────────────────────────────────────────────────

/// The fixed `SleepCMD` request this endpoint type accepts: a single
/// `0xA5` discriminant byte, carrying no further fields and no relation to
/// [`crate::request::RequestKind`].
///
/// See this module's doc comment "Provenance note: `SleepCmdRequest` as
/// its own fixed discriminant, not a `RequestKind` member" for why this is
/// a dedicated marker type rather than a tenth `RequestKind` variant, and
/// "Provenance note: `SleepCmdRequest`'s own framing" for why
/// [`SleepCmdRequest::encode`]/[`SleepCmdRequest::decode`] carry exactly
/// one byte and no surrounding frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-WAKE-001
pub struct SleepCmdRequest;

impl SleepCmdRequest {
    /// The fixed `SleepCMD` wire discriminant this checklist bullet names.
    //fusa:req REQ-WAKE-001
    pub const DISCRIMINANT: u8 = 0xA5;

    /// Encode this request to its one-byte wire representation:
    /// [`SleepCmdRequest::DISCRIMINANT`], always.
    //fusa:req REQ-WAKE-001
    pub fn encode(self) -> [u8; 1] {
        [Self::DISCRIMINANT]
    }

    /// Decode a [`SleepCmdRequest`] from a byte slice.
    ///
    /// Returns `Err(RcpError::ShortFrame)` for an empty slice, and
    /// `Err(RcpError::InvalidParameter)` when the first byte is present but
    /// is not [`SleepCmdRequest::DISCRIMINANT`] — matching
    /// [`crate::mdio::MdioAddressingMode::from_u8`]'s own short-vs-invalid
    /// split. Never panics for any input.
    //fusa:req REQ-WAKE-002
    pub fn decode(b: &[u8]) -> Result<Self, RcpError> {
        match b.first() {
            None => Err(RcpError::ShortFrame),
            Some(&Self::DISCRIMINANT) => Ok(Self),
            Some(_) => Err(RcpError::InvalidParameter),
        }
    }
}

// ── WakeSourcePinMask ────────────────────────────────────────────────────────

/// The wake-source pin bitmask width this module uses throughout: 4 bytes,
/// matching [`crate::gpio::GpioBitmask`]'s own width. See this module's doc
/// comment "Provenance note: wake-source pin count, width, and
/// push-vs-poll" for why this is this crate's own consistency choice, not a
/// transcribed wake-source pin count.
pub const WAKE_SOURCE_PIN_MASK_LEN: usize = 4;

/// A per-pin wake-source bitmask: bit `n` corresponds to wake-source pin
/// `n`. Carries no physical-pin identity of its own — see this module's doc
/// comment for why that mapping is [`crate::regmap::HwPinMappingEntry`]'s
/// job, not this type's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-WAKE-003
pub struct WakeSourcePinMask(pub u32);

impl WakeSourcePinMask {
    /// Encode this bitmask to its 4-byte big-endian wire representation.
    //fusa:req REQ-WAKE-003
    pub fn encode(self) -> [u8; WAKE_SOURCE_PIN_MASK_LEN] {
        self.0.to_be_bytes()
    }

    /// Decode a [`WakeSourcePinMask`] from a byte slice.
    ///
    /// Never panics on short, truncated, or arbitrary input — always
    /// returns `Err(RcpError::ShortFrame)` for input shorter than
    /// [`WAKE_SOURCE_PIN_MASK_LEN`] instead. Trailing bytes beyond the
    /// first four are ignored, matching [`crate::gpio::GpioBitmask::decode`]'s
    /// own handling of a longer-than-required slice.
    //fusa:req REQ-WAKE-003
    pub fn decode(b: &[u8]) -> Result<Self, RcpError> {
        if b.len() < WAKE_SOURCE_PIN_MASK_LEN {
            return Err(RcpError::ShortFrame);
        }
        Ok(Self(u32::from_be_bytes([b[0], b[1], b[2], b[3]])))
    }
}

// ── WakeupTriggerConfig / WakeSourceSignals / evaluate_wake_source_signals ───

/// Which wake-source pins are armed to be monitored.
///
/// A single armed-pin mask, deliberately not GPIO's own three-mask
/// change/rising/falling arming shape — see this module's doc comment
/// "Provenance note: wake-source pin count, width, and push-vs-poll" for
/// why this module does not transcribe GPIO's edge-triggered model onto a
/// mechanism this crate has no confirmed basis for reading the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-WAKE-004
pub struct WakeupTriggerConfig {
    /// Per-pin bitmask: this pin is armed to be monitored as a wake source.
    pub wake_enable: WakeSourcePinMask,
}

impl WakeupTriggerConfig {
    /// Encode this config to its 4-byte big-endian wire representation:
    /// [`WakeupTriggerConfig::wake_enable`], unmodified.
    //fusa:req REQ-WAKE-004
    pub fn encode(self) -> [u8; WAKE_SOURCE_PIN_MASK_LEN] {
        self.wake_enable.encode()
    }

    /// Decode a [`WakeupTriggerConfig`] from a byte slice.
    ///
    /// Never panics on short, truncated, or arbitrary input — always
    /// returns `Err(RcpError::ShortFrame)` for input shorter than
    /// [`WAKE_SOURCE_PIN_MASK_LEN`] instead, delegating to
    /// [`WakeSourcePinMask::decode`].
    //fusa:req REQ-WAKE-004
    pub fn decode(b: &[u8]) -> Result<Self, RcpError> {
        Ok(Self {
            wake_enable: WakeSourcePinMask::decode(b)?,
        })
    }
}

/// Which wake-source pins actually fired, as reported by
/// [`evaluate_wake_source_signals`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-WAKE-005
pub struct WakeSourceSignals {
    /// Per-pin bitmask: this pin was armed and observed asserted.
    pub fired: WakeSourcePinMask,
}

impl WakeSourceSignals {
    /// True if any pin fired at all — `fired`'s underlying bitmask is
    /// nonzero. Never panics for any input.
    //fusa:req REQ-WAKE-005
    pub fn any_fired(self) -> bool {
        self.fired.0 != 0
    }
}

/// Compute which wake-source pins fired: every pin bit that is both armed
/// in `config` and set in `observed`.
///
/// A level check against one caller-supplied sample, not an edge check
/// across a previous/current pair — see this module's doc comment
/// "Provenance note: wake-source pin count, width, and push-vs-poll" for
/// why. Never panics for any input.
//fusa:req REQ-WAKE-005
pub fn evaluate_wake_source_signals(
    config: &WakeupTriggerConfig,
    observed: WakeSourcePinMask,
) -> WakeSourceSignals {
    WakeSourceSignals {
        fired: WakeSourcePinMask(observed.0 & config.wake_enable.0),
    }
}

// ── WakeupFunctionalConfig ───────────────────────────────────────────────────

/// Wakeup control's own per-EP-type functional-config content: this
/// endpoint's [`WakeupTriggerConfig`] arming.
///
/// See this module's doc comment "Relationship to `crate::regmap`" for why
/// this is a dedicated type rather than content added directly to
/// [`crate::regmap::PerEpTypeFunctionalConfig`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-WAKE-006
pub struct WakeupFunctionalConfig {
    /// This endpoint's per-pin wake-source trigger arming.
    pub trigger: WakeupTriggerConfig,
}

impl WakeupFunctionalConfig {
    /// The [`crate::regmap::PerEpTypeFunctionalConfig`] generic-layer tag
    /// that matches this Wakeup functional config, for use with
    /// [`crate::regmap::check_functional_config_matches_ep_type`].
    ///
    /// This module does not itself call that function — it only shows how a
    /// caller would obtain the matching tag, per this module's doc comment
    /// "Relationship to `crate::regmap`".
    //fusa:req REQ-WAKE-006
    pub fn layer_tag(&self) -> crate::regmap::PerEpTypeFunctionalConfig {
        crate::regmap::PerEpTypeFunctionalConfig::new(crate::regmap::EndpointType::Wakeup)
    }
}

// ── Wiring into crate::powerstate ────────────────────────────────────────────

/// Request a power-down move via a decoded [`SleepCmdRequest`], composing
/// directly with [`crate::powerstate::try_enter_power_mode`].
///
/// `_cmd` is accepted (rather than the function taking no argument at all)
/// to make the composition's precondition explicit at every call site: a
/// caller must actually hold a decoded `SleepCMD` before this move is
/// attempted. Neither the target `to` mode nor `gate` is inferred from
/// `_cmd` itself — see this module's doc comment "Provenance note: two
/// directions of wiring into `crate::powerstate`" for why `SleepCMD` names
/// no target mode of its own. Returns whatever
/// [`crate::powerstate::try_enter_power_mode`] returns; never panics for
/// any input.
///
/// Because [`crate::powerstate::try_enter_power_mode`] admits the move only
/// when both [`crate::powerstate::PowerModeGateInput`] flags hold, a
/// `SleepCMD` reaches sleep mode only "as soon as all EPs are idle and the
/// responder queues are empty (all responses sent)" — TC18 §13.7.2.3's
/// third sleep-sequence step.
//fusa:req REQ-WAKE-007
//fusa:req REQ-WAKE-009
pub fn request_sleep_via_sleep_cmd(
    _cmd: SleepCmdRequest,
    from: crate::powerstate::PowerMode,
    to: crate::powerstate::PowerMode,
    gate: crate::powerstate::PowerModeGateInput,
) -> Result<crate::powerstate::PowerMode, RcpError> {
    crate::powerstate::try_enter_power_mode(from, to, gate)
}

/// Begin the hot-start-from-Sleep WakeUp handshake from a fired
/// [`WakeSourceSignals`], composing directly with
/// [`crate::powerstate::send_wakeup_request`].
///
/// Returns `Err(RcpError::RequestRejected)` when `signals` reports no fired
/// pin at all ([`WakeSourceSignals::any_fired`] is `false`) — there is no
/// wake event to begin a handshake from. Otherwise delegates to
/// [`crate::powerstate::send_wakeup_request`], which itself only succeeds
/// from [`crate::powerstate::WakeUpHandshakeState::Idle`]. Never panics for
/// any input.
//fusa:req REQ-WAKE-008
pub fn wake_source_signals_trigger_handshake(
    state: crate::powerstate::WakeUpHandshakeState,
    signals: WakeSourceSignals,
) -> Result<crate::powerstate::WakeUpHandshakeState, RcpError> {
    if !signals.any_fired() {
        return Err(RcpError::RequestRejected);
    }
    crate::powerstate::send_wakeup_request(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::powerstate::{PowerMode, PowerModeGateInput, WakeUpHandshakeState};

    // ── SleepCmdRequest: encode/decode round-trip, never panic ─────────────

    #[test]
    //fusa:test REQ-WAKE-001
    fn sleep_cmd_request_encodes_to_the_fixed_0xa5_discriminant() {
        assert_eq!(SleepCmdRequest.encode(), [0xA5u8]);
        assert_eq!(SleepCmdRequest::DISCRIMINANT, 0xA5);
    }

    #[test]
    //fusa:test REQ-WAKE-001
    fn sleep_cmd_request_round_trips_through_encode_decode() {
        let req = SleepCmdRequest;
        assert_eq!(SleepCmdRequest::decode(&req.encode()), Ok(req));
    }

    #[test]
    //fusa:test REQ-WAKE-002
    fn sleep_cmd_request_decode_rejects_empty_input() {
        assert_eq!(SleepCmdRequest::decode(&[]), Err(RcpError::ShortFrame));
    }

    #[test]
    //fusa:test REQ-WAKE-002
    fn sleep_cmd_request_decode_rejects_any_non_0xa5_first_byte() {
        for raw in [0x00u8, 0x01, 0x5A, 0xA4, 0xA6, 0xFF] {
            assert_eq!(
                SleepCmdRequest::decode(&[raw]),
                Err(RcpError::InvalidParameter)
            );
        }
    }

    #[test]
    //fusa:test REQ-WAKE-002
    fn sleep_cmd_request_decode_never_panics_for_any_sampled_input() {
        for buf in [
            vec![],
            vec![0xA5],
            vec![0xA5, 0xFF, 0xFF],
            vec![0x00; 8],
            (0u8..=255).collect::<Vec<_>>(),
        ] {
            let _ = SleepCmdRequest::decode(&buf);
        }
    }

    // ── WakeSourcePinMask: round-trip / never-panic ─────────────────────────

    #[test]
    //fusa:test REQ-WAKE-003
    fn wake_source_pin_mask_round_trips_through_encode_decode() {
        for raw in [0u32, 1, 0x0000_0001, 0x8000_0000, 0xFFFF_FFFF, 0x1234_5678] {
            let mask = WakeSourcePinMask(raw);
            assert_eq!(WakeSourcePinMask::decode(&mask.encode()), Ok(mask));
        }
    }

    #[test]
    //fusa:test REQ-WAKE-003
    fn wake_source_pin_mask_decode_rejects_short_input() {
        for len in 0..WAKE_SOURCE_PIN_MASK_LEN {
            let buf = vec![0xAAu8; len];
            assert_eq!(WakeSourcePinMask::decode(&buf), Err(RcpError::ShortFrame));
        }
    }

    #[test]
    //fusa:test REQ-WAKE-003
    fn wake_source_pin_mask_decode_ignores_trailing_bytes() {
        let buf = [0x00, 0x00, 0x00, 0x2A, 0xFF, 0xFF];
        assert_eq!(WakeSourcePinMask::decode(&buf), Ok(WakeSourcePinMask(0x2A)));
    }

    #[test]
    //fusa:test REQ-WAKE-003
    fn wake_source_pin_mask_decode_never_panics_for_any_sampled_input() {
        for len in [0usize, 1, 2, 3, 4, 5, 9, 64] {
            let buf = vec![0x5Au8; len];
            let _ = WakeSourcePinMask::decode(&buf);
        }
    }

    // ── WakeupTriggerConfig: round-trip / never-panic ───────────────────────

    #[test]
    //fusa:test REQ-WAKE-004
    fn wakeup_trigger_config_round_trips_through_encode_decode() {
        let cfg = WakeupTriggerConfig {
            wake_enable: WakeSourcePinMask(0x0000_0007),
        };
        assert_eq!(WakeupTriggerConfig::decode(&cfg.encode()), Ok(cfg));
    }

    #[test]
    //fusa:test REQ-WAKE-004
    fn wakeup_trigger_config_decode_rejects_short_input() {
        for len in 0..WAKE_SOURCE_PIN_MASK_LEN {
            let buf = vec![0x00u8; len];
            assert_eq!(WakeupTriggerConfig::decode(&buf), Err(RcpError::ShortFrame));
        }
    }

    #[test]
    //fusa:test REQ-WAKE-004
    fn wakeup_trigger_config_default_arms_no_pins() {
        assert_eq!(
            WakeupTriggerConfig::default().wake_enable,
            WakeSourcePinMask(0)
        );
    }

    // ── evaluate_wake_source_signals ─────────────────────────────────────

    #[test]
    //fusa:test REQ-WAKE-005
    fn evaluate_wake_source_signals_reports_only_armed_and_observed_pins() {
        let config = WakeupTriggerConfig {
            wake_enable: WakeSourcePinMask(0b1010),
        };
        let signals = evaluate_wake_source_signals(&config, WakeSourcePinMask(0b1111));
        assert_eq!(signals.fired, WakeSourcePinMask(0b1010));
        assert!(signals.any_fired());
    }

    #[test]
    //fusa:test REQ-WAKE-005
    fn evaluate_wake_source_signals_masks_out_disarmed_pins() {
        let config = WakeupTriggerConfig {
            wake_enable: WakeSourcePinMask(0b0000),
        };
        let signals = evaluate_wake_source_signals(&config, WakeSourcePinMask(0xFFFF_FFFF));
        assert_eq!(signals.fired, WakeSourcePinMask(0));
        assert!(!signals.any_fired());
    }

    #[test]
    //fusa:test REQ-WAKE-005
    fn evaluate_wake_source_signals_no_signal_when_nothing_observed() {
        let config = WakeupTriggerConfig {
            wake_enable: WakeSourcePinMask(0xFFFF_FFFF),
        };
        let signals = evaluate_wake_source_signals(&config, WakeSourcePinMask(0));
        assert_eq!(signals, WakeSourceSignals::default());
        assert!(!signals.any_fired());
    }

    #[test]
    //fusa:test REQ-WAKE-005
    fn evaluate_wake_source_signals_never_panics_for_any_sampled_input() {
        for armed in [0u32, 0x5555_5555, 0xFFFF_FFFF] {
            for observed in [0u32, 0xAAAA_AAAA, 0xFFFF_FFFF] {
                let config = WakeupTriggerConfig {
                    wake_enable: WakeSourcePinMask(armed),
                };
                let _ = evaluate_wake_source_signals(&config, WakeSourcePinMask(observed));
            }
        }
    }

    // ── WakeupFunctionalConfig / layer_tag ──────────────────────────────────

    #[test]
    //fusa:test REQ-WAKE-006
    fn wakeup_functional_config_default_arms_no_pins_and_layer_tag_matches_ep_type_wakeup() {
        let functional = WakeupFunctionalConfig::default();
        assert_eq!(functional.trigger, WakeupTriggerConfig::default());

        let generic = crate::regmap::PerEpConfigBlock::new(crate::regmap::EndpointType::Wakeup);
        let tag = functional.layer_tag();
        assert_eq!(tag.ep_type, crate::regmap::EndpointType::Wakeup);
        assert!(crate::regmap::functional_config_matches_ep_type(
            &generic, &tag
        ));
        assert_eq!(
            crate::regmap::check_functional_config_matches_ep_type(&generic, &tag),
            Ok(())
        );
    }

    #[test]
    //fusa:test REQ-WAKE-006
    fn wakeup_functional_config_layer_tag_rejects_mismatched_ep_type() {
        let functional = WakeupFunctionalConfig {
            trigger: WakeupTriggerConfig {
                wake_enable: WakeSourcePinMask(1),
            },
        };
        let generic = crate::regmap::PerEpConfigBlock::new(crate::regmap::EndpointType::Gpio);
        let tag = functional.layer_tag();
        assert!(!crate::regmap::functional_config_matches_ep_type(
            &generic, &tag
        ));
    }

    // ── request_sleep_via_sleep_cmd ──────────────────────────────────────

    #[test]
    //fusa:test REQ-WAKE-007
    fn request_sleep_via_sleep_cmd_succeeds_for_a_defined_gated_transition() {
        let gate = PowerModeGateInput {
            all_endpoints_idle: true,
            no_pending_response: true,
        };
        assert_eq!(
            request_sleep_via_sleep_cmd(
                SleepCmdRequest,
                PowerMode::Normal,
                PowerMode::StandBy,
                gate
            ),
            Ok(PowerMode::StandBy)
        );
        // TC18 §12.4 Figure 17's other "Go to ..." edge.
        assert_eq!(
            request_sleep_via_sleep_cmd(SleepCmdRequest, PowerMode::Normal, PowerMode::Sleep, gate),
            Ok(PowerMode::Sleep)
        );
    }

    #[test]
    //fusa:test REQ-WAKE-007
    fn request_sleep_via_sleep_cmd_rejects_an_undefined_transition() {
        let gate = PowerModeGateInput {
            all_endpoints_idle: true,
            no_pending_response: true,
        };
        // Figure 17 draws no edge between the two low-power modes: both
        // are entered from `Normal` only.
        assert_eq!(
            request_sleep_via_sleep_cmd(
                SleepCmdRequest,
                PowerMode::StandBy,
                PowerMode::Sleep,
                gate
            ),
            Err(RcpError::RequestRejected)
        );
        assert_eq!(
            request_sleep_via_sleep_cmd(
                SleepCmdRequest,
                PowerMode::Normal,
                PowerMode::Unpowered,
                gate
            ),
            Err(RcpError::RequestRejected)
        );
    }

    #[test]
    //fusa:test REQ-WAKE-007
    fn request_sleep_via_sleep_cmd_rejects_when_not_gated() {
        let gate = PowerModeGateInput::default();
        assert_eq!(
            request_sleep_via_sleep_cmd(
                SleepCmdRequest,
                PowerMode::Normal,
                PowerMode::StandBy,
                gate
            ),
            Err(RcpError::RequestRejected)
        );
    }

    /// TC18 §13.7.2.3 (TC18.txt lines 4151-4158): based on the sleep
    /// request the RC Server brings the implementation to sleep mode only
    /// "as soon as all EPs are idle and the responder queues are empty (all
    /// responses sent)". Both conjuncts are required; neither alone admits
    /// the move.
    #[test]
    //fusa:test REQ-WAKE-009
    fn sleep_cmd_reaches_sleep_only_when_all_eps_idle_and_responder_queues_empty() {
        // TC18 §13.7.2.3's two conditions, enumerated exhaustively.
        let cases = [
            (false, false, false),
            (false, true, false),
            (true, false, false),
            (true, true, true),
        ];
        for (all_endpoints_idle, no_pending_response, admitted) in cases {
            let gate = PowerModeGateInput {
                all_endpoints_idle,
                no_pending_response,
            };
            let result = request_sleep_via_sleep_cmd(
                SleepCmdRequest,
                PowerMode::Normal,
                PowerMode::Sleep,
                gate,
            );
            if admitted {
                assert_eq!(result, Ok(PowerMode::Sleep));
            } else {
                assert_eq!(result, Err(RcpError::RequestRejected));
            }
        }
    }

    // ── wake_source_signals_trigger_handshake ───────────────────────────

    #[test]
    //fusa:test REQ-WAKE-008
    fn wake_source_signals_trigger_handshake_advances_idle_to_request_sent_when_fired() {
        let signals = WakeSourceSignals {
            fired: WakeSourcePinMask(1),
        };
        assert_eq!(
            wake_source_signals_trigger_handshake(WakeUpHandshakeState::Idle, signals),
            Ok(WakeUpHandshakeState::RequestSent)
        );
    }

    #[test]
    //fusa:test REQ-WAKE-008
    fn wake_source_signals_trigger_handshake_rejects_when_nothing_fired() {
        let signals = WakeSourceSignals::default();
        assert_eq!(
            wake_source_signals_trigger_handshake(WakeUpHandshakeState::Idle, signals),
            Err(RcpError::RequestRejected)
        );
    }

    #[test]
    //fusa:test REQ-WAKE-008
    fn wake_source_signals_trigger_handshake_rejects_when_handshake_not_idle() {
        let signals = WakeSourceSignals {
            fired: WakeSourcePinMask(1),
        };
        for state in [
            WakeUpHandshakeState::RequestSent,
            WakeUpHandshakeState::Acknowledged,
        ] {
            assert_eq!(
                wake_source_signals_trigger_handshake(state, signals),
                Err(RcpError::RequestRejected)
            );
        }
    }

    #[test]
    //fusa:test REQ-WAKE-008
    fn wake_source_signals_trigger_handshake_never_panics_for_any_sampled_input() {
        for state in [
            WakeUpHandshakeState::Idle,
            WakeUpHandshakeState::RequestSent,
            WakeUpHandshakeState::Acknowledged,
        ] {
            for fired in [0u32, 1, 0xFFFF_FFFF] {
                let signals = WakeSourceSignals {
                    fired: WakeSourcePinMask(fired),
                };
                let _ = wake_source_signals_trigger_handshake(state, signals);
            }
        }
    }
}
