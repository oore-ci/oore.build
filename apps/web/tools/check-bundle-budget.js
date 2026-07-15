import { gzipSync } from 'node:zlib'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

const distDir = resolve(import.meta.dirname, '../dist')
const manifest = JSON.parse(
  readFileSync(resolve(distDir, '.vite/manifest.json'), 'utf8'),
)

function initialAssets() {
  const entry = Object.values(manifest).find((chunk) => chunk.isEntry)
  if (!entry) throw new Error('Vite manifest has no entry chunk')

  const js = new Set()
  const css = new Set()
  const seen = new Set()

  function visit(chunk) {
    if (seen.has(chunk.file)) return
    seen.add(chunk.file)
    js.add(chunk.file)
    for (const stylesheet of chunk.css ?? []) css.add(stylesheet)
    for (const imported of chunk.imports ?? []) visit(manifest[imported])
  }

  visit(entry)
  return { js, css }
}

function gzipKiB(assetPaths) {
  const bytes = [...assetPaths].reduce((total, assetPath) => {
    const contents = readFileSync(resolve(distDir, assetPath))
    return total + gzipSync(contents).byteLength
  }, 0)
  return bytes / 1024
}

const assets = initialAssets()
const jsKiB = gzipKiB(assets.js)
const cssKiB = gzipKiB(assets.css)
const jsBudgetKiB = Number(process.env.OORE_WEB_JS_BUDGET_KIB ?? 245)
const cssBudgetKiB = Number(process.env.OORE_WEB_CSS_BUDGET_KIB ?? 22)

console.log(
  `Initial bundle: ${jsKiB.toFixed(2)} KiB JS / ${cssKiB.toFixed(2)} KiB CSS gzip`,
)
console.log(
  `Bundle budget:  ${jsBudgetKiB.toFixed(2)} KiB JS / ${cssBudgetKiB.toFixed(2)} KiB CSS gzip`,
)

if (jsKiB > jsBudgetKiB || cssKiB > cssBudgetKiB) {
  console.error('Initial bundle exceeds its production budget.')
  process.exitCode = 1
}
