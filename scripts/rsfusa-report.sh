#!/usr/bin/env bash
# Run an rsfusa *report generator* and gate on it having actually produced
# its report (issue #218).
#
# The rsfusa lifecycle steps in ci.yml/release.yml used to end in `|| true`,
# so a crashed run, a usage error after an rsfusa upgrade, or a report that
# was never written all left CI green. Report generators whose job is to
# RECORD gaps rather than gate on them (the ISO 26262/IEC 61508/DO-178C/
# IEC 62443/ISO 21434 gap reports, the --strict SARIF for code scanning)
# legitimately exit 1 (EXIT_GATE_FAIL) whenever they find a gap -- c-RCP
# keeps those as `|| true` for the same reason (c-RCP/.github/workflows/
# release.yml "Gap/coverage reports ... job is to GENERATE the report
# artifact, not to gate on it (gating lives in ci.yml)").
#
# This wrapper keeps that intent without the blanket `|| true`:
#   exit 0  -> OK
#   exit 1  -> OK (findings/gaps recorded in the report), reported as a notice
#   exit 2+ -> FAIL (EXIT_USAGE / EXIT_RUNTIME: the tool did not run properly)
# and in every case each listed output file must exist and be non-empty.
#
# Usage: bash scripts/rsfusa-report.sh <output-file>... -- <command> [args...]
set -uo pipefail

outputs=()
while [ $# -gt 0 ] && [ "$1" != "--" ]; do
  outputs+=("$1")
  shift
done
if [ $# -lt 2 ] || [ "${#outputs[@]}" -eq 0 ]; then
  echo "usage: $0 <output-file>... -- <command> [args...]" >&2
  exit 2
fi
shift # drop --

rc=0
"$@" || rc=$?

if [ "$rc" -gt 1 ]; then
  echo "::error::'$*' failed with exit code $rc (usage/runtime error, not a recorded finding)"
  exit "$rc"
fi
if [ "$rc" -eq 1 ]; then
  echo "::notice::'$*' recorded findings/gaps (exit 1); see ${outputs[*]}"
fi

missing=0
for f in "${outputs[@]}"; do
  if [ ! -s "$f" ]; then
    echo "::error::'$*' did not produce $f"
    missing=1
  fi
done
exit "$missing"
