#!/usr/bin/env python3
"""Differential runner: every case in cases_*.py is post-processed by the
pinned reproinx.py oracle and by reproinx (Rust); outputs and exit codes must
match under tests/golden/compare.py. Usage: run.py ORACLE BIN [-k substr]."""
import importlib.util, shutil, subprocess, sys, tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)
    sys.modules[name] = mod
    spec.loader.exec_module(mod)
    return mod


def main(argv):
    synth = load("synth", HERE / "synth.py")
    for p in sorted(HERE.glob("cases_*.py")):
        load(p.stem, p)
    if argv and argv[0] == "--emit":
        # Write every case's input tree to DIR/<name>/out plus DIR/<name>/flags.
        for name, flags, build, _ in synth.CASES:
            d = Path(argv[1]) / name
            shutil.rmtree(d, ignore_errors=True)
            (d / "out").mkdir(parents=True)
            build(d / "out")
            (d / "flags").write_text(flags)
        return 0
    oracle, binary = argv[0], argv[1]
    sel = argv[argv.index("-k") + 1] if "-k" in argv else ""
    compare = load("compare", HERE.parent / "golden" / "compare.py")
    failed = []
    n = 0
    for name, flags, build, divergence in synth.CASES:
        if sel not in name:
            continue
        n += 1
        with tempfile.TemporaryDirectory() as t:
            t = Path(t)
            src = t / "src" / "out"
            src.mkdir(parents=True)
            build(src)
            a, b = t / "a" / "out", t / "b" / "out"
            shutil.copytree(src, a, symlinks=True)
            shutil.copytree(src, b, symlinks=True)
            fl = flags.split()
            ra = subprocess.run([sys.executable, "-I", oracle, "--no-convert", *fl, str(a), str(a)],
                                capture_output=True, text=True)
            rb = subprocess.run([binary, "--no-convert", *fl, str(b), str(b)], capture_output=True, text=True)
            import io, contextlib
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf):
                rc = compare.main(str(a), str(b))
            differs = bool(rc) or ra.returncode != rb.returncode
            if divergence:
                print(f"{'ok  ' if differs else 'FAIL'} {name}: documented divergence {divergence}"
                      + ("" if differs else " no longer differs; update README"))
                if not differs:
                    failed.append(name)
                continue
            if differs:
                failed.append(name)
                print(f"FAIL {name} [{flags}] exit oracle={ra.returncode} rust={rb.returncode}")
                print("  " + buf.getvalue().replace("\n", "\n  ").rstrip())
                if "-v" in argv:
                    print("  oracle stderr:\n    " + ra.stderr.replace("\n", "\n    "))
                    print("  rust stderr:\n    " + rb.stderr.replace("\n", "\n    "))
    print(f"{n - len(failed)}/{n} cases match the oracle")
    return 1 if failed or n == 0 else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
