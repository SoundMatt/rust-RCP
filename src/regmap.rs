//fusa:req REQ-RMAP-001
//fusa:req REQ-RMAP-002
//fusa:req REQ-RMAP-003
//fusa:req REQ-RMAP-004
//fusa:req REQ-RMAP-005
//fusa:req REQ-RMAP-006
//fusa:req REQ-RMAP-007
//fusa:req REQ-RMAP-008
//fusa:req REQ-RMAP-009
//fusa:req REQ-RMAP-010
//fusa:req REQ-RMAP-011
//fusa:req REQ-RMAP-012
//fusa:req REQ-RMAP-013
//fusa:req REQ-RMAP-014
//fusa:req REQ-RMAP-015
//fusa:req REQ-RMAP-016
//fusa:req REQ-RMAP-017
//fusa:req REQ-RMAP-018
//fusa:req REQ-RMAP-019
//fusa:req REQ-RMAP-020
//fusa:req REQ-RMAP-021
//fusa:req REQ-RMAP-022
//fusa:req REQ-RMAP-023
//fusa:req REQ-RMAP-024
//fusa:req REQ-RMAP-025
//fusa:req REQ-RMAP-026
//fusa:req REQ-RMAP-027
//fusa:req REQ-RMAP-028
//fusa:req REQ-RMAP-029
//fusa:req REQ-RMAP-030

//! Three-layer per-endpoint config taxonomy, the RC Server's general
//! (whole-server) register-map fields, and the five child config tables
//! `§3.6`'s pointer/capacity rows point at — TC18 register-map model
//! (`ROADMAP.md` Milestone 2, "Register Map" subsection, all three items).
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
//! Map" subsection's second checklist bullet, citing `§3.6`.
//!
//! This module's third and final item gives each of the five child config
//! tables [`GeneralRegisters`]'s own pointer/capacity (or, for one table,
//! pointer-only) rows point at its own row-content type: [`HwPinMappingEntry`]
//! (`§3.7`), [`RequestStreamConfigEntry`] (`§3.8`), [`EpByteBusIdMapEntry`]
//! (`§3.9`), [`ResponseStreamConfigEntry`] (`§3.10`), and
//! [`SequencerStateEntry`] (`§3.11`) — see "Config tables (`§3.7`-`§3.11`)"
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
//!   `ROADMAP.md` Milestone 4's own closing checklist bullet ("Generic
//!   `evt[2:0]` group conventions ... and the shared common
//!   functional-config fields") names three concrete examples for this
//!   layer — `ep_enable`, `ep_clear_req_storage`, `ep_req_crc_enable` —
//!   and this item gives exactly those three a field, no more. See
//!   [`CommonFunctionalConfig`]'s own doc comment for why the bullet's
//!   trailing "etc." is left unenumerated rather than guessed at.
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
//! `Dac`'s special status an unremarked comment, and [`check_ep_type_supported`]
//! (Milestone 7's own closing DAC bullet) goes one step further, giving
//! `is_reserved` a real structural consequence for the three functions that
//! would otherwise construct/validate a `Dac`-tagged config indistinguishably
//! from any implemented endpoint type — see [`EndpointType::Dac`]'s own doc
//! comment for the full closing narrative.
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
//! `RcpError::InvalidParameter` is Milestone 2's "Error Model" item's TC18
//! spec error code for [`check_functional_config_matches_ep_type`] — this
//! function originally returned a crate-invented `EndpointTypeMismatch`
//! sentinel, since remapped onto the same spec code the lifecycle guard
//! rejections in `crate::lifecycle` now use; see [`crate::RcpError`]'s own
//! doc comment for the full provenance/mapping note, including why the
//! three collapse onto one code rather than staying distinct.
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
//! form, matching the same big-endian convention this crate uses for every
//! other multi-byte field it encodes (including `crate::avtp`'s NTSCF/TSCF
//! headers, which absorbed the role `crate::wire` — deleted by `ROADMAP.md`
//! Milestone 9's `wire` REPLACE cutover — used to serve).
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
//!
//!   **Known divergence (recorded, not fixed).** TC18 v0.5.1_RC §12.7.5
//!   Table 18 "RC Server configuration static part" (pp.51-53) *does*
//!   carry an "Absolute address" column, and this sequential packing does
//!   not reproduce it: Table 18 has an 8-bit `reserved` at `0x0017` (so
//!   `svr_io_pin_count` sits at `0x0018`) and a 16-bit `reserved` at
//!   `0x0022`; `svr_request_stream_cfg_capacity`/
//!   `svr_response_stream_cfg_capacity` are 8-bit and sit at
//!   `0x001C`/`0x001D`, *before* their 16-bit pointers at `0x001E`/
//!   `0x0020`; `svr_hw_cfg_ptr` (`0x001A`) has no paired capacity row at
//!   all; `svr_ep_generic_cfg_capacity` is 16-bit at `0x0026`; and
//!   `svr_ep_bytebus_id_map_capacity` is 8-bit at `0x002A`. The
//!   [`TableDescriptor`] shape (adjacent 16-bit ptr+capacity) therefore
//!   matches none of those rows, and [`GeneralRegisters::ENCODED_LEN`] is
//!   65 bytes where Table 18 spans `0x0000`-`0x003F`, i.e. 64 (its final
//!   eight 16-bit rows print no address cell, but the table's own
//!   page-break continuation marker gives the next address as `0x0030`).
//!   This is
//!   recorded as an explicit not-implemented requirement rather than
//!   silently reshaped here; see `REQ-RMAP-040` through `REQ-RMAP-045` in
//!   `.fusa-reqs.json`.
//! - **`svr_implemented_options` mostly left undecomposed.** The extraction
//!   names five option bundles the bitmask covers (compound&wait /
//!   triggered / chained / time-sync&timed / enhanced-cancel) but no
//!   bit-position assignment for any of them. Rather than invent an
//!   ordering this crate has no textual basis for, the raw
//!   [`GeneralRegisters::svr_implemented_options`] field is kept as a plain
//!   `u8`; named per-bit accessors are still deferred to whichever later
//!   item first needs to test a specific optional-feature bundle against a
//!   real bit position. `ROADMAP.md` Milestone 5's "Feature-bundle gating"
//!   checklist bullet — [`crate::request::check_compound_bundle_claim`] —
//!   is the first such item, needing to know which single bit the
//!   "compound request support" bundle occupies. Per Guiding Principle 5,
//!   this crate assigns the five bundles to bits `0`-`4` in the same
//!   top-to-bottom order the extraction itself lists them (compound&wait =
//!   bit `0`, triggered = bit `1`, chained = bit `2`, time-sync&timed = bit
//!   `3`, enhanced-cancel = bit `4`). TC18 §12.7.5 Table 18 writes the
//!   `svr_implemented_options` byte as `abcdefgh` with `a` = compound &
//!   wait requests, `b` = trigger requests, `c` = chained requests, `d` =
//!   time synch and timed requests, `e` = enhanced request cancellation —
//!   a left-to-right naming whose bit-numbering direction the extracted
//!   text does not make unambiguous (`a` may be bit 7 or bit 0). This
//!   crate's assignment therefore remains a crate-local placeholder
//!   ordering, not a confirmed bit-position assignment (see
//!   `REQ-RMAP-041`), and reconciled against a real
//!   RC Server (never against spec prose) before being relied on for
//!   interop, the same caveat as the sequential-byte-packing inference
//!   above. [`GeneralRegisters::claims_compound_wait_bundle`] is the one
//!   named per-bit accessor built on that placeholder assignment so far —
//!   bit `0` only; the other four bundles stay unaddressed by a named
//!   accessor until a later item needs one.
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
//!
//! ## Config tables (`§3.7`-`§3.11`)
//!
//! [`HwPinMappingEntry`], [`RequestStreamConfigEntry`],
//! [`EpByteBusIdMapEntry`], [`ResponseStreamConfigEntry`], and
//! [`SequencerStateEntry`] each model one row of the child config table its
//! corresponding [`GeneralRegisters`] pointer field reaches:
//! [`GeneralRegisters::svr_hw_cfg`] (`§3.7`),
//! [`GeneralRegisters::svr_request_stream_cfg`] (`§3.8`),
//! [`GeneralRegisters::svr_ep_bytebus_id_map`] (`§3.9`),
//! [`GeneralRegisters::svr_response_stream_cfg`] (`§3.10`), and
//! [`GeneralRegisters::svr_sequencer_state_ptr`] (`§3.11`) respectively.
//! Every row type gets the same never-panicking, fixed-length encode/decode
//! treatment [`TableDescriptor`]/[`GeneralRegisters`] already established;
//! [`ConfigTableRow`]/[`encode_rows`]/[`decode_rows`] additionally give the
//! five row types one shared, generic way to pack/unpack an entire table as
//! a flat run of fixed-length rows, rather than five copies of the same
//! chunking loop.
//!
//! A table's *row count* is not carried inside the row type itself — four
//! of the five tables already have it from the paired `capacity` field on
//! their [`GeneralRegisters`] [`TableDescriptor`] (cross-referenced, but not
//! asserted equal to, the more specific per-table capacity fields
//! [`GeneralRegisters::svr_io_pin_count`],
//! [`GeneralRegisters::svr_req_stream_max`], and
//! [`GeneralRegisters::svr_responder_streams_max`] already carry); the
//! fifth, [`SequencerStateEntry`], has no paired capacity field at all
//! (`svr_sequencer_state_ptr` is pointer-only, per this module's doc
//! comment above), so its row count comes from
//! [`GeneralRegisters::svr_sequencers_max`] instead — the same bound this
//! crate's own Milestone 5 lifecycle work is expected to use for
//! sequencer-state ("power-on default state 1, bounded by
//! `svr_sequencers_max`"). None of that cross-referencing is enforced by
//! this module: it performs no register I/O and holds no reference to a
//! live [`GeneralRegisters`] value, so it cannot itself check a decoded
//! table's length against any of these bounds — that check is a later
//! caller's job once one exists.
//!
//! **`§3.9` is a client-side ordering responsibility, not a server-side
//! safety net.** [`EpByteBusIdMapEntry::is_end_of_table`] recognizes the
//! documented end-of-table sentinel row (`map_stream_index == 0`, which
//! this crate's own extraction of `§3.9` also records as doubling for the
//! default EP0 mapping) — a fixed, stated wire convention, not a validation
//! rule this crate invented. What this module deliberately does **not**
//! add is any check that a table's rows are kept in ascending order:
//! per `ROADMAP.md`'s own parenthetical for this checklist bullet
//! ("client-side ordering responsibility, no server-side safety net per
//! spec"), maintaining that order is exclusively the writing client's
//! responsibility, and this crate's own `§3.9` extraction independently
//! notes that ordering violations are implementation-defined with no
//! corrective mechanism specified. Inventing a sorting/validation gate here
//! would be inventing behavior the specification does not require, per
//! Guiding Principle 5 — so none exists.
//!
//! Like every prior Milestone 1/2 entry, all five row types and the
//! [`encode_rows`]/[`decode_rows`] helpers are purely additive: none of
//! this performs register I/O against a real RC Server, and none of it is
//! wired into [`crate::ep0`]'s dispatch path, [`crate::lifecycle`]'s
//! reachability checks, or any other existing caller. Completing this item
//! closed out the "Register Map" subsection of `ROADMAP.md` Milestone 2;
//! that milestone's separate "Error Model" item — which remapped this
//! module's own `EndpointTypeMismatch` sentinel onto
//! [`crate::RcpError::InvalidParameter`] — has since landed too.
//!
//! ### Config tables provenance note
//!
//! **Corrected in v5.0.0.** Earlier revisions of this note claimed the
//! specification recorded these tables' field *names* and *purpose* only,
//! with "no explicit per-field bit-width or byte-offset table" and "no
//! textual basis for a specific bit-position assignment" — and every layout
//! below was built on that claim. It was false. TC18 v0.5.1_RC's own
//! §12.7.7 Table 22, §12.7.8 Table 23, and §12.7.10 Table 25 each carry a
//! "Relative address" column, a "Type" column giving each field's exact bit
//! width, and — for Table 22 — `0x000D.0` through `0x000D.7` bit addresses
//! naming each flag's position within a shared byte. Three of the five row
//! types were laid out wrongly as a result; see the CHANGELOG for v5.0.0.
//! Each row type's own doc comment now reproduces the relevant table's
//! address/width columns and cites its section and page.
//!
//! Row stride is likewise not inferred: each of these tables tabulates the
//! *next* row's first field, which fixes the stride exactly
//! ([`RequestStreamConfigEntry`] 24 bytes from Table 22's `rx_stream_id2`
//! at `0x0018`, [`EpByteBusIdMapEntry`] 4 bytes from Table 23's
//! `2_Request_Stream_Index` at `0x0004`, [`SequencerStateEntry`] 2 bytes
//! from Table 25's `Seq_2` at `0x0002`).
//!
//! What genuinely does remain this crate's own inference, flagged here per
//! Guiding Principle 5 rather than presented as settled:
//!
//! - [`HwPinMappingEntry::hw_pin_props`] is left an undecomposed raw `u8`,
//!   the same choice [`GeneralRegisters::svr_implemented_options`] already
//!   made for a packed multi-property byte.
//! - Reserved blocks ([`RequestStreamConfigEntry`]'s `0x0012` and `0x0014`)
//!   are written as zero and ignored on decode rather than carried as
//!   round-tripping fields — the same treatment [`crate::avtp`]'s TSCF
//!   reserved quadlets get. A row therefore round-trips its *specified*
//!   fields, not arbitrary reserved content.
//!
//! [`ResponseStreamConfigEntry`]'s own layout has since been reconciled
//! against TC18 §12.7.9 Table 24 "Responder QUEUE_config" (p.60) and
//! matches it exactly, field-for-field and stride-for-stride — see that
//! type's own doc comment. What Table 24 additionally states, and this
//! crate does **not** yet model, is that `STREAM_UID`'s 16 bits are the
//! stream identifier's bits `[63:48]` (see `REQ-RMAP-037`).
//!
//! ## `Serialize`/`Deserialize` derives (`ROADMAP.md` Milestone 9, `config`
//! ## REPLACE cutover)
//!
//! [`TableDescriptor`], [`GeneralRegisters`], and the five `§3.7`-`§3.11`
//! row types additionally derive `serde`'s `Serialize`/`Deserialize` here,
//! rather than [`crate::config`] duplicating their field lists in a
//! parallel loader-only shape. This is purely additive — no encode/decode
//! byte layout, wire behavior, or existing derive changes; it only gives
//! this module's own types a JSON/YAML-loadable surface for
//! [`crate::config`] to compose. See [`crate::config`]'s own doc comment
//! for what that composition builds on top of it.

use serde::{Deserialize, Serialize};

use crate::lifecycle::RegisterCategory;
use crate::RcpError;

// ── EndpointType ─────────────────────────────────────────────────────────────

/// The register map's own per-endpoint type discriminant ("`ep_type`"), per
/// `ROADMAP.md` Milestones 4 and 7.
///
/// See this module's doc comment for why EP0 has no variant here, and for
/// [`EndpointType::Dac`]'s reserved status.
///
/// `#[non_exhaustive]` (`ROADMAP.md` Milestone 10, "Public API stability
/// guarantees"): the `ep_type` byte's value space (`0x01..=0x0D` covered
/// here) is a specification-defined enumeration this crate's own
/// spec-extraction pass reads as still having unassigned codes above
/// `0x0D` — a future OPEN Alliance TC18 revision assigning one is not a
/// change this crate controls. Matching on this enum from outside this
/// crate MUST include a wildcard arm; see `docs/SEMVER.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
#[non_exhaustive]
//fusa:req REQ-RMAP-001
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
    /// [`EndpointType::is_reserved`] and [`check_ep_type_supported`].
    ///
    /// Done (v0.10.0-dev): closes `ROADMAP.md` Milestone 7's DAC bullet.
    /// The type code and a `DAC_OUT` pin signal exist in the register-map
    /// enumeration this crate's own spec-extraction pass recovered, but no
    /// functional-config chapter or request semantics for DAC turned up
    /// anywhere in that pass — an actual gap in what has been specified,
    /// not a gap in this crate's reading of it. Per Guiding Principle 5,
    /// this crate records that as an explicit decision rather than
    /// inventing a plausible-sounding register layout to fill it: `Dac`
    /// stays enumerated (the code itself is real), [`EndpointType::is_reserved`]
    /// flags it, and [`check_ep_type_supported`] gives that flag a
    /// structural consequence — every caller that reaches config
    /// construction/validation through it is rejected with
    /// `RcpError::UnsupportedCmd` rather than silently accepted as if DAC
    /// were an ordinary, implemented endpoint type. Tracked as a follow-up
    /// pending an OPEN Alliance TC18 clarification or a later spec
    /// revision; no `src/dac.rs` module exists and none should be added
    /// until that clarification lands.
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
    ///
    /// Every code produced here is the one TC18 v0.5.1_RC §13.2 Table 29
    /// "ep_type values" (p.73) assigns to the same endpoint type.
    //fusa:req REQ-RMAP-001
    //fusa:req REQ-EPGEN-001
    pub fn to_u8(self) -> u8 {
        self as u8
    }

    /// Decode a wire-level `ep_type` byte value into an [`EndpointType`].
    ///
    /// Accepts exactly TC18 §13.2 Table 29's codes `0x01` (WakeUp Ctrl)
    /// through `0x0D` (MDIO). Returns `Err(RcpError::Other(_))` for any
    /// byte outside that range, mirroring
    /// [`crate::lifecycle::RcServerState::from_u8`]'s handling of an
    /// unrecognized state byte. Never panics for any input.
    ///
    /// Table 29 additionally assigns `0x00` to "Server"; this crate does
    /// not model it, so an `ep_generic_config` row carrying `ep_type
    /// 0x00` is rejected here rather than decoded — recorded as
    /// `REQ-EPGEN-002`, not resolved.
    //fusa:req REQ-RMAP-002
    //fusa:req REQ-EPGEN-001
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
                "regmap: unrecognized ep_type byte 0x{other:02X} (expected 0x01..=0x0D)"
            ))),
        }
    }

    /// Is this endpoint type explicitly reserved and out of scope for the
    /// current replacement cycle?
    ///
    /// True only for [`EndpointType::Dac`] — see this module's doc comment.
    /// Never panics for any input.
    //fusa:req REQ-RMAP-001
    pub fn is_reserved(self) -> bool {
        matches!(self, Self::Dac)
    }
}

/// Does this crate support constructing/validating config for `ep_type`?
///
/// Returns `Err(RcpError::UnsupportedCmd)` for every [`EndpointType`] with
/// [`EndpointType::is_reserved`] true — today, [`EndpointType::Dac`] only —
/// and `Ok(())` otherwise. Never panics for any input.
///
/// This is the structural enforcement [`EndpointType::is_reserved`] itself
/// does not provide on its own: without this function,
/// [`PerEpConfigBlock::new`], [`PerEpTypeFunctionalConfig::new`], and
/// [`check_functional_config_matches_ep_type`] all happily construct/accept
/// a fully-formed config pair tagged [`EndpointType::Dac`], indistinguishable
/// from any ordinary, implemented endpoint type. `ROADMAP.md`'s own DAC
/// bullet explicitly forbids inventing a register layout for it, so a
/// caller that reaches this function with a reserved `ep_type` gets the
/// same "recognized on the wire but not supported by this crate" rejection
/// [`crate::gpio::GpioWriteSemantics::Unnamed8th`],
/// [`crate::adc::AdcSamplingMode::Continuous`], and
/// [`crate::fragment`]'s zero-`rx_stream_max_request_size` sentinel already
/// use for the same shape of situation, rather than a distinct, one-off
/// error path.
///
/// Deliberately kept separate from [`check_functional_config_matches_ep_type`]
/// rather than folded into it: that function's own existing contract
/// (REQ-RMAP-004 — returns `Ok(())` whenever `generic`/`per_type` carry the
/// same `EndpointType`, `Err(RcpError::InvalidParameter)` otherwise) already
/// covers a same-`EndpointType` pair unconditionally, including a matching
/// pair of `Dac`-tagged layers; changing that function's behavior for `Dac`
/// would silently narrow an existing, already-tested requirement instead of
/// adding a new one. Callers that need both checks call this function
/// alongside [`check_functional_config_matches_ep_type`], not instead of it.
//fusa:req REQ-RMAP-031
pub fn check_ep_type_supported(ep_type: EndpointType) -> Result<(), RcpError> {
    if ep_type.is_reserved() {
        Err(RcpError::UnsupportedCmd)
    } else {
        Ok(())
    }
}

// ── The three config layers ──────────────────────────────────────────────────

/// The generic, server-owned per-endpoint config block: present for every
/// endpoint regardless of [`EndpointType`].
///
/// See this module's doc comment for why `ep_type` is the only field this
/// item gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
//fusa:req REQ-RMAP-003
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
    //fusa:req REQ-RMAP-003
    pub const LAYER: ConfigLayer = ConfigLayer::Generic;
}

/// The common functional-config block: fields shared across every
/// [`EndpointType`]'s functional config.
///
/// Models the three concrete examples `ROADMAP.md` Milestone 4's own
/// closing checklist bullet names by name — `ep_enable`,
/// `ep_clear_req_storage`, `ep_req_crc_enable` — and no more. That
/// bullet's own wording ends in "etc.", implying a longer, unenumerated
/// field list, but this crate has no textual source (its own
/// spec-extraction pass records no further field names for this
/// particular checklist item, unlike e.g. `§3.6`'s or `§3.8`'s own fully
/// enumerated tables) to draw the remainder from. Per Guiding Principle 5,
/// this type therefore claims only the three fields actually named, rather
/// than inventing plausible-sounding neighbors to fill out "etc." —
/// mirroring how [`crate::gpio::GpioWriteSemantics::Unnamed8th`] left an
/// unconfirmed slot unnamed instead of guessing a name for it. A later item
/// that does recover the remaining field names (against this crate's own
/// spec-extraction pass, never against restated spec prose) is expected to
/// extend this struct then, not now.
///
/// [`CommonFunctionalConfig::encode`]/[`CommonFunctionalConfig::decode`]
/// give this block a never-panicking, fixed-length wire form: each field
/// gets its own full byte (`0x00`/`0x01`), for a 3-byte block.
///
/// **Known divergence (recorded, not fixed).** Earlier revisions of this
/// doc comment justified that one-byte-per-field shape by asserting the
/// crate had "no basis for any bit-position assignment among these three
/// fields". That is no longer true: TC18 v0.5.1_RC §13.7 Table 32 "EP
/// functional config common entries" (p.80) supplies exactly that basis,
/// and this encoding does not match it. Table 32 defines **two**
/// bit-addressed registers — `ep_enable&clr` at relative address `0x0002`
/// (`.0` `ep_enable`, `.1:3` reserved reading `000b`, `.4`
/// `ep_clear_req_storage`) and `ep_options` at `0x0003` (`.0`
/// `ep_req_crc_enable`, `.1` `ep_ack_crc_enable`, `.2`
/// `ep_response_crc_enable`, `.3` `ep_response_ts_enable`, `.4`
/// `ep_error_stream`, `.5` `ep_ack_ts_enable`, `.6`
/// `ep_supress_error_msgs`, `.7` `ep_supress_response`) — so the three
/// fields modeled here belong at three specific *bits* of two bytes, not
/// at three whole bytes, and the seven remaining `ep_options` bits are
/// unmodeled entirely. Table 32 further documents `ep_enable`'s power-on
/// default as `1b` (this type's derived `Default` gives `false`) and
/// `ep_clear_req_storage` as write-1-to-clear, "reads always 0" (this type
/// round-trips it). Recorded as explicit not-implemented requirements
/// rather than silently reshaped here; see `REQ-RMAP-051` and
/// `REQ-RMAP-052` in `.fusa-reqs.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-RMAP-003
//fusa:req REQ-RMAP-028
pub struct CommonFunctionalConfig {
    /// Whether this endpoint is enabled.
    pub ep_enable: bool,
    /// Whether this endpoint's pending-request storage should be cleared.
    pub ep_clear_req_storage: bool,
    /// Whether this endpoint's request CRC checking is enabled.
    pub ep_req_crc_enable: bool,
}

impl CommonFunctionalConfig {
    /// The [`ConfigLayer`] this type always belongs to.
    //fusa:req REQ-RMAP-003
    pub const LAYER: ConfigLayer = ConfigLayer::CommonFunctional;

    /// Encoded wire length in bytes: one full byte per field, per this
    /// type's own doc comment.
    pub const ENCODED_LEN: usize = 3;

    /// Encode this config to its 3-byte wire representation:
    /// `ep_enable` then `ep_clear_req_storage` then `ep_req_crc_enable`,
    /// each a full `0x00`/`0x01` byte. Never panics.
    //fusa:req REQ-RMAP-028
    pub fn encode(&self) -> [u8; Self::ENCODED_LEN] {
        [
            self.ep_enable as u8,
            self.ep_clear_req_storage as u8,
            self.ep_req_crc_enable as u8,
        ]
    }

    /// Decode a [`CommonFunctionalConfig`] from a byte slice.
    ///
    /// Never panics on short, truncated, or arbitrary input — always
    /// returns `Err(RcpError::ShortFrame)` for input shorter than
    /// [`CommonFunctionalConfig::ENCODED_LEN`] instead. Any nonzero byte
    /// decodes as `true`, matching [`crate::acf::decode_byte_message_info`]'s
    /// own bit-to-`bool` convention of treating "nonzero" as set.
    //fusa:req REQ-RMAP-029
    pub fn decode(b: &[u8]) -> Result<Self, RcpError> {
        if b.len() < Self::ENCODED_LEN {
            return Err(RcpError::ShortFrame);
        }
        Ok(Self {
            ep_enable: b[0] != 0,
            ep_clear_req_storage: b[1] != 0,
            ep_req_crc_enable: b[2] != 0,
        })
    }
}

/// A distinct, type-specific functional-config shape for the
/// [`EndpointType`] it is `for`.
///
/// An empty placeholder beyond its [`EndpointType`] tag — see this module's
/// doc comment for why no concrete per-type field (GPIO's write-semantics,
/// SPI's channel configs, etc.) is modeled here yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
//fusa:req REQ-RMAP-003
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
    //fusa:req REQ-RMAP-003
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
//fusa:req REQ-RMAP-005
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
//fusa:req REQ-RMAP-005
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
//fusa:req REQ-RMAP-004
pub fn functional_config_matches_ep_type(
    generic: &PerEpConfigBlock,
    per_type: &PerEpTypeFunctionalConfig,
) -> bool {
    generic.ep_type == per_type.ep_type
}

/// Validating counterpart to [`functional_config_matches_ep_type`].
///
/// Returns `Ok(())` if `per_type` belongs to the same [`EndpointType`] as
/// `generic`, `Err(RcpError::InvalidParameter)` otherwise. Never panics
/// for any input.
//fusa:req REQ-RMAP-004
pub fn check_functional_config_matches_ep_type(
    generic: &PerEpConfigBlock,
    per_type: &PerEpTypeFunctionalConfig,
) -> Result<(), RcpError> {
    if functional_config_matches_ep_type(generic, per_type) {
        Ok(())
    } else {
        Err(RcpError::InvalidParameter)
    }
}

// ── TableDescriptor ──────────────────────────────────────────────────────────

/// A pointer + capacity pair for one of `§3.6`'s child config tables.
///
/// See this module's doc comment "`GeneralRegisters` provenance note" for
/// which `§3.6` rows this shape applies to (and which don't).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
//fusa:req REQ-RMAP-007
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
    //fusa:req REQ-RMAP-007
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
    //fusa:req REQ-RMAP-007
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
//fusa:req REQ-RMAP-008
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
    //fusa:req REQ-RMAP-009
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
    //fusa:req REQ-RMAP-010
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

    /// Bit position this crate assigns `svr_implemented_options`'s
    /// compound&wait option bundle within the placeholder five-bundle
    /// ordering this module's doc comment "`GeneralRegisters` provenance
    /// note" describes — not a confirmed spec bit position.
    const IMPLEMENTED_OPTIONS_COMPOUND_WAIT_BIT: u8 = 0;

    /// Whether `svr_implemented_options` claims the compound & compound-wait
    /// request-kind bundle (`ROADMAP.md` Milestone 5's "compound&wait" name
    /// for it): bit [`Self::IMPLEMENTED_OPTIONS_COMPOUND_WAIT_BIT`] of the
    /// raw bitmask.
    ///
    /// This only reports what the bitmask *claims*; it does not check
    /// whether that claim is honest. See
    /// [`crate::request::check_compound_bundle_claim`] for the "does the
    /// server that claims this bundle actually ship all three prerequisite
    /// pieces together" rule `ROADMAP.md`'s "Feature-bundle gating"
    /// checklist bullet names, and this module's doc comment "provenance
    /// note" for why bit `0` — rather than any other position — is this
    /// crate's own placeholder choice for this one bundle.
    //fusa:req REQ-RMAP-030
    pub fn claims_compound_wait_bundle(&self) -> bool {
        self.svr_implemented_options & (1 << Self::IMPLEMENTED_OPTIONS_COMPOUND_WAIT_BIT) != 0
    }
}

// ── ConfigTableRow / encode_rows / decode_rows ────────────────────────────────

/// Shared row-codec contract for this module's five `§3.7`-`§3.11` child
/// config-table row types.
///
/// Lets [`encode_rows`]/[`decode_rows`] pack/unpack a whole table as a flat
/// run of fixed-length rows once, generically, instead of five copies of
/// the same chunking loop. See this module's doc comment "Config tables"
/// section for what determines a table's row *count* (never carried inside
/// the row type itself).
pub trait ConfigTableRow: Sized {
    /// Encoded wire length of a single row, in bytes. Never zero for any
    /// type implementing this trait in this crate.
    const ROW_LEN: usize;

    /// Encode this row as a fixed-length byte block. Never panics.
    fn encode_row(&self) -> Vec<u8>;

    /// Decode a single row from the front of `bytes`.
    ///
    /// Returns `Err(RcpError::ShortFrame)` if `bytes` is shorter than
    /// `Self::ROW_LEN`. Never panics for any input.
    fn decode_row(bytes: &[u8]) -> Result<Self, RcpError>;
}

/// Encode every row in `rows`, back-to-back, with no padding between rows.
/// Never panics.
//fusa:req REQ-RMAP-027
pub fn encode_rows<T: ConfigTableRow>(rows: &[T]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(rows.len() * T::ROW_LEN);
    for row in rows {
        buf.extend_from_slice(&row.encode_row());
    }
    buf
}

/// Decode `bytes` as a flat run of fixed-length `T::ROW_LEN` rows.
///
/// Returns `Err(RcpError::ShortFrame)` if `bytes.len()` is not an exact
/// multiple of `T::ROW_LEN` (including a non-empty remainder shorter than
/// one row). An empty input decodes to an empty table. Never panics for any
/// input.
//fusa:req REQ-RMAP-027
pub fn decode_rows<T: ConfigTableRow>(bytes: &[u8]) -> Result<Vec<T>, RcpError> {
    debug_assert!(T::ROW_LEN > 0, "ConfigTableRow::ROW_LEN must not be zero");
    if T::ROW_LEN == 0 || bytes.len() % T::ROW_LEN != 0 {
        return Err(RcpError::ShortFrame);
    }
    let mut rows = Vec::with_capacity(bytes.len() / T::ROW_LEN);
    let mut off = 0usize;
    while off < bytes.len() {
        rows.push(T::decode_row(&bytes[off..off + T::ROW_LEN])?);
        off += T::ROW_LEN;
    }
    Ok(rows)
}

// ── HwPinMappingEntry (§3.7) ─────────────────────────────────────────────────

/// One row of the HW pin-mapping table (TC18 v0.5.1_RC §12.7.6 "HW pin
/// mapping configuration", Table 19 "HW_config", p.54): which endpoint owns
/// a physical I/O pin, which of that endpoint's named signal indices is
/// bound to it, and the pin's packed electrical properties.
///
/// # Wire layout (Table 19)
///
/// ```text
/// IO_Pin 1
///   0x0000  hw_ep_nr      8 bit  R/W*  "Endpoint Nr using this IO"
///   0x0001  hw_ep_pin_nr  8 bit  R/W*  "Endpoint Pin Nr mapped to this IO"
///   0x0002  hw_pin_type   8 bit  R/W*  "Properties of the IO Pin"
/// IO_Pin 2
///   0x0003  (next row)
/// ```
///
/// The row is 3 bytes, with the stride fixed by IO_Pin 2's own `hw_ep_nr`
/// at relative address `0x0003` (and IO_Pin 3's at `0x0006`).
///
/// See this module's doc comment "Config tables" section for this table's
/// row-count source ([`GeneralRegisters::svr_hw_cfg`]'s `capacity`,
/// cross-referenced against [`GeneralRegisters::svr_io_pin_count`]) and
/// "Config tables provenance note" for [`Self::hw_pin_props`]'s undecomposed
/// byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
//fusa:req REQ-RMAP-012
pub struct HwPinMappingEntry {
    /// Endpoint slot number this pin is assigned to.
    pub hw_ep_nr: u8,
    /// Index into that endpoint type's own named-signal enumeration (e.g.
    /// SPI's CLK/PICO/POCI/CS lines) that this pin is bound to.
    pub hw_ep_pin_nr: u8,
    /// Packed pin-electrical-property byte (pull-up/down/float, output
    /// stage, drive strength, Schmitt-trigger enable). Left undecomposed —
    /// see this module's doc comment provenance note.
    pub hw_pin_props: u8,
}

impl HwPinMappingEntry {
    /// Encoded wire length in bytes: Table 19's row stride, fixed by
    /// IO_Pin 2's `hw_ep_nr` at relative address `0x0003`.
    //fusa:req REQ-RMAP-032
    pub const ENCODED_LEN: usize = 3;

    /// The [`RegisterCategory`] this table's rows belong to.
    pub const CATEGORY: RegisterCategory = RegisterCategory::HwConfig;

    /// Encode as Table 19's 3-byte row, `[hw_ep_nr (0x0000),
    /// hw_ep_pin_nr (0x0001), hw_pin_type (0x0002)]`. Never panics.
    //fusa:req REQ-RMAP-013
    //fusa:req REQ-RMAP-032
    pub fn encode(&self) -> [u8; Self::ENCODED_LEN] {
        [self.hw_ep_nr, self.hw_ep_pin_nr, self.hw_pin_props]
    }

    /// Decode from the front of `bytes`.
    ///
    /// Returns `Err(RcpError::ShortFrame)` if `bytes` is shorter than
    /// [`Self::ENCODED_LEN`]. Trailing bytes beyond `ENCODED_LEN` are
    /// ignored. Never panics for any input.
    //fusa:req REQ-RMAP-014
    pub fn decode(bytes: &[u8]) -> Result<Self, RcpError> {
        if bytes.len() < Self::ENCODED_LEN {
            return Err(RcpError::ShortFrame);
        }
        Ok(Self {
            hw_ep_nr: bytes[0],
            hw_ep_pin_nr: bytes[1],
            hw_pin_props: bytes[2],
        })
    }
}

impl ConfigTableRow for HwPinMappingEntry {
    const ROW_LEN: usize = Self::ENCODED_LEN;

    fn encode_row(&self) -> Vec<u8> {
        self.encode().to_vec()
    }

    fn decode_row(bytes: &[u8]) -> Result<Self, RcpError> {
        Self::decode(bytes)
    }
}

// ── RequestStreamConfigEntry (§3.8) ──────────────────────────────────────────

/// One row of the request-stream config table (TC18 v0.5.1_RC §12.7.7
/// "Request stream configuration", Table 22, pp.57-58): the receive-side
/// configuration for one stream the RC Server listens on.
///
/// # Wire layout (Table 22)
///
/// Table 22 gives each field an explicit relative address, and the address
/// of the next stream's `rx_stream_id2` (`0x0018`) fixes the row stride at
/// **24 bytes**:
///
/// ```text
/// 0x0000  rx_stream_id                64 bit
/// 0x0008  rx_stream_max_request_size  16 bit
/// 0x000A  rx_wd_timeout_intervall     16 bit
/// 0x000C  rx_secure_channel_index      8 bit
/// 0x000D  flags byte, bit-addressed:
///           .0 rx_enforce_e2e             .4 rx_wd_safestate_enable
///           .1 rx_enforce_seq             .5 rx_ovrflw_safestate_enable
///           .2 rx_seq_safestate_enable    .6 rx_safety_measure
///           .3 rx_wd_enable               .7 rx_wd_info_enable
/// 0x000E  rx_safestate_seqencer        8 bit
/// 0x000F  rx_safe_sequencer_state      8 bit
/// 0x0010  rx_ack_stream_index          8 bit
/// 0x0011  rx_resp_stream_index         8 bit
/// 0x0012  Reserved                    16 bit
/// 0x0014  Reserved                    32 bit
/// 0x0018  (next row)
/// ```
///
/// The eight `1 bit` rows are the only fields Table 22 addresses with a
/// `.bit` suffix rather than a whole-byte address, and all eight share the
/// single address `0x000D` — so they pack into one byte, LSB first, in the
/// `.0`-`.7` order the table lists them. Releases before v5.0.0 gave each
/// flag a byte of its own and dropped the six reserved bytes entirely,
/// making the row 25 bytes and misplacing every field from `0x000D`
/// onward. See [`Self::FLAGS_OFFSET`] and the per-flag mask constants.
///
/// The two reserved blocks are not modeled as fields: they are written as
/// zero by [`Self::encode`] and ignored by [`Self::decode`], the same
/// treatment [`crate::avtp`]'s TSCF reserved quadlets get.
///
/// See this module's doc comment "Config tables" section for this table's
/// row-count source ([`GeneralRegisters::svr_request_stream_cfg`]'s
/// `capacity`, cross-referenced against
/// [`GeneralRegisters::svr_req_stream_max`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
//fusa:req REQ-RMAP-015
pub struct RequestStreamConfigEntry {
    /// 64-bit stream identifier this entry is bound to; a sentinel default
    /// value means "unconfigured, no reception."
    pub rx_stream_id: u64,
    /// Largest single fragmented request this stream will reassemble, in
    /// bytes; `0` means fragmentation is unsupported on this stream.
    pub rx_stream_max_request_size: u16,
    /// Per-stream watchdog timeout, in clock ticks.
    pub rx_wd_timeout_interval: u16,
    /// Required MACsec secure-channel index; `0` means no security /
    /// uncontrolled port.
    pub rx_secure_channel_index: u8,
    /// Flags bit `0x000D.0`. `false`: an E2E-CRC failure at an endpoint
    /// only drops that bad request. `true`: the stream is blocked until
    /// released and safe state is entered.
    pub rx_enforce_e2e: bool,
    /// Flags bit `0x000D.1`. Requests are only filed for execution if the
    /// AVTPDU sequence number is increased.
    pub rx_enforce_seq: bool,
    /// Flags bit `0x000D.2`. Bring all endpoints to safe state if
    /// `Sequence_Nr` has no single increment.
    pub rx_seq_safestate_enable: bool,
    /// Flags bit `0x000D.3`. Enable the watchdog for this stream.
    pub rx_wd_enable: bool,
    /// Flags bit `0x000D.4`. Bring all endpoints for this stream to safe
    /// state if the watchdog triggers.
    pub rx_wd_safestate_enable: bool,
    /// Flags bit `0x000D.5`. Bring all endpoints for this stream to safe
    /// state if the request storage of one endpoint overflows.
    pub rx_ovrflw_safestate_enable: bool,
    /// Flags bit `0x000D.6`. `false`: as safe state, set all I/O pins to
    /// high impedance. `true`: use sequencer-based safety requests.
    pub rx_safety_measure: bool,
    /// Flags bit `0x000D.7`. Send a repetitive notification when in safe
    /// state.
    pub rx_wd_info_enable: bool,
    /// Which sequencer number runs the safety sequence, when
    /// `rx_safety_measure` selects the sequencer-driven safe-state.
    pub rx_safestate_sequencer: u8,
    /// Target sequencer state that kicks off the safety-sequence requests.
    pub rx_safe_sequencer_state: u8,
    /// Which response/ack queue this stream's endpoints use for
    /// acknowledgements; `0` suppresses acknowledgements entirely.
    pub rx_ack_stream_index: u8,
    /// Which response/ack queue is used for data responses.
    pub rx_resp_stream_index: u8,
}

impl RequestStreamConfigEntry {
    /// Encoded wire length in bytes: Table 22's row stride, fixed by the
    /// next row's `rx_stream_id2` at relative address `0x0018`.
    pub const ENCODED_LEN: usize = 24;

    /// Byte offset of Table 22's single bit-addressed flags byte
    /// (relative address `0x000D`).
    pub const FLAGS_OFFSET: usize = 0x0D;

    /// Mask for `rx_enforce_e2e`, Table 22 bit `0x000D.0`.
    pub const FLAG_ENFORCE_E2E: u8 = 1 << 0;
    /// Mask for `rx_enforce_seq`, Table 22 bit `0x000D.1`.
    pub const FLAG_ENFORCE_SEQ: u8 = 1 << 1;
    /// Mask for `rx_seq_safestate_enable`, Table 22 bit `0x000D.2`.
    pub const FLAG_SEQ_SAFESTATE_ENABLE: u8 = 1 << 2;
    /// Mask for `rx_wd_enable`, Table 22 bit `0x000D.3`.
    pub const FLAG_WD_ENABLE: u8 = 1 << 3;
    /// Mask for `rx_wd_safestate_enable`, Table 22 bit `0x000D.4`.
    pub const FLAG_WD_SAFESTATE_ENABLE: u8 = 1 << 4;
    /// Mask for `rx_ovrflw_safestate_enable`, Table 22 bit `0x000D.5`.
    pub const FLAG_OVRFLW_SAFESTATE_ENABLE: u8 = 1 << 5;
    /// Mask for `rx_safety_measure`, Table 22 bit `0x000D.6`.
    pub const FLAG_SAFETY_MEASURE: u8 = 1 << 6;
    /// Mask for `rx_wd_info_enable`, Table 22 bit `0x000D.7`.
    pub const FLAG_WD_INFO_ENABLE: u8 = 1 << 7;

    /// The [`RegisterCategory`] this table's rows belong to.
    pub const CATEGORY: RegisterCategory = RegisterCategory::RcpConfig;

    /// Table 22's `0x000D` flags byte, assembled from the eight `bool`
    /// fields. Never panics.
    //fusa:req REQ-RMAP-016
    pub fn flags_byte(&self) -> u8 {
        let mut b = 0u8;
        if self.rx_enforce_e2e {
            b |= Self::FLAG_ENFORCE_E2E;
        }
        if self.rx_enforce_seq {
            b |= Self::FLAG_ENFORCE_SEQ;
        }
        if self.rx_seq_safestate_enable {
            b |= Self::FLAG_SEQ_SAFESTATE_ENABLE;
        }
        if self.rx_wd_enable {
            b |= Self::FLAG_WD_ENABLE;
        }
        if self.rx_wd_safestate_enable {
            b |= Self::FLAG_WD_SAFESTATE_ENABLE;
        }
        if self.rx_ovrflw_safestate_enable {
            b |= Self::FLAG_OVRFLW_SAFESTATE_ENABLE;
        }
        if self.rx_safety_measure {
            b |= Self::FLAG_SAFETY_MEASURE;
        }
        if self.rx_wd_info_enable {
            b |= Self::FLAG_WD_INFO_ENABLE;
        }
        b
    }

    /// Encode as Table 22's fixed-length, big-endian 24-byte row: each
    /// field at its tabulated relative address, the eight `1 bit` flags
    /// packed into the `0x000D` byte, and both reserved blocks written as
    /// zero. Never panics.
    //fusa:req REQ-RMAP-016
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

        put!(self.rx_stream_id); // 0x0000
        put!(self.rx_stream_max_request_size); // 0x0008
        put!(self.rx_wd_timeout_interval); // 0x000A
        put!(self.rx_secure_channel_index); // 0x000C
        put!(self.flags_byte()); // 0x000D
        put!(self.rx_safestate_sequencer); // 0x000E
        put!(self.rx_safe_sequencer_state); // 0x000F
        put!(self.rx_ack_stream_index); // 0x0010
        put!(self.rx_resp_stream_index); // 0x0011
                                         // 0x0012 Reserved (16 bit) and 0x0014 Reserved (32 bit) stay zeroed.
        debug_assert_eq!(off, Self::FLAGS_OFFSET + 5);
        buf
    }

    /// Decode Table 22's fixed-length, big-endian 24-byte row.
    ///
    /// Reserved bytes `0x0012`-`0x0017` are ignored regardless of content,
    /// so a row round-trips only its eleven specified fields.
    ///
    /// Returns `Err(RcpError::ShortFrame)` if `bytes` is shorter than
    /// [`Self::ENCODED_LEN`]. Trailing bytes beyond `ENCODED_LEN` are
    /// ignored. Never panics for any input.
    //fusa:req REQ-RMAP-017
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
        macro_rules! take_u64 {
            () => {{
                let v = u64::from_be_bytes([
                    bytes[off],
                    bytes[off + 1],
                    bytes[off + 2],
                    bytes[off + 3],
                    bytes[off + 4],
                    bytes[off + 5],
                    bytes[off + 6],
                    bytes[off + 7],
                ]);
                off += 8;
                v
            }};
        }

        let rx_stream_id = take_u64!(); // 0x0000
        let rx_stream_max_request_size = take_u16!(); // 0x0008
        let rx_wd_timeout_interval = take_u16!(); // 0x000A
        let rx_secure_channel_index = take_u8!(); // 0x000C
        debug_assert_eq!(off, Self::FLAGS_OFFSET);
        let flags = take_u8!(); // 0x000D
        let rx_safestate_sequencer = take_u8!(); // 0x000E
        let rx_safe_sequencer_state = take_u8!(); // 0x000F
        let rx_ack_stream_index = take_u8!(); // 0x0010
        let rx_resp_stream_index = take_u8!(); // 0x0011
                                               // 0x0012 / 0x0014 reserved: ignored.
        debug_assert_eq!(off, Self::FLAGS_OFFSET + 5);

        Ok(Self {
            rx_stream_id,
            rx_stream_max_request_size,
            rx_wd_timeout_interval,
            rx_secure_channel_index,
            rx_enforce_e2e: flags & Self::FLAG_ENFORCE_E2E != 0,
            rx_enforce_seq: flags & Self::FLAG_ENFORCE_SEQ != 0,
            rx_seq_safestate_enable: flags & Self::FLAG_SEQ_SAFESTATE_ENABLE != 0,
            rx_wd_enable: flags & Self::FLAG_WD_ENABLE != 0,
            rx_wd_safestate_enable: flags & Self::FLAG_WD_SAFESTATE_ENABLE != 0,
            rx_ovrflw_safestate_enable: flags & Self::FLAG_OVRFLW_SAFESTATE_ENABLE != 0,
            rx_safety_measure: flags & Self::FLAG_SAFETY_MEASURE != 0,
            rx_wd_info_enable: flags & Self::FLAG_WD_INFO_ENABLE != 0,
            rx_safestate_sequencer,
            rx_safe_sequencer_state,
            rx_ack_stream_index,
            rx_resp_stream_index,
        })
    }
}

impl ConfigTableRow for RequestStreamConfigEntry {
    const ROW_LEN: usize = Self::ENCODED_LEN;

    fn encode_row(&self) -> Vec<u8> {
        self.encode().to_vec()
    }

    fn decode_row(bytes: &[u8]) -> Result<Self, RcpError> {
        Self::decode(bytes)
    }
}

// ── EpByteBusIdMapEntry (§3.9) ───────────────────────────────────────────────

/// One row of the EP-ID/`byte_bus_id` mapping table (TC18 v0.5.1_RC
/// §12.7.8 "Endpoint ID and communication configuration", Table 23,
/// "EP_ID_config", p.59): maps a `(request_stream_index, byte_bus_id)` pair
/// to a target endpoint number.
///
/// # Wire layout (Table 23)
///
/// ```text
/// 0x0000  1_Request_Stream_Index   8 bit
/// 0x0001  1_EP_Nr                  8 bit   "EP addressed by 1_BBID"
/// 0x0002  1_BBID                  16 bit   "Byte_bus_id [11bit]"
/// 0x0004  (next row: 2_Request_Stream_Index)
/// ```
///
/// The row is 4 bytes, with `EP_Nr` **before** `BBID`. Releases before
/// v5.0.0 emitted the two transposed.
///
/// See this module's doc comment "Config tables" section for this table's
/// row-count source ([`GeneralRegisters::svr_ep_bytebus_id_map`]'s
/// `capacity`) and, importantly, for why this type and this module add
/// **no** row-ordering validation — that is the writing client's
/// responsibility, per Table 23's own note ("The parameters
/// Request_Stream_Index and BBID shall occur in ascending order. This has
/// to be ensured by the instance that is sending the configuration to this
/// table").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
//fusa:req REQ-RMAP-018
pub struct EpByteBusIdMapEntry {
    /// Table 23 `0x0000`, "Index of request stream ID".
    pub map_stream_index: u8,
    /// The `byte_bus_id` value, scoped to `map_stream_index`, that this row
    /// maps (Table 23 `0x0002`, a 16-bit field carrying an 11-bit
    /// `Byte_bus_id`). The `u16` width is the table's own, and matches
    /// [`crate::acf::ByteMessageInfo::byte_bus_id`]'s existing field width
    /// for the same wire concept.
    pub map_byte_bus_id: u16,
    /// The endpoint slot number this `(stream, byte_bus_id)` pair resolves
    /// to (Table 23 `0x0001`, "EP addressed by 1_BBID").
    pub map_ep_nr: u8,
}

impl EpByteBusIdMapEntry {
    /// Encoded wire length in bytes: Table 23's row stride, fixed by the
    /// next row's `2_Request_Stream_Index` at relative address `0x0004`.
    pub const ENCODED_LEN: usize = 4;

    /// The [`RegisterCategory`] this table's rows belong to.
    pub const CATEGORY: RegisterCategory = RegisterCategory::RcpConfig;

    /// The documented `map_stream_index` sentinel value marking both
    /// end-of-table and the default mapping to EP0.
    pub const END_OF_TABLE_STREAM_INDEX: u8 = 0;

    /// Does this row carry the documented end-of-table sentinel?
    ///
    /// This recognizes a fixed, stated wire convention only — it is not a
    /// row-ordering check. See this type's doc comment. Never panics for
    /// any input.
    //fusa:req REQ-RMAP-018
    pub fn is_end_of_table(&self) -> bool {
        self.map_stream_index == Self::END_OF_TABLE_STREAM_INDEX
    }

    /// Encode as Table 23's 4-byte row: `[Request_Stream_Index (0x0000),
    /// EP_Nr (0x0001), BBID (0x0002, big-endian 16 bit)]`.
    ///
    /// Releases before v5.0.0 emitted `[stream_index, BBID_hi, BBID_lo,
    /// EP_Nr]` — `EP_Nr` and `BBID` transposed against the table's own
    /// relative addresses.
    //fusa:req REQ-RMAP-019
    pub fn encode(&self) -> [u8; Self::ENCODED_LEN] {
        let mut buf = [0u8; Self::ENCODED_LEN];
        buf[0] = self.map_stream_index; // 0x0000
        buf[1] = self.map_ep_nr; // 0x0001
        buf[2..4].copy_from_slice(&self.map_byte_bus_id.to_be_bytes()); // 0x0002
        buf
    }

    /// Decode Table 23's 4-byte row from the front of `bytes`.
    ///
    /// Returns `Err(RcpError::ShortFrame)` if `bytes` is shorter than
    /// [`Self::ENCODED_LEN`]. Trailing bytes beyond `ENCODED_LEN` are
    /// ignored. Never panics for any input.
    //fusa:req REQ-RMAP-020
    pub fn decode(bytes: &[u8]) -> Result<Self, RcpError> {
        if bytes.len() < Self::ENCODED_LEN {
            return Err(RcpError::ShortFrame);
        }
        Ok(Self {
            map_stream_index: bytes[0],
            map_ep_nr: bytes[1],
            map_byte_bus_id: u16::from_be_bytes([bytes[2], bytes[3]]),
        })
    }
}

impl ConfigTableRow for EpByteBusIdMapEntry {
    const ROW_LEN: usize = Self::ENCODED_LEN;

    fn encode_row(&self) -> Vec<u8> {
        self.encode().to_vec()
    }

    fn decode_row(bytes: &[u8]) -> Result<Self, RcpError> {
        Self::decode(bytes)
    }
}

// ── ResponseStreamConfigEntry (§3.10) ────────────────────────────────────────

/// One row of the response/acknowledge queue config table (TC18 v0.5.1_RC
/// §12.7.9 "Response stream configuration", Table 24 "Responder
/// QUEUE_config", p.60).
///
/// # Wire layout (Table 24)
///
/// ```text
/// Responder Queue 1
///   0x0000  STREAM_UID      16 bit  R/W+  "[63:48] Unique stream ID of queue 1"
///   0x0002  Max_AVTPDUsize  16 bit  R/W*  "Maximum length of an AVTPDU
///                                          generated in quadlets"
///   0x0004  queue_size      16 bit  R/W*  "assigned memory in 32bit words"
///   0x0006  flush_on_count  16 bit  R/W+  "1: immediate / 2 to Queue_Size"
///   0x0008  Flush_time      16 bit  R/W+  "0: Flush only by count / nr: µs"
/// Responder Queue 2
///   0x000A  (next row)
/// ```
///
/// The row is 10 bytes, with the stride fixed by Responder Queue 2's own
/// `STREAM_UID` at relative address `0x000A`.
///
/// Table 24 describes `STREAM_UID` as bits `[63:48]` of the queue's
/// destination stream identifier; this type carries the 16-bit register
/// value only and derives nothing from a 64-bit
/// [`crate::avtp::StreamId`] — see this module's "Config tables provenance
/// note" and `REQ-RMAP-037`.
///
/// See this module's doc comment "Config tables" section for this table's
/// row-count source ([`GeneralRegisters::svr_response_stream_cfg`]'s
/// `capacity`, cross-referenced against
/// [`GeneralRegisters::svr_responder_streams_max`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
//fusa:req REQ-RMAP-021
pub struct ResponseStreamConfigEntry {
    /// Table 24 `0x0000`, "`[63:48]` Unique stream ID of queue 1" — the 16
    /// **most**-significant bits of this queue's destination stream
    /// identifier. Carried as the raw register value; this crate derives
    /// no [`crate::avtp::StreamId`] from it and checks no such
    /// relationship (`REQ-RMAP-037`).
    pub resp_stream_uid: u16,
    /// Largest single AVTPDU this queue may generate, respecting network
    /// MTU; larger payloads require fragmentation.
    pub resp_max_avtpdu_size: u16,
    /// Reserved memory for this queue, in 32-bit words.
    pub resp_queue_size: u16,
    /// Quadlet-count threshold that triggers a proactive flush; `1` means
    /// send immediately with no batching.
    pub resp_flush_on_count: u16,
    /// Time-based flush trigger: flush after this much elapsed time even if
    /// the count threshold has not been reached; also drives periodic
    /// empty-payload liveness heartbeats.
    pub resp_flush_time: u16,
}

impl ResponseStreamConfigEntry {
    /// Encoded wire length in bytes: Table 24's row stride, fixed by
    /// Responder Queue 2's `STREAM_UID` at relative address `0x000A`.
    //fusa:req REQ-RMAP-036
    pub const ENCODED_LEN: usize = 10;

    /// The [`RegisterCategory`] this table's rows belong to.
    pub const CATEGORY: RegisterCategory = RegisterCategory::RcpConfig;

    /// Encode as Table 24's fixed-length, big-endian 10-byte row: each
    /// field at its tabulated relative address, with no padding between
    /// fields. Never panics.
    //fusa:req REQ-RMAP-022
    //fusa:req REQ-RMAP-036
    pub fn encode(&self) -> [u8; Self::ENCODED_LEN] {
        let mut buf = [0u8; Self::ENCODED_LEN];
        buf[0..2].copy_from_slice(&self.resp_stream_uid.to_be_bytes());
        buf[2..4].copy_from_slice(&self.resp_max_avtpdu_size.to_be_bytes());
        buf[4..6].copy_from_slice(&self.resp_queue_size.to_be_bytes());
        buf[6..8].copy_from_slice(&self.resp_flush_on_count.to_be_bytes());
        buf[8..10].copy_from_slice(&self.resp_flush_time.to_be_bytes());
        buf
    }

    /// Decode a fixed-length, big-endian byte block produced by
    /// [`Self::encode`].
    ///
    /// Returns `Err(RcpError::ShortFrame)` if `bytes` is shorter than
    /// [`Self::ENCODED_LEN`]. Trailing bytes beyond `ENCODED_LEN` are
    /// ignored. Never panics for any input.
    //fusa:req REQ-RMAP-023
    pub fn decode(bytes: &[u8]) -> Result<Self, RcpError> {
        if bytes.len() < Self::ENCODED_LEN {
            return Err(RcpError::ShortFrame);
        }
        Ok(Self {
            resp_stream_uid: u16::from_be_bytes([bytes[0], bytes[1]]),
            resp_max_avtpdu_size: u16::from_be_bytes([bytes[2], bytes[3]]),
            resp_queue_size: u16::from_be_bytes([bytes[4], bytes[5]]),
            resp_flush_on_count: u16::from_be_bytes([bytes[6], bytes[7]]),
            resp_flush_time: u16::from_be_bytes([bytes[8], bytes[9]]),
        })
    }
}

impl ConfigTableRow for ResponseStreamConfigEntry {
    const ROW_LEN: usize = Self::ENCODED_LEN;

    fn encode_row(&self) -> Vec<u8> {
        self.encode().to_vec()
    }

    fn decode_row(bytes: &[u8]) -> Result<Self, RcpError> {
        Self::decode(bytes)
    }
}

// ── SequencerStateEntry (§3.11) ──────────────────────────────────────────────

/// One row of the `SEQUENCER_config` register block (TC18 v0.5.1_RC
/// §12.7.10 "Sequencer state registers", Table 25, p.61): one sequencer's
/// persistent state, plus the request stream authorized to reach it.
///
/// # Wire layout (Table 25)
///
/// ```text
/// 0x0000  Seq_state              8 bit  R/W   default 1
/// 0x0001  Request_stream_index   8 bit  R/W*
/// 0x0002  (next row: Seq_2's Seq_state)
/// ```
///
/// The row is **2 bytes**, not 1. Releases before v5.0.0 modeled only
/// `Seq_state`, dropping `Request_stream_index` — which §12.7.10 makes the
/// access-control field for the sequencer ("Each sequencer is dedicated to
/// a specific RC Client and its bound endpoints"; the field itself "refers
/// the Client Nr allowed to access this sequencer"). A row read or written
/// through the old 1-byte form both lost that binding and desynchronized
/// every subsequent row in the table.
///
/// See this module's doc comment "Config tables" section for this table's
/// row-count source ([`GeneralRegisters::svr_sequencers_max`] — the one
/// table of the five with no paired `capacity` field on its
/// [`GeneralRegisters`] pointer, since [`GeneralRegisters::svr_sequencer_state_ptr`]
/// is pointer-only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
//fusa:req REQ-RMAP-024
pub struct SequencerStateEntry {
    /// Table 25 `0x0000`, "Recent state if sequencer no N; if manually set
    /// to 0 then disabled". §12.7.10: "Upon power-on reset all sequencer
    /// state values are set to '1'."
    pub seq_state: u8,
    /// Table 25 `0x0001`, "refers the Client Nr allowed to access this
    /// sequencer" — the request-stream index bound to this sequencer.
    /// Table 25's Default column is blank for this field, so
    /// [`Self::power_on_default`] leaves it `0` rather than inventing a
    /// documented reset value.
    pub request_stream_index: u8,
}

impl SequencerStateEntry {
    /// Encoded wire length in bytes: Table 25's row stride, fixed by
    /// `Seq_2`'s `Seq_state` at relative address `0x0002`.
    pub const ENCODED_LEN: usize = 2;

    /// The [`RegisterCategory`] this table's rows belong to.
    pub const CATEGORY: RegisterCategory = RegisterCategory::RcpConfig;

    /// The documented power-on state for a freshly reset sequencer
    /// (§12.7.10: all sequencer state values are set to "1").
    /// `request_stream_index` is left `0` — Table 25 documents no default
    /// for it.
    //fusa:req REQ-RMAP-024
    pub fn power_on_default() -> Self {
        Self {
            seq_state: 1,
            request_stream_index: 0,
        }
    }

    /// Encode as Table 25's 2-byte row, `[Seq_state,
    /// Request_stream_index]`. Never panics.
    //fusa:req REQ-RMAP-025
    pub fn encode(&self) -> [u8; Self::ENCODED_LEN] {
        [self.seq_state, self.request_stream_index]
    }

    /// Decode Table 25's 2-byte row from the front of `bytes`.
    ///
    /// Returns `Err(RcpError::ShortFrame)` if `bytes` is shorter than
    /// [`Self::ENCODED_LEN`]. Trailing bytes beyond `ENCODED_LEN` are
    /// ignored. Never panics for any input.
    //fusa:req REQ-RMAP-026
    pub fn decode(bytes: &[u8]) -> Result<Self, RcpError> {
        if bytes.len() < Self::ENCODED_LEN {
            return Err(RcpError::ShortFrame);
        }
        Ok(Self {
            seq_state: bytes[0],
            request_stream_index: bytes[1],
        })
    }
}

impl Default for SequencerStateEntry {
    /// Defaults to the documented power-on state (`seq_state == 1`), not
    /// all-zero — see [`Self::power_on_default`].
    fn default() -> Self {
        Self::power_on_default()
    }
}

impl ConfigTableRow for SequencerStateEntry {
    const ROW_LEN: usize = Self::ENCODED_LEN;

    fn encode_row(&self) -> Vec<u8> {
        self.encode().to_vec()
    }

    fn decode_row(bytes: &[u8]) -> Result<Self, RcpError> {
        Self::decode(bytes)
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
    //fusa:test REQ-RMAP-001
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
    //fusa:test REQ-RMAP-001
    fn from_u8_round_trips_every_defined_ep_type() {
        for ep_type in ALL_ENDPOINT_TYPES {
            let raw = ep_type.to_u8();
            assert_eq!(EndpointType::from_u8(raw), Ok(ep_type));
        }
    }

    #[test]
    //fusa:test REQ-RMAP-001
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
    //fusa:test REQ-RMAP-001
    fn is_reserved_true_only_for_dac() {
        for ep_type in ALL_ENDPOINT_TYPES {
            assert_eq!(ep_type.is_reserved(), ep_type == EndpointType::Dac);
        }
    }

    #[test]
    //fusa:test REQ-EPGEN-001
    fn endpoint_type_codes_match_tc18_table_29() {
        // Codes transcribed by hand from TC18 v0.5.1_RC §13.2 Table 29
        // "ep_type values" (p.73). Table 29's 0x00 "Server" row has no
        // EndpointType variant by design -- see REQ-EPGEN-002 -- and is
        // asserted rejected below rather than mapped.
        let table_29: [(u8, EndpointType); 13] = [
            (0x01, EndpointType::Wakeup), // WakeUp Ctrl
            (0x02, EndpointType::Gpio),   // GPIO
            (0x03, EndpointType::Spi),    // SPI
            (0x04, EndpointType::I2c),    // I2C
            (0x05, EndpointType::Uart),   // UART
            (0x06, EndpointType::Lin),    // LIN
            (0x07, EndpointType::PwmOut), // PWM_OUT
            (0x08, EndpointType::PwmIn),  // PWM_IN
            (0x09, EndpointType::Adc),    // ADC
            (0x0A, EndpointType::Dac),    // DAC
            (0x0B, EndpointType::Can),    // CAN
            (0x0C, EndpointType::Iseled), // ISELED
            (0x0D, EndpointType::Mdio),   // MDIO
        ];
        for (code, ep_type) in table_29 {
            assert_eq!(
                ep_type.to_u8(),
                code,
                "TC18 Table 29 assigns {ep_type:?} the ep_type code 0x{code:02X}"
            );
            assert_eq!(EndpointType::from_u8(code), Ok(ep_type));
        }

        // Table 29's assigned range ends at 0x0D; 0x00 (Server) is
        // assigned but unmodeled (REQ-EPGEN-002), and 0x0E upward is
        // unassigned.
        assert!(EndpointType::from_u8(0x00).is_err());
        assert!(EndpointType::from_u8(0x0E).is_err());
    }

    // ── check_ep_type_supported: structural DAC rejection ─────────────────

    #[test]
    //fusa:test REQ-RMAP-031
    fn check_ep_type_supported_rejects_only_dac() {
        for ep_type in ALL_ENDPOINT_TYPES {
            let result = check_ep_type_supported(ep_type);
            if ep_type == EndpointType::Dac {
                assert_eq!(result, Err(RcpError::UnsupportedCmd));
            } else {
                assert_eq!(result, Ok(()));
            }
        }
    }

    #[test]
    //fusa:test REQ-RMAP-031
    fn check_ep_type_supported_agrees_with_is_reserved() {
        for ep_type in ALL_ENDPOINT_TYPES {
            assert_eq!(
                check_ep_type_supported(ep_type).is_err(),
                ep_type.is_reserved()
            );
        }
    }

    #[test]
    //fusa:test REQ-RMAP-031
    fn check_ep_type_supported_never_panics_for_any_ep_type() {
        for ep_type in ALL_ENDPOINT_TYPES {
            let _ = check_ep_type_supported(ep_type);
        }
    }

    #[test]
    //fusa:test REQ-RMAP-031
    fn check_ep_type_supported_does_not_change_functional_config_matching() {
        // check_ep_type_supported is additive, composed alongside
        // check_functional_config_matches_ep_type, never folded into it —
        // a matching Dac/Dac pair still matches per REQ-RMAP-004 even
        // though check_ep_type_supported separately rejects Dac.
        let generic = PerEpConfigBlock::new(EndpointType::Dac);
        let per_type = PerEpTypeFunctionalConfig::new(EndpointType::Dac);
        assert_eq!(
            check_functional_config_matches_ep_type(&generic, &per_type),
            Ok(())
        );
        assert_eq!(
            check_ep_type_supported(generic.ep_type),
            Err(RcpError::UnsupportedCmd)
        );
    }

    // ── EndpointType: rejection of unrecognized encodings ────────────────

    #[test]
    //fusa:test REQ-RMAP-002
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
    //fusa:test REQ-RMAP-002
    //fusa:test REQ-RMAP-006
    fn from_u8_never_panics_across_the_full_byte_range() {
        for raw in 0u8..=255 {
            let _ = EndpointType::from_u8(raw);
        }
    }

    // ── The three layers: structural existence and tagging ───────────────

    #[test]
    //fusa:test REQ-RMAP-003
    fn per_ep_config_block_layer_is_generic() {
        assert_eq!(PerEpConfigBlock::LAYER, ConfigLayer::Generic);
    }

    #[test]
    //fusa:test REQ-RMAP-003
    fn common_functional_config_layer_is_common_functional() {
        assert_eq!(CommonFunctionalConfig::LAYER, ConfigLayer::CommonFunctional);
    }

    #[test]
    //fusa:test REQ-RMAP-003
    fn per_ep_type_functional_config_layer_is_tagged_by_its_ep_type() {
        for ep_type in ALL_ENDPOINT_TYPES {
            let cfg = PerEpTypeFunctionalConfig::new(ep_type);
            assert_eq!(cfg.layer(), ConfigLayer::PerTypeFunctional(ep_type));
        }
    }

    #[test]
    //fusa:test REQ-RMAP-003
    fn per_ep_config_block_new_round_trips_ep_type() {
        for ep_type in ALL_ENDPOINT_TYPES {
            assert_eq!(PerEpConfigBlock::new(ep_type).ep_type, ep_type);
        }
    }

    #[test]
    //fusa:test REQ-RMAP-003
    fn common_functional_config_default_is_all_false() {
        let cfg = CommonFunctionalConfig::default();
        assert!(!cfg.ep_enable);
        assert!(!cfg.ep_clear_req_storage);
        assert!(!cfg.ep_req_crc_enable);
    }

    // ── CommonFunctionalConfig: encode/decode round-trip ──────────────────

    fn sample_common_functional_configs() -> [CommonFunctionalConfig; 4] {
        [
            CommonFunctionalConfig {
                ep_enable: false,
                ep_clear_req_storage: false,
                ep_req_crc_enable: false,
            },
            CommonFunctionalConfig {
                ep_enable: true,
                ep_clear_req_storage: false,
                ep_req_crc_enable: false,
            },
            CommonFunctionalConfig {
                ep_enable: false,
                ep_clear_req_storage: true,
                ep_req_crc_enable: true,
            },
            CommonFunctionalConfig {
                ep_enable: true,
                ep_clear_req_storage: true,
                ep_req_crc_enable: true,
            },
        ]
    }

    #[test]
    //fusa:test REQ-RMAP-028
    //fusa:test REQ-RMAP-029
    fn common_functional_config_encode_decode_round_trips() {
        for cfg in sample_common_functional_configs() {
            let encoded = cfg.encode();
            assert_eq!(encoded.len(), CommonFunctionalConfig::ENCODED_LEN);
            assert_eq!(CommonFunctionalConfig::decode(&encoded), Ok(cfg));
        }
    }

    #[test]
    //fusa:test REQ-RMAP-028
    fn common_functional_config_encode_is_one_full_byte_per_field() {
        let cfg = CommonFunctionalConfig {
            ep_enable: true,
            ep_clear_req_storage: false,
            ep_req_crc_enable: true,
        };
        assert_eq!(cfg.encode(), [0x01, 0x00, 0x01]);
    }

    #[test]
    //fusa:test REQ-RMAP-029
    fn common_functional_config_decode_rejects_short_input() {
        for len in 0..CommonFunctionalConfig::ENCODED_LEN {
            let bytes = vec![0u8; len];
            assert_eq!(
                CommonFunctionalConfig::decode(&bytes),
                Err(RcpError::ShortFrame)
            );
        }
    }

    #[test]
    //fusa:test REQ-RMAP-029
    fn common_functional_config_decode_treats_any_nonzero_byte_as_true() {
        let bytes = [0x01, 0xFF, 0x02];
        assert_eq!(
            CommonFunctionalConfig::decode(&bytes),
            Ok(CommonFunctionalConfig {
                ep_enable: true,
                ep_clear_req_storage: true,
                ep_req_crc_enable: true,
            })
        );
    }

    #[test]
    //fusa:test REQ-RMAP-029
    fn common_functional_config_decode_ignores_trailing_bytes() {
        let cfg = CommonFunctionalConfig {
            ep_enable: true,
            ep_clear_req_storage: true,
            ep_req_crc_enable: false,
        };
        let mut bytes = cfg.encode().to_vec();
        bytes.extend_from_slice(&[0xFF, 0xFF]);
        assert_eq!(CommonFunctionalConfig::decode(&bytes), Ok(cfg));
    }

    // ── Cross-layer invariant ─────────────────────────────────────────────

    #[test]
    //fusa:test REQ-RMAP-004
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
    //fusa:test REQ-RMAP-004
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
                    assert_eq!(checked, Err(RcpError::InvalidParameter));
                }
            }
        }
    }

    // ── Relationship to lifecycle::RegisterCategory ──────────────────────

    #[test]
    //fusa:test REQ-RMAP-005
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
    //fusa:test REQ-RMAP-005
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
    //fusa:test REQ-RMAP-006
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
    //fusa:test REQ-RMAP-007
    fn table_descriptor_encode_decode_round_trips() {
        for d in sample_descriptors() {
            let encoded = d.encode();
            assert_eq!(encoded.len(), TableDescriptor::ENCODED_LEN);
            assert_eq!(TableDescriptor::decode(&encoded), Ok(d));
        }
    }

    #[test]
    //fusa:test REQ-RMAP-007
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
    //fusa:test REQ-RMAP-007
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
    //fusa:test REQ-RMAP-008
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
    //fusa:test REQ-RMAP-008
    fn general_registers_default_is_all_zero() {
        let regs = GeneralRegisters::default();
        assert_eq!(regs.svr_oa_tc18_magic_nr, 0);
        assert_eq!(regs.svr_hw_cfg, TableDescriptor::default());
        assert_eq!(regs.svr_security_cfg, TableDescriptor::default());
    }

    #[test]
    //fusa:test REQ-RMAP-008
    fn general_registers_category_is_lifecycle_general() {
        assert_eq!(GeneralRegisters::CATEGORY, RegisterCategory::General);
    }

    // ── GeneralRegisters: claims_compound_wait_bundle ───────────────────────

    #[test]
    //fusa:test REQ-RMAP-030
    fn claims_compound_wait_bundle_reads_bit_zero_only() {
        let mut regs = sample_general_registers();

        regs.svr_implemented_options = 0b0000_0000;
        assert!(!regs.claims_compound_wait_bundle());

        regs.svr_implemented_options = 0b0000_0001;
        assert!(regs.claims_compound_wait_bundle());

        // Every other bit set, bit 0 clear: still false.
        regs.svr_implemented_options = 0b1111_1110;
        assert!(!regs.claims_compound_wait_bundle());

        // Bit 0 set alongside every other bit: still true.
        regs.svr_implemented_options = 0b1111_1111;
        assert!(regs.claims_compound_wait_bundle());
    }

    // ── GeneralRegisters: encode/decode round-trip ─────────────────────────

    #[test]
    //fusa:test REQ-RMAP-009
    fn general_registers_encode_decode_round_trips() {
        let regs = sample_general_registers();
        let encoded = regs.encode();
        assert_eq!(encoded.len(), GeneralRegisters::ENCODED_LEN);
        assert_eq!(GeneralRegisters::decode(&encoded), Ok(regs));
    }

    #[test]
    //fusa:test REQ-RMAP-009
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
    //fusa:test REQ-RMAP-009
    fn general_registers_decode_ignores_trailing_bytes() {
        let regs = sample_general_registers();
        let mut bytes = regs.encode().to_vec();
        bytes.extend_from_slice(&[0xAA, 0xBB, 0xCC]);
        assert_eq!(GeneralRegisters::decode(&bytes), Ok(regs));
    }

    // ── GeneralRegisters: short-input rejection ────────────────────────────

    #[test]
    //fusa:test REQ-RMAP-010
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
    //fusa:test REQ-RMAP-010
    fn general_registers_decode_accepts_exactly_encoded_len() {
        let regs = sample_general_registers();
        let encoded = regs.encode();
        assert!(GeneralRegisters::decode(&encoded[..GeneralRegisters::ENCODED_LEN]).is_ok());
    }

    // ── Fuzz-style: arbitrary byte inputs never panic ──────────────────────

    #[test]
    //fusa:test REQ-RMAP-011
    fn table_descriptor_decode_never_panics_across_arbitrary_lengths() {
        for len in 0..=300usize {
            let bytes: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
            let _ = TableDescriptor::decode(&bytes);
        }
    }

    #[test]
    //fusa:test REQ-RMAP-011
    fn general_registers_decode_never_panics_across_arbitrary_lengths() {
        for len in 0..=300usize {
            let bytes: Vec<u8> = (0..len).map(|i| ((i * 7) % 256) as u8).collect();
            let _ = GeneralRegisters::decode(&bytes);
        }
    }

    #[test]
    //fusa:test REQ-RMAP-011
    fn general_registers_and_table_descriptor_encode_never_panic() {
        let regs = sample_general_registers();
        let _ = regs.encode();
        for d in sample_descriptors() {
            let _ = d.encode();
        }
    }

    // ── HwPinMappingEntry (§3.7) ────────────────────────────────────────────

    fn sample_hw_pin_mapping_entries() -> [HwPinMappingEntry; 3] {
        [
            HwPinMappingEntry::default(),
            HwPinMappingEntry {
                hw_ep_nr: 2,
                hw_ep_pin_nr: 3,
                hw_pin_props: 0b0101_1010,
            },
            HwPinMappingEntry {
                hw_ep_nr: u8::MAX,
                hw_ep_pin_nr: u8::MAX,
                hw_pin_props: u8::MAX,
            },
        ]
    }

    #[test]
    //fusa:test REQ-RMAP-012
    fn hw_pin_mapping_entry_category_is_hw_config() {
        assert_eq!(HwPinMappingEntry::CATEGORY, RegisterCategory::HwConfig);
    }

    #[test]
    //fusa:test REQ-RMAP-012
    fn hw_pin_mapping_entry_fields_are_independently_settable() {
        let e = HwPinMappingEntry {
            hw_ep_nr: 7,
            hw_ep_pin_nr: 9,
            hw_pin_props: 0x3C,
        };
        assert_eq!(e.hw_ep_nr, 7);
        assert_eq!(e.hw_ep_pin_nr, 9);
        assert_eq!(e.hw_pin_props, 0x3C);
    }

    #[test]
    //fusa:test REQ-RMAP-013
    fn hw_pin_mapping_entry_encode_decode_round_trips() {
        for e in sample_hw_pin_mapping_entries() {
            let encoded = e.encode();
            assert_eq!(encoded.len(), HwPinMappingEntry::ENCODED_LEN);
            assert_eq!(HwPinMappingEntry::decode(&encoded), Ok(e));
        }
    }

    #[test]
    //fusa:test REQ-RMAP-013
    fn hw_pin_mapping_entry_decode_ignores_trailing_bytes() {
        let e = HwPinMappingEntry {
            hw_ep_nr: 1,
            hw_ep_pin_nr: 2,
            hw_pin_props: 3,
        };
        let mut bytes = e.encode().to_vec();
        bytes.extend_from_slice(&[0xFF, 0xFF]);
        assert_eq!(HwPinMappingEntry::decode(&bytes), Ok(e));
    }

    #[test]
    //fusa:test REQ-RMAP-032
    fn hw_pin_mapping_entry_matches_table_19_literal_bytes() {
        // Expected bytes derived by hand from TC18 v0.5.1_RC §12.7.6 "HW
        // pin mapping configuration", Table 19 "HW_config" (p.54),
        // IO_Pin 1:
        //   0x0000  hw_ep_nr      8 bit  R/W*
        //   0x0001  hw_ep_pin_nr  8 bit  R/W*
        //   0x0002  hw_pin_type   8 bit  R/W*
        // Distinct values per field so a transposition cannot pass.
        let entry = HwPinMappingEntry {
            hw_ep_nr: 0x03,
            hw_ep_pin_nr: 0x04,
            hw_pin_props: 0xB5,
        };
        assert_eq!(entry.encode(), [0x03, 0x04, 0xB5]);
        assert_eq!(
            entry.encode()[0x00],
            0x03,
            "Table 19 relative address 0x0000 is hw_ep_nr"
        );
        assert_eq!(
            entry.encode()[0x01],
            0x04,
            "Table 19 relative address 0x0001 is hw_ep_pin_nr"
        );
        assert_eq!(
            entry.encode()[0x02],
            0xB5,
            "Table 19 relative address 0x0002 is hw_pin_type"
        );
    }

    #[test]
    //fusa:test REQ-RMAP-032
    fn hw_pin_mapping_rows_land_on_table_19_absolute_addresses() {
        // Table 19 tabulates IO_Pin 2's hw_ep_nr at relative address
        // 0x0003 and IO_Pin 3's at 0x0006, which fixes the row stride at
        // exactly 3 bytes.
        assert_eq!(HwPinMappingEntry::ENCODED_LEN, 3);

        let rows = [
            HwPinMappingEntry {
                hw_ep_nr: 0x11,
                hw_ep_pin_nr: 0x12,
                hw_pin_props: 0x13,
            },
            HwPinMappingEntry {
                hw_ep_nr: 0x21,
                hw_ep_pin_nr: 0x22,
                hw_pin_props: 0x23,
            },
            HwPinMappingEntry {
                hw_ep_nr: 0x31,
                hw_ep_pin_nr: 0x32,
                hw_pin_props: 0x33,
            },
        ];
        let flat = encode_rows(&rows);
        assert_eq!(flat.len(), 9);
        // IO_Pin 2, Table 19 addresses 0x0003 / 0x0004 / 0x0005.
        assert_eq!(flat[0x0003], 0x21);
        assert_eq!(flat[0x0004], 0x22);
        assert_eq!(flat[0x0005], 0x23);
        // IO_Pin 3, Table 19 address 0x0006.
        assert_eq!(flat[0x0006], 0x31);
    }

    #[test]
    //fusa:test REQ-RMAP-014
    fn hw_pin_mapping_entry_decode_rejects_short_input() {
        for len in 0..HwPinMappingEntry::ENCODED_LEN {
            let bytes = vec![0u8; len];
            assert_eq!(HwPinMappingEntry::decode(&bytes), Err(RcpError::ShortFrame));
        }
    }

    #[test]
    //fusa:test REQ-RMAP-014
    fn hw_pin_mapping_entry_never_panics_across_arbitrary_lengths() {
        for len in 0..=50usize {
            let bytes: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
            let _ = HwPinMappingEntry::decode(&bytes);
        }
    }

    // ── RequestStreamConfigEntry (§3.8) ─────────────────────────────────────

    fn sample_request_stream_config_entries() -> [RequestStreamConfigEntry; 3] {
        [
            RequestStreamConfigEntry::default(),
            RequestStreamConfigEntry {
                rx_stream_id: 0x0011_2233_4455_6677,
                rx_stream_max_request_size: 128,
                rx_wd_timeout_interval: 1000,
                rx_secure_channel_index: 1,
                rx_enforce_e2e: true,
                rx_enforce_seq: false,
                rx_seq_safestate_enable: true,
                rx_wd_enable: true,
                rx_wd_safestate_enable: false,
                rx_ovrflw_safestate_enable: true,
                rx_safety_measure: true,
                rx_wd_info_enable: false,
                rx_safestate_sequencer: 2,
                rx_safe_sequencer_state: 5,
                rx_ack_stream_index: 1,
                rx_resp_stream_index: 1,
            },
            RequestStreamConfigEntry {
                rx_stream_id: u64::MAX,
                rx_stream_max_request_size: u16::MAX,
                rx_wd_timeout_interval: u16::MAX,
                rx_secure_channel_index: u8::MAX,
                rx_enforce_e2e: true,
                rx_enforce_seq: true,
                rx_seq_safestate_enable: true,
                rx_wd_enable: true,
                rx_wd_safestate_enable: true,
                rx_ovrflw_safestate_enable: true,
                rx_safety_measure: true,
                rx_wd_info_enable: true,
                rx_safestate_sequencer: u8::MAX,
                rx_safe_sequencer_state: u8::MAX,
                rx_ack_stream_index: u8::MAX,
                rx_resp_stream_index: u8::MAX,
            },
        ]
    }

    #[test]
    //fusa:test REQ-RMAP-015
    fn request_stream_config_entry_category_is_rcp_config() {
        assert_eq!(
            RequestStreamConfigEntry::CATEGORY,
            RegisterCategory::RcpConfig
        );
    }

    #[test]
    //fusa:test REQ-RMAP-015
    fn request_stream_config_entry_fields_are_independently_settable() {
        let e = sample_request_stream_config_entries()[1];
        assert_eq!(e.rx_stream_id, 0x0011_2233_4455_6677);
        assert_eq!(e.rx_stream_max_request_size, 128);
        assert_eq!(e.rx_wd_timeout_interval, 1000);
        assert_eq!(e.rx_secure_channel_index, 1);
        assert_eq!(e.rx_ack_stream_index, 1);
        assert_eq!(e.rx_resp_stream_index, 1);
    }

    #[test]
    //fusa:test REQ-RMAP-016
    fn request_stream_config_entry_encode_decode_round_trips() {
        for e in sample_request_stream_config_entries() {
            let encoded = e.encode();
            assert_eq!(encoded.len(), RequestStreamConfigEntry::ENCODED_LEN);
            assert_eq!(RequestStreamConfigEntry::decode(&encoded), Ok(e));
        }
    }

    #[test]
    //fusa:test REQ-RMAP-016
    fn request_stream_config_entry_decode_ignores_trailing_bytes() {
        let e = sample_request_stream_config_entries()[1];
        let mut bytes = e.encode().to_vec();
        bytes.extend_from_slice(&[0xAA, 0xBB, 0xCC]);
        assert_eq!(RequestStreamConfigEntry::decode(&bytes), Ok(e));
    }

    /// TC18 v0.5.1_RC §12.7.7 Table 22 tabulates `rx_stream_id2` — the
    /// *next* row's first field — at relative address `0x0018`, which fixes
    /// the row stride at 24 bytes. Releases before v5.0.0 used 25.
    #[test]
    //fusa:test REQ-RMAP-016
    fn request_stream_config_entry_row_stride_is_table_22_next_row_address() {
        assert_eq!(RequestStreamConfigEntry::ENCODED_LEN, 0x18);
        assert_eq!(RequestStreamConfigEntry::FLAGS_OFFSET, 0x0D);
    }

    /// Byte-for-byte against Table 22's own "Relative address" and "Type"
    /// columns, laid out by hand from the table rather than from this
    /// crate's encoder:
    ///
    /// ```text
    /// 0x0000 rx_stream_id               64b  0x0011223344556677
    /// 0x0008 rx_stream_max_request_size 16b  128    = 0x0080
    /// 0x000A rx_wd_timeout_intervall    16b  1000   = 0x03E8
    /// 0x000C rx_secure_channel_index     8b  1      = 0x01
    /// 0x000D flags: .0,.2,.3,.5,.6 set; .1,.4,.7 clear
    ///                                        0b0110_1101 = 0x6D
    /// 0x000E rx_safestate_seqencer       8b  2      = 0x02
    /// 0x000F rx_safe_sequencer_state     8b  5      = 0x05
    /// 0x0010 rx_ack_stream_index         8b  1      = 0x01
    /// 0x0011 rx_resp_stream_index        8b  1      = 0x01
    /// 0x0012 Reserved                   16b  zero
    /// 0x0014 Reserved                   32b  zero
    /// ```
    #[test]
    //fusa:test REQ-RMAP-016
    fn request_stream_config_entry_matches_table_22_literal_bytes() {
        let expected: [u8; 24] = [
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, // 0x0000
            0x00, 0x80, // 0x0008
            0x03, 0xE8, // 0x000A
            0x01, // 0x000C
            0x6D, // 0x000D flags
            0x02, // 0x000E
            0x05, // 0x000F
            0x01, // 0x0010
            0x01, // 0x0011
            0x00, 0x00, // 0x0012 Reserved 16 bit
            0x00, 0x00, 0x00, 0x00, // 0x0014 Reserved 32 bit
        ];
        let e = sample_request_stream_config_entries()[1];
        assert_eq!(e.encode(), expected);
        assert_eq!(RequestStreamConfigEntry::decode(&expected), Ok(e));
    }

    /// Table 22 addresses the eight `1 bit` fields `0x000D.0` through
    /// `0x000D.7`, in the listed order, all sharing one byte. Setting any
    /// single flag must therefore light exactly one bit of `0x000D` and
    /// leave all 23 other bytes zero. Releases before v5.0.0 gave each flag
    /// its own byte, which this test would have caught.
    #[test]
    //fusa:test REQ-RMAP-016
    fn request_stream_config_entry_flag_bit_positions_match_table_22() {
        /// Sets one Table 22 flag on an otherwise-default row.
        type SetFlag = fn(&mut RequestStreamConfigEntry);
        // (setter, Table 22 bit number)
        let cases: [(SetFlag, u32); 8] = [
            (|e| e.rx_enforce_e2e = true, 0),
            (|e| e.rx_enforce_seq = true, 1),
            (|e| e.rx_seq_safestate_enable = true, 2),
            (|e| e.rx_wd_enable = true, 3),
            (|e| e.rx_wd_safestate_enable = true, 4),
            (|e| e.rx_ovrflw_safestate_enable = true, 5),
            (|e| e.rx_safety_measure = true, 6),
            (|e| e.rx_wd_info_enable = true, 7),
        ];
        for (set_flag, bit) in cases {
            let mut e = RequestStreamConfigEntry::default();
            set_flag(&mut e);
            let encoded = e.encode();
            let mut expected = [0u8; RequestStreamConfigEntry::ENCODED_LEN];
            expected[RequestStreamConfigEntry::FLAGS_OFFSET] = 1u8 << bit;
            assert_eq!(encoded, expected, "flag at Table 22 bit 0x000D.{bit}");
            assert_eq!(RequestStreamConfigEntry::decode(&encoded), Ok(e));
        }
        // All eight together fill the byte exactly, and nothing else.
        let all = sample_request_stream_config_entries()[2];
        assert_eq!(all.flags_byte(), 0xFF);
    }

    /// Reserved bytes `0x0012`-`0x0017` carry no modeled field: decode must
    /// ignore whatever they hold, and encode must emit zero.
    #[test]
    //fusa:test REQ-RMAP-017
    fn request_stream_config_entry_reserved_bytes_are_ignored_and_zeroed() {
        let e = sample_request_stream_config_entries()[1];
        let mut dirty = e.encode();
        dirty[0x12..0x18].copy_from_slice(&[0xFF; 6]);
        assert_eq!(RequestStreamConfigEntry::decode(&dirty), Ok(e));
        assert_eq!(&e.encode()[0x12..0x18], &[0u8; 6]);
    }

    #[test]
    //fusa:test REQ-RMAP-017
    fn request_stream_config_entry_decode_rejects_short_input() {
        for len in 0..RequestStreamConfigEntry::ENCODED_LEN {
            let bytes = vec![0u8; len];
            assert_eq!(
                RequestStreamConfigEntry::decode(&bytes),
                Err(RcpError::ShortFrame)
            );
        }
    }

    #[test]
    //fusa:test REQ-RMAP-017
    fn request_stream_config_entry_never_panics_across_arbitrary_lengths() {
        for len in 0..=100usize {
            let bytes: Vec<u8> = (0..len).map(|i| ((i * 3) % 256) as u8).collect();
            let _ = RequestStreamConfigEntry::decode(&bytes);
        }
    }

    // ── EpByteBusIdMapEntry (§3.9) ───────────────────────────────────────────

    fn sample_ep_bytebus_id_map_entries() -> [EpByteBusIdMapEntry; 3] {
        [
            EpByteBusIdMapEntry::default(),
            EpByteBusIdMapEntry {
                map_stream_index: 3,
                map_byte_bus_id: 0x0555,
                map_ep_nr: 4,
            },
            EpByteBusIdMapEntry {
                map_stream_index: u8::MAX,
                map_byte_bus_id: u16::MAX,
                map_ep_nr: u8::MAX,
            },
        ]
    }

    #[test]
    //fusa:test REQ-RMAP-018
    fn ep_bytebus_id_map_entry_category_is_rcp_config() {
        assert_eq!(EpByteBusIdMapEntry::CATEGORY, RegisterCategory::RcpConfig);
    }

    #[test]
    //fusa:test REQ-RMAP-018
    fn ep_bytebus_id_map_entry_is_end_of_table_true_only_for_sentinel_stream_index() {
        for stream_index in 0u8..=255 {
            let e = EpByteBusIdMapEntry {
                map_stream_index: stream_index,
                map_byte_bus_id: 42,
                map_ep_nr: 1,
            };
            assert_eq!(
                e.is_end_of_table(),
                stream_index == EpByteBusIdMapEntry::END_OF_TABLE_STREAM_INDEX
            );
        }
    }

    #[test]
    //fusa:test REQ-RMAP-019
    fn ep_bytebus_id_map_entry_encode_decode_round_trips() {
        for e in sample_ep_bytebus_id_map_entries() {
            let encoded = e.encode();
            assert_eq!(encoded.len(), EpByteBusIdMapEntry::ENCODED_LEN);
            assert_eq!(EpByteBusIdMapEntry::decode(&encoded), Ok(e));
        }
    }

    #[test]
    //fusa:test REQ-RMAP-019
    fn ep_bytebus_id_map_entry_decode_ignores_trailing_bytes() {
        let e = EpByteBusIdMapEntry {
            map_stream_index: 1,
            map_byte_bus_id: 0x0102,
            map_ep_nr: 5,
        };
        let mut bytes = e.encode().to_vec();
        bytes.extend_from_slice(&[0xEE]);
        assert_eq!(EpByteBusIdMapEntry::decode(&bytes), Ok(e));
    }

    /// Byte-for-byte against TC18 v0.5.1_RC §12.7.8 Table 23's own
    /// "Relative address" column, laid out by hand from the table:
    ///
    /// ```text
    /// 0x0000 1_Request_Stream_Index  8b  3      = 0x03
    /// 0x0001 1_EP_Nr                 8b  4      = 0x04
    /// 0x0002 1_BBID                 16b  0x0555 = 0x05 0x55
    /// ```
    ///
    /// Note `EP_Nr` precedes `BBID`. Releases before v5.0.0 emitted
    /// `[0x03, 0x05, 0x55, 0x04]` — the two transposed. The literal below
    /// uses distinct values in every byte so a transposition cannot pass.
    #[test]
    //fusa:test REQ-RMAP-019
    fn ep_bytebus_id_map_entry_matches_table_23_literal_bytes() {
        let e = EpByteBusIdMapEntry {
            map_stream_index: 3,
            map_byte_bus_id: 0x0555,
            map_ep_nr: 4,
        };
        let expected: [u8; 4] = [0x03, 0x04, 0x05, 0x55];
        assert_eq!(e.encode(), expected);
        assert_eq!(EpByteBusIdMapEntry::decode(&expected), Ok(e));
    }

    /// Table 23 tabulates `2_Request_Stream_Index` — the next row's first
    /// field — at relative address `0x0004`, fixing the row stride.
    #[test]
    //fusa:test REQ-RMAP-019
    fn ep_bytebus_id_map_entry_row_stride_is_table_23_next_row_address() {
        assert_eq!(EpByteBusIdMapEntry::ENCODED_LEN, 0x04);
    }

    #[test]
    //fusa:test REQ-RMAP-020
    fn ep_bytebus_id_map_entry_decode_rejects_short_input() {
        for len in 0..EpByteBusIdMapEntry::ENCODED_LEN {
            let bytes = vec![0u8; len];
            assert_eq!(
                EpByteBusIdMapEntry::decode(&bytes),
                Err(RcpError::ShortFrame)
            );
        }
    }

    #[test]
    //fusa:test REQ-RMAP-020
    fn ep_bytebus_id_map_entry_never_panics_across_arbitrary_lengths() {
        for len in 0..=50usize {
            let bytes: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
            let _ = EpByteBusIdMapEntry::decode(&bytes);
        }
    }

    // ── ResponseStreamConfigEntry (§3.10) ───────────────────────────────────

    fn sample_response_stream_config_entries() -> [ResponseStreamConfigEntry; 3] {
        [
            ResponseStreamConfigEntry::default(),
            ResponseStreamConfigEntry {
                resp_stream_uid: 0x1234,
                resp_max_avtpdu_size: 1500,
                resp_queue_size: 64,
                resp_flush_on_count: 4,
                resp_flush_time: 200,
            },
            ResponseStreamConfigEntry {
                resp_stream_uid: u16::MAX,
                resp_max_avtpdu_size: u16::MAX,
                resp_queue_size: u16::MAX,
                resp_flush_on_count: u16::MAX,
                resp_flush_time: u16::MAX,
            },
        ]
    }

    #[test]
    //fusa:test REQ-RMAP-021
    fn response_stream_config_entry_category_is_rcp_config() {
        assert_eq!(
            ResponseStreamConfigEntry::CATEGORY,
            RegisterCategory::RcpConfig
        );
    }

    #[test]
    //fusa:test REQ-RMAP-021
    fn response_stream_config_entry_fields_are_independently_settable() {
        let e = sample_response_stream_config_entries()[1];
        assert_eq!(e.resp_stream_uid, 0x1234);
        assert_eq!(e.resp_max_avtpdu_size, 1500);
        assert_eq!(e.resp_queue_size, 64);
        assert_eq!(e.resp_flush_on_count, 4);
        assert_eq!(e.resp_flush_time, 200);
    }

    #[test]
    //fusa:test REQ-RMAP-022
    fn response_stream_config_entry_encode_decode_round_trips() {
        for e in sample_response_stream_config_entries() {
            let encoded = e.encode();
            assert_eq!(encoded.len(), ResponseStreamConfigEntry::ENCODED_LEN);
            assert_eq!(ResponseStreamConfigEntry::decode(&encoded), Ok(e));
        }
    }

    #[test]
    //fusa:test REQ-RMAP-022
    fn response_stream_config_entry_decode_ignores_trailing_bytes() {
        let e = sample_response_stream_config_entries()[1];
        let mut bytes = e.encode().to_vec();
        bytes.extend_from_slice(&[0x11, 0x22]);
        assert_eq!(ResponseStreamConfigEntry::decode(&bytes), Ok(e));
    }

    #[test]
    //fusa:test REQ-RMAP-036
    fn response_stream_config_entry_matches_table_24_literal_bytes() {
        // Expected bytes derived by hand from TC18 v0.5.1_RC §12.7.9
        // "Response stream configuration", Table 24 "Responder
        // QUEUE_config" (p.60), Responder Queue 1:
        //   0x0000  STREAM_UID      16 bit
        //   0x0002  Max_AVTPDUsize  16 bit
        //   0x0004  queue_size      16 bit
        //   0x0006  flush_on_count  16 bit
        //   0x0008  Flush_time      16 bit
        // Distinct byte pairs per field so any reordering fails.
        let entry = ResponseStreamConfigEntry {
            resp_stream_uid: 0xA1A2,
            resp_max_avtpdu_size: 0xB1B2,
            resp_queue_size: 0xC1C2,
            resp_flush_on_count: 0xD1D2,
            resp_flush_time: 0xE1E2,
        };
        assert_eq!(
            entry.encode(),
            [0xA1, 0xA2, 0xB1, 0xB2, 0xC1, 0xC2, 0xD1, 0xD2, 0xE1, 0xE2]
        );

        let b = entry.encode();
        assert_eq!(&b[0x00..0x02], &[0xA1, 0xA2], "Table 24 0x0000 STREAM_UID");
        assert_eq!(
            &b[0x02..0x04],
            &[0xB1, 0xB2],
            "Table 24 0x0002 Max_AVTPDUsize"
        );
        assert_eq!(&b[0x04..0x06], &[0xC1, 0xC2], "Table 24 0x0004 queue_size");
        assert_eq!(
            &b[0x06..0x08],
            &[0xD1, 0xD2],
            "Table 24 0x0006 flush_on_count"
        );
        assert_eq!(&b[0x08..0x0A], &[0xE1, 0xE2], "Table 24 0x0008 Flush_time");
    }

    #[test]
    //fusa:test REQ-RMAP-036
    fn response_stream_config_rows_land_on_table_24_addresses() {
        // Table 24 tabulates Responder Queue 2's STREAM_UID at relative
        // address 0x000A, which fixes the row stride at exactly 10 bytes.
        assert_eq!(ResponseStreamConfigEntry::ENCODED_LEN, 0x000A);

        let rows = [
            ResponseStreamConfigEntry {
                resp_stream_uid: 0x1111,
                resp_max_avtpdu_size: 0x1122,
                resp_queue_size: 0x1133,
                resp_flush_on_count: 0x1144,
                resp_flush_time: 0x1155,
            },
            ResponseStreamConfigEntry {
                resp_stream_uid: 0x2211,
                resp_max_avtpdu_size: 0x2222,
                resp_queue_size: 0x2233,
                resp_flush_on_count: 0x2244,
                resp_flush_time: 0x2255,
            },
        ];
        let flat = encode_rows(&rows);
        assert_eq!(flat.len(), 20);
        // Responder Queue 2's STREAM_UID, Table 24 address 0x000A.
        assert_eq!(&flat[0x000A..0x000C], &[0x22, 0x11]);
        // ... and its Flush_time, 0x000A + 0x0008.
        assert_eq!(&flat[0x0012..0x0014], &[0x22, 0x55]);
    }

    #[test]
    //fusa:test REQ-RMAP-023
    fn response_stream_config_entry_decode_rejects_short_input() {
        for len in 0..ResponseStreamConfigEntry::ENCODED_LEN {
            let bytes = vec![0u8; len];
            assert_eq!(
                ResponseStreamConfigEntry::decode(&bytes),
                Err(RcpError::ShortFrame)
            );
        }
    }

    #[test]
    //fusa:test REQ-RMAP-023
    fn response_stream_config_entry_never_panics_across_arbitrary_lengths() {
        for len in 0..=50usize {
            let bytes: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
            let _ = ResponseStreamConfigEntry::decode(&bytes);
        }
    }

    // ── SequencerStateEntry (§3.11) ─────────────────────────────────────────

    #[test]
    //fusa:test REQ-RMAP-024
    fn sequencer_state_entry_category_is_rcp_config() {
        assert_eq!(SequencerStateEntry::CATEGORY, RegisterCategory::RcpConfig);
    }

    #[test]
    //fusa:test REQ-RMAP-024
    fn sequencer_state_entry_power_on_default_is_state_one() {
        // TC18 §12.7.10: "Upon power-on reset all sequencer state values
        // are set to '1'." Table 25's Default column is blank for
        // Request_stream_index, so it stays 0.
        let d = SequencerStateEntry::power_on_default();
        assert_eq!(d.seq_state, 1);
        assert_eq!(d.request_stream_index, 0);
    }

    #[test]
    //fusa:test REQ-RMAP-024
    fn sequencer_state_entry_default_matches_power_on_default() {
        assert_eq!(
            SequencerStateEntry::default(),
            SequencerStateEntry::power_on_default()
        );
    }

    /// TC18 §12.7.10 Table 25 gives each sequencer **two** 8-bit fields,
    /// and tabulates `Seq_2`'s `Seq_state` at relative address `0x0002`,
    /// fixing the row stride at 2 bytes. Releases before v5.0.0 modeled
    /// only `Seq_state` with a 1-byte stride, which both dropped the
    /// access-control field and misaligned every row after the first.
    #[test]
    //fusa:test REQ-RMAP-024
    fn sequencer_state_entry_row_stride_is_table_25_next_row_address() {
        assert_eq!(SequencerStateEntry::ENCODED_LEN, 0x02);
    }

    #[test]
    //fusa:test REQ-RMAP-025
    fn sequencer_state_entry_encode_decode_round_trips() {
        for seq_state in [0u8, 1, 2, 128, u8::MAX] {
            for request_stream_index in [0u8, 1, 200, u8::MAX] {
                let e = SequencerStateEntry {
                    seq_state,
                    request_stream_index,
                };
                let encoded = e.encode();
                assert_eq!(encoded.len(), SequencerStateEntry::ENCODED_LEN);
                assert_eq!(SequencerStateEntry::decode(&encoded), Ok(e));
            }
        }
    }

    /// Byte-for-byte against Table 25's own "Relative address" column, laid
    /// out by hand from the table:
    ///
    /// ```text
    /// 0x0000 Seq_state             8b  1 = 0x01
    /// 0x0001 Request_stream_index  8b  4 = 0x04
    /// ```
    #[test]
    //fusa:test REQ-RMAP-025
    fn sequencer_state_entry_matches_table_25_literal_bytes() {
        let e = SequencerStateEntry {
            seq_state: 1,
            request_stream_index: 4,
        };
        let expected: [u8; 2] = [0x01, 0x04];
        assert_eq!(e.encode(), expected);
        assert_eq!(SequencerStateEntry::decode(&expected), Ok(e));
    }

    /// A two-sequencer table read back through `decode_rows` must resolve
    /// `Seq_2`'s state from offset `0x0002`, not `0x0001`.
    #[test]
    //fusa:test REQ-RMAP-025
    fn sequencer_state_table_rows_land_on_table_25_addresses() {
        // Seq_1: state 1, client 9.  Seq_2: state 3, client 2.
        let raw: [u8; 4] = [0x01, 0x09, 0x03, 0x02];
        assert_eq!(
            decode_rows::<SequencerStateEntry>(&raw),
            Ok(vec![
                SequencerStateEntry {
                    seq_state: 1,
                    request_stream_index: 9,
                },
                SequencerStateEntry {
                    seq_state: 3,
                    request_stream_index: 2,
                },
            ])
        );
    }

    #[test]
    //fusa:test REQ-RMAP-025
    fn sequencer_state_entry_decode_ignores_trailing_bytes() {
        let e = SequencerStateEntry {
            seq_state: 7,
            request_stream_index: 3,
        };
        let mut bytes = e.encode().to_vec();
        bytes.extend_from_slice(&[0xFF, 0xFF]);
        assert_eq!(SequencerStateEntry::decode(&bytes), Ok(e));
    }

    #[test]
    //fusa:test REQ-RMAP-026
    fn sequencer_state_entry_decode_rejects_short_input() {
        for len in 0..SequencerStateEntry::ENCODED_LEN {
            let bytes = vec![0u8; len];
            assert_eq!(
                SequencerStateEntry::decode(&bytes),
                Err(RcpError::ShortFrame)
            );
        }
    }

    #[test]
    //fusa:test REQ-RMAP-026
    fn sequencer_state_entry_never_panics_across_arbitrary_lengths() {
        for len in 0..=20usize {
            let bytes: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
            let _ = SequencerStateEntry::decode(&bytes);
        }
    }

    // ── ConfigTableRow / encode_rows / decode_rows (all five row types) ────

    #[test]
    //fusa:test REQ-RMAP-027
    fn encode_rows_decode_rows_round_trip_hw_pin_mapping_entries() {
        let rows: Vec<HwPinMappingEntry> = sample_hw_pin_mapping_entries().to_vec();
        let encoded = encode_rows(&rows);
        assert_eq!(encoded.len(), rows.len() * HwPinMappingEntry::ENCODED_LEN);
        assert_eq!(decode_rows::<HwPinMappingEntry>(&encoded), Ok(rows));
    }

    #[test]
    //fusa:test REQ-RMAP-027
    fn encode_rows_decode_rows_round_trip_request_stream_config_entries() {
        let rows: Vec<RequestStreamConfigEntry> = sample_request_stream_config_entries().to_vec();
        let encoded = encode_rows(&rows);
        assert_eq!(
            encoded.len(),
            rows.len() * RequestStreamConfigEntry::ENCODED_LEN
        );
        assert_eq!(decode_rows::<RequestStreamConfigEntry>(&encoded), Ok(rows));
    }

    #[test]
    //fusa:test REQ-RMAP-027
    fn encode_rows_decode_rows_round_trip_ep_bytebus_id_map_entries() {
        let rows: Vec<EpByteBusIdMapEntry> = sample_ep_bytebus_id_map_entries().to_vec();
        let encoded = encode_rows(&rows);
        assert_eq!(encoded.len(), rows.len() * EpByteBusIdMapEntry::ENCODED_LEN);
        assert_eq!(decode_rows::<EpByteBusIdMapEntry>(&encoded), Ok(rows));
    }

    #[test]
    //fusa:test REQ-RMAP-027
    fn encode_rows_decode_rows_round_trip_response_stream_config_entries() {
        let rows: Vec<ResponseStreamConfigEntry> = sample_response_stream_config_entries().to_vec();
        let encoded = encode_rows(&rows);
        assert_eq!(
            encoded.len(),
            rows.len() * ResponseStreamConfigEntry::ENCODED_LEN
        );
        assert_eq!(decode_rows::<ResponseStreamConfigEntry>(&encoded), Ok(rows));
    }

    #[test]
    //fusa:test REQ-RMAP-027
    fn encode_rows_decode_rows_round_trip_sequencer_state_entries() {
        let rows: Vec<SequencerStateEntry> = [(0u8, 0u8), (1, 7), (255, 255)]
            .into_iter()
            .map(|(seq_state, request_stream_index)| SequencerStateEntry {
                seq_state,
                request_stream_index,
            })
            .collect();
        let encoded = encode_rows(&rows);
        assert_eq!(encoded.len(), rows.len() * SequencerStateEntry::ENCODED_LEN);
        assert_eq!(decode_rows::<SequencerStateEntry>(&encoded), Ok(rows));
    }

    #[test]
    //fusa:test REQ-RMAP-027
    fn decode_rows_empty_input_is_empty_table() {
        assert_eq!(
            decode_rows::<HwPinMappingEntry>(&[]),
            Ok(Vec::<HwPinMappingEntry>::new())
        );
        assert_eq!(
            decode_rows::<SequencerStateEntry>(&[]),
            Ok(Vec::<SequencerStateEntry>::new())
        );
    }

    #[test]
    //fusa:test REQ-RMAP-027
    fn decode_rows_rejects_length_not_a_multiple_of_row_len() {
        // One full HwPinMappingEntry row (3 bytes) plus one extra byte is
        // not a whole number of rows.
        let mut bytes = HwPinMappingEntry::default().encode().to_vec();
        bytes.push(0xFF);
        assert_eq!(
            decode_rows::<HwPinMappingEntry>(&bytes),
            Err(RcpError::ShortFrame)
        );

        // A single byte can never be a whole ResponseStreamConfigEntry row
        // (10 bytes).
        assert_eq!(
            decode_rows::<ResponseStreamConfigEntry>(&[0u8]),
            Err(RcpError::ShortFrame)
        );
    }

    #[test]
    //fusa:test REQ-RMAP-027
    fn encode_rows_decode_rows_never_panic_across_arbitrary_lengths() {
        for len in 0..=100usize {
            let bytes: Vec<u8> = (0..len).map(|i| ((i * 5) % 256) as u8).collect();
            let _ = decode_rows::<HwPinMappingEntry>(&bytes);
            let _ = decode_rows::<RequestStreamConfigEntry>(&bytes);
            let _ = decode_rows::<EpByteBusIdMapEntry>(&bytes);
            let _ = decode_rows::<ResponseStreamConfigEntry>(&bytes);
            let _ = decode_rows::<SequencerStateEntry>(&bytes);
        }
        let hw_rows = sample_hw_pin_mapping_entries();
        let _ = encode_rows(&hw_rows);
    }
}
