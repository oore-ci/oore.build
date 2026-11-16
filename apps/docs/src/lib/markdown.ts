import type { Node, Root } from 'fumadocs-core/page-tree'
import { readFileSync } from 'node:fs'

import { docsOrigin, markdownUrl, mdxToMarkdown } from '@/lib/markdown-convert'
import { OPENAPI_CATEGORIES } from '@/lib/openapi-categories'
import {
  categoryMarkdown,
  listOperations,
  operationIndexLine,
  operationMarkdown,
  type OpenAPIDocumentLike,
} from '@/lib/openapi-markdown'
import {
  isAuthoredPage,
  openAPIOperationDetails,
  source,
  type DocsPage,
} from '@/lib/source'

export const openAPIDocument = JSON.parse(
  readFileSync('./public/openapi.json', 'utf8'),
) as OpenAPIDocumentLike

export const siteSummary =
  'Oore CI is a self-hosted, Flutter-first mobile CI and internal app distribution platform. One Mac (or a Linux web node plus Mac runners) builds Flutter apps for Android and iOS, signs them, hands builds to testers and publishes to Google Play and the App Store. Everything the web UI does goes through a documented HTTP API, and the `oore` CLI covers the daily build loop.'

export function pageMarkdown(page: DocsPage) {
  if (isAuthoredPage(page)) {
    return mdxToMarkdown({
      body: page.data._raw.body ?? '',
      description: page.data.description,
      title: page.data.title,
      url: page.url,
    })
  }

  const operation = openAPIOperationDetails(page)
  if (operation) {
    const pathItem = openAPIDocument.paths[operation.path] ?? {}
    const object = pathItem[operation.method.toLowerCase()]
    if (typeof object !== 'object' || object === null) {
      throw new Error(`Operation missing from openapi.json: ${page.url}`)
    }
    return operationMarkdown(openAPIDocument, {
      method: operation.method.toLowerCase(),
      operation: object as Record<string, unknown>,
      path: operation.path,
    })
  }

  const slug = page.slugs.at(-1)
  const category = OPENAPI_CATEGORIES.find((item) => item.slug === slug)
  if (!category) throw new Error(`No Markdown renderer for ${page.url}`)
  return categoryMarkdown(openAPIDocument, category, page.data.description)
}

export interface MarkdownSection {
  pages: DocsPage[]
  title: string
}

function nodeName(node: { name?: unknown }) {
  return typeof node.name === 'string' ? node.name : ''
}

/** Authored pages grouped by the sidebar's top-level sections, in order. */
export function authoredSections(): MarkdownSection[] {
  const byUrl = new Map(source.getPages().map((page) => [page.url, page]))
  const sections: MarkdownSection[] = []
  const seen = new Set<string>()
  let current: MarkdownSection = { pages: [], title: 'Overview' }
  sections.push(current)

  function add(url: string | undefined) {
    if (!url || seen.has(url)) return
    const page = byUrl.get(url)
    if (!page || !isAuthoredPage(page)) return
    seen.add(url)
    current.pages.push(page)
  }

  function walk(nodes: Node[]) {
    for (const node of nodes) {
      if (node.type === 'page') add(node.url)
      else if (node.type === 'folder') {
        add(node.index?.url)
        walk(node.children)
      }
    }
  }

  const root: Root = source.getPageTree()
  for (const node of root.children) {
    if (node.type === 'separator') {
      current = { pages: [], title: nodeName(node) || 'More' }
      sections.push(current)
    } else if (node.type === 'folder') {
      current = { pages: [], title: nodeName(node) || 'More' }
      sections.push(current)
      add(node.index?.url)
      walk(node.children)
    } else {
      add(node.url)
    }
  }

  // Pages the tree does not list still belong in the files.
  const rest = source
    .getPages()
    .filter((page) => isAuthoredPage(page) && !seen.has(page.url))
  if (rest.length > 0) sections.push({ pages: rest, title: 'Other pages' })
  return sections.filter((section) => section.pages.length > 0)
}

function pageLine(page: DocsPage) {
  const title = isAuthoredPage(page) ? page.data.title : page.url
  const description = isAuthoredPage(page)
    ? page.data.description?.replace(/\s+/g, ' ').trim()
    : ''
  const link = `[${title}](${markdownUrl(page.url)})`
  return description ? `- ${link}: ${description}` : `- ${link}`
}

const agentStart = [
  ['/build/automate', 'Start here to drive Oore from a script or an agent.'],
  ['/team/api-tokens', 'Create the token a script or an agent signs in with.'],
  ['/reference/api', 'HTTP API overview. The contract is /openapi.json.'],
  ['/reference/cli', 'The `oore` command line.'],
  ['/reference/errors', 'Error envelope and codes.'],
] as const

function agentLines() {
  const byUrl = new Map(source.getPages().map((page) => [page.url, page]))
  return agentStart.flatMap(([url, note]) => {
    const page = byUrl.get(url)
    if (!page || !isAuthoredPage(page)) return []
    return [`- [${page.data.title}](${markdownUrl(url)}): ${note}`]
  })
}

function preamble() {
  return [
    '# Oore CI documentation',
    '',
    `> ${siteSummary}`,
    '',
    `Every page on ${docsOrigin} is also plain Markdown: add \`.md\` to its URL (\`${docsOrigin}/index.md\` for the home page), or request it with \`Accept: text/markdown\`. The whole site in one file is ${docsOrigin}/llms-full.txt. The HTTP API contract is ${docsOrigin}/openapi.json, and the \`.oore.yaml\` schema is ${docsOrigin}/schema/oore-yaml-v2.json.`,
    '',
    'Oore is self-hosted, so there is no shared API host. Point tools at your own instance with `OORE_URL` (for example `https://ci.example.com`) and sign in with an API token in `OORE_TOKEN`.',
    '',
  ]
}

export function llmsTxt() {
  const out = preamble()
  const agents = agentLines()
  if (agents.length > 0)
    out.push('## For agents and scripts', '', ...agents, '')
  for (const section of authoredSections()) {
    out.push(`## ${section.title}`, '', ...section.pages.map(pageLine), '')
  }
  out.push(
    '## API reference by area',
    '',
    ...OPENAPI_CATEGORIES.map(
      (category) =>
        `- [${category.title}](${markdownUrl(`/reference/api/categories/${category.slug}`)}): ${category.tags.join(', ')}`,
    ),
    '',
    '## Optional',
    '',
    `- [Full documentation](${docsOrigin}/llms-full.txt): every authored page in one Markdown file, with an index of all API operations.`,
    `- [OpenAPI contract](${docsOrigin}/openapi.json): every HTTP operation, parameter and schema.`,
    '',
  )
  return out.join('\n')
}

export function llmsFullTxt() {
  const out = preamble()
  for (const section of authoredSections()) {
    for (const page of section.pages) {
      out.push('---', '', pageMarkdown(page).trim(), '')
    }
  }
  out.push(
    '---',
    '',
    '# API operations',
    '',
    `Each line links to the operation's Markdown page, with parameters, body fields, a curl example and responses. The contract is ${docsOrigin}/openapi.json.`,
    '',
    ...listOperations(openAPIDocument).map(operationIndexLine),
    '',
  )
  return out.join('\n')
}

export const markdownHeaders = {
  'Content-Type': 'text/markdown; charset=utf-8',
}
