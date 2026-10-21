#!/usr/bin/env python3
"""Branch-coverage ratchet gate (issue #221).

Mirrors c-RCP's "MC/DC regression gate (ratchet floor, not 100%)" step
(c-RCP .github/workflows/ci.yml:379): reads the totals llvm-cov already
computed and hard-fails only on *regression* below the committed floor in
.branch-coverage-ratchet.json. It is not a 100% gate.

Usage:
    cargo +nightly-2026-10-21 llvm-cov --branch --all-targets --locked \
        --json --summary-only --output-path branch-summary.json
    python3 scripts/branch_coverage_gate.py branch-summary.json

Exit status: 0 when branch coverage >= floor, 1 on regression or when the
summary carries no branch data (e.g. llvm-cov was run without --branch,
which would otherwise make the gate pass vacuously).
"""

import json
import os
import pathlib
import sys

RATCHET = pathlib.Path(__file__).resolve().parent.parent / ".branch-coverage-ratchet.json"
# Print a "raise the floor" notice once measured coverage exceeds the floor
# by more than this many points, so the ratchet actually ratchets.
RAISE_HINT_MARGIN = 5.0


def main(argv):
    if len(argv) != 2:
        print(f"usage: {argv[0]} <llvm-cov --json --summary-only output>", file=sys.stderr)
        return 2

    ratchet = json.loads(RATCHET.read_text())
    floor = float(ratchet["floor_percent"])

    summary = json.loads(pathlib.Path(argv[1]).read_text())
    branches = summary["data"][0]["totals"].get("branches", {})
    count = branches.get("count", 0)
    covered = branches.get("covered", 0)
    percent = branches.get("percent")

    if not count or percent is None:
        print("::error::branch coverage gate: summary has no branch data "
              "(was llvm-cov run with --branch?)")
        return 1

    note = f"branch coverage {percent:.2f}% ({covered}/{count}), floor {floor:.2f}%"
    step_summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if step_summary:
        with open(step_summary, "a") as f:
            f.write("### Branch coverage (ratchet-gated, not a 100% gate)\n\n" + note + "\n")

    if percent < floor:
        print(f"::error::Branch coverage regression: {note}. This is a ratchet "
              f"floor (issue #221), not a 100% gate -- add tests for the new "
              f"branches; if the drop is an intentional tradeoff, lower "
              f"floor_percent in .branch-coverage-ratchet.json with a "
              f"provenance note. Do not delete or skip this gate.")
        return 1

    print(f"OK: {note}")
    if percent - floor > RAISE_HINT_MARGIN:
        print(f"::notice title=Branch coverage ratchet::measured {percent:.2f}% is more "
              f"than {RAISE_HINT_MARGIN} points above the floor; consider raising "
              f"floor_percent in .branch-coverage-ratchet.json")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
