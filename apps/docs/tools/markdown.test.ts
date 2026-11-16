import { describe, expect, test } from 'bun:test'
import { readFileSync } from 'node:fs'
import path from 'node:path'

import { mdxToMarkdown } from '../src/lib/markdown-convert'
import {
  curlExample,
  operationMarkdown,
  type OpenAPIDocumentLike,
} from '../src/lib/openapi-markdown'
import worker, {
  acceptsMarkdown,
  markdownPath,
  REDIRECTS,
  SECURITY_HEADERS,
} from '../public/_worker.js'

const publicDir = path.resolve(import.meta.dir, '../public')

const env = {
  ASSETS: {
    fetch(request: Request) {
      const pathname = new URL(request.url).pathname
      if (pathname === '/index.md' || pathname === '/build/run/trigger.md') {
        return Promise.resolve(new Response(`# ${pathname}\n`))
      }
      if (pathname.endsWith('.md')) {
        return Promise.resolve(new Response('Not found', { status: 404 }))
      }
      return Promise.resolve(
        new Response('<h1>Oore</h1>', {
          headers: { 'content-type': 'text/html; charset=utf-8' },
        }),
      )
    },
  },
}

describe('docs worker', () => {
  test('serves Markdown for Accept: text/markdown', async () => {
    const response = await worker.fetch(
      new Request('https://docs.oore.build/build/run/trigger', {
        headers: { accept: 'text/markdown' },
      }),
      env,
    )
    expect(response.headers.get('content-type')).toBe(
      'text/markdown; charset=utf-8',
    )
    expect(response.headers.get('vary')).toContain('Accept')
    expect(await response.text()).toBe('# /build/run/trigger.md\n')
  })

  test('serves the home page Markdown', async () => {
    const response = await worker.fetch(
      new Request('https://docs.oore.build/', {
        headers: { accept: 'text/markdown, text/html;q=0.5' },
      }),
      env,
    )
    expect(await response.text()).toBe('# /index.md\n')
  })

  test('falls back to HTML when a page has no Markdown twin', async () => {
    const response = await worker.fetch(
      new Request('https://docs.oore.build/missing', {
        headers: { accept: 'text/markdown' },
      }),
      env,
    )
    expect(response.headers.get('content-type')).toContain('text/html')
  })

  test('keeps HTML, security and cache headers for browsers', async () => {
    const response = await worker.fetch(
      new Request('https://docs.oore.build/', {
        headers: { accept: 'text/html' },
      }),
      env,
    )
    expect(response.headers.get('content-type')).toContain('text/html')
    expect(response.headers.get('cache-control')).toBe(
      'public, max-age=0, must-revalidate',
    )
    expect(response.headers.get('link')).toContain('/index.md')
    for (const [name, value] of Object.entries(SECURITY_HEADERS)) {
      expect(response.headers.get(name)).toBe(value)
    }
  })

  test('keeps immutable caching on build assets', async () => {
    const response = await worker.fetch(
      new Request('https://docs.oore.build/_astro/app.hash.js'),
      env,
    )
    expect(response.headers.get('cache-control')).toBe(
      'public, max-age=31536000, immutable',
    )
  })

  test('applies the moved-page redirects', async () => {
    const response = await worker.fetch(
      new Request('https://docs.oore.build/understand/signing'),
      env,
    )
    expect(response.status).toBe(301)
    expect(response.headers.get('location')).toBe(
      'https://docs.oore.build/concepts/signing',
    )
  })

  test('redirect table matches public/_redirects', () => {
    const fromFile = Object.fromEntries(
      readFileSync(path.join(publicDir, '_redirects'), 'utf8')
        .split('\n')
        .map((line) => line.trim())
        .filter((line) => line && !line.startsWith('#'))
        .map((line) => line.split(/\s+/).slice(0, 2)),
    )
    expect(REDIRECTS).toEqual(fromFile)
  })

  test('security headers match public/_headers', () => {
    const headers = readFileSync(path.join(publicDir, '_headers'), 'utf8')
    for (const [name, value] of Object.entries(SECURITY_HEADERS)) {
      expect(headers.toLowerCase()).toContain(`${name}: ${value}`.toLowerCase())
    }
  })

  test('negotiation rules', () => {
    expect(acceptsMarkdown('text/html, text/markdown;q=0')).toBe(false)
    expect(acceptsMarkdown('text/markdown;q=0.9')).toBe(true)
    expect(markdownPath('/openapi.json')).toBeUndefined()
    expect(markdownPath('/build/')).toBe('/build.md')
    expect(markdownPath('/build.html')).toBe('/build.md')
  })
})

describe('MDX to Markdown', () => {
  test('turns components into Markdown and links into absolute URLs', () => {
    const markdown = mdxToMarkdown({
      body: [
        "import { Foo } from 'bar'",
        '',
        'See [triggers](/build/pipelines/triggers) and ![shot](/demo.png).',
        '',
        '<Callout type="warning" title="Keep a backup">',
        '',
        'Store the key safely.',
        '',
        '</Callout>',
        '',
        '<Cards>',
        '  <Card title="Install" href="/start">',
        '    Install Oore.',
        '  </Card>',
        '</Cards>',
        '',
        '```mdx',
        '<Callout>kept in code</Callout>',
        '```',
      ].join('\n'),
      description: 'A page.',
      title: 'Title',
      url: '/x',
    })
    expect(markdown).toStartWith(
      '# Title\n\n> A page.\n\nSource: https://docs.oore.build/x\n',
    )
    expect(markdown).not.toContain('import ')
    expect(markdown).toContain(
      '(https://docs.oore.build/build/pipelines/triggers)',
    )
    expect(markdown).toContain('(https://docs.oore.build/demo.png)')
    expect(markdown).toContain(
      '> **Warning: Keep a backup**\n>\n> Store the key safely.',
    )
    expect(markdown).toContain(
      '- [Install](https://docs.oore.build/start): Install Oore.',
    )
    expect(markdown).toContain('<Callout>kept in code</Callout>')
    expect(markdown).not.toContain('<Cards>')
  })
})

describe('OpenAPI Markdown', () => {
  const document: OpenAPIDocumentLike = {
    components: {
      schemas: {
        CreateBuildRequest: {
          properties: {
            branch: { type: ['string', 'null'] },
            pipeline_id: { description: 'Pipeline to build', type: 'string' },
          },
          required: ['pipeline_id'],
          type: 'object',
        },
      },
    },
    paths: {
      '/v1/projects/{project_id}/builds': {
        post: {
          operationId: 'create_build',
          parameters: [
            {
              in: 'path',
              name: 'project_id',
              required: true,
              schema: { type: 'string' },
            },
          ],
          requestBody: {
            content: {
              'application/json': {
                schema: { $ref: '#/components/schemas/CreateBuildRequest' },
              },
            },
            required: true,
          },
          responses: { '201': { description: 'Build queued' } },
          security: [{ bearer_auth: [] }],
          summary: 'Create build',
          tags: ['Builds'],
        },
      },
    },
  }
  const ref = {
    method: 'post',
    operation: document.paths['/v1/projects/{project_id}/builds']!
      .post as Record<string, unknown>,
    path: '/v1/projects/{project_id}/builds',
  }

  test('curl uses OORE_URL, OORE_TOKEN and a sample body', () => {
    expect(curlExample(document, ref)).toBe(
      [
        'curl -fsS -X POST "$OORE_URL/v1/projects/$PROJECT_ID/builds" \\',
        '  -H "Authorization: Bearer $OORE_TOKEN" \\',
        '  -H "Content-Type: application/json" \\',
        `  -d '{\n  "pipeline_id": "<string>"\n}'`,
      ].join('\n'),
    )
  })

  test('operation page lists parameters, body fields and responses', () => {
    const markdown = operationMarkdown(document, ref)
    expect(markdown).toContain(
      '- Request: `POST /v1/projects/{project_id}/builds`',
    )
    expect(markdown).toContain('| `project_id` | path | string | yes |')
    expect(markdown).toContain(
      '| `pipeline_id` | string | yes | Pipeline to build |',
    )
    expect(markdown).toContain('| `branch` | string \\| null | no |')
    expect(markdown).toContain('| 201 | Build queued |')
  })
})
