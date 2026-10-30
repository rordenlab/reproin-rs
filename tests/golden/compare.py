#!/usr/bin/env python3
"""Compare two BIDS trees under the reproin-rs compatibility contract.

Exact: relative paths, text/TSV bytes, unchanged binaries.
Structural: JSON (key order, whitespace, numeric spelling may differ; values may not).
Decompressed: .gz content (gzip container/compressor may differ).
Exit 1 and list every difference; exit 0 when identical under the contract;
exit 2 if the comparison itself fails. Unreadable gzip is a difference.
"""
import gzip, json, os, sys, zlib

def files(root):
    out = {}
    for d, dirs, fs in os.walk(root):
        dirs[:] = [x for x in dirs if x != ".reproinx-undo"]
        for f in fs:
            p = os.path.join(d, f)
            out[os.path.relpath(p, root)] = p
        if not fs and not dirs and d != root:
            out[os.path.relpath(d, root) + "/"] = d
    return out

def same(a, b, rel):
    if rel.endswith("/"):
        return True
    ba, bb = open(a, "rb").read(), open(b, "rb").read()
    if ba == bb:
        return True
    if rel.endswith(".json"):
        try:
            return json.loads(ba) == json.loads(bb)
        except ValueError:
            return False
    if rel.endswith(".gz"):
        try:
            return gzip.decompress(ba) == gzip.decompress(bb)
        except (OSError, EOFError, zlib.error):
            return False
    return False

def main(ref, got):
    r, g = files(ref), files(got)
    diffs = [f"only in reference: {p}" for p in sorted(r.keys() - g.keys())]
    diffs += [f"only in output:    {p}" for p in sorted(g.keys() - r.keys())]
    diffs += [f"content differs:   {p}" for p in sorted(r.keys() & g.keys()) if not same(r[p], g[p], p)]
    for d in diffs:
        print(d)
    return 1 if diffs else 0

if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv[1], sys.argv[2]))
    except Exception as e:  # a broken comparison must never read as a match
        print(f"comparator error: {e!r}", file=sys.stderr)
        sys.exit(2)
