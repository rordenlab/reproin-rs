//! Session backfill, physio rescue from derivatives/scanner/, derivatives drop.

use std::collections::{BTreeMap, BTreeSet};

use super::*;
use crate::tables::is_datatype;

re!(
    PHYSIO_INFIX_RE,
    r"_(?:bold|sbref|dwi|asl|epi|m0scan|cbv|T1w|T2w|T2starw|FLAIR|PDw|angio)(_recording-)"
);

/// `_session_from_provenance`.
fn session_from_provenance(rows: &[&Row]) -> Option<String> {
    let mut s = BTreeSet::new();
    for r in rows {
        for f in ["ProtocolName", "SeriesDescription"] {
            if let Some(m) = SES_RE.captures(r.get(f)) {
                s.insert(m[1].to_string());
            }
        }
    }
    (s.len() == 1).then(|| s.into_iter().next().unwrap())
}

/// `_propagate_session`.
pub fn propagate_session(cx: &mut Cx, sub: &str) -> Res<Option<String>> {
    let fs = &mut *cx.fs;
    let root = parent(sub).to_string();
    let sub_name = name(sub).to_string();
    let mut sessions = BTreeSet::new();
    let rows = load_provenance(fs, &root);
    if !rows.is_empty() {
        let tok = format!("/{sub_name}/");
        let own: Vec<&Row> = rows
            .iter()
            .filter(|r| format!("/{}", r.get("OutputStem")).contains(&tok))
            .collect();
        let uids: BTreeSet<&str> = own
            .iter()
            .filter(|r| stem_has_files(fs, &join(&root, r.get("OutputStem"))))
            .map(|r| r.get("StudyInstanceUID"))
            .filter(|u| !u.is_empty())
            .collect();
        if !uids.is_empty() {
            let scoped: Vec<&Row> = own
                .into_iter()
                .filter(|r| uids.contains(r.get("StudyInstanceUID")))
                .collect();
            if let Some(s) = session_from_provenance(&scoped) {
                sessions.insert(s);
            }
        }
    }
    if sessions.is_empty() {
        for d in fs.iterdir(sub) {
            // Divergence D9: only BIDS datatype dirs, never curated ones
            // (code/, stimuli/) whose filenames may mention a session.
            if !fs.is_dir(&d) || name(&d).starts_with("ses-") || !is_datatype(name(&d)) {
                continue;
            }
            for f in fs.iterdir(&d) {
                if fs.is_file(&f)
                    && let Some(m) = SES_RE.captures(name(&f))
                {
                    sessions.insert(m[1].to_string());
                }
            }
        }
        let deriv = join(&root, &format!("derivatives/scanner/{sub_name}"));
        if fs.is_dir(&deriv) {
            for p in fs.rglob(&deriv, "*") {
                if !fs.is_file(&p) {
                    continue;
                }
                for part in fs.parts(&p) {
                    if let Some(m) = SES_RE.captures(part) {
                        sessions.insert(m[1].to_string());
                    }
                }
            }
        }
    }
    if sessions.is_empty() {
        let children: Vec<String> = fs
            .iterdir(sub)
            .into_iter()
            .filter(|d| fs.is_dir(d))
            .collect();
        let ses: Vec<&String> = children
            .iter()
            .filter(|d| name(d).starts_with("ses-"))
            .collect();
        if ses.len() == 1 && children.iter().any(|d| is_datatype(name(d))) {
            sessions.insert(name(ses[0])["ses-".len()..].to_string());
        }
    }
    if sessions.len() != 1 {
        return Ok(None);
    }
    let ses = sessions.into_iter().next().unwrap();
    let target_root = join(sub, &format!("ses-{ses}"));
    for dt in fs.iterdir(sub) {
        if !fs.is_dir(&dt) || name(&dt).starts_with("ses-") || !is_datatype(name(&dt)) {
            continue;
        }
        let new_dt = join(&target_root, name(&dt));
        let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut order = Vec::new();
        for f in fs.iterdir(&dt) {
            if !fs.is_file(&f) {
                continue;
            }
            let st = series_stem(name(&f)).to_string();
            if !groups.contains_key(&st) {
                order.push(st.clone());
            }
            groups.entry(st).or_default().push(f);
        }
        fs.mkdirs(&new_dt)?;
        for st in order {
            let mut planned = Vec::new();
            let mut collision = false;
            for f in &groups[&st] {
                let target = join(&new_dt, &inject_session_into_name(name(f), &sub_name, &ses));
                if fs.exists(&target) {
                    collision = true;
                    break;
                }
                planned.push((f.clone(), target));
            }
            if collision {
                cx.report.log.push(format!(
                    "reproinx: backfill skipped series '{st}' (target exists in {new_dt})"
                ));
                continue;
            }
            for (s, d) in planned {
                fs.replace(&s, &d)?;
            }
        }
        let _ = fs.rmdir(&dt);
    }
    Ok(Some(ses))
}

/// `_rescue_physio_recordings` -> (rescued, skipped).
pub fn rescue_physio_recordings(cx: &mut Cx, root: &str) -> Res<(usize, usize)> {
    let deriv = join(root, "derivatives/scanner");
    if !cx.fs.is_dir(&deriv) {
        return Ok((0, 0));
    }
    let (mut rescued, mut skipped) = (0, 0);
    for sub_dir in cx.fs.iterdir(&deriv) {
        if !cx.fs.is_dir(&sub_dir) || !name(&sub_dir).starts_with("sub-") {
            continue;
        }
        let sub = name(&sub_dir).to_string();
        let main = join(root, &sub);
        let mut ses_token = None;
        let mut multi = false;
        if cx.fs.is_dir(&main) {
            let ses: Vec<String> = cx
                .fs
                .iterdir(&main)
                .into_iter()
                .filter(|d| cx.fs.is_dir(d) && name(d).starts_with("ses-"))
                .collect();
            if ses.len() == 1 {
                ses_token = Some(name(&ses[0]).split_once('-').unwrap().1.to_string());
            } else if ses.len() > 1 {
                multi = true;
            }
        }
        for func in cx.fs.rglob(&sub_dir, "func") {
            if !cx.fs.is_dir(&func) {
                continue;
            }
            let mut stems = BTreeSet::new();
            for p in cx.fs.glob(&func, "*_recording-*_physio.tsv.gz") {
                stems.insert(name(&p).strip_suffix(".tsv.gz").unwrap().to_string());
            }
            for p in cx.fs.glob(&func, "*_recording-*_physio.json") {
                stems.insert(name(&p).strip_suffix(".json").unwrap().to_string());
            }
            if stems.is_empty() {
                continue;
            }
            if multi {
                skipped += stems.len();
                cx.log(format!(
                    "reproinx: physio rescue skipped for {sub} ({} recording(s); multi-session subjects not yet supported)",
                    stems.len()
                ));
                if cx.opts.strict {
                    return err(format!(
                        "strict: physio rescue skipped for multi-session {sub}"
                    ));
                }
                continue;
            }
            for stem in stems {
                let mut new_name = PHYSIO_INFIX_RE.replace_all(&stem, "${1}").into_owned();
                let dest = match &ses_token {
                    Some(s) => {
                        new_name = inject_session_into_name(&new_name, &sub, s);
                        join(root, &format!("{sub}/ses-{s}/func"))
                    }
                    None => join(root, &format!("{sub}/func")),
                };
                match move_stem_files(cx.fs, &join(&func, &stem), &join(&dest, &new_name)) {
                    Ok(n) if n > 0 => rescued += 1,
                    Ok(_) => {}
                    Err(e) => {
                        skipped += 1;
                        let exists = matches!(e, Error::Exists(_));
                        cx.log(if exists {
                            format!("reproinx: physio rescue collision — kept {func}/{stem} (destination in {dest} already present)")
                        } else {
                            format!("reproinx: physio rescue failed for {func}/{stem}: {e}")
                        });
                        if cx.opts.strict {
                            return if exists {
                                err(format!("strict: physio rescue collision for {stem}"))
                            } else {
                                Err(e)
                            };
                        }
                    }
                }
            }
        }
    }
    Ok((rescued, skipped))
}

/// `_drop_derivatives`.
pub fn drop_derivatives(fs: &mut Vfs, out_root: &str, skip: &BTreeSet<String>) -> Res<usize> {
    let mut removed = 0;
    for root in bids_roots(&walk_subjects(fs, out_root)) {
        if skip.contains(&root) {
            continue;
        }
        let scanner = join(&root, "derivatives/scanner");
        if fs.is_dir(&scanner) {
            fs.rmtree(&scanner)?;
            removed += 1;
            let _ = fs.rmdir(&join(&root, "derivatives"));
        }
    }
    Ok(removed)
}
