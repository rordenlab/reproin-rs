#!/usr/bin/env python3
"""Comparator self-test: seeded mismatches must be rejected, encoding-only
differences (JSON spelling/order, gzip container) accepted."""
import gzip, importlib.util, io, contextlib, sys, tempfile
from pathlib import Path

def mod(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(f"{name}.py"))
    m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
    return m
c, bc = mod("compare"), mod("browser_compare")

def tree(root, files):
    for k, v in files.items():
        p = root / k; p.parent.mkdir(parents=True, exist_ok=True); p.write_bytes(v)

def rc(a, b, m=None):
    with tempfile.TemporaryDirectory() as t:
        t = Path(t); tree(t / "a", a); tree(t / "b", b)
        with contextlib.redirect_stdout(io.StringIO()):
            return (m or c).main(str(t / "a"), str(t / "b"))

base = {"x.json": b'{"a": 1.0, "b": [1, 2]}', "y.tsv": b"a\tb\n", "z.nii.gz": gzip.compress(b"img", mtime=0)}
ok = [dict(base, **{"x.json": b'{"b":[1,2],"a":1.00}'}),
      dict(base, **{"z.nii.gz": gzip.compress(b"img", compresslevel=1, mtime=5)})]
bad = [dict(base, **{"x.json": b'{"a": 1.5, "b": [1, 2]}'}), dict(base, **{"x.json": b'{"a": 1.0, "b": [2, 1]}'}),
       dict(base, **{"y.tsv": b"a\tb\r\n"}), dict(base, **{"z.nii.gz": gzip.compress(b"imx", mtime=0)}),
       {k: v for k, v in base.items() if k != "y.tsv"}, dict(base, **{"extra": b""})]
assert rc(base, base) == 0
assert all(rc(base, o) == 0 for o in ok), "encoding-only differences must pass"
assert all(rc(base, o) == 1 for o in bad), "seeded mismatches must fail"
trunc = dict(base, **{"z.nii.gz": gzip.compress(b"img", mtime=0)[:-6]})
assert rc(base, trunc) == 1, "truncated gzip must be a difference"

import struct
def nii(quat=0.25, vox=b"\x01\x02\x03\x04"):
    h = bytearray(352); struct.pack_into("<i", h, 0, 348); struct.pack_into("<f", h, 256, quat)
    return gzip.compress(bytes(h) + vox, mtime=0)
wb = {"i.nii.gz": nii(), "g.bvec": b"0 0.5 -1\n", ".reproin_provenance.tsv": b"H\nr1\nr2\n"}
wok = {"i.nii.gz": nii(quat=0.2500001), "g.bvec": b"0 0.500001 -1\n", ".reproin_provenance.tsv": b"H\nr2\nH\nr1\n"}
assert rc(wb, wok, bc) == 0, "converter rounding and provenance order must pass"
for k, v in {"i.nii.gz": nii(vox=b"\x01\x02\x03\x05"), "g.bvec": b"0 -0.5 -1\n",
             ".reproin_provenance.tsv": b"H\nr1\nr3\n"}.items():
    assert rc(wb, dict(wb, **{k: v}), bc) == 1, f"browser gate must reject a changed {k}"
assert rc(wb, dict(wb, **{"i.nii.gz": nii()[:-6]}), bc) == 1
shape = {"g.bvec": b"1 0\n0 1\n0 0\n"}
assert rc(shape, {"g.bvec": b"1 0 0\n1 0 0\n"}, bc) == 1, "bvec shape change must fail"
assert rc(shape, {"g.bvec": b"1 0.000001\n0 1\n0 0\n"}, bc) == 0
print("comparator self-test ok")
