// src/worker.jpeg.js
import Module from "./dcm2niix.jpeg.js";

// src/worker-core.js
var IN_DIR = "/input";
var OUT_DIR = "/output";
var mimeType = (name) => {
  if (/\.json$/i.test(name)) return "application/json";
  if (/\.gz$/i.test(name)) return "application/gzip";
  if (/\.(bval|bvec|txt|tsv)$/i.test(name)) return "text/plain";
  return "application/octet-stream";
};
var MAX_CACHED_FILE = 64 * 1024 * 1024;
var READAHEAD_FILE = 8 * 1024 * 1024;
var READAHEAD_BYTES = 16 * 1024 * 1024;
var CACHE_BYTES = 256 * 1024 * 1024;
var cachingReads = (FS) => {
  const cache = /* @__PURE__ */ new Map();
  let cached = 0;
  let nextInOrder = null;
  const contents = (node) => {
    const hit = cache.get(node);
    if (hit) {
      cache.delete(node);
      cache.set(node, hit);
      nextInOrder = node.nextFile;
      return hit;
    }
    const batch = [node];
    let size = node.size;
    if (node === nextInOrder)
      for (let n = node.nextFile; n && !cache.has(n) && n.size <= READAHEAD_FILE && size + n.size <= READAHEAD_BYTES; n = n.nextFile) {
        batch.push(n);
        size += n.size;
      }
    const read = () => new Uint8Array(new FileReaderSync().readAsArrayBuffer(new Blob(batch.map((n) => n.contents))));
    let data;
    try {
      data = read();
    } catch (err) {
      if (batch.length === 1) throw err;
      batch.length = 1;
      data = read();
    }
    let offset = 0;
    for (const n of batch) {
      cache.set(n, batch.length === 1 ? data : data.slice(offset, offset + n.size));
      offset += n.size;
      cached += n.size;
    }
    for (const [n, bytes] of cache) {
      if (cached <= CACHE_BYTES || n === node) break;
      cache.delete(n);
      cached -= bytes.byteLength;
    }
    nextInOrder = batch[batch.length - 1].nextFile;
    return cache.get(node);
  };
  const ops = FS.filesystems.WORKERFS.stream_ops;
  const uncached = ops.read;
  ops.read = (stream, buffer, offset, length, position) => {
    const node = stream.node;
    if (node.size > MAX_CACHED_FILE) return uncached(stream, buffer, offset, length, position);
    if (position >= node.size) return 0;
    const bytes = contents(node).subarray(position, position + length);
    buffer.set(bytes, offset);
    return bytes.length;
  };
  return (nodes) => {
    cache.clear();
    cached = 0;
    nextInOrder = null;
    nodes.forEach((node, i) => node.nextFile = nodes[i + 1] || null);
  };
};
var safeRelativePath = (path, fallback) => {
  const parts = String(path || "").split("/").filter((p) => p && p !== "." && p !== "..");
  return parts.length ? parts.join("/") : fallback;
};
function serve(Module2) {
  let mod = null;
  let log = [];
  const emit = (stream) => (text) => {
    log.push(text);
    self.postMessage({ type: "log", stream, text });
  };
  const fail = (message) => self.postMessage({ type: "error", message });
  let resetCache = null;
  Module2({ print: emit("stdout"), printErr: emit("stderr") }).then(
    (m) => {
      mod = m;
      resetCache = cachingReads(m.FS);
      self.postMessage({ type: "ready" });
    },
    (err) => fail(`dcm2niix WASM failed to load: ${err && err.message || err}`)
  );
  self.onerror = (message, source, line, col, error) => fail(String(message || "dcm2niix worker error"));
  self.onunhandledrejection = (event) => fail(event.reason && event.reason.message || "dcm2niix worker: unhandled rejection");
  const remove = (path) => {
    const FS = mod.FS;
    if (!FS.analyzePath(path).exists) return;
    if (FS.isDir(FS.stat(path).mode)) {
      for (const name of FS.readdir(path)) if (name !== "." && name !== "..") remove(`${path}/${name}`);
      FS.rmdir(path);
    } else FS.unlink(path);
  };
  const filesInWalkOrder = (dir, out = []) => {
    for (const name of mod.FS.readdir(dir)) {
      if (name === "." || name === "..") continue;
      const { node } = mod.FS.lookupPath(`${dir}/${name}`);
      if (mod.FS.isDir(node.mode)) filesInWalkOrder(`${dir}/${name}`, out);
      else out.push(node);
    }
    return out;
  };
  const unmountInput = () => {
    resetCache([]);
    const node = mod.FS.analyzePath(IN_DIR);
    if (node.exists && node.object.mount && node.object.mount.mountpoint === IN_DIR) mod.FS.unmount(IN_DIR);
    remove(IN_DIR);
  };
  const stageInput = (fileList) => {
    const FS = mod.FS;
    const names = fileList.map((item, i) => safeRelativePath(item.webkitRelativePath, item.file.name || `file${i}`));
    const dirs = new Set(names.flatMap((n) => n.split("/").slice(0, -1).map((_, k, parts) => parts.slice(0, k + 1).join("/"))));
    const seen = /* @__PURE__ */ new Set();
    const blobs = names.map((n, i) => {
      let name = n;
      for (let k = i; seen.has(name) || dirs.has(name); k++) name = `${n}_${k}`;
      seen.add(name);
      return { name, data: fileList[i].file };
    });
    FS.mkdir(IN_DIR);
    FS.mount(FS.filesystems.WORKERFS, { blobs }, IN_DIR);
    resetCache(filesInWalkOrder(IN_DIR));
    const top = blobs.length ? blobs[0].name.split("/")[0] : "";
    return blobs.length && blobs.every((b) => b.name.startsWith(`${top}/`)) ? `${IN_DIR}/${top}` : IN_DIR;
  };
  const collectOutput = (dir, rel, out) => {
    const FS = mod.FS;
    for (const name of FS.readdir(dir)) {
      if (name.startsWith(".") && name !== ".reproin_provenance.tsv") continue; // reproin-rs: keep -f %H provenance
      const path = `${dir}/${name}`;
      const relPath = rel ? `${rel}/${name}` : name;
      const { node } = FS.lookupPath(path);
      if (FS.isDir(node.mode)) collectOutput(path, relPath, out);
      else {
        const bytes = FS.filesystems.MEMFS.getFileDataAsTypedArray(node);
        out.push({ path: relPath, file: new File([bytes], name, { type: mimeType(name) }) });
        FS.unlink(path);
      }
    }
    return out;
  };
  self.addEventListener("message", (e) => {
    try {
      const { fileList, cmd } = e.data;
      if (!mod) throw new Error("dcm2niix WASM module not loaded yet");
      const inputPath = stageInput(fileList);
      mod.FS.mkdir(OUT_DIR);
      log = [];
      let exitCode = 0;
      try {
        exitCode = mod.callMain(["-d", "9", ...cmd, "-o", OUT_DIR, inputPath]);
      } catch (err) {
        if (err && typeof err === "object" && "status" in err) exitCode = err.status;
        else throw err;
      }
      const outputs = collectOutput(OUT_DIR, "", []);
      self.postMessage({ type: "result", convertedFiles: outputs.map((o) => o.file), paths: outputs.map((o) => o.path), exitCode, log });
    } catch (err) {
      fail(err && err.message || String(err) || "dcm2niix worker error");
    } finally {
      if (mod) {
        unmountInput();
        remove(OUT_DIR);
      }
    }
  });
}

// src/worker.jpeg.js
serve(Module);
