// Real-browser check: DICOM folder -> @niivue/dcm2niix (WASM) -> reproin-rs
// post-pass -> ZIP, in headless Chromium. Prints timing and the peak resident
// memory of the browser's processes; writes the ZIP for comparison with the
// native pipeline. Usage:
//   node js/tools/browser.mjs <dicom-dir> <out.zip> [playwright-module-dir]
import { execFileSync } from 'node:child_process'
import fs from 'node:fs'
import http from 'node:http'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const [dicom, outZip, pwDir] = process.argv.slice(2)
const jsDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const { chromium } = await import(pwDir ? path.join(pwDir, 'index.mjs') : 'playwright')

const list = []
const walk = (d, rel) => {
  for (const e of fs.readdirSync(d, { withFileTypes: true })) {
    const r = rel ? `${rel}/${e.name}` : e.name
    if (e.isDirectory()) walk(path.join(d, e.name), r)
    else if (e.isFile() && !e.name.startsWith('.')) list.push(r)
  }
}
walk(dicom, '')
const top = path.basename(path.resolve(dicom))

const page = `<!doctype html><meta charset="utf-8">
<script type="module">
import { convertToBids, load, zip } from '/js/index.js'
window.run = async () => {
  await load()
  const names = await (await fetch('/list.json')).json()
  const files = await Promise.all(names.map(async (n) => {
    const f = new File([await (await fetch('/dicom/' + n)).blob()], n.split('/').pop())
    f._webkitRelativePath = '${top}/' + n
    return f
  }))
  const t0 = performance.now()
  const r = await convertToBids(files, {})
  const t1 = performance.now()
  const z = await zip(r.files)
  const bytes = new Uint8Array(await z.arrayBuffer())
  let s = ''
  for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode(...bytes.subarray(i, i + 0x8000))
  return { ms: t1 - t0, ops: r.ops.length, fatal: r.fatal, failures: r.failures, exitCode: r.exitCode, files: r.files.size, zip: btoa(s) }
}
</script>`

const types = { '.js': 'text/javascript', '.wasm': 'application/wasm', '.json': 'application/json', '.html': 'text/html' }
const server = http.createServer((req, res) => {
  const u = decodeURIComponent(new URL(req.url, 'http://x').pathname)
  let file
  if (u === '/') return res.end(page)
  if (u === '/list.json') return res.end(JSON.stringify(list))
  if (u.startsWith('/js/')) file = path.join(jsDir, u.slice(4))
  else if (u.startsWith('/dicom/')) file = path.join(dicom, u.slice(7))
  if (!file || !fs.existsSync(file)) return res.writeHead(404).end()
  res.writeHead(200, { 'content-type': types[path.extname(file)] ?? 'application/octet-stream' })
  fs.createReadStream(file).pipe(res)
})
await new Promise((r) => server.listen(0, '127.0.0.1', r))

const browser = await chromium.launch()
// Sum RSS (KiB) of every process running this Playwright Chromium build.
const exe = chromium.executablePath()
const root = exe.slice(0, exe.indexOf('/chrom') > 0 ? exe.indexOf('/chrom') : exe.length)
const rss = () =>
  execFileSync('ps', ['-axo', 'rss=,command='])
    .toString()
    .split('\n')
    .filter((l) => l.includes(root))
    .reduce((n, l) => n + Number(l.trim().split(/\s+/)[0]), 0)
const ctx = await browser.newContext()
const tab = await ctx.newPage()
tab.on('pageerror', (e) => console.error('pageerror', e))
await tab.goto(`http://127.0.0.1:${server.address().port}/`)
const base = rss()
let peak = base
const timer = setInterval(() => (peak = Math.max(peak, rss())), 100)
const r = await tab.evaluate(() => window.run())
clearInterval(timer)
await browser.close()
server.close()
fs.writeFileSync(outZip, Buffer.from(r.zip, 'base64'))
const inMB = list.reduce((n, f) => n + fs.statSync(path.join(dicom, f)).size, 0) / 1e6
console.log(JSON.stringify({ dicomFiles: list.length, dicomMB: +inMB.toFixed(1), convertAndPlanMs: Math.round(r.ms), ops: r.ops, outputFiles: r.files, exitCode: r.exitCode, fatal: r.fatal, failures: r.failures, browserBaselineMB: Math.round(base / 1024), browserPeakMB: Math.round(peak / 1024) }))
if (r.fatal || r.exitCode !== 0) process.exit(1)
