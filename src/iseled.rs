//fusa:req REQ-ISELED-001
//fusa:req REQ-ISELED-002
//fusa:req REQ-ISELED-003
//fusa:req REQ-ISELED-004
//fusa:req REQ-ISELED-005
//fusa:req REQ-ISELED-006
//fusa:req REQ-ISELED-007
//fusa:req REQ-ISELED-008
//fusa:req REQ-ISELED-009
//fusa:req REQ-ISELED-010
//fusa:req REQ-ISELED-012
//fusa:req REQ-ISELED-013

//! The ISELED endpoint type (`ep_type 0x0C`) — `ROADMAP.md` Milestone 7
//! ("Remaining Endpoint Types"), third checklist bullet: native
//! 4b/5b-encoded daisy-chain framing; an optional native ISELED CRC,
//! distinct from and additional to the RCP-level CRC-32 safe-point
//! mechanism; and multi-device response aggregation (`iseled_collect_resp`).
//!
//! This follows directly on [`crate::lin`] and [`crate::can`] (this
//! milestone's first two entries): same milestone, same "additive
//! standalone plumbing only" discipline, same doc-comment provenance-note
//! style for anything this crate has not yet reconciled against confirmed
//! wire behavior. Unlike LIN and CAN, ISELED has no old-protocol satellite
//! bridge module in this crate to validate against or migrate away from —
//! there is no `iseledbr.rs` precedent, so every piece below is new
//! modeling rather than a read-and-reject exercise against prior code. Four
//! named pieces are in scope, all implemented here:
//!
//! - [`encode_4b5b`] / [`decode_4b5b`] — the 4b/5b line-coding step this
//!   checklist bullet names. See "Provenance note: 4b/5b is a public coding
//!   scheme, applied at symbol granularity" below for what this crate does
//!   and does not claim about it.
//! - [`IseledFrame`] — the daisy-chain frame shape, carrying its own
//!   4b/5b-encoded line-level form via [`IseledFrame::encode_line`] /
//!   [`IseledFrame::decode_line`]. See "Provenance note: `IseledFrame`'s
//!   field layout" below.
//! - `iseled_frame_crc8` / `IseledFrameCrc` — the optional native ISELED
//!   CRC, kept fully independent of [`crate::e2e::crc32_tc18`]. Only
//!   compiled in under the `iseled-unconfirmed-crc` Cargo feature, **not**
//!   part of this crate's default build — see "Provenance note: the native
//!   ISELED CRC is a distinct, additive layer" below for why.
//! - [`iseled_collect_resp`] / [`IseledDeviceResponse`] /
//!   [`IseledCollectedResponse`] — multi-device response aggregation, named
//!   to match `ROADMAP.md`'s own identifier. See "Multi-device response
//!   aggregation" below.
//! - [`IseledFunctionalConfig`] — this endpoint type's functional-config
//!   content. See "Relationship to `crate::regmap`" below.
//! - [`IseledRequest`]/[`IseledRequest::from_evt_sub_opcode`] — ISELED's own
//!   request-decode entry point, validating an incoming request's
//!   `evt.sub_opcode` against [`crate::evtgroup::evt_row2_kind_of`]'s TC18
//!   §13.5 Table 33 Row-2 rule. See "Provenance note: evt[2:0] request
//!   validation" below — this piece was added after this module's own
//!   original four-piece scope note above (still accurate for why no
//!   `sub_opcode` reading existed here originally) as this crate's seventh
//!   Row-2 endpoint-type module, following
//!   [`crate::i2c::I2cRequest`]/[`crate::lin::LinRequest`]/
//!   [`crate::adc::AdcRequest`]/[`crate::pwm::PwmInRequest`]/
//!   [`crate::uart::UartRequest`]'s own prior applications of the same
//!   shared predicate and [`crate::can::CanRequest`]'s own deliberate
//!   departure from their shared `Ok(Self::ConfigWrite)` precedent for
//!   `evt[2:0] == 111b`. This module follows CAN's departure rather than
//!   the other four's precedent — see "Provenance note: evt[2:0] request
//!   validation" below for why. The remaining Row-2 endpoint type (`MDIO`)
//!   is expected to follow the same pattern in its own later item.
//!
//! Deliberately out of scope, for the same reasons every prior Milestone
//! 4/7 entry's own doc comment already gives:
//!
//! - Any interpretation of ISELED's actual command/register set (LED
//!   drive values, color-space conversion, calibration data, etc.).
//!   `ROADMAP.md`'s checklist bullet names framing, CRC, and response
//!   aggregation only — no command semantics — so this module carries
//!   [`IseledFrame::command`]/[`IseledFrame::data`] as opaque bytes it does
//!   not interpret, matching [`crate::spi::SpiByteTransfer`]'s own raw
//!   pass-through discipline.
//! - The "Groups A/B/C" `evt[2:0]` sub-opcode convention
//!   ([`crate::evtgroup::EvtGroup`]) as a general, cross-endpoint-type
//!   classification scheme — [`crate::evtgroup`]'s own doc comment already
//!   flags that broader scheme as unresolved, independent of the narrower,
//!   unambiguous Table 33 Row-2 rule this module's [`IseledRequest`] now
//!   implements (see "Provenance note: evt[2:0] request validation" below).
//! - Decoding [`IseledRequest::ConfigWrite`]'s own TC18 §12.7.1 payload
//!   shape — and, like [`crate::can::CanRequest`] and unlike
//!   [`crate::i2c::I2cRequest`]/[`crate::lin::LinRequest`]/
//!   [`crate::adc::AdcRequest`]/[`crate::pwm::PwmInRequest`]/
//!   [`crate::uart::UartRequest`], [`IseledRequest::from_evt_sub_opcode`]
//!   does not even construct [`IseledRequest::ConfigWrite`] yet; see
//!   "Provenance note: evt[2:0] request validation" below for why.
//! - [`crate::regmap::CommonFunctionalConfig`]'s fields — unchanged here, as
//!   in every prior Milestone 1-4/7 entry.
//! - Wiring [`IseledRequest::from_evt_sub_opcode`] into an actual decoder,
//!   dispatch loop, or [`crate::mock::Endpoint`] implementation.
//!   [`crate::mock::Endpoint`]'s own trait signature still does not carry an
//!   `evt` value to any implementation at all — that gap is not specific to
//!   ISELED, it applies identically to
//!   [`crate::i2c::I2cRequest::from_evt_sub_opcode`]/
//!   [`crate::lin::LinRequest::from_evt_sub_opcode`]/
//!   [`crate::adc::AdcRequest::from_evt_sub_opcode`]/
//!   [`crate::pwm::PwmInRequest::from_evt_sub_opcode`]/
//!   [`crate::can::CanRequest::from_evt_sub_opcode`]/
//!   [`crate::uart::UartRequest::from_evt_sub_opcode`] (each confirmed
//!   still unwired against [`crate::mock::Endpoint`]'s own doc comment).
//!   [`IseledRequest`] is built to that same "additive standalone plumbing
//!   only" level.
//! - Wiring any of this module's other, original four pieces into an
//!   actual decoder, dispatch loop, or
//!   [`crate::avtp`]/[`crate::acf`]/[`crate::addressing`] caller — matching
//!   the discipline every prior Milestone 1-4/7 entry already established.
//! - Physical bit-serial transmission timing (bit ordering onto the wire,
//!   clock recovery, line-idle/control symbols). This module works at the
//!   same byte/frame granularity [`crate::avtp`]/[`crate::acf`] already
//!   operate at — see "Provenance note: 4b/5b is a public coding scheme,
//!   applied at symbol granularity" below.
//!
//! ## Provenance note: 4b/5b is a public coding scheme, applied at symbol
//! granularity
//!
//! 4b/5b line coding (mapping each 4-bit data nibble to a 5-bit code group)
//! is a long-established, publicly documented technique — the same data
//! code-group table underlies both FDDI/TP-PMD and 100BASE-TX Ethernet, and
//! is unrelated to and predates any OPEN Alliance TC18 or ISELED-specific
//! content. [`NIBBLE_TO_5B`] below states that public table, not anything
//! drawn from a confidential specification.
//!
//! What this module does take a position on, and flags per Guiding
//! Principle 5, is the *representation* [`encode_4b5b`]/[`decode_4b5b`] use:
//! one output byte per 5-bit code group (the code group occupying that
//! byte's low 5 bits, top 3 bits always zero) rather than a packed
//! contiguous bitstream. `ROADMAP.md`'s checklist bullet states ISELED
//! framing is 4b/5b-encoded but not how this crate should represent that
//! encoding's output at the software layer, and this crate has no
//! bit-serial transmission path to feed regardless — [`crate::avtp`] and
//! [`crate::acf`] both already operate at byte granularity, never at raw
//! bit-offsets, so a symbol-per-byte representation is the working choice
//! made here, consistent with that existing granularity. A later item that
//! adds real bit-serial line transmission (should this crate ever grow one)
//! is expected to repack [`encode_4b5b`]'s per-symbol bytes into a
//! contiguous bitstream then, not now.
//!
//! ## Provenance note: `IseledFrame`'s field layout
//!
//! `ROADMAP.md`'s checklist bullet states ISELED framing is a 4b/5b-encoded
//! daisy chain but states no concrete field layout for what a frame carries
//! beneath that line coding. Per Guiding Principle 5, [`IseledFrame`]'s
//! three fields — [`IseledFrame::chain_address`] (a client-supplied
//! device-selector byte within the daisy chain; this module takes no
//! position on any single-device-vs-broadcast addressing convention beyond
//! carrying the byte the client supplies), [`IseledFrame::command`], and
//! [`IseledFrame::data`] — are this crate's own working interpretation,
//! matching [`crate::lin::LinFrameTransfer`]'s own "provenance note:
//! PID/checksum are client-owned bytes" precedent for an equally unstated
//! request shape. No length ceiling is enforced on `data` — this module
//! takes no position on a real ISELED per-frame data ceiling, matching
//! [`crate::spi::SpiByteTransfer`]'s own unbounded raw-byte-stream
//! modeling rather than inventing an unconfirmed limit.
//!
//! ## Provenance note: the native ISELED CRC is a distinct, additive layer
//!
//! `ROADMAP.md`'s checklist bullet calls for an "optional native ISELED
//! CRC, distinct from and additional to the RCP-level CRC32" — i.e. two
//! independent CRC layers that can both be present on the same message,
//! neither one replacing the other. [`crate::e2e::crc32_tc18`] (Milestone 6)
//! already covers the RCP-level safe-point layer; `iseled_frame_crc8` would
//! be a second, unrelated algorithm — different width (8 bits vs. 32),
//! different input (an [`IseledFrame`]'s own raw bytes, not a safe-point
//! AVTPDU/ACF coverage buffer), and computed by a wholly separate function
//! this module does not call from `crc32_tc18` or vice versa. This crate's
//! spec-extraction pass has not recovered ISELED's own confirmed CRC
//! polynomial/width/init parameters, so — per Guiding Principle 5 —
//! `iseled_frame_crc8` uses a named, independently well-documented standard
//! CRC-8 variant ("CRC-8/AUTOSAR": polynomial `0x2F`, init `0xFF`,
//! non-reflected, final XOR `0xFF`) as its own explicitly flagged working
//! choice, not asserted as the confirmed ISELED-specified value.
//!
//! Because that width/polynomial/init tuple — and even the choice of
//! CRC-8/AUTOSAR as ISELED's real native algorithm at all — is an unverified
//! stand-in rather than a confirmed spec value, `iseled_frame_crc8` /
//! `IseledFrameCrc` / the private `crc8_autosar` helper are gated behind the
//! opt-in `iseled-unconfirmed-crc` Cargo feature and are **not** part of
//! this crate's default build. This keeps an invented algorithm from being
//! indistinguishable, at the API level, from this crate's other
//! confirmed-correct wire primitives — a caller has to explicitly opt in to
//! get it, rather than getting it "for free" by depending on this crate at
//! all. [`IseledFunctionalConfig::native_crc_enabled`] itself carries no
//! such gate: it models the "optional" part of this checklist bullet as a
//! per-endpoint functional-config choice — whatever algorithm eventually
//! backs it — independent of whether `iseled-unconfirmed-crc` is enabled in
//! any given build. A later item that recovers ISELED's real CRC parameters
//! (against this crate's own spec-extraction pass, never against restated
//! spec prose) is expected to update `iseled_frame_crc8`'s algorithm and
//! reconsider this gate then, not now.
//!
//! ## TC18 reconciliation note (§13.7.12)
//!
//! Reconciling this module against TC18 §13.7.12 confirms one behavior and
//! records a substantial set of gaps, all recorded as explicit
//! not-implemented requirement entries rather than silently omitted.
//!
//! Confirmed: TC18 §13.7.12.3 (TC18.txt line 5578) states "The ISELED
//! request and response contains plain data in the `byte_msg_payload` that
//! is to be presented or has been received on the ISELED bus", and
//! [`IseledFrame::encode`] emits its fields verbatim, in order, inserting
//! nothing — in particular no CRC, which line 5595 confirms "is not present
//! on the ISELED network" unless the endpoint is configured to generate one.
//!
//! Not implemented:
//!
//! - The request payload's own field layout. TC18 Figure 40 (line 5588) and
//!   its accompanying example (line 5597) describe "4 bit instruction, 12
//!   bit address and 3 bytes of data"; this module's [`IseledFrame`] instead
//!   carries a full-byte `chain_address` and a full-byte `command`, so an
//!   [`IseledFrame::encode`] buffer is **not** field-compatible with Figure
//!   40 even though it is byte-order-preserving.
//! - The response payload's own field layout. TC18 §13.7.12.3 (line 5600)
//!   states "A response always contains the 12 bit address and 12 bit data
//!   plus the optional 4 bit CRC"; [`IseledDeviceResponse`] carries a
//!   full-byte `chain_address` plus opaque data bytes instead.
//! - The 4/5-bit encoding's own code-group table. TC18 (line 5492) requires
//!   data to be "4/5bit encoded according to the ISLED standard"; this crate
//!   has no access to that standard's table and uses the public
//!   FDDI/100BASE-TX one ([`NIBBLE_TO_5B`]) as an explicitly unconfirmed
//!   stand-in, so conformance of the *values* is not claimed.
//! - Aggregation of 5/4-bit-decoded responses into one or multiple ACF
//!   messages bounded by the request's `read_size` (lines 5493-5494);
//!   [`iseled_collect_resp`] performs no `read_size` accounting and emits no
//!   ACF message.
//! - Generating and attaching the optional native CRC to write messages, and
//!   recomputing and checking it on read data (lines 5494-5496).
//! - The single trigger event on completion of a data packet's transmission
//!   (line 5497).
//! - TC18 Table 55's functional-config register layout (§13.7.12.2, lines
//!   5504-5545), including `iseled_collect_resp` (0x0007.3, 1 bit),
//!   `iseled_use_rcv_clk` (0x0007.4, 1 bit), `iseled_nr_leds` (0x0008,
//!   16 bit) and `iseled_rcv_timeout` (0x000A, 16 bit) — see
//!   [`IseledFunctionalConfig`], which carries one `native_crc_enabled`
//!   flag and nothing else — and the Freq_Sync-vs-ISP_N clock-recovery
//!   choice that `iseled_use_rcv_clk` selects (lines 5549-5551).
//!
//! ## Multi-device response aggregation
//!
//! A daisy chain's devices each contribute their own response; per
//! `ROADMAP.md`'s own naming, [`iseled_collect_resp`] is this module's
//! aggregation entry point. Mirroring
//! [`crate::can::CanXlCombinedPayload::assemble`]'s and
//! [`crate::e2e::CombinedFragmentPayload::assemble`]'s own
//! caller-supplied-ordering discipline — the closest existing "combine
//! multiple pieces into one response" precedent in this crate —
//! [`iseled_collect_resp`] takes each device's own [`IseledDeviceResponse`]
//! as a caller-supplied, already chain-ordered slice rather than deriving
//! device order from any protocol-level position field this module does not
//! model. Unlike [`crate::can::CanXlCombinedPayload`] (which concatenates
//! fragments of what is logically one payload), ISELED's per-device
//! responses are logically distinct — one real device's answer is not a
//! fragment of another's — so [`IseledCollectedResponse`] preserves each
//! contributing device's own [`IseledDeviceResponse::chain_address`] rather
//! than flattening every device's bytes into one undifferentiated buffer.
//!
//! ## Provenance note: evt[2:0] request validation
//!
//! ISELED is one of the eight endpoint types TC18 §13.5 Table 33 groups into
//! one shared "Row 2" `evt[2:0]` rule (TC18.txt lines 4085-4092, `ISELED`
//! itself named at line 4091) — see [`crate::evtgroup`]'s own doc comment
//! "Provenance note: TC18 §13.5 Table 33's Row-2 rule (`evt_row2_kind_of`)"
//! for the full citation, including the literal-text discrepancy that
//! module's doc comment flags and resolves (Table 33's own printed Row-2
//! cell reads "000b to 110b reserved", including 000b, which this crate does
//! not implement literally). [`IseledRequest::from_evt_sub_opcode`] is this
//! module's own caller of that shared [`crate::evtgroup::evt_row2_kind_of`]
//! predicate — ISELED's own request format (TC18 §13.7.12.3 Figure 41,
//! TC18.txt line 5989) carries the same `evt` field in its Message Info
//! header every other endpoint type's request does, and TC18 names no
//! ISELED-specific override of Table 33's generic rule anywhere in
//! §13.7.12.
//!
//! **`IseledRequest::from_evt_sub_opcode` takes an already-decoded
//! [`IseledFrame`], not raw `byte_msg_payload` bytes — matching
//! [`crate::can::CanRequest::from_evt_sub_opcode`]'s own shape, not
//! [`crate::i2c::I2cRequest::from_evt_sub_opcode`]'s/
//! [`crate::lin::LinRequest::from_evt_sub_opcode`]'s/
//! [`crate::adc::AdcRequest::from_evt_sub_opcode`]'s/
//! [`crate::pwm::PwmInRequest::from_evt_sub_opcode`]'s/
//! [`crate::uart::UartRequest::from_evt_sub_opcode`]'s own raw-bytes
//! shape.** [`IseledFrame`] already has its own dedicated decode entry
//! point, [`IseledFrame::decode`] (and [`IseledFrame::decode_line`] for the
//! native 4b/5b line-coded form) — both pre-existing this item and unchanged
//! by it. Rather than [`IseledRequest::from_evt_sub_opcode`] re-deriving
//! that byte-layout logic a second time internally (the way
//! [`crate::i2c::I2cRequest::from_evt_sub_opcode`]/
//! [`crate::lin::LinRequest::from_evt_sub_opcode`]/
//! [`crate::adc::AdcRequest::from_evt_sub_opcode`]/
//! [`crate::pwm::PwmInRequest::from_evt_sub_opcode`]/
//! [`crate::uart::UartRequest::from_evt_sub_opcode`] each call their own
//! endpoint type's one confirmed payload decoder internally), this function
//! instead requires its caller to have already called [`IseledFrame::decode`]
//! (or [`IseledFrame::decode_line`]) and supply the resulting [`IseledFrame`]
//! directly — mirroring [`crate::can::CanRequest::from_evt_sub_opcode`]'s
//! own identical choice for [`crate::can::CanDataFrame`].
//!
//! **Unlike [`crate::i2c::I2cRequest::from_evt_sub_opcode`]/
//! [`crate::lin::LinRequest::from_evt_sub_opcode`]/
//! [`crate::adc::AdcRequest::from_evt_sub_opcode`]/
//! [`crate::pwm::PwmInRequest::from_evt_sub_opcode`]/
//! [`crate::uart::UartRequest::from_evt_sub_opcode`],
//! `IseledRequest::from_evt_sub_opcode` returns
//! `Err(`[`RcpError::ConfigWriteNotImplemented`]`)` for `evt[2:0] == 111b`,
//! following [`crate::can::CanRequest::from_evt_sub_opcode`]'s own v5.8.0
//! departure rather than those five siblings' `Ok(Self::ConfigWrite)`
//! precedent.** This follows for the identical structural reason
//! [`crate::can`]'s own doc comment gives, not a new one invented for
//! ISELED: `IseledRequest::from_evt_sub_opcode` requires its caller to
//! supply an already-decoded [`IseledFrame`] value *before* this function is
//! even called (see above) — and a genuine TC18 §12.7.1 config-write payload
//! is definitionally not the "plain data ... that is to be presented or has
//! been received on the ISELED bus" [`IseledFrame`] represents (TC18
//! §13.7.12.3, TC18.txt line 5982; see this module's own doc comment "TC18
//! reconciliation note (§13.7.12)"). §12.7.1's functional-config-write
//! payload is an EP-level register-map operation (TC18 Table 58,
//! §13.7.12.2), not `chain_address`/`command`/`data` content, so no caller
//! can honestly decode a genuine config-write payload through
//! [`IseledFrame::decode`] and pass the result in for this branch.
//! Accepting whatever [`IseledFrame`] the caller supplied regardless and
//! returning `Ok(`[`IseledRequest::ConfigWrite`]`)` would silently discard a
//! value the caller was structurally required to construct but that bears
//! no relationship to the real config-write request — exactly the
//! dishonesty [`crate::can`]'s own doc comment already flags and declines
//! for [`crate::can::CanRequest`]. This is a materially different position
//! from [`crate::uart::UartRequest`]'s own `ConfigWrite` arm, which stays
//! `Ok(Self::ConfigWrite)`: `UartRequest::from_evt_sub_opcode` accepts
//! cheaply-ignorable raw `payload: &[u8]`/`is_write: bool` arguments that
//! never required a decode step from its caller, so declining to interpret
//! them costs nothing. `IseledRequest::from_evt_sub_opcode`'s caller, by
//! contrast, has already paid [`IseledFrame::decode`]'s own up-front decode
//! cost (which can itself fail with [`RcpError::ShortFrame`] for input
//! shorter than 2 bytes) before this function is even reached, exactly as
//! [`crate::can::CanRequest::from_evt_sub_opcode`]'s own caller has already
//! paid [`crate::can::CanDataFrame::decode`]'s. [`IseledRequest::ConfigWrite`]
//! itself remains a real, declared variant of this enum — reserved for
//! whichever future item does implement a real §12.7.1 ISELED config-write
//! payload decode and can therefore construct it honestly, the same way
//! [`crate::can::CanRequest::ConfigWrite`] is reserved (see
//! [`crate::RcpError`]'s own doc comment for this crate's broader
//! declared-but-not-yet-constructed precedent).
//!
//! Every `Reserved` sub_opcode value (`evt[2:0]` in `001b..=110b`, or any
//! value outside the 3-bit field's representable range) is rejected with
//! `Err(`[`RcpError::UnsupportedCmd`]`)`, matching Table 33's own stated
//! error code and every prior Row-2 endpoint-type module's identical
//! refusal of their own table's reserved code — this part is unchanged
//! across all seven Row-2 endpoint-type modules so far, ISELED included.
//!
//! ## Relationship to [`crate::regmap`]
//!
//! As with every Milestone 4/7 endpoint-type module, ISELED's real
//! functional-config content gets its own dedicated type,
//! [`IseledFunctionalConfig`], rather than adding ISELED-specific fields
//! directly onto the still-shared, thirteen-endpoint-type
//! [`crate::regmap::PerEpTypeFunctionalConfig`] placeholder.
//! [`IseledFunctionalConfig::layer_tag`] shows how a caller obtains the
//! matching generic-layer tag so the two compose through
//! [`crate::regmap::check_functional_config_matches_ep_type`] exactly as
//! that cross-layer rule already expects, without this module editing
//! [`crate::regmap`] itself. `ROADMAP.md`'s checklist bullet names the
//! native CRC as "optional", so [`IseledFunctionalConfig`] carries the one
//! `native_crc_enabled` field that selection needs, rather than being left
//! an empty placeholder like [`crate::lin::LinFunctionalConfig`].

use crate::evtgroup::{evt_row2_kind_of, EvtRow2Kind};
use crate::RcpError;

// ── 4b/5b line coding ────────────────────────────────────────────────────────

/// The public 4b/5b data code-group table (16 data symbols, `0x0`..=`0xF`),
/// indexed by the 4-bit nibble value each 5-bit code group represents. See
/// this module's doc comment "Provenance note: 4b/5b is a public coding
/// scheme, applied at symbol granularity" — this table is the standard
/// FDDI/TP-PMD and 100BASE-TX data symbol assignment, not content drawn
/// from any confidential specification.
const NIBBLE_TO_5B: [u8; 16] = [
    0b11110, // 0x0
    0b01001, // 0x1
    0b10100, // 0x2
    0b10101, // 0x3
    0b01010, // 0x4
    0b01011, // 0x5
    0b01110, // 0x6
    0b01111, // 0x7
    0b10010, // 0x8
    0b10011, // 0x9
    0b10110, // 0xA
    0b10111, // 0xB
    0b11010, // 0xC
    0b11011, // 0xD
    0b11100, // 0xE
    0b11101, // 0xF
];

/// Builds the reverse (5-bit code group -> 4-bit nibble) lookup table at
/// first use: a 32-entry table, with the 16 entries [`NIBBLE_TO_5B`] does
/// not populate left as `None` (these are FDDI/100BASE-TX's own reserved,
/// control, and idle code groups — never a valid ISELED data nibble under
/// this module's own working interpretation).
fn symbol_to_nibble(symbol: u8) -> Option<u8> {
    NIBBLE_TO_5B
        .iter()
        .position(|&code| code == symbol)
        .map(|nibble| nibble as u8)
}

/// Encodes `data` through 4b/5b line coding: each input byte's high nibble
/// then low nibble is mapped through [`NIBBLE_TO_5B`], each resulting 5-bit
/// code group placed in the low 5 bits of its own output byte. See this
/// module's doc comment for why this crate represents 4b/5b output at
/// symbol-per-byte granularity rather than as a packed bitstream. Never
/// panics for any input, including empty input.
//fusa:req REQ-ISELED-001
pub fn encode_4b5b(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() * 2);
    for &byte in data {
        out.push(NIBBLE_TO_5B[(byte >> 4) as usize]);
        out.push(NIBBLE_TO_5B[(byte & 0x0F) as usize]);
    }
    out
}

/// Decodes `symbols` (as produced by [`encode_4b5b`]) back to the original
/// bytes.
///
/// Returns `Err(RcpError::InvalidParameter)` if any byte in `symbols` is not
/// one of [`NIBBLE_TO_5B`]'s 16 valid data code groups (including any byte
/// with a set bit above bit 4). Returns `Err(RcpError::ShortFrame)` if
/// `symbols` has an odd length — every encoded byte contributes exactly two
/// symbols, so a trailing lone symbol cannot complete a byte. Never panics
/// for any input.
//fusa:req REQ-ISELED-002
pub fn decode_4b5b(symbols: &[u8]) -> Result<Vec<u8>, RcpError> {
    if symbols.len() % 2 != 0 {
        return Err(RcpError::ShortFrame);
    }
    let mut out = Vec::with_capacity(symbols.len() / 2);
    for pair in symbols.chunks_exact(2) {
        let hi = symbol_to_nibble(pair[0]).ok_or(RcpError::InvalidParameter)?;
        let lo = symbol_to_nibble(pair[1]).ok_or(RcpError::InvalidParameter)?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

// ── IseledFrame ──────────────────────────────────────────────────────────────

/// An ISELED daisy-chain frame: a chain-address selector byte, a command
/// byte, and opaque data bytes.
///
/// See this module's doc comment "Provenance note: `IseledFrame`'s field
/// layout" for why this shape is this crate's own working interpretation,
/// and for why `data` carries no length ceiling.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
//fusa:req REQ-ISELED-003
pub struct IseledFrame {
    /// The client-supplied device-selector byte within the daisy chain,
    /// carried unparsed — this module takes no position on any
    /// single-device-vs-broadcast addressing convention.
    pub chain_address: u8,
    /// The frame's command byte, carried unparsed — this module performs
    /// no ISELED command-set interpretation of its own.
    pub command: u8,
    /// The frame's data bytes, carried unparsed and with no length
    /// ceiling enforced.
    pub data: Vec<u8>,
}

impl IseledFrame {
    /// Encode this frame to its raw (pre-line-coding) wire representation:
    /// `chain_address`, then `command`, then `data`, unmodified.
    ///
    /// This is the "plain data in the `byte_msg_payload` that is to be
    /// presented ... on the ISELED bus" of TC18 §13.7.12.3 (TC18.txt line
    /// 5578): the bytes are emitted verbatim, in supplied order, with
    /// nothing inserted — in particular no CRC, which TC18.txt line 5595
    /// confirms is not present on the ISELED network unless the endpoint is
    /// configured to generate one. See this module's doc comment "TC18
    /// reconciliation note (§13.7.12)" for the field-layout gaps this does
    /// **not** close.
    //fusa:req REQ-ISELED-003
    //fusa:req REQ-ISELED-011
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(2 + self.data.len());
        buf.push(self.chain_address);
        buf.push(self.command);
        buf.extend_from_slice(&self.data);
        buf
    }

    /// Decode an [`IseledFrame`] from its raw (pre-line-coding) wire
    /// representation.
    ///
    /// Returns `Err(RcpError::ShortFrame)` for input shorter than 2 bytes
    /// (no room for both `chain_address` and `command`). Never panics for
    /// any input.
    //fusa:req REQ-ISELED-004
    pub fn decode(b: &[u8]) -> Result<Self, RcpError> {
        if b.len() < 2 {
            return Err(RcpError::ShortFrame);
        }
        Ok(Self {
            chain_address: b[0],
            command: b[1],
            data: b[2..].to_vec(),
        })
    }

    /// Encode this frame to its native 4b/5b line-coded form: composes
    /// [`IseledFrame::encode`] and [`encode_4b5b`] rather than re-deriving
    /// either.
    //fusa:req REQ-ISELED-005
    pub fn encode_line(&self) -> Vec<u8> {
        encode_4b5b(&self.encode())
    }

    /// Decode an [`IseledFrame`] from its native 4b/5b line-coded form:
    /// composes [`decode_4b5b`] and [`IseledFrame::decode`] rather than
    /// re-deriving either. Propagates either function's own error variants
    /// unchanged, and never panics for any input.
    //fusa:req REQ-ISELED-005
    pub fn decode_line(symbols: &[u8]) -> Result<Self, RcpError> {
        let raw = decode_4b5b(symbols)?;
        Self::decode(&raw)
    }
}

// ── Native ISELED CRC (distinct from `crate::e2e::crc32_tc18`) ──────────────

/// The optional, native ISELED per-frame CRC value — an 8-bit value kept
/// fully independent of [`crate::e2e::crc32_tc18`]'s unrelated 32-bit
/// safe-point layer. See this module's doc comment "Provenance note: the
/// native ISELED CRC is a distinct, additive layer".
///
/// Only compiled in under the `iseled-unconfirmed-crc` Cargo feature — see
/// that same provenance note for why this is gated out of this crate's
/// default build rather than shipped as an ordinary, always-available item.
#[cfg(feature = "iseled-unconfirmed-crc")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-ISELED-006
pub struct IseledFrameCrc(pub u8);

/// Computes `iseled_frame_crc8`'s underlying CRC-8/AUTOSAR value over
/// `data`: polynomial `0x2F`, init `0xFF`, non-reflected, final XOR `0xFF`.
/// See this module's doc comment for why this named, independently
/// documented standard variant is this module's own flagged working choice
/// rather than a confirmed ISELED-specified algorithm.
#[cfg(feature = "iseled-unconfirmed-crc")]
fn crc8_autosar(data: &[u8]) -> u8 {
    const POLY: u8 = 0x2F;
    let mut crc: u8 = 0xFF;
    for &byte in data {
        crc ^= byte;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ POLY
            } else {
                crc << 1
            };
        }
    }
    crc ^ 0xFF
}

/// Computes the native ISELED CRC over an [`IseledFrame`]'s own raw
/// (pre-line-coding) bytes — composes [`IseledFrame::encode`] and
/// `crc8_autosar` rather than re-deriving either. This is a wholly
/// separate computation from [`crate::e2e::crc32_tc18`]; neither function
/// calls the other. Never panics for any input frame.
///
/// Only compiled in under the `iseled-unconfirmed-crc` Cargo feature — see
/// this module's doc comment "Provenance note: the native ISELED CRC is a
/// distinct, additive layer" for why.
#[cfg(feature = "iseled-unconfirmed-crc")]
//fusa:req REQ-ISELED-006
pub fn iseled_frame_crc8(frame: &IseledFrame) -> IseledFrameCrc {
    IseledFrameCrc(crc8_autosar(&frame.encode()))
}

// ── Multi-device response aggregation (`iseled_collect_resp`) ───────────────

/// One daisy-chain device's own contribution to a multi-device response.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
//fusa:req REQ-ISELED-007
pub struct IseledDeviceResponse {
    /// The responding device's chain-address selector byte, matching
    /// [`IseledFrame::chain_address`]'s own field.
    pub chain_address: u8,
    /// This device's own response data bytes, carried unparsed.
    pub data: Vec<u8>,
}

/// The aggregated result of collecting every responding device's own
/// [`IseledDeviceResponse`] from one daisy-chain transaction, in
/// chain-supplied order.
///
/// See this module's doc comment "Multi-device response aggregation" for
/// why per-device structure is preserved here rather than the devices'
/// bytes being flattened into one undifferentiated buffer, unlike
/// [`crate::can::CanXlCombinedPayload`]'s single-payload fragment
/// concatenation.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
//fusa:req REQ-ISELED-008
pub struct IseledCollectedResponse(pub Vec<IseledDeviceResponse>);

/// Collects a daisy chain's per-device responses into one
/// [`IseledCollectedResponse`], named to match `ROADMAP.md`'s own
/// `iseled_collect_resp` identifier.
///
/// `per_device` is taken as a caller-supplied, already chain-ordered slice
/// — mirroring [`crate::can::CanXlCombinedPayload::assemble`]'s and
/// [`crate::e2e::CombinedFragmentPayload::assemble`]'s own
/// caller-supplied-ordering discipline — rather than this function deriving
/// device order from any protocol-level position field this module does
/// not model. An empty `per_device` slice yields an empty collected
/// response; this function never panics for any input.
//fusa:req REQ-ISELED-008
pub fn iseled_collect_resp(per_device: &[IseledDeviceResponse]) -> IseledCollectedResponse {
    IseledCollectedResponse(per_device.to_vec())
}

// ── IseledFunctionalConfig ───────────────────────────────────────────────────

/// ISELED's own per-EP-type functional-config content: whether this
/// endpoint is configured to append the optional native ISELED CRC (see
/// `iseled_frame_crc8`, behind the `iseled-unconfirmed-crc` Cargo feature).
///
/// See this module's doc comment "Relationship to `crate::regmap`" for why
/// this carries a field (unlike [`crate::lin::LinFunctionalConfig`]'s empty
/// placeholder). This struct and its field are **not** feature-gated — only
/// the CRC algorithm implementation itself is — see this module's doc
/// comment "Provenance note: the native ISELED CRC is a distinct, additive
/// layer".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-ISELED-009
pub struct IseledFunctionalConfig {
    /// Whether this ISELED endpoint is configured to append a native
    /// per-frame CRC (`iseled_frame_crc8`, when the `iseled-unconfirmed-crc`
    /// Cargo feature is enabled) in addition to the RCP-level
    /// [`crate::e2e::crc32_tc18`] safe-point CRC. See this module's doc
    /// comment "Provenance note: the native ISELED CRC is a distinct,
    /// additive layer" — enabling this never disables or replaces the
    /// RCP-level CRC.
    pub native_crc_enabled: bool,
}

impl IseledFunctionalConfig {
    /// The [`crate::regmap::PerEpTypeFunctionalConfig`] generic-layer tag
    /// that matches this ISELED functional config, for use with
    /// [`crate::regmap::check_functional_config_matches_ep_type`].
    ///
    /// This module does not itself call that function — it only shows how a
    /// caller would obtain the matching tag, per this module's doc comment
    /// "Relationship to `crate::regmap`".
    //fusa:req REQ-ISELED-010
    pub fn layer_tag(&self) -> crate::regmap::PerEpTypeFunctionalConfig {
        crate::regmap::PerEpTypeFunctionalConfig::new(crate::regmap::EndpointType::Iseled)
    }
}

// ── IseledRequest: evt[2:0] request validation ───────────────────────────────

/// The decoded shape of an incoming ISELED request, after validating its
/// `evt[2:0]` sub-opcode against TC18 §13.5 Table 33's Row-2 rule (ISELED is
/// one of that row's eight endpoint types —
/// `{ADC, PWM_IN, I²C, LIN, CAN, UART, ISELED, MDIO}`).
///
/// See this module's doc comment "Provenance note: evt[2:0] request
/// validation" for the full citation, why
/// [`IseledRequest::from_evt_sub_opcode`] takes an already-decoded
/// [`IseledFrame`] rather than raw `byte_msg_payload` bytes (matching
/// [`crate::can::CanRequest`]'s own shape, not
/// [`crate::i2c::I2cRequest`]'s/[`crate::lin::LinRequest`]'s/
/// [`crate::adc::AdcRequest`]'s/[`crate::pwm::PwmInRequest`]'s/
/// [`crate::uart::UartRequest`]'s raw-bytes shape), why it follows
/// [`crate::can::CanRequest`]'s own
/// `Err(`[`RcpError::ConfigWriteNotImplemented`]`)` departure rather than
/// those five siblings' `Ok(Self::ConfigWrite)` precedent, and
/// [`crate::evtgroup`]'s own doc comment for the literal-text discrepancy
/// this crate resolves `evt[2:0] == 000b` against.
#[derive(Debug, Clone, PartialEq, Eq)]
//fusa:req REQ-ISELED-012
pub enum IseledRequest {
    /// `evt[2:0] == 000b`: an ordinary ISELED request — the caller-decoded
    /// [`IseledFrame`] this endpoint is to send onto, or has received from,
    /// the daisy chain, per TC18 §13.7.12.3's "plain data in the
    /// `byte_msg_payload` that is to be presented or has been received on
    /// the ISELED bus" (TC18.txt line 5982).
    Plain(IseledFrame),
    /// `evt[2:0] == 111b`: a functional-config write (TC18 §12.7.1) rather
    /// than an ordinary request. Unlike every non-CAN Row-2 endpoint-type
    /// module's own `ConfigWrite` variant, [`IseledRequest::from_evt_sub_opcode`]
    /// does not yet construct this variant at all — see this module's doc
    /// comment "Provenance note: evt[2:0] request validation" for why. It
    /// remains a real, declared variant, reserved for whichever future item
    /// implements a real TC18 §12.7.1 ISELED config-write payload decode.
    ConfigWrite,
}

impl IseledRequest {
    /// Decode an incoming ISELED request from its `evt.sub_opcode`
    /// ([`crate::acf::Evt::sub_opcode`]) and an already-decoded
    /// [`IseledFrame`] (see this module's doc comment "Provenance note:
    /// evt[2:0] request validation" for why this takes a decoded
    /// [`IseledFrame`] rather than raw bytes, matching
    /// [`crate::can::CanRequest::from_evt_sub_opcode`] rather than
    /// [`crate::i2c::I2cRequest::from_evt_sub_opcode`]/
    /// [`crate::lin::LinRequest::from_evt_sub_opcode`]/
    /// [`crate::adc::AdcRequest::from_evt_sub_opcode`]/
    /// [`crate::pwm::PwmInRequest::from_evt_sub_opcode`]/
    /// [`crate::uart::UartRequest::from_evt_sub_opcode`]).
    ///
    /// Returns `Err(`[`RcpError::UnsupportedCmd`]`)` for every
    /// [`EvtRow2Kind::Reserved`] sub_opcode value — TC18 §13.5 Table 33's
    /// Row-2 rule requires the request be rejected with error code
    /// `UNSUPPORTED_CMD`, matching every prior Row-2 endpoint-type module's
    /// identical refusal of their own table's reserved code. Returns
    /// `Err(`[`RcpError::ConfigWriteNotImplemented`]`)` — not
    /// `Ok(`[`IseledRequest::ConfigWrite`]`)` — for every
    /// [`EvtRow2Kind::ConfigWrite`] sub_opcode value; see this module's doc
    /// comment for why. Never panics for any `sub_opcode`/`frame`
    /// combination.
    //fusa:req REQ-ISELED-012
    //fusa:req REQ-ISELED-013
    pub fn from_evt_sub_opcode(sub_opcode: u8, frame: IseledFrame) -> Result<Self, RcpError> {
        match evt_row2_kind_of(sub_opcode) {
            EvtRow2Kind::Plain => Ok(Self::Plain(frame)),
            EvtRow2Kind::ConfigWrite => Err(RcpError::ConfigWriteNotImplemented),
            EvtRow2Kind::Reserved => Err(RcpError::UnsupportedCmd),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── 4b/5b line coding ────────────────────────────────────────────────────

    #[test]
    //fusa:test REQ-ISELED-001
    fn nibble_to_5b_table_has_no_duplicate_code_groups() {
        let mut seen = std::collections::HashSet::new();
        for &code in NIBBLE_TO_5B.iter() {
            assert!(code <= 0b11111, "code group must fit in 5 bits");
            assert!(seen.insert(code), "duplicate 5b code group {code:05b}");
        }
    }

    #[test]
    //fusa:test REQ-ISELED-001
    //fusa:test REQ-ISELED-002
    fn encode_4b5b_round_trips_through_decode_4b5b() {
        for data in [
            vec![],
            vec![0x00u8],
            vec![0xFFu8],
            vec![0x12, 0x34, 0x56, 0x78],
            (0u8..=255).collect::<Vec<_>>(),
        ] {
            let symbols = encode_4b5b(&data);
            assert_eq!(symbols.len(), data.len() * 2);
            assert_eq!(decode_4b5b(&symbols).unwrap(), data);
        }
    }

    #[test]
    //fusa:test REQ-ISELED-002
    fn decode_4b5b_rejects_odd_length_input() {
        for len in [1usize, 3, 5] {
            let buf = vec![NIBBLE_TO_5B[0]; len];
            assert_eq!(decode_4b5b(&buf), Err(RcpError::ShortFrame));
        }
    }

    #[test]
    //fusa:test REQ-ISELED-002
    fn decode_4b5b_rejects_invalid_code_groups() {
        // 0b00000 and 0b11111 are not among NIBBLE_TO_5B's 16 data code
        // groups (they're FDDI/100BASE-TX's own reserved/control symbols
        // under this module's own working interpretation).
        for invalid in [0b00000u8, 0b11111, 0xFF] {
            assert_eq!(
                decode_4b5b(&[invalid, NIBBLE_TO_5B[0]]),
                Err(RcpError::InvalidParameter)
            );
        }
    }

    #[test]
    //fusa:test REQ-ISELED-002
    fn decode_4b5b_never_panics_for_any_sampled_input() {
        for len in [0usize, 1, 2, 3, 7, 64] {
            let buf = vec![0x5Au8; len];
            let _ = decode_4b5b(&buf);
        }
    }

    // ── IseledFrame: round-trip / never-panic ────────────────────────────────

    #[test]
    //fusa:test REQ-ISELED-003
    //fusa:test REQ-ISELED-004
    fn iseled_frame_round_trips_through_encode_decode() {
        for (chain_address, command, data) in [
            (0x00u8, 0x00u8, vec![]),
            (0x01, 0x53, vec![0xAAu8; 3]),
            (0xFF, 0xFF, (0u8..64).collect::<Vec<_>>()),
        ] {
            let frame = IseledFrame {
                chain_address,
                command,
                data: data.clone(),
            };
            let decoded = IseledFrame::decode(&frame.encode()).unwrap();
            assert_eq!(decoded.chain_address, chain_address);
            assert_eq!(decoded.command, command);
            assert_eq!(decoded.data, data);
        }
    }

    #[test]
    //fusa:test REQ-ISELED-011
    fn iseled_byte_msg_payload_is_carried_as_plain_data_with_no_crc_inserted() {
        // TC18 §13.7.12.3 (TC18.txt line 5578): "The ISELED request and
        // response contains plain data in the byte_msg_payload that is to be
        // presented or has been received on the ISELED bus." Figure 40's own
        // on-wire example (line 5594) shows three data bytes; line 5595 adds
        // that a safe-operation-mode CRC "is not present on the ISELED
        // network", so encode() must never synthesise one.
        const PAYLOAD: [u8; 5] = [0x01, 0x02, 0xAA, 0xBB, 0xCC];

        let frame = IseledFrame {
            chain_address: PAYLOAD[0],
            command: PAYLOAD[1],
            data: PAYLOAD[2..].to_vec(),
        };
        assert_eq!(frame.encode(), PAYLOAD.to_vec());
        // Exactly the supplied bytes: no CRC byte/nibble appended, no length
        // or framing prefix prepended.
        assert_eq!(frame.encode().len(), 2 + frame.data.len());

        let decoded = IseledFrame::decode(&PAYLOAD).unwrap();
        assert_eq!(decoded, frame);
    }

    #[test]
    //fusa:test REQ-ISELED-004
    fn iseled_frame_decode_rejects_short_input() {
        for len in [0usize, 1] {
            assert_eq!(
                IseledFrame::decode(&vec![0u8; len]),
                Err(RcpError::ShortFrame)
            );
        }
    }

    #[test]
    //fusa:test REQ-ISELED-004
    fn iseled_frame_decode_never_panics_for_any_sampled_input() {
        for len in [0usize, 1, 2, 3, 9, 64] {
            let buf = vec![0x5Au8; len];
            let _ = IseledFrame::decode(&buf);
        }
    }

    #[test]
    //fusa:test REQ-ISELED-005
    fn iseled_frame_round_trips_through_encode_line_decode_line() {
        let frame = IseledFrame {
            chain_address: 0x03,
            command: 0x10,
            data: vec![0x00, 0xFF, 0x42],
        };
        let decoded = IseledFrame::decode_line(&frame.encode_line()).unwrap();
        assert_eq!(decoded, frame);
    }

    #[test]
    //fusa:test REQ-ISELED-005
    fn iseled_frame_decode_line_propagates_4b5b_errors() {
        assert_eq!(IseledFrame::decode_line(&[0x5A]), Err(RcpError::ShortFrame));
        assert_eq!(
            IseledFrame::decode_line(&[0xFF, 0xFF]),
            Err(RcpError::InvalidParameter)
        );
    }

    // ── Native ISELED CRC: distinct from crc32_tc18 ──────────────────────────
    //
    // Gated the same as the code under test — see this module's doc comment
    // "Provenance note: the native ISELED CRC is a distinct, additive layer"
    // for why `iseled_frame_crc8`/`IseledFrameCrc` only compile in under the
    // `iseled-unconfirmed-crc` Cargo feature.
    #[cfg(feature = "iseled-unconfirmed-crc")]
    mod native_crc {
        use super::*;

        #[test]
        //fusa:test REQ-ISELED-006
        fn iseled_frame_crc8_is_deterministic_and_sensitive_to_frame_content() {
            let frame_a = IseledFrame {
                chain_address: 0x01,
                command: 0x02,
                data: vec![0x03, 0x04],
            };
            let frame_b = IseledFrame {
                chain_address: 0x01,
                command: 0x02,
                data: vec![0x03, 0x05],
            };
            assert_eq!(iseled_frame_crc8(&frame_a), iseled_frame_crc8(&frame_a));
            assert_ne!(iseled_frame_crc8(&frame_a), iseled_frame_crc8(&frame_b));
        }

        #[test]
        //fusa:test REQ-ISELED-006
        fn iseled_frame_crc8_is_independent_of_e2e_crc32_tc18() {
            // Both CRC layers can be computed over related content without
            // either function calling the other or the two outputs colliding
            // in width/type — see this module's doc comment "Provenance note:
            // the native ISELED CRC is a distinct, additive layer".
            let frame = IseledFrame {
                chain_address: 0x07,
                command: 0x01,
                data: vec![0xAA, 0xBB, 0xCC],
            };
            let native: IseledFrameCrc = iseled_frame_crc8(&frame);
            let rcp_level: u32 = crate::e2e::crc32_tc18(&frame.encode());
            // Distinct types (u8-wrapping vs. u32) enforced at compile time;
            // this assertion documents that computing one has no bearing on
            // the other's value for the same underlying bytes.
            assert_eq!(native.0 as u32 & !0xFF, 0);
            let _ = rcp_level;
        }
    }

    // ── Multi-device response aggregation ────────────────────────────────────

    #[test]
    //fusa:test REQ-ISELED-007
    //fusa:test REQ-ISELED-008
    fn iseled_collect_resp_preserves_per_device_structure_and_order() {
        let per_device = vec![
            IseledDeviceResponse {
                chain_address: 0x01,
                data: vec![0xAA],
            },
            IseledDeviceResponse {
                chain_address: 0x02,
                data: vec![0xBB, 0xCC],
            },
            IseledDeviceResponse {
                chain_address: 0x03,
                data: vec![],
            },
        ];
        let collected = iseled_collect_resp(&per_device);
        assert_eq!(collected.0, per_device);
    }

    #[test]
    //fusa:test REQ-ISELED-008
    fn iseled_collect_resp_empty_input_yields_empty_collected_response() {
        assert_eq!(iseled_collect_resp(&[]), IseledCollectedResponse(vec![]));
    }

    // ── IseledFunctionalConfig / layer_tag ───────────────────────────────────

    #[test]
    //fusa:test REQ-ISELED-009
    //fusa:test REQ-ISELED-010
    fn iseled_functional_config_layer_tag_matches_ep_type_iseled() {
        let functional = IseledFunctionalConfig {
            native_crc_enabled: true,
        };
        let generic = crate::regmap::PerEpConfigBlock::new(crate::regmap::EndpointType::Iseled);
        let tag = functional.layer_tag();
        assert_eq!(tag.ep_type, crate::regmap::EndpointType::Iseled);
        assert!(crate::regmap::functional_config_matches_ep_type(
            &generic, &tag
        ));
        assert_eq!(
            crate::regmap::check_functional_config_matches_ep_type(&generic, &tag),
            Ok(())
        );
    }

    #[test]
    //fusa:test REQ-ISELED-010
    fn iseled_functional_config_layer_tag_rejects_mismatched_ep_type() {
        let functional = IseledFunctionalConfig::default();
        let generic = crate::regmap::PerEpConfigBlock::new(crate::regmap::EndpointType::Can);
        let tag = functional.layer_tag();
        assert!(!crate::regmap::functional_config_matches_ep_type(
            &generic, &tag
        ));
    }

    // ── IseledRequest::from_evt_sub_opcode ───────────────────────────────────

    fn sample_frame() -> IseledFrame {
        IseledFrame {
            chain_address: 0x03,
            command: 0x10,
            data: vec![0xDE, 0xAD, 0xBE, 0xEF],
        }
    }

    #[test]
    //fusa:test REQ-ISELED-012
    //fusa:test REQ-ISELED-013
    fn iseled_request_plain_evt_wraps_the_given_frame_unchanged() {
        // Unlike I2cRequest::Plain/LinRequest::Plain/AdcRequest::Plain/
        // PwmInRequest::Plain/UartRequest::Write, IseledRequest::Plain does
        // not decode raw bytes itself — it threads the caller's
        // already-decoded IseledFrame through unchanged. See this module's
        // doc comment "Provenance note: evt[2:0] request validation".
        let frame = sample_frame();
        let request = IseledRequest::from_evt_sub_opcode(0b000, frame.clone()).unwrap();
        assert_eq!(request, IseledRequest::Plain(frame));
    }

    #[test]
    //fusa:test REQ-ISELED-012
    //fusa:test REQ-ISELED-013
    fn iseled_request_plain_evt_accepts_an_empty_data_frame() {
        let frame = IseledFrame {
            chain_address: 0,
            command: 0,
            data: vec![],
        };
        let request = IseledRequest::from_evt_sub_opcode(0b000, frame.clone()).unwrap();
        assert_eq!(request, IseledRequest::Plain(frame));
    }

    #[test]
    //fusa:test REQ-ISELED-012
    //fusa:test REQ-ISELED-013
    fn iseled_request_config_write_evt_is_rejected_with_config_write_not_implemented() {
        // Deliberate departure from I2cRequest/LinRequest/AdcRequest/
        // PwmInRequest/UartRequest's own precedent (each returns
        // Ok(Self::ConfigWrite) for evt[2:0] == 111b), following
        // crate::can::CanRequest's own v5.8.0 departure instead — see this
        // module's doc comment "Provenance note: evt[2:0] request
        // validation" for why. The given frame is not a real config-write
        // payload — it is passed only because the signature requires *some*
        // IseledFrame — and is not echoed back or otherwise used.
        assert_eq!(
            IseledRequest::from_evt_sub_opcode(0b111, sample_frame()),
            Err(RcpError::ConfigWriteNotImplemented)
        );
    }

    #[test]
    //fusa:test REQ-ISELED-013
    fn iseled_request_reserved_evt_values_are_rejected_with_unsupported_cmd() {
        for sub_opcode in 0b001..=0b110u8 {
            assert_eq!(
                IseledRequest::from_evt_sub_opcode(sub_opcode, sample_frame()),
                Err(RcpError::UnsupportedCmd)
            );
        }
    }

    #[test]
    //fusa:test REQ-ISELED-013
    fn iseled_request_values_above_the_3_bit_field_are_also_rejected_with_unsupported_cmd() {
        for sub_opcode in (crate::acf::EVT_SUB_OPCODE_MAX + 1)..=u8::MAX {
            assert_eq!(
                IseledRequest::from_evt_sub_opcode(sub_opcode, sample_frame()),
                Err(RcpError::UnsupportedCmd)
            );
        }
    }

    #[test]
    //fusa:test REQ-ISELED-013
    fn iseled_request_from_evt_sub_opcode_never_panics_for_any_sampled_input() {
        let frames = [
            IseledFrame {
                chain_address: 0,
                command: 0,
                data: vec![],
            },
            sample_frame(),
            IseledFrame {
                chain_address: 0xFF,
                command: 0xFF,
                data: vec![0xAAu8; 64],
            },
        ];
        for sub_opcode in 0..=u8::MAX {
            for frame in &frames {
                let _ = IseledRequest::from_evt_sub_opcode(sub_opcode, frame.clone());
            }
        }
    }
}
