"""`_bidsguess_cleanup` family: reclassification (R1/R2), orphan runs, discard/,
3D bold demote, part entities + phase Units, .bidsignore sweeps, __dup naming."""
import gzip
from pathlib import Path

from synth import case, js, nii, prov, series, text

SIE = "Siemens"


def nii_dim0(path, dim0, dims):
    """Header whose dim[0] disagrees with dim[1..4] (e.g. dim0=3 but dim4=10)."""
    nii(path, dims)
    raw = bytearray(gzip.decompress(Path(path).read_bytes()))
    raw[40:42] = dim0.to_bytes(2, "little", signed=True)
    Path(path).write_bytes(gzip.compress(bytes(raw), mtime=0))


def long_bold(d, stem="sub-01_task-rest_bold", n=10, **extra):
    series(d, stem, {"PhaseEncodingDirection": "j", **extra}, (64, 64, 30, n))


# --- R1: short EPI -> fmap/_dir-<L>_epi --------------------------------------

@case("cleanup_r1_short_dwi_to_epi")
def _(r):
    s = r / "sub-01"
    long_bold(s / "func")
    series(s / "dwi", "sub-01_dwi", {"PhaseEncodingDirection": "j-"}, (64, 64, 30, 2))
    text(s / "dwi" / "sub-01_dwi.bval", "0 0\n")
    text(s / "dwi" / "sub-01_dwi.bvec", "0 0\n0 0\n0 0\n")


@case("cleanup_r1_short_bold_events_dropped")
def _(r):
    f = r / "sub-01" / "ses-1" / "func"
    long_bold(f, "sub-01_ses-1_task-a_bold")
    series(f, "sub-01_ses-1_task-b_bold", {"PhaseEncodingDirection": "j"}, (64, 64, 30, 3))
    text(f / "sub-01_ses-1_task-b_events.tsv", "onset\tduration\n")
    text(f / "sub-01_ses-1_task-b_bold.tsv", "x\n")
    text(f / "sub-01_ses-1_task-a_events.tsv", "onset\tduration\n1\t2\n")


@case("cleanup_r1_existing_dir_and_orphan_run")
def _(r):
    s = r / "sub-01"
    series(s / "dwi", "sub-01_dir-AP_run-1_dwi", {"PhaseEncodingDirection": "j-"}, (64, 64, 30, 30))
    series(s / "dwi", "sub-01_dir-PA_run-2_dwi", {"PhaseEncodingDirection": "j"}, (64, 64, 30, 1))
    for st in ("sub-01_dir-AP_run-1_dwi", "sub-01_dir-PA_run-2_dwi"):
        text(s / "dwi" / f"{st}.bval", "0\n")
        text(s / "dwi" / f"{st}.bvec", "0\n0\n0\n")


@case("cleanup_r1_multiecho_skip")
def _(r):
    f = r / "sub-01" / "func"
    long_bold(f)
    for e in (1, 2):
        series(f, f"sub-01_task-pe_echo-{e}_bold", {"PhaseEncodingDirection": "j-"}, (64, 64, 30, 3))
    series(f, "sub-01_task-one_echo-1_bold", {"PhaseEncodingDirection": "j-"}, (64, 64, 30, 1))


@case("cleanup_r1_missing_pe_then_demote")
def _(r):
    f = r / "sub-01" / "func"
    long_bold(f)
    series(f, "sub-01_task-a_bold", {"EchoTime": 0.03}, (64, 64, 30, 3))
    series(f, "sub-01_task-b_bold", {"PhaseEncodingDirection": None}, (64, 64, 30, 1))
    text(f / "sub-01_task-b_bold_events.tsv", "onset\n")
    nii(f / "sub-01_task-c_bold.nii.gz", (64, 64, 30, 2))  # no sidecar at all


@case("cleanup_r1_pe_value_variants")
def _(r):
    s = r / "sub-01"
    long_bold(s / "func")
    pes = {"i": "i", "kn": "k-", "sp": " j+ ", "x": "x", "num": 5, "lst": ["j"],
           "bool": True, "empty": "", "upper": "J-", "zero": 0, "obj": {"a": 1}}
    for acq, pe in pes.items():
        series(s / "dwi", f"sub-01_acq-{acq}_dwi", {"PhaseEncodingDirection": pe}, (64, 64, 30, 2))


@case("cleanup_r1_no_long_sibling")
def _(r):
    s = r / "sub-01"
    series(s / "func", "sub-01_task-a_bold", {"PhaseEncodingDirection": "j"}, (64, 64, 30, 4))
    series(s / "dwi", "sub-01_dwi", {"PhaseEncodingDirection": "j-"}, (64, 64, 30, 3))
    series(s / "anat", "sub-01_T1w", {}, (64, 64, 30, 50))  # long, but not func/dwi


@case("cleanup_r1_min_volumes_low", "-N 3")
def _(r):
    s = r / "sub-01"
    series(s / "func", "sub-01_task-a_bold", {"PhaseEncodingDirection": "j"}, (64, 64, 30, 4))
    series(s / "func", "sub-01_task-b_bold", {"PhaseEncodingDirection": "j-"}, (64, 64, 30, 2))
    series(s / "dwi", "sub-01_dwi", {"PhaseEncodingDirection": "i"}, (64, 64, 30, 3))


@case("cleanup_r1_min_volumes_high", "--min-volumes=12")
def _(r):
    s = r / "sub-01"
    series(s / "func", "sub-01_task-a_bold", {"PhaseEncodingDirection": "j"}, (64, 64, 30, 10))
    series(s / "dwi", "sub-01_dwi", {"PhaseEncodingDirection": "j-"}, (64, 64, 30, 12))


@case("cleanup_r1_min_volumes_one", "-N1")
def _(r):
    f = r / "sub-01" / "func"
    long_bold(f)
    series(f, "sub-01_task-b_bold", {"PhaseEncodingDirection": "j-"}, (64, 64, 30, 1))


@case("cleanup_r1_min_volumes_zero_negative", "-N -2")
def _(r):
    f = r / "sub-01" / "func"
    series(f, "sub-01_task-a_bold", {"PhaseEncodingDirection": "j"}, (64, 64, 30, 1))


@case("cleanup_r1_fmap_target_collision")
def _(r):
    s = r / "sub-01"
    long_bold(s / "func")
    series(s / "func", "sub-01_task-a_bold", {"PhaseEncodingDirection": "j"}, (64, 64, 30, 3))
    series(s / "func", "sub-01_task-b_bold", {"PhaseEncodingDirection": "j"}, (64, 64, 30, 3))
    js(s / "fmap" / "sub-01_dir-Jn_epi.json", {"curated": True})
    series(s / "func", "sub-01_task-c_bold", {"PhaseEncodingDirection": "j-"}, (64, 64, 30, 3))


@case("cleanup_r1_entity_order_and_extras")
def _(r):
    s = r / "sub-01" / "ses-2"
    long_bold(s / "func", "sub-01_ses-2_task-rest_bold")
    for st in ("sub-01_ses-2_task-x_acq-fast_foo-bar_run-2_ce-gd_bold",
               "sub-01_ses-2_task-y_junk_acq-a-b_acq-c_bold",
               "sub-01_ses-2_task-z_acq-_rec-x_bold"):
        series(s / "func", st, {"PhaseEncodingDirection": "i-"}, (64, 64, 30, 2))


# cleanup_r1_nii_and_niigz_same_stem omitted: reproinx.py picks an unsorted glob match (README D1).

@case("cleanup_r1_dim0_disagrees")
def _(r):
    f = r / "sub-01" / "func"
    long_bold(f)
    nii_dim0(f / "sub-01_task-b_bold.nii.gz", 3, (64, 64, 30, 10))  # dim0<4 -> 1 vol
    js(f / "sub-01_task-b_bold.json", {"PhaseEncodingDirection": "j-"})
    nii_dim0(f / "sub-01_task-c_bold.nii.gz", 4, (64, 64, 30, 0))   # dim4=0 -> 1 vol
    js(f / "sub-01_task-c_bold.json", {"EchoTime": 1})


@case("cleanup_r1_malformed_sidecar")
def _(r):
    f = r / "sub-01" / "func"
    long_bold(f)
    series(f, "sub-01_task-b_bold", None, (64, 64, 30, 3))
    js(f / "sub-01_task-b_bold.json", None, raw='{"PhaseEncodingDirection": "j-",')


def _nonobject_reclass(r):
    f = r / "sub-01" / "func"
    long_bold(f)
    series(f, "sub-01_task-b_bold", {"PhaseEncodingDirection": "j-"}, (64, 64, 30, 3))
    js(f / "sub-01_task-c_bold.json", ["PhaseEncodingDirection"])
    nii(f / "sub-01_task-c_bold.nii.gz", (64, 64, 30, 3))


case("cleanup_r1_nonobject_sidecar")(_nonobject_reclass)
case("cleanup_r1_nonobject_sidecar_strict", "--strict")(_nonobject_reclass)


# --- R2: co-planar T2w -> inplaneT2 ------------------------------------------

@case("cleanup_r2_inplane_t2")
def _(r):
    s = r / "sub-01" / "ses-1"
    long_bold(s / "func", "sub-01_ses-1_task-rest_bold")
    series(s / "anat", "sub-01_ses-1_T2w", {}, (64, 64, 20))
    series(s / "anat", "sub-01_ses-1_acq-hi_T2w", {}, (128, 128, 20))
    series(s / "anat", "sub-01_ses-1_acq-swap_T2w", {}, (64, 30, 64))
    nii(s / "anat" / "sub-01_ses-1_acq-nojs_T2w.nii.gz", (64, 64, 20))
    series(s / "anat", "sub-01_ses-1_acq-nosform_T2w", {}, (64, 64, 20), sform=False)


@case("cleanup_r2_collision_and_nosform_bold")
def _(r):
    s = r / "sub-01"
    series(s / "func", "sub-01_task-rest_bold", {}, (80, 80, 30, 10), sform=False)
    series(s / "func", "sub-01_task-b_bold", {}, (64, 64, 30, 1))  # 3D bold still counts
    series(s / "anat", "sub-01_T2w", {}, (64, 64, 20))
    js(s / "anat" / "sub-01_inplaneT2.json", {"curated": 1})
    series(s / "anat", "sub-01_acq-x_T2w", {}, (80, 80, 20))


# --- _strip_orphan_runs ------------------------------------------------------

@case("cleanup_strip_orphan_runs")
def _(r):
    a = r / "sub-01" / "anat"
    series(a, "sub-01_run-1_T1w", {"ProtocolName": "anat-T1w"})
    series(a, "sub-01_run-1_T2w", {})
    series(a, "sub-01_run-2_T2w", {})
    series(a, "sub-01_run-3_FLAIR", {"ProtocolName": "anat-FLAIR_run-3"})
    series(a, "sub-01_run-4_PDw", {"SeriesDescription": "run-04 pd"})
    series(a, "sub-01_acq-x_run-5_T2starw", {"ProtocolName": "anat_run-x"})
    series(a, "sub-01_acq-y_run-6_T2starw", {"ProtocolName": ["run-6"]})
    series(a, "sub-01_acq-z_run-1_run-2_T2starw", {"ProtocolName": None})
    series(a, "sub-01_acq-q_run-7a_UNIT1", {})
    series(a, "sub-01_acq-m_run-8_MEGRE", {})  # no sidecar needed: malformed below
    js(a / "sub-01_acq-m_run-8_MEGRE.json", None, raw="{bad")


@case("cleanup_strip_orphan_target_exists_and_dirs")
def _(r):
    s = r / "sub-01" / "ses-1"
    series(s / "anat", "sub-01_ses-1_run-1_T1w", {})
    js(s / "anat" / "sub-01_ses-1_T1w.json", {"curated": 1})
    nii(s / ".hidden" / "sub-01_ses-1_run-1_x.nii.gz")
    nii(s / "code" / "sub-01_ses-1_run-2_thing.nii", gz=False)
    text(s / "func" / "sub-01_ses-1_task-a_run-1_physio.tsv.gz", "x", binary=False)
    series(s / "func", "sub-01_ses-1_task-a_run-01_bold", {}, (8, 8, 4, 10))
    text(s / "func" / "sub-01_ses-1_task-a_run-01_events.tsv", "onset\n")


def _nonobject_strip(r):
    a = r / "sub-01" / "anat"
    series(a, "sub-01_run-1_T1w", {})
    js(a / "sub-01_run-1_T1w.json", None, raw='"just a string"')
    series(a, "sub-01_run-2_T2w", {})


# Non-strict twin omitted: reproinx.py iterates unsorted here (README D1).
case("cleanup_strip_orphan_nonobject_strict", "--strict")(_nonobject_strip)


# --- discard/ ----------------------------------------------------------------

@case("cleanup_discard_removed")
def _(r):
    s = r / "sub-01"
    series(s / "ses-1" / "anat", "sub-01_ses-1_T1w", {})
    series(s / "ses-1" / "discard", "sub-01_ses-1_localizer", {})
    text(s / "ses-1" / "discard" / "deep" / "x.txt", "x")
    series(s / "discard", "sub-01_scout", {})  # session-less sibling dir is a session too
    text(r / "sub-02" / "ses-1" / "discard", "a file, not a dir")
    series(r / "sub-02" / "ses-1" / "anat", "sub-02_ses-1_T1w", {})


# --- 3D bold demote ----------------------------------------------------------

@case("cleanup_demote_3d_bold")
def _(r):
    f = r / "sub-01" / "func"
    series(f, "sub-01_task-a_bold", {"TaskName": "a"}, (64, 64, 30, 1))
    text(f / "sub-01_task-a_bold_events.tsv", "onset\n")
    text(f / "sub-01_task-a_events.tsv", "onset\n")
    series(f, "sub-01_task-b_bold", {}, (64, 64, 30, 2))
    nii(f / "sub-01_task-c_bold.nii", (8, 8, 4, 1), gz=False)
    js(f / "sub-01_task-c_bold.json", {})
    series(f, "sub-01_task-d_bold", {}, (64, 64, 30, 1))
    nii(f / "sub-01_task-d_sbref.nii", (8, 8, 4), gz=False)
    series(f, "sub-01_task-e_bold", {}, (64, 64, 30))
    js(f / "sub-01_task-e_sbref.json", {})
    text(f / "sub-01_task-e_bold_events.tsv", "onset\n")
    text(f / "sub-01_task-f_bold.nii.gz", "not gzip", binary=False)


# --- part entities -----------------------------------------------------------

def _it(t):
    return ["ORIGINAL", "PRIMARY", "M", "NONE", t]


@case("cleanup_part_basic_pair")
def _(r):
    f = r / "sub-01" / "func"
    series(f, "sub-01_task-rest_bold", {"Manufacturer": SIE, "ImageType": _it("MAGNITUDE")}, (8, 8, 4, 10))
    series(f, "sub-01_task-rest_bolda", {"Manufacturer": SIE, "ImageType": _it("PHASE")}, (8, 8, 4, 10))
    text(f / "sub-01_task-rest_events.tsv", "onset\n")


@case("cleanup_part_bidsguess_precedence")
def _(r):
    f = r / "sub-01" / "ses-1" / "func"
    base = "sub-01_ses-1_task-rest_acq-2d2echo_run-02_echo-1"

    def mk(stem, it, g, n=1):
        d = {"Manufacturer": SIE, "BidsGuess": ["func", f"_echo-1_{g}"]}
        if it:
            d["ImageType"] = ["ORIGINAL", "PRIMARY", "FMRI", "NONE", it]
        series(f, stem, d, (8, 8, 4, n))
    mk(f"{base}_bold", "MAGNITUDE", "bold", 10)
    mk(f"{base}_bolda", None, "part-phase_sbref")
    mk(f"{base}_boldb", "MAGNITUDE", "part-phase_bold", 10)  # BidsGuess beats ImageType
    mk(f"{base}_sbref", "MAGNITUDE", "sbref")


@case("cleanup_part_repeated_runs")
def _(r):
    f = r / "sub-01" / "func"
    base = "sub-01_task-rest_acq-fs"
    for sfx, it, t in (("", "MAGNITUDE", "17:17:50"), ("a", "PHASE", "17:07:19"),
                       ("b", "PHASE", "17:17:50"), ("c", "MAGNITUDE", "17:07:19")):
        series(f, f"{base}_bold{sfx}", {"Manufacturer": SIE, "AcquisitionTime": t, "ImageType": _it(it)}, (8, 8, 4, 5))


@case("cleanup_part_missing_middle_sbref")
def _(r):
    f = r / "sub-01" / "func"
    base = "sub-01_task-rest"

    def mk(stem, it, sfx, t):
        series(f, stem, {"Manufacturer": SIE, "AcquisitionTime": t, "BidsGuess": ["func", f"_{sfx}"],
                         "ImageType": _it(it)}, (8, 8, 4, 5))
    for i, t in enumerate(("10:00:00", "11:00:00", "12:00:00")):
        mk(f"{base}_bold{'abcdef'[2 * i - 1] if i else ''}", "MAGNITUDE", "bold", t)
        mk(f"{base}_bold{'abcdef'[2 * i]}", "PHASE", "bold", t)
    for i, t in enumerate(("09:59:50", "11:59:50")):
        mk(f"{base}_sbref{'abc'[2 * i - 1] if i else ''}", "MAGNITUDE", "sbref", t)
        mk(f"{base}_sbref{'abc'[2 * i]}", "PHASE", "sbref", t)


@case("cleanup_part_sbref_two_claim_one_bold")
def _(r):
    f = r / "sub-01" / "func"
    base = "sub-01_task-x"
    for sfx, it, t in (("", "MAGNITUDE", "10:00:00"), ("a", "PHASE", "10:00:00"),
                       ("b", "MAGNITUDE", "11:00:00"), ("c", "PHASE", "11:00:00")):
        series(f, f"{base}_bold{sfx}", {"Manufacturer": SIE, "AcquisitionTime": t, "ImageType": _it(it)}, (8, 8, 4, 5))
    for sfx, it, t in (("", "MAGNITUDE", "09:59:50"), ("a", "PHASE", "09:59:50"),
                       ("b", "MAGNITUDE", "09:59:55"), ("c", "PHASE", "09:59:55")):
        series(f, f"{base}_sbref{sfx}", {"Manufacturer": SIE, "AcquisitionTime": t, "ImageType": _it(it)}, (8, 8, 4))


@case("cleanup_part_noRF_real_imag")
def _(r):
    f = r / "sub-01" / "func"
    series(f, "sub-01_task-a_noRF", {"Manufacturer": SIE, "ImageType": _it("MAGNITUDE")}, (8, 8, 4, 3))
    series(f, "sub-01_task-a_noRFa", {"Manufacturer": SIE, "ImageType": _it("PHASE")}, (8, 8, 4, 3))
    series(f, "sub-01_task-b_bold", {"ImageType": _it("REAL")}, (8, 8, 4, 10))
    series(f, "sub-01_task-b_bolda", {"ImageType": _it("IMAGINARY")}, (8, 8, 4, 10))
    series(f, "sub-01_task-c_bold", {"ImageType": _it("MAGNITUDE")}, (8, 8, 4, 10))
    series(f, "sub-01_task-c_bolda", {"ImageType": _it("MAGNITUDE")}, (8, 8, 4, 10))


@case("cleanup_part_ambiguous_and_unclassifiable")
def _(r):
    f = r / "sub-01" / "func"
    series(f, "sub-01_task-p_run-01_bold", {"Manufacturer": "Philips",
           "BidsGuess": ["func", "_part-phase_part-mag_bold"]}, (8, 8, 4, 10))
    series(f, "sub-01_task-q_bold", {"Manufacturer": SIE, "BidsGuess": ["func", "_bold"]}, (8, 8, 4, 10))
    series(f, "sub-01_task-q_bolda", {"Manufacturer": SIE, "ImageType": ["ORIGINAL", "PRIMARY", "PHASE"],
           "BidsGuess": ["func", "_part-phase_bold"]}, (8, 8, 4, 10))
    series(f, "sub-01_task-r_bold", {"ImageType": ["MAGNITUDE", "PHASE"]}, (8, 8, 4, 10))
    series(f, "sub-01_task-r_bolda", {"ImageType": _it("PHASE")}, (8, 8, 4, 10))
    nii(f / "sub-01_task-s_bold.nii.gz", (8, 8, 4, 10))  # orphan base without JSON: no taint
    series(f, "sub-01_task-s_bolda", {"ImageType": _it("PHASE")}, (8, 8, 4, 10))


@case("cleanup_part_bidsguess_shapes")
def _(r):
    f = r / "sub-01" / "func"
    # BidsGuess shapes: not a pair, non-str, _part-magnitude (lookahead), trailing \n
    series(f, "sub-01_task-a_bold", {"BidsGuess": ["func", "_bold", "x"], "ImageType": _it("MAGNITUDE")}, (8, 8, 4, 10))
    series(f, "sub-01_task-a_bolda", {"BidsGuess": ["func", 7], "ImageType": _it("PHASE")}, (8, 8, 4, 10))
    series(f, "sub-01_task-b_bold", {"BidsGuess": ["func", "_part-magnitude_bold"], "ImageType": _it("MAGNITUDE")}, (8, 8, 4, 10))
    series(f, "sub-01_task-b_bolda", {"BidsGuess": "func_part-phase_bold", "ImageType": ["phase"]}, (8, 8, 4, 10))
    series(f, "sub-01_task-c_bold", {"BidsGuess": ["func", "_part-mag_bold"], "ImageType": "MAGNITUDE"}, (8, 8, 4, 10))
    series(f, "sub-01_task-c_bolda", {"BidsGuess": ["func", "_part-phase"], "ImageType": [1, "PHASE", None]}, (8, 8, 4, 10))


@case("cleanup_part_bidsguess_dollar_newline")
def _(r):
    f = r / "sub-01" / "func"
    series(f, "sub-01_task-a_bold", {"ImageType": _it("MAGNITUDE")}, (8, 8, 4, 10))
    series(f, "sub-01_task-a_bolda", {"BidsGuess": ["func", "_bold_part-phase\n"], "ImageType": _it("MAGNITUDE")}, (8, 8, 4, 10))


@case("cleanup_part_target_collision")
def _(r):
    f = r / "sub-01" / "func"
    series(f, "sub-01_task-a_bold", {"ImageType": _it("MAGNITUDE")}, (8, 8, 4, 10))
    series(f, "sub-01_task-a_bolda", {"ImageType": _it("PHASE")}, (8, 8, 4, 10))
    text(f / "sub-01_task-a_part-phase_bold.tsv", "x\n")
    # same-part pair without times: _part_runs None
    series(f, "sub-01_task-b_bold", {"ImageType": _it("PHASE")}, (8, 8, 4, 10))
    series(f, "sub-01_task-b_bolda", {"ImageType": _it("PHASE"), "AcquisitionTime": "10:00:00"}, (8, 8, 4, 10))
    # repeated but prefix already carries a run
    for sfx, it, t in (("", "MAGNITUDE", "10:00:00"), ("a", "PHASE", "10:00:00"),
                       ("b", "MAGNITUDE", "11:00:00"), ("c", "PHASE", "11:00:00")):
        series(f, f"sub-01_task-c_run-1_bold{sfx}", {"AcquisitionTime": t, "ImageType": _it(it)}, (8, 8, 4, 10))
    # same part twice at the same time
    series(f, "sub-01_task-d_bold", {"ImageType": _it("PHASE"), "AcquisitionTime": "10:00:00"}, (8, 8, 4, 10))
    series(f, "sub-01_task-d_bolda", {"ImageType": _it("PHASE"), "AcquisitionTime": "10:00:00"}, (8, 8, 4, 10))


@case("cleanup_part_acqtime_variants")
def _(r):
    f = r / "sub-01" / "func"
    for sfx, it, t in (("", "MAGNITUDE", 0), ("a", "PHASE", 0.0), ("b", "MAGNITUDE", 1.5), ("c", "PHASE", 1.5)):
        series(f, f"sub-01_task-a_bold{sfx}", {"AcquisitionTime": t, "ImageType": _it(it)}, (8, 8, 4, 10))
    # sbref run assignment with odd time strings
    for sfx, it, t in (("", "MAGNITUDE", "10:00:00"), ("a", "PHASE", "10:00:00"),
                       ("b", "MAGNITUDE", "11:00:00"), ("c", "PHASE", "11:00:00")):
        series(f, f"sub-01_task-b_bold{sfx}", {"AcquisitionTime": t, "ImageType": _it(it)}, (8, 8, 4, 10))
    for sfx, it, t in (("", "MAGNITUDE", " 10:00:01 "), ("a", "PHASE", " 10:00:01 "),
                       ("b", "MAGNITUDE", "1_1:00:01"), ("c", "PHASE", "1_1:00:01")):
        series(f, f"sub-01_task-b_sbref{sfx}", {"AcquisitionTime": t, "ImageType": _it(it)}, (8, 8, 4))


@case("cleanup_part_idempotent_canonical")
def _(r):
    f = r / "sub-01" / "func"
    base = "sub-01_task-rest_run-01_echo-1"
    series(f, f"{base}_part-mag_bold", {"Manufacturer": SIE, "ImageType": _it("MAGNITUDE")}, (8, 8, 4, 10))
    series(f, f"{base}_part-phase_bold", {"Manufacturer": SIE, "ImageType": _it("PHASE")}, (8, 8, 4, 10))
    series(f, "sub-01_task-x_part-phase_boldx", {"Manufacturer": SIE, "ImageType": _it("PHASE")}, (8, 8, 4, 10))


def _part_malformed(r):
    f = r / "sub-01" / "func"
    series(f, "sub-01_task-a_bold", {"ImageType": _it("MAGNITUDE")}, (8, 8, 4, 10))
    series(f, "sub-01_task-a_bolda", {"ImageType": _it("PHASE")}, (8, 8, 4, 10))
    series(f, "sub-01_task-z_bold", {}, (8, 8, 4, 10))
    js(f / "sub-01_task-z_bold.json", None, raw="{not json")


case("cleanup_part_malformed_json")(_part_malformed)
case("cleanup_part_malformed_json_strict", "--strict")(_part_malformed)


@case("cleanup_part_nonobject_json")
def _(r):
    f = r / "sub-01" / "func"
    series(f, "sub-01_task-a_bold", {}, (8, 8, 4, 10))
    js(f / "sub-01_task-a_bold.json", [1, 2])
    series(f, "sub-01_task-a_bolda", {"ImageType": _it("PHASE")}, (8, 8, 4, 10))
    series(f, "sub-01_task-b_bold", {"ImageType": _it("MAGNITUDE")}, (8, 8, 4, 10))
    series(f, "sub-01_task-b_bolda", {"ImageType": _it("PHASE")}, (8, 8, 4, 10))


# --- _siemens_phase_units ----------------------------------------------------

@case("cleanup_phase_units")
def _(r):
    s = r / "sub-01"
    series(s / "anat", "sub-01_part-phase_T2starw", {"Manufacturer": SIE})
    series(s / "anat", "sub-01_acq-u_part-phase_T2starw", {"Manufacturer": SIE, "Units": "rad"})
    series(s / "anat", "sub-01_acq-ge_part-phase_T2starw", {"Manufacturer": "GE"})
    series(s / "anat", "sub-01_acq-hc_part-phase_T2starw", {"Manufacturer": "SIEMENS Healthineers"})
    series(s / "anat", "sub-01_acq-l_part-phase_T2starw", {"Manufacturer": ["Siemens"]})
    series(s / "anat", "sub-01_acq-n_part-phase_T2starw", {"Manufacturer": None, "Units": None})
    js(s / "anat" / "sub-01_acq-bad_part-phase_T2starw.json", None, raw="{")
    text(s / "anat" / "sub-01_acq-utf_part-phase_T2starw.json", b'{"Manufacturer": "Siemens\xff"}', binary=True)
    js(s / "anat" / "sub-01_acq-lst_part-phase_T2starw.json", ["Siemens"])
    js(s / "anat" / "deep" / "sub-01_part-phase_T2starw.json", {"Manufacturer": SIE})
    js(s / "anat" / "sub-01_acq-end_part-phase.json", {"Manufacturer": SIE})


# --- .bidsignore sweeps ------------------------------------------------------

@case("cleanup_singlevol_dwi_bidsignore")
def _(r):
    d = r / "sub-01" / "ses-1" / "dwi"
    series(d, "sub-01_ses-1_dwi", {}, (8, 8, 4, 1))
    text(d / "sub-01_ses-1_dwi.bval", "0\n")
    text(d / "sub-01_ses-1_dwi.bvec", "0\n0\n0\n")
    nii(d / "sub-01_ses-1_acq-u_dwi.nii", (8, 8, 4), gz=False)
    series(d, "sub-01_ses-1_acq-multi_dwi", {}, (8, 8, 4, 7))
    text(d / "sub-01_ses-1_acq-bad_dwi.nii.gz", "garbage")


@case("cleanup_collision_bidsignore_curated")
def _(r):
    s = r / "sub-01"
    for n in ("anat/sub-01_T1wa.nii.gz", "anat/sub-01_T1wa.json", "fmap/sub-01_magnitude1b.json",
              "fmap/sub-01_phasediffc.nii", "dwi/sub-01_FAa.nii.gz", "dwi/sub-01_dwia.bvec",
              "anat/sub-01_T1wA.json", "anat/sub-01_T1waa.json", "perf/sub-01_aslcontextb.tsv",
              "fmap/sub-01_epiz.json", "anat/sub-01_inplaneT2a.json"):
        text(s / n, "{}")
    text(r / "derivatives" / "scanner" / "sub-01" / "sub-01_boldb.json", "{}")
    text(r / ".bidsignore", "# curated\nanat/sub-01_T1wa.json\nextra/*")


@case("cleanup_bidsignore_existing_variants")
def _(r):
    s = r / "sub-01" / "anat"
    text(s / "sub-01_T2wa.json", "{}")
    text(s / "sub-01_T2wb.json", "{}")
    text(r / ".bidsignore", "a/\r\nb/\r\n  sub-01/anat/sub-01_T2wb.json  \n\n", binary=False)


@case("cleanup_bidsignore_unicode_line_breaks")
def _(r):
    s = r / "sub-01" / "anat"
    text(s / "sub-01_T2wa.json", "{}")
    text(r / ".bidsignore", "a/\x0cb/ c/\x1dd/\n", binary=False)


def _bidsignore_badutf8(r):
    text(r / "sub-01" / "anat" / "sub-01_T2wa.json", "{}")
    text(r / ".bidsignore", b"curated\xff\n", binary=True)


case("cleanup_bidsignore_invalid_utf8")(_bidsignore_badutf8)
case("cleanup_bidsignore_invalid_utf8_strict", "--strict")(_bidsignore_badutf8)


# --- _apply_dup_naming -------------------------------------------------------

def _dup_row(stem, sn, prot="anat-T1w", uid="1.2.3"):
    return {"OutputStem": stem, "SeriesNumber": sn, "ProtocolName": prot,
            "SeriesDescription": prot, "StudyInstanceUID": uid}


@case("cleanup_dup_two_phase_reorder")
def _(r):
    a = r / "sub-01" / "anat"
    series(a, "sub-01_T1w", {"SeriesNumber": 7})
    series(a, "sub-01_T1wa", {"SeriesNumber": 3})
    series(a, "sub-01_T1wb", {"SeriesNumber": 9})
    prov(r, [_dup_row("sub-01/anat/sub-01_T1w", 7), _dup_row("sub-01/anat/sub-01_T1wa", 3),
             _dup_row("sub-01/anat/sub-01_T1wb", 9)])


@case("cleanup_dup_missing_base_and_resolved")
def _(r):
    f = r / "sub-01" / "func"
    nii(f / "sub-01_task-rest_bolda.nii.gz", (8, 8, 4, 10))  # base gone, survivor stays
    # part-resolved group: none of the as-written stems exist any more
    series(f, "sub-01_task-x_bold", {"ImageType": _it("MAGNITUDE")}, (8, 8, 4, 10))
    series(f, "sub-01_task-x_bolda", {"ImageType": _it("PHASE")}, (8, 8, 4, 10))
    prov(r, [_dup_row("sub-01/func/sub-01_task-rest_bold", 1, "rest"),
             _dup_row("sub-01/func/sub-01_task-rest_bolda", 2, "rest"),
             _dup_row("sub-01/func/sub-01_task-x_bold", 3, "x"),
             _dup_row("sub-01/func/sub-01_task-x_bolda", 4, "x")])


@case("cleanup_dup_guards")
def _(r):
    a = r / "sub-01" / "anat"
    # different ProtocolName: not a collision group
    series(a, "sub-01_acq-a_T2w", {})
    series(a, "sub-01_acq-a_T2wb", {})
    # stale target __dup-01 already present (not a current stem)
    series(a, "sub-01_FLAIR", {})
    series(a, "sub-01_FLAIRa", {})
    js(a / "sub-01_FLAIR__dup-01.json", {})
    # every target already exists: no-op
    series(a, "sub-01_PDw", {})
    series(a, "sub-01_PDw__dup-01", {})
    series(a, "sub-01_PDwa", {})
    prov(r, [_dup_row("sub-01/anat/sub-01_acq-a_T2w", 1, "t2"), _dup_row("sub-01/anat/sub-01_acq-a_T2wb", 2, "t2b"),
             _dup_row("sub-01/anat/sub-01_FLAIR", 3, "fl"), _dup_row("sub-01/anat/sub-01_FLAIRa", 4, "fl"),
             _dup_row("sub-01/anat/sub-01_PDw", 5, "pd"), _dup_row("sub-01/anat/sub-01_PDwa", 6, "pd"),
             _dup_row("sub-01/anat/sub-01_PDwa", 1, "dupe-row-ignored")])


@case("cleanup_dup_seriesnumber_forms")
def _(r):
    a = r / "sub-01" / "anat"
    for sfx in ("", "a", "b", "c", "d"):
        series(a, f"sub-01_T1w{sfx}", {"tag": sfx})
    prov(r, [_dup_row("sub-01/anat/sub-01_T1w", " 9 "), _dup_row("sub-01/anat/sub-01_T1wa", "1_0"),
             _dup_row("sub-01/anat/sub-01_T1wb", "+2"), _dup_row("sub-01/anat/sub-01_T1wc", "x"),
             _dup_row("sub-01/anat/sub-01_T1wd", "")])


@case("cleanup_dup_seriesnumber_unicode_digits", divergence="D3 non-ASCII digits")
def _(r):
    a = r / "sub-01" / "anat"
    for sfx in ("", "a"):
        series(a, f"sub-01_T1w{sfx}", {"tag": sfx})
    prov(r, [_dup_row("sub-01/anat/sub-01_T1w", "1"), _dup_row("sub-01/anat/sub-01_T1wa", "\u0663")])  # Arabic-Indic 3


def _dup_stale_tmp(r):
    a = r / "sub-01" / "anat"
    series(a, "sub-01_T1w", {"tag": "base"})
    series(a, "sub-01_T1wa", {"tag": "a"})
    js(a / "sub-01_T1w__reproinx-tmp-001.json", {"stale": True})
    # both stems must move (a has the lower SeriesNumber): tmp-000 ok, tmp-001 clashes
    prov(r, [_dup_row("sub-01/anat/sub-01_T1w", 2), _dup_row("sub-01/anat/sub-01_T1wa", 1)])


case("cleanup_dup_stale_tmp_rollback")(_dup_stale_tmp)
case("cleanup_dup_stale_tmp_rollback_strict", "--strict")(_dup_stale_tmp)
