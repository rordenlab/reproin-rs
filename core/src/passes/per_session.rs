//! Per-session pass 2: sbref gradient strip, scans.tsv, events stubs, B0 fields.

use std::collections::BTreeSet;

use md5::{Digest, Md5};
use serde_json::{Map, Value};

use super::*;
use crate::tables::MATCHING_PARAMETERS;

re!(
    FMAP_SUFFIX_RE,
    r"(_dir-[A-Za-z0-9]+)?(_phase[12])?(_phasediff)?(_magnitude[12])?(_fieldmap)?"
);
re!(SUB_PREFIX_RE, r"^sub-[A-Za-z0-9]+_?");
re!(SES_PREFIX_RE, r"^ses-[A-Za-z0-9]+_?");
re!(PART_RE, r"_part-[A-Za-z0-9]+");

/// `_strip_sbref_gradients`.
pub fn strip_sbref_gradients(fs: &mut Vfs, ses: &str) -> Res<usize> {
    let mut n = 0;
    for nii in fs.rglob(ses, "*_sbref.nii*") {
        let stem = nii_stem(name(&nii)).to_string();
        for ext in [".bval", ".bvec"] {
            let g = join(parent(&nii), &format!("{stem}{ext}"));
            if fs.is_file(&g) {
                fs.unlink(&g)?;
                n += 1;
            }
        }
    }
    Ok(n)
}

/// `_load_bidsignore_patterns`.
fn bidsignore_patterns(fs: &Vfs, root: &str) -> Res<Vec<String>> {
    let p = join(root, ".bidsignore");
    if !fs.is_file(&p) {
        return Ok(Vec::new());
    }
    Ok(py_lines(&fs.read_text(&p)?)
        .into_iter()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
        .map(String::from)
        .collect())
}

/// `_series_randstr`.
fn series_randstr(d: &Map<String, Value>) -> String {
    let mut pieces: Vec<String> = ["SeriesInstanceUID", "StudyInstanceUID"]
        .iter()
        .filter(|k| truthy(d.get(**k)))
        .map(|k| py_str(d.get(*k)))
        .collect();
    if pieces.is_empty() {
        return "n/a".into();
    }
    pieces.sort();
    let h = Md5::digest(pieces.concat().as_bytes());
    h.iter().take(4).map(|b| format!("{b:02x}")).collect()
}

/// `_write_scans_tsv`.
pub fn write_scans_tsv(fs: &mut Vfs, ses: &str) -> Res<()> {
    let root = bids_root_for_session(fs, ses);
    let ignore = bidsignore_patterns(fs, &root)?;
    let mut rows: Vec<(String, String, String, bool)> = Vec::new();
    for jp in fs.glob(ses, "*/*.json") {
        let Some(nii) = nii_for_json(fs, &jp) else {
            continue;
        };
        if !ignore.is_empty() {
            let rel = vfs::relative(&nii, &root).unwrap_or(name(&nii));
            let base = name(rel);
            if ignore
                .iter()
                .any(|p| vfs::fnmatch(rel, p) || vfs::fnmatch(base, p))
            {
                continue;
            }
        }
        let Some(d) = fs.try_obj(&jp)? else {
            continue;
        };
        let (acq, is_str) = acq_iso(&d)?.unwrap_or_else(|| ("n/a".into(), true));
        rows.push((
            vfs::relative(&nii, ses).unwrap().to_string(),
            acq,
            series_randstr(&d),
            is_str,
        ));
    }
    if rows.is_empty() {
        return Ok(());
    }
    // Python sorts the raw values: a number beside a string is a TypeError;
    // numbers alone sort numerically.
    let dated: Vec<&(String, String, String, bool)> =
        rows.iter().filter(|r| r.1 != "n/a").collect();
    if dated.iter().any(|r| r.3) && dated.iter().any(|r| !r.3) {
        return err("'<' not supported between instances of 'str' and a number");
    }
    let key = |r: &(String, String, String, bool)| r.1.parse::<f64>().ok().filter(|_| !r.3);
    rows.sort_by(|a, b| {
        (a.1 == "n/a")
            .cmp(&(b.1 == "n/a"))
            .then_with(|| match (key(a), key(b)) {
                (Some(x), Some(y)) => x.total_cmp(&y),
                _ => a.1.cmp(&b.1),
            })
    });
    let rows: Vec<(String, String, String)> =
        rows.into_iter().map(|(f, a, r, _)| (f, a, r)).collect();
    let ses_name = fs.name_of(ses).to_string();
    let file = if ses_name.starts_with("ses-") {
        format!("{}_{ses_name}_scans.tsv", fs.name_of(parent(ses)))
    } else {
        format!("{ses_name}_scans.tsv")
    };
    let mut out = String::from("filename\tacq_time\toperator\trandstr\r\n");
    for (f, a, r) in rows {
        out.push_str(&format!("{f}\t{a}\tn/a\t{r}\r\n"));
    }
    fs.write(&join(ses, &file), out.into_bytes())
}

/// `_emit_events_tsv`.
pub fn emit_events_tsv(fs: &mut Vfs, ses: &str) -> Res<()> {
    let func = join(ses, "func");
    if !fs.is_dir(&func) {
        return Ok(());
    }
    let mut seen = BTreeSet::new();
    for bold in fs.glob(&func, "*_bold.nii*") {
        let stem = name(&bold).split("_bold").next().unwrap();
        let stem = ECHO_TOKEN_RE.replace_all(stem, "");
        let stem = PART_RE.replace_all(&stem, "").into_owned();
        if !seen.insert(stem.clone()) {
            continue;
        }
        let ev = join(&func, &format!("{stem}_events.tsv"));
        if !fs.exists(&ev) {
            fs.write(&ev, b"onset\tduration\ttrial_type\n".to_vec())?;
        }
    }
    Ok(())
}

fn b0_identifier(prefix: &str) -> String {
    let s = SUB_PREFIX_RE.replace(prefix, "");
    let s = SES_PREFIX_RE.replace(&s, "").into_owned();
    if s.is_empty() { prefix.to_string() } else { s }
}

fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::Bool(b) => Some(*b as u8 as f64),
        _ => None,
    }
}

/// `_allclose` (rtol 5%, atol 1e-8, symmetric).
fn allclose(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| allclose(p, q))
        }
        _ => match (num(a), num(b)) {
            (Some(x), Some(y)) => {
                !x.is_nan() && !y.is_nan() && (x - y).abs() <= 1e-8 + 0.05 * x.abs().max(y.abs())
            }
            _ => a == b,
        },
    }
}

/// `_params_match`.
fn params_match(a: &Option<Vec<Value>>, b: &Option<Vec<Value>>) -> bool {
    let (Some(a), Some(b)) = (a, b) else {
        return false;
    };
    if a.len() != b.len() {
        return false;
    }
    if a.iter().all(Value::is_string) && b.iter().all(Value::is_string) {
        return a == b;
    }
    a.iter().zip(b).all(|(x, y)| allclose(x, y))
}

/// `_key_info`.
fn key_info(fs: &Vfs, jp: &str, param: &str) -> Res<Option<Vec<Value>>> {
    let d = fs.load_obj(jp)?;
    Ok(match param {
        "Shims" => d
            .get("ShimSetting")
            .filter(|v| !v.is_null())
            .map(|v| vec![v.clone()]),
        _ => imaging_volume(fs, jp).map(|(aff, shape)| {
            let aff: Vec<Value> = aff
                .iter()
                .map(|r| r.iter().map(|&x| Value::from(x)).collect())
                .collect();
            vec![
                Value::Array(aff),
                shape.iter().map(|&x| Value::from(x)).collect(),
            ]
        }),
    })
}

fn acq_secs(fs: &Vfs, jp: &str) -> Res<Option<f64>> {
    Ok(hms_seconds(fs.load_obj(jp)?.get("AcquisitionTime")))
}

/// `_populate_b0_fields`.
pub fn populate_b0_fields(cx: &mut Cx, ses: &str) -> Res<usize> {
    let fmap = join(ses, "fmap");
    if !cx.fs.is_dir(&fmap) {
        return Ok(0);
    }
    let jsons = cx.fs.glob(&fmap, "*.json");
    let strip = |j: &str| FMAP_SUFFIX_RE.replace_all(vfs::stem(j), "").into_owned();
    let prefixes: BTreeSet<String> = jsons.iter().map(|j| strip(j)).collect();
    let mut groups: Vec<(String, Vec<String>)> = prefixes
        .into_iter()
        .map(|p| {
            let m = jsons.iter().filter(|j| strip(j) == p).cloned().collect();
            (p, m)
        })
        .collect();

    // Same-polarity `_epi` groups cannot estimate a field: drop, unlink stale links.
    let mut stale: Vec<Value> = Vec::new();
    let mut i = 0;
    while i < groups.len() {
        if !same_polarity_epi_group(cx.fs, &groups[i].1)? {
            i += 1;
            continue;
        }
        let (prefix, members) = groups.remove(i);
        cx.log(format!(
            "reproinx: {prefix}: all _epi share one PhaseEncodingDirection; not usable as a fieldmap, left unlinked"
        ));
        stale.push(Value::from(b0_identifier(&prefix)));
        for fm in members {
            let mut d = cx.fs.load_obj(&fm)?;
            if let Some(old) = d.shift_remove("B0FieldIdentifier") {
                if !old.is_null() {
                    match old {
                        Value::Array(a) => stale.extend(a),
                        v => stale.push(v),
                    }
                }
                cx.fs.save_json(&fm, &Value::Object(d))?;
            }
        }
    }
    let live: Vec<Value> = groups
        .iter()
        .map(|(p, _)| Value::from(b0_identifier(p)))
        .collect();
    stale.retain(|v| !live.contains(v));
    if !stale.is_empty() {
        for jp in cx.fs.glob(ses, "*/*.json") {
            if name(parent(&jp)) == "fmap" {
                continue;
            }
            let mut d = cx.fs.load_obj(&jp)?;
            match d.get("B0FieldSource").cloned() {
                Some(Value::Array(src)) if src.iter().any(|v| stale.contains(v)) => {
                    let keep: Vec<Value> = src.into_iter().filter(|v| !stale.contains(v)).collect();
                    if keep.is_empty() {
                        d.shift_remove("B0FieldSource");
                    } else {
                        d.insert("B0FieldSource".into(), Value::Array(keep));
                    }
                }
                Some(s @ Value::String(_)) if stale.contains(&s) => {
                    d.shift_remove("B0FieldSource");
                }
                _ => continue,
            }
            cx.fs.save_json(&jp, &Value::Object(d))?;
        }
    }
    if groups.is_empty() {
        return Ok(0);
    }
    let targets: Vec<String> = cx
        .fs
        .glob(ses, "*/*.json")
        .into_iter()
        .filter(|p| name(parent(p)) != "fmap" && !vfs::stem(p).ends_with("_noRF"))
        .collect();
    if targets.is_empty() {
        return Ok(0);
    }
    let mut selected: Vec<(String, Option<usize>)> = Vec::new();
    for jp in &targets {
        let info: Vec<Option<Vec<Value>>> = MATCHING_PARAMETERS
            .iter()
            .map(|p| key_info(cx.fs, jp, p))
            .collect::<Res<_>>()?;
        let mut compat = Vec::new();
        for (gi, (_, g)) in groups.iter().enumerate() {
            let mut ok = true;
            for (pi, p) in MATCHING_PARAMETERS.iter().enumerate() {
                if !params_match(&info[pi], &key_info(cx.fs, &g[0], p)?) {
                    ok = false;
                    break;
                }
            }
            if ok {
                compat.push(gi);
            }
        }
        let sel = match compat.len() {
            0 => None,
            1 => Some(compat[0]),
            _ => match acq_secs(cx.fs, jp)? {
                None => Some(compat[0]),
                Some(t) => {
                    let mut best = (None, f64::INFINITY);
                    for &gi in &compat {
                        let times: Vec<f64> = groups[gi]
                            .1
                            .iter()
                            .map(|p| acq_secs(cx.fs, p))
                            .collect::<Res<Vec<_>>>()?
                            .into_iter()
                            .flatten()
                            .collect();
                        if times.is_empty() {
                            continue;
                        }
                        let d = (times.iter().sum::<f64>() / times.len() as f64 - t).abs();
                        if d < best.1 {
                            best = (Some(gi), d);
                        }
                    }
                    best.0
                }
            },
        };
        selected.push((jp.clone(), sel));
    }
    // An sbref shares its bold sibling's readout: force one shared selection.
    for i in 0..selected.len() {
        let jp = selected[i].0.clone();
        let Some(base) = jp.strip_suffix("_sbref.json") else {
            continue;
        };
        let sib = format!("{base}_bold.json");
        let Some(si) = selected.iter().position(|(p, _)| *p == sib) else {
            continue;
        };
        let shared = if acq_secs(cx.fs, &sib)?.is_some() {
            selected[si].1
        } else if acq_secs(cx.fs, &jp)?.is_some() {
            selected[i].1
        } else {
            selected[si].1.or(selected[i].1)
        };
        selected[i].1 = shared;
        selected[si].1 = shared;
    }
    let used: BTreeSet<usize> = selected.iter().filter_map(|s| s.1).collect();
    for &gi in &used {
        let id = b0_identifier(&groups[gi].0);
        for fm in groups[gi].1.clone() {
            let mut d = cx.fs.load_obj(&fm)?;
            d.insert("B0FieldIdentifier".into(), Value::from(id.clone()));
            cx.fs.save_json(&fm, &Value::Object(d))?;
        }
    }
    for (jp, sel) in &selected {
        let Some(gi) = sel else { continue };
        let mut d = cx.fs.load_obj(jp)?;
        d.insert(
            "B0FieldSource".into(),
            Value::from(b0_identifier(&groups[*gi].0)),
        );
        cx.fs.save_json(jp, &Value::Object(d))?;
    }
    Ok(used.len())
}

/// `_is_same_polarity_epi_group`.
fn same_polarity_epi_group(fs: &Vfs, group: &[String]) -> Res<bool> {
    let epis: Vec<&String> = group
        .iter()
        .filter(|p| vfs::stem(p).ends_with("_epi"))
        .collect();
    if epis.len() < 2 {
        return Ok(false);
    }
    let mut dirs: Vec<Value> = Vec::new();
    for p in epis {
        let Ok(Value::Object(d)) = fs.load_json(p) else {
            return Ok(false);
        };
        let v = d
            .get("PhaseEncodingDirection")
            .cloned()
            .unwrap_or(Value::Null);
        if v.is_array() || v.is_object() {
            return err(format!("{p}: unhashable PhaseEncodingDirection"));
        }
        if !dirs.contains(&v) {
            dirs.push(v);
        }
    }
    Ok(dirs.len() == 1 && dirs[0].is_string())
}
