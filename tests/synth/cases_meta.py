"""Metadata differential cases: scans.tsv, events, sbref gradients, B0 pairing,
root scaffolding, participants, task stubs, TaskName backfill, --shift-dates."""
import gzip, struct
from pathlib import Path
from synth import case, js, nii, prov, series, text

BOLD = (8, 8, 4, 10)  # 4D, >= min-volumes: never demoted/reclassified
SHIM = [1, 2, 3, 4]


def hdr(path, dims=(8, 8, 4, 1), endian="<", sizeof=348, qform=None, sform=None,
        pixdim=(1, 2, 2, 2), truncate=None):
    """NIfTI-1 header with explicit qform (b, c, d, ox, oy, oz) / sform (12 floats)."""
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    h = bytearray(352)
    e = endian
    struct.pack_into(e + "i", h, 0, sizeof)
    d = list(dims) + [1] * (4 - len(dims))
    struct.pack_into(e + "8h", h, 40, 4 if d[3] > 1 else 3, *d, 1, 1, 1)
    struct.pack_into(e + "h", h, 72, 16)
    struct.pack_into(e + "8f", h, 76, *pixdim, 1, 1, 1, 1)
    struct.pack_into(e + "f", h, 108, 352.0)
    if qform:
        struct.pack_into(e + "h", h, 252, 1)
        struct.pack_into(e + "6f", h, 256, *qform)
    if sform:
        struct.pack_into(e + "h", h, 254, 1)
        struct.pack_into(e + "12f", h, 280, *sform)
    raw = bytes(h)[:truncate] if truncate else bytes(h)
    path.write_bytes(gzip.compress(raw, mtime=0))


def b0(d, stem, t=None, shim=SHIM, pe="j", dims=None, **extra):
    """Series with the default sform + shim (+AcquisitionTime) — fmap-compatible."""
    side = {"ShimSetting": shim, "PhaseEncodingDirection": pe, **extra}
    if shim is None:
        del side["ShimSetting"]
    if t:
        side["AcquisitionTime"] = t
    series(d, stem, side, dims or ((8, 8, 4, 10) if stem.endswith("_bold") else (8, 8, 4, 1)))


def epi_pair(fm, prefix, t="10:00:00", pes=("j", "j-"), **kw):
    for lab, pe in zip(("AP", "PA"), pes):
        b0(fm, f"{prefix}_dir-{lab}_epi", t, pe=pe, **kw)


# --- scans.tsv ---------------------------------------------------------------

@case("meta_scans_acqtime_sources")
def _(r):
    s = r / "sub-01" / "ses-1"
    series(s / "anat", "sub-01_ses-1_T1w", {"AcquisitionDateTime": "2026-05-12T12:00:00"})
    series(s / "func", "sub-01_ses-1_task-rest_bold", {"AcquisitionDate": "20260512", "AcquisitionTime": "11:00:00.5"}, BOLD)
    series(s / "anat", "sub-01_ses-1_T2w", {"AcquisitionDate": "20260512", "AcquisitionTime": "12:05"})
    series(s / "anat", "sub-01_ses-1_FLAIR", {"AcquisitionDate": "2026-05-12", "AcquisitionTime": "09:00:00"})
    series(s / "anat", "sub-01_ses-1_PDw", {"AcquisitionTime": "08:00:00"})
    series(s / "func", "sub-01_ses-1_task-tie_bold", {"AcquisitionDateTime": "2026-05-12T12:00:00"}, BOLD)
    series(s / "anat", "sub-01_ses-1_acq-adt_T1w", {"AcquisitionDateTime": "", "AcquisitionDate": "20260512", "AcquisitionTime": "07:00:00"})
    series(s / "dwi", "sub-01_ses-1_dwi", {}, (8, 8, 4, 10))


@case("meta_scans_randstr")
def _(r):
    a = r / "sub-01" / "anat"
    series(a, "sub-01_T1w", {"StudyInstanceUID": "9.9", "SeriesInstanceUID": "1.2.3"})
    series(a, "sub-01_T2w", {"SeriesInstanceUID": "1.2.3"})
    series(a, "sub-01_FLAIR", {"StudyInstanceUID": "", "SeriesInstanceUID": ""})
    series(a, "sub-01_PDw", {"StudyInstanceUID": 12345, "SeriesInstanceUID": "1.2"})
    series(a, "sub-01_T2starw", {"StudyInstanceUID": "1.2.840.Ü"})


@case("meta_scans_bidsignore")
def _(r):
    s = r / "sub-01"
    text(r / ".bidsignore", "# comment\n\n  *_bold.nii.gz  \nsub-01/*.nii.gz\nfunc/*\n")
    series(s / "func", "sub-01_task-rest_bold", {"AcquisitionDateTime": "2026-05-12T12:00:00"}, BOLD)
    series(s / "anat", "sub-01_T1w", {"AcquisitionDateTime": "2026-05-12T11:00:00"})
    nii(s / "anat" / "sub-01_T2w.nii", gz=False)
    js(s / "anat" / "sub-01_T2w.json", {"AcquisitionDateTime": "2026-05-12T10:00:00"})


@case("meta_scans_bidsignore_charclass")
def _(r):
    s = r / "sub-01" / "ses-a"
    text(r / ".bidsignore", "sub-01/ses-a/anat/*_T[12]w.nii*\n**/dwi/*\n")
    series(s / "anat", "sub-01_ses-a_T1w", {})
    series(s / "anat", "sub-01_ses-a_T2w", {})
    series(s / "anat", "sub-01_ses-a_FLAIR", {})
    series(s / "dwi", "sub-01_ses-a_dwi", {}, (8, 8, 4, 10))


@case("meta_scans_sessionless_overwrite")
def _(r):
    s = r / "sub-01"
    series(s / "anat", "sub-01_T1w", {"AcquisitionDateTime": "2026-05-12T12:00:00"})
    js(s / "anat" / "sub-01_T2w.json", {"AcquisitionDateTime": "2026-05-12T09:00:00"})  # no image
    text(s / "sub-01_scans.tsv", "filename\tacq_time\ncurated\tx\n")
    js(s / "anat" / "sub-01_PDw.json", {}, raw="{ broken")
    nii(s / "anat" / "sub-01_PDw.nii.gz")


@case("meta_scans_nii_glob_quirks")
def _(r):
    a = r / "sub-01" / "anat"
    js(a / "sub-01_T1w.json", {"AcquisitionDateTime": "2026-05-12T12:00:00"})
    text(a / "sub-01_T1w.nii.bak", "x")  # `.nii*` glob matches it
    series(a, "sub-01_acq-[x]_T2w", {"AcquisitionDateTime": "2026-05-12T11:00:00"})
    js(a / "sub-01_acq-x_T2w.json", {})


@case("meta_scans_hidden_dir")
def _(r):
    s = r / "sub-01"
    series(s / "anat", "sub-01_T1w", {"AcquisitionDateTime": "2026-05-12T12:00:00"})
    series(s / ".ipynb_checkpoints", "sub-01_T2w", {"AcquisitionDateTime": "2026-05-12T11:00:00"})


@case("meta_scans_int_acqdate")  # oracle: TypeError aborts this session's pass
def _(r):
    s = r / "sub-01" / "ses-1"
    series(s / "anat", "sub-01_ses-1_T1w", {"AcquisitionDate": 20260512, "AcquisitionTime": "10:00:00"})
    series(s / "func", "sub-01_ses-1_task-rest_bold", {"AcquisitionDateTime": "2026-05-12T12:00:00"}, BOLD)
    series(r / "sub-01" / "ses-2" / "func", "sub-01_ses-2_task-rest_bold", {"AcquisitionDateTime": "2026-05-13T12:00:00"}, BOLD)


@case("meta_scans_nonobject_json")
def _(r):
    s = r / "sub-01"
    series(s / "anat", "sub-01_T1w", [1, 2])
    series(s / "func", "sub-01_task-rest_bold", {"AcquisitionDateTime": "2026-05-12T12:00:00"}, BOLD)


# --- events / sbref gradients ------------------------------------------------

@case("meta_events")
def _(r):
    f = r / "sub-01" / "ses-1" / "func"
    base = "sub-01_ses-1_task-rest_run-01"
    for e in (1, 2):
        for part in ("mag", "phase"):
            series(f, f"{base}_echo-{e}_part-{part}_bold", {"EchoTime": e / 100}, BOLD)
    nii(f / "sub-01_ses-1_task-motor_bold.nii", BOLD, gz=False)
    js(f / "sub-01_ses-1_task-motor_bold.json", {})
    series(f, "sub-01_ses-1_task-nback_bold", {}, BOLD)
    text(f / "sub-01_ses-1_task-nback_events.tsv", "onset\tduration\n1\t2\n")
    series(r / "sub-01" / "ses-1" / "anat", "sub-01_ses-1_task-x_bold", {}, BOLD)


@case("meta_sbref_gradients")
def _(r):
    d = r / "sub-01" / "dwi"
    series(d, "sub-01_dwi", {}, (8, 8, 4, 10))
    text(d / "sub-01_dwi.bval", "0 1000\n")
    text(d / "sub-01_dwi.bvec", "0 1\n0 0\n0 0\n")
    series(d, "sub-01_sbref", {})
    text(d / "sub-01_sbref.bval", "0\n")
    text(d / "sub-01_sbref.bvec", "0\n0\n0\n")
    nii(d / "sub-01_acq-u_sbref.nii", gz=False)
    js(d / "sub-01_acq-u_sbref.json", {})
    text(d / "sub-01_acq-u_sbref.bvec", "0\n0\n0\n")
    text(d / "sub-01_acq-orphan_sbref.bval", "0\n")


# --- B0FieldIdentifier / B0FieldSource ---------------------------------------

@case("meta_b0_shim_tolerance")
def _(r):
    s = r / "sub-01" / "ses-1"
    epi_pair(s / "fmap", "sub-01_ses-1_acq-se", shim=[1, 2, 3, 100])
    f = s / "func"
    b0(f, "sub-01_ses-1_task-a_bold", "10:01:00", shim=[1, 2, 3, 104.9])
    b0(f, "sub-01_ses-1_task-b_bold", "10:01:00", shim=[1, 2, 3.2, 100])
    b0(f, "sub-01_ses-1_task-c_bold", "10:01:00", shim=[1, 2, 3])
    b0(f, "sub-01_ses-1_task-d_bold", "10:01:00", shim=None)
    b0(f, "sub-01_ses-1_task-e_bold", "10:01:00", shim=[True, 2, 3, 100])
    b0(f, "sub-01_ses-1_task-f_bold", "10:01:00", shim=[0, 2, 3, 100])


@case("meta_b0_shim_zero_and_strings")
def _(r):
    s = r / "sub-01"
    fm, f = s / "fmap", s / "func"
    b0(fm, "sub-01_acq-z_dir-AP_epi", shim=[0, 0], pe="j")
    b0(fm, "sub-01_acq-z_dir-PA_epi", shim=[0, 0], pe="j-")
    b0(f, "sub-01_task-z_bold", shim=[0, 1e-9])
    b0(f, "sub-01_task-y_bold", shim=[0, 1e-7])
    b0(fm, "sub-01_acq-s_dir-AP_epi", shim=["a", "b"], pe="j")
    b0(f, "sub-01_task-s_bold", shim=["a", "b"])
    b0(f, "sub-01_task-t_bold", shim=["a", "B"])
    b0(f, "sub-01_task-u_bold", shim=7)
    b0(fm, "sub-01_acq-u_dir-AP_epi", shim=7.2, pe="j")


@case("meta_b0_shape_affine_tolerance")
def _(r):
    s = r / "sub-01"
    series(s / "fmap", "sub-01_dir-AP_epi", {"ShimSetting": SHIM, "PhaseEncodingDirection": "j"}, (64, 64, 30, 1))
    series(s / "fmap", "sub-01_dir-PA_epi", {"ShimSetting": SHIM, "PhaseEncodingDirection": "j-"}, (64, 64, 30, 1))
    f = s / "func"
    series(f, "sub-01_task-a_bold", {"ShimSetting": SHIM}, (67, 64, 30, 10))
    series(f, "sub-01_task-b_bold", {"ShimSetting": SHIM}, (68, 64, 30, 10))
    series(f, "sub-01_task-c_bold", {"ShimSetting": SHIM}, (64, 64, 30, 10), affine=(2, 0, 0, 10.4, 0, 2, 0, 20, 0, 0, 2, 30))
    series(f, "sub-01_task-d_bold", {"ShimSetting": SHIM}, (64, 64, 30, 10), affine=(2, 0.001, 0, 10, 0, 2, 0, 20, 0, 0, 2, 30))
    series(f, "sub-01_task-e_bold", {"ShimSetting": SHIM}, (64, 64, 30, 10), affine=(-2, 0, 0, 10, 0, 2, 0, 20, 0, 0, 2, 30))


@case("meta_b0_qform_headers")
def _(r):
    s = r / "sub-01"
    fm, f = s / "fmap", s / "func"
    epi_pair(fm, "sub-01_acq-s")  # default sform == identity qform below
    side = {"ShimSetting": SHIM}
    hdr(f / "sub-01_task-q_bold.nii.gz", BOLD, qform=(0, 0, 0, 10, 20, 30))
    js(f / "sub-01_task-q_bold.json", side)
    hdr(f / "sub-01_task-qfac_bold.nii.gz", BOLD, qform=(0, 0, 0, 10, 20, 30), pixdim=(-1, 2, 2, 2))
    js(f / "sub-01_task-qfac_bold.json", side)
    hdr(f / "sub-01_task-qfac0_bold.nii.gz", BOLD, qform=(0, 0, 0, 10, 20, 30), pixdim=(0, 2, 2, 2))
    js(f / "sub-01_task-qfac0_bold.json", side)
    hdr(f / "sub-01_task-none_bold.nii.gz", BOLD)
    js(f / "sub-01_task-none_bold.json", side)
    hdr(f / "sub-01_task-be_bold.nii.gz", BOLD, endian=">", sform=(2, 0, 0, 10, 0, 2, 0, 20, 0, 0, 2, 30))
    js(f / "sub-01_task-be_bold.json", side)
    hdr(f / "sub-01_task-n2_bold.nii.gz", BOLD, sizeof=540, sform=(2, 0, 0, 10, 0, 2, 0, 20, 0, 0, 2, 30))
    js(f / "sub-01_task-n2_bold.json", side)
    hdr(f / "sub-01_task-trunc_bold.nii.gz", BOLD, truncate=300)
    js(f / "sub-01_task-trunc_bold.json", side)
    # sform_code wins over a conflicting qform
    hdr(f / "sub-01_task-both_bold.nii.gz", BOLD, qform=(0, 0, 1, 0, 0, 0), sform=(2, 0, 0, 10, 0, 2, 0, 20, 0, 0, 2, 30))
    js(f / "sub-01_task-both_bold.json", side)


@case("meta_b0_qform_rotation")
def _(r):
    s = r / "sub-01"
    side = {"ShimSetting": SHIM, "PhaseEncodingDirection": "j"}
    q = (0.1, -0.2, 0.3, -90.5, 120.25, -60)
    hdr(s / "fmap" / "sub-01_dir-AP_epi.nii.gz", qform=q, pixdim=(-1, 2.5, 2.5, 3))
    js(s / "fmap" / "sub-01_dir-AP_epi.json", side)
    hdr(s / "fmap" / "sub-01_dir-PA_epi.nii.gz", qform=q, pixdim=(-1, 2.5, 2.5, 3))
    js(s / "fmap" / "sub-01_dir-PA_epi.json", {**side, "PhaseEncodingDirection": "j-"})
    hdr(s / "func" / "sub-01_task-same_bold.nii.gz", BOLD, qform=q, pixdim=(-1, 2.5, 2.5, 3))
    js(s / "func" / "sub-01_task-same_bold.json", side)
    hdr(s / "func" / "sub-01_task-a_bold.nii.gz", BOLD, qform=(0.1, -0.2, 0.31, -90.5, 120.25, -60), pixdim=(-1, 2.5, 2.5, 3))
    js(s / "func" / "sub-01_task-a_bold.json", side)
    hdr(s / "func" / "sub-01_task-b_bold.nii.gz", BOLD, qform=(0.1, -0.2, 0.6, -90.5, 120.25, -60), pixdim=(-1, 2.5, 2.5, 3))
    js(s / "func" / "sub-01_task-b_bold.json", side)
    # |b,c,d| > 1: a clamps to 0
    hdr(s / "func" / "sub-01_task-c_bold.nii.gz", BOLD, qform=(0.8, 0.8, 0.8, 0, 0, 0))
    js(s / "func" / "sub-01_task-c_bold.json", side)
    hdr(s / "fmap" / "sub-01_acq-c_dir-AP_epi.nii.gz", qform=(0.8, 0.8, 0.8, 0, 0, 0))
    js(s / "fmap" / "sub-01_acq-c_dir-AP_epi.json", side)


@case("meta_b0_group_sample_and_ids")
def _(r):
    s = r / "sub-01" / "ses-1"
    fm, f = s / "fmap", s / "func"
    b0(fm, "sub-01_ses-1_dir-AP_epi", pe="j")
    b0(fm, "sub-01_ses-1_dir-PA_epi", pe="j-", shim=[9, 9, 9, 9])  # only group[0] sampled
    b0(f, "sub-01_ses-1_task-rest_bold")
    # phasediff group → prefix "sub-01" → identifier falls back to raw prefix
    s2 = r / "sub-02"
    for suf in ("magnitude1", "magnitude2", "phasediff"):
        b0(s2 / "fmap", f"sub-02_{suf}")
    b0(s2 / "func", "sub-02_task-rest_bold")
    b0(s2 / "anat", "sub-02_T1w")


@case("meta_b0_closest_time")
def _(r):
    s = r / "sub-01"
    fm, f = s / "fmap", s / "func"
    b0(fm, "sub-01_acq-A_dir-AP_epi", "10:00:00")
    b0(fm, "sub-01_acq-A_dir-PA_epi", "10:02:00", pe="j-")  # mean 10:01
    b0(fm, "sub-01_acq-B_dir-AP_epi", "10:10:00")
    b0(fm, "sub-01_acq-C_dir-AP_epi")  # untimed: skipped when target timed
    b0(f, "sub-01_task-a_bold", "10:04:00")
    b0(f, "sub-01_task-b_bold", "10:07:00")
    b0(f, "sub-01_task-tie_bold", "10:05:30")  # equidistant → first sorted
    b0(f, "sub-01_task-u_bold", AcquisitionDateTime="2026-05-12T10:09:00")  # untimed → first
    b0(f, "sub-01_task-bad_bold", "10:09")  # 2-part time → untimed
    b0(f, "sub-01_task-late_bold", "23:59:59.999")


@case("meta_b0_untimed_groups")
def _(r):
    s = r / "sub-01"
    b0(s / "fmap", "sub-01_acq-A_dir-AP_epi")
    b0(s / "fmap", "sub-01_acq-B_dir-AP_epi")
    b0(s / "func", "sub-01_task-a_bold", "10:00:00")  # all groups untimed → None
    b0(s / "func", "sub-01_task-b_bold")  # untimed target → first


@case("meta_b0_sbref_mirror")
def _(r):
    fmA, fmB = "acq-A_dir-AP_epi", "acq-B_dir-AP_epi"
    for ses in ("1", "2", "3", "4"):
        s = r / "sub-x" / f"ses-{ses}"
        b0(s / "fmap", f"sub-x_ses-{ses}_{fmA}", "10:00:00")
        b0(s / "fmap", f"sub-x_ses-{ses}_{fmB}", "10:10:00")
    f = r / "sub-x" / "ses-1" / "func"  # bold timed → bold wins
    b0(f, "sub-x_ses-1_task-t_bold", "10:01:00")
    b0(f, "sub-x_ses-1_task-t_sbref", "10:09:00")
    f = r / "sub-x" / "ses-2" / "func"  # bold untimed → timed sbref wins
    b0(f, "sub-x_ses-2_task-t_bold", AcquisitionDateTime="2026-01-01T10:01:00")
    b0(f, "sub-x_ses-2_task-t_sbref", "10:09:00")
    f = r / "sub-x" / "ses-3" / "func"  # neither timed; sbref incompatible keeps bold match
    b0(f, "sub-x_ses-3_task-t_bold")
    b0(f, "sub-x_ses-3_task-t_sbref", shim=[9, 9, 9, 9])
    f = r / "sub-x" / "ses-4" / "func"  # neither timed; bold incompatible → sbref's
    b0(f, "sub-x_ses-4_task-t_bold", shim=[9, 9, 9, 9])
    b0(f, "sub-x_ses-4_task-t_sbref")
    b0(f, "sub-x_ses-4_task-lone_sbref", "10:09:00")  # no bold sibling: independent
    b0(f, "sub-x_ses-4_task-t_noRF", "10:09:00")


@case("meta_b0_same_polarity_stale")
def _(r):
    s = r / "sub-x" / "ses-1"
    fm, f = s / "fmap", s / "func"
    for lab in ("AP", "PA"):
        b0(fm, f"sub-x_ses-1_dir-{lab}_epi", "10:00:00", pe="j", B0FieldIdentifier=["epi", "old"])
    b0(fm, "sub-x_ses-1_acq-x_dir-AP_epi", "10:00:00", shim=[5, 5, 5, 5])
    b0(f, "sub-x_ses-1_task-t_bold", "10:01:00", B0FieldSource="epi")
    b0(f, "sub-x_ses-1_task-u_bold", "10:02:00", B0FieldSource="other")
    b0(f, "sub-x_ses-1_task-v_bold", "10:02:00", B0FieldSource=["epi", "old", "acq-x_epi"])
    b0(f, "sub-x_ses-1_task-w_bold", "10:02:00", B0FieldSource=["old"])


@case("meta_b0_same_polarity_variants")
def _(r):
    s = r / "sub-01"
    fm = s / "fmap"
    epi_pair(fm, "sub-01_acq-missing", pes=("j", None))  # one PE missing → kept
    epi_pair(fm, "sub-01_acq-int", pes=(1, 1))  # non-str PE → kept
    epi_pair(fm, "sub-01_acq-null", pes=(None, None))  # {None} → kept
    epi_pair(fm, "sub-01_acq-same", "09:00:00", pes=("i", "i"))  # dropped
    b0(s / "func", "sub-01_task-rest_bold", "09:00:00")


@case("meta_b0_rerun_and_malformed_fmap")
def _(r):
    s = r / "sub-01" / "ses-1"
    b0(s / "fmap", "sub-01_ses-1_dir-AP_epi", B0FieldIdentifier="stale")
    b0(s / "func", "sub-01_ses-1_task-a_bold", shim=[9, 9, 9, 9], B0FieldSource="stale")
    b0(s / "func", "sub-01_ses-1_task-b_bold", B0FieldSource="dir-AP_epi")
    s2 = r / "sub-01" / "ses-2"  # malformed fmap json aborts this session's B0 pass
    series(s2 / "fmap", "sub-01_ses-2_dir-AP_epi", None)
    js(s2 / "fmap" / "sub-01_ses-2_dir-AP_epi.json", None, raw="{oops")
    b0(s2 / "fmap", "sub-01_ses-2_acq-b_dir-AP_epi")
    b0(s2 / "func", "sub-01_ses-2_task-a_bold", AcquisitionDateTime="2026-01-01T10:00:00")


# --- root scaffolding --------------------------------------------------------

@case("meta_root_fresh_version")
def _(r):
    series(r / "sub-02" / "anat", "sub-02_T1w", {"ConversionSoftwareVersion": "v1.0.2"})
    series(r / "sub-01" / "anat", "sub-01_T1w", {"ConversionSoftwareVersion": " v1.0.1 "})
    js(r / "derivatives" / "scanner" / "sub-01" / "anat" / "sub-01_scout.json", {"ConversionSoftwareVersion": "DERIV"})
    js(r / "participants.json", {"ConversionSoftwareVersion": "PJ"})


@case("meta_root_version_rules")
def _(r):
    a = lambda n: r / f"sub-0{n}" / "anat"
    series(a(1), "sub-01_T1w", {"ConversionSoftware": "other", "ConversionSoftwareVersion": "OTHER"})
    series(a(2), "sub-02_T1w", {"ConversionSoftware": "", "ConversionSoftwareVersion": 5})
    series(a(3), "sub-03_T1w", {"ConversionSoftwareVersion": "   "})
    series(a(4), "sub-04_T1w", [1])
    series(a(5), "sub-05_T1w", {"ConversionSoftware": "", "ConversionSoftwareVersion": "EMPTYSW"})
    series(a(6), "sub-06_T1w", {"ConversionSoftware": "dcm2niix", "ConversionSoftwareVersion": "LATE"})


@case("meta_root_version_path_order")
def _(r):
    # Path sorts part-wise: sub-01/... < sub-01-extra.json (string sort disagrees)
    series(r / "sub-01" / "anat", "sub-01_T1w", {"ConversionSoftwareVersion": "DIR"})
    js(r / "sub-01-extra.json", {"ConversionSoftwareVersion": "FILE"})


@case("meta_root_no_version")
def _(r):
    series(r / "sub-01" / "anat", "sub-01_T1w", {"ConversionSoftware": "dcm2niix"})


@case("meta_root_dummy_ddesc_kept_files")
def _(r):
    series(r / "sub-01" / "anat", "sub-01_T1w", {"ConversionSoftwareVersion": "v1.0.20250505"})
    js(r / "dataset_description.json", {"Name": "dcm2niix dummy dataset", "BIDSVersion": "1.0.0", "Extra": 1})
    text(r / "CHANGES", "curated changes\n")
    text(r / "README", "curated readme")
    js(r / "scans.json", {"filename": {"Description": "mine"}})


@case("meta_root_curated_ddesc_backfill")
def _(r):
    series(r / "sub-01" / "anat", "sub-01_T1w", {"ConversionSoftwareVersion": "v1"})
    js(r / "dataset_description.json", {"Name": "My study", "GeneratedBy": [{"Name": "me"}], "BIDSVersion": "1.9.0"})


@case("meta_root_curated_ddesc_complete")
def _(r):
    series(r / "sub-01" / "anat", "sub-01_T1w", {"ConversionSoftwareVersion": "v1"})
    js(r / "dataset_description.json", None, raw='{"Name":"x","DatasetType":"raw","GeneratedBy":[],"SourceDatasets":[]}')


@case("meta_root_ddesc_list")
def _(r):
    series(r / "sub-01" / "anat", "sub-01_T1w", {})
    js(r / "dataset_description.json", None, raw='["not", "a", "dict"]')


@case("meta_root_ddesc_malformed")
def _(r):
    series(r / "sub-01" / "anat", "sub-01_T1w", {})
    js(r / "dataset_description.json", None, raw='{"Name": ')


@case("meta_root_two_studies")
def _(r):
    series(r / "StudyA" / "sub-01" / "anat", "sub-01_T1w", {"ConversionSoftwareVersion": "A"})
    series(r / "StudyB" / "sub-02" / "func", "sub-02_task-rest_bold", {"ConversionSoftwareVersion": "B"}, BOLD)


# --- participants ------------------------------------------------------------

def _subs(r, *names):
    for n in names:
        series(r / n / "anat", f"{n}_T1w", {})


@case("meta_part_fresh_provenance")
def _(r):
    _subs(r, "sub-01", "sub-02", "sub-03", "sub-04", "sub-05", "sub-06")
    prov(r, [
        {"OutputStem": "sub-01/anat/sub-01_T1w", "ProtocolName": "anat-T1w", "PatientAge": "026Y", "PatientSex": "M", "StudyDate": "20260512", "StudyTime": "120000"},
        {"OutputStem": "sub-02/anat/sub-02_T1w", "ProtocolName": "anat-T1w", "PatientAge": "026M", "PatientSex": "X", "StudyDate": "20250101", "StudyTime": "080000"},
        {"OutputStem": "sub-02/anat/sub-02_T2w", "ProtocolName": "anat-T2w", "PatientAge": "040Y", "PatientSex": "F", "StudyDate": "20240101"},
        {"OutputStem": "sub-03/anat/sub-03_T1w", "ProtocolName": "anat-T1w", "PatientAge": "000Y", "PatientSex": "O"},
        {"OutputStem": "out/sub-04/anat/sub-04_T1w", "ProtocolName": "anat-T1w", "PatientAge": " 7Y ", "PatientSex": " F ", "StudyTime": "090000"},
        {"OutputStem": "sub-05_T1w", "ProtocolName": "anat-T1w", "PatientAge": "30Y"},
        {"OutputStem": "sub-07/anat/sub-07_T1w", "ProtocolName": "anat-T1w", "PatientAge": "50Y", "StudyDate": "19990101"},
    ])


@case("meta_part_default_merge")
def _(r):
    _subs(r, "sub-01", "sub-02")
    text(r / "participants.tsv", "participant_id\tage\tsex\tgroup\nsub-01\t30\tF\tpatient\n\n")
    js(r / "participants.json", {"curated": True})


@case("meta_part_default_no_trailing_newline")
def _(r):
    _subs(r, "sub-01", "sub-02")
    text(r / "participants.tsv", "participant_id\tage\tsex\tgroup\nsub-01\t30\tF\tpatient")


@case("meta_part_default_all_listed")
def _(r):
    _subs(r, "sub-01")
    text(r / "participants.tsv", "participant_id\tage\tsex\tgroup\nsub-01\t30\tF\tpatient")


@case("meta_part_curated_crlf")
def _(r):
    _subs(r, "sub-01", "sub-02", "sub-03")
    (r / "participants.tsv").write_bytes(b"participant_id\tage\tsex\thandedness\tgroup\r\nsub-01\t30\tF\tR\tpatient\r\n")
    prov(r, [{"OutputStem": "sub-03/anat/sub-03_T1w", "ProtocolName": "anat-T1w", "PatientAge": "045Y", "PatientSex": "M"}])


@case("meta_part_curated_no_trailing_newline")
def _(r):
    _subs(r, "sub-01", "sub-02")
    text(r / "participants.tsv", "participant_id\tsex\nsub-01\tF")


@case("meta_part_crlf_default_header")
def _(r):
    _subs(r, "sub-01", "sub-02")
    (r / "participants.tsv").write_bytes(b"participant_id\tage\tsex\tgroup\r\nsub-01\tn/a\tn/a\tcontrol\r\n")


@case("meta_part_empty_file")
def _(r):
    _subs(r, "sub-02", "sub-01")
    text(r / "participants.tsv", "")


@case("meta_part_odd_entries")
def _(r):
    _subs(r, "sub-01")
    text(r / "sub-02", "file not dir")
    (r / "sub-.hidden").mkdir()


# --- task stubs / TaskName ---------------------------------------------------

@case("meta_task_stubs")
def _(r):
    f = r / "sub-01" / "func"
    series(f, "sub-01_task-rest_acq-a_bold", {"TaskName": ""}, BOLD)
    series(f, "sub-01_task-rest_acq-b_bold", {"TaskName": None}, BOLD)
    series(f, "sub-01_task-motor_bold", {"TaskName": "custom"}, BOLD)
    series(f, "sub-01_task-nojson_bold", None, BOLD)
    (f / "sub-01_task-nojson_bold.json").unlink()
    series(f, "sub-01_task-list_bold", [1], BOLD)
    js(r / "task-rest_bold.json", {"TaskName": "rest", "CogAtlasID": "x"})
    js(r / "task-motor_acq-y_bold.json", {"TaskName": "motor"})
    js(r / "task-motor_acq-z_bold.json", {"TaskName": "motor", "RepetitionTime": 2})
    js(r / "task-motor_run-01_bold.json", {"TaskName": "motor"})
    js(r / "task-motor_bold.json", {"TaskName": "motor", "RepetitionTime": 1})


@case("meta_task_bare_preserved")
def _(r):
    f = r / "sub-01" / "func"
    series(f, "sub-01_task-rest_acq-a_bold", {}, BOLD)
    series(f, "sub-01_task-nback_acq-a_bold", {}, BOLD)
    series(f, "sub-01_task-go_acq-a_bold", {}, BOLD)
    js(r / "task-rest_bold.json", {"TaskName": "rest", "RepetitionTime": 2})
    js(r / "task-nback_bold.json", {"TaskName": "other"})
    js(r / "task-go_bold.json", None, raw="{bad")
    js(r / "task-orphan_bold.json", {})
    js(r / "task-orphan2_bold.json", ["x"])


@case("meta_task_stem_regex", "--keep-derivatives")
def _(r):
    f = r / "sub-01" / "func"
    series(f, "sub-01_task-rest_run-01_acq-x_bold", {}, BOLD)
    series(f, "sub-01_task-a_acq-b_c_bold", {}, BOLD)
    nii(f / "sub-01_task-unc_bold.nii", BOLD, gz=False)
    js(f / "sub-01_task-unc_bold.json", {})
    series(r / "derivatives" / "scanner" / "sub-01" / "func", "sub-01_task-deriv_acq-q_bold", {}, BOLD)
    series(r / "sub-01" / "anat", "sub-01_task-anat_bold", {}, BOLD)


@case("meta_task_sourcedata_bold")
def _(r):
    series(r / "sub-01" / "func", "sub-01_task-rest_bold", {}, BOLD)
    series(r / "sourcedata" / "sub-01" / "func", "sub-01_task-src_bold", {}, BOLD)


@case("meta_readme_md_stub")
def _(r):
    _subs(r, "sub-01")
    text(r / "README.md", "Generated using dcm2niix (v1.0)\n\nDescribe your dataset here.\n")


@case("meta_readme_md_curated")
def _(r):
    _subs(r, "sub-01")
    text(r / "README.md", "My curated README. Describe your dataset here.\n")
    text(r / "README", "curated")


@case("meta_readme_md_large_stub")
def _(r):
    _subs(r, "sub-01")
    text(r / "README.md", "Generated using dcm2niix\nDescribe your dataset here\n" + "x" * 1024)


@case("meta_taskname_backfill", "--keep-derivatives")
def _(r):
    f = r / "sub-01" / "func"
    series(f, "sub-01_task-rest_bold", {}, BOLD)
    series(f, "sub-01_task-rest_sbref", {})
    series(f, "sub-01_task-rest_acq-x_sbref", {"TaskName": "kept"})
    series(f, "sub-01_task-rest_acq-y_sbref", {"TaskName": ""})
    js(f / "sub-01_task-rest_recording-card_physio.json", {"SamplingFrequency": 100})
    text(f / "sub-01_task-rest_recording-card_physio.tsv.gz", gzip.compress(b"1\n", mtime=0), binary=True)
    js(f / "sub-01_task-bad_physio.json", None, raw="nope")
    series(r / "sub-01" / "dwi", "sub-01_sbref", {})
    series(r / "derivatives" / "scanner" / "sub-01" / "func", "sub-01_task-rest_sbref", {})


# --- --shift-dates -----------------------------------------------------------

SD = "--shift-dates"


@case("meta_shift_two_subjects", SD)
def _(r):
    for sub, d1, d2 in (("sub-01", "2023-09-14", "2023-09-21"), ("sub-02", "2026-02-28", "2026-03-01")):
        series(r / sub / "ses-1" / "anat", f"{sub}_ses-1_T1w", {"AcquisitionDateTime": f"{d1}T10:00:00", "AcquisitionDate": d1.replace("-", ""), "AcquisitionTime": "10:00:00"})
        series(r / sub / "ses-2" / "func", f"{sub}_ses-2_task-rest_bold", {"AcquisitionDateTime": f"{d2}T09:30:00.250000"}, BOLD)
        series(r / sub / "ses-2" / "anat", f"{sub}_ses-2_T1w", {"AcquisitionDateTime": f"{d1}T23:59:59"})


@case("meta_shift_already_anchored", SD)
def _(r):
    series(r / "sub-01" / "anat", "sub-01_T1w", {"AcquisitionDateTime": "1925-01-01T10:00:00"})
    series(r / "sub-02" / "anat", "sub-02_T1w", {"AcquisitionDateTime": "1925-01-01T10:00:00"})
    series(r / "sub-02" / "func", "sub-02_task-rest_bold", {"AcquisitionDateTime": "2026-05-12T10:00:00"}, BOLD)


@case("meta_shift_datetime_formats", SD)
def _(r):
    a = r / "sub-01" / "anat"
    for i, v in enumerate(["2026-05-12T12:00", "2026-05-12T12:00:01.5", "2026-05-12T12:00:01.1234567",
                           "2026-05-12T12:00:00.000", "20260512T120000", "2026-05-12 08:00:00", "2026-05-12T12"]):
        series(a, f"sub-01_acq-f{i}_T1w", {"AcquisitionDateTime": v})
    for i, v in enumerate(["n/a", "garbage", "2026-13-01T00:00:00"]):
        series(a, f"sub-01_acq-bad{i}_T2w", {"AcquisitionDateTime": v})


@case("meta_shift_utc_z", SD)
def _(r):
    a = r / "sub-01" / "anat"
    series(a, "sub-01_T1w", {"AcquisitionDateTime": "2026-05-12T12:00Z"})
    series(a, "sub-01_T2w", {"AcquisitionDateTime": "2026-05-14T12:00:00+02:00"})


@case("meta_shift_int_adt", SD)  # int ADT: scans sort TypeError, fromisoformat("20260512") parses
def _(r):
    a = r / "sub-01" / "anat"
    series(a, "sub-01_T1w", {"AcquisitionDateTime": 20260512})
    series(a, "sub-01_T2w", {"AcquisitionDateTime": "2026-05-13T08:00:00"})


@case("meta_shift_tz_only", SD)
def _(r):
    a = r / "sub-01" / "anat"
    series(a, "sub-01_T1w", {"AcquisitionDateTime": "2026-05-12T23:30:00-05:00"})
    series(a, "sub-01_T2w", {"AcquisitionDateTime": "2026-05-13T01:00:00+09:00"})


@case("meta_shift_mixed_tz_fatal", SD)
def _(r):
    a = r / "sub-01" / "anat"
    series(a, "sub-01_T1w", {"AcquisitionDateTime": "2026-05-12T10:00:00+02:00"})
    series(a, "sub-01_T2w", {"AcquisitionDateTime": "2026-05-12T11:00:00"})


@case("meta_shift_acqdate_variants", SD)
def _(r):
    a = r / "sub-01" / "anat"
    series(a, "sub-01_T1w", {"AcquisitionDate": "20230914"})
    series(a, "sub-01_T2w", {"AcquisitionDate": "20230915", "AcquisitionTime": "10:00:00"})
    series(a, "sub-01_FLAIR", {"AcquisitionDate": "20231340"})
    series(a, "sub-01_PDw", {"AcquisitionDate": "2023091a", "AcquisitionDateTime": "2023-09-16T01:00:00"})
    series(a, "sub-01_T2starw", {"AcquisitionDate": "20240229"})


@case("meta_shift_scans_only", SD)
def _(r):
    # anat has no sidecar → _write_scans_tsv leaves the curated TSVs alone
    s = r / "sub-01" / "ses-1"
    nii(s / "anat" / "sub-01_ses-1_T1w.nii.gz")
    text(s / "sub-01_ses-1_scans.tsv",
         "acq_time\tfilename\textra\n2023-09-14T08:00:00\tanat/sub-01_ses-1_T1w.nii.gz\tz\n"
         "n/a\tanat/x.nii.gz\n\nshort\n2023-09-20T08:00:00.5\tanat/y.nii.gz\tq\n\n")
    s2 = r / "sub-01" / "ses-2"
    nii(s2 / "anat" / "sub-01_ses-2_T1w.nii.gz")
    text(s2 / "sub-01_ses-2_scans.tsv", "filename\tno_time\nanat/a.nii.gz\t2023-09-14T08:00:00\n")
    text(r / "sub-01" / "ses-3" / "sub-01_ses-3_scans.tsv", "filename\tacq_time\n")
    nii(r / "sub-01" / "ses-3" / "anat" / "sub-01_ses-3_T1w.nii.gz")


@case("meta_shift_keep_derivatives", "--keep-derivatives --shift-dates")
def _(r):
    series(r / "sub-01" / "anat", "sub-01_T1w", {"AcquisitionDateTime": "2023-09-14T10:00:00"})
    d = r / "derivatives" / "scanner" / "sub-01" / "anat"
    js(d / "sub-01_scout.json", {"AcquisitionDateTime": "2023-09-14T09:00:00", "AcquisitionDate": "20230914"})
    js(r / "derivatives" / "scanner" / "sub-99" / "anat" / "sub-99_scout.json", {"AcquisitionDateTime": "2020-01-05T09:00:00"})
    text(r / "derivatives" / "scanner" / "sub-01" / "sub-01_scans.tsv", "filename\tacq_time\r\nanat/sub-01_scout.nii.gz\t2023-09-15T09:00:00\r\n")


@case("meta_shift_drop_derivatives", SD)
def _(r):
    series(r / "sub-01" / "anat", "sub-01_T1w", {"AcquisitionDateTime": "2023-09-14T10:00:00"})
    js(r / "derivatives" / "scanner" / "sub-01" / "anat" / "sub-01_scout.json", {"AcquisitionDateTime": "2023-09-14T09:00:00"})


@case("meta_shift_provenance_removed", SD)
def _(r):
    series(r / "sub-01" / "anat", "sub-01_T1w", {"AcquisitionDateTime": "2023-09-14T10:00:00"})
    prov(r, [{"OutputStem": "sub-01/anat/sub-01_T1w", "ProtocolName": "anat-T1w", "StudyDate": "20230914", "PatientID": "REAL", "PatientAge": "030Y"}])
    text(r / ".reproin_provenance.tsv.bak", "StudyDate\n20230914\n")
    text(r / "sub-01" / ".reproin_provenance.tsv", "StudyDate\n20230914\n")


@case("meta_shift_no_dates", SD)
def _(r):
    series(r / "sub-01" / "anat", "sub-01_T1w", {"AcquisitionTime": "10:00:00"})
    prov(r, [{"OutputStem": "sub-01/anat/sub-01_T1w", "ProtocolName": "anat-T1w", "StudyDate": "20230914"}])


@case("meta_shift_unknown_subjectless", SD)
def _(r):
    u = r / "Unknown"
    js(u / "1_ct.json", {"AcquisitionDateTime": "2023-09-14T10:00:00", "Modality": "CT"})
    text(u / "1_ct_scans.tsv", "filename\tacq_time\r\nct/1_ct.nii.gz\t2023-09-14T10:00:00\r\n")
    js(u / "sub-real_ct.json", {"AcquisitionDateTime": "2021-03-04T10:00:00", "Modality": "CT"})
    prov(r, [{"OutputStem": "Unknown/1_ct", "StudyDate": "20230914", "PatientID": "REAL"}])


@case("meta_shift_unknown_beside_subject", SD)
def _(r):
    series(r / "sub-01" / "anat", "sub-01_T1w", {"AcquisitionDateTime": "2023-09-14T10:00:00"})
    js(r / "Unknown" / "sub-01_ct.json", {"AcquisitionDateTime": "2023-09-20T10:00:00", "Modality": "CT"})
    nii(r / "Unknown" / "sub-01_ct.nii.gz")


@case("meta_shift_fatal_sidecar", SD)
def _(r):
    series(r / "sub-01" / "anat", "sub-01_T1w", {"AcquisitionDateTime": "2023-09-14T10:00:00"})
    js(r / "sub-01" / "anat" / "sub-01_T2w.json", None, raw="{ this is not valid json")
    nii(r / "sub-01" / "anat" / "sub-01_T2w.nii.gz")
    prov(r, [{"OutputStem": "sub-01/anat/sub-01_T1w", "ProtocolName": "anat-T1w", "StudyDate": "20230914"}])


@case("meta_shift_fatal_ddesc", SD)
def _(r):
    series(r / "sub-01" / "anat", "sub-01_T1w", {"AcquisitionDateTime": "2023-09-14T10:00:00"})
    js(r / "dataset_description.json", None, raw='{"Name": ')


@case("meta_shift_nonobject_sidecar", SD)
def _(r):
    series(r / "sub-01" / "anat", "sub-01_T1w", {"AcquisitionDateTime": "2023-09-14T10:00:00"})
    series(r / "sub-01" / "anat", "sub-01_T2w", ["2020-01-01T00:00:00"])


@case("meta_shift_timestamp_session", SD)
def _(r):
    s = r / "sub-01" / "ses-20230914T103000"
    series(s / "anat", "sub-01_ses-20230914T103000_T1w", {"AcquisitionDateTime": "2023-09-14T10:30:00"})


@case("meta_shift_pre_reference", SD)
def _(r):
    series(r / "sub-01" / "anat", "sub-01_T1w", {"AcquisitionDateTime": "1900-06-01T10:00:00", "AcquisitionDate": "19000601"})
    series(r / "sub-01" / "func", "sub-01_task-rest_bold", {"AcquisitionDateTime": "1900-06-03T10:00:00"}, BOLD)


def _exotic(r, vals, anchor="2026-05-01T00:00:00"):
    a = r / "sub-01" / "anat"
    series(a, "sub-01_T1w", {"AcquisitionDateTime": anchor})
    for i, v in enumerate(vals):
        series(a, f"sub-01_acq-x{i}_T2w", {"AcquisitionDateTime": v})


# Python 3.11+ fromisoformat corner cases (interpreter-dependent oracle behaviour)
@case("meta_shift_iso_mainstream", SD)
def _(r):
    _exotic(r, ["2026-05-12T12:00:00,5", "2026-05-12T1200", "2026-05-12X12:00", "2026-05-12T12:00:00.1234567890",
                "2026-05-12T", "2026-05-12T120000.5", "2026-05-12T12:0000", "2026-05-12T1200:00",
                "2026-0512", "202605-12", "2026-05-12T24:00:01"])





@case("meta_shift_iso_tz_mainstream", SD)
def _(r):
    _exotic(r, ["2026-05-12T12:00:00+02", "2026-05-12T12:00:00+23:59", "2026-05-12T12:00:00.5Z",
                "2026-05-12T12:00:00+0200", "2026-05-12T12:00:00+02:00.5", "2026-05-12T12:00:00-00:00",
                "2026-05-12T12:00:00z"], anchor="2026-05-01T00:00:00+00:00")



@case("meta_shift_int_acqdate_fatal", SD)  # oracle: _acq_iso TypeError → exit 1
def _(r):
    series(r / "sub-01" / "anat", "sub-01_T1w", {"AcquisitionDate": 20230914, "AcquisitionTime": "10:00:00"})


@case("meta_participants_formfeed_line_break")
def _(r):
    # Python splitlines() also breaks at \x0c / \x1c; the merge must agree.
    series(r / "sub-01" / "anat", "sub-01_T1w", {})
    series(r / "sub-02" / "anat", "sub-02_T1w", {})
    text(r / "participants.tsv", "participant_id\tage\tsex\tgroup\nsub-01\tn/a\tn/a\tx\x0cextra\x1cmore\n")
