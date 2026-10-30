//! `_apply_dup_naming`: dcm2niix a/b/c collision suffix -> heudiconv `__dup-NN`.

use std::collections::HashMap;

use super::*;

pub fn apply_dup_naming(cx: &mut Cx, root: &str) -> Res<usize> {
    let rows = load_provenance(cx.fs, root);
    if rows.is_empty() {
        return Ok(0);
    }
    let mut order: Vec<&str> = Vec::new();
    let mut by_stem: HashMap<&str, &Row> = HashMap::new();
    for r in &rows {
        let s = r.get("OutputStem");
        if !s.is_empty() && !by_stem.contains_key(s) {
            by_stem.insert(s, r);
            order.push(s);
        }
    }
    let same = |a: &Row, b: &Row| {
        ["StudyInstanceUID", "ProtocolName", "SeriesDescription"]
            .iter()
            .all(|k| a.get(k) == b.get(k))
    };
    let mut groups: Vec<(&str, Vec<&Row>)> = Vec::new();
    for &stem in &order {
        let mut cs = stem.chars();
        let Some(last) = cs.next_back() else { continue };
        let base = cs.as_str();
        if stem.chars().count() < 2 || !last.is_ascii_lowercase() {
            continue;
        }
        if let Some(b) = by_stem.get(base)
            && same(by_stem[stem], b)
        {
            match groups.iter_mut().find(|(k, _)| *k == base) {
                Some((_, m)) => m.push(by_stem[stem]),
                None => groups.push((base, vec![by_stem[stem]])),
            }
        }
    }
    let mut renamed = 0;
    for (base, mut members) in groups {
        members.push(by_stem[base]);
        members.sort_by_key(|r| py_int(r.get("SeriesNumber")).unwrap_or(0));
        let target = |i: usize| {
            if i == 0 {
                base.to_string()
            } else {
                format!("{base}__dup-{i:02}")
            }
        };
        let at = |s: &str| join(root, s);
        if (0..members.len()).all(|i| stem_has_files(cx.fs, &at(&target(i)))) {
            continue;
        }
        let moves: Vec<(String, String)> = members
            .iter()
            .enumerate()
            .map(|(i, r)| (r.get("OutputStem").to_string(), target(i)))
            .filter(|(c, t)| c != t)
            .collect();
        if moves.is_empty() {
            continue;
        }
        if !members
            .iter()
            .any(|r| stem_has_files(cx.fs, &at(r.get("OutputStem"))))
        {
            continue;
        }
        let currents: Vec<&String> = moves.iter().map(|(c, _)| c).collect();
        let mut unsafe_ = moves.iter().any(|(c, t)| {
            !stem_has_files(cx.fs, &at(c))
                || (stem_has_files(cx.fs, &at(t)) && !currents.contains(&t))
        });
        if !unsafe_ {
            unsafe_ = members.iter().enumerate().any(|(i, r)| {
                r.get("OutputStem") == target(i) && !stem_has_files(cx.fs, &at(&target(i)))
            });
        }
        if unsafe_ {
            cx.log(format!(
                "reproinx: __dup rename skipped stale or partial group '{base}'"
            ));
            continue;
        }
        // Two-phase rename (stems -> tmp -> target); on failure undo both phases.
        let mut staged: Vec<(String, String, String)> = Vec::new();
        let mut finalized = 0;
        let r: Res<()> = (|| {
            for (i, (c, t)) in moves.iter().enumerate() {
                let tmp = format!("{base}__reproinx-tmp-{i:03}");
                if move_stem_files(cx.fs, &at(c), &at(&tmp))? < 1 {
                    return err(format!("source family disappeared: {c}"));
                }
                staged.push((c.clone(), tmp, t.clone()));
            }
            for (_, tmp, t) in &staged {
                if move_stem_files(cx.fs, &at(tmp), &at(t))? < 1 {
                    return err(format!("temporary family disappeared: {tmp}"));
                }
                finalized += 1;
            }
            Ok(())
        })();
        if let Err(e) = r {
            for (_, tmp, t) in staged[..finalized].iter().rev() {
                let _ = move_stem_files(cx.fs, &at(t), &at(tmp));
            }
            for (c, tmp, _) in staged.iter().rev() {
                if stem_has_files(cx.fs, &at(tmp)) {
                    let _ = move_stem_files(cx.fs, &at(tmp), &at(c));
                }
            }
            return Err(e);
        }
        renamed += staged.len();
    }
    Ok(renamed)
}
