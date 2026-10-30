"""Fixture builders for differential tests: each case builds a tiny dcm2niix
`-f %H`-style tree; run.py post-processes one copy with the pinned reproinx.py
oracle and another with reproinx (Rust) and requires identical results."""
import gzip, json, struct
from pathlib import Path

CASES = []  # (name, flags, builder, divergence)

PROV_COLS = ["StudyInstanceUID", "SeriesNumber", "ProtocolName", "SeriesDescription",
             "StudyDescription", "OutputStem", "PatientAge", "PatientSex",
             "StudyDate", "StudyTime", "PatientID"]


def case(name, flags="", divergence=None):
    """Register `builder(root: Path)` as a differential case. `divergence`
    names a README "Differences from reproinx.py" entry: the case must then
    DIFFER from the oracle (so a fix on either side is noticed)."""
    def deco(fn):
        CASES.append((name, flags, fn, divergence))
        return fn
    return deco


def nii(path, dims=(64, 64, 30, 1), sform=True, gz=True, bitpix=16, data=False,
        affine=(2, 0, 0, 10, 0, 2, 0, 20, 0, 0, 2, 30), pixdim=(1, 2, 2, 2)):
    """NIfTI-1 header (vox_offset 352); `data=True` appends deterministic voxels."""
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    h = bytearray(352)
    struct.pack_into("<i", h, 0, 348)
    d = list(dims) + [1] * (4 - len(dims))
    struct.pack_into("<8h", h, 40, 4 if d[3] > 1 else 3, *d, 1, 1, 1)
    struct.pack_into("<h", h, 72, bitpix)
    struct.pack_into("<8f", h, 76, *pixdim, 1, 1, 1, 1)
    struct.pack_into("<f", h, 108, 352.0)
    if sform:
        struct.pack_into("<h", h, 254, 1)
        struct.pack_into("<12f", h, 280, *affine)
    raw = bytes(h)
    if data:
        n = d[0] * d[1] * d[2] * d[3] * (bitpix // 8)
        raw += bytes((i * 7) % 251 for i in range(n))
    if gz:
        path.write_bytes(gzip.compress(raw, mtime=0))
    else:
        path.write_bytes(raw)


def js(path, obj, raw=None):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(raw if raw is not None else json.dumps(obj, indent=1))


def text(path, s, binary=False):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(s) if binary else path.write_text(s)


def series(dir_, stem, sidecar, dims=(64, 64, 30, 1), **kw):
    """Image + sidecar pair `<dir>/<stem>.nii.gz` + `.json`."""
    nii(Path(dir_) / f"{stem}.nii.gz", dims, **kw)
    js(Path(dir_) / f"{stem}.json", sidecar)


def prov(root, rows):
    """Write `.reproin_provenance.tsv`; rows are dicts over PROV_COLS (missing = "")."""
    lines = ["\t".join(PROV_COLS)]
    for r in rows:
        lines.append("\t".join(str(r.get(c, "")) for c in PROV_COLS))
    text(Path(root) / ".reproin_provenance.tsv", "\n".join(lines) + "\n")
