"""Core differential cases: pCASL volume removal, basic layouts."""
from synth import case, js, nii, prov, series, text

PLD8 = [0, 0, 0, 1.8, 1.8, 1.8, 1.8, 1.8]


@case("asl_drop_dummies")
def _(r):
    p = r / "sub-01" / "perf"
    nii(p / "sub-01_asl.nii.gz", (4, 4, 2, 8), data=True)
    js(p / "sub-01_asl.json", {"PostLabelingDelay": PLD8, "ReproinxDropVolumes": [1, 2], "Manufacturer": "Siemens"})


@case("asl_drop_uncompressed_bigindex")
def _(r):
    p = r / "sub-01" / "perf"
    nii(p / "sub-01_asl.nii", (3, 3, 2, 8), gz=False, data=True, bitpix=32)
    js(p / "sub-01_asl.json", {"PostLabelingDelay": PLD8, "ReproinxDropVolumes": [1, 2, 99, -1, 2.0, True]})


@case("asl_rerun_already_trimmed")
def _(r):
    p = r / "sub-01" / "perf"
    nii(p / "sub-01_asl.nii.gz", (4, 4, 2, 6), data=True)
    js(p / "sub-01_asl.json", {"PostLabelingDelay": PLD8, "ReproinxDropVolumes": [1, 2]})


@case("asl_count_mismatch_untouched")
def _(r):
    p = r / "sub-01" / "perf"
    nii(p / "sub-01_asl.nii.gz", (4, 4, 2, 7), data=True)
    js(p / "sub-01_asl.json", {"PostLabelingDelay": PLD8, "ReproinxDropVolumes": [1, 2]})


@case("asl_context_exists_and_scalar_pld")
def _(r):
    p = r / "sub-01" / "ses-1" / "perf"
    series(p, "sub-01_ses-1_asl", {"PostLabelingDelay": [0, 1.5, 1.5, 2.0, 2.0]}, (4, 4, 2, 5), data=True)
    text(p / "sub-01_ses-1_aslcontext.tsv", "volume_type\ncurated\n")
    series(p, "sub-01_ses-1_acq-b_asl", {"PostLabelingDelay": 1.8}, (4, 4, 2, 4))


@case("basic_func_anat_fmap")
def _(r):
    s = r / "sub-01"
    series(s / "anat", "sub-01_T1w", {"AcquisitionDateTime": "2026-05-12T12:00:01.5", "SeriesInstanceUID": "1.2", "StudyInstanceUID": "1"})
    for i, pe in ((1, "j"), (2, "j-")):
        series(s / "fmap", f"sub-01_dir-{'AP' if i == 1 else 'PA'}_epi", {"PhaseEncodingDirection": pe, "ShimSetting": [1, 2, 3], "AcquisitionTime": "12:05:00"})
    series(s / "func", "sub-01_task-rest_bold", {"ShimSetting": [1, 2, 3], "AcquisitionTime": "12:06:00", "AcquisitionDateTime": "2026-05-12T12:06:00"}, (64, 64, 30, 10))
    series(s / "func", "sub-01_task-rest_sbref", {"ShimSetting": [1, 2, 3.01], "AcquisitionTime": "12:05:59"})


@case("basic_shift_dates", "--shift-dates")
def _(r):
    s = r / "sub-01" / "ses-1"
    series(s / "anat", "sub-01_ses-1_T1w", {"AcquisitionDateTime": "2026-05-12T12:00:01.5", "AcquisitionDate": "20260512"})
    series(s / "func", "sub-01_ses-1_task-rest_bold", {"AcquisitionDateTime": "2026-05-13T08:00:00"}, (8, 8, 4, 10))
    prov(r, [{"OutputStem": "sub-01/ses-1/anat/sub-01_ses-1_T1w", "StudyDate": "20260512", "StudyTime": "120000", "PatientID": "x"}])


@case("unicode_and_float_sidecars")
def _(r):
    s = r / "sub-01" / "anat"
    series(s, "sub-01_T1w", {"InstitutionName": "Universität ☃ 😀", "EchoTime": 0.00123, "Tiny": 1e-07, "Big": 1e16, "N": 10, "F": 2.0, "Neg": -0.0})


@case("hostile_stem_equals_suffix_bold")
def _(r):
    f = r / "sub-01" / "func"
    nii(f / "bold.nii", (8, 8, 4, 2), gz=False)
    js(f / "bold.json", {"PhaseEncodingDirection": "j-"})
    series(f, "sub-01_task-rest_bold", {"PhaseEncodingDirection": "j"}, (8, 8, 4, 10))


@case("hostile_bare_T2w_inplane")
def _(r):
    s = r / "sub-01"
    nii(s / "anat" / "T2w.nii", (8, 8, 4, 1), gz=False)
    js(s / "anat" / "T2w.json", {})
    series(s / "func", "sub-01_task-rest_bold", {}, (8, 8, 4, 10))


@case("asl_truncated_uncompressed_skipped")
def _(r):
    p = r / "sub-01" / "perf"
    nii(p / "sub-01_asl.nii", (4, 4, 2, 8), gz=False, data=True)
    with open(p / "sub-01_asl.nii", "r+b") as fh:
        fh.truncate(352 + 4 * 4 * 2 * 2 * 8 - 10)
    js(p / "sub-01_asl.json", {"PostLabelingDelay": PLD8, "ReproinxDropVolumes": [1, 2]})
