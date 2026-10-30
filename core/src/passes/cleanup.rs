//! `_bidsguess_cleanup`: per-session reclassification and hygiene, then
//! per-root `.bidsignore`.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde_json::{Map, Value};

use super::*;
use crate::tables::{FMAP_EPI_ENTITY_ORDER, PART_FUNC_SUFFIXES};

re!(PART_SUFFIX_RE, r"_part-[A-Za-z0-9]+$");
re!(PART_GUESS_RE, r"_part-(mag|phase|real|imag)");
re!(
    COLLISION_RE,
    r"_(magnitude[0-9]|phasediff|phase[0-9]|fieldmap|bold|sbref|T1w|T2w|FLAIR|PDw|T2starw|UNIT1|inplaneT[12]|MEGRE|MESE|VFA|IRT1|MP2RAGE|MPM|MTS|MTR|dwi|epi|m0scan|asl|aslcontext|cbv|noRF|defacemask|FA|ADC|colFA|expADC|trace|S0map|TENSOR|svs|mrsi|unloc|mrsref)[a-z](\.json|\.nii(\.gz)?|\.bvec|\.bval|\.tsv)$"
);

fn nii_files(fs: &Vfs, dir: &str, a: &str, b: &str) -> Vec<String> {
    let mut v = fs.glob(dir, a);
    v.extend(fs.glob(dir, b));
    vfs::sort_paths(&mut v);
    v
}

/// `_bidsguess_cleanup`.
pub fn bidsguess_cleanup(cx: &mut Cx, out_root: &str) -> Res<()> {
    let mut by_root: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for ses in walk_sessions(cx.fs, out_root) {
        let root = bids_root_for_session(cx.fs, &ses);
        match reclassify_session(cx.fs, &ses, cx.opts.min_volumes) {
            Ok(n) if n > 0 => cx.log(format!(
                "  {ses}: reclassified {n} series via session context"
            )),
            Ok(_) => {}
            Err(e) => cx.fail(format!("session reclassification failed for {ses}: {e}"))?,
        }
        match strip_orphan_runs(cx.fs, &ses) {
            Ok(n) if n > 0 => cx.log(format!("  {ses}: stripped {n} non-disambiguating run(s)")),
            Ok(_) => {}
            Err(e) => cx.fail(format!("orphan-run strip failed for {ses}: {e}"))?,
        }
        let discard = join(&ses, "discard");
        if cx.fs.is_dir(&discard) {
            match cx.fs.rmtree(&discard) {
                Ok(()) => cx.log(format!("  {ses}: removed discard/")),
                Err(e) => cx.fail(format!("discard removal failed for {ses}: {e}"))?,
            }
        }
        match demote_3d_bold(cx, &ses) {
            Ok(n) if n > 0 => cx.log(format!("  {ses}: demoted {n} 3D _bold → _sbref")),
            Ok(_) => {}
            Err(e) => cx.fail(format!("3D bold demote failed for {ses}: {e}"))?,
        }
        match resolve_part_entities(cx, &ses) {
            Ok(n) if n > 0 => cx.log(format!(
                "  {ses}: resolved {n} func part-mag/part-phase file(s)"
            )),
            Ok(_) => {}
            Err(e) => cx.fail(format!("part-entity resolution failed for {ses}: {e}"))?,
        }
        if let Err(e) = siemens_phase_units(cx.fs, &ses) {
            cx.fail(format!("phase Units failed for {ses}: {e}"))?;
        }
        match emit_aslcontext(cx.fs, &ses) {
            Ok(n) if n > 0 => cx.log(format!("  {ses}: wrote {n} _aslcontext.tsv file(s)")),
            Ok(_) => {}
            Err(e) => cx.fail(format!("aslcontext emission failed for {ses}: {e}"))?,
        }
        let pats = singlevol_dwi_patterns(cx.fs, &ses, &root);
        if !pats.is_empty() {
            cx.log(format!(
                "  {ses}: marked {} single-volume dwi artifact file(s) for .bidsignore",
                pats.len()
            ));
            by_root.entry(root).or_default().extend(pats);
        }
    }
    let mut roots: BTreeSet<String> = bids_roots(&walk_subjects(cx.fs, out_root))
        .into_iter()
        .collect();
    for u in cx.fs.rglob(out_root, "Unknown") {
        if cx.fs.is_dir(&u) && !cx.fs.has_part(&u, "derivatives") {
            roots.insert(parent(&u).to_string());
        }
    }
    let mut roots: Vec<String> = roots.into_iter().collect();
    vfs::sort_paths(&mut roots);
    for root in roots {
        let mut pats: BTreeSet<String> = collision_files(cx.fs, &root).into_iter().collect();
        pats.extend(by_root.get(&root).cloned().unwrap_or_default());
        pats.extend(unknown_leftover_files(cx.fs, &root));
        match write_bidsignore(cx.fs, &root, &pats.into_iter().collect::<Vec<_>>()) {
            Ok(n) if n > 0 => cx.log(format!("  {root}: added {n} entries to .bidsignore")),
            Ok(_) => {}
            Err(e) => cx.fail(format!(".bidsignore write failed for {root}: {e}"))?,
        }
    }
    Ok(())
}

/// `_pe_dir_label`.
fn pe_dir_label(pe: Option<&Value>) -> Option<String> {
    if !truthy(pe) {
        return None;
    }
    let s = py_str(pe);
    let s = s.trim();
    let c = s.chars().next()?;
    if !matches!(c.to_ascii_lowercase(), 'i' | 'j' | 'k') {
        return None;
    }
    Some(format!(
        "{}{}",
        c.to_ascii_uppercase(),
        if s.ends_with('-') { "n" } else { "" }
    ))
}

/// `_to_fmap_epi_stem`.
fn to_fmap_epi_stem(src: &str, dir_label: &str) -> String {
    let mut ents: HashMap<&str, &str> = HashMap::new();
    let mut extra = Vec::new();
    for tok in src.split('_') {
        let Some((k, v)) = tok.split_once('-') else {
            continue;
        };
        if k == "task" {
            continue;
        }
        if FMAP_EPI_ENTITY_ORDER.contains(&k) {
            ents.insert(k, v);
        } else {
            extra.push(tok);
        }
    }
    ents.entry("dir").or_insert(dir_label);
    let mut parts: Vec<String> = FMAP_EPI_ENTITY_ORDER
        .iter()
        .filter_map(|k| ents.get(k).map(|v| format!("{k}-{v}")))
        .collect();
    parts.extend(extra.into_iter().map(String::from));
    format!("{}_epi", parts.join("_"))
}

/// `_reclassify_session`.
fn reclassify_session(fs: &mut Vfs, ses: &str, min_volumes: i64) -> Res<usize> {
    if !fs.is_dir(ses) {
        return Ok(0);
    }
    struct S {
        dir: String,
        dt: &'static str,
        stem: String,
        suf: String,
        ndim: Option<i64>,
        shape: Option<[i64; 3]>,
        pe: Option<Value>,
    }
    let mut series = Vec::new();
    for dt in ["func", "dwi", "anat"] {
        let d = join(ses, dt);
        if !fs.is_dir(&d) {
            continue;
        }
        for nii in nii_files(fs, &d, "*.nii", "*.nii.gz") {
            let stem = nii_stem(name(&nii)).to_string();
            let jp = join(&d, &format!("{stem}.json"));
            let (mut shape, mut pe) = (None, None);
            if fs.is_file(&jp) {
                shape = imaging_volume(fs, &jp).map(|v| v.1);
                if let Some(m) = fs.try_obj(&jp)? {
                    pe = m.get("PhaseEncodingDirection").cloned();
                }
            }
            let suf = stem.rsplit('_').next().unwrap().to_string();
            series.push(S {
                dir: d.clone(),
                dt,
                stem,
                suf,
                ndim: nii_ndim(fs, &nii),
                shape,
                pe,
            });
        }
    }
    let long_epi = series.iter().any(|s| {
        matches!(s.dt, "func" | "dwi")
            && matches!(s.suf.as_str(), "bold" | "dwi")
            && s.ndim.unwrap_or(0) >= min_volumes
    });
    let func_xy: BTreeSet<[i64; 2]> = series
        .iter()
        .filter(|s| s.dt == "func" && s.suf == "bold")
        .filter_map(|s| s.shape.map(|sh| [sh[0], sh[1]]))
        .collect();
    let mut count = 0;
    for s in &series {
        if matches!(s.dt, "func" | "dwi")
            && matches!(s.suf.as_str(), "bold" | "dwi")
            && long_epi
            && s.ndim.is_some_and(|n| n < min_volumes)
            && !ECHO_TOKEN_RE.is_match(&s.stem)
        {
            let Some(label) = pe_dir_label(s.pe.as_ref()) else {
                continue;
            };
            let new_stem = to_fmap_epi_stem(&s.stem, &label);
            let dst = join(ses, &format!("fmap/{new_stem}"));
            match move_stem_files(fs, &join(&s.dir, &s.stem), &dst) {
                Err(Error::Exists(_)) => continue,
                r => r?,
            };
            for ext in [".bval", ".bvec"] {
                let stray = format!("{dst}{ext}");
                if fs.is_file(&stray) {
                    fs.unlink(&stray)?;
                }
            }
            let ev = join(
                &s.dir,
                &format!(
                    "{}_events.tsv",
                    &s.stem[..s.stem.len().saturating_sub(s.suf.len() + 1)]
                ),
            );
            if fs.is_file(&ev) {
                fs.unlink(&ev)?;
            }
            count += 1;
            continue;
        }
        if s.dt == "anat"
            && s.suf == "T2w"
            && s.shape.is_some_and(|sh| func_xy.contains(&[sh[0], sh[1]]))
        {
            let new_stem = format!("{}_inplaneT2", s.stem.strip_suffix("_T2w").unwrap_or(""));
            match move_stem_files(fs, &join(&s.dir, &s.stem), &join(&s.dir, &new_stem)) {
                Ok(_) => count += 1,
                Err(Error::Exists(_)) => {}
                Err(e) => return Err(e),
            }
        }
    }
    Ok(count)
}

/// `_strip_orphan_runs`.
fn strip_orphan_runs(fs: &mut Vfs, ses: &str) -> Res<usize> {
    if !fs.is_dir(ses) {
        return Ok(0);
    }
    let mut count = 0;
    for dt in fs.iterdir(ses) {
        if !fs.is_dir(&dt) || name(&dt).starts_with("ses-") {
            continue;
        }
        let mut groups: Vec<(String, Vec<String>)> = Vec::new();
        let mut niis = fs.glob(&dt, "*.nii.gz");
        niis.extend(fs.glob(&dt, "*.nii"));
        for nii in niis {
            let stem = nii_stem(name(&nii)).to_string();
            let base = RUN_ENTITY_RE.replace_all(&stem, "").into_owned();
            match groups.iter_mut().find(|(b, _)| *b == base) {
                Some((_, m)) => m.push(stem),
                None => groups.push((base, vec![stem])),
            }
        }
        for (base, members) in groups {
            if members.len() != 1 || !RUN_ENTITY_RE.is_match(&members[0]) {
                continue;
            }
            let stem = &members[0];
            let jp = join(&dt, &format!("{stem}.json"));
            let mut prot_run = false;
            if let Some(m) = fs.try_obj(&jp)? {
                prot_run = ["ProtocolName", "SeriesDescription"]
                    .iter()
                    .any(|f| RUN_PROTOCOL_RE.is_match(&py_str(m.get(*f))));
            }
            if prot_run {
                continue;
            }
            match move_stem_files(fs, &join(&dt, stem), &join(&dt, &base)) {
                Ok(_) => count += 1,
                Err(Error::Exists(_)) => {}
                Err(e) => return Err(e),
            }
        }
    }
    Ok(count)
}

/// `_bidsguess_demote_3d_bold`.
fn demote_3d_bold(cx: &mut Cx, ses: &str) -> Res<usize> {
    let func = join(ses, "func");
    if !cx.fs.is_dir(&func) {
        return Ok(0);
    }
    let mut renamed = 0;
    for nii in nii_files(cx.fs, &func, "*_bold.nii", "*_bold.nii.gz") {
        if nii_ndim(cx.fs, &nii).is_none_or(|n| n >= 2) {
            continue;
        }
        let n = name(&nii);
        let (stem, ext) = match n.strip_suffix(".nii.gz") {
            Some(s) => (s.to_string(), ".nii.gz"),
            None => (vfs::stem(n).to_string(), ".nii"),
        };
        let Some(base) = stem.strip_suffix("_bold") else {
            continue;
        };
        let new_stem = format!("{base}_sbref");
        if [".nii", ".nii.gz", ".json"]
            .iter()
            .any(|e| cx.fs.exists(&join(&func, &format!("{new_stem}{e}"))))
        {
            cx.log(format!(
                "reproinx: skipping 3D bold demote for {stem} — {new_stem} target already exists"
            ));
            continue;
        }
        let ev = join(&func, &format!("{stem}_events.tsv"));
        if cx.fs.is_file(&ev) {
            cx.fs.unlink(&ev)?;
        }
        for e in [ext, ".json"] {
            let src = join(&func, &format!("{stem}{e}"));
            if cx.fs.is_file(&src) {
                cx.fs
                    .replace(&src, &join(&func, &format!("{new_stem}{e}")))?;
            }
        }
        renamed += 1;
    }
    Ok(renamed)
}

fn guess_suffix(d: &Map<String, Value>) -> Option<&str> {
    match d.get("BidsGuess") {
        Some(Value::Array(g)) if g.len() == 2 => g[1].as_str(),
        _ => None,
    }
}

/// `_part_of` (with `_part_from_imagetype`).
fn part_of(d: &Map<String, Value>) -> Option<&'static str> {
    let label = |s: &str| {
        ["mag", "phase", "real", "imag"]
            .into_iter()
            .find(|p| *p == s)
    };
    if let Some(g) = guess_suffix(d) {
        let ms: Vec<&str> = PART_GUESS_RE
            .captures_iter(g)
            // Python `(?=_|$)`: `$` also matches before a final newline.
            .filter(|c| {
                matches!(&g[c.get(0).unwrap().end()..], "" | "\n")
                    || g[c.get(0).unwrap().end()..].starts_with('_')
            })
            .map(|c| c.get(1).unwrap().as_str())
            .collect();
        match ms.len() {
            1 => return label(ms[0]),
            n if n > 1 => return None,
            _ => {}
        }
    }
    let Some(Value::Array(it)) = d.get("ImageType") else {
        return None;
    };
    let toks: BTreeSet<String> = it.iter().map(|x| py_str(Some(x)).to_uppercase()).collect();
    let found: Vec<&str> = [
        ("PHASE", "phase"),
        ("MAGNITUDE", "mag"),
        ("REAL", "real"),
        ("IMAGINARY", "imag"),
    ]
    .into_iter()
    .filter(|(k, _)| toks.contains(*k))
    .map(|(_, p)| p)
    .collect();
    (found.len() == 1).then(|| found[0])
}

/// `_func_suffix`.
fn func_suffix(d: &Map<String, Value>, token: &str) -> Option<&'static str> {
    if let Some(g) = guess_suffix(d) {
        let last = g.rsplit('_').next().unwrap();
        if let Some(s) = PART_FUNC_SUFFIXES.into_iter().find(|s| *s == last) {
            return Some(s);
        }
    }
    PART_FUNC_SUFFIXES
        .into_iter()
        .find(|s| token.starts_with(s))
}

type Member = (String, &'static str, String); // stem, part, AcquisitionTime

/// `_part_runs`.
fn part_runs(
    prefix: &str,
    members: &[Member],
    bold_times: &[String],
) -> Option<HashMap<String, String>> {
    let parts: BTreeSet<&str> = members.iter().map(|m| m.1).collect();
    if bold_times.len() < 2 && parts.len() == members.len() {
        return Some(
            members
                .iter()
                .map(|m| (m.0.clone(), String::new()))
                .collect(),
        );
    }
    let times: Vec<&String> = members
        .iter()
        .map(|m| &m.2)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if times.iter().any(|t| t.is_empty()) || prefix.contains("_run-") {
        return None;
    }
    for t in &times {
        let at: Vec<&str> = members.iter().filter(|m| &m.2 == *t).map(|m| m.1).collect();
        if at.len() != at.iter().collect::<BTreeSet<_>>().len() {
            return None;
        }
    }
    if bold_times.len() < 2 {
        let idx = |t: &String| times.iter().position(|x| *x == t).unwrap();
        return Some(
            members
                .iter()
                .map(|m| (m.0.clone(), format!("run-{:02}", idx(&m.2) + 1)))
                .collect(),
        );
    }
    let refs: Vec<f64> = bold_times
        .iter()
        .map(|t| hms_str(t))
        .collect::<Option<_>>()?;
    let mut run: HashMap<&String, usize> = HashMap::new();
    for t in &times {
        let s = hms_str(t)?;
        let mut best = 0;
        for i in 1..refs.len() {
            if (refs[i] - s).abs() < (refs[best] - s).abs() {
                best = i;
            }
        }
        run.insert(*t, best);
    }
    if run.values().collect::<BTreeSet<_>>().len() != run.len() {
        return None;
    }
    Some(
        members
            .iter()
            .map(|m| (m.0.clone(), format!("run-{:02}", run[&m.2] + 1)))
            .collect(),
    )
}

/// `_resolve_part_entities`.
fn resolve_part_entities(cx: &mut Cx, ses: &str) -> Res<usize> {
    let func = join(ses, "func");
    if !cx.fs.is_dir(&func) {
        return Ok(0);
    }
    let mut groups: Vec<((String, &'static str), Vec<Member>)> = Vec::new();
    let mut unresolved: BTreeSet<String> = BTreeSet::new();
    for jp in cx.fs.glob(&func, "*.json") {
        let stem = vfs::stem(&jp).to_string();
        let (entities, token) = stem.rsplit_once('_').unwrap_or((&stem, ""));
        if !PART_FUNC_SUFFIXES.iter().any(|s| token.starts_with(s)) {
            continue;
        }
        let prefix = PART_SUFFIX_RE.replace(entities, "").into_owned();
        let data = cx.fs.load_json(&jp)?;
        let (part, suffix) = match &data {
            Value::Object(d) => (part_of(d), func_suffix(d, token)),
            _ => (None, None),
        };
        let (Some(part), Some(suffix)) = (part, suffix) else {
            unresolved.insert(prefix);
            continue;
        };
        let at = data.get("AcquisitionTime");
        let t = if truthy(at) {
            py_str(at)
        } else {
            String::new()
        };
        let key = (prefix, suffix);
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, m)) => m.push((stem.clone(), part, t)),
            None => groups.push((key, vec![(stem.clone(), part, t)])),
        }
    }
    let mut renamed = 0;
    for ((prefix, suffix), members) in &groups {
        if unresolved.contains(prefix) {
            cx.log(format!("reproinx: skipping part-entity resolution for {prefix} — unclassifiable sibling in family"));
            continue;
        }
        if members.iter().all(|m| m.1 == "mag") {
            continue;
        }
        let bold: Vec<String> = if *suffix == "bold" {
            Vec::new()
        } else {
            let b = groups
                .iter()
                .find(|(k, _)| k.0 == *prefix && k.1 == "bold")
                .map_or(&[][..], |g| &g.1[..]);
            b.iter()
                .map(|m| m.2.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect()
        };
        let run_of = part_runs(prefix, members, &bold);
        let mut plan = Vec::new();
        let mut conflict = run_of.is_none();
        if let Some(run_of) = &run_of {
            let mut dsts = BTreeSet::new();
            for (stem, part, _) in members {
                let run = &run_of[stem];
                let dst = if run.is_empty() {
                    format!("{prefix}_part-{part}_{suffix}")
                } else {
                    insert_run_entity(&format!("_{prefix}_part-{part}_{suffix}"), run)[1..]
                        .to_string()
                };
                if dst == *stem {
                    continue;
                }
                if dsts.contains(&dst)
                    || crate::tables::BIDS_EXTS
                        .iter()
                        .any(|e| cx.fs.exists(&join(&func, &format!("{dst}{e}"))))
                {
                    conflict = true;
                    break;
                }
                dsts.insert(dst.clone());
                plan.push((stem.clone(), dst));
            }
        }
        if conflict {
            cx.log(format!(
                "reproinx: skipping part-entity resolution for {prefix}_{suffix} — target collision"
            ));
            continue;
        }
        for (stem, dst) in plan {
            if move_stem_files(cx.fs, &join(&func, &stem), &join(&func, &dst))? > 0 {
                renamed += 1;
            }
        }
    }
    Ok(renamed)
}

/// `_siemens_phase_units`.
fn siemens_phase_units(fs: &mut Vfs, ses: &str) -> Res<usize> {
    let mut n = 0;
    for jp in fs.glob(ses, "*/*_part-phase_*.json") {
        let Ok(Value::Object(mut d)) = fs.load_json(&jp) else {
            continue;
        };
        if !d.contains_key("Units")
            && py_str(d.get("Manufacturer"))
                .to_lowercase()
                .starts_with("siemens")
        {
            d.insert("Units".into(), "arbitrary".into());
            fs.save_json(&jp, &Value::Object(d))?;
            n += 1;
        }
    }
    Ok(n)
}

/// `== 0` in Python (0, 0.0, False).
fn is_zero(v: &Value) -> bool {
    match v {
        Value::Number(n) => n.as_f64() == Some(0.0),
        Value::Bool(b) => !b,
        _ => false,
    }
}

/// `_emit_aslcontext`.
fn emit_aslcontext(fs: &mut Vfs, ses: &str) -> Res<usize> {
    let perf = join(ses, "perf");
    if !fs.is_dir(&perf) {
        return Ok(0);
    }
    let mut written = 0;
    for asl in fs.glob(&perf, "*_asl.json") {
        let Value::Object(mut d) = fs.load_json(&asl)? else {
            continue;
        };
        let Some(Value::Array(mut pld)) = d.get("PostLabelingDelay").cloned() else {
            continue;
        };
        if pld.is_empty() {
            continue;
        }
        let base = name(&asl).strip_suffix("_asl.json").unwrap().to_string();
        if let Some(Value::Array(drop)) = d.get("ReproinxDropVolumes")
            && !drop.is_empty()
        {
            let set: BTreeSet<i64> = drop
                .iter()
                .filter_map(|v| match v {
                    Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f.trunc() as i64)),
                    Value::Bool(b) => Some(*b as i64),
                    _ => None,
                })
                .collect();
            let n_drop = set
                .iter()
                .filter(|&&i| 0 <= i && (i as usize) < pld.len())
                .count();
            let nii = [".nii.gz", ".nii"]
                .iter()
                .map(|e| join(&perf, &format!("{base}_asl{e}")))
                .find(|p| fs.is_file(p));
            let cur = nii.as_ref().and_then(|p| nii_ndim(fs, p));
            let mut trimmed = false;
            if cur == Some(pld.len() as i64) {
                let idx: Vec<usize> = set
                    .iter()
                    .filter(|&&i| i >= 0)
                    .map(|&i| i as usize)
                    .collect();
                trimmed = fs.drop_volumes(nii.as_ref().unwrap(), idx);
            } else if cur == Some((pld.len() - n_drop) as i64) {
                trimmed = true;
            }
            if trimmed {
                pld = pld
                    .into_iter()
                    .enumerate()
                    .filter(|(i, _)| !set.contains(&(*i as i64)))
                    .map(|x| x.1)
                    .collect();
                d.insert("PostLabelingDelay".into(), Value::Array(pld.clone()));
                d.shift_remove("ReproinxDropVolumes");
                fs.save_json(&asl, &Value::Object(d))?;
            }
        }
        let ctx = join(&perf, &format!("{base}_aslcontext.tsv"));
        if fs.exists(&ctx) {
            continue;
        }
        let mut rows = Vec::new();
        let mut n_pair = 0;
        for v in &pld {
            if is_zero(v) {
                rows.push("m0scan");
            } else {
                rows.push(if n_pair % 2 == 0 { "label" } else { "control" });
                n_pair += 1;
            }
        }
        fs.write(
            &ctx,
            format!("volume_type\n{}\n", rows.join("\n")).into_bytes(),
        )?;
        written += 1;
    }
    Ok(written)
}

/// `_bidsguess_singlevol_dwi_patterns`.
fn singlevol_dwi_patterns(fs: &Vfs, ses: &str, root: &str) -> Vec<String> {
    let dwi = join(ses, "dwi");
    let mut out = Vec::new();
    if !fs.is_dir(&dwi) {
        return out;
    }
    for nii in nii_files(fs, &dwi, "*_dwi.nii", "*_dwi.nii.gz") {
        if nii_ndim(fs, &nii).is_none_or(|n| n >= 2) {
            continue;
        }
        let stem = nii_stem(name(&nii));
        for e in [".nii", ".nii.gz", ".json", ".bvec", ".bval"] {
            let c = join(&dwi, &format!("{stem}{e}"));
            if fs.is_file(&c)
                && let Some(r) = vfs::relative(&c, root)
            {
                out.push(r.to_string());
            }
        }
    }
    out
}

/// `_bidsguess_collision_files`.
fn collision_files(fs: &Vfs, root: &str) -> Vec<String> {
    fs.rglob(root, "sub-*")
        .into_iter()
        .filter(|p| fs.is_file(p) && COLLISION_RE.is_match(name(p)))
        .filter_map(|p| vfs::relative(&p, root).map(String::from))
        .collect()
}

/// `_bidsguess_unknown_leftover_files`.
fn unknown_leftover_files(fs: &Vfs, root: &str) -> Vec<String> {
    let u = join(root, "Unknown");
    if !fs.is_dir(&u) {
        return Vec::new();
    }
    fs.rglob(&u, "*")
        .into_iter()
        .filter(|p| fs.is_file(p))
        .filter_map(|p| vfs::relative(&p, root).map(String::from))
        .collect()
}

/// `_bidsguess_write_bidsignore`.
fn write_bidsignore(fs: &mut Vfs, root: &str, patterns: &[String]) -> Res<usize> {
    if patterns.is_empty() {
        return Ok(0);
    }
    let p = join(root, ".bidsignore");
    let mut lines: Vec<String> = if fs.is_file(&p) {
        py_lines(&fs.read_text(&p)?)
            .into_iter()
            .map(String::from)
            .collect()
    } else {
        Vec::new()
    };
    let mut seen: BTreeSet<String> = lines
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let added: Vec<&String> = patterns
        .iter()
        .filter(|x| seen.insert((*x).clone()))
        .collect();
    if added.is_empty() {
        return Ok(0);
    }
    if lines.last().is_some_and(|l| !l.trim().is_empty()) {
        lines.push(String::new());
    }
    lines.push("# reproinx: files dcm2niix could not place cleanly in BIDS".into());
    lines.extend(added.iter().map(|s| s.to_string()));
    fs.write(&p, format!("{}\n", lines.join("\n")).into_bytes())?;
    Ok(added.len())
}
