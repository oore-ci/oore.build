import { gzipSync } from 'node:zlib'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

const distDir = resolve(import.meta.dirname, '../dist')
const manifest = JSON.parse(
  readFileSync(resolve(distDir, '.vite/manifest.json'), 'utf8'),
)

const entryKey = Object.entries(manifest).find(
  ([, chunk]) => chunk.isEntry,
)?.[0]
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
const jsBudgetKiB = Number(process.env.OORE_WEB_JS_BUDGET_KIB ?? 165)
const cssBudgetKiB = Number(process.env.OORE_WEB_CSS_BUDGET_KIB ?? 22)

const profiles = [
  {
    name: 'Field metrics after idle',
    entries: ['src/web-performance.ts'],
    budgetKiB: Number(process.env.OORE_WEB_FIELD_METRICS_BUDGET_KIB ?? 165),
    includeDynamic: true,
  },
  {
    name: 'Mobile shell',
    entries: ['src/components/ui/sidebar-mobile.tsx'],
    budgetKiB: Number(process.env.OORE_WEB_MOBILE_SHELL_BUDGET_KIB ?? 185),
  },
  {
    name: 'Admin shell interactions',
    entries: [
      'src/components/instance-switcher-menu.tsx',
      'src/components/nav-user-menu.tsx',
      'src/components/ui/sidebar-menu-tooltip.tsx',
    ],
    budgetKiB: Number(process.env.OORE_WEB_ADMIN_SHELL_BUDGET_KIB ?? 225),
  },
  {
    name: 'Admin command palette',
    entries: ['src/components/command-palette.tsx'],
    budgetKiB: Number(process.env.OORE_WEB_COMMAND_PALETTE_BUDGET_KIB ?? 210),
  },
  {
    name: 'Operator build detail',
    entries: [
      'src/routes/builds/$buildId.tsx?tsr-split=component',
      'src/components/build-details/build-detail-page.tsx',
    ],
    budgetKiB: Number(process.env.OORE_WEB_BUILD_DETAIL_BUDGET_KIB ?? 210),
  },
  {
    name: 'Operator artifact sharing',
    entries: [
      'src/routes/builds/$buildId.tsx?tsr-split=component',
      'src/components/build-details/build-detail-page.tsx',
      'src/components/build-details/artifact-share-menu.tsx',
    ],
    budgetKiB: Number(process.env.OORE_WEB_ARTIFACT_SHARE_BUDGET_KIB ?? 270),
  },
  {
    name: 'QA artifact install',
    entries: [
      'src/routes/builds/$buildId.tsx?tsr-split=component',
      'src/components/build-details/artifact-install-page.tsx',
    ],
    budgetKiB: Number(process.env.OORE_WEB_QA_INSTALL_BUDGET_KIB ?? 175),
  },
  {
    name: 'QA install with changelog',
    entries: [
      'src/routes/builds/$buildId.tsx?tsr-split=component',
      'src/components/build-details/artifact-install-page.tsx',
      'src/components/build-details/changelog-markdown.tsx',
    ],
    budgetKiB: Number(process.env.OORE_WEB_QA_CHANGELOG_BUDGET_KIB ?? 210),
  },
  {
    name: 'Projects cold route',
    entries: ['src/routes/projects/index.tsx?tsr-split=component'],
    budgetKiB: Number(process.env.OORE_WEB_PROJECTS_ROUTE_BUDGET_KIB ?? 175),
  },
  {
    name: 'Build history cold route',
    entries: ['src/routes/builds/index.tsx?tsr-split=component'],
    budgetKiB: Number(process.env.OORE_WEB_BUILDS_ROUTE_BUDGET_KIB ?? 225),
  },
  {
    name: 'Owner dashboard route',
    entries: ['src/routes/index.tsx?tsr-split=component'],
    budgetKiB: Number(process.env.OORE_WEB_DASHBOARD_ROUTE_BUDGET_KIB ?? 235),
  },
  {
    name: 'Project detail route',
    entries: ['src/routes/projects/$projectId/index.tsx?tsr-split=component'],
    budgetKiB: Number(process.env.OORE_WEB_PROJECT_ROUTE_BUDGET_KIB ?? 215),
  },
  {
    name: 'Pipeline detail route',
    entries: [
      'src/routes/projects/$projectId/pipelines/$pipelineId.tsx?tsr-split=component',
    ],
    budgetKiB: Number(process.env.OORE_WEB_PIPELINE_ROUTE_BUDGET_KIB ?? 205),
  },
  {
    name: 'QA dashboard route',
    entries: [
      'src/routes/index.tsx?tsr-split=component',
      'src/components/qa-releases-page.tsx',
    ],
    budgetKiB: Number(process.env.OORE_WEB_QA_ROUTE_BUDGET_KIB ?? 245),
  },
  {
    name: 'QA diagnostic logs',
    entries: [
      'src/routes/builds/$buildId.tsx?tsr-split=component',
      'src/components/build-details/artifact-install-page.tsx',
      'src/components/build-details/qa-build-logs.tsx',
    ],
    budgetKiB: Number(process.env.OORE_WEB_QA_LOGS_BUDGET_KIB ?? 200),
  },
  {
    name: 'Preferences cold route',
    entries: ['src/routes/settings/preferences.lazy.tsx'],
    budgetKiB: Number(process.env.OORE_WEB_PREFERENCES_ROUTE_BUDGET_KIB ?? 285),
  },
  {
    name: 'Users cold route',
    entries: ['src/routes/settings/users.tsx?tsr-split=component'],
    budgetKiB: Number(process.env.OORE_WEB_USERS_ROUTE_BUDGET_KIB ?? 245),
  },
  {
    name: 'User invite dialog',
    entries: [
      'src/routes/settings/users.tsx?tsr-split=component',
      'src/routes/settings/-invite-user-dialog.tsx',
    ],
    budgetKiB: Number(process.env.OORE_WEB_USER_INVITE_BUDGET_KIB ?? 285),
  },
  {
    name: 'Pipeline create route',
    entries: [
      'src/routes/projects/$projectId/pipelines/new.tsx?tsr-split=component',
    ],
    budgetKiB: Number(process.env.OORE_WEB_PIPELINE_CREATE_BUDGET_KIB ?? 265),
  },
  {
    name: 'Pipeline edit route',
    entries: [
      'src/routes/projects/$projectId/pipelines/$pipelineId_.edit.tsx?tsr-split=component',
    ],
    budgetKiB: Number(process.env.OORE_WEB_PIPELINE_EDIT_BUDGET_KIB ?? 265),
  },
  {
    name: 'Notification edit route',
    entries: ['src/routes/settings/notifications/$channelId.lazy.tsx'],
    budgetKiB: Number(process.env.OORE_WEB_NOTIFICATION_EDIT_BUDGET_KIB ?? 255),
  },
  {
    name: 'API tokens route',
    entries: ['src/routes/settings/api-tokens.lazy.tsx'],
    budgetKiB: Number(process.env.OORE_WEB_API_TOKENS_BUDGET_KIB ?? 255),
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
  if (profile.includeDynamic) {
    for (const profileEntry of profile.entries) {
      for (const dynamicEntry of manifest[profileEntry]?.dynamicImports ?? []) {
        const dynamicAssets = assetsFor([dynamicEntry])
        for (const asset of dynamicAssets.js) profileAssets.js.add(asset)
        for (const asset of dynamicAssets.css) profileAssets.css.add(asset)
      }
    }
  }
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
