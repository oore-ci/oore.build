#!/usr/bin/env bun

import fs from 'node:fs'
import path from 'node:path'

const DEFAULT_LISTEN = process.env.OORE_WEB_LISTEN || '127.0.0.1:4173'
const DEFAULT_BACKEND_URL =
  process.env.OORE_WEB_BACKEND_URL || 'http://127.0.0.1:8787'
const DEFAULT_DIST_DIR =
  process.env.OORE_WEB_DIST_DIR ||
  path.resolve(path.dirname(process.execPath), '..', 'web-dist')

function printHelp() {
  console.log(`oore-web - local self-hosted oore.build frontend launcher

Usage:
  oore-web [--listen <host:port>] [--backend-url <url>] [--dist-dir <path>]

Options:
  --listen        Listen address (default: ${DEFAULT_LISTEN})
  --backend-url   Backend API base URL (default: ${DEFAULT_BACKEND_URL})
  --dist-dir      Path to web static assets (default: ${DEFAULT_DIST_DIR})
  --help          Show this help text
`)
}

function parseListen(raw) {
  const value = raw.trim()
  if (!value) throw new Error('listen value cannot be empty')

  if (value.startsWith('http://') || value.startsWith('https://')) {
    const parsed = new URL(value)
    const hostname = parsed.hostname || '127.0.0.1'
    const port = Number(parsed.port || 80)
    if (!Number.isInteger(port) || port <= 0 || port > 65535) {
      throw new Error(`invalid listen port: ${parsed.port}`)
    }
    return { hostname, port }
  }

  const lastColon = value.lastIndexOf(':')
  if (lastColon <= 0 || lastColon === value.length - 1) {
    throw new Error(`listen must be <host:port>, got: ${value}`)
  }

  const hostname = value.slice(0, lastColon)
  const port = Number(value.slice(lastColon + 1))
  if (!Number.isInteger(port) || port <= 0 || port > 65535) {
    throw new Error(`invalid listen port: ${value}`)
  }

  return { hostname, port }
}

function parseArgs(argv) {
  const config = {
    listen: DEFAULT_LISTEN,
    backendUrl: DEFAULT_BACKEND_URL,
    distDir: DEFAULT_DIST_DIR,
  }

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i]
    if (arg === '--help' || arg === '-h') {
      printHelp()
      process.exit(0)
    }

    if (arg === '--listen') {
      const value = argv[i + 1]
      if (!value) throw new Error('--listen requires a value')
      config.listen = value
      i += 1
      continue
    }

    if (arg === '--backend-url') {
      const value = argv[i + 1]
      if (!value) throw new Error('--backend-url requires a value')
      config.backendUrl = value
      i += 1
      continue
    }

    if (arg === '--dist-dir') {
      const value = argv[i + 1]
      if (!value) throw new Error('--dist-dir requires a value')
      config.distDir = value
      i += 1
      continue
    }

    throw new Error(`unknown argument: ${arg}`)
  }

  return config
}

function resolveAssetPath(distDir, pathname) {
  const decoded = decodeURIComponent(pathname)
  const stripped = decoded.replace(/^\/+/, '')
  const requested = stripped.length === 0 ? 'index.html' : stripped
  const candidate = path.resolve(distDir, requested)
  const root = path.resolve(distDir)
  if (candidate !== root && !candidate.startsWith(`${root}${path.sep}`)) {
    return null
  }
  return candidate
}

function fileExists(filePath) {
  try {
    const stat = fs.statSync(filePath)
    return stat.isFile()
  } catch {
    return false
  }
}

function isDirectory(filePath) {
  try {
    const stat = fs.statSync(filePath)
    return stat.isDirectory()
  } catch {
    return false
  }
}

function isApiPath(pathname) {
  return (
    pathname === '/healthz' || pathname === '/v1' || pathname.startsWith('/v1/')
  )
}

function withProxyHeader(response) {
  const headers = new Headers(response.headers)
  headers.set('x-oore-web-proxy', '1')
  return new Response(response.body, {
    status: response.status,
    statusText: response.statusText,
    headers,
  })
}

async function proxyRequest(request, backendUrl, url) {
  const upstream = new URL(`${url.pathname}${url.search}`, backendUrl)
  const headers = new Headers(request.headers)
  headers.delete('host')
  headers.delete('content-length')

  const init = {
    method: request.method,
    headers,
    redirect: 'manual',
  }

  if (request.method !== 'GET' && request.method !== 'HEAD') {
    init.body = request.body
  }

  try {
    const response = await fetch(upstream, init)
    return withProxyHeader(response)
  } catch (error) {
    return Response.json(
      {
        error: 'Backend unreachable from local web launcher',
        code: 'backend_unreachable',
        details: error instanceof Error ? error.message : 'request_failed',
      },
      { status: 502 },
    )
  }
}

function serveSpa(distDir, pathname, acceptHeader) {
  const assetPath = resolveAssetPath(distDir, pathname)
  if (!assetPath) {
    return new Response('Not found', { status: 404 })
  }

  if (isDirectory(assetPath)) {
    const indexPath = path.join(assetPath, 'index.html')
    if (fileExists(indexPath)) {
      return new Response(Bun.file(indexPath))
    }
  }

  if (fileExists(assetPath)) {
    return new Response(Bun.file(assetPath))
  }

  const wantsHtml =
    acceptHeader.includes('text/html') || acceptHeader.includes('*/*')
  if (wantsHtml) {
    const indexPath = path.join(distDir, 'index.html')
    if (!fileExists(indexPath)) {
      return new Response('index.html not found', { status: 500 })
    }
    return new Response(Bun.file(indexPath))
  }

  return new Response('Not found', { status: 404 })
}

function main() {
  let config
  try {
    config = parseArgs(process.argv.slice(2))
  } catch (error) {
    console.error(
      `[oore-web] ${error instanceof Error ? error.message : 'failed to parse args'}`,
    )
    printHelp()
    process.exit(2)
  }

  let backendUrl
  try {
    backendUrl = new URL(config.backendUrl)
  } catch {
    console.error(`[oore-web] invalid backend URL: ${config.backendUrl}`)
    process.exit(2)
  }

  let listen
  try {
    listen = parseListen(config.listen)
  } catch (error) {
    console.error(
      `[oore-web] ${error instanceof Error ? error.message : 'invalid listen'}`,
    )
    process.exit(2)
  }

  const distDir = path.resolve(config.distDir)
  const indexPath = path.join(distDir, 'index.html')
  if (!fileExists(indexPath)) {
    console.error(
      `[oore-web] missing web assets at ${indexPath}. Reinstall or set --dist-dir.`,
    )
    process.exit(2)
  }

  const server = Bun.serve({
    hostname: listen.hostname,
    port: listen.port,
    fetch: (request) => {
      const url = new URL(request.url)

      if (url.pathname === '/__oore_web_healthz') {
        return Response.json({
          ok: true,
          backend_url: backendUrl.toString(),
          dist_dir: distDir,
        })
      }

      if (isApiPath(url.pathname)) {
        return proxyRequest(request, backendUrl, url)
      }

      const acceptHeader = request.headers.get('accept') || ''
      return serveSpa(distDir, url.pathname, acceptHeader)
    },
  })

  console.log(
    `[oore-web] listening on http://${listen.hostname}:${listen.port} (backend: ${backendUrl.toString()})`,
  )

  const shutdown = () => {
    try {
      server.stop(true)
    } catch {
      // ignore
    }
    process.exit(0)
  }

  process.on('SIGINT', shutdown)
  process.on('SIGTERM', shutdown)
}

main()
