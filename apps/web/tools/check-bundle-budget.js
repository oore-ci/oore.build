import { gzipSync } from 'node:zlib'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

const distDir = resolve(import.meta.dirname, '../dist')
const html = readFileSync(resolve(distDir, 'index.html'), 'utf8')

function initialAssets(extension) {
  const pattern = new RegExp(
    `(?:src|href)="(/assets/[^"]+\\.${extension})"`,
    'g',
  )
  return [...new Set([...html.matchAll(pattern)].map((match) => match[1]))]
}

function gzipKiB(assetPaths) {
  const bytes = assetPaths.reduce((total, assetPath) => {
    const contents = readFileSync(resolve(distDir, assetPath.slice(1)))
    return total + gzipSync(contents).byteLength
  }, 0)
  return bytes / 1024
}

const jsKiB = gzipKiB(initialAssets('js'))
const cssKiB = gzipKiB(initialAssets('css'))
const jsBudgetKiB = Number(process.env.OORE_WEB_JS_BUDGET_KIB ?? 240)
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
