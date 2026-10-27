// Cloudflare Pages runs this for /assets/* only (see _routes.json).
//
// Pages answers a request for a missing file with index.html, and _headers
// caches everything under /assets/ for a year. A browser that asked for a
// chunk while a deploy was still rolling out kept that HTML page under the
// chunk's name and could never load the app again until its site data was
// cleared. Missing assets get an uncached 404 instead.
const SECURITY_HEADERS = {
  'X-Content-Type-Options': 'nosniff',
  'X-Frame-Options': 'DENY',
  'Referrer-Policy': 'strict-origin-when-cross-origin',
  'Permissions-Policy': 'camera=(), microphone=(), geolocation=()',
  'Content-Security-Policy':
    "base-uri 'self'; object-src 'none'; frame-ancestors 'none'",
}

export default {
  async fetch(request, env) {
    const response = await env.ASSETS.fetch(request)
    const contentType = response.headers.get('Content-Type') ?? ''
    if (response.ok && !contentType.startsWith('text/html')) {
      return response
    }
    return new Response('Not found', {
      status: 404,
      headers: {
        ...SECURITY_HEADERS,
        'Cache-Control': 'no-store',
        'Content-Type': 'text/plain; charset=utf-8',
      },
    })
  },
}
