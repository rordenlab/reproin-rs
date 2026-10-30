#!/usr/bin/env bash
# Run reproinx --no-convert on a copy of each golden raw tree and compare with
# reproinx.py (pinned oracle) run on another copy with the same flags.
# Usage: tests/golden/run.sh [name...]; FLAGS="--shift-dates" selects a variant.
# Exit 1 on any difference.
set -uo pipefail
cd "$(dirname "$0")/../.."
cargo build --release --locked -q -p reproinx || exit 2
BIN=$PWD/target/release/reproinx
FLAGS=${FLAGS:-}
tag=$(echo "${FLAGS:-default}" | tr -c 'A-Za-z0-9\n' '_')
fail=0; ran=0
for d in .golden/*/; do
  d=${d%/}; name=$(basename "$d")
  [[ -d "$d/raw" ]] || continue
  if (($#)) && [[ " $* " != *" $name "* ]]; then continue; fi
  ref="$d/ref"; ref_rc=$(cat "$d/ref1.rc" 2>/dev/null || echo 0)
  if [[ -n "$FLAGS" ]]; then
    ref="$d/ref-$tag"
    if [[ ! -d "$ref" ]]; then
      cp -Rp "$d/raw" "$ref"
      python3 -I .golden/reproinx.py --no-convert $FLAGS "$ref" "$ref" > "$ref.log" 2>&1; echo $? > "$ref.rc"
    fi
    ref_rc=$(cat "$ref.rc")
  fi
  out="$d/out-$tag"; rm -rf "$out"; cp -Rp "$d/raw" "$out"
  "$BIN" --no-convert $FLAGS "$out" "$out" > "$out.log" 2>&1; rc=$?
  python3 -I tests/golden/compare.py "$ref" "$out" > "$out.diff"; crc=$?
  n=$(wc -l < "$out.diff" | tr -d ' ')
  ((crc == 0 || n > 0)) || n=1  # comparator failure is never a match
  [[ "$rc" == "$ref_rc" ]] || { echo "$name: exit $rc, reference $ref_rc"; n=$((n+1)); }
  echo "$name [${FLAGS:-default}]: $n difference(s)"; ran=$((ran+1))
  ((n)) && fail=1
done
((ran)) || { echo "no golden datasets; run tests/golden/make_refs.sh"; exit 3; }
exit $fail
