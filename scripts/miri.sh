#!/usr/bin/env bash
# Run the library unit-test suite under Miri (issue #221).
#
# Rust counterpart of c-RCP's ASan/UBSan `sanitize` job
# (c-RCP .github/workflows/ci.yml:86). This crate is #![forbid(unsafe_code)],
# but Miri still checks every `unsafe` block it executes in std and in our
# dependencies (tokio, serde, crc, ...) for undefined behaviour, plus
# memory leaks and data races, exactly as the C sanitizers do for c-RCP.
#
# Scope: `cargo miri test --lib` minus the exclusions in
# scripts/miri-skips.txt (see that file's header for why each exclusion
# exists and for the whole-target exclusions). Any Miri UB report, leak, or
# test failure fails the script.
#
# Usage: bash scripts/miri.sh            (host target)
#        MIRI_TARGET=x86_64-unknown-linux-gnu bash scripts/miri.sh
#
# Isolation is disabled because several tests read the wall clock
# (clock_gettime(CLOCK_REALTIME), e.g. deadline/adapt tests) and the
# environment; under default isolation Miri aborts the whole test binary on
# the first such call. Disabling isolation does not weaken UB checking.

set -euo pipefail

TOOLCHAIN="${MIRI_TOOLCHAIN:-nightly-2026-10-21}"
SKIPS_FILE="$(dirname "$0")/miri-skips.txt"

skip_args=()
while IFS= read -r line || [ -n "$line" ]; do
  pattern="${line%%#*}"
  pattern="$(echo "$pattern" | xargs)"
  [ -n "$pattern" ] && skip_args+=(--skip "$pattern")
done < "$SKIPS_FILE"

target_args=()
if [ -n "${MIRI_TARGET:-}" ]; then
  target_args+=(--target "$MIRI_TARGET")
fi

export MIRIFLAGS="${MIRIFLAGS:--Zmiri-disable-isolation}"
echo "miri: toolchain=$TOOLCHAIN MIRIFLAGS=$MIRIFLAGS target=${MIRI_TARGET:-host}"
echo "miri: ${#skip_args[@]} skip args: ${skip_args[*]:-<none>}"

cargo +"$TOOLCHAIN" miri test --lib --locked "${target_args[@]}" -- \
  --test-threads=1 "${skip_args[@]}"
