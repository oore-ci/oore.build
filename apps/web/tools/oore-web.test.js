import { describe, expect, it } from 'vitest'
import path from 'node:path'
import { spawn } from 'node:child_process'
import { createServer } from 'node:http'

import {
  applyTrustedProxyHeaders,
  authorizeOwner,
  isApiPath,
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

describe('oore-web trusted proxy contract', () => {
  it('proxies the backend readiness endpoint', () => {
    expect(isApiPath('/readyz')).toBe(true)
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
