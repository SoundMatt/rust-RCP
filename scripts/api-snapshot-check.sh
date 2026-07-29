#!/usr/bin/env bash
# Public API stability gate (ROADMAP.md Milestone 10, "Public API stability
# guarantees (semver) for the new core"): fails if the crate's public API
# surface differs from the committed snapshot in docs/PUBLIC_API.txt. See
# docs/SEMVER.md for the stability policy this snapshot enforces. Requires
# a nightly toolchain (cargo-public-api drives rustdoc's JSON output,
# nightly-only as of this writing). Run from repo root.

set -euo pipefail

SNAPSHOT="docs/PUBLIC_API.txt"

if ! command -v cargo-public-api &>/dev/null && ! cargo public-api --help &>/dev/null; then
  echo "ERROR: cargo-public-api required (cargo install cargo-public-api --locked)" >&2
  exit 1
fi

if [ ! -f "$SNAPSHOT" ]; then
  echo "ERROR: $SNAPSHOT not found" >&2
  exit 1
fi

current="$(mktemp)"
trap 'rm -f "$current"' EXIT

cargo +nightly public-api --simplified >"$current" 2>/dev/null \
  || cargo public-api --simplified >"$current"

if ! diff -u "$SNAPSHOT" "$current"; then
  echo
  echo "Public API surface differs from $SNAPSHOT."
  echo
  echo "If this change is intentional, regenerate the snapshot with:"
  echo "  cargo +nightly public-api --simplified > $SNAPSHOT"
  echo "and follow the version-bump guidance in docs/SEMVER.md."
  exit 1
fi

echo "OK — public API surface matches $SNAPSHOT"
