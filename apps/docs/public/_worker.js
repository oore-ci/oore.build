// Cloudflare Pages advanced-mode worker for docs.oore.build.
//
// It serves each page's Markdown twin (`<path>.md`, built by
// src/pages/[...slug].md.ts) to a request that sends `Accept: text/markdown`,
// and passes everything else to the static assets. A worker takes over every
// request, so it also applies the redirects and headers that `_redirects` and
// `_headers` describe. tools/markdown-worker.test.js keeps them in step.

// Keep in step with public/_redirects.
const REDIRECTS = {
  '/start/first-build': '/start/android/first-build',
  '/operate/deploy': '/concepts/which-setup',
  '/operate/known-limitations': '/reference/known-limitations',
  '/understand': '/concepts',
  '/understand/architecture': '/concepts/architecture',
  '/understand/artifact-delivery': '/concepts/artifact-delivery',
  '/understand/build-lifecycle': '/concepts/build-lifecycle',
  '/understand/multiple-instances': '/concepts/multiple-instances',
  '/understand/pipeline-configuration': '/concepts/pipeline-configuration',
  '/understand/runner-trust': '/concepts/runner-trust',
  '/understand/security': '/concepts/security',
  '/understand/signing': '/concepts/signing',
}

// Keep in step with public/_headers.
const SECURITY_HEADERS = {
  'content-security-policy':
    "base-uri 'self'; object-src 'none'; frame-ancestors 'none'",
  'permissions-policy': 'camera=(), microphone=(), geolocation=()',
  'referrer-policy': 'strict-origin-when-cross-origin',
  'x-content-type-options': 'nosniff',
  'x-frame-options': 'DENY',
}

function acceptsMarkdown(header) {
  if (!header) return false
  return header.split(',').some((entry) => {
    const [mediaType, ...parameters] = entry.split(';')
    if (mediaType.trim().toLowerCase() !== 'text/markdown') return false
    const quality = parameters
      .map((parameter) => parameter.trim().toLowerCase())
      .find((parameter) => parameter.startsWith('q='))
    return quality ? Number(quality.slice(2)) > 0 : true
  })
}

function appendVary(headers, value) {
  const values = new Set(
    (headers.get('vary') ?? '')
      .split(',')
      .map((entry) => entry.trim())
      .filter(Boolean),
  )
  values.add(value)
  headers.set('vary', [...values].join(', '))
}

/** The Markdown asset for a page path, or undefined for non-page paths. */
function markdownPath(pathname) {
  if (pathname === '/' || pathname === '/index' || pathname === '/index.html') {
    return '/index.md'
  }
  const clean = pathname.replace(/\/+$/, '').replace(/\.html$/, '')
  const last = clean.split('/').pop() ?? ''
  // Paths with an extension are files (images, openapi.json, llms.txt).
  if (last.includes('.') || clean.startsWith('/_astro/')) return undefined
  return `${clean}.md`
}

function withDocsHeaders(response, url) {
  const headers = new Headers(response.headers)
  for (const [name, value] of Object.entries(SECURITY_HEADERS)) {
    headers.set(name, value)
  }
  const path = url.pathname
  if (path === '/' || path === '/index.html') {
    headers.set('cache-control', 'public, max-age=0, must-revalidate')
  } else if (path.startsWith('/_astro/')) {
    headers.set('cache-control', 'public, max-age=31536000, immutable')
  } else if (path === '/api/search') {
    headers.set('content-type', 'application/json; charset=utf-8')
  }
  if (path.endsWith('.md')) {
    headers.set('content-type', 'text/markdown; charset=utf-8')
  } else if (path === '/llms.txt' || path === '/llms-full.txt') {
    headers.set('content-type', 'text/plain; charset=utf-8')
    headers.set('cache-control', 'public, max-age=300, must-revalidate')
  }
  return new Response(response.body, {
    status: response.status,
    statusText: response.statusText,
    headers,
  })
}

export { acceptsMarkdown, markdownPath, REDIRECTS, SECURITY_HEADERS }

export default {
  async fetch(request, env) {
    const url = new URL(request.url)

    const redirect = REDIRECTS[url.pathname.replace(/\/+$/, '') || '/']
    if (redirect) {
      return withDocsHeaders(
        new Response(null, {
          status: 301,
          headers: { location: new URL(redirect + url.search, url).toString() },
        }),
        url,
      )
    }

    const markdown = markdownPath(url.pathname)
    if (markdown && acceptsMarkdown(request.headers.get('accept'))) {
      const asset = await env.ASSETS.fetch(
        new Request(new URL(markdown, url), request),
      )
      if (asset.ok) {
        const body = request.method === 'HEAD' ? '' : await asset.text()
        const headers = new Headers(asset.headers)
        headers.set('content-type', 'text/markdown; charset=utf-8')
        if (body) {
          headers.set('x-markdown-tokens', String(Math.ceil(body.length / 4)))
        }
        appendVary(headers, 'Accept')
        return withDocsHeaders(
          new Response(body, { status: 200, headers }),
          url,
        )
      }
    }

    const response = await env.ASSETS.fetch(request)
    if (!markdown) return withDocsHeaders(response, url)
    const headers = new Headers(response.headers)
    appendVary(headers, 'Accept')
    if (response.ok) {
      headers.append(
        'link',
        `<${markdown}>; rel="alternate"; type="text/markdown"`,
      )
    }
    return withDocsHeaders(
      new Response(response.body, {
        status: response.status,
        statusText: response.statusText,
        headers,
      }),
      url,
    )
  },
}
