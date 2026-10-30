//! `--shift-dates`: per-subject whole-day shift so the earliest scan lands on
//! 1925-01-01. Fail-closed: any failure is fatal regardless of `--strict`.

use std::collections::HashMap;

use serde_json::{Map, Value};

use super::*;
use crate::tables::PROVENANCE_TSV;

re!(TS_SESSION_RE, r"^ses-[0-9]{8}T[0-9]{6}$");

const REF_YEAR: i64 = 1925;

/// Days since 1970-01-01 (proleptic Gregorian).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + (m <= 2) as i64, m, d)
}

fn valid_date(y: i64, m: i64, d: i64) -> bool {
    (1..=9999).contains(&y)
        && (1..=12).contains(&m)
        && d >= 1
        && civil_from_days(days_from_civil(y, m, d)) == (y, m, d)
}

/// Naive datetime (+ optional UTC offset text kept for output).
#[derive(Clone, Copy, PartialEq, PartialOrd)]
struct Dt {
    days: i64,
    secs: i64,
    micros: i64,
    tz: Option<i64>, // UTC offset, seconds
}

/// Python datetime ordering: aware values compare in UTC; comparing aware
/// with naive raises TypeError.
fn earlier(a: &Dt, b: &Dt) -> Res<bool> {
    if a.tz.is_some() != b.tz.is_some() {
        return err("can't compare offset-naive and offset-aware datetimes");
    }
    let k = |d: &Dt| (d.days * 86400 + d.secs - d.tz.unwrap_or(0), d.micros);
    Ok(k(a) < k(b))
}

fn earliest(dates: impl IntoIterator<Item = Dt>) -> Res<Option<Dt>> {
    let mut e: Option<Dt> = None;
    for d in dates {
        if e.is_none() || earlier(&d, e.as_ref().unwrap())? {
            e = Some(d);
        }
    }
    Ok(e)
}

impl Dt {
    /// Subtract whole days; Python raises OverflowError outside years 1..9999.
    fn shift(&mut self, off: i64) -> Res<()> {
        self.days -= off;
        if (1..=9999).contains(&civil_from_days(self.days).0) {
            Ok(())
        } else {
            err("date value out of range")
        }
    }

    fn iso(&self) -> String {
        let (y, m, d) = civil_from_days(self.days);
        let mut s = format!(
            "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}",
            self.secs / 3600,
            self.secs / 60 % 60,
            self.secs % 60
        );
        if self.micros != 0 {
            s.push_str(&format!(".{:06}", self.micros));
        }
        if let Some(tz) = self.tz {
            let a = tz.abs();
            s.push_str(&format!(
                "{}{:02}:{:02}",
                if tz < 0 { '-' } else { '+' },
                a / 3600,
                a / 60 % 60
            ));
            if a % 60 != 0 {
                s.push_str(&format!(":{:02}", a % 60));
            }
        }
        s
    }
}

fn digits(s: &str, n: usize) -> Option<i64> {
    (s.len() >= n && s.as_bytes()[..n].iter().all(u8::is_ascii_digit))
        .then(|| s[..n].parse().unwrap())
}

/// Subset of Python 3.11+ `datetime.fromisoformat` that dcm2niix and
/// reproinx emit: extended/basic dates, `T`/space separator, HH[:MM[:SS[.f+]]],
/// optional Z/±HH[:MM].
fn parse_iso(s: &str) -> Option<Dt> {
    if !s.is_ascii() {
        return None;
    }
    let (y, m, d, rest) = if s.len() >= 10 && s.as_bytes()[4] == b'-' {
        (
            digits(s, 4)?,
            digits(&s[5..], 2)?,
            digits(&s[8..], 2)?,
            &s[10..],
        )
    } else {
        (
            digits(s, 4)?,
            digits(&s[4..], 2)?,
            digits(&s[6..], 2)?,
            &s[8..],
        )
    };
    if !valid_date(y, m, d) {
        return None;
    }
    let mut dt = Dt {
        days: days_from_civil(y, m, d),
        secs: 0,
        micros: 0,
        tz: None,
    };
    if rest.is_empty() {
        return Some(dt);
    }
    let mut t = &rest[1..]; // any single separator character
    let (tz_at, sign) = match t.find(['+', '-', 'Z']) {
        Some(i) => (i, t.as_bytes()[i]),
        None => (t.len(), 0),
    };
    let tz = &t[tz_at..];
    t = &t[..tz_at];
    let (hms, frac) = t.split_once(['.', ',']).map_or((t, ""), |(a, b)| (a, b));
    let parts: Vec<&str> = if hms.contains(':') {
        hms.split(':').collect()
    } else {
        hms.as_bytes()
            .chunks(2)
            .map(|c| std::str::from_utf8(c).unwrap())
            .collect()
    };
    if parts.is_empty() || parts.len() > 3 || parts.iter().any(|p| p.len() != 2) {
        return None;
    }
    let v: Vec<i64> = parts.iter().map(|p| digits(p, 2)).collect::<Option<_>>()?;
    let (h, mi, se) = (v[0], *v.get(1).unwrap_or(&0), *v.get(2).unwrap_or(&0));
    if h > 23 || mi > 59 || se > 59 || (!frac.is_empty() && parts.len() != 3) {
        return None;
    }
    if !frac.is_empty() {
        if !frac.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        dt.micros = format!("{:0<6}", &frac[..frac.len().min(6)])
            .parse()
            .unwrap();
    }
    dt.secs = h * 3600 + mi * 60 + se;
    if !tz.is_empty() {
        if sign == b'Z' {
            if tz.len() != 1 {
                return None;
            }
            dt.tz = Some(0);
        } else {
            // Python: ±HH[:MM[:SS]], strictly less than 24 h.
            let b = tz[1..].replace(':', "");
            if ![2, 4, 6].contains(&b.len()) {
                return None;
            }
            let f = |i: usize| {
                if b.len() > i {
                    digits(&b[i..], 2)
                } else {
                    Some(0)
                }
            };
            let (h, m, s) = (f(0)?, f(2)?, f(4)?);
            if h > 23 || m > 59 || s > 59 {
                return None;
            }
            let off = h * 3600 + m * 60 + s;
            dt.tz = Some(if sign == b'-' { -off } else { off });
        }
    }
    Some(dt)
}

/// `_parse_acq_dt`.
fn parse_acq_dt(s: Option<String>) -> Option<Dt> {
    let s = s?;
    if s.is_empty() || s == "n/a" {
        None
    } else {
        parse_iso(&s)
    }
}

fn ref_offset(dt: &Dt) -> i64 {
    dt.days - days_from_civil(REF_YEAR, 1, 1)
}

fn date8(v: Option<&Value>) -> Option<(i64, i64, i64)> {
    let Some(Value::String(a)) = v else {
        return None;
    };
    if a.len() != 8 || !a.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (y, m, d) = (digits(a, 4)?, digits(&a[4..], 2)?, digits(&a[6..], 2)?);
    valid_date(y, m, d).then_some((y, m, d))
}

/// `_sidecar_date`.
fn sidecar_date(d: &Map<String, Value>) -> Res<Option<Dt>> {
    Ok(parse_acq_dt(acq_iso(d)?.map(|a| a.0)).or_else(|| {
        let (y, m, dd) = date8(d.get("AcquisitionDate"))?;
        Some(Dt {
            days: days_from_civil(y, m, dd),
            secs: 0,
            micros: 0,
            tz: None,
        })
    }))
}

/// `_shift_sidecar_dates`.
fn shift_sidecar_dates(d: &mut Map<String, Value>, off: i64) -> Res<bool> {
    let mut changed = false;
    let adt = d
        .get("AcquisitionDateTime")
        .filter(|v| truthy(Some(v)))
        .map(|v| py_str(Some(v)));
    if let Some(mut dt) = parse_acq_dt(adt) {
        dt.shift(off)?;
        d.insert("AcquisitionDateTime".into(), dt.iso().into());
        changed = true;
    }
    if let Some((y, m, dd)) = date8(d.get("AcquisitionDate")) {
        let (y, m, dd) = civil_from_days(days_from_civil(y, m, dd) - off);
        if !(1..=9999).contains(&y) {
            return err("date value out of range");
        }
        d.insert(
            "AcquisitionDate".into(),
            format!("{y:04}{m:02}{dd:02}").into(),
        );
        changed = true;
    }
    Ok(changed)
}

fn strict_load(fs: &Vfs, p: &str) -> Res<Map<String, Value>> {
    match fs.load_json(p) {
        Ok(Value::Object(m)) => Ok(m),
        Ok(_) => Ok(Map::new()),
        Err(e) => err(format!("cannot read {p} for date-shift: {e}")),
    }
}

fn acq_col(lines: &[&str]) -> Option<usize> {
    lines.first()?.split('\t').position(|c| c == "acq_time")
}

/// `_scans_acq_dates`.
fn scans_acq_dates(fs: &Vfs, p: &str) -> Res<Vec<Dt>> {
    let text = fs.read_text(p)?;
    let lines: Vec<&str> = py_lines(&text);
    if lines.len() < 2 {
        return Ok(Vec::new());
    }
    let Some(c) = acq_col(&lines) else {
        return Ok(Vec::new());
    };
    Ok(lines[1..]
        .iter()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| {
            l.split('\t')
                .nth(c)
                .and_then(|v| parse_acq_dt(Some(v.to_string())))
        })
        .collect())
}

/// `_shift_scans_tsv_dates`.
fn shift_scans_tsv_dates(fs: &mut Vfs, p: &str, off: i64) -> Res<()> {
    let text = fs.read_text(p)?;
    let lines: Vec<&str> = py_lines(&text);
    if lines.len() < 2 {
        return Ok(());
    }
    let Some(c) = acq_col(&lines) else {
        return Ok(());
    };
    let mut out = vec![lines[0].to_string()];
    for l in &lines[1..] {
        if l.trim().is_empty() {
            out.push(l.to_string());
            continue;
        }
        let mut cols: Vec<String> = l.split('\t').map(String::from).collect();
        if let Some(mut dt) = cols.get(c).and_then(|v| parse_acq_dt(Some(v.clone()))) {
            dt.shift(off)?;
            cols[c] = dt.iso();
        }
        out.push(cols.join("\t"));
    }
    fs.write(p, format!("{}\r\n", out.join("\r\n")).into_bytes())
}

/// `_shift_dates` -> subjects shifted.
fn shift_dates(cx: &mut Cx, root: &str) -> Res<usize> {
    let subjects = walk_subjects(cx.fs, root);
    let mut offsets: HashMap<String, i64> = HashMap::new();
    let mut shifted = 0;
    for sub in &subjects {
        let jsons = cx.fs.rglob(sub, "*.json");
        let scans = cx.fs.rglob(sub, "*_scans.tsv");
        let mut found = Vec::new();
        for jp in &jsons {
            found.extend(sidecar_date(&strict_load(cx.fs, jp)?)?);
        }
        for sc in &scans {
            found.extend(scans_acq_dates(cx.fs, sc)?);
        }
        let earliest = earliest(found)?;
        let Some(e) = earliest else { continue };
        let off = ref_offset(&e);
        offsets.insert(name(sub).to_string(), off);
        if off == 0 {
            continue;
        }
        for jp in &jsons {
            let mut d = strict_load(cx.fs, jp)?;
            if shift_sidecar_dates(&mut d, off)? {
                cx.fs.save_json(jp, &Value::Object(d))?;
            }
        }
        for sc in &scans {
            shift_scans_tsv_dates(cx.fs, sc, off)?;
        }
        shifted += 1;
    }
    for sub in &subjects {
        let leaky: Vec<String> = cx
            .fs
            .glob(sub, "ses-*")
            .iter()
            .map(|p| name(p).to_string())
            .filter(|n| TS_SESSION_RE.is_match(n))
            .collect();
        if !leaky.is_empty() {
            cx.log(format!(
                "reproinx: WARNING {}: --shift-dates anchored sidecars/scans.tsv to {REF_YEAR}, but timestamp session dir(s) {leaky:?} still leak the real date via the path. Use named sessions to fully de-identify.",
                name(sub)
            ));
        }
    }
    let already = |p: &str| {
        subjects
            .iter()
            .any(|s| vfs::relative(p, s).is_some_and(|r| !r.is_empty()))
    };
    let offset_for = |fs: &Vfs, p: &str, own: &Dt| {
        let parts = fs.parts(p);
        let sub = parts[..parts.len() - 1]
            .iter()
            .find(|x| x.starts_with("sub-"));
        sub.and_then(|s| offsets.get(*s).copied())
            .unwrap_or_else(|| ref_offset(own))
    };
    for jp in cx.fs.rglob(root, "*.json") {
        if already(&jp) {
            continue;
        }
        let mut d = strict_load(cx.fs, &jp)?;
        let Some(dt) = sidecar_date(&d)? else {
            continue;
        };
        let off = offset_for(cx.fs, &jp, &dt);
        if off != 0 && shift_sidecar_dates(&mut d, off)? {
            cx.fs.save_json(&jp, &Value::Object(d))?;
        }
    }
    for sc in cx.fs.rglob(root, "*_scans.tsv") {
        if already(&sc) {
            continue;
        }
        let dates = scans_acq_dates(cx.fs, &sc)?;
        let Some(min) = earliest(dates)? else {
            continue;
        };
        let off = offset_for(cx.fs, &sc, &min);
        if off != 0 {
            shift_scans_tsv_dates(cx.fs, &sc, off)?;
        }
    }
    for n in [PROVENANCE_TSV.to_string(), format!("{PROVENANCE_TSV}.bak")] {
        for p in cx.fs.rglob(root, &n) {
            cx.fs.unlink(&p)?;
        }
    }
    Ok(shifted)
}

/// `_run_shift_dates`.
pub fn run_shift_dates(cx: &mut Cx, root: &str) -> Res<()> {
    match shift_dates(cx, root) {
        Ok(n) => {
            if n > 0 {
                cx.log(format!(
                    "  shifted acquisition dates to {REF_YEAR} for {n} subject(s)"
                ));
            }
            Ok(())
        }
        Err(e) => {
            cx.log(format!(
                "reproinx: ERROR --shift-dates FAILED ({e}); the output may contain real acquisition dates — treat it as NOT de-identified."
            ));
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_roundtrip_and_shift() {
        let mut d = parse_iso("2026-05-12T12:10:38.825").unwrap();
        assert_eq!(d.iso(), "2026-05-12T12:10:38.825000");
        d.days -= ref_offset(&d);
        assert_eq!(d.iso(), "1925-01-01T12:10:38.825000");
        assert_eq!(
            parse_iso("2026-05-12").unwrap().iso(),
            "2026-05-12T00:00:00"
        );
        assert_eq!(
            parse_iso("20260512T121038").unwrap().iso(),
            "2026-05-12T12:10:38"
        );
        assert_eq!(
            parse_iso("2026-05-12T12:10:38Z").unwrap().iso(),
            "2026-05-12T12:10:38+00:00"
        );
        assert!(parse_iso("2026-02-30").is_none());
        assert_eq!(
            parse_iso("2026-05-12T12:00:00+02:00:30").unwrap().iso(),
            "2026-05-12T12:00:00+02:00:30"
        );
        assert!(parse_iso("2026-05-12T12:00:00+24:00").is_none());
        assert!(parse_iso("2026-05-12T12:00:00+99:99").is_none());
        assert!(parse_iso("n/a").is_none());
        assert_eq!(civil_from_days(days_from_civil(2024, 2, 29)), (2024, 2, 29));
    }
}
