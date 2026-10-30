//! reproin-core: plans the ReproIn post-pass over a dcm2niix `-f %H` output
//! tree (port of dcm2niix `tools/reproinx.py` @ 428fabe9). No filesystem or
//! process access: hosts supply an inventory and replay the returned ops.

pub mod json;
pub mod nifti;
mod passes;
pub mod tables;
pub mod vfs;

pub use vfs::{Data, Op, Vfs};

/// Post-pass options (reproinx.py flags).
#[derive(Debug, Clone)]
pub struct Options {
    pub strict: bool,
    pub keep_derivatives: bool,
    pub min_volumes: i64,
    pub shift_dates: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            strict: false,
            keep_derivatives: false,
            min_volumes: tables::DEFAULT_MIN_VOLUMES,
            shift_dates: false,
        }
    }
}

/// Conversion options shared by every host.
#[derive(Debug, Clone, Default)]
pub struct Convert {
    pub subject: Option<String>,
    pub session: Option<String>,
    pub anonymize: bool,
}

/// dcm2niix arguments after the executable (`_run_dcm2niix`).
pub fn dcm2niix_args(c: &Convert, outdir: &str, indir: &str) -> Vec<String> {
    let mut a: Vec<String> = [
        "-f",
        "%H",
        "-z",
        "y",
        "-o",
        outdir,
        "-ba",
        if c.anonymize { "y" } else { "o" },
    ]
    .map(String::from)
    .into();
    if let Some(s) = c.subject.as_deref().filter(|s| !s.is_empty()) {
        a.extend(["-bi".into(), s.into()]);
    }
    if let Some(s) = c.session.as_deref().filter(|s| !s.is_empty()) {
        a.extend(["-bv".into(), s.into()]);
    }
    a.push(indir.into());
    a
}

/// dcm2niix exit codes that mean partial success (post-pass still runs).
pub const DCM2NIIX_PARTIAL_OK: [i32; 2] = [8, 10];

/// What a host must read for one file of the output tree.
#[derive(Debug, PartialEq, Eq)]
pub enum Want {
    /// Whole file (sidecars, TSVs, small text).
    Full,
    /// First `nifti::HEADER_BYTES` bytes, decompressed for `.nii.gz`.
    Header,
    /// Name and size only.
    Nothing,
}

pub fn want(path: &str, size: u64) -> Want {
    let n = vfs::name(path);
    if n.ends_with(".nii") || n.ends_with(".nii.gz") {
        return Want::Header;
    }
    let text =
        n.ends_with(".json") || n.ends_with(".tsv") || n.ends_with(".md") || n == ".bidsignore";
    if text && size <= vfs::JSON_MAX_BYTES {
        Want::Full
    } else {
        Want::Nothing
    }
}

/// Outcome of planning.
#[derive(Debug, Default)]
pub struct Report {
    /// The reference's stderr progress/warning lines, in order.
    pub log: Vec<String>,
    /// Best-effort per-scope failures (the reference logged and continued).
    pub failures: Vec<String>,
    /// BIDS roots (relative) that were scaffolded.
    pub roots: Vec<String>,
    /// Set when the run aborted: `--strict` failure or a `--shift-dates`
    /// failure. Ops before the abort are still in the plan, as the reference
    /// leaves them on disk; a host may roll them back instead.
    pub fatal: Option<String>,
}

pub struct Plan {
    pub ops: Vec<Op>,
    pub report: Report,
}

/// Run every pass over the inventoried tree.
pub fn plan(mut tree: Vfs, opts: &Options) -> Plan {
    let mut report = Report::default();
    let mut cx = passes::Cx {
        fs: &mut tree,
        opts,
        report: &mut report,
    };
    if let Err(e) = passes::post_process(&mut cx) {
        report.fatal = Some(e.to_string());
    }
    Plan {
        ops: std::mem::take(&mut tree.ops),
        report,
    }
}
