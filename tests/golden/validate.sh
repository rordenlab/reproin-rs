#!/usr/bin/env bash
# Every BIDS root in every golden output must have exactly the validator errors
# accepted in validator_accepted.tsv (normally none).
# Usage: tests/golden/validate.sh <bids-validator>   (pinned: 2.4.1, BIDSvue sidecar build)
set -uo pipefail
cd "$(dirname "$0")/../.."
V=$1; fail=0
for out in .golden/*/out-default; do
  [[ -d "$out" ]] || continue
  ds=$(basename "$(dirname "$out")")
  for dd in $(cd "$out" && find . -name dataset_description.json -not -path "*/derivatives/*"); do
    r="$out/$(dirname "$dd")"
    "$V" --json "$r" > "$r.validator.json" 2>/dev/null  # exits 1 when errors exist
    msg=$(python3 -I - "$r.validator.json" "$ds" tests/golden/validator_accepted.tsv <<'PY'
import collections, json, sys
issues = json.load(open(sys.argv[1]))["issues"]["issues"]
got = collections.Counter((i["code"], i.get("subCode") or "") for i in issues if i["severity"] == "error")
want = collections.Counter()
for line in open(sys.argv[3]):
    if line.startswith("#") or not line.strip():
        continue
    ds, code, sub, n, _ = line.rstrip("\n").split("\t", 4)
    if ds == sys.argv[2]:
        want[(code, sub)] += int(n)
print("ok" if got == want else f"errors {dict(got)} != accepted {dict(want)}")
PY
) || msg="validator output unreadable"
    rm -f "$r.validator.json"
    echo "$r: $msg"; [[ "$msg" == ok ]] || fail=1
  done
done
exit $fail
