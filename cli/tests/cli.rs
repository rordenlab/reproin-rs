//! CLI contract: argument/exit-code parity, dry run, rollback on failure.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use reproin_core::Op;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_reproinx"))
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("reproinx-test-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// path -> contents (dirs as None), relative to `root`.
fn snapshot(root: &Path) -> BTreeMap<String, Option<Vec<u8>>> {
    let mut m = BTreeMap::new();
    fn walk(root: &Path, d: &Path, m: &mut BTreeMap<String, Option<Vec<u8>>>) {
        for e in fs::read_dir(d).unwrap() {
            let p = e.unwrap().path();
            let rel = p.strip_prefix(root).unwrap().to_string_lossy().into_owned();
            if p.is_dir() {
                m.insert(rel, None);
                walk(root, &p, m);
            } else {
                m.insert(rel, Some(fs::read(&p).unwrap()));
            }
        }
    }
    walk(root, root, &mut m);
    m
}

fn fixture(name: &str) -> PathBuf {
    let d = tmp(name);
    let func = d.join("sub-01/func");
    fs::create_dir_all(&func).unwrap();
    fs::write(
        func.join("sub-01_task-rest_bold.json"),
        r#"{"AcquisitionDateTime": "2026-01-02T03:04:05"}"#,
    )
    .unwrap();
    let mut h = vec![0u8; 352];
    h[0..4].copy_from_slice(&348i32.to_le_bytes());
    for (i, v) in [4i16, 2, 2, 2, 10, 1, 1, 1].iter().enumerate() {
        h[40 + 2 * i..42 + 2 * i].copy_from_slice(&v.to_le_bytes());
    }
    fs::write(func.join("sub-01_task-rest_bold.nii"), h).unwrap();
    d
}

#[test]
fn argument_errors_exit_2() {
    let code = |args: &[&str]| bin().args(args).output().unwrap().status.code();
    assert_eq!(code(&[]), Some(2));
    assert_eq!(code(&["-N", "x", "."]), Some(2));
    assert_eq!(code(&["--bogus", "."]), Some(2));
    assert_eq!(code(&["a", "b", "c", "d", "e"]), Some(2));
    assert_eq!(code(&["/nonexistent/reproinx"]), Some(2));
    assert_eq!(code(&["-h"]), Some(0));
    assert_eq!(code(&["--version"]), Some(0));
}

#[test]
fn post_pass_and_dry_run() {
    let d = fixture("dry");
    let before = snapshot(&d);
    let out = bin()
        .args(["--no-conv", "--dry-run"])
        .arg(&d)
        .arg(&d)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).contains("write   participants.tsv"));
    assert_eq!(snapshot(&d), before, "dry run must not mutate");
    let out = bin()
        .args(["--no-convert", "-N3"])
        .arg(&d)
        .arg(&d)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(d.join("participants.tsv").is_file());
    assert!(d.join("sub-01/func/sub-01_task-rest_events.tsv").is_file());
    assert!(!d.join(".reproinx-undo").exists());
    fs::remove_dir_all(d).unwrap();
}

#[test]
fn failed_apply_rolls_back() {
    let d = tmp("rollback");
    fs::write(d.join("b"), "b").unwrap();
    fs::write(d.join("c"), "old c").unwrap();
    fs::write(d.join("del"), "del").unwrap();
    fs::create_dir(d.join("empty")).unwrap();
    let before = snapshot(&d);
    let w = |p: &str, s: &str| Op::Write {
        path: p.into(),
        bytes: s.as_bytes().to_vec(),
    };
    let ops = vec![
        Op::Mkdir { path: "x".into() },
        w("x/a", "a"),
        Op::Rename {
            from: "b".into(),
            to: "c".into(),
        },
        Op::Delete { path: "del".into() },
        Op::RemoveDir {
            path: "empty".into(),
        },
        w("c", "new c"),
        Op::Rename {
            from: "missing".into(),
            to: "e".into(),
        },
    ];
    assert!(reproinx::apply_all(&d, &ops).is_err());
    assert_eq!(snapshot(&d), before);
    assert!(
        reproinx::apply_all(
            &d,
            &[Op::Write {
                path: "../escape".into(),
                bytes: vec![]
            }]
        )
        .is_err()
    );
    assert!(!d.parent().unwrap().join("escape").exists());
    fs::write(d.join("v.nii"), vec![0u8; 400]).unwrap();
    let before = snapshot(&d);
    assert!(
        reproinx::apply_all(
            &d,
            &[
                w("y", "y"),
                Op::DropVolumes {
                    path: "v.nii".into(),
                    indices: vec![0]
                }
            ]
        )
        .is_err()
    );
    assert_eq!(snapshot(&d), before, "untrimmable image: nothing changes");
    fs::remove_dir_all(d).unwrap();
}

#[test]
fn recovery_data_is_never_discarded() {
    let d = tmp("recovery");
    fs::write(d.join("keep"), "original").unwrap();
    // Journal write fails (journal.txt is a directory): the moved file comes back.
    fs::create_dir_all(d.join(".reproinx-undo/journal.txt")).unwrap();
    let mut a = reproinx::Applier::new(&d);
    assert!(
        a.apply(&Op::Delete {
            path: "keep".into()
        })
        .is_err()
    );
    a.rollback().unwrap();
    assert_eq!(fs::read(d.join("keep")).unwrap(), b"original");
    // A rerun refuses to touch an existing recovery directory.
    fs::create_dir_all(d.join(".reproinx-undo")).unwrap();
    fs::write(d.join(".reproinx-undo/1"), "backup").unwrap();
    let out = bin().arg("--no-convert").arg(&d).arg(&d).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(reproinx::apply_all(&d, &[]).is_err());
    assert_eq!(fs::read(d.join(".reproinx-undo/1")).unwrap(), b"backup");
    fs::remove_dir_all(d.join(".reproinx-undo")).unwrap();
    // A rollback that cannot restore keeps the backup.
    let mut a = reproinx::Applier::new(&d);
    a.apply(&Op::Delete {
        path: "keep".into(),
    })
    .unwrap();
    fs::create_dir(d.join("keep")).unwrap(); // blocks the restore
    assert!(a.rollback().is_err());
    assert_eq!(fs::read(d.join(".reproinx-undo/1")).unwrap(), b"original");
    fs::remove_dir_all(d).unwrap();
}

#[cfg(unix)]
#[test]
fn symlinks_are_read_but_never_modified() {
    let d = fixture("symlink");
    let real = d.with_extension("real.tsv");
    fs::write(
        &real,
        "participant_id\tage\tsex\tgroup\nsub-old\tn/a\tn/a\tcontrol\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(&real, d.join("participants.tsv")).unwrap();
    let before = snapshot(&d);
    let out = bin().arg("--no-convert").arg(&d).arg(&d).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("refusing to modify symlink participants.tsv")
    );
    assert_eq!(snapshot(&d), before, "rolled back");
    assert!(
        fs::symlink_metadata(d.join("participants.tsv"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    fs::remove_dir_all(&d).unwrap();
    fs::remove_file(real).unwrap();
}

#[test]
fn dry_run_without_conversion_creates_nothing() {
    let d = tmp("nodir").join("missing");
    let out = bin()
        .args(["--no-convert", "--dry-run"])
        .arg(d.parent().unwrap())
        .arg(&d)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(!d.exists());
}

/// A write that fails part-way (file-size limit) must not leave a partial file.
#[cfg(unix)]
#[test]
fn partial_write_is_rolled_back() {
    // 200 series make the new sub-01_scans.tsv (~9 KB) exceed the 4-block limit.
    let d = fixture("partial");
    let anat = d.join("sub-01/anat");
    fs::create_dir_all(&anat).unwrap();
    for i in 0..200 {
        fs::write(anat.join(format!("sub-01_acq-a{i:03}_T1w.json")), "{}").unwrap();
        fs::write(anat.join(format!("sub-01_acq-a{i:03}_T1w.nii")), b"").unwrap();
    }
    let before = snapshot(&d);
    let sh = format!(
        "trap '' XFSZ; ulimit -f 4; exec '{}' --no-convert '{}' '{}'",
        env!("CARGO_BIN_EXE_reproinx"),
        d.display(),
        d.display()
    );
    let out = Command::new("sh").arg("-c").arg(sh).output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("rolled back"));
    assert_eq!(snapshot(&d), before);
    fs::remove_dir_all(d).unwrap();
}

/// A planted symlink at the DropVolumes temp name must not be written through.
#[cfg(unix)]
#[test]
fn drop_volumes_never_writes_through_a_planted_temp() {
    let d = tmp("plant");
    let mut h = vec![0u8; 352];
    h[0..4].copy_from_slice(&348i32.to_le_bytes());
    for (i, v) in [4i16, 2, 1, 1, 4, 1, 1, 1].iter().enumerate() {
        h[40 + 2 * i..42 + 2 * i].copy_from_slice(&v.to_le_bytes());
    }
    h[72..74].copy_from_slice(&8i16.to_le_bytes());
    h[108..112].copy_from_slice(&352f32.to_le_bytes());
    h.extend([0u8, 0, 1, 1, 2, 2, 3, 3]);
    fs::write(d.join("a.nii"), &h).unwrap();
    fs::write(d.join("victim.txt"), "PRECIOUS").unwrap();
    let planted = d.join(format!("a.nii.reproinx-tmp-{}-0", std::process::id()));
    std::os::unix::fs::symlink(d.join("victim.txt"), &planted).unwrap();
    reproinx::apply_all(
        &d,
        &[Op::DropVolumes {
            path: "a.nii".into(),
            indices: vec![1],
        }],
    )
    .unwrap();
    assert_eq!(fs::read(d.join("victim.txt")).unwrap(), b"PRECIOUS");
    assert_eq!(fs::read(d.join("a.nii")).unwrap().len(), 352 + 6);
    fs::remove_dir_all(d).unwrap();
}
