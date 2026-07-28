// fusa:req REQ-RMAP-001
// fusa:req REQ-RMAP-002
// fusa:req REQ-RMAP-003
// fusa:req REQ-RMAP-004
// fusa:req REQ-RMAP-005
// fusa:req REQ-RMAP-006

//! Three-layer per-endpoint config taxonomy — TC18 register-map model
//! (`ROADMAP.md` Milestone 2, "Register Map" subsection, first item).
//!
//! Per Guiding Principle 2 ("sequence work so nothing is built on a
//! foundation that will itself change later ... lifecycle model and
//! register-map split before endpoints"), this item establishes the
//! *shape* of the register map's per-endpoint config before either the
//! concrete field names ("Register Map" subsection's next two checklist
//! bullets, `§3.6` general fields and `§3.7`-`§3.11` config tables) or any
//! endpoint-type work (Milestones 4 and 7) exist. It deliberately invents
//! no concrete field content beyond the one tag ([`EndpointType`]) that the
//! taxonomy itself needs to distinguish its layers — see "Provenance note"
//! below.
//!
//! [`ROADMAP.md`]'s own checklist bullet names three distinct layers, not
//! the old crate's single flat `ep_type`-less config model:
//!
//! - [`PerEpConfigBlock`] — the generic, server-owned per-EP config block:
//!   present for *every* endpoint regardless of [`EndpointType`]. This
//!   module gives it exactly one field, `ep_type`, itself: the register
//!   map's own per-endpoint type discriminant, which every endpoint must
//!   carry so that a [`PerEpTypeFunctionalConfig`] can ever be selected for
//!   it in the first place. Every *other* generic field the eventual
//!   `§3.6`/`§3.7`-`§3.11` bullets will define is deliberately left out —
//!   this item claims only the tag this taxonomy itself depends on, not
//!   the rest of that later work.
//! - [`CommonFunctionalConfig`] — the common functional-config block:
//!   fields shared across *every* [`EndpointType`]'s functional config.
//!   `ROADMAP.md`'s own Milestone 4 success-criteria text names three
//!   examples for this layer — `ep_enable`, `ep_clear_req_storage`,
//!   `ep_req_crc_enable` — but none of the three is modeled as a concrete
//!   field here; this type is an empty placeholder standing in for that
//!   still-unbuilt shape, mirroring how [`crate::lifecycle::RegisterCategory`]
//!   stood in for the whole register map ahead of this very subsection.
//! - [`PerEpTypeFunctionalConfig`] — a distinct, type-specific config shape
//!   for each [`EndpointType`]. Tagged by [`EndpointType`] rather than
//!   given thirteen separate concrete shapes, since no endpoint type's
//!   actual functional-config fields (GPIO's eight write-semantics, SPI's
//!   up to six channel configs, etc.) exist in this crate yet — that is
//!   Milestone 4's (six of the thirteen) and Milestone 7's (the remaining
//!   seven, one of which, DAC, is explicitly deferred rather than
//!   implemented) job, not this item's.
//!
//! [`functional_config_matches_ep_type`]/
//! [`check_functional_config_matches_ep_type`] give the taxonomy its first
//! real cross-layer rule: a [`PerEpTypeFunctionalConfig`] only validly
//! belongs to an endpoint whose [`PerEpConfigBlock::ep_type`] matches it.
//! This is the one relationship the three layers already have to each
//! other even before any concrete field exists — later endpoint-type work
//! builds on top of it rather than reinventing it.
//!
//! ## Relationship to [`crate::lifecycle::RegisterCategory`]
//!
//! [`crate::lifecycle::RegisterCategory`] (`General`/`HwConfig`/`RcpConfig`)
//! and this module's three-layer taxonomy are two *different, orthogonal*
//! classifications over the same eventual register map, not competing
//! models of it:
//!
//! - `RegisterCategory` answers "when, lifecycle-state-wise, is this
//!   register reachable/writable at all" — see
//!   [`crate::lifecycle::is_register_reachable`]/
//!   [`crate::lifecycle::is_register_writable`].
//! - This module's taxonomy answers a structurally different question:
//!   "whose config is this register, and how many endpoints share its
//!   shape" — generic-to-every-endpoint, common-to-every-functional-config,
//!   or specific to one [`EndpointType`].
//!
//! Both axes necessarily apply *simultaneously* to any concrete register
//! the later `§3.6`-`§3.11` bullets define: a real GPIO write-semantics
//! field, for instance, is both "`PerEpTypeFunctionalConfig` for
//! `EndpointType::Gpio`" (this module's axis) and reachable/writable
//! according to *some* `RegisterCategory` (`crate::lifecycle`'s axis).
//! [`ConfigLayer`]/[`register_category`] give this crate's own provisional
//! guess at how the two axes line up, so that [`crate::ep0`]'s existing
//! `RegisterCategory`-granularity access checks have *something* to
//! consult for a taxonomy-layer register before the concrete Register Map
//! exists — see "Provenance note" below for the very different confidence
//! levels behind the [`ConfigLayer::Generic`] guess versus the two
//! functional-layer guesses.
//!
//! ## Relationship to [`crate::ep0`]
//!
//! [`EndpointType`] intentionally has **no** variant for EP0 itself. EP0 is
//! addressed structurally, by the reserved `byte_bus_id 0`
//! ([`crate::ep0::EP0_BYTE_BUS_ID`]) — it is the RC Server acting as a
//! pseudo-endpoint over its own whole register map, not a device-facing
//! endpoint with a register-map `ep_type` value of its own. `ROADMAP.md`
//! itself draws the same line: Milestone 7's success criteria describes
//! "thirteen defined endpoint types (EP0 + Wakeup + eleven device-facing
//! types)" as a *human* headcount that includes EP0 informally, while its
//! own per-bullet `ep_type` citations (`0x01` through `0x0D`, thirteen
//! *numeric* codes counting Wakeup through MDIO) never assign EP0 a code at
//! all. [`EndpointType`] models the latter, numeric-`ep_type` enumeration —
//! the thirteen codes `0x01`-`0x0D` — not the former headcount; see
//! "Provenance note" below for why this crate reads "thirteen" as
//! referring to two different countable sets depending on context, and
//! flags rather than silently resolves that apparent mismatch.
//!
//! This module performs no register I/O, does not wire itself into
//! [`crate::ep0`], [`crate::lifecycle`], or any other existing caller, and
//! is purely additive standalone plumbing, matching the discipline every
//! prior Milestone 1/2 entry already established.
//!
//! ## Provenance note
//!
//! The thirteen `ep_type` numeric codes `EndpointType` enumerates
//! (`0x01`-`0x0D`) are taken directly from `ROADMAP.md`'s own Milestone 4
//! and Milestone 7 checklist bullets, which in turn cite the OPEN Alliance
//! TC18 Remote Control Protocol Specification v0.5.1_RC by name only, never
//! by section number, for this particular "Register Map" checklist item —
//! unlike the sibling bullets a few lines further down the same subsection,
//! whose text already cites `§3.6`-`§3.11`. This module's own doc comments
//! therefore cite no `§3.x` section number for the taxonomy itself, matching
//! [`crate::lifecycle`]'s and [`crate::ep0`]'s own precedent for
//! `ROADMAP.md` subsections with no recorded section number yet.
//! [`EndpointType::Dac`] is enumerated (not omitted) because `ROADMAP.md`'s
//! own Milestone 7 bullet says the DAC type code itself "exist[s] in the
//! register-map enumeration" even though it names the type "reserved and
//! out of scope for this cycle" — [`EndpointType::is_reserved`] gives that
//! explicit decision a structural, queryable form rather than leaving
//! `Dac`'s special status an unremarked comment.
//!
//! [`PerEpConfigBlock`], [`CommonFunctionalConfig`], and
//! [`PerEpTypeFunctionalConfig`] are this crate's own structural reading of
//! the checklist bullet's three-layer wording, not a transcription of any
//! specified register layout — no concrete field beyond the `ep_type` tag
//! is invented, per this module's doc comment above.
//!
//! [`ConfigLayer`]/[`register_category`]'s mapping from a taxonomy layer to
//! a [`crate::lifecycle::RegisterCategory`] goes a step further and is
//! flagged, per Guiding Principle 5, at two distinctly different confidence
//! levels:
//!
//! - `CommonFunctional` and `PerTypeFunctional` both mapping to
//!   [`crate::lifecycle::RegisterCategory::RcpConfig`] has real textual
//!   support: both taxonomy layers are named with the word "functional" in
//!   the checklist bullet itself ("common **functional**-config block",
//!   "per-EP-type **functional** config"), which echoes
//!   `RegisterCategory::RcpConfig`'s own doc comment verbatim
//!   ("Registers configuring RCP-level/**functional** behavior") — the same
//!   kind of naming-echo evidence [`crate::lifecycle`]'s own doc comment
//!   already relied on for its `HwConfig`/`RcpConfig` split.
//! - `Generic` mapping to [`crate::lifecycle::RegisterCategory::HwConfig`]
//!   is a **weaker** guess with no equivalent naming echo to point to: it
//!   rests only on the looser analogy that "generic (**server-owned**)"
//!   config, assigned once per endpoint rather than tuned at runtime,
//!   plausibly belongs to the same "foundation, assigned during hardware
//!   configuration" role `crate::lifecycle`'s own doc comment already
//!   assigns to `HwConfig`. This crate flags the confidence gap explicitly
//!   rather than presenting both mappings as equally well-evidenced.
//!
//! Both mappings remain this crate's own working interpretation, pending
//! reconciliation against the specification's actual behavior (never its
//! prose) before being relied on for interop with a real TC18 RC Server —
//! and neither mapping is wired into [`crate::ep0::check_ep0_access`] or
//! any other existing caller by this item.
//!
//! `RcpError::EndpointTypeMismatch` is this crate's own provisional error
//! name for [`check_functional_config_matches_ep_type`], matching the
//! pre-Error-Model-item style already used by
//! `RcpError::RegisterUnreachable`/`RcpError::RegisterLocked`/
//! `RcpError::RootClientRequired` before it; which of the specification's
//! own error codes it ultimately maps to is `ROADMAP.md` Milestone 2's
//! later "Error Model" item's call to make, not this one's.

use crate::lifecycle::RegisterCategory;
use crate::RcpError;

// ── EndpointType ─────────────────────────────────────────────────────────────

/// The register map's own per-endpoint type discriminant ("`ep_type`"), per
/// `ROADMAP.md` Milestones 4 and 7.
///
/// See this module's doc comment for why EP0 has no variant here, and for
/// [`EndpointType::Dac`]'s reserved status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
// fusa:req REQ-RMAP-001
pub enum EndpointType {
    /// `ep_type 0x01`. Wakeup control (`ROADMAP.md` Milestone 7).
    Wakeup = 0x01,
    /// `ep_type 0x02`. GPIO (`ROADMAP.md` Milestone 4).
    Gpio = 0x02,
    /// `ep_type 0x03`. SPI (`ROADMAP.md` Milestone 4).
    Spi = 0x03,
    /// `ep_type 0x04`. I²C (`ROADMAP.md` Milestone 4).
    I2c = 0x04,
    /// `ep_type 0x05`. UART (`ROADMAP.md` Milestone 4).
    Uart = 0x05,
    /// `ep_type 0x06`. LIN commander (`ROADMAP.md` Milestone 7).
    Lin = 0x06,
    /// `ep_type 0x07`. PWM_OUT (`ROADMAP.md` Milestone 4).
    PwmOut = 0x07,
    /// `ep_type 0x08`. PWM_IN (`ROADMAP.md` Milestone 4).
    PwmIn = 0x08,
    /// `ep_type 0x09`. ADC (`ROADMAP.md` Milestone 4).
    Adc = 0x09,
    /// `ep_type 0x0A`. DAC. Reserved and out of scope for the current
    /// replacement cycle per `ROADMAP.md` Milestone 7 — see
    /// [`EndpointType::is_reserved`].
    Dac = 0x0A,
    /// `ep_type 0x0B`. CAN controller (`ROADMAP.md` Milestone 7).
    Can = 0x0B,
    /// `ep_type 0x0C`. ISELED (`ROADMAP.md` Milestone 7).
    Iseled = 0x0C,
    /// `ep_type 0x0D`. MDIO (`ROADMAP.md` Milestone 7).
    Mdio = 0x0D,
}

impl EndpointType {
    /// Encode this endpoint type as its wire-level `ep_type` byte value.
    // fusa:req REQ-RMAP-001
    pub fn to_u8(self) -> u8 {
        self as u8
    }

    /// Decode a wire-level `ep_type` byte value into an [`EndpointType`].
    ///
    /// Returns `Err(RcpError::Other(_))` for any byte outside `0x01..=0x0D`,
    /// mirroring [`crate::lifecycle::RcServerState::from_u8`]'s handling of
    /// an unrecognized state byte. Never panics for any input.
    // fusa:req REQ-RMAP-002
    pub fn from_u8(raw: u8) -> Result<Self, RcpError> {
        match raw {
            0x01 => Ok(Self::Wakeup),
            0x02 => Ok(Self::Gpio),
            0x03 => Ok(Self::Spi),
            0x04 => Ok(Self::I2c),
            0x05 => Ok(Self::Uart),
            0x06 => Ok(Self::Lin),
            0x07 => Ok(Self::PwmOut),
            0x08 => Ok(Self::PwmIn),
            0x09 => Ok(Self::Adc),
            0x0A => Ok(Self::Dac),
            0x0B => Ok(Self::Can),
            0x0C => Ok(Self::Iseled),
            0x0D => Ok(Self::Mdio),
            other => Err(RcpError::Other(format!(
                "register_map: unrecognized ep_type byte 0x{other:02X} (expected 0x01..=0x0D)"
            ))),
        }
    }

    /// Is this endpoint type explicitly reserved and out of scope for the
    /// current replacement cycle?
    ///
    /// True only for [`EndpointType::Dac`] — see this module's doc comment.
    /// Never panics for any input.
    // fusa:req REQ-RMAP-001
    pub fn is_reserved(self) -> bool {
        matches!(self, Self::Dac)
    }
}

// ── The three config layers ──────────────────────────────────────────────────

/// The generic, server-owned per-endpoint config block: present for every
/// endpoint regardless of [`EndpointType`].
///
/// See this module's doc comment for why `ep_type` is the only field this
/// item gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// fusa:req REQ-RMAP-003
pub struct PerEpConfigBlock {
    /// This endpoint's register-map type discriminant.
    pub ep_type: EndpointType,
}

impl PerEpConfigBlock {
    /// Construct a generic per-EP config block for `ep_type`.
    pub fn new(ep_type: EndpointType) -> Self {
        Self { ep_type }
    }

    /// The [`ConfigLayer`] this type always belongs to.
    // fusa:req REQ-RMAP-003
    pub const LAYER: ConfigLayer = ConfigLayer::Generic;
}

/// The common functional-config block: fields shared across every
/// [`EndpointType`]'s functional config.
///
/// An empty placeholder — see this module's doc comment for the three
/// concrete examples `ROADMAP.md` itself names (`ep_enable`,
/// `ep_clear_req_storage`, `ep_req_crc_enable`) and why none of them is
/// modeled as a field here yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
// fusa:req REQ-RMAP-003
pub struct CommonFunctionalConfig;

impl CommonFunctionalConfig {
    /// The [`ConfigLayer`] this type always belongs to.
    // fusa:req REQ-RMAP-003
    pub const LAYER: ConfigLayer = ConfigLayer::CommonFunctional;
}

/// A distinct, type-specific functional-config shape for the
/// [`EndpointType`] it is `for`.
///
/// An empty placeholder beyond its [`EndpointType`] tag — see this module's
/// doc comment for why no concrete per-type field (GPIO's write-semantics,
/// SPI's channel configs, etc.) is modeled here yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// fusa:req REQ-RMAP-003
pub struct PerEpTypeFunctionalConfig {
    /// The [`EndpointType`] this functional-config shape is for.
    pub ep_type: EndpointType,
}

impl PerEpTypeFunctionalConfig {
    /// Construct a per-EP-type functional-config placeholder for
    /// `ep_type`.
    pub fn new(ep_type: EndpointType) -> Self {
        Self { ep_type }
    }

    /// The [`ConfigLayer`] this value belongs to, tagged with its
    /// [`EndpointType`].
    // fusa:req REQ-RMAP-003
    pub fn layer(&self) -> ConfigLayer {
        ConfigLayer::PerTypeFunctional(self.ep_type)
    }
}

// ── ConfigLayer ───────────────────────────────────────────────────────────────

/// Which of the three taxonomy layers a register belongs to.
///
/// See this module's doc comment "Relationship to
/// `crate::lifecycle::RegisterCategory`" section for how this differs from,
/// and composes with, [`crate::lifecycle::RegisterCategory`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// fusa:req REQ-RMAP-005
pub enum ConfigLayer {
    /// [`PerEpConfigBlock`]'s layer: generic, present for every endpoint.
    Generic,
    /// [`CommonFunctionalConfig`]'s layer: functional, shared across every
    /// [`EndpointType`].
    CommonFunctional,
    /// [`PerEpTypeFunctionalConfig`]'s layer: functional, distinct per
    /// [`EndpointType`].
    PerTypeFunctional(EndpointType),
}

/// This crate's own provisional mapping from a [`ConfigLayer`] to the
/// coarser [`crate::lifecycle::RegisterCategory`] lifecycle-reachability
/// grouping.
///
/// See this module's doc comment Provenance note for the two different
/// confidence levels behind this mapping's two branches. Never panics for
/// any input.
// fusa:req REQ-RMAP-005
pub fn register_category(layer: ConfigLayer) -> RegisterCategory {
    match layer {
        ConfigLayer::Generic => RegisterCategory::HwConfig,
        ConfigLayer::CommonFunctional => RegisterCategory::RcpConfig,
        ConfigLayer::PerTypeFunctional(_) => RegisterCategory::RcpConfig,
    }
}

// ── Cross-layer invariant: functional config belongs to its endpoint's type ──

/// Does `per_type`'s [`EndpointType`] match the owning endpoint's declared
/// `ep_type` in `generic`?
///
/// The one relationship this taxonomy's three layers already have to each
/// other before any concrete field exists — see this module's doc comment.
/// Never panics for any input.
// fusa:req REQ-RMAP-004
pub fn functional_config_matches_ep_type(
    generic: &PerEpConfigBlock,
    per_type: &PerEpTypeFunctionalConfig,
) -> bool {
    generic.ep_type == per_type.ep_type
}

/// Validating counterpart to [`functional_config_matches_ep_type`].
///
/// Returns `Ok(())` if `per_type` belongs to the same [`EndpointType`] as
/// `generic`, `Err(RcpError::EndpointTypeMismatch)` otherwise. Never panics
/// for any input.
// fusa:req REQ-RMAP-004
pub fn check_functional_config_matches_ep_type(
    generic: &PerEpConfigBlock,
    per_type: &PerEpTypeFunctionalConfig,
) -> Result<(), RcpError> {
    if functional_config_matches_ep_type(generic, per_type) {
        Ok(())
    } else {
        Err(RcpError::EndpointTypeMismatch)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    const ALL_ENDPOINT_TYPES: [EndpointType; 13] = [
        EndpointType::Wakeup,
        EndpointType::Gpio,
        EndpointType::Spi,
        EndpointType::I2c,
        EndpointType::Uart,
        EndpointType::Lin,
        EndpointType::PwmOut,
        EndpointType::PwmIn,
        EndpointType::Adc,
        EndpointType::Dac,
        EndpointType::Can,
        EndpointType::Iseled,
        EndpointType::Mdio,
    ];

    // ── EndpointType: numeric encoding / round-trip ──────────────────────

    #[test]
    // fusa:test REQ-RMAP-001
    fn endpoint_type_encodings_match_roadmap_values() {
        assert_eq!(EndpointType::Wakeup.to_u8(), 0x01);
        assert_eq!(EndpointType::Gpio.to_u8(), 0x02);
        assert_eq!(EndpointType::Spi.to_u8(), 0x03);
        assert_eq!(EndpointType::I2c.to_u8(), 0x04);
        assert_eq!(EndpointType::Uart.to_u8(), 0x05);
        assert_eq!(EndpointType::Lin.to_u8(), 0x06);
        assert_eq!(EndpointType::PwmOut.to_u8(), 0x07);
        assert_eq!(EndpointType::PwmIn.to_u8(), 0x08);
        assert_eq!(EndpointType::Adc.to_u8(), 0x09);
        assert_eq!(EndpointType::Dac.to_u8(), 0x0A);
        assert_eq!(EndpointType::Can.to_u8(), 0x0B);
        assert_eq!(EndpointType::Iseled.to_u8(), 0x0C);
        assert_eq!(EndpointType::Mdio.to_u8(), 0x0D);
    }

    #[test]
    // fusa:test REQ-RMAP-001
    fn from_u8_round_trips_every_defined_ep_type() {
        for ep_type in ALL_ENDPOINT_TYPES {
            let raw = ep_type.to_u8();
            assert_eq!(EndpointType::from_u8(raw), Ok(ep_type));
        }
    }

    #[test]
    // fusa:test REQ-RMAP-001
    fn all_thirteen_ep_type_codes_are_distinct() {
        let mut seen = std::collections::HashSet::new();
        for ep_type in ALL_ENDPOINT_TYPES {
            assert!(
                seen.insert(ep_type.to_u8()),
                "duplicate ep_type code for {ep_type:?}"
            );
        }
        assert_eq!(seen.len(), 13);
    }

    #[test]
    // fusa:test REQ-RMAP-001
    fn is_reserved_true_only_for_dac() {
        for ep_type in ALL_ENDPOINT_TYPES {
            assert_eq!(ep_type.is_reserved(), ep_type == EndpointType::Dac);
        }
    }

    // ── EndpointType: rejection of unrecognized encodings ────────────────

    #[test]
    // fusa:test REQ-RMAP-002
    fn from_u8_rejects_every_byte_outside_the_defined_range() {
        for raw in 0u8..=255 {
            let result = EndpointType::from_u8(raw);
            match raw {
                0x01..=0x0D => assert!(result.is_ok(), "0x{raw:02X} should decode"),
                _ => assert!(result.is_err(), "0x{raw:02X} should be rejected"),
            }
        }
    }

    #[test]
    // fusa:test REQ-RMAP-002
    // fusa:test REQ-RMAP-006
    fn from_u8_never_panics_across_the_full_byte_range() {
        for raw in 0u8..=255 {
            let _ = EndpointType::from_u8(raw);
        }
    }

    // ── The three layers: structural existence and tagging ───────────────

    #[test]
    // fusa:test REQ-RMAP-003
    fn per_ep_config_block_layer_is_generic() {
        assert_eq!(PerEpConfigBlock::LAYER, ConfigLayer::Generic);
    }

    #[test]
    // fusa:test REQ-RMAP-003
    fn common_functional_config_layer_is_common_functional() {
        assert_eq!(CommonFunctionalConfig::LAYER, ConfigLayer::CommonFunctional);
    }

    #[test]
    // fusa:test REQ-RMAP-003
    fn per_ep_type_functional_config_layer_is_tagged_by_its_ep_type() {
        for ep_type in ALL_ENDPOINT_TYPES {
            let cfg = PerEpTypeFunctionalConfig::new(ep_type);
            assert_eq!(cfg.layer(), ConfigLayer::PerTypeFunctional(ep_type));
        }
    }

    #[test]
    // fusa:test REQ-RMAP-003
    fn per_ep_config_block_new_round_trips_ep_type() {
        for ep_type in ALL_ENDPOINT_TYPES {
            assert_eq!(PerEpConfigBlock::new(ep_type).ep_type, ep_type);
        }
    }

    #[test]
    // fusa:test REQ-RMAP-003
    fn common_functional_config_is_a_stable_zst_placeholder() {
        // A ZST placeholder: every instance compares equal to every other,
        // including a Copy of itself.
        let a = CommonFunctionalConfig;
        let b = a;
        assert_eq!(a, b);
    }

    // ── Cross-layer invariant ─────────────────────────────────────────────

    #[test]
    // fusa:test REQ-RMAP-004
    fn functional_config_matches_ep_type_true_only_when_tags_agree() {
        for generic_type in ALL_ENDPOINT_TYPES {
            for per_type_type in ALL_ENDPOINT_TYPES {
                let generic = PerEpConfigBlock::new(generic_type);
                let per_type = PerEpTypeFunctionalConfig::new(per_type_type);
                assert_eq!(
                    functional_config_matches_ep_type(&generic, &per_type),
                    generic_type == per_type_type,
                    "{generic_type:?} vs {per_type_type:?}"
                );
            }
        }
    }

    #[test]
    // fusa:test REQ-RMAP-004
    fn check_functional_config_matches_ep_type_agrees_with_the_bool_form() {
        for generic_type in ALL_ENDPOINT_TYPES {
            for per_type_type in ALL_ENDPOINT_TYPES {
                let generic = PerEpConfigBlock::new(generic_type);
                let per_type = PerEpTypeFunctionalConfig::new(per_type_type);
                let matches = functional_config_matches_ep_type(&generic, &per_type);
                let checked = check_functional_config_matches_ep_type(&generic, &per_type);
                assert_eq!(
                    checked.is_ok(),
                    matches,
                    "{generic_type:?} vs {per_type_type:?}"
                );
                if !matches {
                    assert_eq!(checked, Err(RcpError::EndpointTypeMismatch));
                }
            }
        }
    }

    // ── Relationship to lifecycle::RegisterCategory ──────────────────────

    #[test]
    // fusa:test REQ-RMAP-005
    fn register_category_matches_the_documented_mapping() {
        assert_eq!(
            register_category(ConfigLayer::Generic),
            RegisterCategory::HwConfig
        );
        assert_eq!(
            register_category(ConfigLayer::CommonFunctional),
            RegisterCategory::RcpConfig
        );
        for ep_type in ALL_ENDPOINT_TYPES {
            assert_eq!(
                register_category(ConfigLayer::PerTypeFunctional(ep_type)),
                RegisterCategory::RcpConfig,
                "{ep_type:?}"
            );
        }
    }

    #[test]
    // fusa:test REQ-RMAP-005
    fn both_functional_layers_map_to_the_same_register_category() {
        // CommonFunctional and every PerTypeFunctional variant agree with
        // each other, matching this module's doc comment reasoning that
        // both layers are "functional" in the same sense RegisterCategory's
        // own RcpConfig variant already names.
        let common = register_category(ConfigLayer::CommonFunctional);
        for ep_type in ALL_ENDPOINT_TYPES {
            assert_eq!(
                register_category(ConfigLayer::PerTypeFunctional(ep_type)),
                common
            );
        }
    }

    // ── Fuzz-style: arbitrary inputs never panic ──────────────────────────

    #[test]
    // fusa:test REQ-RMAP-006
    fn taxonomy_operations_never_panic_for_any_ep_type_pair() {
        for generic_type in ALL_ENDPOINT_TYPES {
            for per_type_type in ALL_ENDPOINT_TYPES {
                let generic = PerEpConfigBlock::new(generic_type);
                let per_type = PerEpTypeFunctionalConfig::new(per_type_type);
                let _ = functional_config_matches_ep_type(&generic, &per_type);
                let _ = check_functional_config_matches_ep_type(&generic, &per_type);
                let _ = per_type.layer();
                let _ = register_category(per_type.layer());
                let _ = generic_type.is_reserved();
                let _ = generic_type.to_u8();
            }
        }
        let _ = register_category(ConfigLayer::Generic);
        let _ = register_category(ConfigLayer::CommonFunctional);
    }
}
