# reproin-rs — agent notes

Rust port of dcm2niix `tools/reproinx.py` (ReproIn post-pass). Hosts: `cli/` (`reproinx`,
drop-in CLI), `core/` (planner, used by BIDSvue), `wasm/` + `js/` (`@niivue/reproin`, used by
webapps). Read README.md (design, gates, contract, D1–D10) and PROGRESS.md (milestones,
blockers) first. `plan-rust.md` and `audit*.md` are gitignored local notes; anything durable
belongs here, in README or PROGRESS.

## Parity rule
- Passes in `core/src/passes/` are line-by-line ports; each function names the Python function
  it ports. Keep that shape (refactors that split/merge them were declined) so oracle fixes port.
- Oracle: `.golden/reproinx.py` (`make oracle`), dcm2niix commit 428fabe9, sha256 checked by
  `check-m00`. Never change the pin casually.
- Every behaviour change keeps `make synth` and `make golden` green, or becomes a README
  D-entry plus a synth case tagged `divergence="Dn ..."` (it must keep differing; run.py fails
  if it stops).

## Adding a synth case
- Add `@case("area_what", flags="...", divergence=None)` on `def _(root):` in
  `tests/synth/cases_<area>.py`; build the raw `-f %H` tree under `root` with `synth.py`
  helpers (`nii`, `js`, `text`, `series`, `prov`).
- Run one: `python3 -I tests/synth/run.py .golden/reproinx.py target/release/reproinx -k name`.
- Currently 244 cases; divergences: D2 ×2, D3, D9.

## Golden data
- Local only, in `.golden/` (gitignored). Built by `tests/golden/make_refs.sh [name]` from
  `tests/golden/datasets.tsv` (needs native dcm2niix and `../dcm2niix` for the oracle).
- **Never commit real DICOM or converted outputs.** Synth fixtures are generated.
- Validator errors present in the reference too go in `tests/golden/validator_accepted.tsv`.

## WASM
- `regex-lite`, not `regex` (741 → 544 KB). Build only with `make wasm` (opt-level z,
  panic=abort, strip off). Don't add heavy deps to `core/`.

## Gotchas
- Python unsorted `glob`/`iterdir`/`rglob` → we sort (D1). Don't "fix" toward Python order.
- Python string/number semantics: use `py_split(s, keepends)` (splitlines incl. `\x0c` etc.),
  `py_int`, `py_float`, `py_int_value` in `core/src/passes/mod.rs`; ASCII digits only (D3).
- `core/src/json.rs` reproduces `json.dump(indent=2)` with ensure_ascii and Python float repr.
  Parsing rejects NaN/Infinity (D2).
- The published `@niivue/dcm2niix` worker drops dotfiles (`.reproin_provenance.tsv`). No dcm2niix
  release is needed: `js/dcm2niix/` bundles the published 1.4.20261029 files byte-identical, except
  `worker.jpeg.js`, which carries a one-line patch from `js/tools/vendor-dcm2niix.sh`. That script checks
  the npm integrity and fails if the patch site moves; `js/test` checks the hashes. Rerun the script
  to adopt a new dcm2niix version. The WASM also repeats the provenance header per series (musl
  `ftell` after `fopen("a")`); the post-pass and browser_compare tolerate it. Do not edit `../dcm2niix`.
- bids-validator 2.4.1 exits 1 whenever errors exist; `validate.sh` reads the JSON, not the rc.
- CLI: `.reproinx-undo/` left behind means a failed rollback or crash; reproinx refuses to run
  until it is resolved. Never auto-delete it.
- Applier rule: register the Undo entry *before* any fallible I/O (rollback ignores NotFound).
  Temps use `File::create_new` (`<name>.reproinx-tmp-<pid>-<n>`), never follow/truncate.
- Header-derived sizes use checked arithmetic (`volume_bytes` → Option): wasm32 usize is 32-bit.

## Consumers
- BIDSvue (`../BIDSvue-reproin`, branch `reproin-rs`): bridge `src-tauri/src/reproin.rs`
  (`reproin_plan`, `reproin_drop_volumes`); `src/lib/import/reproin.ts` applies ops via
  `OperationContext`. Uses `reproin-core` as a path dep (TODO in src-tauri/Cargo.toml → submodule
  `crates/reproin-rs`). Gate: `make check-m13`.
- webapps (`../webapps-reproin`, branch `reproin-app`): `apps/reproin` vendors dcm2niix and
  `@niivue/reproin` under `vendor/` with sha256s in `vendor/vendor.lock.json`. **Whenever `js/`
  or the wasm changes, rebuild, recopy and refresh those hashes** (app tests check them).
  Gate: `make check-m14`.

## Open follow-ups / user-gated
- Push reproin-rs (rordenlab/reproin-rs) and tag a release → BIDSvue submodule pin, webapps
  package pin (drop `vendor/`). CI (`.github/workflows`) has never run: nothing pushed.
- Optional, no urgency: in a future dcm2niix release, keep `.reproin_provenance.tsv` in
  worker-core.js and `fseek(tp, 0, SEEK_END)` before `ftell` in `reproinAppendProvenance`. Native
  Windows (MSVC) and musl builds probably repeat the header too; reproin-rs tolerates it.
- Public example/test data: HF `neurodeskorg/webapps` @ `0d94f865…` `examples/reproin/xa60/` (XA60 ReproIn
  session, public domain). Pin by commit; never re-upload in place.
- Merge consumer branches (BIDSvue `reproin-rs`, webapps `reproin-app`).
- Apply `release/dcm2niix-reproinx-deprecation.patch` to dcm2niix after a release exists.
- Not run: webapps Electron offline workflow; BIDSvue in-app Tauri GUI path.
