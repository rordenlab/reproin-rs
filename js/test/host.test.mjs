// The JS host must reproduce the native CLI (itself verified against the
// reproinx.py oracle) on every synthetic case and golden tree.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { execFileSync, spawnSync } from 'node:child_process'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { apply, load, plan, postPass, zip } from '../index.js'

const here = path.dirname(fileURLToPath(import.meta.url))
const repo = path.resolve(here, '../..')
const cli = path.join(repo, 'target/release/reproinx')
const compare = path.join(repo, 'tests/golden/compare.py')
await load(fs.readFileSync(path.join(repo, 'js/pkg/reproin_wasm_bg.wasm')))

function readTree(root) {
  const files = new Map()
  const dirs = []
  const walk = (d, rel) => {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const r = rel ? `${rel}/${e.name}` : e.name
      if (e.isDirectory()) {
        dirs.push(r)
        walk(path.join(d, e.name), r)
      } else if (e.isFile()) files.set(r, new Blob([fs.readFileSync(path.join(d, e.name))]))
    }
  }
  walk(root, '')
  return { files, dirs }
}

async function writeTree(root, files, dirs) {
  for (const d of dirs) fs.mkdirSync(path.join(root, d), { recursive: true })
  for (const [p, blob] of files) {
    fs.mkdirSync(path.dirname(path.join(root, p)), { recursive: true })
    fs.writeFileSync(path.join(root, p), Buffer.from(await blob.arrayBuffer()))
  }
}

function options(flags) {
  const f = flags.split(/\s+/).filter(Boolean)
  let minVolumes
  f.forEach((x, i) => {
    if (x === '-N' || x === '--min-volumes') minVolumes = Number(f[i + 1])
    else if (x.startsWith('--min-volumes=')) minVolumes = Number(x.split('=')[1])
    else if (/^-N\d+$/.test(x)) minVolumes = Number(x.slice(2))
  })
  return {
    strict: f.includes('--strict'),
    keepDerivatives: f.includes('--keep-derivatives'),
    shiftDates: f.includes('--shift-dates'),
    minVolumes,
    rootName: 'out',
  }
}

async function jsMatchesCli(src, flags) {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'reproin-js-'))
  try {
    const a = path.join(tmp, 'cli/out')
    const b = path.join(tmp, 'js/out')
    fs.cpSync(src, a, { recursive: true })
    spawnSync(cli, ['--no-convert', ...flags.split(/\s+/).filter(Boolean), a, a])
    const { files, dirs } = readTree(src)
    const p = await plan(files, options(flags), dirs)
    const out = await apply(files, p.ops, { dirs })
    await writeTree(b, out.files, out.dirs)
    fs.mkdirSync(b, { recursive: true })
    const r = spawnSync('python3', ['-I', compare, a, b], { encoding: 'utf8' })
    return r.status === 0 ? '' : r.stdout || r.stderr || `comparator exited ${r.status}`
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true })
  }
}

test('synthetic cases: JS host == native CLI', async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'reproin-cases-'))
  try {
    execFileSync('python3', ['-I', path.join(repo, 'tests/synth/run.py'), '--emit', dir])
    const bad = []
    for (const name of fs.readdirSync(dir).sort()) {
      const diff = await jsMatchesCli(path.join(dir, name, 'out'), fs.readFileSync(path.join(dir, name, 'flags'), 'utf8'))
      if (diff) bad.push(`${name}:\n${diff}`)
    }
    assert.deepEqual(bad, [])
  } finally {
    fs.rmSync(dir, { recursive: true, force: true })
  }
})

test('golden trees: JS host == native CLI', async (t) => {
  const golden = path.join(repo, '.golden')
  const names = fs.existsSync(golden) ? fs.readdirSync(golden).filter((n) => fs.existsSync(path.join(golden, n, 'raw'))) : []
  assert.ok(names.length, 'no golden data: run tests/golden/make_refs.sh')
  for (const n of names) assert.equal(await jsMatchesCli(path.join(golden, n, 'raw'), ''), '', n)
})

test('zip round-trips', async () => {
  const files = new Map([
    ['a/b.json', new Blob(['{"x": 1}'])],
    ['c.tsv', new Blob(['onset\n'])],
  ])
  const z = Buffer.from(await (await zip(files, 'ds/')).arrayBuffer())
  const tmp = path.join(os.tmpdir(), `reproin-${process.pid}.zip`)
  fs.writeFileSync(tmp, z)
  const r = spawnSync('python3', ['-I', '-c', `import zipfile,sys; z=zipfile.ZipFile(sys.argv[1]); assert z.testzip() is None; print(sorted(z.namelist())); print(z.read("ds/a/b.json").decode())`, tmp], { encoding: 'utf8' })
  fs.rmSync(tmp)
  assert.equal(r.status, 0, r.stderr)
  assert.match(r.stdout, /\['ds\/a\/b.json', 'ds\/c.tsv'\]\n\{"x": 1\}/)
})

test('postPass on an all-Unknown tree reports, does not throw', async () => {
  const r = await postPass(new Map([['Unknown/1_x.json', new Blob(['{}'])]]))
  assert.equal(r.fatal, null)
  assert.ok(r.log.some((l) => l.includes('no sub-*')))
})

test('apply honours an aborted signal and leaves the input tree intact', async () => {
  const tree = new Map([['sub-01/anat/sub-01_T1w.json', new Blob(['{}'])]])
  const p = await plan(tree, {})
  const ac = new AbortController()
  ac.abort()
  await assert.rejects(apply(tree, p.ops, { signal: ac.signal }), { name: 'AbortError' })
  assert.deepEqual([...tree.keys()], ['sub-01/anat/sub-01_T1w.json'])
})

test('bundled dcm2niix is the published release except the one-line worker patch', async () => {
  const dir = path.join(repo, 'js/dcm2niix')
  const crypto = await import('node:crypto')
  for (const line of fs.readFileSync(path.join(dir, 'SHA256SUMS'), 'utf8').trim().split('\n')) {
    const [sum, name] = line.split(/\s+/)
    assert.equal(crypto.createHash('sha256').update(fs.readFileSync(path.join(dir, name))).digest('hex'), sum, name)
  }
  // Published 1.4.20261029 hashes of the converter itself (npm tarball).
  const pub = {
    'dcm2niix.jpeg.wasm': '782c20d8e213c0ae14e0de9ae7164ac1e23a13bb8450528d36d8f832201ce178',
    'dcm2niix.jpeg.js': 'd34bf44535100724c8dfb88e8ce1718ff8b55ea725089cead567d6f23f5f3497',
  }
  for (const [name, sum] of Object.entries(pub)) {
    assert.equal(crypto.createHash('sha256').update(fs.readFileSync(path.join(dir, name))).digest('hex'), sum, name)
  }
  assert.match(fs.readFileSync(path.join(dir, 'worker.jpeg.js'), 'utf8'), /name !== "\.reproin_provenance\.tsv"/)
})
