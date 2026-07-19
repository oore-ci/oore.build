import assert from 'node:assert/strict'
import { existsSync, readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import test from 'node:test'

const here = dirname(fileURLToPath(import.meta.url))
const siteRoot = join(here, '..')
const html = readFileSync(join(siteRoot, 'index.html'), 'utf8')
const css = readFileSync(join(siteRoot, 'src/styles.css'), 'utf8')
const javascript = readFileSync(join(siteRoot, 'src/main.js'), 'utf8')
const normalizedHtml = html.replace(/\s+/g, ' ')

test('keeps the static Vite entry points and local assets intact', () => {
  assert.match(html, /href="\/src\/styles\.css"/)
  assert.match(html, /src="\/src\/main\.js"/)

  const assetReferences = [
    ...html.matchAll(
      /(?:href|src)="(\/(?:src|fonts|product)\/[^"?#]+|\/logo\.svg)"/g,
    ),
  ].map((match) => match[1])

  assert.ok(
    assetReferences.length >= 8,
    'expected local font, product, brand, and source assets',
  )
  for (const reference of assetReferences) {
    const path = reference.startsWith('/src/')
      ? join(siteRoot, reference.slice(1))
      : join(siteRoot, 'public', reference.slice(1))
    assert.ok(existsSync(path), `missing local asset ${reference}`)
  }
})

test('uses the current managed-install and Direct runner product contract', () => {
  assert.match(
    normalizedHtml,
    /Register and enable the Direct runner, then approve repositories once/,
  )
  assert.match(normalizedHtml, /Install once\. Update from the UI\./)
  assert.match(
    normalizedHtml,
    /does not require SSH or a separate <code>launchctl kickstart<\/code>/,
  )
  assert.match(normalizedHtml, /It is not a hostile-code sandbox\./)
  assert.match(
    normalizedHtml,
    /External-fork pull and merge requests are ignored\./,
  )

  assert.doesNotMatch(html, /oored run/)
  assert.doesNotMatch(html, /oore setup token/)
  assert.doesNotMatch(html, /getting-started\/public-alpha/)
})

test('keeps the page concise, semantic, and free of templated design tells', () => {
  assert.equal((html.match(/<h1\b/g) ?? []).length, 1)
  assert.match(html, /<main id="main">/)
  assert.match(html, /class="skip-link" href="#main"/)
  assert.match(html, /aria-controls="site-nav"/)
  assert.match(html, /<ol class="workflow">/)

  assert.doesNotMatch(html, /[—–]/)
  assert.doesNotMatch(css, /gradient\(/)
  assert.doesNotMatch(css, /text-transform:\s*uppercase/)
})

test('supports light, dark, keyboard, mobile, and reduced-motion behavior', () => {
  assert.match(css, /:root\s*\{/)
  assert.match(css, /prefers-color-scheme:\s*dark/)
  assert.match(css, /:root\[data-theme=["']dark["']\]/)
  assert.match(css, /prefers-reduced-motion:\s*reduce/)
  assert.match(css, /@media \(max-width: 820px\)/)
  assert.match(css, /min-height:\s*2\.75rem/)

  assert.match(javascript, /event\.key === ["']Escape["']/)
  assert.match(javascript, /aria-expanded/)
  assert.doesNotMatch(javascript, /addEventListener\(['"]scroll/)
})

test('keeps route and social metadata canonical', () => {
  assert.match(html, /<link rel="canonical" href="https:\/\/oore\.build\/"/)
  assert.match(html, /property="og:url" content="https:\/\/oore\.build\/"/)
  assert.match(html, /name="twitter:card" content="summary_large_image"/)
  assert.match(html, /"@type": "SoftwareApplication"/)
})
