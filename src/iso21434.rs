// fusa:req REQ-CYB-001
// fusa:req REQ-CYB-002
// fusa:req REQ-CYB-003
// fusa:req REQ-CYB-004
// fusa:req REQ-CYB-005
// fusa:req REQ-CYB-006

//! ISO 21434 cybersecurity artifacts: TARA (Threat Analysis and Risk Assessment)
//! data types and validation helpers.
//!
//! These types feed the TARA document generation and map to IEC 62443 SL levels.

use std::fmt;

// ── Risk classification ───────────────────────────────────────────────────────

/// SFOP attack feasibility rating dimensions.
// fusa:req REQ-CYB-001
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Feasibility {
    Low,
    Medium,
    High,
    Critical,
}

impl fmt::Display for Feasibility {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Feasibility::Low => "low",
            Feasibility::Medium => "medium",
            Feasibility::High => "high",
            Feasibility::Critical => "critical",
        })
    }
}

/// Impact levels per ISO 21434 §15.
// fusa:req REQ-CYB-002
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Impact {
    Negligible,
    Moderate,
    Major,
    Severe,
}

impl fmt::Display for Impact {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Impact::Negligible => "negligible",
            Impact::Moderate => "moderate",
            Impact::Major => "major",
            Impact::Severe => "severe",
        })
    }
}

// ── Risk level ────────────────────────────────────────────────────────────────

/// Combined risk level = Feasibility × Impact.
// fusa:req REQ-CYB-003
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// Compute risk level from feasibility and impact per ISO 21434 Table 14.
// fusa:req REQ-CYB-004
pub fn risk_level(f: Feasibility, i: Impact) -> RiskLevel {
    match (f, i) {
        (_, Impact::Negligible) => RiskLevel::Low,
        (Feasibility::Low, _) => RiskLevel::Low,
        (Feasibility::Medium, Impact::Moderate) => RiskLevel::Medium,
        (Feasibility::Medium, _) => RiskLevel::High,
        (Feasibility::High, Impact::Moderate) => RiskLevel::High,
        (Feasibility::High, _) => RiskLevel::Critical,
        (Feasibility::Critical, _) => RiskLevel::Critical,
    }
}

// ── Threat entry ──────────────────────────────────────────────────────────────

/// A single threat in the TARA.
// fusa:req REQ-CYB-005
#[derive(Debug, Clone)]
pub struct Threat {
    pub id: String,
    pub description: String,
    pub feasibility: Feasibility,
    pub impact: Impact,
}

impl Threat {
    pub fn risk_level(&self) -> RiskLevel {
        risk_level(self.feasibility, self.impact)
    }
}

/// Filter threats that meet or exceed the minimum risk level.
// fusa:req REQ-CYB-006
pub fn filter_by_risk(threats: &[Threat], min: RiskLevel) -> Vec<&Threat> {
    let rank = |r: RiskLevel| r as u8;
    threats
        .iter()
        .filter(|t| rank(t.risk_level()) >= rank(min))
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    // fusa:test REQ-CYB-003
    // fusa:test REQ-CYB-004
    fn negligible_impact_always_low_risk() {
        for f in [
            Feasibility::Low,
            Feasibility::Medium,
            Feasibility::High,
            Feasibility::Critical,
        ] {
            assert_eq!(risk_level(f, Impact::Negligible), RiskLevel::Low);
        }
    }

    #[test]
    // fusa:test REQ-CYB-004
    fn critical_feasibility_severe_impact_is_critical() {
        assert_eq!(
            risk_level(Feasibility::Critical, Impact::Severe),
            RiskLevel::Critical
        );
    }

    #[test]
    // fusa:test REQ-CYB-004
    fn low_feasibility_is_always_low_risk() {
        for i in [
            Impact::Negligible,
            Impact::Moderate,
            Impact::Major,
            Impact::Severe,
        ] {
            assert_eq!(risk_level(Feasibility::Low, i), RiskLevel::Low);
        }
    }

    #[test]
    // fusa:test REQ-CYB-005
    fn threat_risk_level() {
        let t = Threat {
            id: "T-001".into(),
            description: "replay attack".into(),
            feasibility: Feasibility::High,
            impact: Impact::Severe,
        };
        assert_eq!(t.risk_level(), RiskLevel::Critical);
    }

    #[test]
    // fusa:test REQ-CYB-006
    fn filter_by_risk_high() {
        let threats = vec![
            Threat {
                id: "T-001".into(),
                description: "".into(),
                feasibility: Feasibility::Low,
                impact: Impact::Severe,
            },
            Threat {
                id: "T-002".into(),
                description: "".into(),
                feasibility: Feasibility::High,
                impact: Impact::Severe,
            },
        ];
        let high = filter_by_risk(&threats, RiskLevel::High);
        assert_eq!(high.len(), 1);
        assert_eq!(high[0].id, "T-002");
    }

    #[test]
    // fusa:test REQ-CYB-001
    // fusa:test REQ-CYB-002
    fn display_variants() {
        assert_eq!(format!("{}", Feasibility::Critical), "critical");
        assert_eq!(format!("{}", Impact::Severe), "severe");
    }
}
