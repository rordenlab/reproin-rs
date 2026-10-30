//! In-memory output tree. Passes mutate it with filesystem-like calls (ported
//! from reproinx.py's pathlib usage); every mutation is journaled as an `Op`
//! that a host replays on the real tree.

use std::collections::{BTreeSet, HashMap};

use serde_json::Value;

use crate::{json, nifti};

/// Python's `_JSON_MAX_BYTES`.
pub const JSON_MAX_BYTES: u64 = 50 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Mkdir {
        path: String,
    },
    /// `os.replace` semantics: an existing destination file is overwritten.
    Rename {
        from: String,
        to: String,
    },
    Write {
        path: String,
        bytes: Vec<u8>,
    },
    Delete {
        path: String,
    },
    RemoveDir {
        path: String,
    },
    /// Remove 0-based volumes from a 4D NIfTI-1 (see `nifti::drop_volumes`).
    DropVolumes {
        path: String,
        indices: Vec<usize>,
    },
}

#[derive(Debug, Clone)]
pub enum Data {
    Full(Vec<u8>),
    /// Leading decompressed bytes of a NIfTI image.
    Header(Vec<u8>),
    Opaque,
}

#[derive(Debug, Clone)]
struct File {
    size: u64,
    data: Data,
}

#[derive(Debug)]
pub enum Error {
    /// Python `FileExistsError`.
    Exists(String),
    /// Any other `OSError` / `ValueError` the reference would raise.
    Other(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::Exists(p) => write!(f, "{p} already exists"),
            Error::Other(m) => f.write_str(m),
        }
    }
}

pub type Res<T> = Result<T, Error>;

pub fn err<T>(msg: impl Into<String>) -> Res<T> {
    Err(Error::Other(msg.into()))
}

/// Root-relative, `/`-separated paths; `""` is the output root.
pub struct Vfs {
    files: HashMap<String, File>,
    dirs: HashMap<String, BTreeSet<String>>,
    root_parts: Vec<String>,
    pub ops: Vec<Op>,
}

pub fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else if name.is_empty() {
        dir.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

pub fn parent(p: &str) -> &str {
    p.rfind('/').map_or("", |i| &p[..i])
}

pub fn name(p: &str) -> &str {
    p.rfind('/').map_or(p, |i| &p[i + 1..])
}

/// Python `Path.stem` (one suffix removed).
pub fn stem(p: &str) -> &str {
    let n = name(p);
    match n.rfind('.') {
        Some(i) if i > 0 => &n[..i],
        _ => n,
    }
}

/// Python `Path.suffix`.
pub fn suffix(p: &str) -> &str {
    let n = name(p);
    match n.rfind('.') {
        Some(i) if i > 0 && i + 1 < n.len() => &n[i..],
        _ => "",
    }
}

/// Python `Path` ordering (component-wise).
pub fn path_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    a.split('/').cmp(b.split('/'))
}

pub fn sort_paths(v: &mut [String]) {
    v.sort_by(|a, b| path_cmp(a, b));
}

/// `p` relative to `base` (`base` must be an ancestor or equal).
pub fn relative<'a>(p: &'a str, base: &str) -> Option<&'a str> {
    if base.is_empty() {
        return Some(p);
    }
    if p == base {
        return Some("");
    }
    p.strip_prefix(base)?.strip_prefix('/')
}

/// POSIX `fnmatch.fnmatch` (case-sensitive; `*` also matches `/`).
pub fn fnmatch(s: &str, pat: &str) -> bool {
    let s: Vec<char> = s.chars().collect();
    let p: Vec<char> = pat.chars().collect();
    fn class(p: &[char], mut i: usize, c: char) -> Option<(bool, usize)> {
        // p[i] == '['; returns (matched, index after ']') or None if unterminated.
        i += 1;
        let negate = i < p.len() && p[i] == '!';
        if negate {
            i += 1;
        }
        let start = i;
        let mut hit = false;
        while i < p.len() && (p[i] != ']' || i == start) {
            if i + 2 < p.len() && p[i + 1] == '-' && p[i + 2] != ']' {
                if p[i] <= c && c <= p[i + 2] {
                    hit = true;
                }
                i += 3;
            } else {
                if p[i] == c {
                    hit = true;
                }
                i += 1;
            }
        }
        (i < p.len()).then_some((hit != negate, i + 1))
    }
    fn go(s: &[char], p: &[char]) -> bool {
        let (mut si, mut pi) = (0, 0);
        let mut star: Option<(usize, usize)> = None;
        while si < s.len() {
            let step = if pi < p.len() {
                match p[pi] {
                    '*' => {
                        star = Some((pi, si));
                        pi += 1;
                        continue;
                    }
                    '?' => Some(pi + 1),
                    '[' => match class(p, pi, s[si]) {
                        Some((true, next)) => Some(next),
                        Some((false, _)) => None,
                        None => (s[si] == '[').then_some(pi + 1),
                    },
                    c => (c == s[si]).then_some(pi + 1),
                }
            } else {
                None
            };
            match step {
                Some(next) => {
                    pi = next;
                    si += 1;
                }
                None => match star {
                    Some((sp, ss)) => {
                        pi = sp + 1;
                        si = ss + 1;
                        star = Some((sp, ss + 1));
                    }
                    None => return false,
                },
            }
        }
        p[pi..].iter().all(|&c| c == '*')
    }
    go(&s, &p)
}

/// Paths the planner may create: no empty, `.` or `..` components, so a
/// hostile provenance/BidsGuess string can never escape the output root.
fn creatable(p: &str) -> Res<()> {
    if p.split('/').any(|c| c.is_empty() || c == "." || c == "..") {
        return err(format!("{p}: unsafe path"));
    }
    Ok(())
}

impl Vfs {
    /// `root_parts`: components of the absolute output root (reference checks
    /// such as "any part == derivatives" look at the absolute path).
    pub fn new(root_parts: Vec<String>) -> Self {
        let mut dirs = HashMap::new();
        dirs.insert(String::new(), BTreeSet::new());
        Vfs {
            files: HashMap::new(),
            dirs,
            root_parts,
            ops: Vec::new(),
        }
    }

    /// Inventory insertion (no op recorded). Parents are created implicitly.
    pub fn add_file(&mut self, path: &str, size: u64, data: Data) {
        // Hostile or malformed host paths (or a file/dir clash) are never inventoried.
        if creatable(path).is_err() || self.is_dir(path) || self.file_ancestor(path) {
            return;
        }
        self.ensure_dirs(parent(path), false);
        self.dirs
            .get_mut(parent(path))
            .unwrap()
            .insert(name(path).to_string());
        self.files.insert(path.to_string(), File { size, data });
    }

    pub fn add_dir(&mut self, path: &str) {
        if creatable(path).is_err() || self.is_file(path) || self.file_ancestor(path) {
            return;
        }
        self.ensure_dirs(path, false);
    }

    fn file_ancestor(&self, p: &str) -> bool {
        let mut a = parent(p);
        while !a.is_empty() {
            if self.is_file(a) {
                return true;
            }
            a = parent(a);
        }
        false
    }

    fn ensure_dirs(&mut self, path: &str, record: bool) {
        if self.dirs.contains_key(path) {
            return;
        }
        self.ensure_dirs(parent(path), record);
        self.dirs
            .get_mut(parent(path))
            .unwrap()
            .insert(name(path).to_string());
        self.dirs.insert(path.to_string(), BTreeSet::new());
        if record {
            self.ops.push(Op::Mkdir {
                path: path.to_string(),
            });
        }
    }

    /// Absolute components of `p` (root parts + relative parts).
    pub fn parts<'a>(&'a self, p: &'a str) -> Vec<&'a str> {
        let mut v: Vec<&str> = self.root_parts.iter().map(|s| s.as_str()).collect();
        if !p.is_empty() {
            v.extend(p.split('/'));
        }
        v
    }

    pub fn has_part(&self, p: &str, part: &str) -> bool {
        self.parts(p).contains(&part)
    }

    /// Python `Path.name`, including the root's own name.
    pub fn name_of<'a>(&'a self, p: &'a str) -> &'a str {
        if p.is_empty() {
            self.root_parts.last().map_or("", |s| s)
        } else {
            name(p)
        }
    }

    pub fn is_dir(&self, p: &str) -> bool {
        self.dirs.contains_key(p)
    }

    pub fn is_file(&self, p: &str) -> bool {
        self.files.contains_key(p)
    }

    pub fn exists(&self, p: &str) -> bool {
        self.is_file(p) || self.is_dir(p)
    }

    pub fn size(&self, p: &str) -> Option<u64> {
        self.files.get(p).map(|f| f.size)
    }

    /// Children of a directory, sorted by name (full paths).
    pub fn iterdir(&self, dir: &str) -> Vec<String> {
        self.dirs
            .get(dir)
            .map(|c| c.iter().map(|n| join(dir, n)).collect())
            .unwrap_or_default()
    }

    /// `Path.glob` with a `/`-separated pattern of fnmatch components.
    pub fn glob(&self, dir: &str, pattern: &str) -> Vec<String> {
        let mut cur = vec![dir.to_string()];
        let comps: Vec<&str> = pattern.split('/').collect();
        for (i, comp) in comps.iter().enumerate() {
            let last = i + 1 == comps.len();
            let mut next = Vec::new();
            for d in &cur {
                for c in self.iterdir(d) {
                    if fnmatch(name(&c), comp) && (last || self.is_dir(&c)) {
                        next.push(c);
                    }
                }
            }
            cur = next;
        }
        sort_paths(&mut cur);
        cur
    }

    /// `Path.rglob`: every descendant (file or dir) whose name matches.
    pub fn rglob(&self, dir: &str, pattern: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_string()];
        while let Some(d) = stack.pop() {
            for c in self.iterdir(&d) {
                if fnmatch(name(&c), pattern) {
                    out.push(c.clone());
                }
                if self.is_dir(&c) {
                    stack.push(c);
                }
            }
        }
        sort_paths(&mut out);
        out
    }

    pub fn read(&self, p: &str) -> Res<&[u8]> {
        match self.files.get(p) {
            Some(File {
                data: Data::Full(b),
                ..
            }) => Ok(b),
            Some(_) => err(format!("{p}: contents not in inventory")),
            None => err(format!("{p}: no such file")),
        }
    }

    /// `read_text()` with universal newlines.
    pub fn read_text(&self, p: &str) -> Res<String> {
        let s =
            std::str::from_utf8(self.read(p)?).map_err(|e| Error::Other(format!("{p}: {e}")))?;
        Ok(s.replace("\r\n", "\n").replace('\r', "\n"))
    }

    /// `_load_json`.
    pub fn load_json(&self, p: &str) -> Res<Value> {
        if self
            .size(p)
            .ok_or_else(|| Error::Other(format!("{p}: no such file")))?
            > JSON_MAX_BYTES
        {
            return err(format!("{p} exceeds {JSON_MAX_BYTES} byte cap"));
        }
        let text =
            std::str::from_utf8(self.read(p)?).map_err(|e| Error::Other(format!("{p}: {e}")))?;
        serde_json::from_str(text).map_err(|e| Error::Other(format!("{p}: {e}")))
    }

    /// `_load_json` for callers that then index it as a dict without an
    /// `isinstance` check (the reference raises AttributeError otherwise).
    pub fn load_obj(&self, p: &str) -> Res<serde_json::Map<String, Value>> {
        match self.load_json(p)? {
            Value::Object(m) => Ok(m),
            _ => err(format!("{p}: JSON is not an object")),
        }
    }

    /// `_load_json` where the reference skips unreadable files but then
    /// indexes the value as a dict (non-object raises AttributeError).
    pub fn try_obj(&self, p: &str) -> Res<Option<serde_json::Map<String, Value>>> {
        match self.load_json(p) {
            Err(_) => Ok(None),
            Ok(Value::Object(m)) => Ok(Some(m)),
            Ok(_) => err(format!("{p}: JSON is not an object")),
        }
    }

    /// `_save_json`.
    pub fn save_json(&mut self, p: &str, v: &Value) -> Res<()> {
        self.write(p, json::dumps(v).into_bytes())
    }

    pub fn write(&mut self, p: &str, bytes: Vec<u8>) -> Res<()> {
        creatable(p)?;
        if !self.is_dir(parent(p)) {
            return err(format!("{}: no such directory", parent(p)));
        }
        if self.is_dir(p) {
            return err(format!("{p}: is a directory"));
        }
        self.dirs
            .get_mut(parent(p))
            .unwrap()
            .insert(name(p).to_string());
        self.files.insert(
            p.to_string(),
            File {
                size: bytes.len() as u64,
                data: Data::Full(bytes.clone()),
            },
        );
        self.ops.push(Op::Write {
            path: p.to_string(),
            bytes,
        });
        Ok(())
    }

    /// `mkdir(parents=True, exist_ok=True)`.
    pub fn mkdirs(&mut self, p: &str) -> Res<()> {
        if !p.is_empty() {
            creatable(p)?;
        }
        let mut a = p;
        while !a.is_empty() && !self.is_dir(a) {
            if self.is_file(a) {
                return err(format!("{a}: not a directory"));
            }
            a = parent(a);
        }
        self.ensure_dirs(p, true);
        Ok(())
    }

    /// `os.replace` for files; `os.rename` for a directory onto a free name.
    pub fn replace(&mut self, src: &str, dst: &str) -> Res<()> {
        if src == dst {
            return Ok(());
        }
        creatable(dst)?;
        if !self.is_dir(parent(dst)) {
            return err(format!("{}: no such directory", parent(dst)));
        }
        if let Some(f) = self.files.remove(src) {
            if self.is_dir(dst) {
                self.files.insert(src.to_string(), f);
                return err(format!("{dst}: is a directory"));
            }
            self.dirs.get_mut(parent(src)).unwrap().remove(name(src));
            self.dirs
                .get_mut(parent(dst))
                .unwrap()
                .insert(name(dst).to_string());
            self.files.insert(dst.to_string(), f);
        } else if self.is_dir(src) {
            if self.exists(dst) || relative(dst, src).is_some() {
                return err(format!("cannot move {src} to {dst}"));
            }
            let prefix = format!("{src}/");
            let moved_files: Vec<String> = self
                .files
                .keys()
                .filter(|k| k.starts_with(&prefix))
                .cloned()
                .collect();
            for k in moved_files {
                let f = self.files.remove(&k).unwrap();
                self.files
                    .insert(format!("{dst}/{}", &k[prefix.len()..]), f);
            }
            let moved_dirs: Vec<String> = self
                .dirs
                .keys()
                .filter(|k| *k == src || k.starts_with(&prefix))
                .cloned()
                .collect();
            for k in moved_dirs {
                let c = self.dirs.remove(&k).unwrap();
                self.dirs.insert(format!("{dst}{}", &k[src.len()..]), c);
            }
            self.dirs.get_mut(parent(src)).unwrap().remove(name(src));
            self.dirs
                .get_mut(parent(dst))
                .unwrap()
                .insert(name(dst).to_string());
        } else {
            return err(format!("{src}: no such file"));
        }
        self.ops.push(Op::Rename {
            from: src.to_string(),
            to: dst.to_string(),
        });
        Ok(())
    }

    pub fn unlink(&mut self, p: &str) -> Res<()> {
        if self.files.remove(p).is_none() {
            return err(format!("{p}: no such file"));
        }
        self.dirs.get_mut(parent(p)).unwrap().remove(name(p));
        self.ops.push(Op::Delete {
            path: p.to_string(),
        });
        Ok(())
    }

    pub fn rmdir(&mut self, p: &str) -> Res<()> {
        match self.dirs.get(p) {
            Some(c) if c.is_empty() && !p.is_empty() => {}
            Some(_) => return err(format!("{p}: directory not empty")),
            None => return err(format!("{p}: no such directory")),
        }
        self.dirs.remove(p);
        self.dirs.get_mut(parent(p)).unwrap().remove(name(p));
        self.ops.push(Op::RemoveDir {
            path: p.to_string(),
        });
        Ok(())
    }

    /// `shutil.rmtree`, journaled as per-file deletes then bottom-up rmdirs.
    pub fn rmtree(&mut self, p: &str) -> Res<()> {
        if !self.is_dir(p) {
            return err(format!("{p}: no such directory"));
        }
        for c in self.iterdir(p) {
            if self.is_dir(&c) {
                self.rmtree(&c)?
            } else {
                self.unlink(&c)?
            }
        }
        self.rmdir(p)
    }

    /// Leading header bytes of a NIfTI (`Header` or `Full` data).
    pub fn header(&self, p: &str) -> Option<&[u8]> {
        match &self.files.get(p)?.data {
            Data::Header(h) => Some(h),
            Data::Full(b) => Some(b),
            Data::Opaque => None,
        }
    }

    /// Plan a volume removal; updates the virtual header's dim[4].
    pub fn drop_volumes(&mut self, p: &str, indices: Vec<usize>) -> bool {
        let Some(f) = self.files.get_mut(p) else {
            return false;
        };
        let hdr = match &mut f.data {
            Data::Header(h) | Data::Full(h) => h,
            Data::Opaque => return false,
        };
        let Some(h) = nifti::Header::parse(hdr) else {
            return false;
        };
        if !h.can_drop() {
            return false;
        }
        let nt = h.dim[4] as usize;
        // An uncompressed image must hold every volume (the reference reads
        // the body and skips a short one). README D10 covers truncated .gz.
        let need = h
            .volume_bytes()
            .and_then(|v| v.checked_mul(nt))
            .and_then(|n| n.checked_add(h.vox_offset as usize));
        if !p.ends_with(".gz") && need.is_none_or(|n| f.size < n as u64) {
            return false;
        }
        let keep = (0..nt).filter(|i| !indices.contains(i)).count();
        nifti::set_dim4(hdr, h.big_endian, keep as i16);
        self.ops.push(Op::DropVolumes {
            path: p.to_string(),
            indices,
        });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnmatch_cases() {
        assert!(fnmatch("a_bold.nii.gz", "*_bold.nii*"));
        assert!(fnmatch("x/y.json", "*.json"));
        assert!(!fnmatch("a.json", "*.nii"));
        assert!(fnmatch("sub-01", "sub-*"));
        assert!(fnmatch("ab", "a[a-c]"));
        assert!(!fnmatch("ad", "a[a-c]"));
        assert!(fnmatch("ad", "a[!a-c]"));
        assert!(fnmatch("a[", "a["));
        assert!(fnmatch("", "*"));
    }

    #[test]
    fn rename_dir_and_journal() {
        let mut v = Vfs::new(vec![]);
        v.add_file("a/b/c.json", 2, Data::Full(b"{}".to_vec()));
        v.mkdirs("x").unwrap();
        v.replace("a/b", "x/b").unwrap();
        assert!(v.is_file("x/b/c.json"));
        assert!(!v.is_dir("a/b"));
        assert_eq!(v.iterdir("a"), Vec::<String>::new());
        v.rmtree("x").unwrap();
        assert!(!v.exists("x"));
        assert_eq!(v.ops.len(), 5); // mkdir, rename, delete, rmdir b, rmdir x
    }

    #[test]
    fn refuses_escaping_paths() {
        let mut v = Vfs::new(vec![]);
        v.add_file("a.json", 2, Data::Full(b"{}".to_vec()));
        assert!(v.mkdirs("../x").is_err());
        assert!(v.mkdirs("x/./y").is_err());
        assert!(v.replace("a.json", "../a.json").is_err());
        assert!(v.write("x//a", vec![]).is_err());
        assert!(v.ops.is_empty());
        // Malformed inventory paths are ignored (an empty name once looped rglob).
        v.add_file("b/", 0, Data::Opaque);
        v.add_file("c//d.json", 0, Data::Opaque);
        v.add_dir("../e");
        v.add_file("a.json/x", 0, Data::Opaque); // file used as a directory
        assert_eq!(v.rglob("", "*"), vec!["a.json".to_string()]);
    }

    #[test]
    fn path_order_is_componentwise() {
        let mut v = vec!["a-c".to_string(), "a/b".to_string()];
        sort_paths(&mut v);
        assert_eq!(v, ["a/b", "a-c"]);
    }
}
