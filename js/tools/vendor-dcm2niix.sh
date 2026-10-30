#!/usr/bin/env bash
# Vendor the published @niivue/dcm2niix into js/dcm2niix/ unchanged, except
# worker.jpeg.js, whose output collector skips dotfiles and so drops the
# `.reproin_provenance.tsv` the post-pass needs. Only that JS wrapper line
# changes; the converter (dcm2niix.jpeg.wasm + glue) stays byte-identical to
# the release clinical users signed off on. Writes js/dcm2niix/SHA256SUMS.
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION=1.4.20261029
INTEGRITY='sha512-qmFDNNhwAXDzRC0Nn/aYjDpLYJ74mRjPHpbibmUR7DKFmo36867wdHONLBngv4dllb4yJmGSbhV8ILR163kjZQ=='
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
curl -fsSL "https://registry.npmjs.org/@niivue/dcm2niix/-/dcm2niix-$VERSION.tgz" -o "$tmp/p.tgz"
got="sha512-$(openssl dgst -sha512 -binary "$tmp/p.tgz" | base64 | tr -d '\n')"
[[ "$got" == "$INTEGRITY" ]] || { echo "npm integrity mismatch: $got"; exit 1; }
tar xzf "$tmp/p.tgz" -C "$tmp"
rm -rf dcm2niix && mkdir dcm2niix
cp "$tmp"/package/{LICENSE,NOTICES} "$tmp"/package/dist/{dcm2niix.jpeg.js,dcm2niix.jpeg.wasm,index.jpeg.js,index.d.ts,worker.jpeg.js} dcm2niix/
old='      if (name.startsWith(".")) continue;'
new='      if (name.startsWith(".") \&\& name !== ".reproin_provenance.tsv") continue; // reproin-rs: keep -f %H provenance'
[[ $(grep -cF "$old" dcm2niix/worker.jpeg.js) == 1 ]] || { echo "worker.jpeg.js: patch site not found"; exit 1; }
sed -i.bak "s|$old|$new|" dcm2niix/worker.jpeg.js && rm dcm2niix/worker.jpeg.js.bak
printf '{ "name": "@niivue/dcm2niix", "version": "%s", "note": "published files, worker.jpeg.js patched by tools/vendor-dcm2niix.sh" }\n' "$VERSION" > dcm2niix/VERSION.json
(cd dcm2niix && shasum -a 256 LICENSE NOTICES dcm2niix.jpeg.js dcm2niix.jpeg.wasm index.jpeg.js index.d.ts worker.jpeg.js > SHA256SUMS)
echo "vendored @niivue/dcm2niix $VERSION (worker.jpeg.js patched)"
