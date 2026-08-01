//fusa:req REQ-TS-001
//fusa:req REQ-TS-002
//fusa:req REQ-TS-003
//fusa:req REQ-TS-004
//fusa:req REQ-TS-005
//fusa:req REQ-TS-006

//! Timestamp semantics — TC18 wire format core (`ROADMAP.md` Milestone 1,
//! "Timestamp Semantics" subsection).
//!
//! This module is the third Milestone 1 subsection, picking up right after
//! [`crate::acf`] finished "ACF Messages". Two raw timestamp fields already
//! exist on opposite sides of the wire format:
//!
//! - [`crate::avtp::TscfHeader::avtp_timestamp`] — a 32-bit value,
//!   TSCF-only (NTSCF carries no timestamp at all).
//! - [`crate::acf::AcfGbbMessage::message_timestamp`] — a 64-bit value,
//!   ACF_GBB-only (ACF_ABB carries no timestamp at all).
//!
//! Both fields' own doc comments already flag that they are carried as raw
//! passthrough values only, deferring width/rollover semantics and the
//! invalid-timestamp fallback rule to this exact checklist item. This
//! module is that item, implemented as two standalone newtypes —
//! [`AvtpTimestamp`] and [`MessageTimestamp`] — rather than by changing
//! either header/message struct's field type:
//!
//! - **Distinct widths, distinct rollover periods.** [`AvtpTimestamp`] wraps
//!   a `u32` and rolls over every [`AvtpTimestamp::ROLLOVER_PERIOD`] raw
//!   ticks; [`MessageTimestamp`] wraps a `u64` and rolls over every
//!   [`MessageTimestamp::ROLLOVER_PERIOD`] raw ticks — a period 2^32 times
//!   longer. Being two distinct Rust types (rather than, say, two type
//!   aliases for integers, or one generic type parameterized over width)
//!   means the two can never be compared, subtracted, or confused with one
//!   another by accident — there is no shared trait or conversion between
//!   them, only the one-way `From`/`to_*` conversions each has with its own
//!   matching raw integer type.
//! - **Invalid/uncertain timestamp fallback.** [`AvtpTimestamp::semantics`]
//!   and [`MessageTimestamp::semantics`] fold an all-zero raw value down to
//!   [`TimestampMeaning::Untimed`], matching the roadmap's stated leniency;
//!   every other raw value is [`TimestampMeaning::Timed`].
//!   [`AvtpTimestamp::is_untimed`]/[`MessageTimestamp::is_untimed`] are the
//!   boolean shorthand for the same check.
//!
//! Each type also carries a wraparound-aware comparison pair —
//! [`AvtpTimestamp::wrapping_delta`]/[`AvtpTimestamp::is_after`] and
//! [`MessageTimestamp::wrapping_delta`]/[`MessageTimestamp::is_after`] — so
//! that a rollover near the top of either field's range does not make a
//! timestamp that just wrapped look like it moved backwards. This is not a
//! separate roadmap bullet; it is this module's own reading of what
//! "distinct rollover periods" has to mean operationally (a rollover period
//! is meaningless unless something on top of it accounts for wraparound
//! when comparing two timestamps), and is flagged here, per Guiding
//! Principle 5, alongside the rest of this module's working interpretation.
//!
//! Deliberately out of scope for this module:
//!
//! - Changing [`crate::avtp::TscfHeader::avtp_timestamp`]'s or
//!   [`crate::acf::AcfGbbMessage::message_timestamp`]'s field type to
//!   [`AvtpTimestamp`]/[`MessageTimestamp`], or wiring this module into
//!   either header/message's encode/decode path. Matching every other
//!   Milestone 1 entry, this module is additive: it consumes the two
//!   fields' raw `u32`/`u64` values as conversion inputs/outputs only.
//! - Decode-time validation of any kind (the separate, still-unchecked
//!   "Validation" checklist item — decode functions never panicking on
//!   arbitrary/truncated input). Nothing here decodes a byte slice; both
//!   newtypes are infallible wrappers around an already-decoded integer.
//!
//! ## Provenance note
//!
//! The roadmap states this item as a rule ("distinct widths, distinct
//! rollover periods"; "an all-zero timestamp region folds down to 'treat as
//! untimed'"), citing the OPEN Alliance TC18 Remote Control Protocol
//! Specification v0.5.1_RC by section number only, not by quoting its text.
//! Two specific choices below are this crate's own working interpretation,
//! not a transcription of that (confidential, OPEN-Members-only) document,
//! and are flagged here for reconciliation before being relied on for
//! interop with a real TC18 RC Server:
//!
//! - **The fallback trigger condition.** This module treats *exactly*
//!   all-zero (`0u32`/`0u64`) as the "untimed" sentinel and every other raw
//!   value — including small values like `1` and large ones like
//!   `u32::MAX`/`u64::MAX` — as genuinely timed. The roadmap says "an
//!   all-zero timestamp region", which could instead name a narrower or
//!   wider band of sentinel values (e.g. a handful of reserved low values,
//!   or all-`0xFF` as an additional sentinel alongside all-zero); this
//!   module implements the literal, narrowest reading (exact zero only).
//! - **The two rollover-period lengths.** [`AvtpTimestamp::ROLLOVER_PERIOD`]
//!   and [`MessageTimestamp::ROLLOVER_PERIOD`] are each set to the full
//!   range of the field's own bit width (2^32 and 2^64 raw ticks,
//!   respectively) — i.e. a period counted in raw wire ticks, not in any
//!   particular real-world time unit (nanoseconds, gPTP epochs, etc.). The
//!   specification may define either field's tick unit and/or effective
//!   rollover period differently (e.g. a shorter period than the field's
//!   full bit width, if some high bits are reserved); this module makes no
//!   claim about tick units and assumes the full bit width is significant.

/// How a decoded timestamp value should be interpreted, per this module's
/// invalid/uncertain timestamp fallback rule.
///
/// Returned by [`AvtpTimestamp::semantics`] and [`MessageTimestamp::semantics`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
//fusa:req REQ-TS-004
pub enum TimestampMeaning {
    /// The raw value falls in this module's untimed fallback region (exact
    /// all-zero — see the module's provenance note) and must be treated as
    /// carrying no timing information at all, not as a legitimate "tick
    /// zero" timestamp.
    Untimed,
    /// The raw value is outside the fallback region and carries a genuine,
    /// wraparound-aware timestamp.
    Timed,
}

// ── AvtpTimestamp ─────────────────────────────────────────────────────────────

/// The 32-bit AVTP presentation timestamp carried by
/// [`crate::avtp::TscfHeader::avtp_timestamp`], TSCF-only.
///
/// A distinct type from [`MessageTimestamp`] by design — see the module doc
/// comment's "Distinct widths, distinct rollover periods" section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
//fusa:req REQ-TS-001
pub struct AvtpTimestamp(pub u32);

impl AvtpTimestamp {
    /// This type's rollover period, in raw ticks: the field's full 32-bit
    /// width.
    ///
    /// TC18 §11.4.1 (TC18.txt lines 1952-1953) confirms both the tick unit
    /// and the period: "avtp_timestamp = (AS_sec × 10^9 + AS_ns) mod 2^32
    /// where AS_sec is the gPTP seconds field and AS_ns is the gPTP
    /// nanoseconds field (thus rolls over every 4 seconds)" — i.e. the
    /// ticks are nanoseconds and the modulus is the field's full 2^32
    /// width, which at 1 ns/tick is 4.294967296 s. See the module's
    /// provenance note (now partly reconciled by that clause) and
    /// `REQ-TIME-004` for the gPTP-derivation half of the same clause,
    /// which this crate does not implement.
    //fusa:req REQ-TS-007
    pub const ROLLOVER_PERIOD: u64 = 1u64 << 32;

    /// Wrap a raw `u32` value (e.g. from
    /// [`crate::avtp::TscfHeader::avtp_timestamp`]).
    pub fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// The raw `u32` value, for round-tripping back through
    /// [`crate::avtp::TscfHeader::avtp_timestamp`].
    pub fn to_u32(self) -> u32 {
        self.0
    }

    /// This value's fallback-rule interpretation. See
    /// [`TimestampMeaning`] and the module's provenance note.
    //fusa:req REQ-TS-004
    pub fn semantics(self) -> TimestampMeaning {
        if self.0 == 0 {
            TimestampMeaning::Untimed
        } else {
            TimestampMeaning::Timed
        }
    }

    /// Shorthand for `self.semantics() == TimestampMeaning::Untimed`.
    //fusa:req REQ-TS-004
    pub fn is_untimed(self) -> bool {
        self.semantics() == TimestampMeaning::Untimed
    }

    /// Signed, wraparound-aware difference `self - earlier`, in raw ticks.
    ///
    /// Correctly reports a small positive delta across a rollover (e.g.
    /// `AvtpTimestamp(0).wrapping_delta(AvtpTimestamp(u32::MAX))` is `1`,
    /// not a large negative number), by treating the 32-bit unsigned
    /// difference's own bit pattern as a signed 32-bit value — the same
    /// convention used for wraparound-aware sequence-number comparisons
    /// generally (e.g. RFC 1982 serial number arithmetic). This makes the
    /// result meaningful only for two timestamps within half a rollover
    /// period of one another; a pair exactly half a period apart, or
    /// further, has no unambiguous ordering and this module makes no claim
    /// beyond the sign this arithmetic happens to produce for them (see
    /// the "exactly half a period apart" test below for the boundary this
    /// module resolves to).
    //fusa:req REQ-TS-002
    pub fn wrapping_delta(self, earlier: Self) -> i64 {
        i64::from(self.0.wrapping_sub(earlier.0) as i32)
    }

    /// `true` if `self` is logically after `other`, per
    /// [`Self::wrapping_delta`] — i.e. `self.wrapping_delta(other) > 0`.
    //fusa:req REQ-TS-002
    pub fn is_after(self, other: Self) -> bool {
        self.wrapping_delta(other) > 0
    }
}

impl From<u32> for AvtpTimestamp {
    fn from(raw: u32) -> Self {
        Self::new(raw)
    }
}

impl From<AvtpTimestamp> for u32 {
    fn from(ts: AvtpTimestamp) -> u32 {
        ts.to_u32()
    }
}

// ── MessageTimestamp ──────────────────────────────────────────────────────────

/// The 64-bit message timestamp carried by
/// [`crate::acf::AcfGbbMessage::message_timestamp`], ACF_GBB-only.
///
/// A distinct type from [`AvtpTimestamp`] by design — see the module doc
/// comment's "Distinct widths, distinct rollover periods" section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
//fusa:req REQ-TS-001
pub struct MessageTimestamp(pub u64);

impl MessageTimestamp {
    /// This type's rollover period, in raw ticks: the field's full 64-bit
    /// width — 2^32 times longer than [`AvtpTimestamp::ROLLOVER_PERIOD`].
    ///
    /// TC18 §11.4.1 (TC18.txt lines 1954-1955) confirms both the tick unit
    /// and the period: "message_timestamp = (AS_sec × 10^9 + AS_ns) mod
    /// 2^64 where AS_sec is the gPTP seconds field and AS_ns is the gPTP
    /// nanoseconds field (thus rolls over every 584,9 years)" — i.e. the
    /// ticks are nanoseconds and the modulus is the field's full 2^64
    /// width, which at 1 ns/tick is ~584.9 years of 365 days.
    //fusa:req REQ-TS-007
    pub const ROLLOVER_PERIOD: u128 = 1u128 << 64;

    /// Wrap a raw `u64` value (e.g. from
    /// [`crate::acf::AcfGbbMessage::message_timestamp`]).
    pub fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// The raw `u64` value, for round-tripping back through
    /// [`crate::acf::AcfGbbMessage::message_timestamp`].
    pub fn to_u64(self) -> u64 {
        self.0
    }

    /// This value's fallback-rule interpretation. See
    /// [`TimestampMeaning`] and the module's provenance note.
    //fusa:req REQ-TS-004
    pub fn semantics(self) -> TimestampMeaning {
        if self.0 == 0 {
            TimestampMeaning::Untimed
        } else {
            TimestampMeaning::Timed
        }
    }

    /// Shorthand for `self.semantics() == TimestampMeaning::Untimed`.
    //fusa:req REQ-TS-004
    pub fn is_untimed(self) -> bool {
        self.semantics() == TimestampMeaning::Untimed
    }

    /// Signed, wraparound-aware difference `self - earlier`, in raw ticks.
    ///
    /// Same convention as [`AvtpTimestamp::wrapping_delta`], applied over
    /// this type's own, much longer, 64-bit rollover period: the 64-bit
    /// unsigned difference's own bit pattern is reinterpreted as a signed
    /// 64-bit value. Meaningful only for two timestamps within half a
    /// rollover period of one another — see
    /// [`AvtpTimestamp::wrapping_delta`]'s doc comment for the boundary
    /// case this module resolves to.
    //fusa:req REQ-TS-003
    pub fn wrapping_delta(self, earlier: Self) -> i64 {
        self.0.wrapping_sub(earlier.0) as i64
    }

    /// `true` if `self` is logically after `other`, per
    /// [`Self::wrapping_delta`] — i.e. `self.wrapping_delta(other) > 0`.
    //fusa:req REQ-TS-003
    pub fn is_after(self, other: Self) -> bool {
        self.wrapping_delta(other) > 0
    }
}

impl From<u64> for MessageTimestamp {
    fn from(raw: u64) -> Self {
        Self::new(raw)
    }
}

impl From<MessageTimestamp> for u64 {
    fn from(ts: MessageTimestamp) -> u64 {
        ts.to_u64()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::acf::{AcfGbbMessage, ByteMessageInfo};
    use crate::avtp::TscfHeader;

    // ═══════════════════════════════════════════════════════════════════
    //  Distinct types / distinct widths
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    //fusa:test REQ-TS-001
    fn avtp_timestamp_and_message_timestamp_are_distinct_types() {
        // This is primarily a compile-time property (there is no shared
        // trait or cross-type comparison between the two), but the two
        // types' bit widths differ concretely too: AvtpTimestamp's max
        // value fits in a u32, MessageTimestamp's requires the full u64
        // range.
        let widest_avtp = AvtpTimestamp::new(u32::MAX);
        let widest_message = MessageTimestamp::new(u64::MAX);
        assert_eq!(widest_avtp.to_u32(), u32::MAX);
        assert_eq!(widest_message.to_u64(), u64::MAX);
        assert_ne!(u64::from(widest_avtp.to_u32()), widest_message.to_u64());
    }

    #[test]
    //fusa:test REQ-TS-001
    fn rollover_periods_are_distinct() {
        assert_eq!(AvtpTimestamp::ROLLOVER_PERIOD, 1u64 << 32);
        assert_eq!(MessageTimestamp::ROLLOVER_PERIOD, 1u128 << 64);
        assert_eq!(
            MessageTimestamp::ROLLOVER_PERIOD,
            u128::from(AvtpTimestamp::ROLLOVER_PERIOD) * u128::from(AvtpTimestamp::ROLLOVER_PERIOD)
        );
    }

    // ═══════════════════════════════════════════════════════════════════
    //  AvtpTimestamp: fallback rule
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    //fusa:test REQ-TS-004
    fn avtp_timestamp_zero_is_untimed() {
        assert_eq!(AvtpTimestamp::new(0).semantics(), TimestampMeaning::Untimed);
        assert!(AvtpTimestamp::new(0).is_untimed());
        assert_eq!(AvtpTimestamp::default(), AvtpTimestamp::new(0));
    }

    #[test]
    //fusa:test REQ-TS-004
    fn avtp_timestamp_nonzero_is_timed() {
        for raw in [1u32, 2, 0x1234_5678, u32::MAX] {
            let ts = AvtpTimestamp::new(raw);
            assert_eq!(ts.semantics(), TimestampMeaning::Timed, "raw={raw:#010X}");
            assert!(!ts.is_untimed(), "raw={raw:#010X}");
        }
    }

    // ═══════════════════════════════════════════════════════════════════
    //  MessageTimestamp: fallback rule
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    //fusa:test REQ-TS-004
    fn message_timestamp_zero_is_untimed() {
        assert_eq!(
            MessageTimestamp::new(0).semantics(),
            TimestampMeaning::Untimed
        );
        assert!(MessageTimestamp::new(0).is_untimed());
        assert_eq!(MessageTimestamp::default(), MessageTimestamp::new(0));
    }

    #[test]
    //fusa:test REQ-TS-004
    fn message_timestamp_nonzero_is_timed() {
        for raw in [1u64, 2, 0x0123_4567_89AB_CDEF, u64::MAX] {
            let ts = MessageTimestamp::new(raw);
            assert_eq!(ts.semantics(), TimestampMeaning::Timed, "raw={raw:#018X}");
            assert!(!ts.is_untimed(), "raw={raw:#018X}");
        }
    }

    // ═══════════════════════════════════════════════════════════════════
    //  AvtpTimestamp: wraparound-aware comparison
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    //fusa:test REQ-TS-002
    fn avtp_timestamp_delta_without_wraparound() {
        let earlier = AvtpTimestamp::new(100);
        let later = AvtpTimestamp::new(140);
        assert_eq!(later.wrapping_delta(earlier), 40);
        assert_eq!(earlier.wrapping_delta(later), -40);
        assert!(later.is_after(earlier));
        assert!(!earlier.is_after(later));
    }

    #[test]
    //fusa:test REQ-TS-002
    fn avtp_timestamp_delta_across_rollover_boundary() {
        // 0 is one tick after u32::MAX, wrapping — not "4294967295 ticks
        // behind" if the rollover is accounted for.
        let just_wrapped = AvtpTimestamp::new(0);
        let just_before_wrap = AvtpTimestamp::new(u32::MAX);
        assert_eq!(just_wrapped.wrapping_delta(just_before_wrap), 1);
        assert!(just_wrapped.is_after(just_before_wrap));

        let a = AvtpTimestamp::new(5);
        let b = AvtpTimestamp::new(u32::MAX - 2);
        assert_eq!(a.wrapping_delta(b), 8);
        assert!(a.is_after(b));
        assert!(!b.is_after(a));
    }

    #[test]
    //fusa:test REQ-TS-002
    fn avtp_timestamp_delta_of_self_is_zero() {
        let ts = AvtpTimestamp::new(0xDEAD_BEEF);
        assert_eq!(ts.wrapping_delta(ts), 0);
        assert!(!ts.is_after(ts));
    }

    #[test]
    //fusa:test REQ-TS-002
    fn avtp_timestamp_exactly_half_period_apart_resolves_to_not_after() {
        // Exactly half a rollover period apart has no unambiguous
        // ordering; this module's arithmetic (bit-pattern reinterpreted as
        // signed) resolves this exact boundary to `is_after == false` in
        // both directions. Documented here as a boundary case, not a claim
        // about correctness beyond this module's own convention.
        let a = AvtpTimestamp::new(0);
        let b = AvtpTimestamp::new(1u32 << 31);
        assert_eq!(a.wrapping_delta(b), i64::from(i32::MIN));
        assert!(!a.is_after(b));
        assert_eq!(b.wrapping_delta(a), i64::from(i32::MIN));
        assert!(!b.is_after(a));
    }

    // ═══════════════════════════════════════════════════════════════════
    //  MessageTimestamp: wraparound-aware comparison
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    //fusa:test REQ-TS-003
    fn message_timestamp_delta_without_wraparound() {
        let earlier = MessageTimestamp::new(1_000);
        let later = MessageTimestamp::new(1_500);
        assert_eq!(later.wrapping_delta(earlier), 500);
        assert_eq!(earlier.wrapping_delta(later), -500);
        assert!(later.is_after(earlier));
        assert!(!earlier.is_after(later));
    }

    #[test]
    //fusa:test REQ-TS-003
    fn message_timestamp_delta_across_rollover_boundary() {
        let just_wrapped = MessageTimestamp::new(0);
        let just_before_wrap = MessageTimestamp::new(u64::MAX);
        assert_eq!(just_wrapped.wrapping_delta(just_before_wrap), 1);
        assert!(just_wrapped.is_after(just_before_wrap));

        let a = MessageTimestamp::new(5);
        let b = MessageTimestamp::new(u64::MAX - 2);
        assert_eq!(a.wrapping_delta(b), 8);
        assert!(a.is_after(b));
        assert!(!b.is_after(a));
    }

    #[test]
    //fusa:test REQ-TS-003
    fn message_timestamp_delta_of_self_is_zero() {
        let ts = MessageTimestamp::new(0xDEAD_BEEF_0000_0001);
        assert_eq!(ts.wrapping_delta(ts), 0);
        assert!(!ts.is_after(ts));
    }

    #[test]
    //fusa:test REQ-TS-003
    fn message_timestamp_exactly_half_period_apart_resolves_to_not_after() {
        let a = MessageTimestamp::new(0);
        let b = MessageTimestamp::new(1u64 << 63);
        assert_eq!(a.wrapping_delta(b), i64::MIN);
        assert!(!a.is_after(b));
        assert_eq!(b.wrapping_delta(a), i64::MIN);
        assert!(!b.is_after(a));
    }

    // ═══════════════════════════════════════════════════════════════════
    //  Round trip / interop with the raw wire fields
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    //fusa:test REQ-TS-005
    fn avtp_timestamp_round_trips_through_u32() {
        for raw in [0u32, 1, 0x1234_5678, u32::MAX] {
            assert_eq!(AvtpTimestamp::new(raw).to_u32(), raw);
            assert_eq!(u32::from(AvtpTimestamp::from(raw)), raw);
        }
    }

    #[test]
    //fusa:test REQ-TS-005
    fn message_timestamp_round_trips_through_u64() {
        for raw in [0u64, 1, 0x0123_4567_89AB_CDEF, u64::MAX] {
            assert_eq!(MessageTimestamp::new(raw).to_u64(), raw);
            assert_eq!(u64::from(MessageTimestamp::from(raw)), raw);
        }
    }

    #[test]
    //fusa:test REQ-TS-005
    fn avtp_timestamp_interoperates_with_tscf_header_field() {
        let hdr = TscfHeader {
            sequence_num: 1,
            avtp_timestamp: 0x0BAD_F00D,
            stream_data_length: 0,
            stream_id: 0,
        };
        let ts = AvtpTimestamp::new(hdr.avtp_timestamp);
        assert_eq!(ts.to_u32(), hdr.avtp_timestamp);
        assert_eq!(ts.semantics(), TimestampMeaning::Timed);
    }

    #[test]
    //fusa:test REQ-TS-005
    fn message_timestamp_interoperates_with_acf_gbb_message_field() {
        let msg = AcfGbbMessage {
            info: ByteMessageInfo::default(),
            message_timestamp: 0xCAFE_BABE_0000_0000,
            payload: vec![],
        };
        let ts = MessageTimestamp::new(msg.message_timestamp);
        assert_eq!(ts.to_u64(), msg.message_timestamp);
        assert_eq!(ts.semantics(), TimestampMeaning::Timed);
    }

    #[test]
    //fusa:test REQ-TS-005
    fn zero_message_timestamp_on_acf_gbb_message_is_untimed() {
        // The fallback rule applies identically to a message_timestamp
        // sourced from a real AcfGbbMessage, not just a bare u64.
        let msg = AcfGbbMessage {
            info: ByteMessageInfo::default(),
            message_timestamp: 0,
            payload: vec![],
        };
        assert!(MessageTimestamp::new(msg.message_timestamp).is_untimed());
    }

    // ═══════════════════════════════════════════════════════════════════
    //  Never panics on arbitrary input
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    //fusa:test REQ-TS-006
    fn avtp_timestamp_operations_never_panic_across_arbitrary_input() {
        let mut state: u32 = 0x2468_ACE0;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        let mut values = vec![0u32, u32::MAX, 1, u32::MAX - 1, 1u32 << 31];
        for _ in 0..64 {
            values.push(next());
        }
        for &a in &values {
            for &b in &values {
                let ta = AvtpTimestamp::new(a);
                let tb = AvtpTimestamp::new(b);
                let _ = ta.semantics();
                let _ = ta.is_untimed();
                let _ = ta.wrapping_delta(tb);
                let _ = ta.is_after(tb);
            }
        }
    }

    #[test]
    //fusa:test REQ-TS-006
    fn message_timestamp_operations_never_panic_across_arbitrary_input() {
        let mut state: u64 = 0x1234_5678_9ABC_DEF0;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let mut values = vec![0u64, u64::MAX, 1, u64::MAX - 1, 1u64 << 63];
        for _ in 0..64 {
            values.push(next());
        }
        for &a in &values {
            for &b in &values {
                let ta = MessageTimestamp::new(a);
                let tb = MessageTimestamp::new(b);
                let _ = ta.semantics();
                let _ = ta.is_untimed();
                let _ = ta.wrapping_delta(tb);
                let _ = ta.is_after(tb);
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════════
    //  TC18 §11.4.1 rollover periods
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    //fusa:test REQ-TS-007
    fn rollover_periods_match_tc18_11_4_1_nanosecond_derivation() {
        // TC18 §11.4.1 (TC18.txt lines 1952-1955) states both moduli and
        // both resulting real-world periods, with the tick unit fixed at
        // nanoseconds by the `AS_sec × 10^9 + AS_ns` construction:
        //
        //   avtp_timestamp    = (...) mod 2^32  -> "rolls over every 4 seconds"
        //   message_timestamp = (...) mod 2^64  -> "rolls over every 584,9 years"
        //
        // The moduli below are written out as literal powers of two read
        // from that clause, not from this crate's own constants.
        const TC18_AVTP_MODULUS: u64 = 4_294_967_296; // 2^32
        const TC18_MESSAGE_MODULUS: u128 = 18_446_744_073_709_551_616; // 2^64
        const NANOS_PER_SECOND: u128 = 1_000_000_000;
        const SECONDS_PER_365_DAY_YEAR: u128 = 365 * 24 * 60 * 60;

        assert_eq!(AvtpTimestamp::ROLLOVER_PERIOD, TC18_AVTP_MODULUS);
        assert_eq!(MessageTimestamp::ROLLOVER_PERIOD, TC18_MESSAGE_MODULUS);

        // 2^32 ns = 4.294967296 s, i.e. TC18's "every 4 seconds" (whole
        // seconds: 4, and strictly less than 5).
        let avtp_seconds = u128::from(AvtpTimestamp::ROLLOVER_PERIOD) / NANOS_PER_SECOND;
        assert_eq!(avtp_seconds, 4, "TC18 §11.4.1: rolls over every 4 seconds");

        // 2^64 ns = 18 446 744 073.709551616 s = 584.94... 365-day years,
        // i.e. TC18's "584,9 years" to one decimal place.
        let message_tenths_of_a_year = (MessageTimestamp::ROLLOVER_PERIOD * 10)
            / (NANOS_PER_SECOND * SECONDS_PER_365_DAY_YEAR);
        assert_eq!(
            message_tenths_of_a_year, 5849,
            "TC18 §11.4.1: rolls over every 584,9 years"
        );

        // The two periods differ by exactly the 2^32 factor the two field
        // widths imply.
        assert_eq!(
            MessageTimestamp::ROLLOVER_PERIOD / u128::from(AvtpTimestamp::ROLLOVER_PERIOD),
            u128::from(TC18_AVTP_MODULUS)
        );
    }
}
