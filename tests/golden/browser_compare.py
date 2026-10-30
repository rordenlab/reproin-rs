#!/usr/bin/env python3
"""Browser (WASM dcm2niix) vs native output for the same DICOMs.

Everything must match under compare.py except these converter-level
differences between native and WASM dcm2niix (not reproin-rs):
  - NIfTI-1 float orientation fields (quatern_b .. srow_z, bytes 256-327)
    within a relative 1e-4; all other header, extension and voxel bytes exact;
  - .bvec values within 1e-5 (printf rounding);
  - .reproin_provenance.tsv rows as a multiset, ignoring repeated header
    lines (WASM <= 1.4.20261029 appends the header per series).
Exit 0 on match, 1 on any other difference, 2 on comparator failure.
"""
import gzip, importlib.util, math, struct, sys, zlib
from pathlib import Path

spec = importlib.util.spec_from_file_location("compare", Path(__file__).with_name("compare.py"))
c = importlib.util.module_from_spec(spec); spec.loader.exec_module(c)


def close(x, y, rel):
    return x == y or (math.isfinite(x) and math.isfinite(y) and abs(x - y) <= rel * max(1.0, abs(x), abs(y)))


def nifti_ok(a, b):
    try:
        a, b = gzip.decompress(a), gzip.decompress(b)
    except (OSError, EOFError, zlib.error):
        return False
    if len(a) != len(b) or len(a) < 352 or a[:256] != b[:256] or a[328:] != b[328:]:
        return False
    fa, fb = struct.unpack("<18f", a[256:328]), struct.unpack("<18f", b[256:328])
    return all(close(x, y, 1e-4) for x, y in zip(fa, fb))


def bvec_ok(a, b):
    ra, rb = a.split(b"\n"), b.split(b"\n")
    try:
        return len(ra) == len(rb) and all(
            len(x.split()) == len(y.split())
            and all(close(float(p), float(q), 1e-5) for p, q in zip(x.split(), y.split()))
            for x, y in zip(ra, rb))
    except ValueError:
        return False


def prov_ok(a, b):
    la, lb = a.decode().splitlines(), b.decode().splitlines()
    if not la or not lb or la[0] != lb[0]:
        return False
    rows = lambda ls: sorted(x for x in ls if x != ls[0])
    return rows(la) == rows(lb)


def main(native, browser):
    n, w = c.files(native), c.files(browser)
    bad = [f"only in native:  {p}" for p in sorted(n.keys() - w.keys())]
    bad += [f"only in browser: {p}" for p in sorted(w.keys() - n.keys())]
    allowed = []
    for p in sorted(n.keys() & w.keys()):
        if c.same(n[p], w[p], p):
            continue
        a, b = Path(n[p]).read_bytes(), Path(w[p]).read_bytes()
        ok = (p.endswith(".nii.gz") and nifti_ok(a, b)) or (p.endswith(".bvec") and bvec_ok(a, b)) \
            or (p.endswith(".reproin_provenance.tsv") and prov_ok(a, b))
        (allowed if ok else bad).append(p)
    for p in allowed:
        print(f"  converter-level: {p}")
    for p in bad:
        print(f"DIFFERS: {p}")
    print("browser output matches native" if not bad else f"{len(bad)} unexpected difference(s)")
    return 1 if bad else 0


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv[1], sys.argv[2]))
    except Exception as e:
        print(f"comparator error: {e!r}", file=sys.stderr)
        sys.exit(2)
