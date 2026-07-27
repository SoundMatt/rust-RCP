// fusa:req REQ-RELAY-009

//! Serde helper: serialize `Vec<u8>` as a base64 string per RELAY spec §15.1.
//!
//! `relay::Message.payload` and `rcp::{Command,Response,Status}.payload` are
//! base64-encoded strings in the canonical JSON representation (mirroring
//! Go's `encoding/json` marshalling of `[]byte`); raw byte arrays are
//! rejected by `relay interop` and `relay conform --strict`.

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

/// Base64 serde for an `Option<Vec<u8>>` payload field (used by
/// `rcp::{Command,Response,Status}.payload`, whose Go source is
/// `[]byte \`json:"payload,omitempty"\`` — `None` is omitted from the
/// serialized object entirely).
pub mod opt {
    use super::*;

    // fusa:req REQ-RELAY-009
    pub fn serialize<S>(bytes: &Option<Vec<u8>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match bytes {
            Some(b) => serializer.serialize_str(&STANDARD.encode(b)),
            None => serializer.serialize_none(),
        }
    }

    // fusa:req REQ-RELAY-009
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Vec<u8>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt: Option<String> = Option::deserialize(deserializer)?;
        match opt {
            Some(s) => STANDARD
                .decode(s.as_bytes())
                .map(Some)
                .map_err(serde::de::Error::custom),
            None => Ok(None),
        }
    }

    /// `skip_serializing_if` predicate for `Option<Vec<u8>>` payload fields.
    pub fn is_none(v: &Option<Vec<u8>>) -> bool {
        v.is_none()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Serialize, serde::Deserialize)]
    struct Wrapper {
        #[serde(with = "super")]
        data: Vec<u8>,
    }

    #[derive(serde::Serialize, serde::Deserialize, Default)]
    struct OptWrapper {
        #[serde(default, skip_serializing_if = "opt::is_none", with = "opt")]
        data: Option<Vec<u8>>,
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

    #[test]
    // fusa:test REQ-RELAY-009
    fn base64_opt_none_omitted() {
        let w = OptWrapper::default();
        let json = serde_json::to_string(&w).unwrap();
        assert_eq!(json, "{}");
        let back: OptWrapper = serde_json::from_str("{}").unwrap();
        assert_eq!(back.data, None);
    }

    #[test]
    // fusa:test REQ-RELAY-009
    fn base64_opt_some_round_trip() {
        let w = OptWrapper {
            data: Some(vec![1, 2, 3]),
        };
        let json = serde_json::to_string(&w).unwrap();
        let back: OptWrapper = serde_json::from_str(&json).unwrap();
        assert_eq!(back.data, Some(vec![1, 2, 3]));
    }
}
