#!/usr/bin/env python3
"""HARA ASIL-derivation gate (rust-RCP-N2-03 / rust-RCP-N2-04).

Mechanically re-derives each hazard's ASIL rating from its own S/E/C
inputs against ISO 26262-3:2018 Table 4, and fails if the value recorded
in `.fusa-hara.json` disagrees. This exists so a transcription slip in the
S/E/C-to-ASIL mapping (as previously happened for H-003/H-006/H-008/H-010)
cannot silently reoccur — see `HARA.md`'s own note above its Hazard
Summary table.

Also cross-checks `HARA.md`'s two markdown tables (Hazard Summary and
Safety Goals) against `.fusa-hara.json`, the CI-enforced source of truth:
the JSON can be internally correct while the human-readable doc drifts out
of sync with it (this happened for SG-010, corrected in the Hazard
Summary table but left stale at ASIL-A in the separate Safety Goals table
until a later pass caught it) — that specific failure mode isn't caught by
the S/E/C re-derivation above, since the JSON itself was already right.

Run from the repo root: `python3 scripts/hara_asil_check.py`.
"""

import json
import re
import sys
from pathlib import Path

# ISO 26262-3:2018 Table 4 ("ASIL determination"): (severity, exposure,
# controllability) -> ASIL. C0 always yields QM regardless of S/E and is
# handled as a special case below rather than duplicated across every S/E
# pair.
TABLE_4 = {
    ("S1", "E1", "C1"): "QM", ("S1", "E1", "C2"): "QM", ("S1", "E1", "C3"): "QM",
    ("S1", "E2", "C1"): "QM", ("S1", "E2", "C2"): "QM", ("S1", "E2", "C3"): "QM",
    ("S1", "E3", "C1"): "QM", ("S1", "E3", "C2"): "QM", ("S1", "E3", "C3"): "ASIL-A",
    ("S1", "E4", "C1"): "QM", ("S1", "E4", "C2"): "ASIL-A", ("S1", "E4", "C3"): "ASIL-B",
    ("S2", "E1", "C1"): "QM", ("S2", "E1", "C2"): "QM", ("S2", "E1", "C3"): "QM",
    ("S2", "E2", "C1"): "QM", ("S2", "E2", "C2"): "QM", ("S2", "E2", "C3"): "ASIL-A",
    ("S2", "E3", "C1"): "QM", ("S2", "E3", "C2"): "ASIL-A", ("S2", "E3", "C3"): "ASIL-B",
    ("S2", "E4", "C1"): "ASIL-A", ("S2", "E4", "C2"): "ASIL-B", ("S2", "E4", "C3"): "ASIL-C",
    ("S3", "E1", "C1"): "QM", ("S3", "E1", "C2"): "QM", ("S3", "E1", "C3"): "ASIL-A",
    ("S3", "E2", "C1"): "QM", ("S3", "E2", "C2"): "ASIL-A", ("S3", "E2", "C3"): "ASIL-B",
    ("S3", "E3", "C1"): "ASIL-A", ("S3", "E3", "C2"): "ASIL-B", ("S3", "E3", "C3"): "ASIL-C",
    ("S3", "E4", "C1"): "ASIL-B", ("S3", "E4", "C2"): "ASIL-C", ("S3", "E4", "C3"): "ASIL-D",
}


def expected_asil(severity: str, exposure: str, controllability: str) -> str:
    if controllability == "C0":
        # Table 4: C0 ("controllable in general") always yields QM,
        # independent of severity/exposure.
        return "QM"
    key = (severity, exposure, controllability)
    if key not in TABLE_4:
        raise ValueError(f"no Table 4 entry for {key}")
    return TABLE_4[key]


def markdown_asil_by_id(markdown: str, heading: str, id_prefix: str) -> dict:
    """Extract {id: asil} from the first `| ID | ... | ASIL-x | ... |` table
    following the given `## heading` in HARA.md. Assumes the row's ID is the
    first pipe-delimited cell and its ASIL is the first cell matching
    `QM`/`ASIL-[A-D]` after that."""
    section = markdown.split(heading, 1)[1]
    # Stop at the next top-level heading so we don't read past this table.
    section = re.split(r"\n## ", section, maxsplit=1)[0]
    out = {}
    for line in section.splitlines():
        if not line.startswith(f"| {id_prefix}"):
            continue
        cells = [c.strip() for c in line.strip("|").split("|")]
        row_id = cells[0]
        asil_cells = [c for c in cells if c == "QM" or re.fullmatch(r"ASIL-[A-D]", c)]
        if not asil_cells:
            continue
        out[row_id] = asil_cells[0]
    return out


def main() -> int:
    repo_root = Path(__file__).resolve().parent.parent
    hara_json_path = repo_root / ".fusa-hara.json"
    hara_md_path = repo_root / "HARA.md"
    data = json.loads(hara_json_path.read_text())
    markdown = hara_md_path.read_text()

    failures = []
    for hazard in data["hazards"]:
        want = expected_asil(
            hazard["severity"], hazard["exposure"], hazard["controllability"]
        )
        got = hazard["asil"]
        if got != want:
            failures.append(
                f"{hazard['id']}: S={hazard['severity']} E={hazard['exposure']} "
                f"C={hazard['controllability']} -> Table 4 says {want}, "
                f".fusa-hara.json says {got}"
            )

    # Cross-check HARA.md's two markdown tables against the JSON source of
    # truth, so a doc that drifts out of sync with an already-correct JSON
    # (rather than a bad S/E/C-to-ASIL derivation) is also caught.
    md_hazards = markdown_asil_by_id(markdown, "## Hazard", "H-")
    md_goals = markdown_asil_by_id(markdown, "## Safety Goals", "SG-")

    for hazard in data["hazards"]:
        md_asil = md_hazards.get(hazard["id"])
        if md_asil is None:
            failures.append(f"{hazard['id']}: missing from HARA.md's Hazard Summary table")
        elif md_asil != hazard["asil"]:
            failures.append(
                f"{hazard['id']}: HARA.md Hazard Summary table says {md_asil}, "
                f".fusa-hara.json says {hazard['asil']}"
            )

    for goal in data["safety_goals"]:
        md_asil = md_goals.get(goal["id"])
        if md_asil is None:
            failures.append(f"{goal['id']}: missing from HARA.md's Safety Goals table")
        elif md_asil != goal["asil"]:
            failures.append(
                f"{goal['id']}: HARA.md Safety Goals table says {md_asil}, "
                f".fusa-hara.json says {goal['asil']}"
            )

    if failures:
        print("HARA ASIL derivation check FAILED:", file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1

    print(
        f"OK — {len(data['hazards'])} hazards' ASIL ratings match ISO 26262-3:2018 "
        f"Table 4, and HARA.md's Hazard Summary + Safety Goals tables both match "
        f".fusa-hara.json"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
