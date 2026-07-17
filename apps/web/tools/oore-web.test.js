import { afterEach, describe, expect, it, vi } from 'vitest'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { spawn } from 'node:child_process'
import { createServer } from 'node:http'

import {
  applyTrustedProxyHeaders,
  authorizeOwner,
  candidateValidationArgs,
  getWebUpdateStatus,
  installUpdateCandidate,
  isApiPath,
  parseBackendUrl,
  parseListen,
  parseServeArgs,
  spaCacheControl,
  spaResponseHeaders,
} from './oore-web.js'

const ooreWebPath = path.resolve(process.cwd(), 'tools/oore-web.js')

async function runOoreWeb(args) {
  return await new Promise((resolve, reject) => {
    const child = spawn('bun', [ooreWebPath, ...args])
    let stdout = ''
    let stderr = ''
    child.stdout.on('data', (chunk) => (stdout += chunk))
    child.stderr.on('data', (chunk) => (stderr += chunk))
    child.once('error', reject)
    child.once('close', (exitCode) => resolve({ stdout, stderr, exitCode }))
  })
}

async function runStatus(url, json = false) {
  return runOoreWeb(['status', '--url', url, ...(json ? ['--json'] : [])])
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

  it('denies framing without changing cache behavior', () => {
    const headers = new Headers(spaResponseHeaders('/'))

    expect(headers.get('cache-control')).toBe(
      'public, max-age=0, must-revalidate',
    )
    expect(headers.get('content-security-policy')).toContain(
      "frame-ancestors 'none'",
    )
    expect(headers.get('x-frame-options')).toBe('DENY')
  })
})

describe('oore-web launcher security policy', () => {
  it('rejects literal trusted-proxy proofs and keeps file inputs', async () => {
    for (const [flag, replacement] of [
      ['--trusted-proxy-secret', '--trusted-proxy-secret-file'],
      [
        '--upstream-trusted-proxy-secret',
        '--upstream-trusted-proxy-secret-file',
      ],
    ]) {
      expect(() => parseServeArgs([flag, 'PLACEHOLDER_PROOF'])).toThrow(
        replacement,
      )
    }

    const help = await runOoreWeb(['--help'])
    expect(help.exitCode).toBe(0)
    expect(help.stdout).not.toMatch(/--(?:upstream-)?trusted-proxy-secret\s/)

    const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'oore-web-test-'))
    const backendProof = path.join(tempDir, 'backend-proof')
    const upstreamProof = path.join(tempDir, 'upstream-proof')
    try {
      fs.writeFileSync(backendProof, 'backend-proof\n', { mode: 0o600 })
      fs.writeFileSync(upstreamProof, 'upstream-proof\n', { mode: 0o600 })
      const config = parseServeArgs([
        '--trusted-proxy-secret-file',
        backendProof,
        '--upstream-trusted-proxy-secret-file',
        upstreamProof,
      ])
      expect(config.trustedProxySecret).toBe('backend-proof')
      expect(config.upstreamTrustedProxySecret).toBe('upstream-proof')
    } finally {
      fs.rmSync(tempDir, { recursive: true, force: true })
    }
  })

  it('requires HTTPS or an explicit protected transport for remote backends', () => {
    expect(parseBackendUrl('https://backend.example.test').protocol).toBe(
      'https:',
    )
    for (const raw of [
      'http://localhost:8787',
      'http://127.23.45.67:8787',
      'http://[::1]:8787',
    ]) {
      expect(parseBackendUrl(raw).protocol).toBe('http:')
    }
    for (const raw of [
      'http://192.0.2.10:8787',
      'http://10.0.0.20:8787',
      'http://backend.example.test:8787',
    ]) {
      expect(() => parseBackendUrl(raw)).toThrow(
        '--backend-transport-protected',
      )
    }
    expect(parseBackendUrl('http://192.0.2.10:8787', true).protocol).toBe(
      'http:',
    )
    expect(() => parseBackendUrl('ftp://backend.example.test', true)).toThrow(
      'http or https',
    )
  })

  it('requires explicit protected ingress for non-loopback HTTP listeners', () => {
    expect(parseListen('127.0.0.1:4173')).toEqual({
      hostname: '127.0.0.1',
      port: 4173,
    })
    expect(parseListen('[::1]:4173')).toEqual({
      hostname: '::1',
      port: 4173,
    })
    for (const raw of [
      '0.0.0.0:4173',
      '[::]:4173',
      '192.0.2.10:4173',
      'web.example.test:4173',
    ]) {
      expect(() => parseListen(raw)).toThrow('--browser-transport-protected')
    }
    expect(parseListen('192.0.2.10:4173', true)).toEqual({
      hostname: '192.0.2.10',
      port: 4173,
    })
    expect(() => parseListen('https://192.0.2.10:4173', true)).toThrow(
      'does not terminate TLS',
    )
  })

  it('preflights an update candidate with the active transport assertions', async () => {
    const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'oore-web-test-'))
    try {
      fs.writeFileSync(path.join(tempDir, 'index.html'), 'ok')
      const config = parseServeArgs([
        '--listen',
        '100.107.193.2:4174',
        '--backend-url',
        'http://100.107.193.1:8787',
        '--browser-transport-protected',
        '--backend-transport-protected',
        '--dist-dir',
        tempDir,
      ])
      const args = candidateValidationArgs(config, tempDir)
      const accepted = await runOoreWeb(args)
      const rejected = await runOoreWeb(
        args.filter((arg) => arg !== '--backend-transport-protected'),
      )
      const rejectedBrowser = await runOoreWeb(
        args.filter((arg) => arg !== '--browser-transport-protected'),
      )

      expect(accepted.exitCode).toBe(0)
      expect(accepted.stderr).toBe('')
      expect(rejected.exitCode).toBe(2)
      expect(rejected.stderr).toContain('--backend-transport-protected')
      expect(rejectedBrowser.exitCode).toBe(2)
      expect(rejectedBrowser.stderr).toContain('--browser-transport-protected')
    } finally {
      fs.rmSync(tempDir, { recursive: true, force: true })
    }
  })

  it('rejects an incompatible extracted candidate before replacing live files', () => {
    const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'oore-web-test-'))
    const installRoot = path.join(tempDir, 'install')
    const extractRoot = path.join(tempDir, 'extract')
    const extractedBinary = path.join(extractRoot, 'bin', 'oore-web')
    const extractedDist = path.join(extractRoot, 'web-dist')
    const extractedVersion = path.join(extractRoot, 'VERSION')
    try {
      fs.mkdirSync(path.join(installRoot, 'bin'), { recursive: true })
      fs.mkdirSync(path.join(installRoot, 'web-dist'), { recursive: true })
      fs.mkdirSync(path.dirname(extractedBinary), { recursive: true })
      fs.mkdirSync(extractedDist, { recursive: true })
      fs.writeFileSync(path.join(installRoot, 'bin', 'oore-web'), 'live-binary')
      fs.writeFileSync(path.join(installRoot, 'web-dist', 'index.html'), 'live')
      fs.writeFileSync(path.join(installRoot, 'VERSION'), '1.0.0\n')
      fs.writeFileSync(extractedBinary, '#!/bin/sh\nexit 23\n', { mode: 0o755 })
      fs.writeFileSync(path.join(extractedDist, 'index.html'), 'candidate')
      fs.writeFileSync(extractedVersion, '1.1.0\n')

      const activeConfig = parseServeArgs([
        '--listen',
        '100.107.193.2:4174',
        '--backend-url',
        'http://100.107.193.1:8787',
        '--browser-transport-protected',
        '--backend-transport-protected',
        '--dist-dir',
        path.join(installRoot, 'web-dist'),
      ])

      expect(() =>
        installUpdateCandidate({
          installRoot,
          extractedBinary,
          extractedDist,
          extractedVersion,
          extractedLicense: path.join(extractRoot, 'LICENSE'),
          channel: 'alpha',
          repo: 'oore-ci/oore.build',
          activeConfig,
        }),
      ).toThrow('candidate launcher rejected the active service configuration')
      expect(
        fs.readFileSync(path.join(installRoot, 'bin', 'oore-web'), 'utf8'),
      ).toBe('live-binary')
      expect(
        fs.readFileSync(
          path.join(installRoot, 'web-dist', 'index.html'),
          'utf8',
        ),
      ).toBe('live')
      expect(fs.readFileSync(path.join(installRoot, 'VERSION'), 'utf8')).toBe(
        '1.0.0\n',
      )
      expect(fs.readdirSync(path.join(installRoot, 'bin'))).toEqual([
        'oore-web',
      ])
    } finally {
      fs.rmSync(tempDir, { recursive: true, force: true })
    }
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
  it('reports an HTML proxy outage by its HTTP status', async () => {
    const { server, url } = await startServer((_request, response) => {
      response.statusCode = 503
      response.setHeader('content-type', 'text/html')
      response.end('No server is available to handle this request.')
    })

    try {
      const result = await runStatus(url)

      expect(result.exitCode).toBe(1)
      expect(result.stdout).toContain('Frontend check failed (HTTP 503)')
      expect(result.stdout).not.toContain('invalid JSON')
    } finally {
      await stopServer(server)
    }
  })

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
