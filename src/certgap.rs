// fusa:req REQ-GAP-001
// fusa:req REQ-GAP-002
// fusa:req REQ-GAP-003
// fusa:req REQ-GAP-004
// fusa:req REQ-GAP-005

//! Certification gap analysis — identifies untraced requirements and missing
//! test coverage for FuSa and cybersecurity standards.

use std::collections::{HashMap, HashSet};

// ── Gap analysis ──────────────────────────────────────────────────────────────

/// Result of a gap analysis run.
// fusa:req REQ-GAP-001
#[derive(Debug, Clone, Default)]
pub struct GapReport {
    /// Requirements that have no implementation annotation.
    pub unimplemented: Vec<String>,
    /// Requirements that have no test annotation.
    pub untested: Vec<String>,
    /// Requirements present in tests but not declared.
    pub undeclared: Vec<String>,
}

impl GapReport {
    pub fn has_gaps(&self) -> bool {
        !self.unimplemented.is_empty() || !self.untested.is_empty() || !self.undeclared.is_empty()
    }
}

/// Run a gap analysis.
///
/// - `declared_reqs`: requirements listed in the spec/`.fusa-reqs.json`
/// - `implemented_reqs`: requirements traced in source (`// fusa:req REQ-*`)
/// - `tested_reqs`: requirements traced in tests (`// fusa:test REQ-*`)
// fusa:req REQ-GAP-002
pub fn analyse(
    declared_reqs: &HashSet<String>,
    implemented_reqs: &HashSet<String>,
    tested_reqs: &HashSet<String>,
) -> GapReport {
    let mut report = GapReport::default();

    for req in declared_reqs {
        if !implemented_reqs.contains(req) {
            report.unimplemented.push(req.clone());
        }
        if !tested_reqs.contains(req) {
            report.untested.push(req.clone());
        }
    }

    for req in tested_reqs {
        if !declared_reqs.contains(req) {
            report.undeclared.push(req.clone());
        }
    }

    report.unimplemented.sort();
    report.untested.sort();
    report.undeclared.sort();
    report
}

/// Coverage ratio: 0.0 (none) to 1.0 (all).
// fusa:req REQ-GAP-003
pub fn coverage(declared: &HashSet<String>, covered: &HashSet<String>) -> f64 {
    if declared.is_empty() {
        return 1.0;
    }
    let matched = declared.iter().filter(|r| covered.contains(*r)).count();
    matched as f64 / declared.len() as f64
}

/// Summarise coverage by requirement prefix (e.g. "REQ-CTRL").
// fusa:req REQ-GAP-004
pub fn coverage_by_prefix(
    declared: &HashSet<String>,
    covered: &HashSet<String>,
) -> HashMap<String, f64> {
    let mut groups: HashMap<String, (usize, usize)> = HashMap::new();
    for req in declared {
        let prefix = req
            .rsplitn(2, '-')
            .last()
            .unwrap_or(req.as_str())
            .to_string();
        let entry = groups.entry(prefix).or_insert((0, 0));
        entry.0 += 1;
        if covered.contains(req) {
            entry.1 += 1;
        }
    }
    groups
        .into_iter()
        .map(|(k, (total, done))| (k, done as f64 / total as f64))
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn set(items: &[&str]) -> HashSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    // fusa:test REQ-GAP-002
    fn no_gaps_when_fully_covered() {
        let reqs = set(&["REQ-A-001", "REQ-A-002"]);
        let r = analyse(&reqs, &reqs, &reqs);
        assert!(!r.has_gaps());
    }

    #[test]
    // fusa:test REQ-GAP-002
    fn unimplemented_detected() {
        let reqs = set(&["REQ-A-001", "REQ-A-002"]);
        let imp = set(&["REQ-A-001"]);
        let r = analyse(&reqs, &imp, &reqs);
        assert_eq!(r.unimplemented, vec!["REQ-A-002"]);
    }

    #[test]
    // fusa:test REQ-GAP-002
    fn untested_detected() {
        let reqs = set(&["REQ-A-001", "REQ-A-002"]);
        let tested = set(&["REQ-A-001"]);
        let r = analyse(&reqs, &reqs, &tested);
        assert_eq!(r.untested, vec!["REQ-A-002"]);
    }

    #[test]
    // fusa:test REQ-GAP-002
    fn undeclared_detected() {
        let declared = set(&["REQ-A-001"]);
        let all = set(&["REQ-A-001", "REQ-PHANTOM-001"]);
        let r = analyse(&declared, &all, &all);
        assert_eq!(r.undeclared, vec!["REQ-PHANTOM-001"]);
    }

    #[test]
    // fusa:test REQ-GAP-003
    fn coverage_ratio() {
        let d = set(&["A", "B", "C", "D"]);
        let c = set(&["A", "B"]);
        let ratio = coverage(&d, &c);
        assert!((ratio - 0.5).abs() < 1e-9);
    }

    #[test]
    // fusa:test REQ-GAP-003
    fn coverage_empty_declared_is_one() {
        assert_eq!(coverage(&set(&[]), &set(&[])), 1.0);
    }

    #[test]
    // fusa:test REQ-GAP-004
    fn coverage_by_prefix_groups_correctly() {
        let declared = set(&["REQ-CTRL-001", "REQ-CTRL-002", "REQ-WIRE-001"]);
        let covered = set(&["REQ-CTRL-001", "REQ-WIRE-001"]);
        let by_prefix = coverage_by_prefix(&declared, &covered);
        assert!((by_prefix["REQ-CTRL"] - 0.5).abs() < 1e-9);
        assert!((by_prefix["REQ-WIRE"] - 1.0).abs() < 1e-9);
    }

    #[test]
    // fusa:test REQ-GAP-001
    fn gap_report_has_gaps_flag() {
        let mut r = GapReport::default();
        assert!(!r.has_gaps());
        r.unimplemented.push("REQ-X-001".into());
        assert!(r.has_gaps());
    }

    #[test]
    // fusa:test REQ-GAP-005
    fn zero_coverage_when_nothing_covered() {
        let declared = set(&["REQ-A-001", "REQ-A-002", "REQ-B-001"]);
        let covered: HashSet<String> = HashSet::new();
        let ratio = coverage(&declared, &covered);
        assert!((ratio - 0.0).abs() < 1e-9);
        let by_prefix = coverage_by_prefix(&declared, &covered);
        assert!((by_prefix["REQ-A"] - 0.0).abs() < 1e-9);
        assert!((by_prefix["REQ-B"] - 0.0).abs() < 1e-9);
    }
}
