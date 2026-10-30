# Retirement of reproinx.py (M15)

`dcm2niix-reproinx-deprecation.patch` turns `tools/reproinx.py` into a
deprecation stub pointing at `reproinx` (reproin-rs) and updates `REPROIN.md`.
Apply in a dcm2niix checkout only after the CLI, BIDSvue and webapps gates pass
and a reproin-rs release exists:

    git -C ../dcm2niix apply ../reproin-rs/release/dcm2niix-reproinx-deprecation.patch

reproin-rs keeps testing against the pinned oracle (commit 428fabe9) regardless.
