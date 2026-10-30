#!/usr/bin/env bash
# Build golden trees: raw = dcm2niix -f %H output; ref = reproinx.py@ORACLE post-pass on a copy.
# Usage: tests/golden/make_refs.sh [name...]   (default: every local-required dataset)
set -euo pipefail
cd "$(dirname "$0")/../.."
ORACLE=428fabe9da66aa73737a0abee8529764eee7edcb
DCM=${DCM2NIIX:-dcm2niix}
G=.golden
mkdir -p "$G"
git -C ../dcm2niix show "$ORACLE:tools/reproinx.py" > "$G/reproinx.py"
"$DCM" -v > "$G/dcm2niix.version" 2>&1 || true
want=("$@")
while IFS=$'\t' read -r name src class _; do
  [[ -z "$name" || "$name" == \#* ]] && continue
  if ((${#want[@]})); then [[ " ${want[*]} " == *" $name "* ]] || continue
  elif [[ "$class" != local-required ]]; then continue; fi
  [[ -d "$src" ]] || { echo "missing: $src" >&2; exit 3; }
  d="$PWD/$G/$name"; rm -rf "$d"; mkdir -p "$d/raw"
  set +e; "$DCM" -f %H -z y -ba o -o "$d/raw" "$src" > "$d/dcm2niix.log" 2>&1; rc=$?; set -e
  mkdir -p "$d/raw"
  echo "$rc" > "$d/dcm2niix.rc"
  for i in 1 2; do
    rm -rf "$d/ref$i"; cp -Rp "$d/raw" "$d/ref$i"
    set +e; python3 -I "$G/reproinx.py" --no-convert "$d/ref$i" "$d/ref$i" > "$d/ref$i.log" 2>&1; echo $? > "$d/ref$i.rc"; set -e
  done
  if diff -r "$d/ref1" "$d/ref2" > "$d/nondeterminism.diff"; then rm "$d/nondeterminism.diff"; mv "$d/ref1" "$d/ref"; rm -rf "$d/ref2"
  else echo "$name: reference is nondeterministic (see $d/nondeterminism.diff)" >&2; mv "$d/ref1" "$d/ref"; fi
  (cd "$d/raw" && find . -type f -print0 | sort -z | xargs -0 shasum -a 256) > "$d/raw.SHA256SUMS"
  echo "$name: dcm2niix rc=$rc reproinx rc=$(cat $d/ref1.rc 2>/dev/null || cat $d/ref2.rc) files=$(find $d/ref -type f | wc -l)"
done < tests/golden/datasets.tsv
