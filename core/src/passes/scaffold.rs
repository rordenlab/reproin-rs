//! Pass 3: BIDS root scaffolding, participants, task sidecars, TaskName backfill.

use std::collections::{BTreeSet, HashMap};

use serde_json::{Map, Value};

use super::*;
use crate::tables::{
    CHANGES_TEMPLATE, DATASET_DESCRIPTION, DCM2NIIX_DDESC_NAME, PARTICIPANTS_JSON, README_TEMPLATE,
    SCANS_JSON,
};

re!(DICOM_AGE_RE, r"^0*([0-9]+)Y$");
re!(SUB_IN_STEM_RE, r"(^|/)(sub-[^/]+)/");
re!(TASK_STEM_RE, r"(task-[A-Za-z0-9]+(?:_acq-[A-Za-z0-9]+)?)_");
re!(
    TASK_STUB_NAME_RE,
    r"^task-[A-Za-z0-9]+(_acq-[A-Za-z0-9]+)?_bold\.json$"
);

fn template(s: &str) -> Map<String, Value> {
    serde_json::from_str(s).unwrap()
}

pub fn scaffold_roots(cx: &mut Cx, roots: &[String]) -> Res<()> {
    for root in roots {
        write_root_scaffolding(cx.fs, root)?;
        write_participants(cx, root)?;
        emit_task_bold_jsons(cx, root)?;
        let n = backfill_tasknames(cx.fs, root, "*_physio.json")?;
        if n > 0 {
            cx.log(format!(
                "  {root}: backfilled TaskName into {n} physio sidecar(s)"
            ));
        }
        let n = backfill_tasknames(cx.fs, root, "*_sbref.json")?;
        if n > 0 {
            cx.log(format!(
                "  {root}: backfilled TaskName into {n} sbref sidecar(s)"
            ));
        }
    }
    Ok(())
}

fn write_if_absent(fs: &mut Vfs, p: &str, s: &str) -> Res<()> {
    if fs.exists(p) {
        Ok(())
    } else {
        fs.write(p, s.as_bytes().to_vec())
    }
}

fn write_root_scaffolding(fs: &mut Vfs, root: &str) -> Res<()> {
    write_if_absent(fs, &join(root, "CHANGES"), CHANGES_TEMPLATE)?;
    write_if_absent(fs, &join(root, "README"), README_TEMPLATE)?;
    upgrade_dataset_description(fs, root)?;
    let sj = join(root, "scans.json");
    if !fs.exists(&sj) {
        fs.save_json(&sj, &Value::Object(template(SCANS_JSON)))?;
    }
    Ok(())
}

/// `_discover_dcm2niix_version`.
fn discover_dcm2niix_version(fs: &Vfs, root: &str) -> Option<String> {
    let skip = [
        "dataset_description.json",
        "participants.json",
        "scans.json",
    ];
    for jp in fs.rglob(root, "*.json") {
        if skip.contains(&name(&jp)) || fs.has_part(&jp, "derivatives") {
            continue;
        }
        let Ok(Value::Object(d)) = fs.load_json(&jp) else {
            continue;
        };
        if let Some(Value::String(sw)) = d.get("ConversionSoftware")
            && !sw.is_empty()
            && sw != "dcm2niix"
        {
            continue;
        }
        if let Some(Value::String(v)) = d.get("ConversionSoftwareVersion")
            && !v.trim().is_empty()
        {
            return Some(v.trim().to_string());
        }
    }
    None
}

fn generated_by(fs: &Vfs, root: &str) -> Value {
    let mut e = Map::new();
    e.insert("Name".into(), "dcm2niix".into());
    e.insert("Description".into(), "DICOM to NIfTI converter".into());
    e.insert(
        "CodeURL".into(),
        "https://github.com/rordenlab/dcm2niix".into(),
    );
    if let Some(v) = discover_dcm2niix_version(fs, root) {
        e.insert("Version".into(), v.into());
    }
    Value::Array(vec![Value::Object(e)])
}

/// `_upgrade_dataset_description`.
fn upgrade_dataset_description(fs: &mut Vfs, root: &str) -> Res<()> {
    let p = join(root, "dataset_description.json");
    let fresh = |fs: &Vfs| {
        let mut d = template(DATASET_DESCRIPTION);
        d.insert("DatasetType".into(), "raw".into());
        d.insert("GeneratedBy".into(), generated_by(fs, root));
        d.insert("SourceDatasets".into(), Value::Array(vec![]));
        Value::Object(d)
    };
    if !fs.exists(&p) {
        let d = fresh(fs);
        return fs.save_json(&p, &d);
    }
    let Ok(Value::Object(mut d)) = fs.load_json(&p) else {
        return Ok(());
    };
    if d.get("Name").and_then(Value::as_str) == Some(DCM2NIIX_DDESC_NAME) {
        let d = fresh(fs);
        return fs.save_json(&p, &d);
    }
    let mut changed = false;
    if !d.contains_key("DatasetType") {
        d.insert("DatasetType".into(), "raw".into());
        changed = true;
    }
    if !d.contains_key("GeneratedBy") {
        d.insert("GeneratedBy".into(), generated_by(fs, root));
        changed = true;
    }
    if !d.contains_key("SourceDatasets") {
        d.insert("SourceDatasets".into(), Value::Array(vec![]));
        changed = true;
    }
    if changed {
        fs.save_json(&p, &Value::Object(d))?;
    }
    Ok(())
}

struct Demo {
    age: String,
    sex: String,
    sdt: String,
}

/// `_demographics_from_provenance`.
fn demographics(fs: &Vfs, root: &str) -> HashMap<String, Demo> {
    let mut out: HashMap<String, Demo> = HashMap::new();
    for r in load_provenance(fs, root) {
        let Some(m) = SUB_IN_STEM_RE.captures(r.get("OutputStem")) else {
            continue;
        };
        let age = DICOM_AGE_RE
            .captures(r.get("PatientAge").trim())
            .map_or("n/a".into(), |m| m[1].to_string());
        let sex_raw = r.get("PatientSex").trim();
        let sex = if ["M", "F", "O"].contains(&sex_raw) {
            sex_raw.to_string()
        } else {
            "n/a".into()
        };
        let (sd, st) = (r.get("StudyDate").trim(), r.get("StudyTime").trim());
        let sdt = format!("{sd}{st}");
        let cur = out.entry(m[2].to_string()).or_insert(Demo {
            age: "n/a".into(),
            sex: "n/a".into(),
            sdt: String::new(),
        });
        if cur.age == "n/a" && age != "n/a" {
            cur.age = age;
        }
        if cur.sex == "n/a" && sex != "n/a" {
            cur.sex = sex;
        }
        if cur.sdt.is_empty() && !sdt.is_empty() {
            cur.sdt = sdt;
        }
    }
    out
}

/// `_write_participants`.
fn write_participants(cx: &mut Cx, root: &str) -> Res<()> {
    let fs = &mut *cx.fs;
    let subs: BTreeSet<String> = fs
        .iterdir(root)
        .into_iter()
        .filter(|s| fs.is_dir(s) && name(s).starts_with("sub-") && !name(s).starts_with("sub-."))
        .map(|s| name(&s).to_string())
        .collect();
    if subs.is_empty() {
        return Ok(());
    }
    let demos = demographics(fs, root);
    let sdt = |s: &str| demos.get(s).map_or("", |d| d.sdt.as_str()).to_string();
    let mut subs: Vec<String> = subs.into_iter().collect();
    subs.sort_by_key(|s| (sdt(s).is_empty(), sdt(s), s.clone()));
    let header = "participant_id\tage\tsex\tgroup\n";
    let age_sex = |s: &str| {
        demos
            .get(s)
            .map_or(("n/a", "n/a"), |d| (d.age.as_str(), d.sex.as_str()))
    };
    let row = |s: &str| {
        let (a, x) = age_sex(s);
        format!("{s}\t{a}\t{x}\tcontrol\n")
    };
    let tsv = join(root, "participants.tsv");
    if fs.exists(&tsv) {
        let text = std::str::from_utf8(fs.read(&tsv)?)
            .map_err(|e| Error::Other(format!("{tsv}: {e}")))?
            .to_string();
        let mut content = text.clone();
        let mut existing: Vec<String> = py_split(&text, true)
            .into_iter()
            .map(String::from)
            .collect();
        if existing.is_empty() {
            content = format!(
                "{header}{}",
                subs.iter().map(|s| row(s)).collect::<String>()
            );
            existing = std::iter::once(header.to_string())
                .chain(subs.iter().map(|s| row(s)))
                .collect();
        }
        let seen: BTreeSet<&str> = existing[1..]
            .iter()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.split('\t').next().unwrap())
            .collect();
        let new: Vec<&String> = subs.iter().filter(|s| !seen.contains(s.as_str())).collect();
        if existing[0] != header {
            cx.report.log.push(format!(
                "reproinx: {tsv} has a non-default header — preserving existing columns and appending only new participant_ids"
            ));
            let cols: Vec<&str> = existing[0]
                .trim_end_matches(['\r', '\n'])
                .split('\t')
                .collect();
            if !new.is_empty() {
                let le = if existing[0].ends_with("\r\n") {
                    "\r\n"
                } else {
                    "\n"
                };
                if !existing.last().unwrap().ends_with(['\n', '\r']) {
                    content.push_str(le);
                }
                for s in new {
                    let (a, x) = age_sex(s);
                    let mut cells = vec![s.as_str()];
                    for c in &cols[1..] {
                        cells.push(match *c {
                            "age" => a,
                            "sex" => x,
                            "group" => "control",
                            _ => "n/a",
                        });
                    }
                    content.push_str(&cells.join("\t"));
                    content.push_str(le);
                }
            }
        } else {
            for s in new {
                content.push_str(&row(s));
            }
        }
        if content != text {
            fs.write(&tsv, content.into_bytes())?;
        }
    } else {
        let content = format!(
            "{header}{}",
            subs.iter().map(|s| row(s)).collect::<String>()
        );
        fs.write(&tsv, content.into_bytes())?;
    }
    let pj = join(root, "participants.json");
    if !fs.exists(&pj) {
        fs.save_json(&pj, &Value::Object(template(PARTICIPANTS_JSON)))?;
    }
    Ok(())
}

fn task_of(stem: &str) -> &str {
    &stem.split("_acq-").next().unwrap()["task-".len()..]
}

/// `_ensure_taskname`.
fn ensure_taskname(fs: &mut Vfs, jp: &str, task: &str) -> Res<()> {
    if !fs.is_file(jp) {
        return Ok(());
    }
    let Ok(Value::Object(mut d)) = fs.load_json(jp) else {
        return Ok(());
    };
    if truthy(d.get("TaskName")) {
        return Ok(());
    }
    d.insert("TaskName".into(), task.into());
    fs.save_json(jp, &Value::Object(d))
}

/// `_emit_task_bold_jsons`.
fn emit_task_bold_jsons(cx: &mut Cx, root: &str) -> Res<()> {
    let mut stems = BTreeSet::new();
    let mut with_acq = BTreeSet::new();
    for nii in cx.fs.rglob(root, "*_bold.nii*") {
        if cx.fs.has_part(&nii, "derivatives") {
            continue;
        }
        let Some(m) = TASK_STEM_RE.captures(name(&nii)) else {
            continue;
        };
        let stem = m[1].to_string();
        let task = task_of(&stem).to_string();
        if stem.contains("_acq-") {
            with_acq.insert(task.clone());
        }
        stems.insert(stem);
        let n = name(&nii);
        let side = n.strip_suffix(".nii.gz").or_else(|| n.strip_suffix(".nii"));
        if let Some(base) = side {
            ensure_taskname(cx.fs, &join(parent(&nii), &format!("{base}.json")), &task)?;
        }
    }
    for stem in &stems {
        let task = task_of(stem);
        if !stem.contains("_acq-") && with_acq.contains(task) {
            continue;
        }
        let p = join(root, &format!("{stem}_bold.json"));
        if !cx.fs.exists(&p) {
            let mut d = Map::new();
            d.insert("TaskName".into(), task.into());
            cx.fs.save_json(&p, &Value::Object(d))?;
        }
    }
    for p in cx.fs.glob(root, "task-*_bold.json") {
        if !TASK_STUB_NAME_RE.is_match(name(&p)) {
            continue;
        }
        let stem = name(&p).strip_suffix("_bold.json").unwrap().to_string();
        let task = task_of(&stem).to_string();
        let bare_with_acq = !stem.contains("_acq-") && with_acq.contains(&task);
        if stems.contains(&stem) && !bare_with_acq {
            continue;
        }
        let generated = match cx.fs.load_json(&p) {
            Ok(Value::Object(d)) => {
                d.keys().all(|k| k == "TaskName" || k == "CogAtlasID")
                    && d.get("TaskName").and_then(Value::as_str) == Some(&task)
            }
            _ => false,
        };
        if generated {
            let _ = cx.fs.unlink(&p);
        } else {
            let why = if bare_with_acq {
                "conflicts with _acq- task sidecars"
            } else {
                "has no matching bold data"
            };
            cx.log(format!("reproinx: {p} {why}; preserving curated file"));
        }
    }
    let (md, readme) = (join(root, "README.md"), join(root, "README"));
    if cx.fs.exists(&readme) && cx.fs.exists(&md) && is_dcm2niix_readme_stub(cx.fs, &md)? {
        let _ = cx.fs.unlink(&md);
    }
    Ok(())
}

fn is_dcm2niix_readme_stub(fs: &Vfs, p: &str) -> Res<bool> {
    let text = match fs.read(p) {
        Ok(_) => fs.read_text(p)?,
        Err(_) => return Ok(false),
    };
    Ok(text.chars().count() <= 1024
        && text.starts_with("Generated using dcm2niix")
        && text.contains("Describe your dataset here"))
}

/// `_backfill_tasknames`.
fn backfill_tasknames(fs: &mut Vfs, root: &str, pattern: &str) -> Res<usize> {
    let mut n = 0;
    for jp in fs.rglob(root, pattern) {
        if fs.has_part(&jp, "derivatives") {
            continue;
        }
        let Some(m) = TASK_RE.captures(name(&jp)) else {
            continue;
        };
        let task = m[1].to_string();
        let Ok(Value::Object(mut d)) = fs.load_json(&jp) else {
            continue;
        };
        if truthy(d.get("TaskName")) {
            continue;
        }
        d.insert("TaskName".into(), task.into());
        fs.save_json(&jp, &Value::Object(d))?;
        n += 1;
    }
    Ok(n)
}
