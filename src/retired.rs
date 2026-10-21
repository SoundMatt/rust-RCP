//! Retirement guard for requirements whose modules were removed outright.
//!
//! Issue #211 removed five modules c-RCP's Satellite Disposition table
//! (c-RCP `ROADMAP.md:91-131`) marks **DEPRECATE** as having no place in
//! the TC18 model: `tlstransport` (c-RCP `ROADMAP.md:104`, removed at
//! `ROADMAP.md:3803-3804`, cross-referenced by `include/rcp/authz.h:45`:
//! TC18's security control is MACsec / IEEE 802.1AE at the link layer, a
//! deployment-level control, not an application-level TLS wrapper),
//! `proxy` (`:114`), `redundancy` (`:115`), `federation` (`:116`) and
//! `dyndata` (`:120`). They were removed with no deprecation shim, along
//! with every line of code and test that traced their requirements.
//!
//! Their `.fusa-reqs.json` entries are **kept** (for audit history) but
//! marked `"status": "retired"`, `"scope": "retired"` — c-RCP's own
//! convention for withdrawn requirements (c-RCP `.fusa-reqs.json`,
//! `REQ-RMAP-004`..`008`). c-RCP parity: c-RCP itself deleted
//! `REQ-TLS-001`..`013` outright (`ROADMAP.md:3803-3804`); this crate keeps
//! the entries as retired instead, so the requirement-ID history stays
//! auditable and no retired ID is ever silently reused.
//!
//! This module is the only place a retired requirement may still be
//! annotated. What it verifies is the retirement itself: that every entry
//! stays marked retired in the corpus, that the removed modules stay
//! removed, and that no live module re-annotates a retired requirement.
//! `scripts/fusa-gap-check.sh` enforces the same rule from the other side
//! (a retired ID traced from any other file is an error).

//fusa:req REQ-TLS-001
//fusa:req REQ-TLS-002
//fusa:req REQ-TLS-003
//fusa:req REQ-TLS-004
//fusa:req REQ-TLS-005
//fusa:req REQ-PROXY-001
//fusa:req REQ-PROXY-002
//fusa:req REQ-PROXY-003
//fusa:req REQ-PROXY-004
//fusa:req REQ-PROXY-005
//fusa:req REQ-PROXY-006
//fusa:req REQ-RED-001
//fusa:req REQ-RED-002
//fusa:req REQ-RED-003
//fusa:req REQ-RED-004
//fusa:req REQ-RED-005
//fusa:req REQ-RED-006
//fusa:req REQ-RED-007
//fusa:req REQ-RED-008
//fusa:req REQ-FED-001
//fusa:req REQ-FED-002
//fusa:req REQ-FED-003
//fusa:req REQ-FED-004
//fusa:req REQ-FED-005
//fusa:req REQ-DYN-001
//fusa:req REQ-DYN-002
//fusa:req REQ-DYN-003
//fusa:req REQ-DYN-004
//fusa:req REQ-DYN-005
/// Requirement IDs retired by issue #211, grouped by the removed module
/// (relative to the crate root) that used to implement them.
const RETIRED_BY_ISSUE_211: &[(&str, &[&str])] = &[
    (
        "src/tlstransport.rs",
        &[
            "REQ-TLS-001",
            "REQ-TLS-002",
            "REQ-TLS-003",
            "REQ-TLS-004",
            "REQ-TLS-005",
        ],
    ),
    (
        "src/proxy.rs",
        &[
            "REQ-PROXY-001",
            "REQ-PROXY-002",
            "REQ-PROXY-003",
            "REQ-PROXY-004",
            "REQ-PROXY-005",
            "REQ-PROXY-006",
        ],
    ),
    (
        "src/redundancy.rs",
        &[
            "REQ-RED-001",
            "REQ-RED-002",
            "REQ-RED-003",
            "REQ-RED-004",
            "REQ-RED-005",
            "REQ-RED-006",
            "REQ-RED-007",
            "REQ-RED-008",
        ],
    ),
    (
        "src/federation.rs",
        &[
            "REQ-FED-001",
            "REQ-FED-002",
            "REQ-FED-003",
            "REQ-FED-004",
            "REQ-FED-005",
        ],
    ),
    (
        "src/dyndata.rs",
        &[
            "REQ-DYN-001",
            "REQ-DYN-002",
            "REQ-DYN-003",
            "REQ-DYN-004",
            "REQ-DYN-005",
        ],
    ),
];

/// The value c-RCP's schema uses for both `status` and `scope` of a
/// withdrawn requirement.
const RETIRED: &str = "retired";

/// This file, relative to the crate root — the one place a retired
/// requirement may still be annotated.
const THIS_FILE: &str = "src/retired.rs";

fn retired_ids() -> impl Iterator<Item = &'static str> {
    RETIRED_BY_ISSUE_211
        .iter()
        .flat_map(|(_, ids)| ids.iter().copied())
}

/// Every `.rs` file under `dir`, recursively.
fn rust_sources(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read src dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// The annotation-tag prefix, split with `concat!` so this file's own
/// helper code is not itself mistaken for an annotation by rsfusa or
/// `scripts/fusa-gap-check.sh`.
const TAG: &str = concat!("//", "fusa:");

/// Requirement IDs carrying a `req` or `test` annotation tag in `text`.
fn annotated_ids(text: &str) -> std::collections::BTreeSet<String> {
    text.lines()
        .filter_map(|l| {
            let l = l.trim_start();
            let rest = l.strip_prefix(TAG)?;
            rest.strip_prefix("req ")
                .or_else(|| rest.strip_prefix("test "))
        })
        .filter_map(|rest| rest.split_whitespace().next())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corpus() -> Vec<serde_json::Value> {
        let v: serde_json::Value = serde_json::from_str(include_str!("../.fusa-reqs.json"))
            .expect("parse .fusa-reqs.json");
        v["requirements"]
            .as_array()
            .expect("requirements array")
            .clone()
    }

    #[test]
    //fusa:test REQ-TLS-001
    //fusa:test REQ-TLS-002
    //fusa:test REQ-TLS-003
    //fusa:test REQ-TLS-004
    //fusa:test REQ-TLS-005
    //fusa:test REQ-PROXY-001
    //fusa:test REQ-PROXY-002
    //fusa:test REQ-PROXY-003
    //fusa:test REQ-PROXY-004
    //fusa:test REQ-PROXY-005
    //fusa:test REQ-PROXY-006
    //fusa:test REQ-RED-001
    //fusa:test REQ-RED-002
    //fusa:test REQ-RED-003
    //fusa:test REQ-RED-004
    //fusa:test REQ-RED-005
    //fusa:test REQ-RED-006
    //fusa:test REQ-RED-007
    //fusa:test REQ-RED-008
    //fusa:test REQ-FED-001
    //fusa:test REQ-FED-002
    //fusa:test REQ-FED-003
    //fusa:test REQ-FED-004
    //fusa:test REQ-FED-005
    //fusa:test REQ-DYN-001
    //fusa:test REQ-DYN-002
    //fusa:test REQ-DYN-003
    //fusa:test REQ-DYN-004
    //fusa:test REQ-DYN-005
    fn retired_requirements_stay_marked_retired_in_the_corpus() {
        let corpus = corpus();
        for id in retired_ids() {
            let entry = corpus
                .iter()
                .find(|r| r["id"] == id)
                .unwrap_or_else(|| panic!("{id}: retired entries are kept, not deleted"));
            assert_eq!(entry["status"], RETIRED, "{id}: status");
            assert_eq!(entry["scope"], RETIRED, "{id}: scope");
            let title = entry["title"].as_str().expect("title");
            assert!(title.starts_with("RETIRED -- "), "{id}: title {title:?}");
        }
        // Conversely, every entry the corpus marks retired is one this
        // guard knows about, so no retirement goes unverified.
        for r in &corpus {
            if r["status"] == RETIRED {
                let id = r["id"].as_str().expect("id");
                assert!(
                    retired_ids().any(|k| k == id),
                    "{id}: marked retired but not listed in {THIS_FILE}"
                );
            }
        }
    }

    #[test]
    //fusa:test REQ-TLS-001
    //fusa:test REQ-TLS-002
    //fusa:test REQ-TLS-003
    //fusa:test REQ-TLS-004
    //fusa:test REQ-TLS-005
    //fusa:test REQ-PROXY-001
    //fusa:test REQ-PROXY-002
    //fusa:test REQ-PROXY-003
    //fusa:test REQ-PROXY-004
    //fusa:test REQ-PROXY-005
    //fusa:test REQ-PROXY-006
    //fusa:test REQ-RED-001
    //fusa:test REQ-RED-002
    //fusa:test REQ-RED-003
    //fusa:test REQ-RED-004
    //fusa:test REQ-RED-005
    //fusa:test REQ-RED-006
    //fusa:test REQ-RED-007
    //fusa:test REQ-RED-008
    //fusa:test REQ-FED-001
    //fusa:test REQ-FED-002
    //fusa:test REQ-FED-003
    //fusa:test REQ-FED-004
    //fusa:test REQ-FED-005
    //fusa:test REQ-DYN-001
    //fusa:test REQ-DYN-002
    //fusa:test REQ-DYN-003
    //fusa:test REQ-DYN-004
    //fusa:test REQ-DYN-005
    fn retired_modules_stay_removed_and_unannotated() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        for (module, _) in RETIRED_BY_ISSUE_211 {
            assert!(
                !root.join(module).exists(),
                "{module} was removed by issue #211 and must not come back"
            );
        }
        let mut files = Vec::new();
        rust_sources(&root.join("src"), &mut files);
        assert!(!files.is_empty());
        let retired: std::collections::BTreeSet<&str> = retired_ids().collect();
        for file in files {
            if file.ends_with(THIS_FILE) {
                continue;
            }
            let text = std::fs::read_to_string(&file).expect("read source");
            for id in annotated_ids(&text) {
                assert!(
                    !retired.contains(id.as_str()),
                    "{}: annotates retired requirement {id}",
                    file.display()
                );
            }
        }
        // This file itself traces every retired ID exactly as rsfusa
        // needs it to (traced AND tested).
        let own = std::fs::read_to_string(root.join(THIS_FILE)).expect("read self");
        let own: Vec<&str> = own.lines().map(str::trim).collect();
        for id in &retired {
            for kind in ["req", "test"] {
                let tag = format!("{TAG}{kind} {id}");
                assert!(own.contains(&tag.as_str()), "{id}: missing {tag}");
            }
        }
    }

    #[test]
    fn annotated_ids_parses_both_tag_kinds() {
        // Split with `concat!` so the fixture lines are not themselves
        // picked up as annotations by rsfusa / fusa-gap-check.sh.
        let ids = annotated_ids(concat!(
            "  //",
            "fusa:req REQ-A-001\n//",
            "fusa:test REQ-B-002 x\n// fusa:req REQ-C\n"
        ));
        let ids: Vec<_> = ids.iter().map(String::as_str).collect();
        assert_eq!(ids, ["REQ-A-001", "REQ-B-002"]);
    }
}
