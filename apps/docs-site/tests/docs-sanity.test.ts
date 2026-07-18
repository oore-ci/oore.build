import fs from 'node:fs'
import path from 'node:path'

import { describe, expect, it } from 'vitest'

const docsDir = path.resolve(__dirname, '../docs')
const appDir = path.resolve(__dirname, '..')

function markdownFiles(directory = docsDir): Array<string> {
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const filePath = path.join(directory, entry.name)
    if (entry.isDirectory()) {
      return entry.name === '.vitepress' ? [] : markdownFiles(filePath)
    }
    return entry.name.endsWith('.md') ? [filePath] : []
  })
}

function routeExists(route: string) {
  const cleanRoute = route.split(/[?#]/, 1)[0]
  if (!cleanRoute || cleanRoute === '/') {
    return fs.existsSync(path.join(docsDir, 'index.md'))
  }

  const relative = decodeURIComponent(
    cleanRoute.replace(/^\//, '').replace(/\/$/, ''),
  )
  const candidates = [
    path.join(docsDir, `${relative}.md`),
    path.join(docsDir, relative, 'index.md'),
    path.join(docsDir, 'public', relative),
  ]

  if (relative.startsWith('openapi/operations/')) {
    candidates.push(path.join(docsDir, 'openapi/operations/[operationId].md'))
  }

  return candidates.some((candidate) => fs.existsSync(candidate))
}

describe('documentation structure', () => {
  it('keeps the primary task and reference entry points', () => {
    const requiredPages = [
      'index.md',
      'getting-started/index.md',
      'getting-started/install.md',
      'guides/index.md',
      'reference/index.md',
      'reference/config/installer.md',
      'openapi/index.md',
      'operations/index.md',
      'operations/known-limitations.md',
      'operations/troubleshooting.md',
    ]

    for (const page of requiredPages) {
      expect(fs.existsSync(path.join(docsDir, page)), page).toBe(true)
    }
  })

  it('has no broken root-relative links in authored Markdown', () => {
    const broken: Array<string> = []
    const markdownLink = /\[[^\]]*\]\((\/[^)\s]+)(?:\s+['"][^)]*['"])?\)/g
    const htmlLink = /href=["'](\/[^"']+)["']/g

    for (const file of markdownFiles()) {
      const source = fs.readFileSync(file, 'utf8')
      for (const pattern of [markdownLink, htmlLink]) {
        pattern.lastIndex = 0
        for (const match of source.matchAll(pattern)) {
          if (!routeExists(match[1])) {
            broken.push(`${path.relative(docsDir, file)} -> ${match[1]}`)
          }
        }
      }
    }

    expect(broken).toEqual([])
  })

  it('keeps generated OpenAPI as the only API contract', () => {
    const overview = fs.readFileSync(
      path.join(docsDir, 'openapi/index.md'),
      'utf8',
    )
    expect(overview).not.toContain('<OASpec')

    const apiDir = path.join(docsDir, 'reference/api')
    for (const file of fs
      .readdirSync(apiDir)
      .filter((name) => name.endsWith('.md'))) {
      const source = fs.readFileSync(path.join(apiDir, file), 'utf8')
      expect(source, file).not.toMatch(/^### (Request|Response|Path|Query)/m)
    }
  })

  it('loads the OpenAPI client only for OpenAPI routes', () => {
    const theme = fs.readFileSync(
      path.join(docsDir, '.vitepress/theme/index.ts'),
      'utf8',
    )
    expect(theme).toContain("import('vitepress-openapi/client')")
    expect(theme).not.toMatch(/^import .* from 'vitepress-openapi\/client'/m)
  })

  it('documents managed runner restarts without the obsolete contradiction', () => {
    const install = fs.readFileSync(
      path.join(docsDir, 'getting-started/install.md'),
      'utf8',
    )
    expect(install).toContain('restarts the managed Direct runner service')
    expect(install).not.toContain('Remote runner updates are not available yet')
  })

  it('uses the canonical Direct runner policy controls', () => {
    const obsoleteLocations: Array<string> = []
    const obsoleteControl =
      /(?:direct(?: macos)? runner[\s\S]{0,200}settings\s*(?:>|→)\s*preferences|settings\s*(?:>|→)\s*preferences[\s\S]{0,200}direct(?: macos)? runner)/gi

    for (const file of markdownFiles()) {
      const source = fs.readFileSync(file, 'utf8')
      if (obsoleteControl.test(source)) {
        obsoleteLocations.push(path.relative(docsDir, file))
      }
      obsoleteControl.lastIndex = 0
    }

    expect(obsoleteLocations).toEqual([])

    const runnerGuide = fs.readFileSync(
      path.join(docsDir, 'guides/runners/external-runner.md'),
      'utf8',
    )
    expect(runnerGuide).toContain('Settings > Runners')
    expect(runnerGuide).toContain('Settings > Sources')
  })

  it('uses per-route canonical metadata', () => {
    const config = fs.readFileSync(
      path.join(docsDir, '.vitepress/config.mts'),
      'utf8',
    )
    expect(config).toContain('transformHead({ pageData })')
    expect(config).toContain("property: 'og:url'")
  })

  it('has only the VitePress application scaffold', () => {
    expect(fs.existsSync(path.join(appDir, 'src/main.tsx'))).toBe(false)
    expect(fs.existsSync(path.join(appDir, 'vite.config.ts'))).toBe(false)
    expect(fs.existsSync(path.join(appDir, 'index.html'))).toBe(false)
  })
})
