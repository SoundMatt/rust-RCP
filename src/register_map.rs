// fusa:req REQ-RMAP-001
// fusa:req REQ-RMAP-002
// fusa:req REQ-RMAP-003
// fusa:req REQ-RMAP-004
// fusa:req REQ-RMAP-005
// fusa:req REQ-RMAP-006
// fusa:req REQ-RMAP-007
// fusa:req REQ-RMAP-008
// fusa:req REQ-RMAP-009
// fusa:req REQ-RMAP-010
// fusa:req REQ-RMAP-011

//! Three-layer per-endpoint config taxonomy, plus the RC Server's general
//! (whole-server) register-map fields — TC18 register-map model
//! (`ROADMAP.md` Milestone 2, "Register Map" subsection, first and second
//! items).
//!
//! Per Guiding Principle 2 ("sequence work so nothing is built on a
//! foundation that will itself change later ... lifecycle model and
//! register-map split before endpoints"), the three-layer taxonomy
//! ([`PerEpConfigBlock`]/[`CommonFunctionalConfig`]/[`PerEpTypeFunctionalConfig`])
//! established the *shape* of the register map's per-endpoint config before
//! either concrete field names or any endpoint-type work (Milestones 4 and
//! 7) existed. It deliberately invented no concrete field content beyond
//! the one tag ([`EndpointType`]) that the taxonomy itself needs to
//! distinguish its layers — see "Provenance note" below.
//!
//! This module's second item, [`GeneralRegisters`], gives the register
//! map's *general* section (the RC Server's own whole-server identity,
//! capacity, and child-table-pointer fields, read via EP0 rather than any
//! per-endpoint config) its first concrete field content — the "Register
//! Map" subsection's second checklist bullet, citing `§3.6`. The
//! subsection's remaining bullet (config tables `§3.7`-`§3.11`) is later,
//! still-unbuilt work; [`GeneralRegisters`] claims only `§3.6`'s own table.
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
//!
//! ## `GeneralRegisters` (`§3.6`)
//!
//! [`GeneralRegisters`] models `§3.6`'s general register-map table
//! (`ROADMAP.md`'s own quoted checklist wording: `svr_oa_tc18_magic_nr`,
//! `svr_version`, `svr_vendor_id`, `svr_device_id`, `svr_ep_count`,
//! `svr_implemented_options`, "and the rest of `§3.6`'s table") as a plain
//! struct, one field per table row, typed by the row's own declared bit
//! width (`u8`/`u16`/`u32`). Unlike [`PerEpConfigBlock`] and its siblings,
//! this is a whole-*server* register block, read via EP0 rather than any
//! per-endpoint config — it corresponds to
//! [`crate::lifecycle::RegisterCategory::General`], not to any
//! [`ConfigLayer`] this module's taxonomy defines (`ConfigLayer` classifies
//! only per-*endpoint* config; `§3.6`'s general fields are server-wide and
//! sit outside that axis entirely, the same way EP0 itself sits outside
//! [`EndpointType`] — see "Relationship to `crate::ep0`" above).
//!
//! Several `§3.6` table rows are themselves a pointer/capacity pair for a
//! later child config table (e.g. `svr_hw_cfg_ptr` / capacity, pointing at
//! `§3.7`'s HW pin-mapping table) — [`TableDescriptor`] gives that
//! recurring two-field shape its own reusable type rather than repeating it
//! nine times. [`GeneralRegisters::encode`]/[`GeneralRegisters::decode`]
//! give the whole block a never-panicking, fixed-length, big-endian wire
//! form, matching [`crate::wire`]'s own big-endian convention for every
//! other multi-byte field this crate already encodes.
//!
//! Like every prior Milestone 1/2 entry, [`GeneralRegisters`] is purely
//! additive: it performs no register I/O against a real RC Server, and is
//! not wired into [`crate::ep0`]'s dispatch path, [`crate::lifecycle`]'s
//! reachability checks, or any other existing caller. See "Provenance note"
//! below for the byte-layout and bitmask-decomposition inferences this
//! type's encode/decode form depends on.
//!
//! ### `GeneralRegisters` provenance note
//!
//! The 24 fields [`GeneralRegisters`] models (`svr_oa_tc18_magic_nr`
//! through `svr_security_cfg`, ending with the four remaining
//! [`TableDescriptor`]-shaped pointer rows) and each field's bit width are
//! taken directly from this crate's own `§3.6` table extraction, which
//! names every row and its width/access designation explicitly. The six
//! field names `ROADMAP.md`'s own checklist bullet quotes verbatim
//! (`svr_oa_tc18_magic_nr`, `svr_version`, `svr_vendor_id`,
//! `svr_device_id`, `svr_ep_count`, `svr_implemented_options`) are modeled
//! first, in the checklist's own order, with the remaining eighteen rows
//! following in the table's own top-to-bottom order.
//!
//! Two inferences beyond that extraction are this crate's own, and are
//! flagged here per Guiding Principle 5 rather than presented as
//! spec-cited fact:
//!
//! - **Sequential byte packing.** The extraction records each row's bit
//!   width and relative order but no explicit byte-offset table. This
//!   module's [`GeneralRegisters::encode`]/[`GeneralRegisters::decode`]
//!   therefore pack every field back-to-back in table order with no
//!   padding, each field big-endian at its declared width — a plausible
//!   reading, not a confirmed one. A real RC Server's actual register
//!   layout (byte offsets, alignment, padding) must be reconciled against
//!   this guess (never against spec prose) before this encode/decode form
//!   is relied on for interop.
//! - **`svr_implemented_options` left undecomposed.** The extraction names
//!   five option bundles the bitmask covers (compound&wait / triggered /
//!   chained / time-sync&timed / enhanced-cancel) but no bit-position
//!   assignment for any of them. Rather than invent an ordering this crate
//!   has no textual basis for, [`GeneralRegisters::implemented_options`] is
//!   left as a raw `u8`; named per-bit accessors are deferred to whichever
//!   later item first needs to test a specific optional-feature bundle
//!   against a real bit position.
//!
//! Several rows pair a pointer with a capacity field for a later child
//! config table (HW pin-mapping `§3.7`, request-stream config `§3.8`,
//! response/ack queue config `§3.10`, the common per-EP config block, the
//! EP/`byte_bus_id` mapping table `§3.9`, plus three product-specific
//! blocks); two rows (`svr_ep_functional_cfg_ptr`, `svr_sequencer_state_ptr`)
//! are pointer-only, with no paired capacity field in the extraction.
//! [`GeneralRegisters`] follows that same ptr-vs-ptr+capacity split
//! field-by-field rather than assuming every pointer row is uniformly
//! shaped.

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

// ── TableDescriptor ──────────────────────────────────────────────────────────

/// A pointer + capacity pair for one of `§3.6`'s child config tables.
///
/// See this module's doc comment "`GeneralRegisters` provenance note" for
/// which `§3.6` rows this shape applies to (and which don't).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
// fusa:req REQ-RMAP-007
pub struct TableDescriptor {
    /// Register-map address of the child table's first entry.
    pub ptr: u16,
    /// Number of entries the child table has room for.
    pub capacity: u16,
}

impl TableDescriptor {
    /// Encoded wire length in bytes.
    pub const ENCODED_LEN: usize = 4;

    /// Encode as big-endian `[ptr, capacity]`. Never panics.
    // fusa:req REQ-RMAP-007
    pub fn encode(&self) -> [u8; Self::ENCODED_LEN] {
        let mut buf = [0u8; Self::ENCODED_LEN];
        buf[0..2].copy_from_slice(&self.ptr.to_be_bytes());
        buf[2..4].copy_from_slice(&self.capacity.to_be_bytes());
        buf
    }

    /// Decode a big-endian `[ptr, capacity]` pair from the front of `bytes`.
    ///
    /// Returns `Err(RcpError::ShortFrame)` if `bytes` is shorter than
    /// [`Self::ENCODED_LEN`]. Trailing bytes beyond `ENCODED_LEN` are
    /// ignored. Never panics for any input.
    // fusa:req REQ-RMAP-007
    pub fn decode(bytes: &[u8]) -> Result<Self, RcpError> {
        if bytes.len() < Self::ENCODED_LEN {
            return Err(RcpError::ShortFrame);
        }
        let ptr = u16::from_be_bytes([bytes[0], bytes[1]]);
        let capacity = u16::from_be_bytes([bytes[2], bytes[3]]);
        Ok(Self { ptr, capacity })
    }
}

// ── GeneralRegisters (§3.6) ──────────────────────────────────────────────────

/// The RC Server's general (whole-server) register-map fields, per `§3.6`.
///
/// See this module's doc comment "`GeneralRegisters` (`§3.6`)" section for
/// what this models and "`GeneralRegisters` provenance note" for the
/// byte-layout and bitmask inferences [`Self::encode`]/[`Self::decode`]
/// depend on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
// fusa:req REQ-RMAP-008
pub struct GeneralRegisters {
    /// Fixed constant this crate's own decoder can use to recognize an OA
    /// TC18 RC Server on the wire.
    pub svr_oa_tc18_magic_nr: u32,
    /// RCP protocol version the server implements.
    pub svr_version: u32,
    /// OA-assigned vendor identifier.
    pub svr_vendor_id: u16,
    /// Vendor-specific device/part identifier.
    pub svr_device_id: u16,
    /// Number of endpoints the server implements.
    pub svr_ep_count: u16,
    /// Maximum number of request streams the server supports.
    pub svr_req_stream_max: u8,
    /// Maximum number of response/ack queues the server supports.
    pub svr_responder_streams_max: u8,
    /// Total response-queue memory, in 32-bit words, across all queues.
    pub svr_responder_mem_size: u16,
    /// Total EP request-queue memory, in 32-bit words.
    pub svr_req_mem_size: u16,
    /// Number of sequencer registers available; `0` means compound
    /// operations are unsupported.
    pub svr_sequencers_max: u8,
    /// Whether "locked-class" parameters are currently writable (`0x00`) or
    /// write-protected (any other value).
    pub svr_configuration_lock: u8,
    /// Bitmask of implemented optional feature bundles. Left undecomposed
    /// — see this module's doc comment provenance note.
    pub svr_implemented_options: u8,
    /// Number of assignable physical I/O pins.
    pub svr_io_pin_count: u16,
    /// HW pin-mapping table (`§3.7`) descriptor.
    pub svr_hw_cfg: TableDescriptor,
    /// Request-stream config table (`§3.8`) descriptor.
    pub svr_request_stream_cfg: TableDescriptor,
    /// Response/ack queue config table (`§3.10`) descriptor.
    pub svr_response_stream_cfg: TableDescriptor,
    /// Common per-EP config block descriptor.
    pub svr_ep_generic_cfg: TableDescriptor,
    /// EP/`byte_bus_id` mapping table (`§3.9`) descriptor.
    pub svr_ep_bytebus_id_map: TableDescriptor,
    /// Pointer to the per-EP-type functional config block (one per
    /// endpoint); no paired capacity field.
    pub svr_ep_functional_cfg_ptr: u16,
    /// Pointer to the sequencer-state block (`§3.11`); no paired capacity
    /// field.
    pub svr_sequencer_state_ptr: u16,
    /// Network-interface config descriptor. Product-specific content;
    /// `capacity == 0` means unsupported.
    pub svr_network_interface_cfg: TableDescriptor,
    /// Physical-layer config descriptor. Product-specific content;
    /// `capacity == 0` means unsupported/hidden behind an MDIO endpoint.
    pub svr_physical_layer_cfg: TableDescriptor,
    /// Time-sync (e.g. gPTP) config descriptor; `capacity == 0` means
    /// time-sync is unsupported.
    pub svr_time_synch_cfg: TableDescriptor,
    /// Security (e.g. MACsec) config descriptor; `capacity == 0` means
    /// unsupported.
    pub svr_security_cfg: TableDescriptor,
}

impl GeneralRegisters {
    /// Encoded wire length in bytes.
    pub const ENCODED_LEN: usize = 65;

    /// The [`crate::lifecycle::RegisterCategory`] every `§3.6` general
    /// register belongs to, per this module's doc comment.
    pub const CATEGORY: RegisterCategory = RegisterCategory::General;

    /// Encode as a fixed-length, big-endian byte block, table-row order,
    /// with no padding between fields. Never panics.
    ///
    /// See this module's doc comment provenance note for the sequential
    /// byte-packing inference this encoding depends on.
    // fusa:req REQ-RMAP-009
    pub fn encode(&self) -> [u8; Self::ENCODED_LEN] {
        let mut buf = [0u8; Self::ENCODED_LEN];
        let mut off = 0usize;

        macro_rules! put {
            ($val:expr) => {{
                let bytes = $val.to_be_bytes();
                buf[off..off + bytes.len()].copy_from_slice(&bytes);
                off += bytes.len();
            }};
        }
        macro_rules! put_descriptor {
            ($val:expr) => {{
                let bytes = $val.encode();
                buf[off..off + bytes.len()].copy_from_slice(&bytes);
                off += bytes.len();
            }};
        }

        put!(self.svr_oa_tc18_magic_nr);
        put!(self.svr_version);
        put!(self.svr_vendor_id);
        put!(self.svr_device_id);
        put!(self.svr_ep_count);
        put!(self.svr_req_stream_max);
        put!(self.svr_responder_streams_max);
        put!(self.svr_responder_mem_size);
        put!(self.svr_req_mem_size);
        put!(self.svr_sequencers_max);
        put!(self.svr_configuration_lock);
        put!(self.svr_implemented_options);
        put!(self.svr_io_pin_count);
        put_descriptor!(self.svr_hw_cfg);
        put_descriptor!(self.svr_request_stream_cfg);
        put_descriptor!(self.svr_response_stream_cfg);
        put_descriptor!(self.svr_ep_generic_cfg);
        put_descriptor!(self.svr_ep_bytebus_id_map);
        put!(self.svr_ep_functional_cfg_ptr);
        put!(self.svr_sequencer_state_ptr);
        put_descriptor!(self.svr_network_interface_cfg);
        put_descriptor!(self.svr_physical_layer_cfg);
        put_descriptor!(self.svr_time_synch_cfg);
        put_descriptor!(self.svr_security_cfg);

        debug_assert_eq!(off, Self::ENCODED_LEN);
        buf
    }

    /// Decode a fixed-length, big-endian byte block produced by
    /// [`Self::encode`].
    ///
    /// Returns `Err(RcpError::ShortFrame)` if `bytes` is shorter than
    /// [`Self::ENCODED_LEN`]. Trailing bytes beyond `ENCODED_LEN` are
    /// ignored. Never panics for any input.
    // fusa:req REQ-RMAP-010
    pub fn decode(bytes: &[u8]) -> Result<Self, RcpError> {
        if bytes.len() < Self::ENCODED_LEN {
            return Err(RcpError::ShortFrame);
        }
        let mut off = 0usize;

        macro_rules! take_u8 {
            () => {{
                let v = bytes[off];
                off += 1;
                v
            }};
        }
        macro_rules! take_u16 {
            () => {{
                let v = u16::from_be_bytes([bytes[off], bytes[off + 1]]);
                off += 2;
                v
            }};
        }
        macro_rules! take_u32 {
            () => {{
                let v = u32::from_be_bytes([
                    bytes[off],
                    bytes[off + 1],
                    bytes[off + 2],
                    bytes[off + 3],
                ]);
                off += 4;
                v
            }};
        }
        macro_rules! take_descriptor {
            () => {{
                // Bounds already guaranteed by the ENCODED_LEN check above;
                // TableDescriptor::decode's own length check cannot fail
                // here, but is still consulted rather than sliced past.
                let d = TableDescriptor::decode(&bytes[off..])?;
                off += TableDescriptor::ENCODED_LEN;
                d
            }};
        }

        let svr_oa_tc18_magic_nr = take_u32!();
        let svr_version = take_u32!();
        let svr_vendor_id = take_u16!();
        let svr_device_id = take_u16!();
        let svr_ep_count = take_u16!();
        let svr_req_stream_max = take_u8!();
        let svr_responder_streams_max = take_u8!();
        let svr_responder_mem_size = take_u16!();
        let svr_req_mem_size = take_u16!();
        let svr_sequencers_max = take_u8!();
        let svr_configuration_lock = take_u8!();
        let svr_implemented_options = take_u8!();
        let svr_io_pin_count = take_u16!();
        let svr_hw_cfg = take_descriptor!();
        let svr_request_stream_cfg = take_descriptor!();
        let svr_response_stream_cfg = take_descriptor!();
        let svr_ep_generic_cfg = take_descriptor!();
        let svr_ep_bytebus_id_map = take_descriptor!();
        let svr_ep_functional_cfg_ptr = take_u16!();
        let svr_sequencer_state_ptr = take_u16!();
        let svr_network_interface_cfg = take_descriptor!();
        let svr_physical_layer_cfg = take_descriptor!();
        let svr_time_synch_cfg = take_descriptor!();
        let svr_security_cfg = take_descriptor!();

        debug_assert_eq!(off, Self::ENCODED_LEN);
        Ok(Self {
            svr_oa_tc18_magic_nr,
            svr_version,
            svr_vendor_id,
            svr_device_id,
            svr_ep_count,
            svr_req_stream_max,
            svr_responder_streams_max,
            svr_responder_mem_size,
            svr_req_mem_size,
            svr_sequencers_max,
            svr_configuration_lock,
            svr_implemented_options,
            svr_io_pin_count,
            svr_hw_cfg,
            svr_request_stream_cfg,
            svr_response_stream_cfg,
            svr_ep_generic_cfg,
            svr_ep_bytebus_id_map,
            svr_ep_functional_cfg_ptr,
            svr_sequencer_state_ptr,
            svr_network_interface_cfg,
            svr_physical_layer_cfg,
            svr_time_synch_cfg,
            svr_security_cfg,
        })
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

    // ── TableDescriptor: round-trip / short-input rejection ───────────────

    fn sample_descriptors() -> [TableDescriptor; 4] {
        [
            TableDescriptor {
                ptr: 0,
                capacity: 0,
            },
            TableDescriptor {
                ptr: 1,
                capacity: 1,
            },
            TableDescriptor {
                ptr: 0x1234,
                capacity: 0x5678,
            },
            TableDescriptor {
                ptr: u16::MAX,
                capacity: u16::MAX,
            },
        ]
    }

    #[test]
    // fusa:test REQ-RMAP-007
    fn table_descriptor_encode_decode_round_trips() {
        for d in sample_descriptors() {
            let encoded = d.encode();
            assert_eq!(encoded.len(), TableDescriptor::ENCODED_LEN);
            assert_eq!(TableDescriptor::decode(&encoded), Ok(d));
        }
    }

    #[test]
    // fusa:test REQ-RMAP-007
    fn table_descriptor_decode_ignores_trailing_bytes() {
        let d = TableDescriptor {
            ptr: 0x0102,
            capacity: 0x0304,
        };
        let mut bytes = d.encode().to_vec();
        bytes.extend_from_slice(&[0xFF, 0xFF, 0xFF]);
        assert_eq!(TableDescriptor::decode(&bytes), Ok(d));
    }

    #[test]
    // fusa:test REQ-RMAP-007
    fn table_descriptor_decode_rejects_short_input() {
        for len in 0..TableDescriptor::ENCODED_LEN {
            let bytes = vec![0u8; len];
            assert_eq!(TableDescriptor::decode(&bytes), Err(RcpError::ShortFrame));
        }
    }

    // ── GeneralRegisters: structural coverage of the roadmap-named fields ──

    fn sample_general_registers() -> GeneralRegisters {
        GeneralRegisters {
            svr_oa_tc18_magic_nr: 0x4F41_5443, // arbitrary sample pattern, not a specified constant
            svr_version: 0x0005_0001,
            svr_vendor_id: 0x1234,
            svr_device_id: 0xABCD,
            svr_ep_count: 7,
            svr_req_stream_max: 4,
            svr_responder_streams_max: 2,
            svr_responder_mem_size: 512,
            svr_req_mem_size: 256,
            svr_sequencers_max: 3,
            svr_configuration_lock: 0,
            svr_implemented_options: 0b0001_0110,
            svr_io_pin_count: 64,
            svr_hw_cfg: TableDescriptor {
                ptr: 0x0100,
                capacity: 16,
            },
            svr_request_stream_cfg: TableDescriptor {
                ptr: 0x0200,
                capacity: 4,
            },
            svr_response_stream_cfg: TableDescriptor {
                ptr: 0x0300,
                capacity: 2,
            },
            svr_ep_generic_cfg: TableDescriptor {
                ptr: 0x0400,
                capacity: 7,
            },
            svr_ep_bytebus_id_map: TableDescriptor {
                ptr: 0x0500,
                capacity: 7,
            },
            svr_ep_functional_cfg_ptr: 0x0600,
            svr_sequencer_state_ptr: 0x0700,
            svr_network_interface_cfg: TableDescriptor {
                ptr: 0,
                capacity: 0,
            },
            svr_physical_layer_cfg: TableDescriptor {
                ptr: 0,
                capacity: 0,
            },
            svr_time_synch_cfg: TableDescriptor {
                ptr: 0,
                capacity: 0,
            },
            svr_security_cfg: TableDescriptor {
                ptr: 0,
                capacity: 0,
            },
        }
    }

    #[test]
    // fusa:test REQ-RMAP-008
    fn general_registers_names_the_six_roadmap_quoted_fields_independently() {
        // Every field ROADMAP.md's own checklist bullet names verbatim is
        // independently settable/readable and distinguishable from every
        // other such field.
        let regs = sample_general_registers();
        assert_eq!(regs.svr_oa_tc18_magic_nr, 0x4F41_5443);
        assert_eq!(regs.svr_version, 0x0005_0001);
        assert_eq!(regs.svr_vendor_id, 0x1234);
        assert_eq!(regs.svr_device_id, 0xABCD);
        assert_eq!(regs.svr_ep_count, 7);
        assert_eq!(regs.svr_implemented_options, 0b0001_0110);
    }

    #[test]
    // fusa:test REQ-RMAP-008
    fn general_registers_default_is_all_zero() {
        let regs = GeneralRegisters::default();
        assert_eq!(regs.svr_oa_tc18_magic_nr, 0);
        assert_eq!(regs.svr_hw_cfg, TableDescriptor::default());
        assert_eq!(regs.svr_security_cfg, TableDescriptor::default());
    }

    #[test]
    // fusa:test REQ-RMAP-008
    fn general_registers_category_is_lifecycle_general() {
        assert_eq!(GeneralRegisters::CATEGORY, RegisterCategory::General);
    }

    // ── GeneralRegisters: encode/decode round-trip ─────────────────────────

    #[test]
    // fusa:test REQ-RMAP-009
    fn general_registers_encode_decode_round_trips() {
        let regs = sample_general_registers();
        let encoded = regs.encode();
        assert_eq!(encoded.len(), GeneralRegisters::ENCODED_LEN);
        assert_eq!(GeneralRegisters::decode(&encoded), Ok(regs));
    }

    #[test]
    // fusa:test REQ-RMAP-009
    fn general_registers_encode_decode_round_trips_default_and_max_values() {
        for regs in [
            GeneralRegisters::default(),
            GeneralRegisters {
                svr_oa_tc18_magic_nr: u32::MAX,
                svr_version: u32::MAX,
                svr_vendor_id: u16::MAX,
                svr_device_id: u16::MAX,
                svr_ep_count: u16::MAX,
                svr_req_stream_max: u8::MAX,
                svr_responder_streams_max: u8::MAX,
                svr_responder_mem_size: u16::MAX,
                svr_req_mem_size: u16::MAX,
                svr_sequencers_max: u8::MAX,
                svr_configuration_lock: u8::MAX,
                svr_implemented_options: u8::MAX,
                svr_io_pin_count: u16::MAX,
                svr_hw_cfg: TableDescriptor {
                    ptr: u16::MAX,
                    capacity: u16::MAX,
                },
                svr_request_stream_cfg: TableDescriptor {
                    ptr: u16::MAX,
                    capacity: u16::MAX,
                },
                svr_response_stream_cfg: TableDescriptor {
                    ptr: u16::MAX,
                    capacity: u16::MAX,
                },
                svr_ep_generic_cfg: TableDescriptor {
                    ptr: u16::MAX,
                    capacity: u16::MAX,
                },
                svr_ep_bytebus_id_map: TableDescriptor {
                    ptr: u16::MAX,
                    capacity: u16::MAX,
                },
                svr_ep_functional_cfg_ptr: u16::MAX,
                svr_sequencer_state_ptr: u16::MAX,
                svr_network_interface_cfg: TableDescriptor {
                    ptr: u16::MAX,
                    capacity: u16::MAX,
                },
                svr_physical_layer_cfg: TableDescriptor {
                    ptr: u16::MAX,
                    capacity: u16::MAX,
                },
                svr_time_synch_cfg: TableDescriptor {
                    ptr: u16::MAX,
                    capacity: u16::MAX,
                },
                svr_security_cfg: TableDescriptor {
                    ptr: u16::MAX,
                    capacity: u16::MAX,
                },
            },
        ] {
            let encoded = regs.encode();
            assert_eq!(GeneralRegisters::decode(&encoded), Ok(regs));
        }
    }

    #[test]
    // fusa:test REQ-RMAP-009
    fn general_registers_decode_ignores_trailing_bytes() {
        let regs = sample_general_registers();
        let mut bytes = regs.encode().to_vec();
        bytes.extend_from_slice(&[0xAA, 0xBB, 0xCC]);
        assert_eq!(GeneralRegisters::decode(&bytes), Ok(regs));
    }

    // ── GeneralRegisters: short-input rejection ────────────────────────────

    #[test]
    // fusa:test REQ-RMAP-010
    fn general_registers_decode_rejects_every_length_shorter_than_encoded_len() {
        for len in 0..GeneralRegisters::ENCODED_LEN {
            let bytes = vec![0u8; len];
            assert_eq!(
                GeneralRegisters::decode(&bytes),
                Err(RcpError::ShortFrame),
                "length {len} should be rejected"
            );
        }
    }

    #[test]
    // fusa:test REQ-RMAP-010
    fn general_registers_decode_accepts_exactly_encoded_len() {
        let regs = sample_general_registers();
        let encoded = regs.encode();
        assert!(GeneralRegisters::decode(&encoded[..GeneralRegisters::ENCODED_LEN]).is_ok());
    }

    // ── Fuzz-style: arbitrary byte inputs never panic ──────────────────────

    #[test]
    // fusa:test REQ-RMAP-011
    fn table_descriptor_decode_never_panics_across_arbitrary_lengths() {
        for len in 0..=300usize {
            let bytes: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
            let _ = TableDescriptor::decode(&bytes);
        }
    }

    #[test]
    // fusa:test REQ-RMAP-011
    fn general_registers_decode_never_panics_across_arbitrary_lengths() {
        for len in 0..=300usize {
            let bytes: Vec<u8> = (0..len).map(|i| ((i * 7) % 256) as u8).collect();
            let _ = GeneralRegisters::decode(&bytes);
        }
    }

    #[test]
    // fusa:test REQ-RMAP-011
    fn general_registers_and_table_descriptor_encode_never_panic() {
        let regs = sample_general_registers();
        let _ = regs.encode();
        for d in sample_descriptors() {
            let _ = d.encode();
        }
    }
}
