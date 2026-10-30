"""Unknown/-rescue, physio rescue, all-discard purge, non-reproin root collapse,
and the Unknown-leftover .bidsignore sweep."""
from synth import PROV_COLS, case, js, nii, prov, series, text

SUID = "1.2.840.99"
BOLD = (4, 4, 2, 10)


def u(r, sn, proto, guess, suid=SUID, dims=(4, 4, 2), extra=None, raw=None, exts=(), d="Unknown"):
    """Unknown/<SN>_<proto> image + sidecar; `exts` adds 2-line text companions."""
    stem = f"{sn}_{proto}"
    side = {"SeriesNumber": sn, "StudyInstanceUID": suid, "ProtocolName": proto}
    if guess is not None:
        side["BidsGuess"] = guess
    side.update(extra or {})
    nii(r / d / f"{stem}.nii.gz", dims)
    js(r / d / f"{stem}.json", side, raw)
    for e in exts:
        text(r / d / f"{stem}{e}", "0 1000\n")
    return stem


def row(sn, proto, pid="Sub_01", suid=SUID, date="20260512", time="120000.5", stem=None, **kw):
    return dict(StudyInstanceUID=suid, SeriesNumber=sn, ProtocolName=proto,
                OutputStem=stem or f"Unknown/{sn}_{proto}", PatientID=pid,
                StudyDate=date, StudyTime=time, **kw)


def physio(r, stem, label="cardiac", d="Unknown"):
    text(r / d / f"{stem}_recording-{label}_physio.tsv.gz", b"\x1f\x8b fake", binary=True)
    js(r / d / f"{stem}_recording-{label}_physio.json", {"SamplingFrequency": 400, "StartTime": 0, "Columns": ["cardiac"]})


# --- basic promotion -------------------------------------------------------

@case("rescue_anat_basic")
def _(r):
    u(r, 2, "mprage", ["anat", "_T1w"])
    prov(r, [row(2, "mprage")])


@case("rescue_dwi_companions")
def _(r):
    u(r, 5, "diff", ["dwi", "_dir-AP_dwi"], dims=(4, 4, 2, 3), exts=(".bval", ".bvec"))
    prov(r, [row(5, "diff")])


@case("rescue_func_task_from_protocol")
def _(r):
    u(r, 3, "my_task-nback_scan", ["func", "_bold"], dims=BOLD)
    u(r, 4, "resting", ["func", "_bold"], dims=BOLD, extra={"SeriesDescription": "x task-faces y"})
    u(r, 6, "plain", ["func", "_bold"], dims=BOLD)
    u(r, 7, "has", ["func", "_task-given_bold"], dims=BOLD, extra={"ProtocolName": "task-ignored"})
    prov(r, [row(3, "my_task-nback_scan"), row(4, "resting"), row(6, "plain"), row(7, "has")])


@case("rescue_run_entity_normalisation")
def _(r):
    u(r, 8, "foo_run-2_task-motor", ["func", "_run-8_echo-1_bold"], dims=BOLD)
    u(r, 9, "bar", ["anat", "_acq-x_run-9_part-mag_T2starw"])
    u(r, 10, "baz_run-sedley", ["anat", "_run-10_T2w"])
    u(r, 11, "q", ["anat", "_FLAIR"], extra={"SeriesDescription": "run-007"})
    prov(r, [row(8, "foo_run-2_task-motor"), row(9, "bar"), row(10, "baz_run-sedley"), row(11, "q")])


@case("rescue_subject_token_sanitised")
def _(r):
    u(r, 2, "a", ["anat", "_T1w"])
    u(r, 3, "b", ["anat", "_T2w"], suid="9.9")
    prov(r, [row(2, "a", pid="Sub-01_A.b"), row(3, "b", pid="José Ü", suid="9.9", time="080000")])


@case("rescue_datatype_case_and_whitespace")
def _(r):
    u(r, 2, "a", [" anat ", "_T1w"])
    u(r, 3, "b", ["Fmap", "_epi"])
    prov(r, [row(2, "a"), row(3, "b")])


# --- skips / path safety --------------------------------------------------

@case("rescue_path_safety_rejections")
def _(r):
    u(r, 2, "trav", ["../etc", "_T1w"])
    u(r, 3, "badsuf", ["anat", "_T1w/../../x"])
    u(r, 4, "empty", ["anat", ""])
    u(r, 5, "ct", ["ct", "_ct"])
    u(r, 6, "blankdt", ["  ", "_T1w"])
    u(r, 7, "short", ["anat"])
    u(r, 8, "notlist", "anat_T1w")
    u(r, 9, "noguess", None)
    u(r, 10, "nonstr", [None, 5])
    u(r, 11, "numsuf", ["anat", 5])
    u(r, 12, "dotsuf", ["anat", "_T1w.x"])
    prov(r, [row(n, p) for n, p in ((2, "trav"), (3, "badsuf"), (4, "empty"), (5, "ct"), (6, "blankdt"), (7, "short"),
                                    (8, "notlist"), (9, "noguess"), (10, "nonstr"), (11, "numsuf"), (12, "dotsuf"))])


@case("rescue_provenance_missing_and_mismatch")
def _(r):
    u(r, 2, "nosuid", ["anat", "_T1w"], suid="other")
    u(r, 3, "nopid", ["anat", "_T2w"])
    u(r, 4, "baddate", ["anat", "_PDw"])
    u(r, 5, "badtime", ["anat", "_FLAIR"])
    u(r, 6, "ok", ["anat", "_T1w"])
    u(r, 7, "noprov", ["anat", "_T2starw"])
    prov(r, [row(2, "nosuid"), row(3, "nopid", pid="-_-"), row(4, "baddate", date="2026-05-12"),
             row(5, "badtime", time="12:00:00"), row(6, "ok")])


@case("rescue_no_provenance_file")
def _(r):
    u(r, 2, "a", ["anat", "_T1w"])
    u(r, 3, "loc", ["discard", "_loc"])


@case("rescue_provenance_header_only")
def _(r):
    u(r, 2, "a", ["anat", "_T1w"])
    prov(r, [])


@case("rescue_provenance_short_rows_and_extra_cols")
def _(r):
    u(r, 2, "a", ["anat", "_T1w"])
    u(r, 3, "b", ["anat", "_T2w"])
    hdr = PROV_COLS + ["Extra"]
    good = row(3, "b")
    lines = ["\t".join(hdr),
             "\t".join(str(row(2, "a").get(c, "")) for c in PROV_COLS[:5]),
             "\t".join(str(good.get(c, "")) for c in PROV_COLS) + "\tz"]
    text(r / ".reproin_provenance.tsv", "\n".join(lines) + "\n")


@case("rescue_provenance_crlf_pid_last")
def _(r):
    u(r, 2, "a", ["anat", "_T1w"])
    lines = ["\t".join(PROV_COLS), "\t".join(str(row(2, "a").get(c, "")) for c in PROV_COLS)]
    text(r / ".reproin_provenance.tsv", "\r\n".join(lines) + "\r\n", binary=False)


@case("rescue_provenance_crlf_reordered")
def _(r):
    u(r, 2, "a", ["anat", "_T1w"])
    cols = [c for c in PROV_COLS if c != "ProtocolName"] + ["ProtocolName"]
    rr = row(2, "ses-pre_a")
    rr["OutputStem"] = "Unknown/2_a"
    lines = ["\t".join(cols), "\t".join(str(rr.get(c, "")) for c in cols)]
    (r / ".reproin_provenance.tsv").write_bytes(("\r\n".join(lines) + "\r\n").encode())


@case("rescue_seriesnumber_coercions")
def _(r):
    u(r, 2, "str", ["anat", "_T1w"], extra={"SeriesNumber": "2"})
    u(r, 3, "flt", ["anat", "_T2w"], extra={"SeriesNumber": 3.9})
    u(r, 4, "bool", ["anat", "_PDw"], extra={"SeriesNumber": True})
    u(r, 5, "null", ["anat", "_FLAIR"], extra={"SeriesNumber": None})
    u(r, 6, "junk", ["anat", "_T2starw"], extra={"SeriesNumber": "6.0"})
    u(r, 7, "nosn", ["anat", "_inplaneT1"], extra={"SeriesNumber": "x"})
    prov(r, [row(2, "str"), row(3, "flt"), row(1, "bool"), row(0, "zero")])


@case("rescue_prov_seriesnumber_formats")
def _(r):
    u(r, 7, "pad", ["anat", "_T1w"])
    u(r, 8, "plus", ["anat", "_T2w"])
    u(r, 9, "dup", ["anat", "_PDw"])
    prov(r, [row(" 07", "pad"), row("+8", "plus"), row(9, "dup", pid="first"), row("09", "dup", pid="second")])


@case("rescue_malformed_json_skipped")
def _(r):
    u(r, 2, "bad", None, raw="{not json")
    u(r, 3, "bin", None, raw="")
    (r / "Unknown" / "3_bin.json").write_bytes(b'{"BidsGuess": ["anat", "_T1w"], "X": "\xff"}')
    u(r, 4, "ok", ["anat", "_T1w"])
    prov(r, [row(2, "bad"), row(3, "bin"), row(4, "ok")])


@case("rescue_nonobject_json")
def _(r):
    u(r, 2, "list", None, raw='["anat", "_T1w"]')
    u(r, 3, "ok", ["anat", "_T1w"])
    prov(r, [row(2, "list"), row(3, "ok")])


@case("rescue_nonobject_json_strict", "--strict")
def _(r):
    u(r, 2, "list", None, raw='"just a string"')
    u(r, 3, "ok", ["anat", "_T1w"])
    prov(r, [row(2, "list"), row(3, "ok")])


@case("rescue_seriesnumber_infinity", divergence="D2 non-finite JSON")
def _(r):
    u(r, 2, "inf", None, raw='{"BidsGuess": ["anat", "_T1w"], "SeriesNumber": Infinity, "StudyInstanceUID": "1.2.840.99"}')
    u(r, 3, "ok", ["anat", "_T2w"])
    prov(r, [row(3, "ok")])


# --- sessions --------------------------------------------------------------

@case("rescue_study_session_from_protocol")
def _(r):
    u(r, 2, "ses-pre_task-bernd_bold", ["func", "_bold"], dims=BOLD)
    u(r, 3, "mprage", ["anat", "_T1w"])
    prov(r, [row(2, "ses-pre_task-bernd_bold"), row(3, "mprage")])


@case("rescue_study_session_conflict_and_desc")
def _(r):
    # study A: conflicting ses- tokens -> timestamp; study B: ses- via SeriesDescription with non-alnum tail
    u(r, 2, "ses-a_x", ["anat", "_T1w"])
    u(r, 3, "ses-b_y", ["anat", "_T2w"])
    u(r, 4, "z", ["anat", "_T1w"], suid="2.2")
    u(r, 5, "w", ["anat", "_T2w"], suid="2.2")
    prov(r, [row(2, "ses-a_x"), row(3, "ses-b_y"), row(4, "z", suid="2.2", SeriesDescription="foo_ses-post-op"),
             row(5, "w", suid="2.2", SeriesDescription="ses-post"), row(6, "ses-c_q", time="000000"),
             row(7, "ses-c_q2", suid="", time="000000")])


@case("rescue_two_studies_same_patient")
def _(r):
    u(r, 2, "t1", ["anat", "_T1w"])
    u(r, 2, "t1b", ["anat", "_T1w"], suid="3.3")
    prov(r, [row(2, "t1"), row(2, "t1b", suid="3.3", date="20260601", time="093000")])


@case("rescue_existing_reproin_subject_sessionless")
def _(r):
    series(r / "sub-sub01" / "anat", "sub-sub01_T2w", {"SeriesNumber": 1})
    u(r, 2, "t1", ["anat", "_T1w"])
    prov(r, [row(2, "t1"), row(1, "anat-T2w", stem="sub-sub01/anat/sub-sub01_T2w")])


# --- collisions ------------------------------------------------------------

@case("rescue_collision_seriesnumber_order")
def _(r):
    u(r, 10, "t1late", ["anat", "_T1w"])
    u(r, 2, "t1early", ["anat", "_run-2_T1w"])
    u(r, 3, "mid", ["anat", "_part-mag_T1w"])
    u(r, 30, "mid2", ["anat", "_part-mag_T1w"])
    prov(r, [row(10, "t1late"), row(2, "t1early"), row(3, "mid"), row(30, "mid2")])


@case("rescue_collision_preexisting_family")
def _(r):
    d = r / "sub-sub01" / "ses-20260512T120000" / "anat"
    series(d, "sub-sub01_ses-20260512T120000_T1w", {"Pre": 1})
    series(d, "sub-sub01_ses-20260512T120000_run-01_T1w", {"Pre": 2})
    text(d / "sub-sub01_ses-20260512T120000_run-03_T1w.bval", "0\n")
    u(r, 4, "a", ["anat", "_T1w"])
    u(r, 5, "b", ["anat", "_T1w"])
    prov(r, [row(4, "a"), row(5, "b")])


@case("rescue_collision_single_vs_existing")
def _(r):
    d = r / "sub-sub01" / "ses-20260512T120000" / "anat"
    js(d / "sub-sub01_ses-20260512T120000_T2w.json", {"Pre": 1})
    u(r, 4, "a", ["anat", "_T2w"])
    prov(r, [row(4, "a")])


@case("rescue_collision_tsv_companion")
def _(r):
    d = r / "sub-sub01" / "ses-20260512T120000" / "anat"
    text(d / "sub-sub01_ses-20260512T120000_T1w.tsv", "curated\n")
    u(r, 4, "a", ["anat", "_T1w"], exts=(".tsv",))
    prov(r, [row(4, "a")])


@case("rescue_collision_tsv_companion_strict", "--strict")
def _(r):
    d = r / "sub-sub01" / "ses-20260512T120000" / "anat"
    text(d / "sub-sub01_ses-20260512T120000_T1w.tsv", "curated\n")
    u(r, 4, "a", ["anat", "_T1w"], exts=(".tsv",))
    prov(r, [row(4, "a")])


@case("rescue_orphan_image_without_json")
def _(r):
    u(r, 2, "a", ["anat", "_T1w"])
    nii(r / "Unknown" / "3_lonely.nii.gz", (4, 4, 2))
    text(r / "Unknown" / "3_lonely.bvec", "0\n")
    prov(r, [row(2, "a"), row(3, "lonely")])


# --- derived / discard routing & purge ------------------------------------

@case("rescue_discard_derived_routing", "--keep-derivatives")
def _(r):
    u(r, 2, "loc", ["discard", "_loc"])
    u(r, 3, "fa", [" Derived ", "_FA"], exts=(".bval",))
    u(r, 4, "t1", ["anat", "_T1w"])
    prov(r, [row(4, "t1")])


@case("rescue_discard_routing_dropped")
def _(r):
    u(r, 2, "loc", ["discard", "_loc"])
    u(r, 3, "fa", ["derived", "_FA"])
    prov(r, [row(9, "unrelated")])


@case("rescue_discard_dest_exists", "--keep-derivatives")
def _(r):
    u(r, 2, "loc", ["discard", "_loc"])
    js(r / "derivatives" / "scanner" / "Unknown" / "2_loc.json", {"curated": True})
    prov(r, [row(9, "x")])


@case("rescue_purge_short_discard_guess")
def _(r):
    # len-1 BidsGuess skipped by rescue, then purged as all-discard
    u(r, 2, "loc", ["DISCARD"])
    u(r, 3, "loc2", ["discard"], exts=(".bval",))
    prov(r, [row(2, "loc")])


@case("rescue_purge_blocked_by_orphan_hidden_nested")
def _(r):
    u(r, 2, "loc", ["discard"])
    text(r / "Unknown" / ".DS_Store", "x")
    u(r, 3, "loc", ["discard"], d="Unknown/nested")
    prov(r, [row(2, "loc")])


@case("rescue_purge_blocked_by_nested_dir_only")
def _(r):
    u(r, 2, "loc", ["discard"])
    (r / "Unknown" / "emptydir").mkdir()
    prov(r, [row(2, "loc")])


@case("rescue_purge_nonobject_json")
def _(r):
    u(r, 2, "loc", ["discard"])
    js(r / "Unknown" / "3_x.json", None, raw="[1, 2]")


@case("rescue_hidden_json_in_unknown")
def _(r):
    u(r, 2, "a", ["anat", "_T1w"])
    js(r / "Unknown" / ".hidden.json", {"BidsGuess": ["anat", "_T2w"], "SeriesNumber": 2, "StudyInstanceUID": SUID})
    prov(r, [row(2, "a")])


# --- physio ---------------------------------------------------------------

@case("rescue_physio_pairs_with_bold")
def _(r):
    u(r, 5, "rest_task-rest_run-1", ["func", "_bold"], dims=BOLD)
    physio(r, "6_rest_task-rest_run-1")
    physio(r, "6_rest_task-rest_run-1", "respiratory")
    prov(r, [row(5, "rest_task-rest_run-1")])


@case("rescue_physio_multiecho_drops_echo")
def _(r):
    u(r, 5, "me_task-motor", ["func", "_echo-1_bold"], dims=BOLD)
    u(r, 6, "me_task-motor", ["func", "_echo-2_bold"], dims=BOLD, d="Unknown2")
    # second echo shares the protocol: put it in Unknown/ under a distinct SN
    for ext in (".nii.gz", ".json"):
        (r / "Unknown2" / f"6_me_task-motor{ext}").rename(r / "Unknown" / f"7_me_task-motor{ext}")
    (r / "Unknown2").rmdir()
    import json
    jp = r / "Unknown" / "7_me_task-motor.json"
    d = json.loads(jp.read_text()); d["SeriesNumber"] = 7; jp.write_text(json.dumps(d))
    physio(r, "8_me_task-motor")
    prov(r, [row(5, "me_task-motor"), row(7, "me_task-motor")])


@case("rescue_physio_ambiguous_subject")
def _(r):
    u(r, 5, "rest_task-rest", ["func", "_bold"], dims=BOLD)
    u(r, 5, "rest_task-rest", ["func", "_bold"], dims=BOLD, suid="7.7", d="U2")
    for ext in (".nii.gz", ".json"):
        (r / "U2" / f"5_rest_task-rest{ext}").rename(r / "Unknown" / f"15_rest_task-rest{ext}")
    (r / "U2").rmdir()
    import json
    jp = r / "Unknown" / "15_rest_task-rest.json"
    d = json.loads(jp.read_text()); d["SeriesNumber"] = 15; jp.write_text(json.dumps(d))
    physio(r, "6_rest_task-rest")
    prov(r, [row(5, "rest_task-rest"), row(15, "rest_task-rest", pid="other", suid="7.7")])


@case("rescue_physio_no_task_no_match")
def _(r):
    u(r, 5, "rest", ["func", "_bold"], dims=BOLD)
    physio(r, "6_rest")
    physio(r, "badname_recording-x_physio".replace("badname", "nonnum"))
    prov(r, [row(5, "rest")])


@case("rescue_physio_dest_exists")
def _(r):
    u(r, 5, "r_task-rest", ["func", "_bold"], dims=BOLD)
    physio(r, "6_r_task-rest")
    js(r / "sub-sub01" / "ses-20260512T120000" / "func" / "sub-sub01_ses-20260512T120000_task-rest_recording-cardiac_physio.json", {"curated": 1})
    prov(r, [row(5, "r_task-rest")])


@case("rescue_physio_json_only_and_sanitised_proto")
def _(r):
    u(r, 5, "my_proto_task-go", ["func", "_bold"], dims=BOLD, extra={"ProtocolName": "my proto task-go"})
    js(r / "Unknown" / "6_my_proto_task-go_recording-pulse_physio.json", {"SamplingFrequency": 50})
    prov(r, [row(5, "my proto task-go", stem="Unknown/5_my_proto_task-go")])


# --- collapse ---------------------------------------------------------------

@case("rescue_collapse_all_unknown")
def _(r):
    s = r / "Study" / "Study"
    u(s, 2, "t1", ["anat", "_T1w"])
    u(s, 3, "loc", ["discard", "_loc"])
    prov(s, [row(2, "t1"), row(3, "loc", stem="derivatives/scanner/3_loc")])


@case("rescue_collapse_blocked_by_reproin_row")
def _(r):
    s = r / "Smith" / "Aging"
    u(s, 2, "t1", ["anat", "_T1w"])
    series(s / "sub-x" / "anat", "sub-x_T2w", {})
    prov(s, [row(2, "t1"), row(1, "anat-T2w", stem="sub-x/anat/sub-x_T2w")])


@case("rescue_collapse_blocked_by_sibling")
def _(r):
    s = r / "Study" / "Study"
    u(s, 2, "t1", ["anat", "_T1w"])
    prov(s, [row(2, "t1")])
    text(r / "Study" / "notes.txt", "hi")


@case("rescue_collapse_blocked_by_hidden_root_file")
def _(r):
    s = r / "Study"
    u(s, 2, "t1", ["anat", "_T1w"])
    prov(s, [row(2, "t1")])
    text(r / ".DS_Store", "x")


@case("rescue_collapse_name_collision")
def _(r):
    s = r / "A"
    u(s, 2, "t1", ["anat", "_T1w"])
    text(s / "A" / "keep.txt", "x")
    prov(s, [row(2, "t1")])


@case("rescue_collapse_empty_provenance")
def _(r):
    s = r / "Study"
    u(s, 2, "t1", ["anat", "_T1w"])
    prov(s, [])


@case("rescue_multi_study_no_collapse")
def _(r):
    for name, pid, suid in (("Alpha", "p1", "1.1"), ("Beta", "p2", "2.2")):
        s = r / name
        u(s, 2, "t1", ["anat", "_T1w"], suid=suid)
        u(s, 3, "ct", ["ct", "_ct"], suid=suid)
        prov(s, [row(2, "t1", pid=pid, suid=suid), row(3, "ct", pid=pid, suid=suid)])


@case("rescue_collapse_tsv_at_root")
def _(r):
    u(r, 2, "t1", ["anat", "_T1w"])
    prov(r, [row(2, "t1")])
    s = r / "Other"
    u(s, 3, "t2", ["anat", "_T2w"])


# --- leftover sweep ---------------------------------------------------------

@case("rescue_leftover_bidsignore_existing")
def _(r):
    u(r, 2, "ct", ["ct", "_ct"])
    u(r, 3, "t1", ["anat", "_T1w"])
    text(r / "Unknown" / "deep" / "er" / "f.txt", "x")
    text(r / "Unknown" / ".hidden", "x")
    text(r / ".bidsignore", "curated/\nUnknown/2_ct.json")
    prov(r, [row(3, "t1")])


@case("rescue_unknown_only_ct_no_subjects")
def _(r):
    u(r, 2, "ct", ["ct", "_ct"])
    prov(r, [row(2, "ct")])


@case("rescue_nested_unknown_in_unknown")
def _(r):
    u(r, 2, "t1", ["anat", "_T1w"])
    u(r, 4, "t2", ["anat", "_T2w"], d="Unknown/Unknown")
    prov(r, [row(2, "t1")])
    prov(r / "Unknown", [row(4, "t2", pid="inner")])


@case("rescue_unknown_is_file")
def _(r):
    text(r / "Unknown", "not a dir")
    series(r / "sub-01" / "anat", "sub-01_T1w", {})


@case("rescue_unknown_under_derivatives_ignored", "--keep-derivatives")
def _(r):
    series(r / "sub-01" / "anat", "sub-01_T1w", {})
    u(r, 2, "t1", ["anat", "_T2w"], d="derivatives/scanner/Unknown")
    prov(r, [row(2, "t1")])


@case("rescue_json_nan_field", divergence="D2 non-finite JSON")
def _(r):
    u(r, 2, "nan", None, raw='{"BidsGuess": ["anat", "_T1w"], "SeriesNumber": 2, "StudyInstanceUID": "1.2.840.99", "EchoTime": NaN}')
    u(r, 3, "nansn", None, raw='{"BidsGuess": ["anat", "_T2w"], "SeriesNumber": NaN, "StudyInstanceUID": "1.2.840.99"}')
    prov(r, [row(2, "nan"), row(0, "nansn")])
