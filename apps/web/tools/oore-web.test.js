import { afterEach, describe, expect, it, vi } from 'vitest'
import path from 'node:path'
import { spawn } from 'node:child_process'
import { createServer } from 'node:http'

import {
  applyTrustedProxyHeaders,
  authorizeOwner,
  getWebUpdateStatus,
  isApiPath,
  spaCacheControl,
} from './oore-web.js'

const ooreWebPath = path.resolve(process.cwd(), 'tools/oore-web.js')

async function runStatus(url, json = false) {
  return await new Promise((resolve, reject) => {
    const child = spawn('bun', [
      ooreWebPath,
      'status',
      '--url',
      url,
      ...(json ? ['--json'] : []),
    ])
    let stdout = ''
    let stderr = ''
    child.stdout.on('data', (chunk) => (stdout += chunk))
    child.stderr.on('data', (chunk) => (stderr += chunk))
    child.once('error', reject)
    child.once('close', (exitCode) => resolve({ stdout, stderr, exitCode }))
  })
}

async function startServer(handler) {
  const server = createServer(handler)
  await new Promise((resolve, reject) => {
    server.once('error', reject)
    server.listen(0, '127.0.0.1', resolve)
  })
  return { server, url: `http://127.0.0.1:${server.address().port}` }
}

async function stopServer(server) {
  await new Promise((resolve) => server.close(resolve))
}

function sendJson(response, data, proxied = false) {
  response.setHeader('content-type', 'application/json')
  if (proxied) response.setHeader('x-oore-web-proxy', '1')
  response.end(JSON.stringify(data))
}

afterEach(() => vi.unstubAllGlobals())

describe('oore-web SPA caching', () => {
  it('caches hashed assets immutably and always revalidates HTML', () => {
    expect(spaCacheControl('/assets/index-abc123.js')).toBe(
      'public, max-age=31536000, immutable',
    )
    expect(spaCacheControl('/')).toBe('public, max-age=0, must-revalidate')
    expect(spaCacheControl('/index.html')).toBe(
      'public, max-age=0, must-revalidate',
    )
  })
})

describe('oore-web runtime release metadata', () => {
  it('returns the changelog and release URL with update availability', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        Response.json({
          schema_version: 1,
          channel: 'alpha',
          version: '1.2.3-alpha.2',
          tag: 'v1.2.3-alpha.2',
          release_name: 'Alpha 2',
          release_notes:
            '- Faster builds\n\n**Full Changelog**: https://github.com/oore-ci/oore.build/compare/v1.2.3-alpha.1...v1.2.3-alpha.2',
          release_url:
            'https://github.com/oore-ci/oore.build/releases/tag/v1.2.3-alpha.2',
          changelog_url:
            'https://github.com/oore-ci/oore.build/compare/v1.2.3-alpha.1...v1.2.3-alpha.2',
          download_base_url:
            'https://github.com/oore-ci/oore.build/releases/download/v1.2.3-alpha.2',
        }),
      ),
    )

    const status = await getWebUpdateStatus(
      { phase: 'idle', error: null },
      new URLSearchParams({
        current: '1.2.3-alpha.1',
        channel: 'alpha',
        repo: 'oore-ci/oore.build',
      }),
    )

    expect(status).toMatchObject({
      version: '1.2.3-alpha.1',
      latest_version: '1.2.3-alpha.2',
      update_available: true,
      release_name: 'Alpha 2',
      release_notes:
        '- Faster builds\n\n**Full Changelog**: https://github.com/oore-ci/oore.build/compare/v1.2.3-alpha.1...v1.2.3-alpha.2',
      release_url:
        'https://github.com/oore-ci/oore.build/releases/tag/v1.2.3-alpha.2',
      changelog_url:
        'https://github.com/oore-ci/oore.build/compare/v1.2.3-alpha.1...v1.2.3-alpha.2',
    })
    expect(fetch).toHaveBeenCalledWith(
      'https://releases.oore.build/latest/alpha.json',
      expect.objectContaining({
        headers: expect.objectContaining({ Accept: 'application/json' }),
      }),
    )
  })
})

describe('oore-web trusted proxy contract', () => {
  it('proxies the backend readiness endpoint', () => {
    expect(isApiPath('/readyz')).toBe(true)
  })

  it('proxies only the dedicated install route prefix', () => {
    expect(isApiPath('/install/ios/token/manifest.plist')).toBe(true)
    expect(isApiPath('/install/artifact/token')).toBe(true)
    expect(isApiPath('/installer')).toBe(false)
  })

  it('forwards identity only with valid upstream proof', () => {
    const config = {
      trustedProxySecret: 'backend-proof',
      upstreamTrustedProxySecret: 'upstream-proof',
      trustedProxyUserEmailHeader: 'x-warpgate-username',
      upstreamTrustedProxySecretHeader: 'x-oore-web-trusted-proxy-secret',
    }

    const unprovedRequest = new Request('https://ci.example.com/v1/auth', {
      headers: {
        'x-warpgate-username': 'attacker@example.com',
        'x-oore-web-trusted-proxy-secret': 'wrong-proof',
        'x-oore-trusted-proxy-secret': 'attacker-proof',
      },
    })
    const unprovedHeaders = new Headers(unprovedRequest.headers)
    applyTrustedProxyHeaders(unprovedRequest, unprovedHeaders, config)

    expect(unprovedHeaders.get('x-warpgate-username')).toBeNull()
    expect(unprovedHeaders.get('x-oore-web-trusted-proxy-secret')).toBeNull()
    expect(unprovedHeaders.get('x-oore-trusted-proxy-secret')).toBe(
      'backend-proof',
    )

    const provedRequest = new Request('https://ci.example.com/v1/auth', {
      headers: {
        'x-warpgate-username': 'owner@example.com',
        'x-oore-web-trusted-proxy-secret': 'upstream-proof',
      },
    })
    const provedHeaders = new Headers(provedRequest.headers)
    applyTrustedProxyHeaders(provedRequest, provedHeaders, config)

    expect(provedHeaders.get('x-warpgate-username')).toBe('owner@example.com')
    expect(provedHeaders.get('x-oore-web-trusted-proxy-secret')).toBeNull()
    expect(provedHeaders.get('x-oore-trusted-proxy-secret')).toBe(
      'backend-proof',
    )
  })

  it('allows runtime updates only for a backend-confirmed owner', async () => {
    const { server, url } = await startServer((_request, response) => {
      sendJson(response, { user: { role: 'owner' } })
    })
    try {
      const allowed = await authorizeOwner(
        new Request('https://ci.example.com/__oore_web_update', {
          headers: { authorization: 'Bearer session' },
        }),
        new URL(url),
        {
          trustedProxySecret: '',
          upstreamTrustedProxySecret: '',
          trustedProxyUserEmailHeader: 'x-oore-user-email',
          upstreamTrustedProxySecretHeader: 'x-oore-web-trusted-proxy-secret',
        },
        null,
      )
      expect(allowed).toBe(true)
    } finally {
      await stopServer(server)
    }
  })
})

describe('oore-web status', () => {
  it('reports frontend and backend versions with dependency readiness', async () => {
    const { server, url } = await startServer((request, response) => {
      if (request.url === '/__oore_web_healthz') {
        sendJson(response, {
          ok: true,
          version: 'web-1',
          channel: 'alpha',
          backend_url: 'http://secret-backend',
        })
        return
      }
      if (request.url === '/healthz') {
        sendJson(
          response,
          { ok: true, version: 'backend-2', channel: 'beta' },
          true,
        )
        return
      }
      if (request.url === '/readyz') {
        sendJson(
          response,
          {
            ok: true,
            database: true,
            migrations: true,
            encryption: true,
          },
          true,
        )
        return
      }
      response.statusCode = 404
      response.end('not found')
    })

    try {
      const result = await runStatus(url, true)
      const report = JSON.parse(result.stdout)

      expect(result.exitCode).toBe(0)
      expect(result.stderr).toBe('')
      expect(report).toMatchObject({
        ok: true,
        frontend: { ok: true, version: 'web-1', channel: 'alpha' },
        backend: {
          ok: true,
          version: 'backend-2',
          channel: 'beta',
          ready: true,
          checks: { database: true, migrations: true, encryption: true },
        },
      })
      expect(result.stdout).not.toContain('secret-backend')
    } finally {
      await stopServer(server)
    }
  })

  it('reports an actionable frontend connection failure', async () => {
    const { server, url } = await startServer((_request, response) =>
      response.end(),
    )
    await stopServer(server)

    const result = await runStatus(url)

    expect(result.exitCode).toBe(1)
    expect(result.stdout).toContain('Frontend: failed')
    expect(result.stdout).toContain(
      'Check that oore-web is running and --url is correct.',
    )
    expect(result.stdout).toContain('Backend:  skipped')
    expect(result.stderr).toBe('')
  })
})
