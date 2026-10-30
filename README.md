# reproin-rs

ReproIn DICOM → BIDS in one Rust codebase for three hosts:

- **`reproinx`**: the command line, a drop-in replacement for dcm2niix's
  `tools/reproinx.py` (same positional arguments, flags, defaults and exit codes).
- **BIDSvue**: the `reproin-core` crate (a git submodule once reproin-rs is
  published; a local path dependency until then). BIDSvue applies the
  planned operations through its own mutation, undo and authorization layer.
- **webapps**: a WASM package (`js/`) that converts on-device with the bundled published
  `@niivue/dcm2niix` 1.4.20261029 (`js/dcm2niix/`). The converter is byte-identical; only
  `worker.jpeg.js` has a one-line patch, applied by `js/tools/vendor-dcm2niix.sh`, that keeps
  `.reproin_provenance.tsv`. No dcm2niix release is needed.

[dcm2niix](https://github.com/rordenlab/dcm2niix) still converts the images and does
the one-pass ReproIn naming (`-f %H`). reproin-rs runs dcm2niix and then the
cross-series post-pass: Unknown/ rescue, reclassification, `part-` entities, `__dup`
names, session backfill, physio, scans/events, B0 fields, scaffolding, pCASL context
and date shifting.

## Design

```
host: run dcm2niix → inventory → reproin_core::plan() → apply ops
```

- `core/` has no filesystem, process or async runtime. It takes an inventory and
  returns an ordered list of operations (`Mkdir`, `Rename`, `Write`, `Delete`,
  `RemoveDir`, `DropVolumes`) plus a report.
  - The inventory is paths and sizes, the full text of sidecars and TSVs, and only
    the first 348 bytes of each NIfTI.
  - Planning never reads voxel data. `DropVolumes` (pCASL dummy scans) is the only
    op that touches voxels. Hosts apply it through the streaming
    `nifti::VolumeFilter`, which buffers at most one volume.
- The passes run on an in-memory tree (`vfs.rs`) with pathlib-like calls. They are
  ported almost line for line from `reproinx.py`, and each function names the Python
  function it ports.
- `tables.rs` holds the BIDS/ReproIn constants. A BIDS release means updating this
  file, then the synthetic cases and golden references.
- `cli/` is the native host. Before overwriting or deleting a file it moves it into
  `<out>/.reproinx-undo/` and logs `backup<TAB>original` in its `journal.txt`, so a
  failed apply rolls back. The undo folder is removed only when every restore
  succeeds; after a crash or a failed restore it stays for manual recovery, and
  `reproinx` refuses to start (exit 1, before converting) while it exists. Ops that
  would modify, move or delete a symlink are refused (D5).
- `js/` (`@niivue/reproin`) is the browser/Node host over `wasm/`: `plan`, `apply`,
  `postPass`, `convertToBids` (bundled dcm2niix by default) and `zip`. It has no
  conversion rules.
- BIDSvue calls `reproin_core::plan` from its Tauri bridge and applies the ops
  through its own `OperationContext` (one journal entry, rollback).

## Build and test

```sh
make build            # target/release/reproinx
make wasm             # js/pkg (opt-level z, panic=abort; ~544 KB)
make test lint        # cargo test; clippy -D warnings + fmt
make synth            # synthetic differential cases vs the pinned oracle
make golden           # local real-data golden trees (.golden/, see tests/golden)
```

Milestone gates (`make check-mNN`) fail on any missing tool or data. `m02`–`m12`
each include the previous gate; `m13` and `m14` build on `m12`:

| Gate | Adds | Needs |
|---|---|---|
| `check-m00` | oracle hash, dcm2niix `-f %H`, reference trees unchanged | native `dcm2niix`, `.golden/` |
| `check-m01` | test, lint, wasm | `wasm-pack` |
| `check-m02` | synth, comparator self-test | `python3` (oracle via `make oracle`) |
| `check-m03`…`m10` | golden | `.golden/` (`tests/golden/make_refs.sh`) |
| `check-m11` | six flag variants, bids-validator | `VALIDATOR` (bids-validator 2.4.1; default: on `PATH`) |
| `check-m12` | Node host tests, real browser vs native | `PLAYWRIGHT` (package dir) |
| `check-m13` | BIDSvue `bun run check` + bridge e2e on every golden tree | `bun`, `BIDSVUE` (default `../BIDSvue-reproin`); `XA60E` (XA60e DICOM dir) |
| `check-m14` | webapps build, app tests, Playwright, smoke | `WEBAPPS` (default `../webapps-reproin`) |
| `check-m15` | versions agree, `npm pack`, retirement patch applies | m13 + m14, `DCM2NIIX_REPO` (default `../dcm2niix`) |

`BROWSER_DICOM` (m12, m14) defaults to `$(XA60E)/20260512115434_DR`.

## Compatibility contract

`tests/golden/compare.py` (exit 1 on a difference, 2 on comparator failure):

- relative paths, text and TSV bytes, and unchanged images must match exactly;
- JSON is compared structurally, so key order and numeric spelling may differ;
- `.gz` files are compared after decompression; unreadable gzip is a difference.

`tests/golden/browser_compare.py` (m12) compares WASM-dcm2niix output with native
and allows only converter-level differences: NIfTI float orientation fields
(bytes 256–327, relative 1e-4), `.bvec` values (1e-5), and provenance rows as a
multiset ignoring repeated header lines. Voxels, extensions and other header bytes
must match.

`tests/golden/validator_accepted.tsv` lists validator errors the reference output
also has (7TMOTOR MP2RAGE `RepetitionTimeExcitation` ×4); any other error fails.

On the seven local datasets (`tests/golden/datasets.tsv`) the output is
byte-identical to `reproinx.py` under every flag variant.

The oracle is dcm2niix `tools/reproinx.py` at commit
`428fabe9da66aa73737a0abee8529764eee7edcb`, with SHA-256 `43eca09a…e156e`.

## Differences from reproinx.py

Each entry below is deliberate. Synthetic cases tagged `divergence=` must keep
differing from the oracle.

- **D1. Directory order.** Where `reproinx.py` iterates an unsorted `glob` or
  `iterdir` (`_write_scans_tsv` picking the image for a sidecar,
  `_strip_orphan_runs`, `rglob` walks), reproin-rs uses sorted order. Python's result
  there depends on the filesystem.
- **D2. Non-finite JSON.** Sidecars containing `NaN` or `Infinity` are invalid JSON
  (RFC 8259) and are treated as unreadable. Python's `json` accepts them.
- **D3. ASCII digits.** `int()`, `float()` and regex `\d` accept only ASCII digits.
  Python also accepts other Unicode decimal digits.
- **D4. Path containment.** A provenance `OutputStem` or `BidsGuess` value that
  would create a `.`, `..` or empty path component is refused. Python would write
  outside the output root.
- **D5. Symlinks.** Symlinked files are read through the link, as the reference
  does, but a plan that would modify, move or delete a symlink is refused: the
  whole run rolls back and exits 1. For git-annex/datalad trees, unlock the files
  first. Symlinked directories are not descended into. BIDSvue rejects the whole
  plan if the import tree contains any symlink.
- **Other host notes.** `reproinx` resolves `dcm2niix` only from absolute `PATH`
  entries (relative/empty entries are ignored, unlike `shutil.which`). The
  `--dry-run` temp dir (0700) is removed on normal exit only; SIGKILL leaves it.
  The browser host rejects multi-member `.nii.gz` (DecompressionStream) that the
  native host accepts; it fails safe.
- **D6. Apply failures.** The plan is computed before anything changes, so an I/O
  failure while applying rolls back the whole run (see `cli/` above). Python leaves
  a partial tree.
  A `--strict` or `--shift-dates` abort still applies the ops planned before the
  abort, as Python leaves them on disk, and exits 1.
- **D7. pCASL gzip.** A rewritten `_asl.nii.gz` is gzip level 9 with mtime 0. Its
  decompressed bytes equal Python's; the gzip container bytes may differ.
- **D8. Date parsing.** `--shift-dates` parses the ISO 8601 forms dcm2niix and
  reproinx write: `YYYY-MM-DD` or `YYYYMMDD`, a separator, `HH[:MM[:SS[.f…]]]` or
  the basic form, and an optional `Z`/`±HH[:MM[:SS]]` offset below 24 h (as Python;
  seconds kept). Python 3.11+ `fromisoformat` accepts more (week dates, ordinal
  dates).
- **D9. Curated subject folders.** When there is no usable provenance, the
  session fallback reads `_ses-` only from BIDS datatype folders. `reproinx.py`
  also reads curated folders such as `code/` or `stimuli/`, so a note file there
  could move a subject's data into a session. The retired BIDSvue port already
  excluded them.
- **D10. Unusable pCASL images.** A pCASL image whose header cannot describe a
  volume removal is left untouched, as Python leaves it. That covers zero or
  negative dimensions, a non-positive bitpix, a `vox_offset` of 2 GiB or more,
  and an uncompressed body shorter than its volumes. A truncated `.nii.gz` is
  only discovered while streaming, so it aborts the run with a rollback; Python
  skips that image.

## License

BSD 2-Clause. The passes port dcm2niix's `reproinx.py` (same authors, BSD-style).
