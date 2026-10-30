//! Native host for reproin-core: inventory a real output tree and apply a
//! plan's ops with before-images so a failed apply rolls back.

use std::fs;
use std::io::{self, BufReader, BufWriter, Read};
use std::path::{Path, PathBuf};

use reproin_core::{Data, Op, Vfs, Want, nifti, want};

/// Inventory `root`. Symlinked files are read through the link (like the
/// reference's `is_file`); symlinked directories are not descended. Ops that
/// would modify a symlink are refused at apply time (README D5).
pub fn inventory(root: &Path) -> io::Result<Vfs> {
    let abs = fs::canonicalize(root)?;
    let parts = abs
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    let mut v = Vfs::new(parts);
    walk(&abs, "", &mut v)?;
    Ok(v)
}

fn walk(dir: &Path, rel: &str, v: &mut Vfs) -> io::Result<()> {
    for e in fs::read_dir(dir)? {
        let e = e?;
        let name = e.file_name().to_string_lossy().into_owned();
        if rel.is_empty() && name == UNDO_DIR {
            continue;
        }
        let r = if rel.is_empty() {
            name.clone()
        } else {
            format!("{rel}/{name}")
        };
        let ft = e.file_type()?;
        if ft.is_dir() {
            v.add_dir(&r);
            walk(&e.path(), &r, v)?;
        } else if let Ok(m) = fs::metadata(e.path())
            && m.is_file()
        {
            let size = m.len();
            let data = match want(&r, size) {
                Want::Full => Data::Full(fs::read(e.path())?),
                Want::Header => Data::Header(read_header(&e.path())?),
                Want::Nothing => Data::Opaque,
            };
            v.add_file(&r, size, data);
        }
    }
    Ok(())
}

fn read_header(p: &Path) -> io::Result<Vec<u8>> {
    let f = BufReader::new(fs::File::open(p)?);
    if p.to_string_lossy().ends_with(".gz") {
        return Ok(nifti::gunzip_prefix(f, nifti::HEADER_BYTES));
    }
    let mut b = Vec::new();
    f.take(nifti::HEADER_BYTES as u64).read_to_end(&mut b)?;
    Ok(b)
}

/// Undo record for one applied op.
enum Undo {
    Created(PathBuf),
    Dir(PathBuf),
    Renamed(PathBuf, PathBuf),
    Backed(PathBuf, PathBuf), // original, backup
    RemovedDir(PathBuf),
}

/// Recovery directory under the output root.
pub const UNDO_DIR: &str = ".reproinx-undo";

/// A recovery directory left by an interrupted or failed run, if any. Hosts
/// must refuse to start (before converting) while it exists.
pub fn pending_recovery(root: &Path) -> Option<PathBuf> {
    let d = root.join(UNDO_DIR);
    fs::symlink_metadata(&d).is_ok().then_some(d)
}

/// Applies ops in order. Overwritten and deleted files are first moved into
/// `<root>/.reproinx-undo/` so a failure restores the tree; the backups are
/// removed only after a successful apply or a complete rollback. Otherwise the
/// directory and its `journal.txt` (`backup<TAB>original` lines) remain for
/// manual recovery.
pub struct Applier {
    root: PathBuf,
    undo_dir: PathBuf,
    journal: Vec<Undo>,
    n: usize,
}

impl Applier {
    pub fn new(root: &Path) -> Self {
        Applier {
            root: root.to_path_buf(),
            undo_dir: root.join(UNDO_DIR),
            journal: Vec::new(),
            n: 0,
        }
    }

    fn p(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    /// Public-API boundary: op paths must stay inside the root, and files
    /// the op modifies must not be symlinks (README D5).
    fn check(&self, op: &Op) -> io::Result<()> {
        let (paths, modifies): (Vec<&str>, bool) = match op {
            Op::Mkdir { path } | Op::RemoveDir { path } => (vec![path], false),
            Op::Write { path, .. } | Op::Delete { path } | Op::DropVolumes { path, .. } => {
                (vec![path], true)
            }
            Op::Rename { from, to } => (vec![from, to], true),
        };
        for p in paths {
            let unsafe_path = p.is_empty()
                || Path::new(p).is_absolute()
                || p.split('/').any(|c| {
                    c.is_empty()
                        || c == "."
                        || c == ".."
                        || (cfg!(windows) && (c.contains('\\') || c.contains(':')))
                });
            if unsafe_path {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unsafe op path: {p}"),
                ));
            }
            if modifies && fs::symlink_metadata(self.p(p)).is_ok_and(|m| m.file_type().is_symlink())
            {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!("refusing to modify symlink {p} (git-annex/datalad: unlock it first)"),
                ));
            }
        }
        Ok(())
    }

    /// Move an existing file aside before it is overwritten or deleted.
    fn backup(&mut self, path: &Path) -> io::Result<()> {
        if fs::symlink_metadata(path).is_err() {
            return Ok(());
        }
        fs::create_dir_all(&self.undo_dir)?;
        self.n += 1;
        let b = self.undo_dir.join(self.n.to_string());
        fs::rename(path, &b)?;
        // Record the move before fallible journal I/O so rollback can restore it.
        self.journal
            .push(Undo::Backed(path.to_path_buf(), b.clone()));
        let line = format!("{}\t{}\n", b.display(), path.display());
        use io::Write;
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.undo_dir.join("journal.txt"))?
            .write_all(line.as_bytes())
    }

    pub fn apply(&mut self, op: &Op) -> io::Result<()> {
        self.check(op)?;
        match op {
            Op::Mkdir { path } => {
                fs::create_dir(self.p(path))?;
                self.journal.push(Undo::Dir(self.p(path)));
            }
            Op::Rename { from, to } => {
                let (f, t) = (self.p(from), self.p(to));
                // Back up any non-directory occupant (file, fifo, socket).
                if fs::symlink_metadata(&t).is_ok_and(|m| !m.is_dir()) {
                    self.backup(&t)?;
                }
                fs::rename(&f, &t)?;
                self.journal.push(Undo::Renamed(f, t));
            }
            Op::Write { path, bytes } => {
                let p = self.p(path);
                self.backup(&p)?;
                // Registered first: a write that fails part-way leaves a file.
                self.journal.push(Undo::Created(p.clone()));
                fs::write(&p, bytes)?;
            }
            Op::Delete { path } => self.backup(&self.p(path))?,
            Op::RemoveDir { path } => {
                fs::remove_dir(self.p(path))?;
                self.journal.push(Undo::RemovedDir(self.p(path)));
            }
            Op::DropVolumes { path, indices } => {
                let p = self.p(path);
                // create_new: never follow or truncate a planted/leftover temp.
                let (tmp, file) = (0..100)
                    .find_map(|i| {
                        let t = p.with_file_name(format!(
                            "{}.reproinx-tmp-{}-{i}",
                            p.file_name().unwrap().to_string_lossy(),
                            std::process::id()
                        ));
                        fs::File::create_new(&t).ok().map(|f| (t, f))
                    })
                    .ok_or_else(|| io::Error::other(format!("{path}: no free temporary name")))?;
                let ok = {
                    let src = BufReader::new(fs::File::open(&p)?);
                    let dst = BufWriter::new(file);
                    nifti::drop_volumes_file(path, src, dst, indices)
                };
                if let Err(e) = ok {
                    let _ = fs::remove_file(&tmp);
                    return Err(io::Error::new(e.kind(), format!("{path}: {e}")));
                }
                self.backup(&p)?;
                fs::rename(&tmp, &p)?;
                self.journal.push(Undo::Created(p));
            }
        }
        Ok(())
    }

    /// Undo every applied op (reverse order). Backups are kept if any
    /// restoration fails.
    pub fn rollback(&mut self) -> io::Result<()> {
        let mut first_err = None;
        while let Some(u) = self.journal.pop() {
            let r = match u {
                Undo::Created(p) => match fs::remove_file(p) {
                    Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
                    r => r,
                },
                Undo::Dir(p) => fs::remove_dir(p),
                Undo::Renamed(f, t) => fs::rename(t, f),
                Undo::Backed(orig, b) => fs::rename(b, orig),
                Undo::RemovedDir(p) => fs::create_dir(p),
            };
            if let Err(e) = r {
                first_err.get_or_insert(e);
            }
        }
        match first_err {
            None => self.finish(),
            Some(e) => Err(io::Error::new(
                e.kind(),
                format!("{e}; backups kept in {}", self.undo_dir.display()),
            )),
        }
    }

    /// Discard before-images after a successful apply.
    pub fn finish(&mut self) -> io::Result<()> {
        self.journal.clear();
        if self.undo_dir.exists() {
            fs::remove_dir_all(&self.undo_dir)
        } else {
            Ok(())
        }
    }
}

/// Apply all ops; on failure roll back and return the error.
pub fn apply_all(root: &Path, ops: &[Op]) -> io::Result<()> {
    if let Some(d) = pending_recovery(root) {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "{} exists from an earlier run; recover or remove it first",
                d.display()
            ),
        ));
    }
    let mut a = Applier::new(root);
    for op in ops {
        if let Err(e) = a.apply(op) {
            let rb = a.rollback();
            return Err(match rb {
                Ok(()) => io::Error::new(e.kind(), format!("{e} (changes rolled back)")),
                Err(r) => io::Error::new(e.kind(), format!("{e}; rollback also failed: {r}")),
            });
        }
    }
    a.finish()
}
