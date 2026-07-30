#!/usr/bin/env python3
"""HARA ASIL-derivation gate (rust-RCP-N2-03 / rust-RCP-N2-04).

Mechanically re-derives each hazard's ASIL rating from its own S/E/C
inputs against ISO 26262-3:2018 Table 4, and fails if the value recorded
in `.fusa-hara.json` disagrees. This exists so a transcription slip in the
S/E/C-to-ASIL mapping (as previously happened for H-003/H-006/H-008/H-010)
cannot silently reoccur — see `HARA.md`'s own note above its Hazard
Summary table.

Run from the repo root: `python3 scripts/hara_asil_check.py`.
"""

import json
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


def main() -> int:
    repo_root = Path(__file__).resolve().parent.parent
    hara_path = repo_root / ".fusa-hara.json"
    data = json.loads(hara_path.read_text())

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

    if failures:
        print("HARA ASIL derivation check FAILED:", file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1

    print(f"OK — {len(data['hazards'])} hazards' ASIL ratings match ISO 26262-3:2018 Table 4")
    return 0


if __name__ == "__main__":
    sys.exit(main())
