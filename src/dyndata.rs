// fusa:req REQ-DYN-001
// fusa:req REQ-DYN-002
// fusa:req REQ-DYN-003
// fusa:req REQ-DYN-004
// fusa:req REQ-DYN-005

//! Dynamic data store — runtime key/value parameters accessible to controllers.
//!
//! Provides a thread-safe store of named byte-vector parameters that can be
//! read, written, and deleted at runtime without restarting controllers.

use std::collections::HashMap;
use std::sync::RwLock;

// ── DynStore ──────────────────────────────────────────────────────────────────

/// Thread-safe dynamic parameter store.
// fusa:req REQ-DYN-001
pub struct DynStore {
    data: RwLock<HashMap<String, Vec<u8>>>,
}

impl DynStore {
    pub fn new() -> Self {
        DynStore {
            data: RwLock::new(HashMap::new()),
        }
    }

    /// Insert or replace a parameter.
    // fusa:req REQ-DYN-002
    pub fn set(&self, key: impl Into<String>, value: Vec<u8>) {
        self.data.write().unwrap().insert(key.into(), value);
    }

    /// Retrieve a parameter value, or `None` if not present.
    // fusa:req REQ-DYN-003
    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.data.read().unwrap().get(key).cloned()
    }

    /// Delete a parameter. Returns `true` if it existed.
    // fusa:req REQ-DYN-004
    pub fn delete(&self, key: &str) -> bool {
        self.data.write().unwrap().remove(key).is_some()
    }

    /// All parameter keys currently present.
    // fusa:req REQ-DYN-005
    pub fn keys(&self) -> Vec<String> {
        self.data.read().unwrap().keys().cloned().collect()
    }

    /// Number of parameters in the store.
    pub fn len(&self) -> usize {
        self.data.read().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.read().unwrap().is_empty()
    }
}

impl Default for DynStore {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    // fusa:test REQ-DYN-001
    fn new_store_is_empty() {
        assert!(DynStore::new().is_empty());
    }

    #[test]
    // fusa:test REQ-DYN-002
    // fusa:test REQ-DYN-003
    fn set_and_get() {
        let s = DynStore::new();
        s.set("key", b"value".to_vec());
        assert_eq!(s.get("key").unwrap(), b"value");
    }

    #[test]
    // fusa:test REQ-DYN-003
    fn get_absent_returns_none() {
        assert!(DynStore::new().get("missing").is_none());
    }

    #[test]
    // fusa:test REQ-DYN-004
    fn delete_removes_key() {
        let s = DynStore::new();
        s.set("k", vec![1]);
        assert!(s.delete("k"));
        assert!(s.get("k").is_none());
        assert!(!s.delete("k"), "second delete should return false");
    }

    #[test]
    // fusa:test REQ-DYN-005
    fn keys_lists_all() {
        let s = DynStore::new();
        s.set("a", vec![]);
        s.set("b", vec![]);
        let mut keys = s.keys();
        keys.sort();
        assert_eq!(keys, vec!["a", "b"]);
    }

    #[test]
    // fusa:test REQ-DYN-002
    fn overwrite_replaces_value() {
        let s = DynStore::new();
        s.set("k", b"v1".to_vec());
        s.set("k", b"v2".to_vec());
        assert_eq!(s.get("k").unwrap(), b"v2");
        assert_eq!(s.len(), 1);
    }
}
