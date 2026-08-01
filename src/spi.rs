//fusa:req REQ-SPI-001
//fusa:req REQ-SPI-002
//fusa:req REQ-SPI-003
//fusa:req REQ-SPI-004
//fusa:req REQ-SPI-005
//fusa:req REQ-SPI-006
//fusa:req REQ-SPI-007
//fusa:req REQ-SPI-008
//fusa:req REQ-SPI-009
//fusa:req REQ-SPI-010
//fusa:req REQ-SPI-011
//fusa:req REQ-SPI-012

//! The SPI endpoint type (`ep_type 0x03`) — `ROADMAP.md` Milestone 4
//! ("Basic Endpoint Types"), second checklist bullet: "up to 6
//! pre-configured channel configs selected via `evt[2:0]`; raw PICO/POCI
//! byte transfer; compound-wait's 4-of-20-byte status truncation rule."
//!
//! This follows directly on [`crate::gpio`] (Milestone 4's opening item):
//! same milestone, same "additive standalone plumbing only" discipline, same
//! doc-comment provenance-note style for anything this crate has not yet
//! reconciled against confirmed wire behavior. Three named pieces are in
//! scope, all implemented here:
//!
//! - [`SpiChannelSelect`] — the up-to-6 pre-configured channel selection,
//!   read via the same already-generic [`crate::acf::Evt::sub_opcode`] field
//!   GPIO's own write-semantics selection already reuses (see "Provenance
//!   note: channel selection via `evt.sub_opcode`" below), plus
//!   [`resolve_spi_channel_index`] and [`SpiFunctionalConfig`] /
//!   [`select_spi_channel_config`] for turning a selection into one of up to
//!   six pre-configured channel slots.
//! - [`SpiByteTransfer`] / [`SpiByteTransferResult`] — the raw PICO
//!   (controller-to-peripheral) / POCI (peripheral-to-controller) byte
//!   transfer, modeled as an unstructured byte stream rather than an
//!   interpreted payload, matching how [`crate::gpio::GpioBitmask`] modeled
//!   its own fixed-length wire form for a value this crate does not
//!   otherwise interpret.
//! - [`SpiStatus`] / [`SpiCompoundWaitStatus`] /
//!   [`truncate_spi_status_for_compound_wait`] — the 20-byte SPI status
//!   shape and its 4-byte truncated form for compound-wait interaction. See
//!   "Provenance note: the compound-wait status truncation" below for why
//!   this is modeled as a standalone shape only, not wired into any
//!   compound-wait dispatch.
//!
//! Deliberately out of scope, for the same reasons [`crate::gpio`]'s own doc
//! comment already gives:
//!
//! - The "Groups A/B/C" `evt[2:0]` sub-opcode convention as a general,
//!   cross-endpoint-type classification scheme — this module reads
//!   `sub_opcode` only as SPI's own private channel-selection
//!   interpretation, per this milestone's still-outstanding last checklist
//!   bullet.
//! - [`crate::regmap::CommonFunctionalConfig`]'s fields — unchanged here, as
//!   in every prior Milestone 1-4 entry.
//! - Wiring any of the below into an actual decoder, dispatch loop, or
//!   [`crate::avtp`]/[`crate::acf`]/[`crate::addressing`] caller, and — new
//!   to this item specifically — wiring [`truncate_spi_status_for_compound_wait`]
//!   into any compound-wait execution path, since compound-wait itself is
//!   `ROADMAP.md` Milestone 5, not yet built. This module represents the
//!   truncation shape only.
//! - The content of each of the up to 6 pre-configured channel configs
//!   themselves (clock rate, polarity/phase, bit order, and so on).
//!   `ROADMAP.md`'s checklist text names the up-to-6 *selection* mechanism
//!   concretely but does not itself enumerate what a channel config
//!   contains, so [`SpiChannelConfigSlot`] is left an intentionally empty
//!   placeholder — the same discipline
//!   [`crate::regmap::CommonFunctionalConfig`] already applies to its own
//!   still-unnamed fields — rather than this crate guessing plausible SPI
//!   clock/mode fields on its own.
//!
//! ## Relationship to [`crate::regmap`]
//!
//! As with [`crate::gpio::GpioFunctionalConfig`], SPI's real functional
//! -config content gets its own dedicated type, [`SpiFunctionalConfig`],
//! rather than adding SPI-specific fields directly onto the still-shared,
//! thirteen-endpoint-type [`crate::regmap::PerEpTypeFunctionalConfig`]
//! placeholder. [`SpiFunctionalConfig::layer_tag`] shows how a caller
//! obtains the matching generic-layer tag so the two compose through
//! [`crate::regmap::check_functional_config_matches_ep_type`] exactly as
//! that cross-layer rule already expects, without this module editing
//! [`crate::regmap`] itself.
//!
//! ## Provenance note: channel selection via `evt.sub_opcode`
//!
//! [`crate::acf::Evt::sub_opcode`] is a 3-bit field spanning eight values
//! (`0..=`[`crate::acf::EVT_SUB_OPCODE_MAX`]). `ROADMAP.md`'s SPI checklist
//! bullet names "up to 6" pre-configured channel configs "selected via
//! `evt[2:0]`" — the same three-bit field name GPIO's own write-semantics
//! selection already reads. This module follows that same precedent:
//! [`SpiChannelSelect::to_sub_opcode`]/[`SpiChannelSelect::from_sub_opcode`]
//! read/write the field directly rather than inventing a separate
//! SPI-private selector byte. That `sub_opcode` is the selecting field
//! remains this crate's own working interpretation, flagged per Guiding
//! Principle 5, since `ROADMAP.md` itself does not say so explicitly.
//!
//! `ROADMAP.md`'s own checklist text caps the meaningful channel selection
//! at six, leaving two of the eight `sub_opcode` values outside the
//! six-channel range. An earlier revision of this module read those two
//! remaining values as interchangeable "spare" channel selections, both
//! silently resolved to `Err(RcpError::UnsupportedCmd)` by
//! [`resolve_spi_channel_index`] for the same undifferentiated reason.
//! Issue #100 corrects that: the SPI endpoint-specific evt-bits table in
//! the OPEN Alliance TC18 Remote Control Protocol Specification v0.5.1_RC
//! gives those two values two different, spec-confirmed meanings, neither
//! of which is an ordinary channel selection — one is reserved and must be
//! rejected, the other reconfigures the endpoint (its payload is used for
//! configuration, not presented to the interface). [`SpiChannelSelect`]
//! now carries them as two distinctly named variants,
//! [`SpiChannelSelect::Reserved6`] and [`SpiChannelSelect::Reconfigure7`],
//! rather than the earlier interchangeable "spare" naming.
//! [`resolve_spi_channel_index`] still refuses both
//! (`Err(RcpError::UnsupportedCmd)`), since neither resolves to a real
//! channel index and this crate has no compound-wait/dispatch machinery yet
//! to route [`SpiChannelSelect::Reconfigure7`] to actual endpoint
//! reconfiguration — see "Deliberately out of scope" above — but the two
//! are no longer treated as interchangeable. The specific `0..=5` ordering
//! [`SpiChannelSelect::to_sub_opcode`] assigns to the six channel variants
//! remains this crate's own choice (ascending channel-index order), not a
//! transcription of a confirmed wire encoding; only the two high codes'
//! meanings are spec-confirmed by issue #100.
//!
//! ## Provenance note: the compound-wait status truncation
//!
//! `ROADMAP.md`'s SPI checklist bullet names a "4-of-20-byte status
//! truncation rule" for compound-wait's interaction with SPI status
//! reporting, without itself stating which 4 of the 20 bytes survive
//! truncation. Per Guiding Principle 5, [`truncate_spi_status_for_compound_wait`]
//! takes the leading four bytes of [`SpiStatus`]'s 20-byte form as this
//! crate's own working interpretation (front truncation being the ordinary
//! reading of unqualified "N-of-M truncation") — not a confirmed statement
//! of which status fields those four bytes correspond to. Because
//! compound-wait itself (`ROADMAP.md` Milestone 5's "Compound / compound
//! -wait" checklist item) does not exist in this crate yet, this module
//! stops at representing the 20-byte/4-byte shapes and the truncation
//! between them; it does not attempt to wire that truncation into a
//! compound-wait execution path that has nothing to attach to.

use crate::RcpError;

// ── SpiChannelSelect ─────────────────────────────────────────────────────────

/// Number of pre-configured SPI channel slots [`SpiChannelSelect`] can
/// address ("up to 6" per this module's doc comment).
pub const SPI_CHANNEL_COUNT: usize = 6;

/// The up-to-6 pre-configured SPI channel selection, carried by
/// [`crate::acf::Evt::sub_opcode`]'s full 3-bit (`0..=7`) range.
///
/// See this module's doc comment "Provenance note: channel selection via
/// `evt.sub_opcode`" for why the two high values
/// ([`SpiChannelSelect::Reserved6`]/[`SpiChannelSelect::Reconfigure7`]) are
/// modeled as explicit, distinctly named variants rather than silently
/// accepted as channel selections, rejected outright at decode time, or
/// (as an earlier revision of this module did) treated as interchangeable
/// "spare" values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
//fusa:req REQ-SPI-001
pub enum SpiChannelSelect {
    /// Pre-configured channel 0.
    Channel0 = 0,
    /// Pre-configured channel 1.
    Channel1 = 1,
    /// Pre-configured channel 2.
    Channel2 = 2,
    /// Pre-configured channel 3.
    Channel3 = 3,
    /// Pre-configured channel 4.
    Channel4 = 4,
    /// Pre-configured channel 5.
    Channel5 = 5,
    /// The SPI endpoint-specific evt-bits table's reserved `sub_opcode`
    /// value — not a channel selection. See this module's doc comment
    /// "Provenance note: channel selection via `evt.sub_opcode`" —
    /// [`resolve_spi_channel_index`] refuses this rather than treating it
    /// as a channel.
    Reserved6 = 6,
    /// The SPI endpoint-specific evt-bits table's reconfigure-endpoint
    /// `sub_opcode` value: its payload is used for endpoint configuration,
    /// not presented to the interface, so it does not select a channel
    /// either. See this module's doc comment "Provenance note: channel
    /// selection via `evt.sub_opcode`" — [`resolve_spi_channel_index`]
    /// refuses this too, since this crate has no compound-wait/dispatch
    /// machinery yet to route it to actual endpoint reconfiguration.
    Reconfigure7 = 7,
}

impl SpiChannelSelect {
    /// Encode this channel selection as its `evt.sub_opcode` value
    /// (`0..=7`).
    //fusa:req REQ-SPI-001
    pub fn to_sub_opcode(self) -> u8 {
        self as u8
    }

    /// Decode an `evt.sub_opcode` value into a [`SpiChannelSelect`].
    ///
    /// Returns `Err(RcpError::InvalidParameter)` for any value outside the
    /// 3-bit `sub_opcode` field's range
    /// (`> `[`crate::acf::EVT_SUB_OPCODE_MAX`]``), matching
    /// [`crate::gpio::GpioWriteSemantics::from_sub_opcode`]'s own range
    /// check. Never panics for any input.
    //fusa:req REQ-SPI-002
    pub fn from_sub_opcode(raw: u8) -> Result<Self, RcpError> {
        match raw {
            0 => Ok(Self::Channel0),
            1 => Ok(Self::Channel1),
            2 => Ok(Self::Channel2),
            3 => Ok(Self::Channel3),
            4 => Ok(Self::Channel4),
            5 => Ok(Self::Channel5),
            6 => Ok(Self::Reserved6),
            7 => Ok(Self::Reconfigure7),
            _ => Err(RcpError::InvalidParameter),
        }
    }

    /// True for [`SpiChannelSelect::Channel0`]..[`SpiChannelSelect::Channel5`]
    /// — the six real channel selections `ROADMAP.md`'s checklist text
    /// names. False for [`SpiChannelSelect::Reserved6`] (spec-reserved,
    /// rejected) and [`SpiChannelSelect::Reconfigure7`] (endpoint
    /// reconfiguration, not a channel selection).
    //fusa:req REQ-SPI-003
    pub fn is_named(self) -> bool {
        !matches!(self, Self::Reserved6 | Self::Reconfigure7)
    }
}

/// Resolve a [`SpiChannelSelect`] to its `0..=5` channel index into
/// [`SpiFunctionalConfig::channels`].
///
/// Returns `Err(RcpError::UnsupportedCmd)` for
/// [`SpiChannelSelect::Reserved6`]/[`SpiChannelSelect::Reconfigure7`] —
/// neither resolves to a real channel index — rather than guessing a
/// channel for them; see this module's doc comment "Provenance note:
/// channel selection via `evt.sub_opcode`". Never panics for any input.
//fusa:req REQ-SPI-004
pub fn resolve_spi_channel_index(select: SpiChannelSelect) -> Result<usize, RcpError> {
    match select {
        SpiChannelSelect::Channel0 => Ok(0),
        SpiChannelSelect::Channel1 => Ok(1),
        SpiChannelSelect::Channel2 => Ok(2),
        SpiChannelSelect::Channel3 => Ok(3),
        SpiChannelSelect::Channel4 => Ok(4),
        SpiChannelSelect::Channel5 => Ok(5),
        SpiChannelSelect::Reserved6 | SpiChannelSelect::Reconfigure7 => {
            Err(RcpError::UnsupportedCmd)
        }
    }
}

// ── SpiFunctionalConfig ──────────────────────────────────────────────────────

/// One pre-configured SPI channel's config content.
///
/// An intentionally empty placeholder — see this module's doc comment for
/// why the actual per-channel fields (clock rate, polarity/phase, bit
/// order, and so on) are left unmodeled here rather than guessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-SPI-005
pub struct SpiChannelConfigSlot;

/// SPI's own per-EP-type functional-config content: up to
/// [`SPI_CHANNEL_COUNT`] pre-configured channel slots, addressed by
/// [`SpiChannelSelect`].
///
/// See this module's doc comment "Relationship to `crate::regmap`" for why
/// this is a dedicated type rather than content added directly to
/// [`crate::regmap::PerEpTypeFunctionalConfig`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-SPI-005
pub struct SpiFunctionalConfig {
    /// This endpoint's up-to-6 pre-configured channel slots, indexed by
    /// [`resolve_spi_channel_index`].
    pub channels: [SpiChannelConfigSlot; SPI_CHANNEL_COUNT],
}

impl SpiFunctionalConfig {
    /// The [`crate::regmap::PerEpTypeFunctionalConfig`] generic-layer tag
    /// that matches this SPI functional config, for use with
    /// [`crate::regmap::check_functional_config_matches_ep_type`].
    ///
    /// This module does not itself call that function — it only shows how a
    /// caller would obtain the matching tag, per this module's doc comment
    /// "Relationship to `crate::regmap`".
    //fusa:req REQ-SPI-007
    pub fn layer_tag(&self) -> crate::regmap::PerEpTypeFunctionalConfig {
        crate::regmap::PerEpTypeFunctionalConfig::new(crate::regmap::EndpointType::Spi)
    }
}

/// Select one of `config`'s up to [`SPI_CHANNEL_COUNT`] pre-configured
/// channel slots via `select`.
///
/// Returns `Err(RcpError::UnsupportedCmd)` for the two spare
/// [`SpiChannelSelect`] values, via [`resolve_spi_channel_index`]. Never
/// panics for any input.
//fusa:req REQ-SPI-006
pub fn select_spi_channel_config(
    select: SpiChannelSelect,
    config: &SpiFunctionalConfig,
) -> Result<SpiChannelConfigSlot, RcpError> {
    let index = resolve_spi_channel_index(select)?;
    Ok(config.channels[index])
}

// ── Raw PICO/POCI byte transfer ──────────────────────────────────────────────

/// A raw PICO (Peripheral-In, Controller-Out) byte transfer: the bytes an
/// SPI request sends from controller to peripheral.
///
/// Modeled as an unstructured, variable-length byte stream — this module
/// does not interpret its contents — matching how
/// [`crate::gpio::GpioBitmask`] modeled its own fixed-length wire form for a
/// value this crate does not otherwise interpret. Unlike
/// [`crate::gpio::GpioBitmask`]'s fixed 4-byte length, a raw byte stream of
/// any length has no invalid encoding, so [`SpiByteTransfer::decode`] is
/// infallible.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
//fusa:req REQ-SPI-008
pub struct SpiByteTransfer {
    /// The raw bytes sent from controller to peripheral.
    pub pico: Vec<u8>,
}

impl SpiByteTransfer {
    /// Encode this transfer to its raw wire representation: `pico`'s bytes,
    /// unmodified and unframed.
    //fusa:req REQ-SPI-008
    pub fn encode(&self) -> Vec<u8> {
        self.pico.clone()
    }

    /// Decode a [`SpiByteTransfer`] from a byte slice.
    ///
    /// Every possible byte slice, including an empty one, is a valid PICO
    /// transfer, so this never fails and never panics for any input.
    //fusa:req REQ-SPI-008
    pub fn decode(b: &[u8]) -> Self {
        Self { pico: b.to_vec() }
    }
}

/// A raw POCI (Peripheral-Out, Controller-In) byte transfer: the bytes an
/// SPI response returns from peripheral to controller.
///
/// See [`SpiByteTransfer`]'s doc comment — this is the same unstructured,
/// variable-length byte-stream modeling for the opposite transfer
/// direction.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
//fusa:req REQ-SPI-009
pub struct SpiByteTransferResult {
    /// The raw bytes returned from peripheral to controller.
    pub poci: Vec<u8>,
}

impl SpiByteTransferResult {
    /// Encode this transfer result to its raw wire representation: `poci`'s
    /// bytes, unmodified and unframed.
    //fusa:req REQ-SPI-009
    pub fn encode(&self) -> Vec<u8> {
        self.poci.clone()
    }

    /// Decode a [`SpiByteTransferResult`] from a byte slice.
    ///
    /// Every possible byte slice, including an empty one, is a valid POCI
    /// transfer result, so this never fails and never panics for any input.
    //fusa:req REQ-SPI-009
    pub fn decode(b: &[u8]) -> Self {
        Self { poci: b.to_vec() }
    }
}

// ── SPI status / compound-wait truncation ────────────────────────────────────

/// Length, in bytes, of the full SPI status shape named by this module's
/// doc comment's "4-of-20-byte status truncation rule".
pub const SPI_STATUS_LEN: usize = 20;

/// Length, in bytes, of the compound-wait-truncated SPI status shape named
/// by this module's doc comment's "4-of-20-byte status truncation rule".
pub const SPI_COMPOUND_WAIT_STATUS_LEN: usize = 4;

/// The SPI endpoint's full, untruncated 20-byte status.
///
/// See this module's doc comment "Provenance note: the compound-wait status
/// truncation" — this crate does not otherwise interpret this status's
/// byte layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
//fusa:req REQ-SPI-010
pub struct SpiStatus(pub [u8; SPI_STATUS_LEN]);

impl SpiStatus {
    /// Encode this status to its 20-byte wire representation.
    //fusa:req REQ-SPI-010
    pub fn encode(self) -> [u8; SPI_STATUS_LEN] {
        self.0
    }

    /// Decode an [`SpiStatus`] from a byte slice.
    ///
    /// Never panics on short, truncated, or arbitrary input — always
    /// returns `Err(RcpError::ShortFrame)` for input shorter than
    /// [`SPI_STATUS_LEN`] instead. Trailing bytes beyond the first 20 are
    /// ignored, matching [`crate::gpio::GpioBitmask::decode`]'s own handling
    /// of a longer-than-required slice.
    //fusa:req REQ-SPI-010
    pub fn decode(b: &[u8]) -> Result<Self, RcpError> {
        if b.len() < SPI_STATUS_LEN {
            return Err(RcpError::ShortFrame);
        }
        let mut buf = [0u8; SPI_STATUS_LEN];
        buf.copy_from_slice(&b[0..SPI_STATUS_LEN]);
        Ok(Self(buf))
    }
}

/// The 4-byte SPI status shape produced by truncating an [`SpiStatus`] for
/// compound-wait, per [`truncate_spi_status_for_compound_wait`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
//fusa:req REQ-SPI-011
pub struct SpiCompoundWaitStatus(pub [u8; SPI_COMPOUND_WAIT_STATUS_LEN]);

impl SpiCompoundWaitStatus {
    /// Encode this truncated status to its 4-byte wire representation.
    //fusa:req REQ-SPI-011
    pub fn encode(self) -> [u8; SPI_COMPOUND_WAIT_STATUS_LEN] {
        self.0
    }

    /// Decode an [`SpiCompoundWaitStatus`] from a byte slice.
    ///
    /// Never panics on short, truncated, or arbitrary input — always
    /// returns `Err(RcpError::ShortFrame)` for input shorter than
    /// [`SPI_COMPOUND_WAIT_STATUS_LEN`] instead.
    //fusa:req REQ-SPI-011
    pub fn decode(b: &[u8]) -> Result<Self, RcpError> {
        if b.len() < SPI_COMPOUND_WAIT_STATUS_LEN {
            return Err(RcpError::ShortFrame);
        }
        let mut buf = [0u8; SPI_COMPOUND_WAIT_STATUS_LEN];
        buf.copy_from_slice(&b[0..SPI_COMPOUND_WAIT_STATUS_LEN]);
        Ok(Self(buf))
    }
}

/// Truncate a full 20-byte [`SpiStatus`] to the 4-byte
/// [`SpiCompoundWaitStatus`] shape compound-wait's status truncation rule
/// names.
///
/// See this module's doc comment "Provenance note: the compound-wait status
/// truncation" for why this takes the leading four bytes specifically, and
/// for why this function is standalone plumbing not wired into any
/// compound-wait execution path (`ROADMAP.md` Milestone 5, not yet built).
/// Never panics for any input.
//fusa:req REQ-SPI-012
pub fn truncate_spi_status_for_compound_wait(status: SpiStatus) -> SpiCompoundWaitStatus {
    let mut buf = [0u8; SPI_COMPOUND_WAIT_STATUS_LEN];
    buf.copy_from_slice(&status.0[0..SPI_COMPOUND_WAIT_STATUS_LEN]);
    SpiCompoundWaitStatus(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── SpiChannelSelect: sub_opcode round-trip ─────────────────────────────

    const ALL_CHANNEL_SELECTS: [SpiChannelSelect; 8] = [
        SpiChannelSelect::Channel0,
        SpiChannelSelect::Channel1,
        SpiChannelSelect::Channel2,
        SpiChannelSelect::Channel3,
        SpiChannelSelect::Channel4,
        SpiChannelSelect::Channel5,
        SpiChannelSelect::Reserved6,
        SpiChannelSelect::Reconfigure7,
    ];

    #[test]
    //fusa:test REQ-SPI-001
    fn spi_channel_select_sub_opcode_round_trips_for_all_eight_values() {
        for select in ALL_CHANNEL_SELECTS {
            let raw = select.to_sub_opcode();
            assert!(raw <= crate::acf::EVT_SUB_OPCODE_MAX);
            assert_eq!(SpiChannelSelect::from_sub_opcode(raw), Ok(select));
        }
    }

    #[test]
    //fusa:test REQ-SPI-001
    fn spi_channel_select_sub_opcode_values_are_the_full_0_to_7_range() {
        let mut raws: Vec<u8> = ALL_CHANNEL_SELECTS
            .iter()
            .map(|s| s.to_sub_opcode())
            .collect();
        raws.sort_unstable();
        assert_eq!(raws, (0u8..=7).collect::<Vec<_>>());
    }

    #[test]
    //fusa:test REQ-SPI-002
    fn spi_channel_select_from_sub_opcode_rejects_out_of_range() {
        for raw in [8u8, 9, 0x7F, 0xFF] {
            assert_eq!(
                SpiChannelSelect::from_sub_opcode(raw),
                Err(RcpError::InvalidParameter)
            );
        }
    }

    #[test]
    //fusa:test REQ-SPI-003
    fn spi_channel_select_is_named_true_only_for_the_six_named_channels() {
        for select in ALL_CHANNEL_SELECTS {
            let expected = !matches!(
                select,
                SpiChannelSelect::Reserved6 | SpiChannelSelect::Reconfigure7
            );
            assert_eq!(select.is_named(), expected);
        }
    }

    #[test]
    //fusa:test REQ-SPI-001
    fn spi_channel_select_reserved6_and_reconfigure7_are_distinct_values() {
        // Issue #100: the reserved and reconfigure high codes must no longer
        // be interchangeable "spare" values — they are distinct `sub_opcode`
        // values with distinct, spec-confirmed meanings.
        assert_ne!(SpiChannelSelect::Reserved6, SpiChannelSelect::Reconfigure7);
        assert_eq!(SpiChannelSelect::Reserved6.to_sub_opcode(), 6);
        assert_eq!(SpiChannelSelect::Reconfigure7.to_sub_opcode(), 7);
    }

    // ── resolve_spi_channel_index ────────────────────────────────────────────

    #[test]
    //fusa:test REQ-SPI-004
    fn resolve_spi_channel_index_maps_named_channels_to_0_through_5() {
        let expected = [
            (SpiChannelSelect::Channel0, 0usize),
            (SpiChannelSelect::Channel1, 1),
            (SpiChannelSelect::Channel2, 2),
            (SpiChannelSelect::Channel3, 3),
            (SpiChannelSelect::Channel4, 4),
            (SpiChannelSelect::Channel5, 5),
        ];
        for (select, index) in expected {
            assert_eq!(resolve_spi_channel_index(select), Ok(index));
        }
    }

    #[test]
    //fusa:test REQ-SPI-004
    fn resolve_spi_channel_index_refuses_reserved_and_reconfigure() {
        for select in [SpiChannelSelect::Reserved6, SpiChannelSelect::Reconfigure7] {
            assert_eq!(
                resolve_spi_channel_index(select),
                Err(RcpError::UnsupportedCmd)
            );
        }
    }

    // ── SpiFunctionalConfig / select_spi_channel_config ─────────────────────

    #[test]
    //fusa:test REQ-SPI-005
    fn spi_functional_config_has_exactly_six_channel_slots() {
        let config = SpiFunctionalConfig::default();
        assert_eq!(config.channels.len(), SPI_CHANNEL_COUNT);
        assert_eq!(SPI_CHANNEL_COUNT, 6);
    }

    #[test]
    //fusa:test REQ-SPI-006
    fn select_spi_channel_config_resolves_named_channels() {
        let config = SpiFunctionalConfig::default();
        for select in [
            SpiChannelSelect::Channel0,
            SpiChannelSelect::Channel1,
            SpiChannelSelect::Channel2,
            SpiChannelSelect::Channel3,
            SpiChannelSelect::Channel4,
            SpiChannelSelect::Channel5,
        ] {
            assert_eq!(
                select_spi_channel_config(select, &config),
                Ok(SpiChannelConfigSlot)
            );
        }
    }

    #[test]
    //fusa:test REQ-SPI-006
    fn select_spi_channel_config_refuses_reserved_and_reconfigure_selections() {
        let config = SpiFunctionalConfig::default();
        for select in [SpiChannelSelect::Reserved6, SpiChannelSelect::Reconfigure7] {
            assert_eq!(
                select_spi_channel_config(select, &config),
                Err(RcpError::UnsupportedCmd)
            );
        }
    }

    #[test]
    //fusa:test REQ-SPI-007
    fn spi_functional_config_layer_tag_matches_ep_type_spi() {
        let functional = SpiFunctionalConfig::default();
        let generic = crate::regmap::PerEpConfigBlock::new(crate::regmap::EndpointType::Spi);
        let tag = functional.layer_tag();
        assert_eq!(tag.ep_type, crate::regmap::EndpointType::Spi);
        assert!(crate::regmap::functional_config_matches_ep_type(
            &generic, &tag
        ));
        assert_eq!(
            crate::regmap::check_functional_config_matches_ep_type(&generic, &tag),
            Ok(())
        );
    }

    #[test]
    //fusa:test REQ-SPI-007
    fn spi_functional_config_layer_tag_rejects_mismatched_ep_type() {
        let functional = SpiFunctionalConfig::default();
        let generic = crate::regmap::PerEpConfigBlock::new(crate::regmap::EndpointType::Gpio);
        let tag = functional.layer_tag();
        assert!(!crate::regmap::functional_config_matches_ep_type(
            &generic, &tag
        ));
    }

    // ── SpiByteTransfer / SpiByteTransferResult: round-trip / never-panic ──

    #[test]
    //fusa:test REQ-SPI-008
    fn spi_byte_transfer_round_trips_through_encode_decode() {
        for bytes in [vec![], vec![0x00], vec![0xAA; 3], (0u8..=255).collect()] {
            let transfer = SpiByteTransfer {
                pico: bytes.clone(),
            };
            assert_eq!(SpiByteTransfer::decode(&transfer.encode()).pico, bytes);
        }
    }

    #[test]
    //fusa:test REQ-SPI-008
    fn spi_byte_transfer_decode_never_panics_for_any_sampled_input() {
        for len in [0usize, 1, 2, 7, 64] {
            let buf = vec![0x5Au8; len];
            let _ = SpiByteTransfer::decode(&buf);
        }
    }

    #[test]
    //fusa:test REQ-SPI-009
    fn spi_byte_transfer_result_round_trips_through_encode_decode() {
        for bytes in [vec![], vec![0xFF], vec![0x01, 0x02, 0x03]] {
            let result = SpiByteTransferResult {
                poci: bytes.clone(),
            };
            assert_eq!(SpiByteTransferResult::decode(&result.encode()).poci, bytes);
        }
    }

    #[test]
    //fusa:test REQ-SPI-009
    fn spi_byte_transfer_result_decode_never_panics_for_any_sampled_input() {
        for len in [0usize, 1, 5, 32] {
            let buf = vec![0xA5u8; len];
            let _ = SpiByteTransferResult::decode(&buf);
        }
    }

    // ── SpiStatus / SpiCompoundWaitStatus: round-trip / never-panic ────────

    fn sample_status() -> SpiStatus {
        let mut buf = [0u8; SPI_STATUS_LEN];
        for (i, b) in buf.iter_mut().enumerate() {
            *b = i as u8;
        }
        SpiStatus(buf)
    }

    #[test]
    //fusa:test REQ-SPI-010
    fn spi_status_round_trips_through_encode_decode() {
        let status = sample_status();
        assert_eq!(SpiStatus::decode(&status.encode()).unwrap(), status);
    }

    #[test]
    //fusa:test REQ-SPI-010
    fn spi_status_decode_rejects_short_input() {
        for len in 0..SPI_STATUS_LEN {
            let short = vec![0xAAu8; len];
            assert_eq!(SpiStatus::decode(&short), Err(RcpError::ShortFrame));
        }
    }

    #[test]
    //fusa:test REQ-SPI-010
    fn spi_status_decode_ignores_trailing_bytes() {
        let mut b = sample_status().encode().to_vec();
        b.extend_from_slice(&[0xFF, 0xFF]);
        assert_eq!(SpiStatus::decode(&b).unwrap(), sample_status());
    }

    #[test]
    //fusa:test REQ-SPI-011
    fn spi_compound_wait_status_round_trips_through_encode_decode() {
        let status = SpiCompoundWaitStatus([0x00, 0x01, 0x02, 0x03]);
        assert_eq!(
            SpiCompoundWaitStatus::decode(&status.encode()).unwrap(),
            status
        );
    }

    #[test]
    //fusa:test REQ-SPI-011
    fn spi_compound_wait_status_decode_rejects_short_input() {
        for len in 0..SPI_COMPOUND_WAIT_STATUS_LEN {
            let short = vec![0xAAu8; len];
            assert_eq!(
                SpiCompoundWaitStatus::decode(&short),
                Err(RcpError::ShortFrame)
            );
        }
    }

    // ── truncate_spi_status_for_compound_wait ───────────────────────────────

    #[test]
    //fusa:test REQ-SPI-012
    fn truncate_spi_status_for_compound_wait_keeps_the_leading_four_bytes() {
        let status = sample_status();
        let truncated = truncate_spi_status_for_compound_wait(status);
        assert_eq!(truncated.0, [0u8, 1, 2, 3]);
        assert_eq!(&truncated.0[..], &status.0[0..SPI_COMPOUND_WAIT_STATUS_LEN]);
    }

    #[test]
    //fusa:test REQ-SPI-012
    fn truncate_spi_status_for_compound_wait_never_panics_for_any_sampled_input() {
        let samples = [
            [0u8; SPI_STATUS_LEN],
            [0xFFu8; SPI_STATUS_LEN],
            sample_status().0,
        ];
        for sample in samples {
            let _ = truncate_spi_status_for_compound_wait(SpiStatus(sample));
        }
    }
}
