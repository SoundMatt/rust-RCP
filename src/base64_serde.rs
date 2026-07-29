// fusa:req REQ-RELAY-009

//! Serde helper: serialize `Vec<u8>` as a base64 string per RELAY spec §15.1.
//!
//! `relay::Message.payload` is a base64-encoded string in the canonical
//! JSON representation (mirroring Go's `encoding/json` marshalling of
//! `[]byte`); raw byte arrays are rejected by `relay interop` and
//! `relay conform --strict`.
//!
//! This module previously also carried an `opt` submodule for the optional
//! `payload` field of this crate's pre-Milestone-10 `Command`/`Response`/
//! `Status` types. Those types were removed outright by `ROADMAP.md`
//! Milestone 10's core-surface cutover, and `opt` had no other caller, so
//! it is removed here too rather than left dead.

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Deserializer, Serializer};

/// Base64 serde for a required `Vec<u8>` field (used by [`crate::relay::Message`]).
pub fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&STANDARD.encode(bytes))
}

/// Base64 serde-deserialize for a required `Vec<u8>` field.
pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    STANDARD
        .decode(s.as_bytes())
        .map_err(serde::de::Error::custom)
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    #[derive(serde::Serialize, serde::Deserialize)]
    struct Wrapper {
        #[serde(with = "super")]
        data: Vec<u8>,
    }

    #[test]
    // fusa:test REQ-RELAY-009
    fn base64_round_trip() {
        let w = Wrapper {
            data: vec![0xDE, 0xAD, 0xBE, 0xEF],
        };
        let json = serde_json::to_string(&w).unwrap();
        assert_eq!(json, r#"{"data":"3q2+7w=="}"#);
        let back: Wrapper = serde_json::from_str(&json).unwrap();
        assert_eq!(back.data, w.data);
    }
}
