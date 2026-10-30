//! `reproinx`: run dcm2niix in ReproIn mode and apply second-pass BIDS fixes.
//! Drop-in replacement for dcm2niix `tools/reproinx.py` (same arguments,
//! defaults and exit codes).

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use reproin_core::{Convert, DCM2NIIX_PARTIAL_OK, Op, Options, dcm2niix_args, plan};

const USAGE: &str =
    "usage: reproinx [-h] [--version] [--no-convert] [--anonymize] [--strict] [--min-volumes N]
                [--keep-derivatives] [--shift-dates] [--dry-run]
                indir [outdir] [subject] [session]";

const HELP: &str = "Run dcm2niix in ReproIn mode and apply second-pass fixes.

positional arguments:
  indir                 DICOM input directory
  outdir                BIDS output root (default: indir)
  subject               BIDS subject (default: derived from PatientID)
  session               BIDS session (default: derived from ProtocolName)

options:
  -h, --help            show this help message and exit
  --version             show program's version number and exit
  --no-convert          Skip dcm2niix; only run the post-process pass.
  --anonymize           Use dcm2niix's BIDS anonymisation default (-ba y); AcquisitionDateTime is
                        stripped from sidecars and fmap matching falls back to first-compatible.
  --strict              Exit non-zero if any session's post-processing fails. Default is to log
                        per-session failures and continue.
  --min-volumes N, -N N Cross-series classification threshold (default 5). A timeseries with fewer
                        than N volumes that sits beside a long (>=N) EPI is treated as a PEPOLAR
                        distortion map (fmap/_dir-<L>_epi) rather than func/dwi data.
  --keep-derivatives    Keep the `derivatives/scanner/` subtree (scouts, DERIVED-flagged images,
                        physio) after post-processing.
  --shift-dates         De-identify acquisition dates: per subject, shift every acq_time /
                        AcquisitionDateTime by whole days so the earliest scan lands on 1925-01-01.
                        Does not rename timestamp-based ses-<YYYYMMDD...> labels (warns).
  --dry-run             Print the planned changes without modifying outdir. With conversion,
                        dcm2niix writes to a private temporary directory (existing outdir content
                        is then not considered).

dcm2niix is found via $DCM2NIIX, then beside this executable, then PATH.";

struct Args {
    pos: Vec<String>,
    no_convert: bool,
    anonymize: bool,
    dry_run: bool,
    opts: Options,
}

fn usage_err(msg: &str) -> ExitCode {
    eprintln!("{USAGE}\nreproinx: error: {msg}");
    ExitCode::from(2)
}

/// argparse-compatible parsing (long-option prefixes, `--opt=value`, `--`).
fn parse(argv: &[String]) -> Result<Args, ExitCode> {
    const LONG: [&str; 9] = [
        "help",
        "version",
        "no-convert",
        "anonymize",
        "strict",
        "min-volumes",
        "keep-derivatives",
        "shift-dates",
        "dry-run",
    ];
    let mut a = Args {
        pos: Vec::new(),
        no_convert: false,
        anonymize: false,
        dry_run: false,
        opts: Options::default(),
    };
    let mut it = argv.iter();
    let mut only_pos = false;
    while let Some(arg) = it.next() {
        if only_pos || arg == "-" || !arg.starts_with('-') {
            a.pos.push(arg.clone());
            continue;
        }
        if arg == "--" {
            only_pos = true;
            continue;
        }
        let (key, inline) = match arg.split_once('=') {
            Some((k, v)) if k.starts_with("--") => (k.to_string(), Some(v.to_string())),
            _ => (arg.clone(), None),
        };
        let name = if let Some(l) = key.strip_prefix("--") {
            let hits: Vec<&str> = LONG.iter().copied().filter(|o| o.starts_with(l)).collect();
            match hits.as_slice() {
                [_] if LONG.contains(&l) => l.to_string(),
                [one] => one.to_string(),
                [] => return Err(usage_err(&format!("unrecognized arguments: {arg}"))),
                _ => {
                    return Err(usage_err(&format!(
                        "ambiguous option: {key} could match {}",
                        hits.join(", ")
                    )));
                }
            }
        } else if key == "-h" {
            "help".into()
        } else if let Some(v) = key.strip_prefix("-N") {
            a.opts.min_volumes = int_arg(if v.is_empty() {
                it.next().cloned()
            } else {
                Some(v.to_string())
            })?;
            continue;
        } else {
            return Err(usage_err(&format!("unrecognized arguments: {arg}")));
        };
        match name.as_str() {
            "help" => {
                println!("{USAGE}\n\n{HELP}");
                return Err(ExitCode::SUCCESS);
            }
            "version" => {
                println!("reproinx {}", env!("CARGO_PKG_VERSION"));
                return Err(ExitCode::SUCCESS);
            }
            "min-volumes" => a.opts.min_volumes = int_arg(inline.or_else(|| it.next().cloned()))?,
            _ if inline.is_some() => {
                return Err(usage_err(&format!(
                    "argument --{name}: ignored explicit argument"
                )));
            }
            "no-convert" => a.no_convert = true,
            "anonymize" => a.anonymize = true,
            "strict" => a.opts.strict = true,
            "keep-derivatives" => a.opts.keep_derivatives = true,
            "shift-dates" => a.opts.shift_dates = true,
            "dry-run" => a.dry_run = true,
            _ => unreachable!(),
        }
    }
    if a.pos.is_empty() {
        return Err(usage_err("the following arguments are required: indir"));
    }
    if a.pos.len() > 4 {
        return Err(usage_err(&format!(
            "unrecognized arguments: {}",
            a.pos[4..].join(" ")
        )));
    }
    Ok(a)
}

fn int_arg(v: Option<String>) -> Result<i64, ExitCode> {
    let v = v.ok_or_else(|| usage_err("argument --min-volumes/-N: expected one argument"))?;
    v.trim().parse().map_err(|_| {
        usage_err(&format!(
            "argument --min-volumes/-N: invalid int value: '{v}'"
        ))
    })
}

fn is_exe(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        p.metadata()
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    p.is_file()
}

fn find_dcm2niix() -> Result<PathBuf, String> {
    let exe = if cfg!(windows) {
        "dcm2niix.exe"
    } else {
        "dcm2niix"
    };
    let mut cands = Vec::new();
    if let Some(e) = std::env::var_os("DCM2NIIX") {
        cands.push(PathBuf::from(e));
    }
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        cands.push(dir.join(exe));
    }
    if let Some(path) = std::env::var_os("PATH") {
        // Relative/empty PATH entries would run a dcm2niix from the cwd.
        cands.extend(
            std::env::split_paths(&path)
                .filter(|d| d.is_absolute())
                .map(|d| d.join(exe)),
        );
    }
    let bin = cands.into_iter().find(|p| is_exe(p)).ok_or(
        "Cannot find dcm2niix. Set $DCM2NIIX, place it beside reproinx, or put it on PATH."
            .to_string(),
    )?;
    // ReproIn one-pass naming (`-f %H`) appeared in dcm2niix v1.0.20261011.
    let help = Command::new(&bin)
        .arg("-h")
        .output()
        .map_err(|e| format!("{}: {e}", bin.display()))?;
    if !String::from_utf8_lossy(&help.stdout).contains("%H") {
        return Err(format!(
            "{} lacks ReproIn naming (-f %H); install dcm2niix v1.0.20261011 or newer",
            bin.display()
        ));
    }
    Ok(bin)
}

fn run_dcm2niix(c: &Convert, indir: &Path, outdir: &Path) -> Result<i32, String> {
    let bin = find_dcm2niix()?;
    let args = dcm2niix_args(c, &outdir.to_string_lossy(), &indir.to_string_lossy());
    eprintln!("+ {} {}", bin.display(), args.join(" "));
    let rc = Command::new(&bin)
        .args(&args)
        .status()
        .map_err(|e| e.to_string())?
        .code()
        .unwrap_or(1);
    if rc != 0 && !DCM2NIIX_PARTIAL_OK.contains(&rc) {
        return Err(format!(
            "Command '{} {}' returned non-zero exit status {rc}.",
            bin.display(),
            args.join(" ")
        ));
    }
    if rc != 0 {
        eprintln!("dcm2niix exited {rc} (partial conversion); continuing with post-pass.");
    }
    Ok(rc)
}

fn describe(op: &Op) -> String {
    match op {
        Op::Mkdir { path } => format!("mkdir   {path}"),
        Op::Rename { from, to } => format!("rename  {from} -> {to}"),
        Op::Write { path, bytes } => format!("write   {path} ({} bytes)", bytes.len()),
        Op::Delete { path } => format!("delete  {path}"),
        Op::RemoveDir { path } => format!("rmdir   {path}"),
        Op::DropVolumes { path, indices } => format!("dropvol {path} {indices:?}"),
    }
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let a = match parse(&argv) {
        Ok(a) => a,
        Err(code) => return code,
    };
    let abs = |p: &str| std::path::absolute(p).unwrap_or_else(|_| PathBuf::from(p));
    let indir = abs(&a.pos[0]);
    let outdir = a.pos.get(1).map_or_else(|| indir.clone(), |p| abs(p));
    if !indir.is_dir() {
        eprintln!("reproinx: indir does not exist: {}", indir.display());
        return ExitCode::from(2);
    }
    let conv = Convert {
        subject: a.pos.get(2).cloned(),
        session: a.pos.get(3).cloned(),
        anonymize: a.anonymize,
    };
    let mut work = outdir.clone();
    let mut _tmp = None;
    if a.dry_run && !a.no_convert {
        // Converted PHI goes to a fresh private directory, removed on exit.
        match TempDir::new() {
            Ok(t) => {
                work = t.0.clone();
                _tmp = Some(t);
            }
            Err(e) => {
                eprintln!("reproinx: cannot create a private temporary directory: {e}");
                return ExitCode::from(1);
            }
        }
    } else if a.dry_run {
        if !outdir.is_dir() {
            eprintln!("reproinx: outdir does not exist: {}", outdir.display());
            return ExitCode::from(2);
        }
    } else if let Err(e) = std::fs::create_dir_all(&work) {
        eprintln!("reproinx: cannot create {}: {e}", work.display());
        return ExitCode::from(1);
    }
    if let Some(d) = reproinx::pending_recovery(&outdir) {
        eprintln!(
            "reproinx: {} exists from an interrupted or failed run. Restore its files \
             (journal.txt lists backup<TAB>original) or remove it, then rerun.",
            d.display()
        );
        return ExitCode::from(1);
    }
    run(&a, &conv, &indir, &work)
}

/// Owner-only temporary directory with an unpredictable name.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> std::io::Result<TempDir> {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        for i in 0..100u128 {
            let n =
                seed ^ (std::process::id() as u128) << 64 ^ i.wrapping_mul(0x9e37_79b9_7f4a_7c15);
            let p = std::env::temp_dir().join(format!("reproinx-dry-run-{n:032x}"));
            let mut b = std::fs::DirBuilder::new();
            #[cfg(unix)]
            std::os::unix::fs::DirBuilderExt::mode(&mut b, 0o700);
            match b.create(&p) {
                Ok(()) => return Ok(TempDir(p)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        Err(std::io::Error::other("no free temporary directory name"))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run(a: &Args, conv: &Convert, indir: &Path, outdir: &Path) -> ExitCode {
    let mut rc = 0;
    if !a.no_convert {
        match run_dcm2niix(conv, indir, outdir) {
            Ok(r) => rc = r,
            Err(e) => {
                eprintln!("reproinx: {e}");
                return ExitCode::from(1);
            }
        }
    }
    let tree = match reproinx::inventory(outdir) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("reproinx: cannot read {}: {e}", outdir.display());
            return ExitCode::from(1);
        }
    };
    let p = plan(tree, &a.opts);
    for l in &p.report.log {
        eprintln!("{l}");
    }
    if a.dry_run {
        for op in &p.ops {
            println!("{}", describe(op));
        }
    } else if let Err(e) = reproinx::apply_all(outdir, &p.ops) {
        eprintln!("reproinx: applying changes failed: {e}");
        return ExitCode::from(1);
    }
    if let Some(f) = &p.report.fatal {
        eprintln!("reproinx: post-processing aborted: {f}");
        return ExitCode::from(1);
    }
    ExitCode::from(rc as u8)
}
