#!/usr/bin/env bash
# IEC 62443 SL-2 cyber gap check: verify every threat in .fusa-iec62443.json
# has at least one countermeasure requirement with a fusa:test annotation.
# Exits 1 if any gap is found. Run from repo root.

set -euo pipefail

IEC_FILE=".fusa-iec62443.json"
SRC_DIR="src"

if ! command -v python3 &>/dev/null; then
  echo "ERROR: python3 required for cyber gap check" >&2
  exit 1
fi

if [ ! -f "$IEC_FILE" ]; then
  echo "WARNING: $IEC_FILE not found — no IEC 62443 threats declared, skipping"
  exit 0
fi

python3 - "$IEC_FILE" "$SRC_DIR" <<'EOF'
import sys, json, re, pathlib

iec_file = pathlib.Path(sys.argv[1])
src_dir  = pathlib.Path(sys.argv[2])

data = json.loads(iec_file.read_text())
threats = data if isinstance(data, list) else data.get("threats", [])
src_text = "\n".join(p.read_text() for p in src_dir.rglob("*.rs"))
in_test  = set(re.findall(r"//\s*fusa:test\s+(REQ-[\w-]+)", src_text))

gaps = []
for threat in threats:
    tid = threat.get("id", "?")
    countermeasures = threat.get("countermeasures", [])
    if not countermeasures:
        gaps.append((tid, threat.get("title", ""), [], "no countermeasures declared"))
        continue
    untested = [c for c in countermeasures if c not in in_test]
    if untested:
        gaps.append((tid, threat.get("title", ""), untested, "countermeasure(s) lack test annotation"))

if gaps:
    print(f"\nIEC 62443 CYBER GAP REPORT — {len(gaps)} threat(s) with coverage gaps:\n")
    for tid, title, reqs, reason in gaps:
        print(f"  {tid} ({title}): {reason}")
        for r in reqs:
            print(f"    missing fusa:test for {r}")
    sys.exit(1)

total = len(threats)
print(f"Coverage: {total}/{total} (100%) threats have tested countermeasures")
print("OK — no IEC 62443 cyber gaps detected")
EOF
