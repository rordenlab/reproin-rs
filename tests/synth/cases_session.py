"""Session backfill, physio rescue, derivatives drop, subject/session/root discovery."""
import gzip
from synth import case, js, nii, prov, series, text

UID = "1.2.3"
SC = {"AcquisitionTime": "10:00:00"}


def physio(dir_, stem, tsv=True, json=True):
    if tsv:
        text(dir_ / f"{stem}.tsv.gz", gzip.compress(b"1\t2\n", mtime=0), binary=True)
    if json:
        js(dir_ / f"{stem}.json", {"SamplingFrequency": 400, "StartTime": 0, "Columns": ["cardiac"]})


def row(stem, proto="", uid=UID, desc=""):
    return {"StudyInstanceUID": uid, "OutputStem": stem, "ProtocolName": proto, "SeriesDescription": desc or proto}


# --- _propagate_session: provenance -----------------------------------------

@case("session_prov_basic")
def _(r):
    s = r / "sub-01"
    series(s / "anat", "sub-01_T1w", SC)
    series(s / "func", "sub-01_task-rest_bold", SC, (8, 8, 4, 5))
    prov(r, [row("derivatives/scanner/sub-01/anat/sub-01_scout", "anat-scout_ses-pre"),
             row("sub-01/anat/sub-01_T1w", "anat-T1w"),
             row("sub-01/func/sub-01_task-rest_bold", "func-bold_task-rest")])


@case("session_prov_scout_in_derivatives")
def _(r):
    s = r / "sub-01"
    series(s / "anat", "sub-01_T1w", SC)
    series(r / "derivatives" / "scanner" / "sub-01" / "anat", "sub-01_scout", SC)
    prov(r, [row("derivatives/scanner/sub-01/anat/sub-01_scout", "anat-scout_ses-01a"),
             row("sub-01/anat/sub-01_T1w", "anat-T1w")])


@case("session_prov_seriesdescription_marker")
def _(r):
    series(r / "sub-01" / "anat", "sub-01_T1w", SC)
    prov(r, [{"StudyInstanceUID": UID, "OutputStem": "sub-01/anat/sub-01_T1w",
              "ProtocolName": "anat-T1w", "SeriesDescription": "anat-T1w_ses-desc"}])


@case("session_prov_sibling_marker_ignored")
def _(r):
    series(r / "sub-01" / "anat", "sub-01_T1w", SC)
    series(r / "sub-02" / "anat", "sub-02_T1w", SC)
    prov(r, [row("sub-01/anat/sub-01_T1w", "anat-T1w_ses-a"),
             row("sub-02/anat/sub-02_T1w", "anat-T1w")])


@case("session_prov_two_subjects_different_sessions")
def _(r):
    for sub, ses in (("01", "a"), ("02", "b")):
        series(r / f"sub-{sub}" / "anat", f"sub-{sub}_T1w", SC)
        series(r / f"sub-{sub}" / "func", f"sub-{sub}_task-x_bold", SC, (8, 8, 4, 5))
    prov(r, [row("sub-01/anat/sub-01_T1w", "anat-T1w_ses-a", uid="1.1"),
             row("sub-01/func/sub-01_task-x_bold", "func-bold_task-x", uid="1.1"),
             row("sub-02/anat/sub-02_T1w", "anat-T1w_ses-b", uid="2.2"),
             row("sub-02/func/sub-02_task-x_bold", "func-bold_task-x", uid="2.2")])


@case("session_prov_stale_study_ignored")
def _(r):
    # The old study's marker has no files on disk; the current study carries none.
    series(r / "sub-01" / "anat", "sub-01_T1w", SC)
    prov(r, [row("sub-01/anat/sub-01_old", "anat-scout_ses-old", uid="9.9"),
             row("sub-01/anat/sub-01_T1w", "anat-T1w", uid=UID)])


@case("session_prov_same_study_marker_without_files")
def _(r):
    # Marker row lacks files but shares the current StudyInstanceUID -> honoured.
    series(r / "sub-01" / "anat", "sub-01_T1w", SC)
    prov(r, [row("sub-01/anat/sub-01_scout", "anat-scout_ses-now"),
             row("sub-01/anat/sub-01_T1w", "anat-T1w")])


@case("session_prov_no_uid_falls_back_to_filename")
def _(r):
    series(r / "sub-01" / "anat", "sub-01_ses-fn_T1w", SC)
    series(r / "sub-01" / "func", "sub-01_task-x_bold", SC, (8, 8, 4, 5))
    prov(r, [row("sub-01/anat/sub-01_ses-fn_T1w", "anat-T1w_ses-prov", uid=""),
             row("sub-01/func/sub-01_task-x_bold", "func-bold_task-x", uid="")])


@case("session_prov_ambiguous_falls_back")
def _(r):
    series(r / "sub-01" / "anat", "sub-01_ses-z_T1w", SC)
    series(r / "sub-01" / "func", "sub-01_task-x_bold", SC, (8, 8, 4, 5))
    prov(r, [row("sub-01/anat/sub-01_ses-z_T1w", "anat-T1w_ses-a"),
             row("sub-01/func/sub-01_task-x_bold", "func-bold_ses-b_task-x")])


@case("session_prov_stem_by_bval_only")
def _(r):
    # _stem_has_files accepts any BIDS ext; only a .bvec exists for the row stem.
    d = r / "sub-01" / "dwi"
    text(d / "sub-01_dwi.bvec", "0 1\n0 0\n0 0\n")
    series(r / "sub-01" / "anat", "sub-01_T1w", SC)
    prov(r, [row("sub-01/dwi/sub-01_dwi", "dwi_ses-gr")])


@case("session_prov_crlf_and_short_rows")
def _(r):
    series(r / "sub-01" / "anat", "sub-01_T1w", SC)
    text(r / ".reproin_provenance.tsv",
         "StudyInstanceUID\tSeriesNumber\tProtocolName\tSeriesDescription\tStudyDescription\tOutputStem\n"
         "1.2\tx\tanat-T1w_ses-c\t\t\tsub-01/anat/sub-01_T1w\n"
         "1.2\t3\tshort\n")


# --- _propagate_session: filename / derivatives / consolidation --------------

@case("session_filename_fallback")
def _(r):
    s = r / "sub-01"
    series(s / "anat", "sub-01_ses-2_T1w", SC)
    series(s / "func", "sub-01_task-rest_bold", SC, (8, 8, 4, 5))
    text(s / "func" / "sub-01_task-rest_events.tsv", "onset\tduration\n")


@case("session_filename_ambiguous")
def _(r):
    s = r / "sub-01"
    series(s / "anat", "sub-01_ses-1_T1w", SC)
    series(s / "anat", "sub-01_ses-2_T2w", SC)
    series(s / "func", "sub-01_task-rest_bold", SC, (8, 8, 4, 5))


@case("session_filename_from_curated_dir", divergence="D9 curated dirs")
def _(r):
    # reproinx.py's fallback scans curated dirs too and would move the T1 into
    # ses-cur; reproin-rs (like the retired BIDSvue port) ignores them.
    s = r / "sub-01"
    text(s / "code" / "sub-01_ses-cur_notes.txt", "x")
    series(s / "anat", "sub-01_T1w", SC)


@case("session_deriv_scout_fallback")
def _(r):
    series(r / "sub-01" / "anat", "sub-01_T1w", SC)
    series(r / "derivatives" / "scanner" / "sub-01" / "anat", "sub-01_ses-dv_scout", SC)


@case("session_deriv_dirname_only_no_match")
def _(r):
    # `ses-X` as a bare dir name lacks the leading underscore _SES_RE needs.
    series(r / "sub-01" / "anat", "sub-01_T1w", SC)
    series(r / "derivatives" / "scanner" / "sub-01" / "ses-dv" / "anat", "sub-01_scout", SC)


@case("session_deriv_other_subject_ignored")
def _(r):
    series(r / "sub-01" / "anat", "sub-01_T1w", SC)
    series(r / "derivatives" / "scanner" / "sub-02" / "anat", "sub-02_ses-x_scout", SC)


@case("session_single_existing_consolidates")
def _(r):
    s = r / "sub-01"
    series(s / "ses-20260101" / "anat", "sub-01_ses-20260101_T1w", SC)
    series(s / "fmap", "sub-01_dir-AP_epi", {"PhaseEncodingDirection": "j"})


@case("session_two_existing_no_consolidate")
def _(r):
    s = r / "sub-01"
    series(s / "ses-1" / "anat", "sub-01_ses-1_T1w", SC)
    series(s / "ses-2" / "anat", "sub-01_ses-2_T1w", SC)
    series(s / "fmap", "sub-01_dir-AP_epi", {"PhaseEncodingDirection": "j"})


@case("session_existing_with_only_curated")
def _(r):
    s = r / "sub-01"
    series(s / "ses-1" / "anat", "sub-01_ses-1_T1w", SC)
    text(s / "stimuli" / "img.png", b"\x89PNG", binary=True)


@case("session_collision_skips_series")
def _(r):
    s = r / "sub-01"
    series(s / "anat", "sub-01_T1w", SC)
    series(s / "anat", "sub-01_T2w", SC)
    js(s / "ses-1" / "anat" / "sub-01_ses-1_T1w.json", {"curated": True})
    prov(r, [row("sub-01/anat/sub-01_T1w", "anat-T1w_ses-1"), row("sub-01/anat/sub-01_T2w", "anat-T2w")])


@case("session_collision_strict", "--strict")
def _(r):
    s = r / "sub-01"
    series(s / "anat", "sub-01_T1w", SC)
    nii(s / "ses-1" / "anat" / "sub-01_ses-1_T1w.nii.gz")
    prov(r, [row("sub-01/anat/sub-01_T1w", "anat-T1w_ses-1")])


@case("session_curated_and_hidden_left")
def _(r):
    s = r / "sub-01"
    series(s / "anat", "sub-01_ses-k_T1w", SC)
    series(s / "func", "sub-01_task-x_bold", SC, (8, 8, 4, 5))
    text(s / "code" / "run.sh", "#!/bin/sh\n")
    text(s / ".ipynb_checkpoints" / "a.json", "{}")
    text(s / "sub-01_sessions.tsv", "session_id\n")
    text(s / "anat" / "extra" / "keep.txt", "x")


@case("session_inject_noprefix_and_other_ses")
def _(r):
    s = r / "sub-01" / "anat"
    series(s, "sub-01_T1w", SC)
    series(s, "T2w_nosub", SC)
    series(s, "sub-01_ses-B_FLAIR", SC)
    prov(r, [row("sub-01/anat/sub-01_T1w", "anat-T1w_ses-A")])


@case("session_dwi_family_and_ext_mix")
def _(r):
    d = r / "sub-01" / "dwi"
    series(d, "sub-01_dwi", SC, (8, 8, 4, 3))
    text(d / "sub-01_dwi.bval", "0 1000 1000\n")
    text(d / "sub-01_dwi.bvec", "0 1 0\n0 0 1\n0 0 0\n")
    nii(d / "sub-01_acq-u_dwi.nii", gz=False)
    text(d / "sub-01_dwi.txt", "odd ext")
    prov(r, [row("sub-01/dwi/sub-01_dwi", "dwi_ses-d")])


@case("session_idempotent_rerun")
def _(r):
    s = r / "sub-01" / "ses-pre"
    series(s / "anat", "sub-01_ses-pre_T1w", SC)
    series(s / "func", "sub-01_ses-pre_task-x_bold", SC, (8, 8, 4, 5))
    prov(r, [row("sub-01/ses-pre/anat/sub-01_ses-pre_T1w", "anat-T1w_ses-pre")])


@case("session_rerun_new_session_beside_old")
def _(r):
    s = r / "sub-01"
    series(s / "ses-A" / "anat", "sub-01_ses-A_T1w", SC)
    series(s / "anat", "sub-01_T1w", SC)
    prov(r, [row("sub-01/ses-A/anat/sub-01_ses-A_T1w", "anat-T1w_ses-A", uid="1"),
             row("sub-01/anat/sub-01_T1w", "anat-T1w_ses-B", uid="2")])


@case("session_marker_regex_edges")
def _(r):
    series(r / "sub-01" / "anat", "sub-01_T1w", SC)
    series(r / "sub-02" / "anat", "sub-02_T1w", SC)
    prov(r, [row("sub-01/anat/sub-01_T1w", "anat-T1w_ses-01a-b+c"),
             row("sub-02/anat/sub-02_T1w", "ses-x_anat-T1w")])


# --- _rescue_physio_recordings ----------------------------------------------

DS = ("derivatives", "scanner")


@case("session_physio_single_session")
def _(r):
    series(r / "sub-01" / "ses-1" / "func", "sub-01_ses-1_task-rest_bold", SC, (8, 8, 4, 5))
    physio(r.joinpath(*DS, "sub-01", "func"), "sub-01_task-rest_bold_recording-cardiac_physio")


@case("session_physio_sessionless")
def _(r):
    series(r / "sub-01" / "func", "sub-01_task-rest_bold", SC, (8, 8, 4, 5))
    physio(r.joinpath(*DS, "sub-01", "func"), "sub-01_task-rest_sbref_recording-resp_physio")
    physio(r.joinpath(*DS, "sub-01", "func"), "sub-01_task-rest_acq-T2starw_T2starw_recording-ext_physio")


@case("session_physio_after_backfill")
def _(r):
    series(r / "sub-01" / "func", "sub-01_task-rest_bold", SC, (8, 8, 4, 5))
    physio(r.joinpath(*DS, "sub-01", "func"), "sub-01_task-rest_bold_recording-cardiac_physio")
    prov(r, [row("sub-01/func/sub-01_task-rest_bold", "func-bold_task-rest_ses-mri")])


@case("session_physio_nested_ses_func_and_infix_edges")
def _(r):
    series(r / "sub-01" / "ses-1" / "func", "sub-01_ses-1_task-rest_bold", SC, (8, 8, 4, 5))
    d = r.joinpath(*DS, "sub-01", "ses-1", "func")
    physio(d, "sub-01_ses-1_task-rest_dwi_recording-puls_physio")
    physio(d, "sub-01_task-rest_bold_physio")              # no _recording-: not matched
    physio(d, "sub-01_task-boldy_recording-r_physio")      # no infix to strip
    series(d, "sub-01_task-rest_bold", SC)                 # non-physio stays, then dropped


@case("session_physio_multi_session_keeps_derivatives")
def _(r):
    for ses in ("1", "2"):
        series(r / "sub-01" / f"ses-{ses}" / "func", f"sub-01_ses-{ses}_task-rest_bold", SC, (8, 8, 4, 5))
    physio(r.joinpath(*DS, "sub-01", "func"), "sub-01_task-rest_bold_recording-cardiac_physio")
    series(r.joinpath(*DS, "sub-01", "anat"), "sub-01_scout", SC)


@case("session_physio_multi_session_strict", "--strict")
def _(r):
    for ses in ("1", "2"):
        series(r / "sub-01" / f"ses-{ses}" / "func", f"sub-01_ses-{ses}_task-rest_bold", SC, (8, 8, 4, 5))
    physio(r.joinpath(*DS, "sub-01", "func"), "sub-01_task-rest_bold_recording-cardiac_physio")


@case("session_physio_collision")
def _(r):
    f = r / "sub-01" / "func"
    series(f, "sub-01_task-rest_bold", SC, (8, 8, 4, 5))
    js(f / "sub-01_task-rest_recording-cardiac_physio.json", {"curated": 1})
    d = r.joinpath(*DS, "sub-01", "func")
    physio(d, "sub-01_task-rest_bold_recording-cardiac_physio")
    physio(d, "sub-01_task-rest_bold_recording-resp_physio")


@case("session_physio_collision_strict", "--strict")
def _(r):
    f = r / "sub-01" / "func"
    series(f, "sub-01_task-rest_bold", SC, (8, 8, 4, 5))
    js(f / "sub-01_task-rest_recording-cardiac_physio.json", {"curated": 1})
    physio(r.joinpath(*DS, "sub-01", "func"), "sub-01_task-rest_bold_recording-cardiac_physio")


@case("session_physio_keep_derivatives", "--keep-derivatives")
def _(r):
    series(r / "sub-01" / "func", "sub-01_task-rest_bold", SC, (8, 8, 4, 5))
    physio(r.joinpath(*DS, "sub-01", "func"), "sub-01_task-rest_bold_recording-cardiac_physio")
    series(r.joinpath(*DS, "sub-01", "anat"), "sub-01_scout", SC)


@case("session_physio_lone_companions")
def _(r):
    series(r / "sub-01" / "func", "sub-01_task-rest_bold", SC, (8, 8, 4, 5))
    d = r.joinpath(*DS, "sub-01", "func")
    physio(d, "sub-01_task-a_bold_recording-c_physio", json=False)
    physio(d, "sub-01_task-b_bold_recording-c_physio", tsv=False)


@case("session_physio_no_main_subject")
def _(r):
    series(r / "sub-01" / "func", "sub-01_task-rest_bold", SC, (8, 8, 4, 5))
    physio(r.joinpath(*DS, "sub-02", "func"), "sub-02_task-rest_bold_recording-c_physio")
    text(r.joinpath(*DS, "notsub", "func", "x_recording-c_physio.json"), "{}")


@case("session_physio_sessionless_name_has_ses")
def _(r):
    series(r / "sub-01" / "func", "sub-01_task-rest_bold", SC, (8, 8, 4, 5))
    physio(r.joinpath(*DS, "sub-01", "ses-9", "func"), "sub-01_ses-9_task-rest_bold_recording-c_physio")


# --- _drop_derivatives ------------------------------------------------------

@case("session_derivs_curated_kept")
def _(r):
    series(r / "sub-01" / "anat", "sub-01_T1w", SC)
    series(r.joinpath(*DS, "sub-01", "anat"), "sub-01_FA", SC)
    js(r / "derivatives" / "fmriprep" / "dataset_description.json", {"Name": "fp"})
    text(r / "derivatives" / "mriqc" / "sub-01" / "x.html", "<p/>")


@case("session_derivs_parent_removed_when_empty")
def _(r):
    series(r / "sub-01" / "anat", "sub-01_T1w", SC)
    series(r.joinpath(*DS, "sub-01", "anat"), "sub-01_FA", SC)


@case("session_derivs_multi_roots_and_outer")
def _(r):
    for st in ("StudyA", "StudyB"):
        series(r / st / "sub-01" / "anat", "sub-01_T1w", SC)
    series(r.joinpath("StudyA", *DS, "sub-01", "anat"), "sub-01_FA", SC)
    series(r.joinpath("StudyB", *DS, "sub-01", "anat"), "sub-01_FA", SC)
    text(r / "StudyB" / "derivatives" / "notes.txt", "keep")
    series(r.joinpath(*DS, "sub-09", "anat"), "sub-09_FA", SC)  # not a BIDS root


# --- _walk_subjects / _walk_sessions / _bids_roots ---------------------------

@case("session_walk_multi_study_roots")
def _(r):
    for st, ses in (("StudyA", "a"), ("StudyB", "b")):
        series(r / st / "sub-01" / "anat", "sub-01_T1w", SC)
        prov(r / st, [row("sub-01/anat/sub-01_T1w", f"anat-T1w_ses-{ses}", uid=st)])


@case("session_walk_nested_hierarchy")
def _(r):
    series(r / "Lab" / "Proj" / "sub-01" / "anat", "sub-01_ses-n_T1w", SC)
    series(r / "Lab" / "sub-02" / "func", "sub-02_task-x_bold", SC, (8, 8, 4, 5))
    series(r / "sub-03" / "anat", "sub-03_T1w", SC)


@case("session_walk_hidden_dirs")
def _(r):
    series(r / "sub-01" / "anat", "sub-01_T1w", SC)
    series(r / ".cache" / "sub-99" / "anat", "sub-99_ses-h_T1w", SC)
    text(r / "sub-01" / ".git" / "HEAD", "ref\n")


@case("session_walk_sessionless_beside_ses")
def _(r):
    s = r / "sub-01"
    series(s / "ses-1" / "anat", "sub-01_ses-1_T1w", {"AcquisitionTime": "09:00:00"})
    series(s / "ses-2" / "anat", "sub-01_ses-2_T1w", {"AcquisitionTime": "11:00:00"})
    series(s / "func", "sub-01_task-x_bold", SC, (8, 8, 4, 5))
    text(s / "notes" / "a.txt", "x")


@case("session_walk_sub_inside_datatype_and_file")
def _(r):
    series(r / "sub-01" / "anat", "sub-01_T1w", SC)
    series(r / "sub-01" / "anat" / "sub-02" / "anat", "sub-02_ses-q_T1w", SC)
    text(r / "sub-03", "a file, not a subject")
    series(r / "derivatives" / "fmriprep" / "sub-04" / "anat", "sub-04_ses-z_T1w", SC)


@case("session_walk_empty_subject_and_no_subjects")
def _(r):
    (r / "sub-01").mkdir()
    (r / "sub-02" / "ses-1").mkdir(parents=True)
    series(r / "sub-03" / "anat", "sub-03_T1w", SC)
