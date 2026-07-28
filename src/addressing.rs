// fusa:req REQ-EPLK-001
// fusa:req REQ-EPLK-002
// fusa:req REQ-EPLK-003
// fusa:req REQ-EPLK-004

//! `(stream_id, byte_bus_id)` → endpoint lookup — TC18 wire format core
//! (`ROADMAP.md` Milestone 1, "Addressing" subsection).
//!
//! This module is the second "Addressing" checklist item, picking up right
//! after [`crate::avtp`]'s `stream_id` construction/parsing work. Both
//! addressing primitives it builds on already exist:
//!
//! - [`crate::avtp::StreamId`] — a decomposed AVTP `stream_id`.
//! - [`crate::acf::ByteMessageInfo::byte_bus_id`] — an ACF message's
//!   bus-relative endpoint id, whose own doc comment already calls out that
//!   it is stream-relative, not global, and defers the lookup mechanics to
//!   this module.
//!
//! [`EndpointTable`] is the lookup structure the roadmap asks for. Its
//! defining property is that `byte_bus_id` is scoped *per stream*: the same
//! `byte_bus_id` value legitimately names different endpoints under two
//! different streams (i.e. under two different RC Servers/clients), so a
//! flat table keyed on `byte_bus_id` alone would silently collide those two
//! endpoints. `EndpointTable` avoids that by construction rather than by
//! convention alone — internally it is a map from [`StreamId`] to a
//! per-stream map from `byte_bus_id` to [`EndpointId`], so two streams each
//! get their own independent `byte_bus_id` keyspace and can never collide
//! with each other no matter what values either one uses.
//!
//! [`EndpointId`] is a crate-internal placeholder handle only. Concrete,
//! device-facing endpoint types (GPIO, SPI, CAN, etc.) are later milestones'
//! work (`ROADMAP.md` Milestone 4 onward); this module does not model, nor
//! does it need to model, what an endpoint actually *is* — only that a
//! `(stream_id, byte_bus_id)` pair resolves to at most one of them.
//!
//! Deliberately out of scope for this module (a separate "Addressing"
//! checklist item):
//!
//! - The echo-back rule (a response/ack must carry the same `byte_bus_id` it
//!   was received under). That is a rule about constructing/validating
//!   outgoing response messages, not about this table's lookup mechanics,
//!   and is not implemented here — it now lives in
//!   [`crate::acf::build_response_info`]/[`crate::acf::verify_echo_back`]
//!   instead, since it is stated purely in terms of `byte_bus_id`, which
//!   already lives on [`crate::acf::ByteMessageInfo`] with no dependency on
//!   this module's `StreamId`/`EndpointTable` machinery.
//!
//! This module does not wire itself into [`crate::avtp`] or [`crate::acf`]
//! decoding, and does not cut over any existing caller — it is additive,
//! standalone plumbing, matching the discipline of every prior Milestone 1
//! entry.
//!
//! ## Provenance note
//!
//! `EndpointTable`'s internal nested-map shape is this crate's own
//! implementation choice for satisfying the roadmap's stream-relative
//! uniqueness requirement — the specification does not (and would not)
//! prescribe an in-memory data structure. The *addressing rule* it
//! enforces (that `byte_bus_id` uniqueness is scoped to a single stream) is
//! not a new claim made here: it restates, and gives an enforced shape to,
//! the same convention already flagged in [`crate::acf::ByteMessageInfo`]'s
//! own doc comment.

use crate::acf::BYTE_MESSAGE_INFO_11BIT_MAX;
use crate::avtp::StreamId;
use crate::RcpError;
use std::collections::HashMap;

/// An opaque, placeholder endpoint handle.
///
/// Stands in for whatever concrete endpoint representation later milestones
/// introduce (`ROADMAP.md` Milestone 4 onward). [`EndpointTable`] only needs
/// something hashable/comparable to resolve a `(stream_id, byte_bus_id)`
/// pair to — it does not need to know what an endpoint actually is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
// fusa:req REQ-EPLK-001
pub struct EndpointId(pub u32);

/// A stream-scoped `(stream_id, byte_bus_id)` → [`EndpointId`] lookup table.
///
/// See the module doc comment for why `byte_bus_id`'s uniqueness is
/// enforced per-[`StreamId`] rather than across the whole table.
#[derive(Debug, Clone, Default)]
// fusa:req REQ-EPLK-001
pub struct EndpointTable {
    streams: HashMap<StreamId, HashMap<u16, EndpointId>>,
}

impl EndpointTable {
    /// Construct an empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `endpoint` under the `(stream_id, byte_bus_id)` pair.
    ///
    /// Returns `Err(RcpError::InvalidSize)` if `byte_bus_id` exceeds the
    /// 11-bit field width also enforced by
    /// [`crate::acf::encode_byte_message_info`]. Returns
    /// `Err(RcpError::EpError)` — without modifying the table — if
    /// `(stream_id, byte_bus_id)` is already registered; this pair is never
    /// silently overwritten. The same `byte_bus_id` may be registered
    /// independently under any number of *different* `stream_id`s, since
    /// uniqueness here is stream-relative, not global.
    // fusa:req REQ-EPLK-002
    pub fn register(
        &mut self,
        stream_id: StreamId,
        byte_bus_id: u16,
        endpoint: EndpointId,
    ) -> Result<(), RcpError> {
        if byte_bus_id > BYTE_MESSAGE_INFO_11BIT_MAX {
            return Err(RcpError::InvalidSize);
        }
        let bus_table = self.streams.entry(stream_id).or_default();
        if bus_table.contains_key(&byte_bus_id) {
            return Err(RcpError::EpError);
        }
        bus_table.insert(byte_bus_id, endpoint);
        Ok(())
    }

    /// Resolve `(stream_id, byte_bus_id)` to its registered [`EndpointId`],
    /// or `None` if no endpoint is registered under that exact pair.
    ///
    /// A `byte_bus_id` registered under one `stream_id` is never visible
    /// under any other `stream_id` — both must match exactly.
    // fusa:req REQ-EPLK-003
    // fusa:req REQ-EPLK-004
    pub fn lookup(&self, stream_id: StreamId, byte_bus_id: u16) -> Option<EndpointId> {
        self.streams.get(&stream_id)?.get(&byte_bus_id).copied()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    fn stream(unique_id: u16) -> StreamId {
        StreamId::new([0x02, 0x11, 0x22, 0x33, 0x44, 0x55], unique_id)
    }

    // ── Round-trip ─────────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-EPLK-001
    // fusa:test REQ-EPLK-002
    // fusa:test REQ-EPLK-003
    fn register_then_lookup_resolves_the_same_endpoint() {
        let mut table = EndpointTable::new();
        let sid = stream(1);
        table.register(sid, 7, EndpointId(42)).unwrap();
        assert_eq!(table.lookup(sid, 7), Some(EndpointId(42)));
    }

    #[test]
    // fusa:test REQ-EPLK-002
    fn register_accepts_byte_bus_id_at_11bit_max() {
        let mut table = EndpointTable::new();
        let sid = stream(1);
        table
            .register(sid, BYTE_MESSAGE_INFO_11BIT_MAX, EndpointId(1))
            .unwrap();
        assert_eq!(
            table.lookup(sid, BYTE_MESSAGE_INFO_11BIT_MAX),
            Some(EndpointId(1))
        );
    }

    // ── Stream-relative (not global) uniqueness ─────────────────────────────

    #[test]
    // fusa:test REQ-EPLK-002
    // fusa:test REQ-EPLK-003
    fn same_byte_bus_id_under_two_streams_resolves_independently() {
        let mut table = EndpointTable::new();
        let sid_a = stream(1);
        let sid_b = stream(2);

        // The same byte_bus_id (7) legitimately names a different endpoint
        // under each stream — this must not collide.
        table.register(sid_a, 7, EndpointId(100)).unwrap();
        table.register(sid_b, 7, EndpointId(200)).unwrap();

        assert_eq!(table.lookup(sid_a, 7), Some(EndpointId(100)));
        assert_eq!(table.lookup(sid_b, 7), Some(EndpointId(200)));
    }

    #[test]
    // fusa:test REQ-EPLK-003
    fn lookup_does_not_leak_across_streams() {
        let mut table = EndpointTable::new();
        let sid_a = stream(1);
        let sid_b = stream(2);
        table.register(sid_a, 3, EndpointId(9)).unwrap();
        assert_eq!(table.lookup(sid_b, 3), None);
    }

    // ── Explicit ambiguity flagging ──────────────────────────────────────────

    #[test]
    // fusa:test REQ-EPLK-002
    fn register_rejects_duplicate_pair_without_overwriting() {
        let mut table = EndpointTable::new();
        let sid = stream(1);
        table.register(sid, 5, EndpointId(1)).unwrap();
        let result = table.register(sid, 5, EndpointId(2));
        assert_eq!(result, Err(RcpError::EpError));
        // The original registration must survive the rejected attempt.
        assert_eq!(table.lookup(sid, 5), Some(EndpointId(1)));
    }

    #[test]
    // fusa:test REQ-EPLK-002
    fn register_rejects_oversized_byte_bus_id() {
        let mut table = EndpointTable::new();
        let sid = stream(1);
        let result = table.register(sid, BYTE_MESSAGE_INFO_11BIT_MAX + 1, EndpointId(1));
        assert_eq!(result, Err(RcpError::InvalidSize));
        assert_eq!(table.lookup(sid, BYTE_MESSAGE_INFO_11BIT_MAX + 1), None);
    }

    // ── Fuzz-style: arbitrary lookups never panic ───────────────────────────

    #[test]
    // fusa:test REQ-EPLK-004
    fn lookup_never_panics_on_empty_or_populated_table() {
        let empty = EndpointTable::new();
        let sweep: &[(StreamId, u16)] = &[
            (StreamId::new([0; 6], 0), 0),
            (StreamId::new([0xFF; 6], u16::MAX), u16::MAX),
            (StreamId::new([0x01, 0x02, 0x03, 0x04, 0x05, 0x06], 0), 0),
            (stream(1), BYTE_MESSAGE_INFO_11BIT_MAX),
        ];
        for &(sid, byte_bus_id) in sweep {
            assert_eq!(empty.lookup(sid, byte_bus_id), None);
        }

        let mut populated = EndpointTable::new();
        populated.register(stream(1), 1, EndpointId(1)).unwrap();
        for &(sid, byte_bus_id) in sweep {
            // None of these are the one registered pair, so all miss.
            let _ = populated.lookup(sid, byte_bus_id);
        }
    }
}
