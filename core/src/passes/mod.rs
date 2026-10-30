//! Post-pass, in reproinx.py `_post_process` order. Function names follow the
//! Python functions they port (leading underscore dropped).

mod cleanup;
mod dates;
mod dup;
mod per_session;
mod rescue;
mod scaffold;
mod session;

use serde_json::Value;
use std::collections::{BTreeSet, HashMap};

use crate::tables::{BIDS_EXTS, POST_RUN_ENTITIES, PROVENANCE_TSV};
pub use crate::vfs::{self, Error, Res, Vfs, err, join, name, parent};
use crate::{Options, Report, nifti};

macro_rules! re {
    ($name:ident, $pat:expr) => {
        pub(crate) static $name: std::sync::LazyLock<regex_lite::Regex> =
            std::sync::LazyLock::new(|| regex_lite::Regex::new($pat).unwrap());
    };
}
pub(crate) use re;

pub struct Cx<'a> {
    pub fs: &'a mut Vfs,
    pub opts: &'a Options,
    pub report: &'a mut Report,
}

impl Cx<'_> {
    pub fn log(&mut self, msg: impl Into<String>) {
        self.report.log.push(msg.into());
    }

    /// `print(...); if strict: raise`.
    pub fn fail(&mut self, msg: String) -> Res<()> {
        let line = format!("reproinx: {msg}");
        self.report.log.push(line.clone());
        self.report.failures.push(msg.clone());
        if self.opts.strict {
            Err(Error::Other(msg))
        } else {
            Ok(())
        }
    }
}

re!(SES_RE, r"_ses-([A-Za-z0-9]+)");
re!(SES_PROTOCOL_RE, r"(?:^|_)ses-([A-Za-z0-9]+)");
re!(RUN_ENTITY_RE, r"_run-[0-9]+");
re!(RUN_PROTOCOL_RE, r"(?:^|_)run-([0-9]+)");
re!(TASK_TOKEN_RE, r"(?:^|_)task-([A-Za-z0-9]+)");
re!(ECHO_TOKEN_RE, r"_echo-[0-9]+");
re!(TASK_RE, r"task-([A-Za-z0-9]+)");

/// Provenance row (`_load_provenance`); `sn` is the int SeriesNumber.
pub struct Row {
    cols: HashMap<String, String>,
    pub sn: i64,
}

impl Row {
    pub fn get(&self, k: &str) -> &str {
        self.cols.get(k).map_or("", |s| s.as_str())
    }
}

pub fn load_provenance(fs: &Vfs, root: &str) -> Vec<Row> {
    let p = join(root, PROVENANCE_TSV);
    let Ok(b) = fs.read(&p) else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(b)
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let mut lines = text.split_inclusive('\n');
    let Some(header) = lines.next() else {
        return Vec::new();
    };
    let cols: Vec<&str> = header.trim_end_matches('\n').split('\t').collect();
    let mut rows = Vec::new();
    for line in lines {
        let parts: Vec<&str> = line.trim_end_matches('\n').split('\t').collect();
        if parts.len() < cols.len() {
            continue;
        }
        let cols: HashMap<String, String> = cols
            .iter()
            .zip(&parts)
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let sn = py_int(cols.get("SeriesNumber").map_or("0", |s| s)).unwrap_or(0);
        rows.push(Row { cols, sn });
    }
    rows
}

/// Python `str.splitlines(keepends)`.
pub fn py_split(s: &str, keepends: bool) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut it = s.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        if matches!(
            c,
            '\n' | '\r'
                | '\x0b'
                | '\x0c'
                | '\x1c'
                | '\x1d'
                | '\x1e'
                | '\u{85}'
                | '\u{2028}'
                | '\u{2029}'
        ) {
            let mut end = i + c.len_utf8();
            if c == '\r' && it.peek().map(|x| x.1) == Some('\n') {
                it.next();
                end += 1;
            }
            out.push(&s[start..if keepends { end } else { i }]);
            start = end;
        }
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

/// Python `str.splitlines()`.
pub fn py_lines(s: &str) -> Vec<&str> {
    py_split(s, false)
}

/// Python numeric literal underscores: allowed only between digits.
fn strip_underscores(s: &str) -> Option<String> {
    let b = s.as_bytes();
    for (i, &c) in b.iter().enumerate() {
        if c == b'_'
            && !(i > 0 && b[i - 1].is_ascii_digit() && b.get(i + 1).is_some_and(u8::is_ascii_digit))
        {
            return None;
        }
    }
    Some(s.replace('_', ""))
}

/// Python `int(str)` (ASCII digits only; see README D3).
pub fn py_int(s: &str) -> Option<i64> {
    strip_underscores(s.trim())?.parse().ok()
}

/// Python `float(str)` for finite decimal literals.
pub fn py_float(s: &str) -> Option<f64> {
    strip_underscores(s.trim())?
        .parse::<f64>()
        .ok()
        .filter(|f| f.is_finite())
}

/// Python `int(x)` on a JSON value (bool/int/float/str).
pub fn py_int_value(v: Option<&Value>) -> Option<i64> {
    match v? {
        Value::Bool(b) => Some(*b as i64),
        Value::Number(n) => n.as_i64().or_else(|| {
            n.as_f64()
                .filter(|f| f.is_finite())
                .map(|f| f.trunc() as i64)
        }),
        Value::String(s) => py_int(s),
        _ => None,
    }
}

/// Python `str(x)` for a JSON value (`""` when absent).
pub fn py_str(v: Option<&Value>) -> String {
    match v {
        None => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) => "None".into(),
        Some(Value::Bool(b)) => if *b { "True" } else { "False" }.into(),
        Some(Value::Number(n)) if n.is_f64() => crate::json::float_repr(n.as_f64().unwrap()),
        Some(v) => v.to_string(),
    }
}

/// Python truthiness of a JSON value.
pub fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64() != Some(0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

/// `_move_stem_files`: all-or-nothing move of a series family.
pub fn move_stem_files(fs: &mut Vfs, src_stem: &str, dst_stem: &str) -> Res<usize> {
    let (src_dir, src_name) = (parent(src_stem), name(src_stem));
    if !fs.is_dir(src_dir) {
        return Ok(0);
    }
    let (dst_dir, dst_name) = (parent(dst_stem), name(dst_stem));
    let mut pairs = Vec::new();
    for ext in BIDS_EXTS {
        let src = join(src_dir, &format!("{src_name}{ext}"));
        if !fs.is_file(&src) {
            continue;
        }
        let dst = join(dst_dir, &format!("{dst_name}{ext}"));
        if fs.exists(&dst) {
            return Err(Error::Exists(dst));
        }
        pairs.push((src, dst));
    }
    if pairs.is_empty() {
        return Ok(0);
    }
    fs.mkdirs(dst_dir)?;
    for (s, d) in &pairs {
        fs.replace(s, d)?;
    }
    Ok(pairs.len())
}

/// `_stem_has_files`.
pub fn stem_has_files(fs: &Vfs, stem: &str) -> bool {
    BIDS_EXTS
        .iter()
        .any(|ext| fs.is_file(&format!("{stem}{ext}")))
}

/// `_series_stem`.
pub fn series_stem(fname: &str) -> &str {
    BIDS_EXTS
        .iter()
        .find_map(|ext| fname.strip_suffix(ext))
        .unwrap_or(fname)
}

/// NIfTI name minus `.nii.gz`, else `Path.stem`.
pub fn nii_stem(n: &str) -> &str {
    n.strip_suffix(".nii.gz").unwrap_or_else(|| vfs::stem(n))
}

/// `_inject_session_into_name`.
pub fn inject_session_into_name(fname: &str, sub_token: &str, ses: &str) -> String {
    if SES_RE.is_match(fname) {
        return fname.to_string();
    }
    match fname.strip_prefix(&format!("{sub_token}_")) {
        Some(rest) => format!("{sub_token}_ses-{ses}_{rest}"),
        None => fname.to_string(),
    }
}

/// `_heudiconv_subject_token`.
pub fn heudiconv_subject_token(patient_id: &str) -> String {
    patient_id
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

/// `_insert_run_entity`.
pub fn insert_run_entity(entity_suffix: &str, run_token: &str) -> String {
    let mut toks: Vec<&str> = entity_suffix
        .split('_')
        .filter(|t| !t.is_empty() && !t.starts_with("run-"))
        .collect();
    let mut at = toks.len().saturating_sub(1);
    for (i, t) in toks.iter().enumerate() {
        if let Some((k, _)) = t.split_once('-')
            && POST_RUN_ENTITIES.contains(&k)
        {
            at = i;
            break;
        }
    }
    toks.insert(at, run_token);
    format!("_{}", toks.join("_"))
}

/// `_nifti_ndim`.
pub fn nii_ndim(fs: &Vfs, p: &str) -> Option<i64> {
    nifti::Header::parse(fs.header(p)?).map(|h| h.ndim())
}

pub type Volume = ([[f64; 4]; 4], [i64; 3]);

/// First `<stem>.nii*` beside a sidecar (sorted; README D1).
pub fn nii_for_json(fs: &Vfs, jp: &str) -> Option<String> {
    let prefix = format!("{}.nii", vfs::stem(jp));
    fs.iterdir(parent(jp))
        .into_iter()
        .find(|c| name(c).starts_with(&prefix))
}

/// `_imaging_volume`: affine + shape of the first `<stem>.nii*` beside `json`.
pub fn imaging_volume(fs: &Vfs, json: &str) -> Option<Volume> {
    let nii = nii_for_json(fs, json)?;
    let h = nifti::Header::parse(fs.header(&nii)?)?;
    Some((h.affine()?, h.shape3()))
}

/// `_hms_seconds`.
pub fn hms_seconds(v: Option<&Value>) -> Option<f64> {
    if truthy(v) { hms_str(&py_str(v)) } else { None }
}

pub fn hms_str(s: &str) -> Option<f64> {
    if s.is_empty() {
        return None;
    }
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 3 {
        return None;
    }
    let f = py_float;
    Some(f(parts[0])? * 3600.0 + f(parts[1])? * 60.0 + f(parts[2])?)
}

/// `_acq_iso` -> (text, came from a JSON string). Python raises TypeError
/// slicing a non-string 8-character `AcquisitionDate`.
pub fn acq_iso(d: &serde_json::Map<String, Value>) -> Res<Option<(String, bool)>> {
    let adt = d.get("AcquisitionDateTime");
    if truthy(adt) {
        return Ok(Some((py_str(adt), adt.is_some_and(Value::is_string))));
    }
    let (dd, t) = (d.get("AcquisitionDate"), d.get("AcquisitionTime"));
    if truthy(dd) && truthy(t) && py_str(dd).chars().count() == 8 {
        let Some(Value::String(dd)) = dd else {
            return err("AcquisitionDate is not a string");
        };
        let s: Vec<char> = dd.chars().collect();
        let c = |a: usize, b: usize| s[a..b].iter().collect::<String>();
        return Ok(Some((
            format!("{}-{}-{}T{}", c(0, 4), c(4, 6), c(6, 8), py_str(t)),
            true,
        )));
    }
    Ok(None)
}

/// `_walk_subjects`.
pub fn walk_subjects(fs: &Vfs, root: &str) -> Vec<String> {
    fs.rglob(root, "sub-*")
        .into_iter()
        .filter(|s| fs.is_dir(s) && !fs.has_part(s, "derivatives"))
        .filter(|s| {
            let p = fs.name_of(parent(s));
            !(p.starts_with("sub-") || p.starts_with("ses-"))
        })
        .collect()
}

/// `_bids_roots`.
pub fn bids_roots(subjects: &[String]) -> Vec<String> {
    let set: BTreeSet<String> = subjects.iter().map(|s| parent(s).to_string()).collect();
    let mut v: Vec<String> = set.into_iter().collect();
    vfs::sort_paths(&mut v);
    v
}

/// `_walk_sessions`.
pub fn walk_sessions(fs: &Vfs, root: &str) -> Vec<String> {
    let mut out = Vec::new();
    for sub in fs.rglob(root, "sub-*") {
        if !fs.is_dir(&sub) || fs.has_part(&sub, "derivatives") {
            continue;
        }
        let child_dirs: Vec<String> = fs
            .iterdir(&sub)
            .into_iter()
            .filter(|d| fs.is_dir(d) && !name(d).starts_with('.'))
            .collect();
        let ses: Vec<&String> = child_dirs
            .iter()
            .filter(|d| name(d).starts_with("ses-"))
            .collect();
        let n_ses = ses.len();
        out.extend(ses.into_iter().cloned());
        if n_ses < child_dirs.len() {
            out.push(sub);
        }
    }
    out
}

/// `_bids_root_for_session`.
pub fn bids_root_for_session(fs: &Vfs, ses: &str) -> String {
    if fs.name_of(ses).starts_with("ses-") {
        parent(parent(ses)).to_string()
    } else {
        parent(ses).to_string()
    }
}

/// `_post_process`.
pub fn post_process(cx: &mut Cx) -> Res<()> {
    let root = "";
    match rescue::maybe_collapse_nonreproin_root(cx, root) {
        Ok(true) => cx.log("  collapsed redundant non-reproin study hierarchy"),
        Ok(false) => {}
        Err(e) => cx.fail(format!("study-root collapse failed: {e}"))?,
    }
    let mut seen = BTreeSet::new();
    for unknown in cx.fs.rglob(root, "Unknown") {
        if !cx.fs.is_dir(&unknown) || cx.fs.has_part(&unknown, "derivatives") {
            continue;
        }
        let r = parent(&unknown).to_string();
        if !seen.insert(r.clone()) {
            continue;
        }
        match rescue::rescue_unknown_dir(cx, &r) {
            Ok(n) if n > 0 => cx.log(format!("  {r}: rescued {n} file-stem(s) from Unknown/")),
            Ok(_) => {}
            Err(e) => cx.fail(format!("Unknown/-rescue failed for {r}: {e}"))?,
        }
        match rescue::purge_all_discard_unknown(cx, &r) {
            Ok(true) => cx.log(format!("  {r}: removed all-discard Unknown/")),
            Ok(false) => {}
            Err(e) => cx.fail(format!("all-discard purge failed for {r}: {e}"))?,
        }
    }
    cleanup::bidsguess_cleanup(cx, root)?;
    for r in bids_roots(&walk_subjects(cx.fs, root)) {
        match dup::apply_dup_naming(cx, &r) {
            Ok(n) if n > 0 => cx.log(format!(
                "  {r}: renamed {n} collision suffix(es) to __dup-NN"
            )),
            Ok(_) => {}
            Err(e) => cx.fail(format!("__dup rename failed for {r}: {e}"))?,
        }
    }
    for sub in walk_subjects(cx.fs, root) {
        match session::propagate_session(cx, &sub) {
            Ok(Some(s)) => cx.log(format!("  {sub}: propagated _ses-{s}")),
            Ok(None) => {}
            Err(e) => cx.fail(format!("session backfill failed for {sub}: {e}"))?,
        }
    }
    let mut physio_skipped: Vec<(String, usize)> = Vec::new();
    for r in bids_roots(&walk_subjects(cx.fs, root)) {
        match session::rescue_physio_recordings(cx, &r) {
            Ok((rescued, skipped)) => {
                if rescued > 0 {
                    cx.log(format!(
                        "  {r}: rescued {rescued} physio file(s) from derivatives/scanner/"
                    ));
                }
                if skipped > 0 {
                    physio_skipped.push((r, skipped));
                }
            }
            Err(e) => cx.fail(format!("physio rescue failed for {r}: {e}"))?,
        }
    }
    let sessions = walk_sessions(cx.fs, root);
    if sessions.is_empty() {
        cx.log("reproinx: no sub-* folders under output root");
        if cx.opts.shift_dates {
            dates::run_shift_dates(cx, root)?;
        }
        return Ok(());
    }
    for ses in &sessions {
        match per_session::strip_sbref_gradients(cx.fs, ses) {
            Ok(n) if n > 0 => cx.log(format!("  {ses}: stripped {n} .bval/.bvec from _sbref")),
            Ok(_) => {}
            Err(e) => cx.fail(format!("sbref gradient strip failed for {ses}: {e}"))?,
        }
        let r = per_session::write_scans_tsv(cx.fs, ses)
            .and_then(|_| per_session::emit_events_tsv(cx.fs, ses))
            .and_then(|_| per_session::populate_b0_fields(cx, ses));
        match r {
            Ok(n) => cx.log(format!("  {ses}: paired {n} fmap group(s)")),
            Err(e) => cx.fail(format!("post-processing failed for {ses}: {e}"))?,
        }
    }
    let roots = bids_roots(&walk_subjects(cx.fs, root));
    cx.report.roots = roots.clone();
    if let Err(e) = scaffold::scaffold_roots(cx, &roots) {
        cx.fail(format!("scaffolding failed: {e}"))?;
    }
    if !cx.opts.keep_derivatives {
        for (r, n) in &physio_skipped {
            cx.log(format!(
                "reproinx: kept derivatives/scanner/ under {r} — {n} un-rescued physio file(s) would be lost. \
                 Re-run with --keep-derivatives to acknowledge, or resolve the collision/multi-session blocker."
            ));
        }
        let skip: BTreeSet<String> = physio_skipped.iter().map(|(r, _)| r.clone()).collect();
        match session::drop_derivatives(cx.fs, root, &skip) {
            Ok(n) if n > 0 => cx.log(format!(
                "  removed derivatives/scanner/ from {n} BIDS root(s)"
            )),
            Ok(_) => {}
            Err(e) => cx.fail(format!("derivatives removal failed: {e}"))?,
        }
    }
    if cx.opts.shift_dates {
        dates::run_shift_dates(cx, root)?;
    }
    Ok(())
}
