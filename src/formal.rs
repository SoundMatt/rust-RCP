// fusa:req REQ-FORMAL-001
// fusa:req REQ-FORMAL-002
// fusa:req REQ-FORMAL-003
// fusa:req REQ-FORMAL-004

//! Formal model helpers — state-machine invariant checking and property witnesses.
//!
//! These are lightweight runtime-checkable invariants that mirror the formal
//! model maintained in offline verification tools (e.g., TLA+). They are used
//! in tests to ensure the implementation never violates key safety properties.

// ── Invariant types ───────────────────────────────────────────────────────────

/// A named invariant with a checkable predicate.
// fusa:req REQ-FORMAL-001
pub struct Invariant<S> {
    pub name:      &'static str,
    pub predicate: Box<dyn Fn(&S) -> bool + Send + Sync>,
}

impl<S> Invariant<S> {
    pub fn new(name: &'static str, f: impl Fn(&S) -> bool + Send + Sync + 'static) -> Self {
        Invariant { name, predicate: Box::new(f) }
    }

    pub fn check(&self, state: &S) -> bool {
        (self.predicate)(state)
    }
}

/// Result of checking a set of invariants against a state.
// fusa:req REQ-FORMAL-002
#[derive(Debug, Default)]
pub struct CheckResult {
    pub passed:  Vec<&'static str>,
    pub failed:  Vec<&'static str>,
}

impl CheckResult {
    pub fn all_passed(&self) -> bool { self.failed.is_empty() }
}

/// Check all invariants against `state`.
// fusa:req REQ-FORMAL-003
pub fn check_all<S>(state: &S, invs: &[Invariant<S>]) -> CheckResult {
    let mut result = CheckResult::default();
    for inv in invs {
        if inv.check(state) {
            result.passed.push(inv.name);
        } else {
            result.failed.push(inv.name);
        }
    }
    result
}

/// Witness: records the first state that violates an invariant.
// fusa:req REQ-FORMAL-004
pub fn witness<S: Clone>(states: &[S], inv: &Invariant<S>) -> Option<S> {
    states.iter().find(|s| !inv.check(s)).cloned()
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    // fusa:test REQ-FORMAL-001
    // fusa:test REQ-FORMAL-003
    fn invariant_passes_when_predicate_true() {
        let inv = Invariant::new("non-negative", |x: &i32| *x >= 0);
        let result = check_all(&42i32, std::slice::from_ref(&inv));
        assert!(result.all_passed());
    }

    #[test]
    // fusa:test REQ-FORMAL-003
    fn invariant_fails_when_predicate_false() {
        let inv = Invariant::new("always-false", |_: &i32| false);
        let result = check_all(&0i32, std::slice::from_ref(&inv));
        assert!(!result.all_passed());
        assert_eq!(result.failed, vec!["always-false"]);
    }

    #[test]
    // fusa:test REQ-FORMAL-002
    fn check_result_tracks_pass_fail() {
        let invs = vec![
            Invariant::new("pass", |x: &i32| *x >= 0),
            Invariant::new("fail", |x: &i32| *x < 0),
        ];
        let r = check_all(&5i32, &invs);
        assert_eq!(r.passed, vec!["pass"]);
        assert_eq!(r.failed, vec!["fail"]);
    }

    #[test]
    // fusa:test REQ-FORMAL-004
    fn witness_finds_first_violation() {
        let inv = Invariant::new("positive", |x: &i32| *x > 0);
        let states = vec![1, 2, -1, 3, -2];
        let w = witness(&states, &inv).unwrap();
        assert_eq!(w, -1);
    }

    #[test]
    // fusa:test REQ-FORMAL-004
    fn witness_returns_none_when_no_violation() {
        let inv = Invariant::new("positive", |x: &i32| *x > 0);
        let states = vec![1, 2, 3];
        assert!(witness(&states, &inv).is_none());
    }
}
