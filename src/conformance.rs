//fusa:req REQ-CONF-001
//fusa:req REQ-CONF-002
//fusa:req REQ-CONF-003
//fusa:req REQ-CONF-004
//fusa:req REQ-CONF-005
//fusa:req REQ-CONF-006

//! Conformance test vectors — `ROADMAP.md` Milestone 10's last checklist
//! item, "Conformance test vectors / interop verification against at least
//! one sibling x-RCP implementation once it has also uplifted, or
//! self-referential wire-format golden vectors if none is ready yet."
//!
//! This module is test-only (`#[cfg(test)]`, see `lib.rs`'s declaration —
//! unlike every other `pub mod` in this crate, it is not part of the public
//! API surface, since it has nothing for a downstream caller to depend on;
//! see "Why `#[cfg(test)]` rather than `pub mod`" below). It has two parts:
//!
//! 1. **Golden vectors** — [`golden`]'s fixed byte-array constants, pinned
//!    as literal values rather than re-derived by calling this crate's own
//!    `avtp`/`acf` encoders inside the test that checks them. An
//!    accidental future change to [`crate::avtp`]'s or [`crate::acf`]'s
//!    encode/decode logic that happened to keep round-tripping internally
//!    consistent (encode/decode still agree with *each other*) but
//!    silently drifted from the specified wire bytes is caught here,
//!    because the expected bytes are literals, not recomputed from the
//!    code under test.
//!
//!    **Every vector's literal bytes are now derived by hand from the OPEN
//!    Alliance TC18 v0.5.1_RC figures, and each constant's doc comment
//!    names the exact figure, page, and field-by-field derivation.** This
//!    is a change of kind, not just of value: these arrays used to be
//!    *captured from this crate's own encoder output*, which made them a
//!    tautology — they could only ever detect drift away from whatever
//!    the encoder happened to do first, never that the encoder was wrong
//!    to begin with. That is exactly how the NTSCF/TSCF header defects
//!    fixed in `v4.0.0` survived: the golden vectors certified them.
//! 2. **Interop cross-check against go-RCP** — this crate's one sibling
//!    x-RCP implementation that has, per its own `ROADMAP.md` item 59 ("TC18
//!    Conformance Cutover & RELAY Re-Certification"), also uplifted to a
//!    real TC18 core and shipped `v1.0.0` (`SoundMatt/go-RCP`, commit
//!    `bdc760fb057f067cfb68199b6c3d0edab9e0c671`). See "Cross-check
//!    methodology and result" below for exactly what was compared and what
//!    it found: **the two implementations' wire bytes are not
//!    byte-identical**, for logically equivalent field values. That
//!    divergence is recorded in [`go_rcp_crosscheck`] and pinned by
//!    [`tests::go_rcp_bytes_diverge_from_this_crates_own_encoding`] — an
//!    assertion that the two byte sequences differ, so a future change that
//!    accidentally made them agree (or diverge differently) would be
//!    noticed rather than silently drift past this record. The
//!    AVTPDU-header portion of that divergence has since been resolved
//!    *against the specification* (not against go-RCP) in `v4.0.0`, and
//!    the divergence that remains is go-RCP's; see "Resolution" below.
//!
//! ## Cross-check methodology and result
//!
//! When this cross-check was first written, [`crate::avtp`] and
//! [`crate::acf`]'s own module doc comments flagged (per Guiding
//! Principle 5) that their byte offsets, field widths, and header lengths
//! were this crate's own working interpretation of IEEE 1722 AVTPDU/ACF
//! framing, pending reconciliation against a real TC18 implementation.
//! Both have since been reconciled directly against the specification's
//! own normative field diagrams — [`crate::acf`] in `v3.0.0`,
//! [`crate::avtp`] in `v4.0.0` — so this section is now a record of a
//! *resolved* investigation rather than an open one. See "Resolution"
//! below.
//!
//! The cross-check was performed by writing a small, standalone Go program
//! (not committed to this repository — go-RCP is a separate Go module and
//! this crate has no Go build dependency) that imports go-RCP's `avtp` and
//! `acf` packages directly and calls `avtp.EncodeHeader`/`acf.EncodeMessage`
//! at commit `bdc760fb057f067cfb68199b6c3d0edab9e0c671`, using field values
//! logically analogous to the ones [`golden`]'s vectors use in this crate
//! (same `sequence_num`, `stream_id` sender-MAC/suffix split, presentation
//! timestamp, and message-body bytes, mapped onto whichever go-RCP fields
//! carry the closest matching meaning). The resulting bytes are recorded
//! verbatim in [`go_rcp_crosscheck`], each with the exact command that
//! produced it, per this crate's Guiding Principle 4/5 discipline of citing
//! rather than re-deriving.
//!
//! Result: **not byte-identical**, at every one of the four vector types.
//! The specific structural differences found, common to every vector pair:
//!
//! - **Header/message length.** go-RCP's untimed (NTSCF-equivalent) header
//!   is 13 bytes; go-RCP's timed (TSCF-equivalent) header is 17 bytes.
//!   This crate's [`crate::avtp::NtscfHeader`] encoded to 16 bytes and
//!   [`crate::avtp::TscfHeader`] to 24 bytes when this cross-check was
//!   written — this crate reserved three zero bytes after the
//!   sequence/length field (and, for TSCF, four more after the timestamp)
//!   that go-RCP's layout does not have at all. **Resolved in `v4.0.0`:
//!   both of this crate's numbers were wrong, and so is go-RCP's 13.** The
//!   specification's own normative diagrams (TC18 v0.5.1_RC §11.1 Figure 6
//!   and Figure 5, page 22) give exactly 12 octets for NTSCF and 24 for
//!   TSCF. [`crate::avtp::NTSCF_HEADER_LEN`] is now 12 — the three
//!   fabricated reserved bytes are gone;
//!   [`crate::avtp::TSCF_HEADER_LEN`]'s 24 was coincidentally the right
//!   total, but its internal field *positions* were wrong and are now
//!   corrected too.
//! - **`data_length`/`ntscf_data_length`/`stream_data_length` field
//!   packing and position.** go-RCP stores its 11-bit-valued length as a
//!   plain 16-bit big-endian field (top 5 bits simply unused), placed
//!   after `sequence_num`. This crate split the same 11 bits across two
//!   bytes — the field's top 8 bits in one byte, its low 3 bits
//!   left-justified into the top 3 bits of the next — also after
//!   `sequence_num`. **Resolved in `v4.0.0`: both were wrong.** Figure 6
//!   puts NTSCF's 11-bit `ntscf_data_length` at bits 13-23, i.e. *before*
//!   `sequence_num` (bits 24-31), packed as the low 3 bits of octet 1
//!   followed by all of octet 2. Figure 5 gives TSCF a genuinely
//!   different field: a full **16-bit** `stream_data_length` in its own
//!   sixth quadlet (octets 20-21), with `sequence_num` at octet 2.
//! - **`subtype` for the timed header.** Not originally listed here as a
//!   divergence because both implementations happened to agree on `0x83`.
//!   **Both were wrong** — Figures 5 and 19 both give TSCF
//!   `subtype(0x05)`, and [`crate::avtp::TSCF_SUBTYPE`] is now `0x05`.
//!   Agreement between two implementations is not evidence of
//!   correctness; this is the clearest illustration in the whole
//!   cross-check of why the comparison had to be made against the
//!   specification and not against a sibling.
//! - **Timestamp-validity marker.** go-RCP's timed header carries an
//!   explicit 2-bit `TimestampStatus` marker (missing/valid/invalid/
//!   uncertain) inside its flags byte. This crate's
//!   [`crate::avtp::TscfHeader`] had no equivalent field at all — timestamp
//!   validity was modeled entirely by
//!   [`crate::timestamp::AvtpTimestamp`]'s all-zero-is-untimed convention,
//!   layered on top of the raw `avtp_timestamp` value rather than carried
//!   as a separate wire bit. **Partially resolved in `v4.0.0`:** Figure 5
//!   defines a single-bit `tv` ("timestamp valid") flag at bit 15, which
//!   [`crate::avtp::encode_tscf_header`] now emits, derived from that same
//!   all-zero-is-untimed convention. Neither implementation's
//!   *modeling* matches the specification's single bit — go-RCP's marker
//!   is two bits wide, this crate's is a derived rather than stored value
//!   — but this crate's *bytes* now do.
//! - **Message-kind discriminant.** go-RCP tags its two ACF-equivalent
//!   message kinds `1` (short/no-timestamp) and `2` (long/timestamped).
//!   This crate uses [`crate::acf::ACF_ABB_MSG_TYPE`] = `0x0E` and
//!   [`crate::acf::ACF_GBB_MSG_TYPE`] = `0x0D`.
//! - **`byte_bus_id`/endpoint-address width and field set.** go-RCP's
//!   `ByteBusID` is a flat, single byte; this crate's
//!   [`crate::acf::ByteMessageInfo::byte_bus_id`] is an 11-bit field packed
//!   across two bytes alongside `acf_msg_length`'s low bits. The two
//!   implementations' control-flag sets are not a clean 1:1 mapping either:
//!   go-RCP's `Control` byte carries Ack/Read/Write/Response/Error/
//!   MoreSegments; this crate's [`crate::acf::ByteMessageInfo`] carries
//!   `hs`/`cs`/`op`/`rsp`/`err`/`ms` plus a separate `evt` (ack + 3-bit
//!   sub-opcode) field go-RCP has no counterpart for at all. The
//!   [`go_rcp_crosscheck`] vectors below map only the flags with a
//!   reasonably direct correspondence (ack, response, an operation/write
//!   flag) and leave the rest at their default/zero value on both sides;
//!   this mismatch in the field sets themselves — not just their bit
//!   positions — is exactly the kind of divergence this cross-check exists
//!   to surface, not paper over.
//!
//! One point of **agreement**, worth recording precisely because it is not
//! a divergence: both implementations split the 64-bit `stream_id` the same
//! way — a 6-byte sender MAC in the high-order bytes, a 2-byte
//! locally-assigned suffix in the low-order bytes (this crate's
//! [`crate::avtp::StreamId`], go-RCP's `avtp.NewStreamID`/`StreamID.MAC`/
//! `StreamID.Suffix`). Every [`golden`] vector's `stream_id` bytes are
//! byte-identical to their [`go_rcp_crosscheck`] counterpart's `stream_id`
//! bytes, confirmed by
//! [`tests::stream_id_bytes_agree_with_go_rcp_crosscheck`] — the one part
//! of this cross-check where the two independently-arrived-at
//! interpretations do coincide.
//!
//! ## Resolution
//!
//! This section originally closed by declaring reconciliation "out of
//! scope for this item", on the reasoning that a divergence between two
//! independent readings of the same confidential specification was worth
//! *recording* but not worth *resolving* without the primary source in
//! hand. That was the wrong call, and the AVTPDU-header half of the
//! divergence turned out to be a genuine, mandatory-path defect in this
//! crate rather than a benign implementation difference: TC18 §12.2 lists
//! "NTSCF header processing" as the first of exactly four mandatory
//! features, every transport in this crate (`udp`, `l2`, `shmem`,
//! `tlstransport`, `mock`) frames unconditionally through it, and until
//! `v4.0.0` every frame this crate produced carried three fabricated
//! reserved octets, `sequence_num` and `ntscf_data_length` transposed,
//! and — for TSCF — an invented `subtype`. Nothing this crate emitted
//! could ever have interoperated with a conformant RC Server.
//!
//! What resolved it was reading the specification's own normative field
//! diagrams (§11.1 Figures 5 and 6, page 22) instead of comparing two
//! implementations against each other. The lesson is recorded here rather
//! than in a commit message because it generalises: a cross-implementation
//! byte comparison can tell you that at least one side is wrong, but never
//! which — and when both sides agree (as they did on `subtype 0x83`) it
//! cannot even tell you that much.
//!
//! The remaining entries above — the ACF-level field-set and
//! discriminant differences, and go-RCP's own 13-byte untimed header —
//! are go-RCP's to reconcile; they are recorded here as observations
//! about a sibling implementation, not as open work items for this crate.
//!
//! ## Why `#[cfg(test)]` rather than `pub mod`
//!
//! Every other module in this crate is declared `pub mod` in `lib.rs` and
//! carries a `docs/SEMVER.md` stability tier, because each is either part
//! of the wire protocol/endpoint surface itself or a general-purpose
//! decorator/tool a downstream caller might reasonably depend on. This
//! module is neither — it exists purely to pin and document today's wire
//! bytes and today's cross-check result for this crate's own CI, with
//! nothing here a downstream caller has a reason to import. Declaring it
//! `#[cfg(test)] mod conformance;` (non-`pub`, test-only) keeps it out of
//! the public API surface entirely, so it needs no `docs/SEMVER.md` tier
//! entry and no `docs/PUBLIC_API.txt` regeneration.

/// Self-referential wire-format golden vectors: this crate's own current
/// `avtp`/`acf` encoders' output for a fixed, documented set of logical
/// field values, frozen as literal byte arrays. See this module's doc
/// comment for why literal freezing (rather than recomputing the expected
/// bytes from the encoder under test) is what makes these a genuine
/// regression check rather than a tautology.
pub mod golden {
    /// Sender MAC / unique-id suffix pair used to build the `stream_id` for
    /// the NTSCF-headed vectors below ([`ntscf_header_fields`]).
    pub const SENDER_MAC_1: [u8; 6] = [0x02, 0x42, 0xAC, 0x11, 0x00, 0x02];
    pub const UNIQUE_ID_1: u16 = 0x0007;

    /// Sender MAC / unique-id suffix pair used to build the `stream_id` for
    /// the TSCF-headed vector below ([`tscf_header_fields`]).
    pub const SENDER_MAC_2: [u8; 6] = [0x02, 0x42, 0xAC, 0x11, 0x00, 0x03];
    pub const UNIQUE_ID_2: u16 = 0x0008;

    /// An [`crate::avtp::NtscfHeader`] whose encoding is pinned by
    /// [`NTSCF_GOLDEN_BYTES`]. `ntscf_data_length` (12) is the exact length
    /// of [`ACF_ABB_GOLDEN_BYTES`] below, matching how a real NTSCF header
    /// would describe the ACF_ABB message it carries.
    pub fn ntscf_header_fields() -> crate::avtp::NtscfHeader {
        crate::avtp::NtscfHeader {
            sequence_num: 0x07,
            ntscf_data_length: 12,
            stream_id: crate::avtp::StreamId::new(SENDER_MAC_1, UNIQUE_ID_1).to_u64(),
        }
    }

    /// Golden bytes for [`ntscf_header_fields`], derived by hand from
    /// **OPEN Alliance TC18 v0.5.1_RC §11.1, Figure 6 "NTSCF-Header
    /// Version 0", page 22** (the normative field diagram), cross-checked
    /// against the worked example in **Figure 20, page 79**
    /// (`subtype(0x82)`, `sv`, `version(0x0)`, `r`,
    /// `ntscf_data_length=0x038`, `sequence_num_lsb`, then `stream_id`).
    /// Never recompute this array by calling the encoder — see this
    /// module's doc comment.
    ///
    /// Octet-by-octet derivation from Figure 6's three quadlets:
    ///
    /// | octet(s) | value  | Figure 6 field(s) |
    /// |----------|--------|-------------------|
    /// | 0        | `0x82` | `subtype` (bits 0-7) |
    /// | 1        | `0x80` | `sv`=1 (bit 8), `version`=0 (9-11), `r`=0 (12), `ntscf_data_length[10:8]`=0 (13-15) |
    /// | 2        | `0x0C` | `ntscf_data_length[7:0]` = 12 (bits 16-23) |
    /// | 3        | `0x07` | `sequence_num` = 0x07 (bits 24-31) |
    /// | 4..12    | MAC + suffix | `stream_id` (quadlets 1-2) |
    ///
    /// `acf_payload_data` begins at octet 12 — Figure 6 shows no reserved
    /// gap anywhere in this header.
    pub const NTSCF_GOLDEN_BYTES: [u8; 12] = [
        0x82, 0x80, 0x0C, 0x07, 0x02, 0x42, 0xAC, 0x11, 0x00, 0x02, 0x00, 0x07,
    ];

    /// A [`crate::avtp::TscfHeader`] whose encoding is pinned by
    /// [`TSCF_GOLDEN_BYTES`]. `avtp_timestamp` (`0x1A2B3C4D`) is
    /// deliberately non-degenerate — not all-zero, which
    /// [`crate::timestamp::AvtpTimestamp`] treats as the untimed sentinel —
    /// so this vector exercises a genuine timed value.
    /// `stream_data_length` (19) is the exact length of
    /// [`ACF_GBB_GOLDEN_BYTES`] below.
    pub fn tscf_header_fields() -> crate::avtp::TscfHeader {
        crate::avtp::TscfHeader {
            sequence_num: 0x2A,
            avtp_timestamp: 0x1A2B_3C4D,
            stream_data_length: 19,
            stream_id: crate::avtp::StreamId::new(SENDER_MAC_2, UNIQUE_ID_2).to_u64(),
        }
    }

    /// Golden bytes for [`tscf_header_fields`], derived by hand from
    /// **OPEN Alliance TC18 v0.5.1_RC §11.1, Figure 5 "TSCF-Header
    /// Version 0", page 22** (the normative field diagram), cross-checked
    /// against the worked example in **Figure 19, page 79**
    /// (`subtype(0x05)`, `sv`, `version(0x0)`, `mr`, `rsv`, `tv`,
    /// `sequence_num_lsb`, `reserved`, `tu`; then `stream_id`,
    /// `avtp_timestamp`, a `reserved` quadlet, and
    /// `stream_data_length(octets) = 0x003C` + `reserved`).
    /// Never recompute — see this module's doc comment.
    ///
    /// Octet-by-octet derivation from Figure 5's six quadlets:
    ///
    /// | octet(s) | value  | Figure 5 field(s) |
    /// |----------|--------|-------------------|
    /// | 0        | `0x05` | `subtype` (bits 0-7) — *not* `0x82`-adjacent |
    /// | 1        | `0x81` | `sv`=1 (bit 8), `version`=0 (9-11), `mr`=0 (12), `rsv`=00 (13-14), `tv`=1 (15) |
    /// | 2        | `0x2A` | `sequence_num` (bits 16-23) |
    /// | 3        | `0x00` | `reserved` (24-30), `tu`=0 (31) |
    /// | 4..12    | MAC + suffix | `stream_id` (quadlets 1-2) |
    /// | 12..16   | `1A 2B 3C 4D` | `avtp_timestamp` (quadlet 3) |
    /// | 16..20   | zeros  | `reserved` (quadlet 4) |
    /// | 20..22   | `00 13` | `stream_data_length(octets)` = 19 (16 bits) |
    /// | 22..24   | zeros  | `reserved` (16 bits) |
    ///
    /// `tv` is 1 here because this vector's `avtp_timestamp` is non-zero;
    /// see [`crate::avtp::encode_tscf_header`] for why that bit is derived
    /// rather than modeled as a struct field.
    pub const TSCF_GOLDEN_BYTES: [u8; 24] = [
        0x05, 0x81, 0x2A, 0x00, 0x02, 0x42, 0xAC, 0x11, 0x00, 0x03, 0x00, 0x08, 0x1A, 0x2B, 0x3C,
        0x4D, 0x00, 0x00, 0x00, 0x00, 0x00, 0x13, 0x00, 0x00,
    ];

    /// An [`crate::acf::AcfAbbMessage`] whose encoding is pinned by
    /// [`ACF_ABB_GOLDEN_BYTES`]. `info.evt.ack` and `info.rsp` are set,
    /// giving the vector a non-default flag pattern rather than an
    /// all-clear header.
    pub fn acf_abb_fields() -> crate::acf::AcfAbbMessage {
        crate::acf::AcfAbbMessage {
            info: crate::acf::ByteMessageInfo {
                // `encode_acf_abb` always derives/overwrites
                // `acf_msg_type`/`acf_msg_length`/`pad` from the message
                // itself rather than trusting these fields verbatim — see
                // `acf.rs`'s `quadlets_and_pad_for_message`. Header (8) +
                // 4-byte payload = 12 bytes, already quadlet-aligned: 3
                // quadlets, 0 pad. These values must already agree with
                // that derivation for this vector's round-trip equality
                // check to hold.
                acf_msg_type: crate::acf::ACF_ABB_MSG_TYPE,
                acf_msg_length: 3,
                pad: 0,
                mtv: false,
                byte_bus_id: 0x005,
                evt: crate::acf::Evt {
                    ack: true,
                    sub_opcode: 0,
                },
                hs: false,
                cs: false,
                transaction_num: 0x11,
                op: false,
                rsp: true,
                err: false,
                ms: false,
                read_size_segment: crate::acf::ReadSizeOrSegment(0x04),
            },
            payload: vec![0xDE, 0xAD, 0xBE, 0xEF],
        }
    }

    /// Golden bytes for [`acf_abb_fields`], captured from
    /// `acf::encode_acf_abb`. Never recompute — see this module's doc
    /// comment.
    pub const ACF_ABB_GOLDEN_BYTES: [u8; 12] = [
        0x1C, 0x03, 0x00, 0x05, 0x80, 0x11, 0x40, 0x04, 0xDE, 0xAD, 0xBE, 0xEF,
    ];

    /// An [`crate::acf::AcfGbbMessage`] whose encoding is pinned by
    /// [`ACF_GBB_GOLDEN_BYTES`]. `message_timestamp`
    /// (`0x0102030405060708`) is deliberately non-zero, exercising a
    /// genuine timed value rather than [`crate::timestamp::MessageTimestamp`]'s
    /// untimed all-zero sentinel.
    pub fn acf_gbb_fields() -> crate::acf::AcfGbbMessage {
        crate::acf::AcfGbbMessage {
            info: crate::acf::ByteMessageInfo {
                // `encode_acf_gbb` always derives/overwrites
                // `acf_msg_type`/`acf_msg_length`/`pad` from the message
                // itself — see `acf_abb_fields`'s comment above. Header
                // (16) + 2-byte payload = 18 bytes -> pad 2 -> 20 bytes
                // total -> 5 quadlets. These values must already agree
                // with that derivation for this vector's round-trip
                // equality check to hold.
                acf_msg_type: crate::acf::ACF_GBB_MSG_TYPE,
                acf_msg_length: 5,
                pad: 2,
                mtv: true,
                byte_bus_id: 0x005,
                evt: crate::acf::Evt {
                    ack: false,
                    sub_opcode: 0,
                },
                hs: false,
                cs: false,
                transaction_num: 0x12,
                op: true,
                rsp: true,
                err: false,
                ms: false,
                read_size_segment: crate::acf::ReadSizeOrSegment(0x00),
            },
            message_timestamp: 0x0102_0304_0506_0708,
            payload: vec![0xCA, 0xFE],
        }
    }

    /// Golden bytes for [`acf_gbb_fields`], captured from
    /// `acf::encode_acf_gbb`. Never recompute — see this module's doc
    /// comment. Note the trailing two `0x00` octets: this vector's `pad`
    /// count (2) is real, encoded padding — not present in the pre-TC18
    /// -reconciliation layout this array used to pin.
    pub const ACF_GBB_GOLDEN_BYTES: [u8; 20] = [
        0x1A, 0x05, 0xA0, 0x05, 0x00, 0x12, 0xC0, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
        0x08, 0xCA, 0xFE, 0x00, 0x00,
    ];

    /// Golden bytes for a composed AVTPDU frame — [`ntscf_header_fields`]'s
    /// header (with its `stream_id`/`sequence_num`) immediately followed by
    /// [`acf_abb_fields`]'s encoded ACF_ABB message. This is the one vector
    /// exercising Milestone 9's frame-composition step
    /// ([`crate::avtp::encode_ntscf_frame`]/
    /// [`crate::avtp::decode_ntscf_frame`]) rather than a bare header or
    /// message alone. Never recompute — see this module's doc comment.
    ///
    /// It is the concatenation, by construction, of
    /// [`NTSCF_GOLDEN_BYTES`] (12 octets, derived from TC18 v0.5.1_RC
    /// §11.1 Figure 6, p.22) and [`ACF_ABB_GOLDEN_BYTES`] (12 octets,
    /// derived from TC18 §11.2.1 Figure 7 / Table 4, p.24) — 24 octets
    /// total. Figure 6 places `acf_payload_data` immediately after
    /// `stream_id`, so there is nothing between the two halves; the
    /// header's `ntscf_data_length` (12) is exactly the ACF half's length.
    pub const NTSCF_ACF_ABB_FRAME_GOLDEN_BYTES: [u8; 24] = [
        // NTSCF header (TC18 Figure 6, p.22)
        0x82, 0x80, 0x0C, 0x07, 0x02, 0x42, 0xAC, 0x11, 0x00, 0x02, 0x00, 0x07,
        // ACF_ABB message (TC18 Figure 7 / Table 4, p.24)
        0x1C, 0x03, 0x00, 0x05, 0x80, 0x11, 0x40, 0x04, 0xDE, 0xAD, 0xBE, 0xEF,
    ];
}

/// go-RCP interop cross-check bytes. Each constant here was produced by a
/// standalone Go program (not part of this repository) built against
/// `SoundMatt/go-RCP` at commit `bdc760fb057f067cfb68199b6c3d0edab9e0c671`,
/// using the field values noted on each constant. See this module's doc
/// comment for the full methodology and the divergence this cross-check
/// found.
pub mod go_rcp_crosscheck {
    /// Produced by:
    /// ```go
    /// avtp.EncodeHeader(avtp.Header{
    ///     Timed: false, StreamIDValid: true, SequenceNum: 0x07,
    ///     DataLength: 13,
    ///     StreamID: avtp.NewStreamID([6]byte{0x02,0x42,0xAC,0x11,0x00,0x02}, 0x0007),
    /// })
    /// ```
    /// — the same `sequence_num`/`stream_id`/logical data-length as
    /// [`super::golden::ntscf_header_fields`]. 13 bytes; TC18 v0.5.1_RC
    /// §11.1 Figure 6 (p.22) specifies 12, which is what
    /// [`super::golden::NTSCF_GOLDEN_BYTES`] now is — go-RCP's extra octet
    /// is a divergence from the specification, not merely from this crate.
    /// See this module's doc comment's "Header/message length" bullet and
    /// "Resolution" section.
    pub const GO_RCP_UNTIMED_HEADER_BYTES: [u8; 13] = [
        0x82, 0x80, 0x07, 0x00, 0x0D, 0x02, 0x42, 0xAC, 0x11, 0x00, 0x02, 0x00, 0x07,
    ];

    /// Produced by:
    /// ```go
    /// avtp.EncodeHeader(avtp.Header{
    ///     Timed: true, StreamIDValid: true, SequenceNum: 0x2A,
    ///     DataLength: 18,
    ///     StreamID: avtp.NewStreamID([6]byte{0x02,0x42,0xAC,0x11,0x00,0x03}, 0x0008),
    ///     Timestamp: 0x1A2B3C4D, TimestampStatus: avtp.TimestampValid,
    /// })
    /// ```
    /// — the same `sequence_num`/`stream_id`/`avtp_timestamp` as
    /// [`super::golden::tscf_header_fields`] (the logical data-length input
    /// differs by one, 18 here vs. 19 there, an artifact of this
    /// cross-check's own setup rather than a spec question — it has no
    /// effect on the structural comparison this module documents). 17
    /// bytes; TC18 v0.5.1_RC §11.1 Figure 5 (p.22) specifies 24. Note also
    /// the leading `0x83` — the subtype both implementations originally
    /// agreed on, and which the specification contradicts: Figure 5 and
    /// Figure 19 (p.79) both give `subtype(0x05)`. See this module's doc
    /// comment's "Header/message length", "`subtype` for the timed
    /// header", and "Timestamp-validity marker" bullets.
    pub const GO_RCP_TIMED_HEADER_BYTES: [u8; 17] = [
        0x83, 0x84, 0x2A, 0x00, 0x12, 0x02, 0x42, 0xAC, 0x11, 0x00, 0x03, 0x00, 0x08, 0x1A, 0x2B,
        0x3C, 0x4D,
    ];

    /// Produced by:
    /// ```go
    /// acf.EncodeMessage(acf.Message{
    ///     Kind: acf.KindShort, ByteBusID: 0x05, TransactionNum: 0x11,
    ///     Control: acf.FlagAck | acf.FlagResponse, ReadSizeOrSegment: 0x04,
    ///     Body: []byte{0xDE, 0xAD, 0xBE, 0xEF},
    /// })
    /// ```
    /// — `byte_bus_id`/`transaction_num`/`read_size_segment`/body bytes
    /// logically matching [`super::golden::acf_abb_fields`], with `Ack`
    /// mapped from `evt.ack` and `Response` from `rsp`. 14 bytes, not 12
    /// (rust-RCP-W01/W02's TC18-reconciled header shrank
    /// [`super::golden::ACF_ABB_GOLDEN_BYTES`] to 12 bytes; go-RCP's own
    /// layout, and this vector, are unaffected by that reconciliation);
    /// message-kind tag `0x01`, not `0x0E`: see this module's doc comment's
    /// "Header/message length", "Message-kind discriminant", and
    /// "`byte_bus_id`/endpoint-address width" divergences.
    pub const GO_RCP_KIND_SHORT_MESSAGE_BYTES: [u8; 14] = [
        0x01, 0x00, 0x00, 0x0E, 0x05, 0x00, 0x11, 0x90, 0x00, 0x04, 0xDE, 0xAD, 0xBE, 0xEF,
    ];

    /// Produced by:
    /// ```go
    /// acf.EncodeMessage(acf.Message{
    ///     Kind: acf.KindLong, ByteBusID: 0x05, TransactionNum: 0x12,
    ///     Control: acf.FlagWrite | acf.FlagResponse,
    ///     Timestamp: 0x0102030405060708, Body: []byte{0xCA, 0xFE},
    /// })
    /// ```
    /// — `byte_bus_id`/`transaction_num`/`message_timestamp`/body bytes
    /// logically matching [`super::golden::acf_gbb_fields`], with `Write`
    /// mapped from `op` and `Response` from `rsp`. Coincidentally also 20
    /// bytes after rust-RCP-W01/W02's TC18-reconciled header (previously
    /// 19; see [`super::golden::ACF_GBB_GOLDEN_BYTES`]'s own doc comment
    /// for why 20 is now correct: 2 real pad octets, not merely a shorter
    /// header) — the byte *contents* still diverge completely, which is
    /// what [`super::tests::go_rcp_bytes_diverge_from_this_crates_own_encoding`]
    /// actually checks; message-kind tag `0x02`, not `0x0D`: same
    /// remaining divergences as [`GO_RCP_KIND_SHORT_MESSAGE_BYTES`] above.
    pub const GO_RCP_KIND_LONG_MESSAGE_BYTES: [u8; 20] = [
        0x02, 0x00, 0x00, 0x14, 0x05, 0x00, 0x12, 0x30, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05,
        0x06, 0x07, 0x08, 0xCA, 0xFE,
    ];
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::{go_rcp_crosscheck, golden};
    use crate::acf;
    use crate::avtp::{self, StreamId};

    // ── Self-referential golden vectors ───────────────────────────────────

    #[test]
    //fusa:test REQ-CONF-001
    fn ntscf_header_matches_golden_bytes_both_directions() {
        let hdr = golden::ntscf_header_fields();
        let encoded = avtp::encode_ntscf_header(&hdr).unwrap();
        assert_eq!(
            encoded,
            golden::NTSCF_GOLDEN_BYTES,
            "encode_ntscf_header's output drifted from the pinned golden vector"
        );
        let decoded = avtp::decode_ntscf_header(&golden::NTSCF_GOLDEN_BYTES).unwrap();
        assert_eq!(
            decoded, hdr,
            "decode_ntscf_header of the pinned golden vector no longer recovers the documented fields"
        );
    }

    #[test]
    //fusa:test REQ-CONF-002
    fn tscf_header_matches_golden_bytes_both_directions() {
        let hdr = golden::tscf_header_fields();
        // Sanity: this vector's avtp_timestamp must stay non-degenerate —
        // if it ever became zero, this would no longer exercise a genuine
        // timed value under crate::timestamp::AvtpTimestamp's
        // all-zero-is-untimed convention.
        assert_ne!(hdr.avtp_timestamp, 0);
        let encoded = avtp::encode_tscf_header(&hdr).unwrap();
        assert_eq!(
            encoded,
            golden::TSCF_GOLDEN_BYTES,
            "encode_tscf_header's output drifted from the pinned golden vector"
        );
        let decoded = avtp::decode_tscf_header(&golden::TSCF_GOLDEN_BYTES).unwrap();
        assert_eq!(
            decoded, hdr,
            "decode_tscf_header of the pinned golden vector no longer recovers the documented fields"
        );
    }

    #[test]
    //fusa:test REQ-CONF-003
    fn acf_abb_matches_golden_bytes_both_directions() {
        let msg = golden::acf_abb_fields();
        let encoded = acf::encode_acf_abb(&msg).unwrap();
        assert_eq!(
            encoded,
            golden::ACF_ABB_GOLDEN_BYTES,
            "encode_acf_abb's output drifted from the pinned golden vector"
        );
        let decoded = acf::decode_acf_abb(&golden::ACF_ABB_GOLDEN_BYTES).unwrap();
        assert_eq!(
            decoded, msg,
            "decode_acf_abb of the pinned golden vector no longer recovers the documented fields"
        );
    }

    #[test]
    //fusa:test REQ-CONF-004
    fn acf_gbb_matches_golden_bytes_both_directions() {
        let msg = golden::acf_gbb_fields();
        // Sanity: this vector's message_timestamp must stay non-zero — see
        // acf_gbb_fields's own doc comment.
        assert_ne!(msg.message_timestamp, 0);
        let encoded = acf::encode_acf_gbb(&msg).unwrap();
        assert_eq!(
            encoded,
            golden::ACF_GBB_GOLDEN_BYTES,
            "encode_acf_gbb's output drifted from the pinned golden vector"
        );
        let decoded = acf::decode_acf_gbb(&golden::ACF_GBB_GOLDEN_BYTES).unwrap();
        assert_eq!(
            decoded, msg,
            "decode_acf_gbb of the pinned golden vector no longer recovers the documented fields"
        );
    }

    #[test]
    //fusa:test REQ-CONF-005
    fn ntscf_acf_abb_frame_matches_golden_bytes_both_directions() {
        let sid = StreamId::new(golden::SENDER_MAC_1, golden::UNIQUE_ID_1);
        let abb_bytes = acf::encode_acf_abb(&golden::acf_abb_fields()).unwrap();
        let frame = avtp::encode_ntscf_frame(sid, 0x07, &abb_bytes).unwrap();
        assert_eq!(
            frame,
            golden::NTSCF_ACF_ABB_FRAME_GOLDEN_BYTES,
            "encode_ntscf_frame's output drifted from the pinned golden vector"
        );

        let (hdr, payload) =
            avtp::decode_ntscf_frame(&golden::NTSCF_ACF_ABB_FRAME_GOLDEN_BYTES).unwrap();
        assert_eq!(hdr, golden::ntscf_header_fields());
        assert_eq!(payload, abb_bytes.as_slice());
        let decoded_abb = acf::decode_acf_abb(payload).unwrap();
        assert_eq!(decoded_abb, golden::acf_abb_fields());
    }

    // ── go-RCP interop cross-check ──────────────────────────────────────

    #[test]
    //fusa:test REQ-CONF-006
    fn go_rcp_bytes_diverge_from_this_crates_own_encoding() {
        // Per this module's doc comment: rust-RCP and go-RCP, both
        // independently interpreting the same (confidential) TC18 spec
        // text, arrived at different concrete wire bytes. This assertion
        // pins that known divergence — see the "Cross-check methodology
        // and result" section of this module's doc comment for the
        // specifics — rather than silently letting it drift unnoticed
        // either direction (into accidental agreement, which would be
        // worth knowing about, or into a *different* kind of disagreement).
        assert_ne!(
            golden::NTSCF_GOLDEN_BYTES.as_slice(),
            go_rcp_crosscheck::GO_RCP_UNTIMED_HEADER_BYTES.as_slice(),
            "rust-RCP's NTSCF header bytes now match go-RCP's — the module doc \
             comment's recorded divergence needs to be revisited, not just this assertion"
        );
        assert_ne!(
            golden::TSCF_GOLDEN_BYTES.as_slice(),
            go_rcp_crosscheck::GO_RCP_TIMED_HEADER_BYTES.as_slice(),
        );
        assert_ne!(
            golden::ACF_ABB_GOLDEN_BYTES.as_slice(),
            go_rcp_crosscheck::GO_RCP_KIND_SHORT_MESSAGE_BYTES.as_slice(),
        );
        assert_ne!(
            golden::ACF_GBB_GOLDEN_BYTES.as_slice(),
            go_rcp_crosscheck::GO_RCP_KIND_LONG_MESSAGE_BYTES.as_slice(),
        );
    }

    #[test]
    //fusa:test REQ-CONF-006
    fn stream_id_bytes_agree_with_go_rcp_crosscheck() {
        // The one part of the cross-check that *does* agree: both
        // implementations place the 6-byte sender MAC in the high-order
        // stream_id bytes and the 2-byte suffix in the low-order bytes.
        // NTSCF-equivalent pair: bytes 4..12 of the rust vector (TC18
        // §11.1 Figure 6's stream_id position, p.22) against bytes 5..13
        // of the go-RCP vector (go-RCP's own stream_id position) — the
        // *positions* differ (documented above), but the 8 stream_id bytes
        // themselves must be identical.
        assert_eq!(
            &golden::NTSCF_GOLDEN_BYTES[4..12],
            &go_rcp_crosscheck::GO_RCP_UNTIMED_HEADER_BYTES[5..13],
            "stream_id bytes no longer agree between rust-RCP and go-RCP"
        );
        assert_eq!(
            &golden::TSCF_GOLDEN_BYTES[4..12],
            &go_rcp_crosscheck::GO_RCP_TIMED_HEADER_BYTES[5..13],
            "stream_id bytes no longer agree between rust-RCP and go-RCP"
        );

        // Cross-checked directly against StreamId's own composition, too.
        let sid1 = StreamId::new(golden::SENDER_MAC_1, golden::UNIQUE_ID_1);
        assert_eq!(
            &golden::NTSCF_GOLDEN_BYTES[4..12],
            sid1.to_u64().to_be_bytes()
        );
    }

    #[test]
    //fusa:test REQ-CONF-006
    fn go_rcp_crosscheck_message_kind_discriminants_differ_from_this_crates_own() {
        // A narrower, explicit check on the "Message-kind discriminant"
        // divergence documented above, independent of the byte-array-wide
        // comparisons the tests above already do.
        assert_eq!(go_rcp_crosscheck::GO_RCP_KIND_SHORT_MESSAGE_BYTES[0], 0x01);
        assert_eq!(acf::ACF_ABB_MSG_TYPE, 0x0E);
        assert_ne!(
            go_rcp_crosscheck::GO_RCP_KIND_SHORT_MESSAGE_BYTES[0],
            acf::ACF_ABB_MSG_TYPE
        );

        assert_eq!(go_rcp_crosscheck::GO_RCP_KIND_LONG_MESSAGE_BYTES[0], 0x02);
        assert_eq!(acf::ACF_GBB_MSG_TYPE, 0x0D);
        assert_ne!(
            go_rcp_crosscheck::GO_RCP_KIND_LONG_MESSAGE_BYTES[0],
            acf::ACF_GBB_MSG_TYPE
        );
    }
}
