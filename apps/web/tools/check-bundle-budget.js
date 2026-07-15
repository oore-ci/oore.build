import { gzipSync } from 'node:zlib'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

const distDir = resolve(import.meta.dirname, '../dist')
const manifest = JSON.parse(
  readFileSync(resolve(distDir, '.vite/manifest.json'), 'utf8'),
)

const entryKey = Object.entries(manifest).find(([, chunk]) => chunk.isEntry)?.[0]
if (!entryKey) throw new Error('Vite manifest has no entry chunk')

function assetsFor(entryKeys) {
  const js = new Set()
  const css = new Set()
  const seen = new Set()

  function visit(key) {
    const chunk = manifest[key]
    if (!chunk) throw new Error(`Vite manifest has no chunk for ${key}`)
    if (seen.has(chunk.file)) return
    seen.add(chunk.file)
    js.add(chunk.file)
    for (const stylesheet of chunk.css ?? []) css.add(stylesheet)
    for (const imported of chunk.imports ?? []) visit(imported)
  }

  for (const key of entryKeys) visit(key)
  return { js, css }
}

function gzipKiB(assetPaths) {
  const bytes = [...assetPaths].reduce((total, assetPath) => {
    const contents = readFileSync(resolve(distDir, assetPath))
    return total + gzipSync(contents).byteLength
  }, 0)
  return bytes / 1024
}

const assets = assetsFor([entryKey])
const jsKiB = gzipKiB(assets.js)
const cssKiB = gzipKiB(assets.css)
const jsBudgetKiB = Number(process.env.OORE_WEB_JS_BUDGET_KIB ?? 190)
const cssBudgetKiB = Number(process.env.OORE_WEB_CSS_BUDGET_KIB ?? 22)

const profiles = [
  {
    name: 'Mobile shell',
    entries: ['src/components/ui/sidebar-mobile.tsx'],
    budgetKiB: Number(process.env.OORE_WEB_MOBILE_SHELL_BUDGET_KIB ?? 195),
  },
  {
    name: 'Admin shell interactions',
    entries: [
      'src/components/instance-switcher-menu.tsx',
      'src/components/nav-user-menu.tsx',
      'src/components/ui/sidebar-menu-tooltip.tsx',
    ],
    budgetKiB: Number(process.env.OORE_WEB_ADMIN_SHELL_BUDGET_KIB ?? 230),
  },
  {
    name: 'Admin command palette',
    entries: ['src/components/command-palette.tsx'],
    budgetKiB: Number(process.env.OORE_WEB_COMMAND_PALETTE_BUDGET_KIB ?? 215),
  },
  {
    name: 'Operator build detail',
    entries: [
      'src/routes/builds/$buildId.tsx?tsr-split=component',
      'src/components/build-details/build-detail-page.tsx',
    ],
    budgetKiB: Number(process.env.OORE_WEB_BUILD_DETAIL_BUDGET_KIB ?? 240),
  },
  {
    name: 'Operator artifact sharing',
    entries: [
      'src/routes/builds/$buildId.tsx?tsr-split=component',
      'src/components/build-details/build-detail-page.tsx',
      'src/components/build-details/artifact-share-menu.tsx',
    ],
    budgetKiB: Number(process.env.OORE_WEB_ARTIFACT_SHARE_BUDGET_KIB ?? 275),
  },
  {
    name: 'QA artifact install',
    entries: [
      'src/routes/builds/$buildId.tsx?tsr-split=component',
      'src/components/build-details/artifact-install-page.tsx',
    ],
    budgetKiB: Number(process.env.OORE_WEB_QA_INSTALL_BUDGET_KIB ?? 195),
  },
  {
    name: 'QA install with changelog',
    entries: [
      'src/routes/builds/$buildId.tsx?tsr-split=component',
      'src/components/build-details/artifact-install-page.tsx',
      'src/components/build-details/changelog-markdown.tsx',
    ],
    budgetKiB: Number(process.env.OORE_WEB_QA_CHANGELOG_BUDGET_KIB ?? 215),
  },
]

console.log(
  `Initial bundle: ${jsKiB.toFixed(2)} KiB JS / ${cssKiB.toFixed(2)} KiB CSS gzip`,
)
console.log(
  `Bundle budget:  ${jsBudgetKiB.toFixed(2)} KiB JS / ${cssBudgetKiB.toFixed(2)} KiB CSS gzip`,
)

let exceedsBudget = jsKiB > jsBudgetKiB || cssKiB > cssBudgetKiB

for (const profile of profiles) {
  const profileAssets = assetsFor([entryKey, ...profile.entries])
  const profileJsKiB = gzipKiB(profileAssets.js)
  console.log(
    `${profile.name.padEnd(24)} ${profileJsKiB.toFixed(2)} KiB JS / ${profile.budgetKiB.toFixed(2)} KiB budget`,
  )
  exceedsBudget ||= profileJsKiB > profile.budgetKiB
}

if (exceedsBudget) {
  console.error('Web bundle exceeds a production budget.')
  process.exitCode = 1
}
