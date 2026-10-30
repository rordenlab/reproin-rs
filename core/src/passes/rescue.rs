//! Pre-passes: non-ReproIn hierarchy collapse, Unknown/ rescue, physio
//! stranded in Unknown/, all-discard purge.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde_json::Value;

use super::*;
use crate::tables::{PROVENANCE_TSV, is_datatype};

re!(BIDS_ENTITY_SUFFIX_RE, r"\A[A-Za-z0-9_-]+\z");
re!(
    PHYSIO_STEM_RE,
    r"^[0-9]+_(?P<proto>.+)_recording-(?P<label>[A-Za-z0-9]+)_physio$"
);
re!(LEADING_SN_RE, r"^[0-9]+_(.+)$");

/// `int(digits):02d` without overflow.
fn pad2(digits: &str) -> String {
    let d = digits.trim_start_matches('0');
    format!("{:0>2}", if d.is_empty() { "0" } else { d })
}

/// `_maybe_collapse_nonreproin_root`.
pub fn maybe_collapse_nonreproin_root(cx: &mut Cx, out_root: &str) -> Res<bool> {
    let fs = &mut *cx.fs;
    let tsvs = fs.rglob(out_root, PROVENANCE_TSV);
    if tsvs.len() != 1 {
        return Ok(false);
    }
    let study_root = parent(&tsvs[0]).to_string();
    if study_root == out_root {
        return Ok(false);
    }
    let mut chain = Vec::new();
    let mut cursor = study_root.clone();
    while cursor != out_root {
        chain.push(cursor.clone());
        let p = parent(&cursor).to_string();
        let sib = fs.iterdir(&p);
        if sib.len() != 1 || sib[0] != cursor {
            return Ok(false);
        }
        cursor = p;
    }
    let rows = load_provenance(fs, &study_root);
    if rows.is_empty() || rows.iter().any(|r| r.get("OutputStem").starts_with("sub-")) {
        return Ok(false);
    }
    let children = fs.iterdir(&study_root);
    if children.iter().any(|c| fs.exists(&join(out_root, name(c)))) {
        return Ok(false);
    }
    for c in &children {
        fs.replace(c, &join(out_root, name(c)))?;
    }
    for d in &chain {
        let _ = fs.rmdir(d);
    }
    Ok(true)
}

/// `_purge_all_discard_unknown`.
pub fn purge_all_discard_unknown(cx: &mut Cx, root: &str) -> Res<bool> {
    let fs = &mut *cx.fs;
    let unknown = join(root, "Unknown");
    if !fs.is_dir(&unknown) {
        return Ok(false);
    }
    let jsons = fs.glob(&unknown, "*.json");
    if jsons.is_empty() {
        return Ok(false);
    }
    let mut discard = BTreeSet::new();
    for jp in &jsons {
        let Some(d) = fs.try_obj(jp)? else {
            return Ok(false);
        };
        let ok = match d.get("BidsGuess") {
            Some(Value::Array(g)) if !g.is_empty() => {
                py_str(g.first()).trim().to_lowercase() == "discard"
            }
            _ => false,
        };
        if !ok {
            return Ok(false);
        }
        discard.insert(name(jp).strip_suffix(".json").unwrap().to_string());
    }
    for p in fs.iterdir(&unknown) {
        if fs.is_dir(&p) {
            return Ok(false);
        }
        let sfx = vfs::suffix(&p);
        if !fs.is_file(&p) || sfx.to_lowercase() == ".json" {
            continue;
        }
        let n = name(&p);
        let stem = n
            .strip_suffix(".nii.gz")
            .unwrap_or(&n[..n.len() - sfx.len()]);
        if !discard.contains(stem) {
            return Ok(false);
        }
    }
    fs.rmtree(&unknown)?;
    Ok(true)
}

struct Candidate {
    target_dir: String,
    base_stem: String,
    run_template: String,
    src_stem: String,
    sn: i64,
    json_name: String,
    chosen: Option<String>,
}

/// `_rescue_unknown_dir`.
pub fn rescue_unknown_dir(cx: &mut Cx, root: &str) -> Res<usize> {
    let unknown = join(root, "Unknown");
    if !cx.fs.is_dir(&unknown) {
        return Ok(0);
    }
    let rows = load_provenance(cx.fs, root);
    if rows.is_empty() {
        return Ok(0);
    }
    let mut prov: HashMap<(String, i64), usize> = HashMap::new();
    for (i, r) in rows.iter().enumerate() {
        prov.insert((r.get("StudyInstanceUID").to_string(), r.sn), i);
    }
    // Study-scoped `ses-<X>` recovery for session-first protocols.
    let mut study_session: HashMap<String, String> = HashMap::new();
    let mut blocked: BTreeSet<String> = BTreeSet::new();
    for r in &rows {
        let suid = r.get("StudyInstanceUID");
        if suid.is_empty() || blocked.contains(suid) {
            continue;
        }
        let label = ["ProtocolName", "SeriesDescription"]
            .iter()
            .find_map(|f| SES_PROTOCOL_RE.captures(r.get(f)))
            .map(|m| {
                m[1].chars()
                    .filter(|c| c.is_ascii_alphanumeric())
                    .collect::<String>()
            })
            .unwrap_or_default();
        if label.is_empty() {
            continue;
        }
        match study_session.get(suid) {
            None => {
                study_session.insert(suid.to_string(), label);
            }
            Some(prev) if *prev != label => {
                study_session.remove(suid);
                blocked.insert(suid.to_string());
            }
            _ => {}
        }
    }
    let mut derived_moved = 0;
    let mut cands: Vec<Candidate> = Vec::new();
    for jp in cx.fs.glob(&unknown, "*.json") {
        let Some(data) = cx.fs.try_obj(&jp)? else {
            continue;
        };
        let Some(Value::Array(guess)) = data.get("BidsGuess") else {
            continue;
        };
        if guess.len() < 2 {
            continue;
        }
        let datatype = py_str(guess.first()).trim().to_string();
        let mut es = py_str(guess.get(1));
        let src_stem = name(&jp).strip_suffix(".json").unwrap().to_string();
        let dl = datatype.to_lowercase();
        if dl == "discard" || dl == "derived" {
            let dest = join(root, "derivatives/scanner/Unknown");
            let r = cx.fs.mkdirs(&dest).and_then(|_| {
                move_stem_files(cx.fs, &join(&unknown, &src_stem), &join(&dest, &src_stem))
            });
            match r {
                Ok(_) => derived_moved += 1,
                Err(Error::Exists(_)) => {}
                Err(e) if cx.opts.strict => return Err(e),
                Err(_) => {}
            }
            continue;
        }
        if dl.is_empty() || !is_datatype(&dl) || !BIDS_ENTITY_SUFFIX_RE.is_match(&es) {
            continue;
        }
        let fields = [
            py_str(data.get("ProtocolName")),
            py_str(data.get("SeriesDescription")),
        ];
        if dl == "func" && !es.contains("_task-") {
            let task = fields
                .iter()
                .find_map(|s| TASK_RE.captures(s))
                .map_or("rest".into(), |m| m[1].to_string());
            es = format!("_task-{task}{es}");
        }
        es = RUN_ENTITY_RE.replace_all(&es, "").into_owned();
        if let Some(run) = fields.iter().find_map(|s| RUN_PROTOCOL_RE.captures(s)) {
            es = insert_run_entity(&es, &format!("run-{}", pad2(&run[1])));
        }
        let suid = py_str(data.get("StudyInstanceUID"));
        let sn = py_int_value(data.get("SeriesNumber")).unwrap_or(0);
        let Some(&ri) = prov.get(&(suid.clone(), sn)) else {
            continue;
        };
        let row = &rows[ri];
        let subj = heudiconv_subject_token(row.get("PatientID"));
        let mut sess = study_session.get(&suid).cloned().unwrap_or_default();
        if sess.is_empty() {
            sess = session_token_from_studydatetime(row.get("StudyDate"), row.get("StudyTime"));
        }
        if subj.is_empty() || sess.is_empty() {
            continue;
        }
        let (sub, ses) = (format!("sub-{subj}"), format!("ses-{sess}"));
        cands.push(Candidate {
            target_dir: join(root, &format!("{sub}/{ses}/{datatype}")),
            base_stem: format!("{sub}_{ses}{es}"),
            run_template: format!("{sub}_{ses}{}", insert_run_entity(&es, "run-{idx}")),
            src_stem,
            sn,
            json_name: name(&jp).to_string(),
            chosen: None,
        });
    }
    let family_exists = |fs: &Vfs, dir: &str, stem: &str| {
        [".nii.gz", ".nii", ".json", ".bvec", ".bval"]
            .iter()
            .any(|e| fs.exists(&join(dir, &format!("{stem}{e}"))))
    };
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new(); // first index -> members
    let mut key_first: HashMap<(String, String), usize> = HashMap::new();
    for (i, c) in cands.iter().enumerate() {
        let first = *key_first
            .entry((c.target_dir.clone(), c.base_stem.clone()))
            .or_insert(i);
        groups.entry(first).or_default().push(i);
    }
    for (_, mut members) in groups {
        let (dir, base) = (
            cands[members[0]].target_dir.clone(),
            cands[members[0]].base_stem.clone(),
        );
        if members.len() == 1 && !family_exists(cx.fs, &dir, &base) {
            cands[members[0]].chosen = Some(base);
            continue;
        }
        members.sort_by_key(|&i| cands[i].sn);
        let tmpl = cands[members[0]].run_template.clone();
        let fill = |idx: usize| tmpl.replace("{idx}", &format!("{idx:02}"));
        let mut free = (1..100).filter(|&i| !family_exists(cx.fs, &dir, &fill(i)));
        let picks: Vec<Option<usize>> = members.iter().map(|_| free.next()).collect();
        if picks.iter().all(Option::is_some) {
            for (&m, p) in members.iter().zip(picks) {
                cands[m].chosen = Some(
                    cands[m]
                        .run_template
                        .replace("{idx}", &format!("{:02}", p.unwrap())),
                );
            }
        }
    }
    let mut rescued = 0;
    for c in &cands {
        let Some(chosen) = &c.chosen else { continue };
        match move_stem_files(
            cx.fs,
            &join(&unknown, &c.src_stem),
            &join(&c.target_dir, chosen),
        ) {
            Ok(n) if n > 0 => rescued += 1,
            Ok(_) => {}
            Err(Error::Exists(_)) => {
                cx.log(format!(
                    "reproinx: Unknown/-rescue collision — kept {} (target {chosen} already present)",
                    c.json_name
                ));
                if cx.opts.strict {
                    return err(format!("{chosen} already exists"));
                }
            }
            Err(e) => {
                cx.log(format!(
                    "reproinx: Unknown/-rescue failed for {}: {e}",
                    c.json_name
                ));
                if cx.opts.strict {
                    return Err(e);
                }
            }
        }
    }
    rescued += rescue_unknown_physio(cx, &unknown, root, &rows)?;
    if derived_moved > 0 {
        cx.log(format!("  {root}: moved {derived_moved} derived/discard stem(s) from Unknown/ to derivatives/scanner/"));
    }
    if cx.fs.is_dir(&unknown) && cx.fs.iterdir(&unknown).is_empty() {
        let _ = cx.fs.rmdir(&unknown);
    }
    Ok(rescued)
}

/// `_session_token_from_studydatetime`.
fn session_token_from_studydatetime(sd: &str, st: &str) -> String {
    let (sd, st) = (sd.trim(), st.trim());
    let digits =
        |s: &str, n: usize| s.len() >= n && s.as_bytes()[..n].iter().all(u8::is_ascii_digit);
    if !digits(sd, 8) || !digits(st, 6) {
        return String::new();
    }
    format!("{}T{}", &sd[..8], &st[..6])
}

/// `_physio_match_key`.
fn physio_match_key(stem: &str) -> (Option<String>, Option<String>) {
    (
        TASK_TOKEN_RE.captures(stem).map(|m| m[1].to_string()),
        RUN_PROTOCOL_RE.captures(stem).map(|m| pad2(&m[1])),
    )
}

/// `_rescue_unknown_physio`.
fn rescue_unknown_physio(cx: &mut Cx, unknown: &str, root: &str, rows: &[Row]) -> Res<usize> {
    let mut stems = BTreeSet::new();
    for p in cx.fs.glob(unknown, "*_recording-*_physio.tsv.gz") {
        stems.insert(name(&p).strip_suffix(".tsv.gz").unwrap().to_string());
    }
    for p in cx.fs.glob(unknown, "*_recording-*_physio.json") {
        stems.insert(name(&p).strip_suffix(".json").unwrap().to_string());
    }
    if stems.is_empty() {
        return Ok(0);
    }
    let mut proto_subjects: HashMap<String, BTreeSet<String>> = HashMap::new();
    for r in rows {
        let stem = r.get("OutputStem").replace('\\', "/");
        let base = name(&stem);
        let Some(m) = LEADING_SN_RE.captures(base) else {
            continue;
        };
        let subj = heudiconv_subject_token(r.get("PatientID"));
        if !subj.is_empty() {
            proto_subjects
                .entry(m[1].to_string())
                .or_default()
                .insert(subj);
        }
    }
    let mut moved = 0;
    for stem in stems {
        let Some(m) = PHYSIO_STEM_RE.captures(&stem) else {
            continue;
        };
        let key = physio_match_key(&stem);
        let Some(subs) = proto_subjects.get(&m["proto"]).filter(|s| s.len() == 1) else {
            continue;
        };
        let scope = join(root, &format!("sub-{}", subs.iter().next().unwrap()));
        if !cx.fs.is_dir(&scope) {
            continue;
        }
        let mut bases = BTreeSet::new();
        let mut bolds = cx.fs.rglob(&scope, "*_bold.nii.gz");
        bolds.extend(cx.fs.rglob(&scope, "*_bold.nii"));
        for nii in bolds {
            let bstem = nii_stem(name(&nii));
            if physio_match_key(bstem) != key {
                continue;
            }
            let base = ECHO_TOKEN_RE.replace_all(bstem, "");
            let base = base.strip_suffix("_bold").unwrap_or(&base).to_string();
            bases.insert((parent(&nii).to_string(), base));
        }
        if bases.len() != 1 {
            continue;
        }
        let (dir, base) = bases.into_iter().next().unwrap();
        let dst = join(&dir, &format!("{base}_recording-{}_physio", &m["label"]));
        match move_stem_files(cx.fs, &join(unknown, &stem), &dst) {
            Ok(n) if n > 0 => moved += 1,
            Ok(_) | Err(Error::Exists(_)) => {}
            Err(e) if cx.opts.strict => return Err(e),
            Err(_) => {}
        }
    }
    Ok(moved)
}
