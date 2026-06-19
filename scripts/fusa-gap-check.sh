#!/usr/bin/env bash
# FuSa gap check: verify every requirement has source annotation and test annotation.
# Exits 1 if any gap is found. Run from repo root.

set -euo pipefail

REQS_FILE=".fusa-reqs.json"
SRC_DIR="src"

if ! command -v python3 &>/dev/null; then
  echo "ERROR: python3 required for gap check" >&2
  exit 1
fi

python3 - "$REQS_FILE" "$SRC_DIR" <<'EOF'
import sys, json, re, pathlib, textwrap

reqs_file = pathlib.Path(sys.argv[1])
src_dir   = pathlib.Path(sys.argv[2])

declared  = {r["id"] for r in json.loads(reqs_file.read_text())}

src_text  = "\n".join(p.read_text() for p in src_dir.rglob("*.rs"))
in_src    = set(re.findall(r"//\s*fusa:req\s+(REQ-[\w-]+)", src_text))
in_test   = set(re.findall(r"//\s*fusa:test\s+(REQ-[\w-]+)", src_text))

gaps = []
for req_id in sorted(declared):
    missing = []
    if req_id not in in_src:
        missing.append("source annotation (// fusa:req)")
    if req_id not in in_test:
        missing.append("test annotation (// fusa:test)")
    if missing:
        gaps.append((req_id, missing))

undeclared_src  = in_src  - declared
undeclared_test = in_test - declared

if gaps:
    print(f"\nFuSa GAP REPORT — {len(gaps)} requirement(s) with missing coverage:\n")
    for req_id, missing in gaps:
        print(f"  {req_id}: missing {', '.join(missing)}")

if undeclared_src:
    print(f"\nWARNING: {len(undeclared_src)} fusa:req annotation(s) not in {reqs_file}:")
    for r in sorted(undeclared_src):
        print(f"  {r}")

if undeclared_test:
    print(f"\nWARNING: {len(undeclared_test)} fusa:test annotation(s) not in {reqs_file}:")
    for r in sorted(undeclared_test):
        print(f"  {r}")

total = len(declared)
covered = len(declared - {g[0] for g in gaps})
pct = 100 * covered // total if total else 0
print(f"\nCoverage: {covered}/{total} ({pct}%) requirements fully traced")

if gaps:
    sys.exit(1)
print("OK — no FuSa gaps detected")
EOF
