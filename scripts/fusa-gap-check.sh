#!/usr/bin/env bash
# FuSa gap check: verify every requirement has source annotation and test annotation.
# Tag format is //fusa:req / //fusa:test with NO space after the slashes — this is
# exactly what rust-FuSa (rsfusa) trace.rs::annotation_kind matches; a space makes
# the annotation invisible to the real tool.
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

reqs_data = json.loads(reqs_file.read_text())
# .fusa-reqs.json schema 1.0: {"schemaVersion": "1.0", "requirements": [...]}.
# Older files were a bare array; accept both for robustness.
reqs_list = reqs_data["requirements"] if isinstance(reqs_data, dict) else reqs_data

# A requirement carrying "status": "not-implemented" is a deliberate, honest
# record that TC18 mandates something this crate does NOT do. It is part of
# the requirements corpus — so that the corpus is a complete map of TC18's
# normative surface rather than only the parts that happen to be built — but
# by definition it has no implementation and no test to trace to, so it is
# exempt from the annotation requirement below and reported separately.
NOT_IMPL   = "not-implemented"
unimpl     = {r["id"]: r for r in reqs_list if r.get("status") == NOT_IMPL}
declared   = {r["id"] for r in reqs_list} - set(unimpl)

src_text  = "\n".join(p.read_text() for p in src_dir.rglob("*.rs"))
in_src    = set(re.findall(r"//fusa:req\s+(REQ-[\w-]+)", src_text))
in_test   = set(re.findall(r"//fusa:test\s+(REQ-[\w-]+)", src_text))

gaps = []
for req_id in sorted(declared):
    missing = []
    if req_id not in in_src:
        missing.append("source annotation (//fusa:req)")
    if req_id not in in_test:
        missing.append("test annotation (//fusa:test)")
    if missing:
        gaps.append((req_id, missing))

undeclared_src  = in_src  - declared - set(unimpl)
undeclared_test = in_test - declared - set(unimpl)

# An entry marked not-implemented must not also be traced to code/tests —
# that would mean the marker is stale and the corpus is lying in the other
# direction. Treat it as a gap so it gets fixed.
stale_unimpl = sorted((in_src | in_test) & set(unimpl))

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

if stale_unimpl:
    print(f"\nERROR: {len(stale_unimpl)} requirement(s) marked "
          f'"{NOT_IMPL}" but traced to code and/or tests:')
    for r in stale_unimpl:
        print(f"  {r}")

total = len(declared)
covered = len(declared - {g[0] for g in gaps})
pct = 100 * covered // total if total else 0
print(f"\nCoverage: {covered}/{total} ({pct}%) implemented requirements fully traced")
if unimpl:
    print(f"Declared but NOT implemented (TC18 clauses this crate does not "
          f"satisfy): {len(unimpl)}")
    for req_id in sorted(unimpl):
        print(f"  {req_id}: {unimpl[req_id].get('title', '')}")

if gaps or stale_unimpl:
    sys.exit(1)
print("OK — no FuSa gaps detected")
EOF
