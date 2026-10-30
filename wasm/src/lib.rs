//! wasm-bindgen surface: JS inventories files, Rust plans, JS applies ops.

use reproin_core::{Data, Op, Options, Vfs, Want};
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

/// What JS must read for a file: 0 nothing, 1 whole file, 2 first 348
/// (decompressed) header bytes.
#[wasm_bindgen]
pub fn want(path: &str, size: f64) -> u8 {
    match reproin_core::want(path, size as u64) {
        Want::Nothing => 0,
        Want::Full => 1,
        Want::Header => 2,
    }
}

/// Inventory builder and planner.
#[wasm_bindgen]
pub struct Planner(Option<Vfs>);

#[wasm_bindgen]
impl Planner {
    /// `root`: name of the virtual output root (e.g. "output").
    #[wasm_bindgen(constructor)]
    pub fn new(root: &str) -> Planner {
        Planner(Some(Vfs::new(vec![root.to_string()])))
    }

    #[wasm_bindgen(js_name = addDir)]
    pub fn add_dir(&mut self, path: &str) {
        self.0.as_mut().unwrap().add_dir(path);
    }

    /// `bytes`: whole file or header per `want`, or undefined.
    #[wasm_bindgen(js_name = addFile)]
    pub fn add_file(&mut self, path: &str, size: f64, bytes: Option<Vec<u8>>) {
        let data = match (bytes, want(path, size)) {
            (Some(b), 2) => Data::Header(b),
            (Some(b), _) => Data::Full(b),
            (None, _) => Data::Opaque,
        };
        self.0.as_mut().unwrap().add_file(path, size as u64, data);
    }

    /// Plan the post-pass. `options`: {strict, keepDerivatives, minVolumes,
    /// shiftDates}. Returns JSON {ops, log, failures, roots, fatal}; write ops
    /// carry UTF-8 `text`.
    pub fn plan(&mut self, options: &str) -> Result<String, JsError> {
        let o: Value = serde_json::from_str(options)?;
        let d = Options::default();
        let opts = Options {
            strict: o["strict"].as_bool().unwrap_or(d.strict),
            keep_derivatives: o["keepDerivatives"].as_bool().unwrap_or(d.keep_derivatives),
            min_volumes: o["minVolumes"].as_i64().unwrap_or(d.min_volumes),
            shift_dates: o["shiftDates"].as_bool().unwrap_or(d.shift_dates),
        };
        let tree = self
            .0
            .take()
            .ok_or_else(|| JsError::new("plan() already called"))?;
        let p = reproin_core::plan(tree, &opts);
        let ops: Vec<Value> = p.ops.iter().map(op_json).collect();
        let r = p.report;
        Ok(json!({"ops": ops, "log": r.log, "failures": r.failures, "roots": r.roots, "fatal": r.fatal}).to_string())
    }
}

fn op_json(op: &Op) -> Value {
    match op {
        Op::Mkdir { path } => json!({"op": "mkdir", "path": path}),
        Op::Rename { from, to } => json!({"op": "rename", "from": from, "to": to}),
        Op::Write { path, bytes } => {
            json!({"op": "write", "path": path, "text": String::from_utf8_lossy(bytes)})
        }
        Op::Delete { path } => json!({"op": "delete", "path": path}),
        Op::RemoveDir { path } => json!({"op": "rmdir", "path": path}),
        Op::DropVolumes { path, indices } => {
            json!({"op": "dropVolumes", "path": path, "indices": indices})
        }
    }
}

/// dcm2niix arguments for `@niivue/dcm2niix` `convert({args})`, which sets
/// the output folder and input itself.
#[wasm_bindgen(js_name = convertArgs)]
pub fn convert_args(
    subject: Option<String>,
    session: Option<String>,
    anonymize: bool,
) -> Vec<String> {
    let c = reproin_core::Convert {
        subject,
        session,
        anonymize,
    };
    let mut a = reproin_core::dcm2niix_args(&c, "", "");
    a.pop(); // input
    let o = a.iter().position(|x| x == "-o").unwrap();
    a.drain(o..o + 2);
    a
}

/// Push filter for a `dropVolumes` op over uncompressed NIfTI bytes (pipe
/// `.nii.gz` through DecompressionStream/CompressionStream around it).
#[wasm_bindgen]
pub struct VolumeFilter(reproin_core::nifti::VolumeFilter);

#[wasm_bindgen]
impl VolumeFilter {
    #[wasm_bindgen(constructor)]
    pub fn new(indices: Vec<u32>) -> VolumeFilter {
        let v: Vec<usize> = indices.into_iter().map(|i| i as usize).collect();
        VolumeFilter(reproin_core::nifti::VolumeFilter::new(&v))
    }

    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<u8>, JsError> {
        self.0.push(chunk).map_err(|e| JsError::new(&e))
    }

    pub fn finish(&self) -> Result<(), JsError> {
        self.0.finish().map_err(|e| JsError::new(&e))
    }
}
