#!/usr/bin/env bash
# M0 evidence gate: pinned oracle, ReproIn-capable dcm2niix, deterministic
# reference trees for every local-required dataset.
set -euo pipefail
cd "$(dirname "$0")/../.."
want=43eca09a543efa196d80a130f105baedbc51adce4d242677f7681e48909e156e
got=$(shasum -a 256 .golden/reproinx.py | cut -d' ' -f1)
[[ "$got" == "$want" ]] || { echo "oracle hash $got != $want"; exit 1; }
${DCM2NIIX:-dcm2niix} -h | grep -q '%H' || { echo "dcm2niix lacks -f %H"; exit 1; }
while IFS=$'\t' read -r name src class _; do
  [[ -z "$name" || "$name" == \#* || "$class" != local-required ]] && continue
  [[ -d ".golden/$name/ref" && -s ".golden/$name/raw.SHA256SUMS" ]] || { echo "$name: no reference (run make_refs.sh)"; exit 1; }
  [[ ! -e ".golden/$name/nondeterminism.diff" ]] || { echo "$name: reference nondeterministic"; exit 1; }
  (cd ".golden/$name/raw" && shasum -a 256 -c --quiet ../raw.SHA256SUMS) || { echo "$name: raw tree changed"; exit 1; }
done < tests/golden/datasets.tsv
echo "M0 evidence ok"
