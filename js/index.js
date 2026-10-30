// reproin-rs browser/Node host: inventory dcm2niix output, plan with the
// WASM core, apply ops to an in-memory tree. No conversion rules live here.
import init, { Planner, VolumeFilter, convertArgs, want } from './pkg/reproin_wasm.js'

let ready
/** Load the WASM core once. `source`: optional URL/bytes for the .wasm. */
export function load(source) {
  ready ??= init(source === undefined ? undefined : { module_or_path: source })
  return ready
}

export { convertArgs }

const HEADER_BYTES = 348

async function readPrefix(blob, gz, n) {
  let stream = blob.stream()
  if (gz) stream = stream.pipeThrough(new DecompressionStream('gzip'))
  const reader = stream.getReader()
  const out = new Uint8Array(n)
  let got = 0
  try {
    while (got < n) {
      const { done, value } = await reader.read()
      if (done) break
      const take = Math.min(value.length, n - got)
      out.set(value.subarray(0, take), got)
      got += take
    }
  } catch {
    // Corrupt gzip: hand the core what was read; it reports a non-NIfTI.
  } finally {
    reader.cancel().catch(() => {})
  }
  return out.subarray(0, got)
}

/**
 * Plan the post-pass over `tree` (Map of root-relative path -> Blob).
 * `dirs`: optional extra (empty) directory paths.
 * Returns {ops, log, failures, roots, fatal}.
 */
export async function plan(tree, options = {}, dirs = []) {
  await load()
  const p = new Planner(options.rootName ?? 'output')
  try {
    for (const d of dirs) p.addDir(d)
    for (const [path, blob] of tree) {
      const w = want(path, blob.size)
      let bytes
      if (w === 1) bytes = new Uint8Array(await blob.arrayBuffer())
      else if (w === 2) bytes = await readPrefix(blob, path.endsWith('.gz'), HEADER_BYTES)
      p.addFile(path, blob.size, bytes)
    }
    return JSON.parse(p.plan(JSON.stringify(options)))
  } finally {
    p.free()
  }
}

async function dropVolumes(blob, path, indices) {
  const gz = path.endsWith('.gz')
  const filter = new VolumeFilter(Uint32Array.from(indices))
  try {
    let src = blob.stream()
    if (gz) src = src.pipeThrough(new DecompressionStream('gzip'))
    let out = src.pipeThrough(
      new TransformStream({
        transform(chunk, c) {
          const o = filter.push(chunk)
          if (o.length) c.enqueue(o)
        },
        flush() {
          filter.finish()
        },
      }),
    )
    if (gz) out = out.pipeThrough(new CompressionStream('gzip'))
    return await new Response(out).blob()
  } finally {
    filter.free()
  }
}

function ancestors(tree, extra = []) {
  const dirs = new Set(extra)
  for (const k of tree.keys()) {
    for (let i = k.indexOf('/'); i > 0; i = k.indexOf('/', i + 1)) dirs.add(k.slice(0, i))
  }
  return dirs
}

/**
 * Apply planned ops to a copy of `tree`; the input map is untouched, so a
 * failure (thrown) leaves the caller's files as they were. Directories
 * (`dirs`, plus every file's ancestors) are tracked like a filesystem: they
 * persist until an `rmdir` op, so empty ones survive.
 */
export async function apply(tree, ops, { onProgress, signal, dirs: extraDirs = [] } = {}) {
  const out = new Map(tree)
  const dirs = ancestors(tree, extraDirs)
  for (let i = 0; i < ops.length; i++) {
    signal?.throwIfAborted()
    const op = ops[i]
    switch (op.op) {
      case 'mkdir':
        dirs.add(op.path)
        break
      case 'rmdir':
        dirs.delete(op.path)
        break
      case 'write':
        out.set(op.path, new Blob([op.text]))
        break
      case 'delete':
        if (!out.delete(op.path)) throw new Error(`delete: no such file ${op.path}`)
        break
      case 'rename': {
        if (out.has(op.from)) {
          out.set(op.to, out.get(op.from))
          out.delete(op.from)
        } else {
          // Directory move: every file below `from/`.
          const pre = `${op.from}/`
          const moved = [...out.keys()].filter((k) => k.startsWith(pre))
          if (!moved.length && !dirs.has(op.from)) throw new Error(`rename: no such path ${op.from}`)
          for (const k of moved) {
            out.set(`${op.to}/${k.slice(pre.length)}`, out.get(k))
            out.delete(k)
          }
          for (const d of [...dirs]) {
            if (d === op.from || d.startsWith(pre)) {
              dirs.delete(d)
              dirs.add(op.to + d.slice(op.from.length))
            }
          }
        }
        break
      }
      case 'dropVolumes':
        out.set(op.path, await dropVolumes(out.get(op.path), op.path, op.indices))
        break
      default:
        throw new Error(`unknown op ${op.op}`)
    }
    onProgress?.((i + 1) / ops.length)
  }
  return { files: out, dirs }
}

/** Post-pass an in-memory dcm2niix output tree. */
export async function postPass(tree, options = {}) {
  const p = await plan(tree, options, options.dirs)
  const { files, dirs } = await apply(tree, p.ops, options)
  return { ...p, files, dirs }
}

/**
 * DICOM files -> BIDS tree, entirely on-device.
 * options: {subject, session, anonymize, strict, keepDerivatives, minVolumes,
 * shiftDates, signal, onLog, onProgress, convert}. `convert` defaults to the
 * bundled published @niivue/dcm2niix (./dcm2niix, see tools/vendor-dcm2niix.sh),
 * whose worker keeps `.reproin_provenance.tsv`.
 */
export async function convertToBids(dicomFiles, options = {}) {
  await load()
  const convert = options.convert ?? (await import('./dcm2niix/index.jpeg.js')).convert
  const args = convertArgs(options.subject || undefined, options.session || undefined, !!options.anonymize)
  const res = await convert(dicomFiles, { args, signal: options.signal, onLog: options.onLog })
  if (res.exitCode !== 0 && res.exitCode !== 8 && res.exitCode !== 10) {
    throw new Error(`dcm2niix exited ${res.exitCode}: ${res.log.slice(-3).join(' ')}`)
  }
  const tree = new Map(res.files.map((f) => [f.relativePath, f]))
  if (![...tree.keys()].some((k) => k.endsWith('.reproin_provenance.tsv'))) {
    throw new Error('dcm2niix output lacks .reproin_provenance.tsv (a custom `convert` must keep dotfiles)')
  }
  const out = await postPass(tree, options)
  return { ...out, exitCode: res.exitCode, dcm2niixLog: res.log }
}

const CRC = new Uint32Array(256).map((_, n) => {
  let c = n
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1
  return c >>> 0
})

async function crc32(blob) {
  let c = 0xffffffff
  const r = blob.stream().getReader()
  for (;;) {
    const { done, value } = await r.read()
    if (done) break
    for (let i = 0; i < value.length; i++) c = CRC[(c ^ value[i]) & 0xff] ^ (c >>> 8)
  }
  return (c ^ 0xffffffff) >>> 0
}

/**
 * Store-only ZIP of a tree (NIfTI is already gzip-compressed). Files are
 * referenced, not copied, so peak memory stays near the inputs. Zip64 is not
 * written: entries and the archive must stay under 4 GiB.
 */
export async function zip(tree, prefix = '') {
  if (tree.size > 0xffff) throw new Error('zip: more than 65535 files (Zip64 not written)')
  const enc = new TextEncoder()
  const parts = []
  const central = []
  let offset = 0
  for (const [path, blob] of [...tree].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))) {
    const name = enc.encode(prefix + path)
    if (name.length > 0xffff) throw new Error(`zip: name too long: ${path}`)
    const crc = await crc32(blob)
    const size = blob.size
    if (size >= 0xffffffff || offset >= 0xffffffff) throw new Error('zip: archive exceeds 4 GiB')
    const head = new DataView(new ArrayBuffer(30))
    head.setUint32(0, 0x04034b50, true)
    head.setUint16(4, 20, true)
    head.setUint16(6, 0x0800, true) // UTF-8 names
    head.setUint32(14, crc, true)
    head.setUint32(18, size, true)
    head.setUint32(22, size, true)
    head.setUint16(26, name.length, true)
    parts.push(head, name, blob)
    const cd = new DataView(new ArrayBuffer(46))
    cd.setUint32(0, 0x02014b50, true)
    cd.setUint16(4, 20, true)
    cd.setUint16(6, 20, true)
    cd.setUint16(8, 0x0800, true)
    cd.setUint32(16, crc, true)
    cd.setUint32(20, size, true)
    cd.setUint32(24, size, true)
    cd.setUint16(28, name.length, true)
    cd.setUint32(42, offset, true)
    central.push(cd, name)
    offset += 30 + name.length + size
  }
  const cdSize = central.reduce((n, p) => n + p.byteLength, 0)
  if (offset + cdSize >= 0xffffffff) throw new Error('zip: archive exceeds 4 GiB')
  const end = new DataView(new ArrayBuffer(22))
  end.setUint32(0, 0x06054b50, true)
  end.setUint16(8, tree.size, true)
  end.setUint16(10, tree.size, true)
  end.setUint32(12, cdSize, true)
  end.setUint32(16, offset, true)
  return new Blob([...parts, ...central, end], { type: 'application/zip' })
}
