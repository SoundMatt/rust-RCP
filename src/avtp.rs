//fusa:req REQ-NTSCF-001
//fusa:req REQ-NTSCF-002
//fusa:req REQ-NTSCF-003
//fusa:req REQ-NTSCF-004
//fusa:req REQ-NTSCF-005
//fusa:req REQ-NTSCF-006
//fusa:req REQ-TSCF-001
//fusa:req REQ-TSCF-002
//fusa:req REQ-TSCF-003
//fusa:req REQ-TSCF-004
//fusa:req REQ-TSCF-005
//fusa:req REQ-TSCF-006
//fusa:req REQ-HVSEL-001
//fusa:req REQ-HVSEL-002
//fusa:req REQ-HVSEL-003
//fusa:req REQ-HVSEL-004
//fusa:req REQ-HVSEL-005
//fusa:req REQ-SID-001
//fusa:req REQ-SID-002
//fusa:req REQ-SID-003
//fusa:req REQ-SID-004
//fusa:req REQ-SID-005

//! IEEE 1722 AVTPDU framing — TC18 wire format core (`ROADMAP.md` Milestone 1).
//!
//! This module begins the replacement of the legacy 16-byte frame that used
//! to live in `crate::wire` (deleted by `ROADMAP.md` Milestone 9's `wire`
//! REPLACE cutover — see this module's "Frame composition" section below)
//! with the real OPEN Alliance TC18 Remote Control Protocol wire format. It
//! models the two AVTPDU header variants named on the
//! Milestone 1 checklist:
//!
//! - **NTSCF** — per this crate's own spec-extraction notes, the only header
//!   variant an RC Server ever sends.
//! - **TSCF** — the client-to-server counterpart, carrying an additional
//!   `avtp_timestamp` field. Per the same notes, a client is the only side
//!   that ever needs to *encode* one; [`encode_tscf_header`] is provided
//!   for symmetry (and so the round-trip tests below can exercise it) but
//!   an RC Server's own send path has no occasion to call it. Nothing in
//!   this module enforces that direction at the type level — it is a
//!   protocol-level convention, documented here rather than compiled in.
//!
//! [`select_header_variant`] layers the Milestone 1 "header-variant
//! selection/rejection rule" on top of the two decoders above: a receiving
//! server's [`TimeSyncCapability`] gates whether a TSCF-subtyped AVTPDU is
//! decoded at all. NTSCF carries no timing assumption and is accepted
//! unconditionally; TSCF's `avtp_timestamp` is only meaningful to a server
//! that participates in time synchronization, so a time-sync-incapable
//! server drops it outright rather than decoding the remainder of the
//! header.
//!
//! The two ACF message types (implemented separately in [`crate::acf`]) are
//! a separate item on the same Milestone 1 checklist and are intentionally
//! not implemented here. Full timestamp semantics — `avtp_timestamp`'s
//! width/rollover behavior and the invalid-timestamp fallback rule — are
//! also a separate item, now implemented in [`crate::timestamp`]
//! ([`crate::timestamp::AvtpTimestamp`]) as a standalone newtype rather
//! than a change to [`TscfHeader::avtp_timestamp`]'s own field type; see
//! that module's doc comment.
//!
//! `stream_id` construction/parsing — the first "Addressing" checklist
//! item — *is* implemented in this module: [`StreamId`] decomposes/composes
//! the opaque 64-bit value carried by [`NtscfHeader::stream_id`] and
//! [`TscfHeader::stream_id`] into a sender MAC address and a
//! locally-assigned unique-id suffix, via [`build_stream_id`]/
//! [`parse_stream_id`] (or the [`StreamId::to_u64`]/[`StreamId::from_u64`]
//! wrappers around them). This is additive, matching the discipline used by
//! every prior Milestone 1 entry: [`NtscfHeader::stream_id`] and
//! [`TscfHeader::stream_id`] remain plain `u64` fields rather than being cut
//! over to [`StreamId`] itself, so no existing caller of either header type
//! changes shape. The remaining two "Addressing" checklist items —
//! `(stream_id, byte_bus_id)` endpoint lookup (with `byte_bus_id`'s
//! stream-relative, not global, uniqueness) and the echo-back rule for
//! responses/acks — are still separate, later work.
//!
//! ## Frame composition (`ROADMAP.md` Milestone 9)
//!
//! Every item above stops at a decoded/encoded *header* — assembling a
//! whole on-wire AVTPDU (an NTSCF header followed by its ACF payload) was
//! explicitly left for "whichever later milestone actually builds that
//! request/response lifecycle." [`encode_ntscf_frame`]/
//! [`decode_ntscf_frame`] are that composition step, added as part of
//! Milestone 9's `wire` REPLACE-disposition cutover: they combine an
//! [`NtscfHeader`] with an already-encoded [`crate::acf::AcfAbbMessage`]/
//! [`crate::acf::AcfGbbMessage`] payload (or any other bytes a caller
//! supplies) into one transportable frame, and split one back apart. This
//! is the piece that lets a transport (`crate::udp`, `crate::shmem`)
//! stop calling the legacy `crate::wire` frame encoder/decoder — deleted by
//! this same milestone item, since nothing else in the crate constructed
//! its 16-byte frame — in favor of the real TC18 wire format built here and
//! in `crate::acf`. Per this module's own discipline, neither function
//! parses or interprets the ACF payload itself; that remains `crate::acf`'s
//! job.
//!
//! ## Provenance
//!
//! Every byte offset, bit width and constant below is transcribed from the
//! OPEN Alliance TC18 "Remote Control Specification for Ethernet"
//! v0.5.1_RC, §11.1 "Usage of IEEE1722 for RCP", page 22:
//!
//! - **Figure 5 — TSCF-Header Version 0** (normative), six quadlets:
//!   `subtype(0x05)` (bits 0-7), `sv` (8), `version(0x0)` (9-11), `mr`
//!   (12), `rsv` (13-14), `tv` (15), `sequence_num` (16-23), `reserved`
//!   (24-30), `tu` (31); then `stream_id` (2 quadlets), `avtp_timestamp`
//!   (1 quadlet), `reserved` (1 quadlet), and a final quadlet holding
//!   `stream_data_length(octets)` (16 bits) + `reserved` (16 bits).
//!   Total: 24 octets, immediately followed by `acf_payload_data`.
//! - **Figure 6 — NTSCF-Header Version 0** (normative), three quadlets:
//!   `subtype(0x82)` (bits 0-7), `sv` (8), `version(0x0)` (9-11), `r`
//!   (12), `ntscf_data_length` (13-23), `sequence_num` (24-31); then
//!   `stream_id` (2 quadlets). Total: 12 octets, immediately followed by
//!   `acf_payload_data` — there is **no** reserved gap between the first
//!   quadlet and `stream_id`, and no `avtp_timestamp`/`reserved` quadlets
//!   at all.
//!
//! Both figures are cross-checked against the specification's own worked
//! examples on page 79 — Figure 19 (single ACF_ABB under a TSCF header,
//! `subtype(0x05)`, `stream_data_length(octets) = 0x003C`) and Figure 20
//! (single ACF_GBB under an NTSCF header, `subtype(0x82)`,
//! `ntscf_data_length = 0x038`) — which reproduce the same field order
//! and widths. Figures 5/6/19/20 are vector images in the source PDF with
//! no extractable text layer; the bit-boundary tick marks were read
//! directly from a 600 dpi render of pages 22 and 79.
//!
//! TC18.txt citation-drift note (issue #164 batch 1): the current TC18.txt
//! revision's own (text) figure captions renumber these two worked
//! examples as Figure 20 ("ACF_ABB under TSCF header (example)", TC18.txt
//! line 4231) and Figure 21 ("ACF_GBB under NTSCF header (example)",
//! TC18.txt line ~4256) rather than Figure 19/Figure 20 — see
//! `crate::acf`'s own "acf_msg_length quadlet semantics" doc section for
//! the same finding and a content-change caveat on the ACF_GBB example.
//! This module's own doc comments and its
//! `ntscf_header_matches_figure_20_worked_example`/
//! `tscf_header_matches_figure_19_worked_example` test names deliberately
//! still say "Figure 19"/"Figure 20" throughout, matching each other and
//! `crate::acf`/`crate::e2e`'s own still-unrenamed test names — renumbering
//! is left for a follow-up that renames those test functions together,
//! rather than mixing old and new figure numbers across files.
//!
//! Note the asymmetry, which is real and not a transcription slip:
//! NTSCF's `ntscf_data_length` is an **11-bit** field packed into the
//! first quadlet, while TSCF's `stream_data_length` is a **16-bit** field
//! occupying its own quadlet. See [`NTSCF_DATA_LENGTH_MAX`] and
//! [`TSCF_DATA_LENGTH_MAX`].
//!
//! [`StreamId`]'s split — sender MAC in the upper 48 bits, locally-assigned
//! unique-id suffix in the lower 16 bits — follows the widely used IEEE
//! 1722 AVTP convention of that name (talker MAC high, per-talker stream
//! discriminant low), and is confirmed by TC18 v0.5.1_RC §12.6.1 Table 18
//! ("Discovery request") and §12.6.2 Table 19 ("Discovery response")
//! (TC18.txt lines 2758/2805 — table renumbered from Table 16/Table 17
//! since this note was written; see issue #164), page 48, which both
//! spell the field out as
//! `stream_id = streamMAC + unique_id` with `streamMAC: 6bytes` and
//! `unique_id = 0x0000` (2 bytes) — MAC first, suffix second, in a 64-bit
//! field. This was previously flagged under Guiding Principle 5 as an
//! unreconciled working interpretation; those two tables are the
//! reconciliation.

use crate::RcpError;

mod transport;
pub(crate) use transport::{deadline_done, poll_slice, remaining};
pub use transport::{
    request_acf_abb, request_acf_gbb, AvtpTransport, LoopbackTransport, AVTP_MAX_FRAME_LEN,
};

// ── Constants ─────────────────────────────────────────────────────────────────

/// AVTPDU `subtype` value identifying an NTSCF-headed PDU.
///
/// TC18 v0.5.1_RC §11.1 Figure 6 (p.22), `subtype(0x82)`.
pub const NTSCF_SUBTYPE: u8 = 0x82;

/// Total NTSCF header length in bytes (up to, but not including, the first
/// ACF message).
///
/// TC18 v0.5.1_RC §11.1 Figure 6 (p.22): exactly three quadlets — one
/// packed quadlet plus a 64-bit `stream_id` — with `acf_payload_data`
/// starting immediately at octet 12.
pub const NTSCF_HEADER_LEN: usize = 12;

/// `ntscf_data_length` is an 11-bit field (TC18 v0.5.1_RC §11.1 Figure 6,
/// p.22, bits 13-23 of the first quadlet); this is its maximum
/// representable value.
pub const NTSCF_DATA_LENGTH_MAX: u16 = 0x07FF;

// ── NtscfHeader ───────────────────────────────────────────────────────────────

/// Decoded NTSCF AVTPDU header.
///
/// `stream_id` is carried here as a plain 64-bit field — this struct itself
/// does not decompose it. Use [`StreamId::from_u64`]/[`StreamId::to_u64`]
/// (or the [`parse_stream_id`]/[`build_stream_id`] functions they wrap) to
/// interpret or construct its sender-MAC/unique-id-suffix structure; see
/// this module's doc comment for why the field remains untyped here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-NTSCF-001
pub struct NtscfHeader {
    /// Per-stream sequence number, incremented once per NTSCF AVTPDU sent.
    pub sequence_num: u8,
    /// Length, in bytes, of the ACF message(s) carried after this header.
    /// Valid range is `0..=NTSCF_DATA_LENGTH_MAX` (11 bits).
    pub ntscf_data_length: u16,
    /// AVTP `stream_id`, carried as a plain `u64`. See [`StreamId`] to
    /// decompose/compose its sender-MAC/unique-id-suffix structure.
    pub stream_id: u64,
}

impl NtscfHeader {
    /// This header's packed octet 1 (`sv | version | r |
    /// ntscf_data_length[10:8]`), exactly as [`encode_ntscf_header`] emits
    /// it — the `header_octet1` value c-RCP's E2E CRC coverage feeds
    /// (`c-RCP/src/e2e.c:142`). `sv` is always 1 and `version`/`r` always 0
    /// (this crate's NTSCF model carries neither).
    //fusa:req REQ-AVTP-035
    pub fn octet1(&self) -> u8 {
        0x80 | ((self.ntscf_data_length >> 8) as u8 & 0x07)
    }
}

// ── Encoding helpers (local to this module; wire.rs's are private there) ──────

fn put_u64_be(buf: &mut [u8], offset: usize, v: u64) {
    buf[offset..offset + 8].copy_from_slice(&v.to_be_bytes());
}

fn get_u64_be(b: &[u8]) -> u64 {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&b[..8]);
    u64::from_be_bytes(bytes)
}

// ── Encode / decode ───────────────────────────────────────────────────────────

/// Encode an [`NtscfHeader`] to its 12-byte wire representation.
///
/// Layout, per TC18 v0.5.1_RC §11.1 Figure 6 (p.22):
///
/// ```text
/// octet 0    : subtype = 0x82
/// octet 1    : sv(1) | version(3) | r(1) | ntscf_data_length[10:8](3)
/// octet 2    : ntscf_data_length[7:0]
/// octet 3    : sequence_num
/// octets 4-11: stream_id (big-endian u64)
/// ```
///
/// Returns `Err(RcpError::InvalidSize)` if `ntscf_data_length` exceeds the
/// 11-bit field width.
//fusa:req REQ-NTSCF-002
pub fn encode_ntscf_header(hdr: &NtscfHeader) -> Result<[u8; NTSCF_HEADER_LEN], RcpError> {
    if hdr.ntscf_data_length > NTSCF_DATA_LENGTH_MAX {
        return Err(RcpError::InvalidSize);
    }

    let mut buf = [0u8; NTSCF_HEADER_LEN];
    buf[0] = NTSCF_SUBTYPE;
    // sv=1 (stream_id valid), version=000, r=0, then ntscf_data_length's
    // top 3 bits share this octet's low nibble-and-a-bit.
    buf[1] = hdr.octet1();
    buf[2] = (hdr.ntscf_data_length & 0x00FF) as u8;
    buf[3] = hdr.sequence_num;
    put_u64_be(&mut buf, 4, hdr.stream_id);
    Ok(buf)
}

/// Decode an [`NtscfHeader`] from a byte slice.
///
/// Never panics on short, truncated, or arbitrary input — always returns
/// `Err` instead.
//fusa:req REQ-NTSCF-003
//fusa:req REQ-NTSCF-004
//fusa:req REQ-NTSCF-006
pub fn decode_ntscf_header(b: &[u8]) -> Result<NtscfHeader, RcpError> {
    if b.len() < NTSCF_HEADER_LEN {
        return Err(RcpError::ShortFrame);
    }
    if b[0] != NTSCF_SUBTYPE {
        return Err(RcpError::Other(format!(
            "ntscf: expected subtype 0x{:02X}, got 0x{:02X}",
            NTSCF_SUBTYPE, b[0]
        )));
    }
    let sv = (b[1] >> 7) & 0x1;
    if sv != 1 {
        return Err(RcpError::Other(
            "ntscf: sv bit must be 1 (stream_id always valid for NTSCF)".into(),
        ));
    }

    // ntscf_data_length (11 bits) = low 3 bits of octet 1 (its high bits)
    // followed by all of octet 2. TC18 §11.1 Figure 6 (p.22), bits 13-23.
    let ntscf_data_length = (u16::from(b[1] & 0x07) << 8) | u16::from(b[2]);
    let sequence_num = b[3];
    let stream_id = get_u64_be(&b[4..12]);

    Ok(NtscfHeader {
        sequence_num,
        ntscf_data_length,
        stream_id,
    })
}

// ── TscfHeader ────────────────────────────────────────────────────────────────

/// AVTPDU `subtype` value identifying a TSCF-headed PDU.
///
/// TC18 v0.5.1_RC §11.1 Figure 5 (p.22), `subtype(0x05)`, confirmed by the
/// worked example in Figure 19 (p.79). Note that this is *not* `0x82 | 1`
/// or any other offset from [`NTSCF_SUBTYPE`] — the two subtypes are
/// unrelated code points assigned by IEEE 1722, and TSCF's is the smaller
/// of the two.
pub const TSCF_SUBTYPE: u8 = 0x05;

/// Total TSCF header length in bytes (up to, but not including, the first
/// ACF message). Wider than [`NTSCF_HEADER_LEN`] to carry `avtp_timestamp`
/// and the two extra reserved/length quadlets.
///
/// TC18 v0.5.1_RC §11.1 Figure 5 (p.22): six quadlets.
pub const TSCF_HEADER_LEN: usize = 24;

/// `stream_data_length` is a full 16-bit field occupying the first half of
/// TSCF's sixth quadlet (TC18 v0.5.1_RC §11.1 Figure 5, p.22); this is its
/// maximum representable value.
///
/// This deliberately differs from [`NTSCF_DATA_LENGTH_MAX`]'s 11 bits —
/// the two header variants really do size their length fields
/// differently, so a value legal under TSCF may be too large for NTSCF.
pub const TSCF_DATA_LENGTH_MAX: u16 = 0xFFFF;

/// Decoded TSCF AVTPDU header.
///
/// TSCF is the client-to-server counterpart of [`NtscfHeader`]: per this
/// module's doc comment, an RC Server never needs to *encode* one, though
/// it must be able to decode one (an RC Client uses TSCF when it has
/// time-synchronized data to send). `stream_id` is carried here as a plain
/// 64-bit value, same as [`NtscfHeader::stream_id`] — see that field's doc
/// comment for how to decompose/compose it via [`StreamId`].
///
/// Every field of TC18 §11.1 Figure 5 is modeled explicitly, mirroring
/// c-RCP's `rcp_avtp_tscf_header_t` (`c-RCP/include/rcp/avtp.h:209-252`):
/// `version`, `mr`, `tv` and `tu` are carried through encode/decode as
/// independent wire bits (in particular `tv` is **not** inferred from
/// `avtp_timestamp != 0`, so a valid timestamp of 0 is representable), and
/// the two reserved regions are surfaced on decode for TC18 §13.3's
/// reserved-bytes rule ([`tscf_reserved_all_zero`]). `sv` is not a field:
/// [`encode_tscf_header`] always sets it and [`decode_tscf_header`] rejects
/// a header without it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
//fusa:req REQ-TSCF-001
//fusa:req REQ-AVTP-004
pub struct TscfHeader {
    /// AVTP `version` (octet 1 bits 6-4). Always 0 for TC18's Version-0
    /// header; carried through unmodified (masked to 3 bits on encode).
    pub version: u8,
    /// Media-clock restart (octet 1 bit 3). Carried through unmodified;
    /// no RCP-level meaning.
    pub mr: bool,
    /// `avtp_timestamp` valid (octet 1 bit 0). TC18 §13.3: a request under
    /// a TSCF header with `tv = 0` has its timestamp ignored.
    pub tv: bool,
    /// Per-stream sequence number, incremented once per TSCF AVTPDU sent.
    pub sequence_num: u8,
    /// `avtp_timestamp` uncertain (octet 3 bit 0). TC18 §13.3: "In case the
    /// time stamp is uncertain (i.e. tu = 1), then this shall be executed
    /// as if tu = 0" — decoded for inspection (and for the E2E CRC
    /// coverage, which includes it) but carrying no admission meaning.
    pub tu: bool,
    /// 32-bit AVTP presentation timestamp (nanoseconds mod 2^32). Only
    /// meaningful when [`Self::tv`] is set; see [`extend_timestamp`] for
    /// reconstructing the 48-bit gPTP instant it denotes.
    pub avtp_timestamp: u32,
    /// Length, in bytes, of the ACF message(s) carried after this header.
    /// The wire field is a full 16 bits wide
    /// ([`TSCF_DATA_LENGTH_MAX`]), so every `u16` is representable.
    pub stream_data_length: u16,
    /// AVTP `stream_id`, carried as a plain `u64`. See
    /// [`NtscfHeader::stream_id`].
    pub stream_id: u64,
    /// Reserved octets 16-19, populated on decode only.
    ///
    /// c-RCP parity: like `rcp_avtp_tscf_header_t::reserved0`
    /// (`c-RCP/include/rcp/avtp.h:236-250`), this is decode-only:
    /// [`encode_tscf_header`] always zero-fills octets 16-19 regardless of
    /// this value, so a non-zero value here does not round-trip.
    pub reserved0: u32,
    /// Reserved octets 22-23, populated on decode only. Same decode-only
    /// convention as [`Self::reserved0`].
    pub reserved1: u16,
}

impl TscfHeader {
    /// This header's packed octet 1 (`sv | version | mr | rsv | tv`), as
    /// [`encode_tscf_header`] emits it — the `header_octet1` value c-RCP's
    /// E2E CRC coverage feeds (`c-RCP/src/e2e.c:142`).
    //fusa:req REQ-AVTP-035
    pub fn octet1(&self) -> u8 {
        0x80 | ((self.version & 0x07) << 4) | (u8::from(self.mr) << 3) | u8::from(self.tv)
    }

    /// The timestamp-validity reading of this header, taken from the `tv`
    /// bit rather than from the raw `avtp_timestamp` value (TC18 §13.3:
    /// "with the time stamp marked as invalid (i.e. tv = 0), the timestamp
    /// is ignored"). `tu` is deliberately not consulted (§13.3: tu = 1 is
    /// executed as if tu = 0).
    //fusa:req REQ-TS-004
    pub fn timestamp_meaning(&self) -> crate::timestamp::TimestampMeaning {
        crate::timestamp::TimestampMeaning::from_valid_bit(self.tv)
    }
}

/// Encode a [`TscfHeader`] to its 24-byte wire representation.
///
/// Layout, per TC18 v0.5.1_RC §11.1 Figure 5 (p.22):
///
/// ```text
/// octet  0    : subtype = 0x05
/// octet  1    : sv(1) | version(3) | mr(1) | rsv(2) | tv(1)
/// octet  2    : sequence_num
/// octet  3    : reserved(7) | tu(1)
/// octets 4-11 : stream_id (big-endian u64)
/// octets 12-15: avtp_timestamp (big-endian u32)
/// octets 16-19: reserved
/// octets 20-21: stream_data_length (big-endian u16)
/// octets 22-23: reserved
/// ```
///
/// Mirrors `rcp_avtp_encode_tscf` (`c-RCP/src/avtp.c:175-207`): `sv` is
/// always 1, `version`/`mr`/`tv`/`tu` are taken from `hdr` verbatim
/// (masked to their field widths), and both reserved regions are always
/// zero-filled regardless of [`TscfHeader::reserved0`]/
/// [`TscfHeader::reserved1`].
///
/// Always returns `Ok`. Unlike [`encode_ntscf_header`], there is no
/// oversize-rejection path to take: `stream_data_length`'s wire field is a
/// full 16 bits ([`TSCF_DATA_LENGTH_MAX`] == [`u16::MAX`]), so every value
/// the struct can hold is representable. The `Result` return type is kept
/// for signature symmetry with [`encode_ntscf_header`] and so that adding
/// a future validation rule is not itself a breaking change.
//fusa:req REQ-TSCF-002
//fusa:req REQ-AVTP-004
pub fn encode_tscf_header(hdr: &TscfHeader) -> Result<[u8; TSCF_HEADER_LEN], RcpError> {
    let mut buf = [0u8; TSCF_HEADER_LEN];
    buf[0] = TSCF_SUBTYPE;
    buf[1] = hdr.octet1();
    buf[2] = hdr.sequence_num;
    buf[3] = u8::from(hdr.tu); // reserved(7) | tu
    put_u64_be(&mut buf, 4, hdr.stream_id);
    buf[12..16].copy_from_slice(&hdr.avtp_timestamp.to_be_bytes());
    // octets 16..20 reserved, left zeroed.
    buf[20..22].copy_from_slice(&hdr.stream_data_length.to_be_bytes());
    // octets 22..24 reserved, left zeroed.
    Ok(buf)
}

/// Decode a [`TscfHeader`] from a byte slice.
///
/// Mirrors `rcp_avtp_decode_tscf`'s field extraction
/// (`c-RCP/src/avtp.c:216-245`): every Figure 5 bit — `version`, `mr`,
/// `tv`, `tu` — and both reserved regions are surfaced. This function
/// decodes the header only; [`decode_tscf_frame`] additionally checks the
/// buffer against `stream_data_length`.
///
/// Never panics on short, truncated, or arbitrary input — always returns
/// `Err` instead.
//fusa:req REQ-TSCF-003
//fusa:req REQ-TSCF-004
//fusa:req REQ-TSCF-006
//fusa:req REQ-AVTP-022
//fusa:req REQ-AVTP-023
pub fn decode_tscf_header(b: &[u8]) -> Result<TscfHeader, RcpError> {
    if b.len() < TSCF_HEADER_LEN {
        return Err(RcpError::ShortFrame);
    }
    if b[0] != TSCF_SUBTYPE {
        return Err(RcpError::Other(format!(
            "tscf: expected subtype 0x{:02X}, got 0x{:02X}",
            TSCF_SUBTYPE, b[0]
        )));
    }
    let sv = (b[1] >> 7) & 0x1;
    if sv != 1 {
        return Err(RcpError::Other(
            "tscf: sv bit must be 1 (stream_id always valid for TSCF)".into(),
        ));
    }

    let mut ts_bytes = [0u8; 4];
    ts_bytes.copy_from_slice(&b[12..16]);
    let mut rsv0_bytes = [0u8; 4];
    rsv0_bytes.copy_from_slice(&b[16..20]);

    Ok(TscfHeader {
        version: (b[1] >> 4) & 0x07,
        mr: (b[1] >> 3) & 0x01 != 0,
        tv: b[1] & 0x01 != 0,
        sequence_num: b[2],
        tu: b[3] & 0x01 != 0,
        stream_id: get_u64_be(&b[4..12]),
        avtp_timestamp: u32::from_be_bytes(ts_bytes),
        // stream_data_length is a full 16-bit big-endian field in its own
        // quadlet. TC18 §11.1 Figure 5 (p.22), octets 20-21.
        stream_data_length: u16::from_be_bytes([b[20], b[21]]),
        reserved0: u32::from_be_bytes(rsv0_bytes),
        reserved1: u16::from_be_bytes([b[22], b[23]]),
    })
}

// ── TSCF policy helpers (c-RCP avtp.c:249-294) ───────────────────────────────

/// Reconstruct the 48-bit gPTP instant a 32-bit TSCF `avtp_timestamp`
/// denotes: of every value congruent to `wire_ts` modulo 2^32, return the
/// one closest to `reference_now`.
///
/// Port of `rcp_avtp_extend_timestamp` (`c-RCP/src/avtp.c:249-266`,
/// documented at `c-RCP/include/rcp/avtp.h:264-298`). Intended to be
/// computed once at admission time (`reference_now` = the gPTP clock at
/// that moment) and then compared per tick with
/// [`crate::request::timed_due`]. Like c-RCP, the result is not masked to
/// 48 bits here — [`crate::request::timed_due`] masks at comparison time.
/// A candidate exactly half a period away is kept as-is (strict `>` on
/// both sides, as c-RCP). Never panics: the arithmetic wraps exactly as
/// c-RCP's unsigned `uint64_t` arithmetic does.
//fusa:req REQ-TIMED-012
pub fn extend_timestamp(wire_ts: u32, reference_now: u64) -> u64 {
    const PERIOD: u64 = 1u64 << 32;
    const HALF: u64 = PERIOD / 2;
    let base = reference_now & !(PERIOD - 1);
    let mut candidate = base | u64::from(wire_ts);
    if candidate > reference_now && (candidate - reference_now) > HALF {
        // More than half a period ahead: one period earlier is closer.
        candidate = candidate.wrapping_sub(PERIOD);
    } else if candidate < reference_now && (reference_now - candidate) > HALF {
        // Symmetric case: one period later is closer.
        candidate = candidate.wrapping_add(PERIOD);
    }
    candidate
}

/// Read the AVTPDU subtype octet (offset 0) so a caller can choose between
/// [`decode_ntscf_frame`] and [`decode_tscf_frame`] without a full decode.
///
/// Port of `rcp_avtp_peek_subtype` (`c-RCP/src/avtp.c:271-276`): returns
/// `Err(RcpError::ShortFrame)` only for an empty buffer.
//fusa:req REQ-AVTP-013
pub fn peek_subtype(b: &[u8]) -> Result<u8, RcpError> {
    b.first().copied().ok_or(RcpError::ShortFrame)
}

/// TC18 §13.3's configurable disposition for a TSCF-headed AVTPDU an RC
/// Server cannot (or chooses not to) honour the ordinary way.
///
/// Port of `rcp_avtp_tscf_fallback_t` (`c-RCP/include/rcp/avtp.h:308-332`):
/// one policy knob shared by [`should_drop_tscf`]'s unsupported-time-sync
/// rule and [`tscf_reserved_all_zero`]'s reserved-bytes rule. The default
/// is [`TscfFallback::Drop`] (TC18 §11.1's unconditional wording).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
//fusa:req REQ-AVTP-021
pub enum TscfFallback {
    /// Drop the frame outright, with no response
    /// (`RCP_AVTP_TSCF_FALLBACK_DROP`).
    #[default]
    Drop = 0,
    /// Process the request as if no presentation time were included
    /// (`RCP_AVTP_TSCF_FALLBACK_IGNORE`).
    Ignore = 1,
}

/// Whether a received AVTPDU must be dropped (silently — no response) under
/// TC18 §11.1/§13.3's TSCF-without-time-sync rule.
///
/// Port of `rcp_avtp_should_drop_tscf` (`c-RCP/src/avtp.c:280-288`):
/// `true` iff `subtype` is [`TSCF_SUBTYPE`], the server does not support
/// time synchronization, and `unsupported_time_sync_policy` is
/// [`TscfFallback::Drop`]. Always `false` for any other subtype.
//fusa:req REQ-AVTP-014
//fusa:req REQ-AVTP-021
pub fn should_drop_tscf(
    server_time_sync_supported: bool,
    subtype: u8,
    unsupported_time_sync_policy: TscfFallback,
) -> bool {
    if subtype != TSCF_SUBTYPE {
        return false;
    }
    if !server_time_sync_supported {
        return unsupported_time_sync_policy == TscfFallback::Drop;
    }
    false
}

/// `true` iff both of `hdr`'s reserved regions (octets 16-19 and 22-23)
/// are zero.
///
/// Port of `rcp_avtp_tscf_reserved_all_zero` (`c-RCP/src/avtp.c:291-294`),
/// the predicate for TC18 §13.3's second configurable rule ("If the
/// reserved bytes in the header are all zero, then the request shall be
/// queued as if the header was in NTSCF format or dropped, depending on
/// configuration"); callers combine it with a [`TscfFallback`].
///
/// c-RCP parity: the rule is implemented exactly as §13.3's literal text
/// reads (all-*zero* triggers the fallback), even though a conformant
/// sender — including [`encode_tscf_header`] — always transmits zeros
/// there; c-RCP applies it the same way (`c-RCP/src/mock.c:1816`).
//fusa:req REQ-AVTP-031
pub fn tscf_reserved_all_zero(hdr: &TscfHeader) -> bool {
    hdr.reserved0 == 0 && hdr.reserved1 == 0
}

// ── Header-variant selection/rejection ───────────────────────────────────────

/// A receiving RC Server's time-synchronization capability.
///
/// TSCF's `avtp_timestamp` only carries meaning for a server that
/// participates in network time synchronization (e.g. gPTP/802.1AS). This
/// crate does not yet model *how* a server learns whether it has that
/// capability (that belongs to a later milestone's server-lifecycle work);
/// this type exists purely to make the header-variant selection rule below
/// testable against both outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
//fusa:req REQ-HVSEL-001
pub enum TimeSyncCapability {
    /// The server participates in time synchronization; TSCF-headed
    /// AVTPDUs may be decoded.
    Capable,
    /// The server has no time-synchronization support; TSCF-headed AVTPDUs
    /// are dropped or have their presentation time ignored, per
    /// [`TscfFallback`].
    Incapable,
}

impl TimeSyncCapability {
    fn accepts_tscf(self) -> bool {
        matches!(self, Self::Capable)
    }
}

/// A decoded AVTPDU header, tagged by which Milestone 1 header variant
/// produced it. Carried by [`HeaderSelection`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
//fusa:req REQ-HVSEL-001
pub enum HeaderVariant {
    /// Decoded via [`decode_ntscf_header`].
    Ntscf(NtscfHeader),
    /// Decoded via [`decode_tscf_header`].
    Tscf(TscfHeader),
}

impl HeaderVariant {
    /// The AVTPDU subtype octet of this header ([`NTSCF_SUBTYPE`] or
    /// [`TSCF_SUBTYPE`]) — c-RCP's `avtp_subtype` E2E CRC input.
    //fusa:req REQ-AVTP-035
    pub fn subtype(&self) -> u8 {
        match self {
            Self::Ntscf(_) => NTSCF_SUBTYPE,
            Self::Tscf(_) => TSCF_SUBTYPE,
        }
    }

    /// This header's packed octet 1 — c-RCP's `header_octet1` E2E CRC
    /// input (`c-RCP/src/e2e.c:142`). See [`NtscfHeader::octet1`] and
    /// [`TscfHeader::octet1`].
    //fusa:req REQ-AVTP-035
    pub fn octet1(&self) -> u8 {
        match self {
            Self::Ntscf(h) => h.octet1(),
            Self::Tscf(h) => h.octet1(),
        }
    }

    /// The header's `tu` bit — c-RCP's `tu` E2E CRC input. Always `false`
    /// for NTSCF, which has no such bit (c-RCP passes `false` for
    /// NTSCF-framed calls, `c-RCP/src/e2e.c:351`).
    //fusa:req REQ-AVTP-035
    pub fn tu(&self) -> bool {
        match self {
            Self::Ntscf(_) => false,
            Self::Tscf(h) => h.tu,
        }
    }

    /// The header's opaque 64-bit `stream_id`.
    //fusa:req REQ-AVTP-035
    pub fn stream_id(&self) -> u64 {
        match self {
            Self::Ntscf(h) => h.stream_id,
            Self::Tscf(h) => h.stream_id,
        }
    }
}

/// Outcome of [`select_header_variant`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
//fusa:req REQ-HVSEL-001
pub enum HeaderSelection {
    /// The header was decoded and is to be processed normally.
    Accept(HeaderVariant),
    /// A TSCF header arrived at a time-sync-incapable server configured
    /// with [`TscfFallback::Ignore`]: the header was decoded (unmodified,
    /// so E2E CRC coverage still sees the real wire bits), but the request
    /// must be processed as if no presentation time were included (TC18
    /// §13.3; c-RCP leaves this to the caller the same way,
    /// `c-RCP/include/rcp/avtp.h:345-350`).
    IgnorePresentationTime(TscfHeader),
    /// The AVTPDU must be dropped silently — no response is sent (TC18
    /// §11.1: "AVTPDUs having a TSCF header are dropped, and no response
    /// send"). The header body is not decoded.
    Drop,
}

/// Decode an AVTPDU header, applying the header-variant selection rule.
///
/// NTSCF-subtyped input is always decoded, regardless of `time_sync`. A
/// TSCF-subtyped input is decoded when `time_sync` is
/// [`TimeSyncCapability::Capable`]. When the server is
/// [`TimeSyncCapability::Incapable`], [`should_drop_tscf`] decides:
/// under [`TscfFallback::Drop`] the result is [`HeaderSelection::Drop`]
/// without decoding the rest of the header (no error, so no response is
/// generated); under [`TscfFallback::Ignore`] the header is decoded and
/// returned as [`HeaderSelection::IgnorePresentationTime`].
///
/// Never panics: returns `Err(RcpError::ShortFrame)` for input too short to
/// contain even the leading subtype byte, and `Err(RcpError::Other(_))` for
/// a subtype that is neither [`NTSCF_SUBTYPE`] nor [`TSCF_SUBTYPE`]. All
/// other decode-rejection paths (short frame past the subtype byte, sv bit,
/// ...) are delegated to [`decode_ntscf_header`]/[`decode_tscf_header`].
/// TC18 §12.8.2 "Reception of frames" (TC18.txt line 3567) states the
/// discard rule this function's `other =>` arm implements: "If neither a
/// gPTP nor an IEEE1722 frame is found the received frame shall be
/// discarded." Only the two subtypes RCP itself defines — NTSCF
/// ([`NTSCF_SUBTYPE`], 0x82) and TSCF ([`TSCF_SUBTYPE`], 0x05) — are
/// accepted; every other leading subtype octet is rejected without any
/// attempt to interpret the bytes that follow it.
//fusa:req REQ-HVSEL-002
//fusa:req REQ-HVSEL-003
//fusa:req REQ-HVSEL-004
//fusa:req REQ-HVSEL-005
//fusa:req REQ-NET-002
//fusa:req REQ-AVTP-021
pub fn select_header_variant(
    b: &[u8],
    time_sync: TimeSyncCapability,
    unsupported_time_sync_policy: TscfFallback,
) -> Result<HeaderSelection, RcpError> {
    let subtype = peek_subtype(b)?;
    match subtype {
        NTSCF_SUBTYPE => decode_ntscf_header(b).map(|h| HeaderSelection::Accept(HeaderVariant::Ntscf(h))),
        TSCF_SUBTYPE => {
            if should_drop_tscf(time_sync.accepts_tscf(), subtype, unsupported_time_sync_policy) {
                return Ok(HeaderSelection::Drop);
            }
            let hdr = decode_tscf_header(b)?;
            if time_sync.accepts_tscf() {
                Ok(HeaderSelection::Accept(HeaderVariant::Tscf(hdr)))
            } else {
                Ok(HeaderSelection::IgnorePresentationTime(hdr))
            }
        }
        other => Err(RcpError::Other(format!(
            "avtp: unrecognized subtype 0x{other:02X} (expected NTSCF 0x{NTSCF_SUBTYPE:02X} or TSCF 0x{TSCF_SUBTYPE:02X})"
        ))),
    }
}

// ── Addressing: stream_id construction/parsing ───────────────────────────────

/// A decomposed AVTP `stream_id`: a sender MAC address plus a
/// locally-assigned unique-id suffix.
///
/// [`NtscfHeader::stream_id`] and [`TscfHeader::stream_id`] both carry
/// `stream_id` as a plain opaque `u64` — this type is the typed view onto
/// that same 64 bits, produced by [`StreamId::from_u64`]/[`parse_stream_id`]
/// and turned back into the wire value by [`StreamId::to_u64`]/
/// [`build_stream_id`]. See the module's provenance note for the bit-layout
/// caveat that applies to both directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
//fusa:req REQ-SID-001
pub struct StreamId {
    /// The stream's sender's 48-bit MAC address, occupying the upper 48
    /// bits of the wire `stream_id` (see the module's provenance note).
    pub sender_mac: [u8; 6],
    /// A suffix the sender assigns locally to distinguish multiple
    /// concurrent streams it originates, occupying the lower 16 bits of the
    /// wire `stream_id`.
    pub unique_id: u16,
}

impl StreamId {
    /// Compose a [`StreamId`] from its sender-MAC and unique-id parts.
    //fusa:req REQ-SID-001
    pub fn new(sender_mac: [u8; 6], unique_id: u16) -> Self {
        Self {
            sender_mac,
            unique_id,
        }
    }

    /// Compose the opaque 64-bit wire `stream_id` value from this
    /// [`StreamId`]'s parts. Equivalent to [`build_stream_id`].
    //fusa:req REQ-SID-001
    pub fn to_u64(self) -> u64 {
        build_stream_id(self.sender_mac, self.unique_id)
    }

    /// Decompose an opaque 64-bit wire `stream_id` value into a
    /// [`StreamId`]. Equivalent to [`parse_stream_id`]. Infallible: every
    /// `u64` value — including all-zero and all-`0xFF` — maps to exactly
    /// one [`StreamId`].
    //fusa:req REQ-SID-001
    pub fn from_u64(raw: u64) -> Self {
        let (sender_mac, unique_id) = parse_stream_id(raw);
        Self::new(sender_mac, unique_id)
    }
}

impl From<StreamId> for u64 {
    fn from(id: StreamId) -> u64 {
        id.to_u64()
    }
}

impl From<u64> for StreamId {
    fn from(raw: u64) -> StreamId {
        StreamId::from_u64(raw)
    }
}

/// Compose an opaque 64-bit AVTP `stream_id` from a sender MAC address and a
/// locally-assigned unique-id suffix, per this module's stream_id bit-layout
/// convention (see the provenance note above): `sender_mac` occupies the
/// upper 48 bits, `unique_id` the lower 16.
///
/// Infallible — every `([u8; 6], u16)` pair maps to exactly one `u64`.
//fusa:req REQ-SID-001
pub fn build_stream_id(sender_mac: [u8; 6], unique_id: u16) -> u64 {
    let mac_bits = (u64::from(sender_mac[0]) << 56)
        | (u64::from(sender_mac[1]) << 48)
        | (u64::from(sender_mac[2]) << 40)
        | (u64::from(sender_mac[3]) << 32)
        | (u64::from(sender_mac[4]) << 24)
        | (u64::from(sender_mac[5]) << 16);
    mac_bits | u64::from(unique_id)
}

/// Decompose an opaque 64-bit AVTP `stream_id` into a sender MAC address and
/// a locally-assigned unique-id suffix — the inverse of [`build_stream_id`].
///
/// Never panics: a `u64` always carries exactly the 64 bits this function
/// reads, so there is no truncated-input case to reject the way the header
/// decoders above do.
//fusa:req REQ-SID-001
pub fn parse_stream_id(raw: u64) -> ([u8; 6], u16) {
    let sender_mac = [
        (raw >> 56) as u8,
        (raw >> 48) as u8,
        (raw >> 40) as u8,
        (raw >> 32) as u8,
        (raw >> 24) as u8,
        (raw >> 16) as u8,
    ];
    let unique_id = raw as u16;
    (sender_mac, unique_id)
}

// ── Frame composition (ROADMAP.md Milestone 9, `wire` REPLACE cutover) ───────

/// Combine an [`NtscfHeader`] (built from `stream_id`/`sequence_num`) with
/// an already-encoded ACF payload into one on-wire AVTPDU frame, ready to
/// hand to a transport.
///
/// `acf_payload` is opaque to this function — it is typically the output of
/// [`crate::acf::encode_acf_abb`] or [`crate::acf::encode_acf_gbb`], but
/// this function does not require that; it only measures the payload's
/// length to populate `ntscf_data_length`. Returns
/// `Err(RcpError::InvalidSize)` if that length exceeds
/// [`NTSCF_DATA_LENGTH_MAX`] (the same 11-bit field width
/// [`encode_ntscf_header`] itself enforces).
/// This is the frame composition entry point an RC Server uses, and it
/// always emits an NTSCF header — matching TC18 §11.1 (TC18.txt lines
/// 1130-1131: the NTSCF header "shall be used for RCP requests which shall
/// be executed as soon as possible or under request specific conditions as
/// well as for responses and acknowledge") and §11.4.3 (TC18.txt line
/// 2354: "The RC Server always uses NTSCF header"). [`encode_tscf_frame`]
/// exists for an RC Client's own TSCF-headed requests only.
//fusa:req REQ-WIRE-001
//fusa:req REQ-WIRE-002
//fusa:req REQ-WIRE-004
//fusa:req REQ-WIRE-007
//fusa:req REQ-NTSCF-007
//fusa:req REQ-AVTP-003
pub fn encode_ntscf_frame(
    stream_id: StreamId,
    sequence_num: u8,
    acf_payload: &[u8],
) -> Result<Vec<u8>, RcpError> {
    let ntscf_data_length = u16::try_from(acf_payload.len()).map_err(|_| RcpError::InvalidSize)?;
    let hdr = NtscfHeader {
        sequence_num,
        ntscf_data_length,
        stream_id: stream_id.to_u64(),
    };
    let hdr_bytes = encode_ntscf_header(&hdr)?;
    let mut buf = Vec::with_capacity(hdr_bytes.len() + acf_payload.len());
    buf.extend_from_slice(&hdr_bytes);
    buf.extend_from_slice(acf_payload);
    Ok(buf)
}

/// Split a decoded [`NtscfHeader`] from its ACF payload bytes.
///
/// Mirrors `rcp_avtp_decode_ntscf` (`c-RCP/src/avtp.c:148-168`): the
/// returned payload is exactly `ntscf_data_length` bytes. A buffer shorter
/// than header + `ntscf_data_length` is rejected with
/// `Err(RcpError::ShortFrame)`; bytes beyond `ntscf_data_length` (e.g.
/// Ethernet minimum-frame padding on L2) are ignored rather than handed to
/// the ACF decoder.
///
/// The returned payload slice is handed unparsed to whichever ACF decoder
/// the caller expects ([`crate::acf::decode_acf_abb`]/
/// [`crate::acf::decode_acf_gbb`]) — this function does not attempt to
/// distinguish between them. Never panics on short, truncated, or arbitrary
/// input: delegates directly to [`decode_ntscf_header`] for the header
/// checks.
//fusa:req REQ-WIRE-003
//fusa:req REQ-WIRE-005
//fusa:req REQ-WIRE-008
//fusa:req REQ-WIRE-009
//fusa:req REQ-AVTP-009
pub fn decode_ntscf_frame(b: &[u8]) -> Result<(NtscfHeader, &[u8]), RcpError> {
    let hdr = decode_ntscf_header(b)?;
    let end = NTSCF_HEADER_LEN + usize::from(hdr.ntscf_data_length);
    if b.len() < end {
        return Err(RcpError::ShortFrame);
    }
    Ok((hdr, &b[NTSCF_HEADER_LEN..end]))
}

/// Combine a [`TscfHeader`] with an already-encoded ACF payload into one
/// TSCF-headed AVTPDU — an RC Client's presentation-time request frame.
///
/// Port of `rcp_avtp_encode_tscf` (`c-RCP/src/avtp.c:175-207`):
/// `hdr.stream_data_length` is ignored and recomputed from
/// `acf_payload.len()`; every other field is encoded as
/// [`encode_tscf_header`] does. Returns `Err(RcpError::InvalidSize)` if the
/// payload exceeds [`TSCF_DATA_LENGTH_MAX`] (c-RCP returns an empty
/// buffer for the same case).
//fusa:req REQ-AVTP-002
//fusa:req REQ-AVTP-004
pub fn encode_tscf_frame(hdr: &TscfHeader, acf_payload: &[u8]) -> Result<Vec<u8>, RcpError> {
    let stream_data_length = u16::try_from(acf_payload.len()).map_err(|_| RcpError::InvalidSize)?;
    let hdr_bytes = encode_tscf_header(&TscfHeader {
        stream_data_length,
        ..*hdr
    })?;
    let mut buf = Vec::with_capacity(hdr_bytes.len() + acf_payload.len());
    buf.extend_from_slice(&hdr_bytes);
    buf.extend_from_slice(acf_payload);
    Ok(buf)
}

/// Split a decoded [`TscfHeader`] from its ACF payload bytes.
///
/// Port of `rcp_avtp_decode_tscf` (`c-RCP/src/avtp.c:216-245`): the
/// returned payload is exactly `stream_data_length` bytes; a buffer shorter
/// than header + `stream_data_length` is `Err(RcpError::ShortFrame)` and
/// trailing bytes beyond it are ignored. Never panics.
//fusa:req REQ-AVTP-004
//fusa:req REQ-AVTP-006
//fusa:req REQ-AVTP-008
//fusa:req REQ-AVTP-024
pub fn decode_tscf_frame(b: &[u8]) -> Result<(TscfHeader, &[u8]), RcpError> {
    let hdr = decode_tscf_header(b)?;
    let end = TSCF_HEADER_LEN + usize::from(hdr.stream_data_length);
    if b.len() < end {
        return Err(RcpError::ShortFrame);
    }
    Ok((hdr, &b[TSCF_HEADER_LEN..end]))
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    // ── Round-trip ─────────────────────────────────────────────────────────

    #[test]
    //fusa:test REQ-NTSCF-001
    //fusa:test REQ-NTSCF-002
    //fusa:test REQ-NTSCF-003
    fn ntscf_header_round_trip() {
        let hdr = NtscfHeader {
            sequence_num: 0x42,
            ntscf_data_length: 0x0355,
            stream_id: 0x0011_2233_4455_6677,
        };
        let frame = encode_ntscf_header(&hdr).unwrap();
        assert_eq!(frame.len(), NTSCF_HEADER_LEN);
        let decoded = decode_ntscf_header(&frame).unwrap();
        assert_eq!(decoded, hdr);
    }

    #[test]
    //fusa:test REQ-NTSCF-002
    fn ntscf_header_round_trip_zero_values() {
        let hdr = NtscfHeader {
            sequence_num: 0,
            ntscf_data_length: 0,
            stream_id: 0,
        };
        let frame = encode_ntscf_header(&hdr).unwrap();
        let decoded = decode_ntscf_header(&frame).unwrap();
        assert_eq!(decoded, hdr);
    }

    #[test]
    //fusa:test REQ-NTSCF-002
    fn ntscf_header_round_trip_max_values() {
        let hdr = NtscfHeader {
            sequence_num: 0xFF,
            ntscf_data_length: NTSCF_DATA_LENGTH_MAX,
            stream_id: u64::MAX,
        };
        let frame = encode_ntscf_header(&hdr).unwrap();
        let decoded = decode_ntscf_header(&frame).unwrap();
        assert_eq!(decoded, hdr);
    }

    #[test]
    //fusa:test REQ-NTSCF-002
    fn encode_rejects_oversized_data_length() {
        let hdr = NtscfHeader {
            ntscf_data_length: NTSCF_DATA_LENGTH_MAX + 1,
            ..Default::default()
        };
        assert_eq!(encode_ntscf_header(&hdr), Err(RcpError::InvalidSize));
    }

    #[test]
    //fusa:test REQ-NTSCF-002
    fn encoded_header_has_expected_subtype_and_sv_bit() {
        let frame = encode_ntscf_header(&NtscfHeader::default()).unwrap();
        assert_eq!(frame[0], NTSCF_SUBTYPE);
        assert_eq!(frame[1] & 0x80, 0x80, "sv bit must be set for NTSCF");
    }

    // ── Decode rejection ──────────────────────────────────────────────────

    #[test]
    //fusa:test REQ-NTSCF-004
    fn decode_rejects_short_input() {
        for len in 0..NTSCF_HEADER_LEN {
            let buf = vec![0u8; len];
            assert_eq!(decode_ntscf_header(&buf), Err(RcpError::ShortFrame));
        }
    }

    #[test]
    //fusa:test REQ-NTSCF-004
    fn decode_rejects_wrong_subtype() {
        let mut frame = encode_ntscf_header(&NtscfHeader::default()).unwrap();
        frame[0] = TSCF_SUBTYPE; // the other RCP subtype, not NTSCF
        assert!(matches!(
            decode_ntscf_header(&frame),
            Err(RcpError::Other(_))
        ));
    }

    #[test]
    //fusa:test REQ-NTSCF-004
    fn decode_rejects_sv_bit_unset() {
        let mut frame = encode_ntscf_header(&NtscfHeader::default()).unwrap();
        frame[1] &= 0x7F; // clear sv
        assert!(matches!(
            decode_ntscf_header(&frame),
            Err(RcpError::Other(_))
        ));
    }

    #[test]
    //fusa:test REQ-NTSCF-004
    fn decode_ignores_reserved_bits() {
        let hdr = NtscfHeader {
            sequence_num: 7,
            ntscf_data_length: 0x123,
            stream_id: 0xABCD,
        };
        let mut frame = encode_ntscf_header(&hdr).unwrap();
        // Per TC18 v0.5.1_RC §11.1 Figure 6 (p.22), the only bits of this
        // header this crate does not model are `version` (bits 9-11) and
        // `r` (bit 12) — octet 1's `0x78` mask. Every other bit is a named
        // field, so there is nothing else to scribble over: unlike the
        // pre-v4.0.0 layout, there is no reserved quadlet after the first
        // one. Setting them must not disturb any decoded field.
        frame[1] |= 0x78;
        let decoded = decode_ntscf_header(&frame).unwrap();
        assert_eq!(decoded, hdr);
    }

    // ── Fuzz-style: arbitrary bytes never panic ───────────────────────────

    #[test]
    //fusa:test REQ-NTSCF-005
    //fusa:test REQ-NTSCF-006
    fn decode_never_panics_on_arbitrary_input() {
        let inputs: &[&[u8]] = &[
            &[],
            &[0x82],
            &[0x82, 0x80],
            &[0xFF; 16],
            &[0x00; 16],
            &[
                0x82, 0x80, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                0xFF,
            ],
            &[0x82; 32],
        ];
        for input in inputs {
            let _ = decode_ntscf_header(input);
        }
    }

    #[test]
    //fusa:test REQ-NTSCF-006
    fn decode_never_panics_on_random_lengths() {
        // Deterministic pseudo-random coverage across many lengths/contents,
        // matching wire.rs's fuzz-style discipline without adding a
        // dedicated cargo-fuzz target for this early a milestone item.
        let mut state: u32 = 0x1234_5678;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for len in 0..40 {
            let buf: Vec<u8> = (0..len).map(|_| (next() & 0xFF) as u8).collect();
            let _ = decode_ntscf_header(&buf);
        }
    }

    // ═══════════════════════════════════════════════════════════════════
    //  TscfHeader
    // ═══════════════════════════════════════════════════════════════════

    // ── Round-trip ─────────────────────────────────────────────────────────

    fn sample_tscf_header() -> TscfHeader {
        TscfHeader {
            version: 0,
            mr: true,
            tv: true,
            sequence_num: 0x42,
            tu: true,
            avtp_timestamp: 0x1122_3344,
            stream_data_length: 0x0355,
            stream_id: 0x0011_2233_4455_6677,
            reserved0: 0,
            reserved1: 0,
        }
    }

    #[test]
    //fusa:test REQ-TSCF-001
    //fusa:test REQ-TSCF-002
    //fusa:test REQ-TSCF-003
    //fusa:test REQ-AVTP-004
    fn tscf_header_round_trip() {
        let hdr = sample_tscf_header();
        let frame = encode_tscf_header(&hdr).unwrap();
        assert_eq!(frame.len(), TSCF_HEADER_LEN);
        let decoded = decode_tscf_header(&frame).unwrap();
        assert_eq!(decoded, hdr);
    }

    #[test]
    //fusa:test REQ-TSCF-002
    //fusa:test REQ-AVTP-004
    fn tscf_header_round_trips_every_flag_combination() {
        // version/mr/tv/tu are independent wire bits, each carried through
        // unmodified (REQ-AVTP-004: sv, version, mr, tv, tu, sequence_num,
        // stream_id, avtp_timestamp recovered unchanged).
        for version in 0u8..=7 {
            for bits in 0u8..8 {
                let hdr = TscfHeader {
                    version,
                    mr: bits & 1 != 0,
                    tv: bits & 2 != 0,
                    tu: bits & 4 != 0,
                    ..sample_tscf_header()
                };
                let decoded = decode_tscf_header(&encode_tscf_header(&hdr).unwrap()).unwrap();
                assert_eq!(decoded, hdr, "version={version} bits={bits:03b}");
            }
        }
    }

    #[test]
    //fusa:test REQ-TSCF-002
    fn tscf_header_round_trip_zero_values() {
        let hdr = TscfHeader::default();
        let frame = encode_tscf_header(&hdr).unwrap();
        let decoded = decode_tscf_header(&frame).unwrap();
        assert_eq!(decoded, hdr);
    }

    #[test]
    //fusa:test REQ-TSCF-002
    fn tscf_header_round_trip_max_values() {
        let hdr = TscfHeader {
            version: 7,
            mr: true,
            tv: true,
            sequence_num: 0xFF,
            tu: true,
            avtp_timestamp: u32::MAX,
            stream_data_length: TSCF_DATA_LENGTH_MAX,
            stream_id: u64::MAX,
            reserved0: 0,
            reserved1: 0,
        };
        let frame = encode_tscf_header(&hdr).unwrap();
        let decoded = decode_tscf_header(&frame).unwrap();
        assert_eq!(decoded, hdr);
    }

    #[test]
    //fusa:test REQ-TSCF-002
    fn tscf_accepts_the_full_16_bit_data_length_range() {
        // TC18 v0.5.1_RC §11.1 Figure 5 (p.22) gives `stream_data_length`
        // its own 16-bit half-quadlet, so — unlike NTSCF's 11-bit
        // `ntscf_data_length` — there is no `u16` this field cannot carry
        // and no oversize-rejection path to exercise.
        assert_eq!(TSCF_DATA_LENGTH_MAX, u16::MAX);
        for len in [0u16, 1, 0x07FF, 0x0800, 0xFFFE, u16::MAX] {
            let hdr = TscfHeader {
                stream_data_length: len,
                ..Default::default()
            };
            let frame = encode_tscf_header(&hdr).expect("every u16 is in range");
            assert_eq!(decode_tscf_header(&frame).unwrap().stream_data_length, len);
        }
    }

    #[test]
    //fusa:test REQ-TSCF-002
    fn tscf_encoded_header_has_expected_subtype_and_sv_bit() {
        let frame = encode_tscf_header(&TscfHeader::default()).unwrap();
        assert_eq!(frame[0], TSCF_SUBTYPE);
        assert_eq!(frame[1] & 0x80, 0x80, "sv bit must be set for TSCF");
    }

    // ── Wire layout, pinned to the specification's worked examples ────────

    #[test]
    //fusa:test REQ-NTSCF-003
    fn ntscf_header_matches_figure_20_worked_example() {
        // TC18 v0.5.1_RC page 79, Figure 20 ("Single ACF_GBB CRC32
        // mandatory fields"). Its AVTPDU header row reads, left to right:
        //   subtype(0x82) | sv | version(0x0) | r |
        //   ntscf_data_length=0x038 | sequence_num_lsb
        // followed by a `stream_id` row spanning two quadlets. The example
        // does not fix a `sequence_num` or `stream_id` value, so only the
        // first three octets are pinned from it; the fourth is this
        // test's own input.
        let hdr = NtscfHeader {
            ntscf_data_length: 0x038,
            sequence_num: 0x5A,
            stream_id: 0x1122_3344_5566_7788,
        };
        let frame = encode_ntscf_header(&hdr).unwrap();

        assert_eq!(frame[0], 0x82, "subtype(0x82), Figure 20 bits 0-7");
        // sv=1 (bit 8), version=000 (9-11), r=0 (12),
        // ntscf_data_length[10:8] = 0x038 >> 8 = 0 (bits 13-15).
        assert_eq!(frame[1], 0x80, "Figure 20 bits 8-15");
        assert_eq!(
            frame[2], 0x38,
            "ntscf_data_length[7:0], Figure 20 bits 16-23"
        );
        assert_eq!(frame[3], 0x5A, "sequence_num, Figure 20 bits 24-31");
        assert_eq!(
            &frame[4..12],
            &0x1122_3344_5566_7788u64.to_be_bytes(),
            "stream_id occupies quadlets 1-2 with no reserved gap before it"
        );
        assert_eq!(frame.len(), 12, "Figure 20's header is three quadlets");
    }

    #[test]
    //fusa:test REQ-TSCF-003
    fn tscf_header_matches_figure_19_worked_example() {
        // TC18 v0.5.1_RC page 79, Figure 19 ("single ACF_ABB CRC32
        // mandatory fields"). Its AVTPDU header rows read:
        //   subtype(0x05) | sv | version(0x0) | mr | rsv | tv |
        //   sequence_num_lsb | reserved | tu
        //   stream_id (2 quadlets) / avtp_timestamp / reserved
        //   stream_data_length(octets) = 0x003C | reserved
        let hdr = TscfHeader {
            tv: true,
            stream_data_length: 0x003C,
            sequence_num: 0x5A,
            avtp_timestamp: 0x1234_5678,
            stream_id: 0x1122_3344_5566_7788,
            ..Default::default()
        };
        let frame = encode_tscf_header(&hdr).unwrap();

        assert_eq!(frame[0], 0x05, "subtype(0x05), Figure 19 bits 0-7");
        // sv=1 (bit 8), version=000 (9-11), mr=0 (12), rsv=00 (13-14),
        // tv=1 (15).
        assert_eq!(frame[1], 0x81, "Figure 19 bits 8-15");
        assert_eq!(frame[2], 0x5A, "sequence_num, Figure 19 bits 16-23");
        assert_eq!(frame[3], 0x00, "reserved(7)|tu, Figure 19 bits 24-31");
        assert_eq!(&frame[4..12], &0x1122_3344_5566_7788u64.to_be_bytes());
        assert_eq!(&frame[12..16], &0x1234_5678u32.to_be_bytes());
        assert_eq!(&frame[16..20], &[0u8; 4], "Figure 19's reserved quadlet");
        assert_eq!(
            &frame[20..22],
            &[0x00, 0x3C],
            "stream_data_length(octets) = 0x003C, Figure 19's sixth quadlet"
        );
        assert_eq!(&frame[22..24], &[0u8; 2], "Figure 19's trailing reserved");
        assert_eq!(frame.len(), 24, "Figure 19's header is six quadlets");
    }

    #[test]
    //fusa:test REQ-TSCF-003
    //fusa:test REQ-AVTP-004
    fn tscf_tv_bit_is_explicit_not_inferred_from_the_timestamp_value() {
        // TC18 v0.5.1_RC §11.1 Figure 5 (p.22) bit 15, `tv`. A valid
        // timestamp of 0 must be sendable (tv=1, ts=0) and a non-zero value
        // may be sent marked invalid (tv=0).
        let timed_zero = encode_tscf_header(&TscfHeader {
            tv: true,
            avtp_timestamp: 0,
            ..Default::default()
        })
        .unwrap();
        assert_eq!(timed_zero[1] & 0x01, 1, "tv=1 with a zero timestamp");
        assert!(decode_tscf_header(&timed_zero).unwrap().tv);

        let untimed_nonzero = encode_tscf_header(&TscfHeader {
            tv: false,
            avtp_timestamp: 1,
            ..Default::default()
        })
        .unwrap();
        assert_eq!(
            untimed_nonzero[1] & 0x01,
            0,
            "tv=0 with a non-zero timestamp"
        );
        assert!(!decode_tscf_header(&untimed_nonzero).unwrap().tv);
    }

    #[test]
    //fusa:test REQ-TSCF-002
    fn tscf_and_ntscf_headers_use_distinct_subtypes() {
        assert_ne!(TSCF_SUBTYPE, NTSCF_SUBTYPE);
    }

    // ── c-RCP golden vectors (generated by c-RCP's own avtp.c) ──────────────

    /// `rcp_avtp_encode_tscf` output for sv=1 version=0 mr=1 tv=1 tu=1
    /// seq=0x5A stream_id=02:42:AC:11:00:03/0x0008 avtp_timestamp=0
    /// reserved0=0xFFFFFFFF reserved1=0xFFFF, payload DE AD BE EF 01.
    /// Generated by linking c-RCP's real `src/avtp.c` (rcp_avtp_encode_tscf,
    /// c-RCP/src/avtp.c:175-207) into a small harness.
    const C_RCP_TSCF_MR_TV_TU_TS0: [u8; 29] = [
        0x05, 0x89, 0x5A, 0x01, 0x02, 0x42, 0xAC, 0x11, 0x00, 0x03, 0x00, 0x08, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0xDE, 0xAD, 0xBE, 0xEF, 0x01,
    ];

    /// `rcp_avtp_encode_tscf` output for version=5 mr=0 tv=0 tu=0 seq=0x01
    /// avtp_timestamp=0x12345678, empty payload (same harness).
    const C_RCP_TSCF_V5_UNTIMED: [u8; 24] = [
        0x05, 0xD0, 0x01, 0x00, 0x02, 0x42, 0xAC, 0x11, 0x00, 0x03, 0x00, 0x08, 0x12, 0x34, 0x56,
        0x78, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    /// `rcp_avtp_encode_ntscf` output for seq=7
    /// stream_id=02:42:AC:11:00:03/0x0042, payload DE AD BE EF 01.
    const C_RCP_NTSCF_5: [u8; 17] = [
        0x82, 0x80, 0x05, 0x07, 0x02, 0x42, 0xAC, 0x11, 0x00, 0x03, 0x00, 0x42, 0xDE, 0xAD, 0xBE,
        0xEF, 0x01,
    ];

    const C_RCP_MAC: [u8; 6] = [0x02, 0x42, 0xAC, 0x11, 0x00, 0x03];
    const C_RCP_PAYLOAD: [u8; 5] = [0xDE, 0xAD, 0xBE, 0xEF, 0x01];

    #[test]
    //fusa:test REQ-AVTP-002
    //fusa:test REQ-AVTP-004
    //fusa:test REQ-AVTP-023
    fn tscf_frame_encode_matches_c_rcp_byte_for_byte() {
        let hdr = TscfHeader {
            version: 0,
            mr: true,
            tv: true,
            tu: true,
            sequence_num: 0x5A,
            stream_id: build_stream_id(C_RCP_MAC, 0x0008),
            avtp_timestamp: 0,
            // c-RCP parity: reserved fields are zero-filled on encode.
            reserved0: 0xFFFF_FFFF,
            reserved1: 0xFFFF,
            // ignored: recomputed from the payload, as c-RCP.
            stream_data_length: 0x1234,
        };
        assert_eq!(
            encode_tscf_frame(&hdr, &C_RCP_PAYLOAD).unwrap(),
            C_RCP_TSCF_MR_TV_TU_TS0
        );

        let untimed = TscfHeader {
            version: 5,
            sequence_num: 0x01,
            stream_id: build_stream_id(C_RCP_MAC, 0x0008),
            avtp_timestamp: 0x1234_5678,
            ..Default::default()
        };
        assert_eq!(
            encode_tscf_frame(&untimed, &[]).unwrap(),
            C_RCP_TSCF_V5_UNTIMED
        );

        // And the decode direction recovers every bit c-RCP encoded.
        let (d, payload) = decode_tscf_frame(&C_RCP_TSCF_MR_TV_TU_TS0).unwrap();
        assert!(d.mr && d.tv && d.tu);
        assert_eq!(d.version, 0);
        assert_eq!(d.avtp_timestamp, 0);
        assert_eq!(d.stream_data_length, 5);
        assert_eq!(payload, &C_RCP_PAYLOAD);
        let (d, payload) = decode_tscf_frame(&C_RCP_TSCF_V5_UNTIMED).unwrap();
        assert_eq!(d.version, 5);
        assert!(!d.mr && !d.tv && !d.tu);
        assert!(payload.is_empty());
    }

    #[test]
    //fusa:test REQ-AVTP-003
    fn ntscf_frame_encode_matches_c_rcp_byte_for_byte() {
        let frame =
            encode_ntscf_frame(StreamId::new(C_RCP_MAC, 0x0042), 7, &C_RCP_PAYLOAD).unwrap();
        assert_eq!(frame, C_RCP_NTSCF_5);
        let (hdr, payload) = decode_ntscf_frame(&C_RCP_NTSCF_5).unwrap();
        assert_eq!(hdr.sequence_num, 7);
        assert_eq!(payload, &C_RCP_PAYLOAD);
    }

    // ── Decode rejection ──────────────────────────────────────────────────

    #[test]
    //fusa:test REQ-TSCF-004
    //fusa:test REQ-AVTP-006
    fn tscf_decode_rejects_short_input() {
        for len in 0..TSCF_HEADER_LEN {
            let buf = vec![0u8; len];
            assert_eq!(decode_tscf_header(&buf), Err(RcpError::ShortFrame));
            assert_eq!(decode_tscf_frame(&buf), Err(RcpError::ShortFrame));
        }
    }

    #[test]
    //fusa:test REQ-TSCF-004
    //fusa:test REQ-AVTP-008
    fn tscf_decode_rejects_wrong_subtype() {
        let mut frame = encode_tscf_header(&TscfHeader::default()).unwrap();
        frame[0] = NTSCF_SUBTYPE; // NTSCF subtype, not TSCF
        assert!(matches!(
            decode_tscf_header(&frame),
            Err(RcpError::Other(_))
        ));
        assert!(matches!(decode_tscf_frame(&frame), Err(RcpError::Other(_))));
    }

    #[test]
    //fusa:test REQ-TSCF-004
    fn tscf_decode_rejects_sv_bit_unset() {
        let mut frame = encode_tscf_header(&TscfHeader::default()).unwrap();
        frame[1] &= 0x7F; // clear sv
        assert!(matches!(
            decode_tscf_header(&frame),
            Err(RcpError::Other(_))
        ));
    }

    #[test]
    //fusa:test REQ-TSCF-004
    //fusa:test REQ-AVTP-022
    //fusa:test REQ-AVTP-023
    fn tscf_decode_surfaces_every_figure_5_field() {
        // Same 26-byte buffer fed to c-RCP's rcp_avtp_decode_tscf in the
        // golden harness: it reported version=7 mr=1 tv=1 tu=1 seq=0x33
        // reserved0=0xA1B2C3D4 reserved1=0xE5F6 payload_len=2.
        let mut buf = [0u8; TSCF_HEADER_LEN + 2];
        buf[0] = 0x05;
        buf[1] = 0xFF;
        buf[2] = 0x33;
        buf[3] = 0xFF;
        buf[16..20].copy_from_slice(&[0xA1, 0xB2, 0xC3, 0xD4]);
        buf[20..22].copy_from_slice(&[0x00, 0x02]);
        buf[22..24].copy_from_slice(&[0xE5, 0xF6]);
        let (hdr, payload) = decode_tscf_frame(&buf).unwrap();
        assert_eq!(hdr.version, 7);
        assert!(hdr.mr && hdr.tv && hdr.tu);
        assert_eq!(hdr.sequence_num, 0x33);
        assert_eq!(hdr.reserved0, 0xA1B2_C3D4);
        assert_eq!(hdr.reserved1, 0xE5F6);
        assert_eq!(payload.len(), 2);
        assert!(!tscf_reserved_all_zero(&hdr));
        // tu decoded faithfully for both values (REQ-AVTP-023).
        buf[3] = 0xFE;
        assert!(!decode_tscf_header(&buf).unwrap().tu);
    }

    // ── Fuzz-style: arbitrary bytes never panic ───────────────────────────

    #[test]
    //fusa:test REQ-TSCF-005
    //fusa:test REQ-TSCF-006
    fn tscf_decode_never_panics_on_arbitrary_input() {
        let inputs: &[&[u8]] = &[
            &[],
            &[0x05],
            &[0x05, 0x80],
            &[0xFF; 24],
            &[0x00; 24],
            &[
                0x05, 0x80, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            ],
            &[0x05; 40],
            // The pre-v4.0.0 placeholder subtype: no longer recognized,
            // and must be rejected rather than panicked on.
            &[0x83; 40],
        ];
        for input in inputs {
            let _ = decode_tscf_header(input);
            let _ = decode_tscf_frame(input);
        }
    }

    #[test]
    //fusa:test REQ-TSCF-006
    fn tscf_decode_never_panics_on_random_lengths() {
        // Deterministic pseudo-random coverage across many lengths/contents,
        // matching wire.rs's fuzz-style discipline without adding a
        // dedicated cargo-fuzz target for this early a milestone item.
        let mut state: u32 = 0x8765_4321;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for len in 0..60 {
            let mut buf: Vec<u8> = (0..len).map(|_| (next() & 0xFF) as u8).collect();
            let _ = decode_tscf_header(&buf);
            if let Some(b0) = buf.first_mut() {
                *b0 = TSCF_SUBTYPE;
            }
            if let Some(b1) = buf.get_mut(1) {
                *b1 |= 0x80;
            }
            let _ = decode_tscf_frame(&buf);
        }
    }

    // ═══════════════════════════════════════════════════════════════════
    //  Header-variant selection/rejection
    // ═══════════════════════════════════════════════════════════════════

    fn sample_tscf_for_selection() -> TscfHeader {
        TscfHeader {
            tv: true,
            sequence_num: 9,
            avtp_timestamp: 0x1234_5678,
            stream_data_length: 40,
            stream_id: 0x0011_2233_4455_6677,
            ..Default::default()
        }
    }

    #[test]
    //fusa:test REQ-HVSEL-001
    fn time_sync_capability_accepts_tscf_only_when_capable() {
        assert!(TimeSyncCapability::Capable.accepts_tscf());
        assert!(!TimeSyncCapability::Incapable.accepts_tscf());
    }

    #[test]
    //fusa:test REQ-HVSEL-004
    fn select_header_variant_accepts_ntscf_regardless_of_time_sync_or_policy() {
        // NTSCF carries no timing assumption: it is accepted regardless of
        // the receiving server's time-sync capability or TSCF policy.
        let hdr = NtscfHeader {
            sequence_num: 3,
            ntscf_data_length: 12,
            stream_id: 0xAABB_CCDD,
        };
        let frame = encode_ntscf_header(&hdr).unwrap();
        for ts in [TimeSyncCapability::Capable, TimeSyncCapability::Incapable] {
            for policy in [TscfFallback::Drop, TscfFallback::Ignore] {
                assert_eq!(
                    select_header_variant(&frame, ts, policy).unwrap(),
                    HeaderSelection::Accept(HeaderVariant::Ntscf(hdr))
                );
            }
        }
    }

    #[test]
    //fusa:test REQ-HVSEL-003
    fn select_header_variant_accepts_tscf_when_time_sync_capable() {
        let hdr = sample_tscf_for_selection();
        let frame = encode_tscf_header(&hdr).unwrap();
        for policy in [TscfFallback::Drop, TscfFallback::Ignore] {
            assert_eq!(
                select_header_variant(&frame, TimeSyncCapability::Capable, policy).unwrap(),
                HeaderSelection::Accept(HeaderVariant::Tscf(hdr))
            );
        }
    }

    #[test]
    //fusa:test REQ-HVSEL-002
    //fusa:test REQ-AVTP-014
    fn select_header_variant_drops_tscf_silently_when_time_sync_incapable() {
        // TC18 §11.1: "AVTPDUs having a TSCF header are dropped, and no
        // response send." Dropping is Ok(Drop), not an Err (an Err would be
        // mapped to an error response code).
        let frame = encode_tscf_header(&sample_tscf_for_selection()).unwrap();
        assert_eq!(
            select_header_variant(&frame, TimeSyncCapability::Incapable, TscfFallback::Drop),
            Ok(HeaderSelection::Drop)
        );
        // The default policy is Drop.
        assert_eq!(
            select_header_variant(
                &frame,
                TimeSyncCapability::Incapable,
                TscfFallback::default()
            ),
            Ok(HeaderSelection::Drop)
        );
    }

    #[test]
    //fusa:test REQ-AVTP-021
    fn select_header_variant_ignores_presentation_time_under_ignore_policy() {
        // TC18 §13.3: "the presentation time shall be ignored, and the
        // request(s) executed as if no presentation time were included or
        // dropped depending on the configuration of the RC Server."
        let hdr = sample_tscf_for_selection();
        let frame = encode_tscf_header(&hdr).unwrap();
        assert_eq!(
            select_header_variant(&frame, TimeSyncCapability::Incapable, TscfFallback::Ignore),
            Ok(HeaderSelection::IgnorePresentationTime(hdr))
        );
    }

    #[test]
    //fusa:test REQ-HVSEL-002
    fn select_header_variant_drops_tscf_before_decoding_body() {
        // Even a TSCF frame that would otherwise fail to decode (too short
        // past the subtype byte) must still be dropped, not reported as a
        // short-frame/decode error, confirming the drop happens before the
        // header body is inspected.
        let short_tscf_looking = [TSCF_SUBTYPE, 0x80];
        assert_eq!(
            select_header_variant(
                &short_tscf_looking,
                TimeSyncCapability::Incapable,
                TscfFallback::Drop
            ),
            Ok(HeaderSelection::Drop)
        );
        // Under Ignore, the body is decoded, so the short frame surfaces.
        assert_eq!(
            select_header_variant(
                &short_tscf_looking,
                TimeSyncCapability::Incapable,
                TscfFallback::Ignore
            ),
            Err(RcpError::ShortFrame)
        );
    }

    #[test]
    //fusa:test REQ-HVSEL-005
    fn select_header_variant_rejects_empty_input() {
        for ts in [TimeSyncCapability::Capable, TimeSyncCapability::Incapable] {
            assert_eq!(
                select_header_variant(&[], ts, TscfFallback::Drop),
                Err(RcpError::ShortFrame)
            );
        }
    }

    #[test]
    //fusa:test REQ-HVSEL-005
    fn select_header_variant_rejects_unrecognized_subtype() {
        let frame = [0x01u8; TSCF_HEADER_LEN];
        assert!(matches!(
            select_header_variant(&frame, TimeSyncCapability::Capable, TscfFallback::Drop),
            Err(RcpError::Other(_))
        ));
    }

    #[test]
    //fusa:test REQ-HVSEL-002
    //fusa:test REQ-HVSEL-003
    fn select_header_variant_propagates_tscf_decode_errors_when_capable() {
        let mut frame = encode_tscf_header(&TscfHeader::default()).unwrap();
        frame[1] &= 0x7F; // clear sv, forcing decode_tscf_header to reject
        assert!(matches!(
            select_header_variant(&frame, TimeSyncCapability::Capable, TscfFallback::Drop),
            Err(RcpError::Other(_))
        ));
    }

    #[test]
    //fusa:test REQ-HVSEL-005
    fn select_header_variant_never_panics_on_arbitrary_input() {
        let mut state: u32 = 0x2468_ACE0;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for len in 0..40 {
            let buf: Vec<u8> = (0..len).map(|_| (next() & 0xFF) as u8).collect();
            for ts in [TimeSyncCapability::Capable, TimeSyncCapability::Incapable] {
                for policy in [TscfFallback::Drop, TscfFallback::Ignore] {
                    let _ = select_header_variant(&buf, ts, policy);
                }
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════════
    //  TSCF policy helpers (c-RCP avtp.c parity)
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    //fusa:test REQ-AVTP-014
    //fusa:test REQ-AVTP-021
    fn should_drop_tscf_matches_c_rcp_truth_table() {
        // Values from the c-RCP golden harness:
        // drop(f,TSCF,DROP)=1 drop(f,TSCF,IGN)=0 drop(t,TSCF,DROP)=0
        // drop(f,NTSCF,DROP)=0.
        assert!(should_drop_tscf(false, TSCF_SUBTYPE, TscfFallback::Drop));
        assert!(!should_drop_tscf(false, TSCF_SUBTYPE, TscfFallback::Ignore));
        assert!(!should_drop_tscf(true, TSCF_SUBTYPE, TscfFallback::Drop));
        assert!(!should_drop_tscf(true, TSCF_SUBTYPE, TscfFallback::Ignore));
        for subtype in (0u8..=0xFF).filter(|&s| s != TSCF_SUBTYPE) {
            for supported in [false, true] {
                for policy in [TscfFallback::Drop, TscfFallback::Ignore] {
                    assert!(!should_drop_tscf(supported, subtype, policy));
                }
            }
        }
        assert_eq!(TscfFallback::Drop as u8, 0);
        assert_eq!(TscfFallback::Ignore as u8, 1);
        assert_eq!(TscfFallback::default(), TscfFallback::Drop);
    }

    #[test]
    //fusa:test REQ-AVTP-031
    fn tscf_reserved_all_zero_requires_both_regions_zero() {
        let zero = TscfHeader::default();
        assert!(tscf_reserved_all_zero(&zero));
        assert!(!tscf_reserved_all_zero(&TscfHeader {
            reserved0: 1,
            ..zero
        }));
        assert!(!tscf_reserved_all_zero(&TscfHeader {
            reserved1: 0x8000,
            ..zero
        }));
        // A conformant encoder always zero-fills, so a decoded frame from
        // encode_tscf_header always satisfies the predicate.
        let wire = encode_tscf_header(&TscfHeader {
            reserved0: u32::MAX,
            reserved1: u16::MAX,
            ..zero
        })
        .unwrap();
        assert!(tscf_reserved_all_zero(&decode_tscf_header(&wire).unwrap()));
    }

    #[test]
    //fusa:test REQ-AVTP-013
    fn peek_subtype_reads_octet_zero_from_a_one_byte_buffer() {
        assert_eq!(peek_subtype(&[TSCF_SUBTYPE]), Ok(TSCF_SUBTYPE));
        assert_eq!(peek_subtype(&[NTSCF_SUBTYPE, 0, 0]), Ok(NTSCF_SUBTYPE));
        assert_eq!(peek_subtype(&[]), Err(RcpError::ShortFrame));
    }

    #[test]
    //fusa:test REQ-TIMED-012
    fn extend_timestamp_matches_c_rcp_golden_vectors() {
        // (wire_ts, reference_now, expected) triples produced by c-RCP's
        // real rcp_avtp_extend_timestamp (c-RCP/src/avtp.c:249-266).
        let vectors: [(u32, u64, u64); 9] = [
            (0x0000_0010, 0x0000_1234_0000_0000, 0x0000_1234_0000_0010),
            (0x0000_0010, 0x0000_1234_FFFF_FF00, 0x0000_1235_0000_0010),
            (0xFFFF_FF00, 0x0000_1234_0000_0010, 0x0000_1233_FFFF_FF00),
            (0x8000_0000, 0x0000_1234_0000_0000, 0x0000_1234_8000_0000),
            (0x8000_0001, 0x0000_1234_0000_0000, 0x0000_1233_8000_0001),
            (0x7FFF_FFFF, 0x0000_1234_FFFF_FFFF, 0x0000_1234_7FFF_FFFF),
            (0xFFFF_FFFF, 0x0000_0000_0000_0000, 0xFFFF_FFFF_FFFF_FFFF),
            (0x0000_0000, 0xFFFF_FFFF_FFFF_FFFF, 0x0000_0000_0000_0000),
            (0x05F5_E100, 0x0000_ABCD_05F0_0000, 0x0000_ABCD_05F5_E100),
        ];
        for (wire, now, want) in vectors {
            assert_eq!(
                extend_timestamp(wire, now),
                want,
                "extend_timestamp({wire:#010X}, {now:#018X})"
            );
        }
    }

    #[test]
    //fusa:test REQ-TIMED-012
    fn extend_timestamp_picks_the_closest_instant_and_feeds_timed_due() {
        // A request ~100 ms in the future whose low 32 bits are numerically
        // below now's must not be read as ~4.29 s in the past.
        let now: u64 = 0x0000_0001_FFFF_0000;
        let target = now + 100_000_000;
        let wire = target as u32;
        assert!(u64::from(wire) < (now & 0xFFFF_FFFF));
        let extended = extend_timestamp(wire, now);
        assert_eq!(extended, target);
        assert!(!crate::request::timed_due(extended, now));
        assert!(crate::request::timed_due(extended, target));
        // The extension never moves more than half a 2^32 period.
        for wire in [0u32, 1, 0x7FFF_FFFF, 0x8000_0000, u32::MAX] {
            let e = extend_timestamp(wire, now);
            assert_eq!(e as u32, wire);
            assert!(e.abs_diff(now) <= 1u64 << 31);
        }
    }

    #[test]
    //fusa:test REQ-AVTP-035
    fn header_octet1_matches_the_encoded_wire_byte() {
        for dlen in [0u16, 0x038, 0x123, NTSCF_DATA_LENGTH_MAX] {
            let h = NtscfHeader {
                ntscf_data_length: dlen,
                ..Default::default()
            };
            let wire = encode_ntscf_header(&h).unwrap();
            assert_eq!(h.octet1(), wire[1]);
            let v = HeaderVariant::Ntscf(h);
            assert_eq!(v.octet1(), wire[1]);
            assert_eq!(v.subtype(), wire[0]);
            assert!(!v.tu());
        }
        for version in 0u8..=7 {
            for bits in 0u8..8 {
                let h = TscfHeader {
                    version,
                    mr: bits & 1 != 0,
                    tv: bits & 2 != 0,
                    tu: bits & 4 != 0,
                    stream_id: 0xABCD,
                    ..Default::default()
                };
                let wire = encode_tscf_header(&h).unwrap();
                assert_eq!(h.octet1(), wire[1]);
                let v = HeaderVariant::Tscf(h);
                assert_eq!(v.octet1(), wire[1]);
                assert_eq!(v.subtype(), wire[0]);
                assert_eq!(v.tu(), wire[3] & 1 != 0);
                assert_eq!(v.stream_id(), 0xABCD);
            }
        }
        // Pinned against c-RCP's golden TSCF frame (mr=1 tv=1 -> 0x89).
        let (d, _) = decode_tscf_frame(&C_RCP_TSCF_MR_TV_TU_TS0).unwrap();
        assert_eq!(d.octet1(), 0x89);
        assert_eq!(
            HeaderVariant::Ntscf(NtscfHeader {
                stream_id: 7,
                ..Default::default()
            })
            .stream_id(),
            7
        );
    }

    // ═══════════════════════════════════════════════════════════════════
    //  Addressing: stream_id construction/parsing
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    //fusa:test REQ-SID-002
    fn stream_id_round_trip() {
        let mac = [0x00, 0x11, 0x22, 0x33, 0x44, 0x55];
        let unique_id = 0xBEEF;
        let raw = build_stream_id(mac, unique_id);
        let (parsed_mac, parsed_unique_id) = parse_stream_id(raw);
        assert_eq!(parsed_mac, mac);
        assert_eq!(parsed_unique_id, unique_id);
    }

    #[test]
    //fusa:test REQ-SID-002
    fn stream_id_round_trip_zero_values() {
        let raw = build_stream_id([0; 6], 0);
        assert_eq!(raw, 0);
        assert_eq!(parse_stream_id(raw), ([0u8; 6], 0));
    }

    #[test]
    //fusa:test REQ-SID-002
    fn stream_id_round_trip_max_values() {
        let raw = build_stream_id([0xFF; 6], u16::MAX);
        assert_eq!(raw, u64::MAX);
        assert_eq!(parse_stream_id(raw), ([0xFF; 6], u16::MAX));
    }

    #[test]
    //fusa:test REQ-SID-001
    //fusa:test REQ-SID-002
    fn stream_id_type_round_trips_through_new_to_u64_from_u64() {
        let id = StreamId::new([0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF], 0x1234);
        let raw = id.to_u64();
        assert_eq!(StreamId::from_u64(raw), id);
        // `From`/`Into` conversions must agree with the named methods.
        assert_eq!(u64::from(id), raw);
        assert_eq!(StreamId::from(raw), id);
    }

    #[test]
    //fusa:test REQ-SID-003
    fn stream_id_places_sender_mac_in_upper_48_bits() {
        // Each MAC octet must land at a distinct, predictable byte position
        // (upper 48 bits, most-significant octet first), independent of
        // unique_id, so a single non-zero octet is recoverable in isolation.
        for (i, shift) in [56u32, 48, 40, 32, 24, 16].into_iter().enumerate() {
            let mut mac = [0u8; 6];
            mac[i] = 0xAB;
            let raw = build_stream_id(mac, 0);
            assert_eq!(
                raw,
                0xABu64 << shift,
                "octet {i} did not land at bit shift {shift}"
            );
        }
    }

    #[test]
    //fusa:test REQ-SID-003
    fn stream_id_places_unique_id_in_lower_16_bits() {
        let raw = build_stream_id([0; 6], 0x1234);
        assert_eq!(raw, 0x0000_0000_0000_1234);
    }

    #[test]
    //fusa:test REQ-SID-004
    fn parse_stream_id_never_panics_across_arbitrary_u64_values() {
        // parse_stream_id/build_stream_id operate on fixed-width integers
        // and fixed-size arrays only, so there is no truncated/malformed
        // input shape to panic on — this sweeps a deterministic spread of
        // values (including the extremes) to document and enforce that.
        let mut state: u64 = 0x1234_5678_9ABC_DEF0;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let mut values = vec![0u64, u64::MAX, 1, u64::MAX - 1];
        for _ in 0..64 {
            values.push(next());
        }
        for raw in values {
            let (mac, unique_id) = parse_stream_id(raw);
            assert_eq!(build_stream_id(mac, unique_id), raw);
        }
    }

    #[test]
    //fusa:test REQ-SID-005
    fn stream_id_interoperates_with_ntscf_header_opaque_field() {
        let id = StreamId::new([0x02, 0x42, 0xAC, 0x11, 0x00, 0x02], 0x0007);
        let hdr = NtscfHeader {
            sequence_num: 1,
            ntscf_data_length: 0,
            stream_id: id.to_u64(),
        };
        let frame = encode_ntscf_header(&hdr).unwrap();
        let decoded = decode_ntscf_header(&frame).unwrap();
        assert_eq!(StreamId::from_u64(decoded.stream_id), id);
    }

    #[test]
    //fusa:test REQ-SID-005
    fn stream_id_interoperates_with_tscf_header_opaque_field() {
        let id = StreamId::new([0x02, 0x42, 0xAC, 0x11, 0x00, 0x03], 0x0008);
        let hdr = TscfHeader {
            sequence_num: 1,
            stream_id: id.to_u64(),
            ..Default::default()
        };
        let frame = encode_tscf_header(&hdr).unwrap();
        let decoded = decode_tscf_header(&frame).unwrap();
        assert_eq!(StreamId::from_u64(decoded.stream_id), id);
    }

    // ═══════════════════════════════════════════════════════════════════
    //  Frame composition (encode_ntscf_frame / decode_ntscf_frame)
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    //fusa:test REQ-WIRE-001
    //fusa:test REQ-WIRE-002
    //fusa:test REQ-WIRE-003
    //fusa:test REQ-WIRE-004
    //fusa:test REQ-WIRE-005
    fn ntscf_frame_round_trips_arbitrary_payload() {
        let sid = StreamId::new([0x02, 0x11, 0x22, 0x33, 0x44, 0x55], 0x0042);
        let payload = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02, 0x03];
        let frame = encode_ntscf_frame(sid, 7, &payload).unwrap();
        assert_eq!(frame.len(), NTSCF_HEADER_LEN + payload.len());
        let (hdr, decoded_payload) = decode_ntscf_frame(&frame).unwrap();
        assert_eq!(hdr.sequence_num, 7);
        assert_eq!(hdr.ntscf_data_length, payload.len() as u16);
        assert_eq!(StreamId::from_u64(hdr.stream_id), sid);
        assert_eq!(decoded_payload, payload.as_slice());
    }

    #[test]
    //fusa:test REQ-WIRE-005
    fn ntscf_frame_round_trips_empty_payload() {
        let sid = StreamId::default();
        let frame = encode_ntscf_frame(sid, 0, &[]).unwrap();
        assert_eq!(frame.len(), NTSCF_HEADER_LEN);
        let (hdr, decoded_payload) = decode_ntscf_frame(&frame).unwrap();
        assert_eq!(hdr.ntscf_data_length, 0);
        assert!(decoded_payload.is_empty());
    }

    #[test]
    //fusa:test REQ-WIRE-001
    //fusa:test REQ-WIRE-007
    fn ntscf_frame_rejects_oversized_payload() {
        let oversized = vec![0u8; NTSCF_DATA_LENGTH_MAX as usize + 1];
        assert_eq!(
            encode_ntscf_frame(StreamId::default(), 0, &oversized),
            Err(RcpError::InvalidSize)
        );
    }

    #[test]
    //fusa:test REQ-WIRE-008
    fn decode_ntscf_frame_propagates_wrong_subtype() {
        let mut frame = encode_ntscf_frame(StreamId::default(), 0, &[1, 2, 3]).unwrap();
        frame[0] = TSCF_SUBTYPE;
        assert!(matches!(
            decode_ntscf_frame(&frame),
            Err(RcpError::Other(_))
        ));
    }

    #[test]
    //fusa:test REQ-WIRE-009
    fn decode_ntscf_frame_rejects_short_input() {
        for len in 0..NTSCF_HEADER_LEN {
            let buf = vec![0u8; len];
            assert_eq!(decode_ntscf_frame(&buf), Err(RcpError::ShortFrame));
        }
    }

    #[test]
    //fusa:test REQ-AVTP-009
    //fusa:test REQ-WIRE-009
    fn decode_ntscf_frame_rejects_a_frame_shorter_than_its_declared_length() {
        // c-RCP avtp.c:158: len < hdr + ntscf_data_length -> SHORT_FRAME.
        let frame = encode_ntscf_frame(StreamId::default(), 1, &[1, 2, 3, 4, 5]).unwrap();
        for cut in NTSCF_HEADER_LEN..frame.len() {
            assert_eq!(
                decode_ntscf_frame(&frame[..cut]),
                Err(RcpError::ShortFrame),
                "truncated to {cut} bytes"
            );
        }
        assert!(decode_ntscf_frame(&frame).is_ok());
    }

    #[test]
    //fusa:test REQ-AVTP-009
    //fusa:test REQ-WIRE-005
    fn decode_ntscf_frame_ignores_ethernet_minimum_frame_padding() {
        // A 46-byte Ethernet minimum payload: a short AVTPDU is zero-padded
        // up to 46 octets on L2. Only ntscf_data_length bytes are payload.
        let payload = [0xA1u8, 0xB2, 0xC3, 0xD4, 0xE5, 0xF6, 0x07, 0x18];
        let mut padded = encode_ntscf_frame(StreamId::default(), 2, &payload).unwrap();
        assert!(padded.len() < 46);
        padded.resize(46, 0x00);
        let (hdr, got) = decode_ntscf_frame(&padded).unwrap();
        assert_eq!(usize::from(hdr.ntscf_data_length), payload.len());
        assert_eq!(
            got, &payload,
            "trailing padding must not reach the ACF decoder"
        );
    }

    #[test]
    //fusa:test REQ-AVTP-002
    //fusa:test REQ-AVTP-004
    fn tscf_frame_round_trips_and_recomputes_stream_data_length() {
        let hdr = TscfHeader {
            tv: true,
            tu: true,
            sequence_num: 3,
            avtp_timestamp: 77,
            stream_id: 0x0102_0304_0506_0708,
            stream_data_length: 999, // ignored by encode_tscf_frame
            ..Default::default()
        };
        let payload = vec![0x5Au8; 300];
        let frame = encode_tscf_frame(&hdr, &payload).unwrap();
        assert_eq!(frame[0], TSCF_SUBTYPE);
        assert_eq!(frame.len(), TSCF_HEADER_LEN + payload.len());
        let (d, got) = decode_tscf_frame(&frame).unwrap();
        assert_eq!(
            d,
            TscfHeader {
                stream_data_length: 300,
                ..hdr
            }
        );
        assert_eq!(got, payload.as_slice());
        let empty = encode_tscf_frame(&hdr, &[]).unwrap();
        assert_eq!(decode_tscf_frame(&empty).unwrap().1, &[] as &[u8]);
    }

    #[test]
    //fusa:test REQ-AVTP-004
    fn tscf_frame_rejects_oversized_payload() {
        let oversized = vec![0u8; usize::from(TSCF_DATA_LENGTH_MAX) + 1];
        assert_eq!(
            encode_tscf_frame(&TscfHeader::default(), &oversized),
            Err(RcpError::InvalidSize)
        );
        let max = vec![0u8; usize::from(TSCF_DATA_LENGTH_MAX)];
        assert!(encode_tscf_frame(&TscfHeader::default(), &max).is_ok());
    }

    #[test]
    //fusa:test REQ-AVTP-024
    fn decode_tscf_frame_rejects_short_and_ignores_padding() {
        let payload = [9u8, 8, 7];
        let frame = encode_tscf_frame(&TscfHeader::default(), &payload).unwrap();
        for cut in TSCF_HEADER_LEN..frame.len() {
            assert_eq!(decode_tscf_frame(&frame[..cut]), Err(RcpError::ShortFrame));
        }
        let mut padded = frame.clone();
        padded.resize(64, 0xEE);
        assert_eq!(decode_tscf_frame(&padded).unwrap().1, &payload);
    }

    #[test]
    //fusa:test REQ-NET-002
    fn select_header_variant_discards_every_subtype_but_the_two_rcp_ones() {
        // TC18 §12.8.2 (TC18.txt line 3567): "If neither a gPTP nor an
        // IEEE1722 frame is found the received frame shall be discarded."
        // The only two AVTPDU subtypes RCP defines are TC18 §11.1
        // Figure 5's TSCF `subtype(0x05)` and Figure 6's NTSCF
        // `subtype(0x82)` — every other value of that octet must be
        // discarded rather than parsed. The literals below are read from
        // those two figures, not from this crate's own constants.
        const TC18_TSCF_SUBTYPE: u8 = 0x05;
        const TC18_NTSCF_SUBTYPE: u8 = 0x82;

        for subtype in 0u8..=0xFF {
            // sv=1 in octet 1 and a full TSCF-length buffer, so the only
            // thing that can decide accept-vs-discard is the subtype octet.
            let mut buf = [0u8; TSCF_HEADER_LEN];
            buf[0] = subtype;
            buf[1] = 0x80;
            let accepted =
                select_header_variant(&buf, TimeSyncCapability::Capable, TscfFallback::Drop)
                    .is_ok();
            let expected = subtype == TC18_TSCF_SUBTYPE || subtype == TC18_NTSCF_SUBTYPE;
            assert_eq!(
                accepted,
                expected,
                "subtype 0x{subtype:02X} must {} be accepted",
                if expected { "" } else { "not" }
            );
        }
    }

    #[test]
    //fusa:test REQ-NTSCF-007
    fn frame_composition_always_emits_the_server_side_ntscf_header() {
        // TC18 §11.1 (TC18.txt line 1130) and §11.4.3 (line 2354): the RC
        // Server always uses the NTSCF header, and responses/acknowledges
        // are always carried under it. `encode_ntscf_frame` is this
        // crate's only whole-frame composition entry point, and its first
        // octet is always Figure 6's literal `subtype(0x82)`.
        for payload_len in [0usize, 1, 4, 7, 64] {
            let payload = vec![0xA5u8; payload_len];
            let frame = encode_ntscf_frame(StreamId::default(), 0x11, &payload).unwrap();
            assert_eq!(
                frame[0], 0x82,
                "TC18 Figure 6 subtype(0x82) for a {payload_len}-byte payload"
            );
            assert_ne!(
                frame[0], 0x05,
                "must never be TC18 Figure 5's client-only TSCF subtype"
            );
        }
    }

    #[test]
    //fusa:test REQ-WIRE-009
    fn ntscf_frame_functions_never_panic_on_arbitrary_input() {
        let mut state: u32 = 0x1357_9BDF;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for len in 0..NTSCF_HEADER_LEN + 20 {
            let buf: Vec<u8> = (0..len).map(|_| (next() & 0xFF) as u8).collect();
            let _ = decode_ntscf_frame(&buf);
        }
    }
}
