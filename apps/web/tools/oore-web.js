#!/usr/bin/env bun

import fs from 'node:fs'
import crypto from 'node:crypto'
import net from 'node:net'
import os from 'node:os'
import path from 'node:path'
import { spawnSync } from 'node:child_process'

const DEFAULT_LISTEN = process.env.OORE_WEB_LISTEN || '127.0.0.1:4173'
const DEFAULT_BACKEND_URL =
  process.env.OORE_WEB_BACKEND_URL || 'http://127.0.0.1:8787'
const DEFAULT_TRUSTED_PROXY_SECRET =
  process.env.OORE_TRUSTED_PROXY_SHARED_SECRET ||
  process.env.OORE_WEB_TRUSTED_PROXY_SHARED_SECRET ||
  ''
const DEFAULT_TRUSTED_PROXY_SECRET_FILE =
  process.env.OORE_TRUSTED_PROXY_SHARED_SECRET_FILE ||
  process.env.OORE_WEB_TRUSTED_PROXY_SHARED_SECRET_FILE ||
  ''
const DEFAULT_TRUSTED_PROXY_USER_EMAIL_HEADER =
  process.env.OORE_WEB_TRUSTED_PROXY_USER_EMAIL_HEADER ||
  process.env.OORE_SETUP_USER_EMAIL_HEADER ||
  'x-oore-user-email'
const DEFAULT_UPSTREAM_TRUSTED_PROXY_SECRET =
  process.env.OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET || ''
const DEFAULT_UPSTREAM_TRUSTED_PROXY_SECRET_FILE =
  process.env.OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE || ''
const DEFAULT_UPSTREAM_TRUSTED_PROXY_SECRET_HEADER =
  process.env.OORE_WEB_UPSTREAM_TRUSTED_PROXY_SECRET_HEADER ||
  'x-oore-web-trusted-proxy-secret'
const DEFAULT_DIST_DIR =
  process.env.OORE_WEB_DIST_DIR ||
  path.resolve(path.dirname(process.execPath), '..', 'web-dist')
const DEFAULT_GITHUB_REPO = 'oore-ci/oore.build'
const LEGACY_GITHUB_REPO = 'devaryakjha/oore.build'
const DEFAULT_RELEASE_INDEX_BASE_URL = 'https://releases.oore.build'
const BACKEND_TRUSTED_PROXY_SECRET_HEADER = 'x-oore-trusted-proxy-secret'
const CLIENT_CONTROLLED_IDENTITY_HEADERS = [
  'x-oore-user-email',
  'x-warpgate-username',
  'x-auth-request-email',
  'x-auth-request-user',
  'x-forwarded-email',
  'x-forwarded-user',
  'remote-user',
]

function printHelp() {
  console.log(`oore-web - local self-hosted Oore CI frontend launcher

Usage:
  oore-web [serve] [--listen <host:port>] [--backend-url <url>] [--dist-dir <path>]
  oore-web status [--url <frontend-url>] [--json]
  oore-web update [--channel stable|beta|alpha] [--repo owner/name] [--check] [--force]
  oore-web version

Options:
  --listen        Listen address (default: ${DEFAULT_LISTEN})
  --backend-url   Backend API base URL (default: ${DEFAULT_BACKEND_URL})
  --dist-dir      Path to web static assets (default: ${DEFAULT_DIST_DIR})
  --browser-transport-protected
                  Assert encrypted ingress before a non-loopback HTTP listen
  --backend-transport-protected
                  Assert an encrypted transport protects a remote HTTP backend
  --trusted-proxy-secret-file
                  File containing the backend trusted-proxy secret
  --trusted-proxy-user-email-header
                  Identity header to forward after upstream proof (default: ${DEFAULT_TRUSTED_PROXY_USER_EMAIL_HEADER})
  --upstream-trusted-proxy-secret-file
                  File containing the upstream auth-proxy secret
  --upstream-trusted-proxy-secret-header
                  Header carrying the upstream proof (default: ${DEFAULT_UPSTREAM_TRUSTED_PROXY_SECRET_HEADER})
  --help          Show this help text
`)
}

function printStatusHelp() {
  console.log(`oore-web status - check frontend and backend readiness

Usage:
  oore-web status [--url <frontend-url>] [--json]

Options:
  --url    Frontend URL (default: derived from ${DEFAULT_LISTEN})
  --json   Print machine-readable output
  --help   Show this help text
`)
}

function readSecretFile(rawPath, label) {
  const filePath = rawPath.trim()
  if (!filePath) return ''
  const value = fs.readFileSync(filePath, 'utf8').trim()
  if (!value) throw new Error(`${label} file is empty: ${filePath}`)
  return value
}

function resolveSecret(value, filePath, label) {
  const inline = value.trim()
  if (inline) return inline
  return readSecretFile(filePath, label)
}

function normalizeHeaderName(raw, label) {
  const value = raw.trim().toLowerCase()
  const valid =
    value.length > 0 &&
    value.length <= 128 &&
    /^[!#$%&'*+\-.^_`|~0-9a-z]+$/.test(value)
  if (!valid) throw new Error(`${label} is not a valid HTTP header name`)
  return value
}

function timingSafeStringEqual(a, b) {
  const left = Buffer.from(a)
  const right = Buffer.from(b)
  return left.length === right.length && crypto.timingSafeEqual(left, right)
}

function printUpdateHelp() {
  console.log(`oore-web update - update the frontend launcher and web assets

Usage:
  oore-web update [--channel stable|beta|alpha] [--repo owner/name] [--check] [--force]

Options:
  --channel   Release channel. Defaults to installed CHANNEL, then current VERSION.
  --repo      GitHub repo. Defaults to installed GITHUB_REPO, then ${DEFAULT_GITHUB_REPO}.
  --check     Only print whether an update is available.
  --force     Reinstall the latest release even if already current.
  --help      Show this help text
`)
}

function normalizeHostname(raw) {
  const hostname = raw.toLowerCase()
  return hostname.startsWith('[') && hostname.endsWith(']')
    ? hostname.slice(1, -1)
    : hostname
}

function isLiteralLoopback(raw) {
  const hostname = normalizeHostname(raw)
  return (
    (net.isIP(hostname) === 4 && hostname.startsWith('127.')) ||
    hostname === '::1'
  )
}

function parseListenAddress(raw) {
  const value = raw.trim()
  if (!value) throw new Error('listen value cannot be empty')

  const scheme = value.match(/^([a-z][a-z0-9+.-]*):\/\//i)?.[1]?.toLowerCase()
  if (scheme === 'https') {
    throw new Error(
      '--listen does not terminate TLS; use a loopback listener behind HTTPS',
    )
  }
  if (scheme && scheme !== 'http') {
    throw new Error('--listen URL must use http')
  }

  if (scheme === 'http') {
    const parsed = new URL(value)
    const hostname = normalizeHostname(parsed.hostname || '127.0.0.1')
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

  const hostname = normalizeHostname(value.slice(0, lastColon))
  const port = Number(value.slice(lastColon + 1))
  if (!Number.isInteger(port) || port <= 0 || port > 65535) {
    throw new Error(`invalid listen port: ${value}`)
  }

  return { hostname, port }
}

export function parseListen(raw, protectedTransport = false) {
  const listen = parseListenAddress(raw)
  if (!isLiteralLoopback(listen.hostname) && !protectedTransport) {
    throw new Error(
      'non-loopback HTTP listen requires --browser-transport-protected after encrypted ingress is configured',
    )
  }
  return listen
}

export function parseBackendUrl(raw, protectedTransport = false) {
  const url = new URL(raw)
  if (!['http:', 'https:'].includes(url.protocol)) {
    throw new Error('--backend-url must use http or https')
  }

  const hostname = normalizeHostname(url.hostname)
  const loopback = hostname === 'localhost' || isLiteralLoopback(hostname)
  if (url.protocol === 'http:' && !loopback && !protectedTransport) {
    throw new Error(
      'non-loopback HTTP backend requires https or --backend-transport-protected after an encrypted transport is configured',
    )
  }
  return url
}

function defaultStatusUrl() {
  const { hostname, port } = parseListenAddress(DEFAULT_LISTEN)
  const host = hostname.includes(':') ? `[${hostname}]` : hostname
  return `http://${host}:${port}`
}

function parseStatusArgs(argv) {
  const config = { url: defaultStatusUrl(), json: false }

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i]
    if (arg === '--help' || arg === '-h') {
      printStatusHelp()
      process.exit(0)
    }
    if (arg === '--url') {
      const value = argv[i + 1]
      if (!value) throw new Error('--url requires a value')
      config.url = value
      i += 1
      continue
    }
    if (arg === '--json') {
      config.json = true
      continue
    }
    throw new Error(`unknown status argument: ${arg}`)
  }

  const url = new URL(config.url)
  if (!['http:', 'https:'].includes(url.protocol)) {
    throw new Error('--url must use http or https')
  }
  if (url.username || url.password) {
    throw new Error('--url must not include credentials')
  }
  config.url = url.origin
  return config
}

export function parseServeArgs(argv) {
  const config = {
    listen: DEFAULT_LISTEN,
    backendUrl: DEFAULT_BACKEND_URL,
    distDir: DEFAULT_DIST_DIR,
    browserTransportProtected: false,
    backendTransportProtected: false,
    trustedProxySecret: DEFAULT_TRUSTED_PROXY_SECRET,
    trustedProxySecretFile: DEFAULT_TRUSTED_PROXY_SECRET_FILE,
    trustedProxyUserEmailHeader: DEFAULT_TRUSTED_PROXY_USER_EMAIL_HEADER,
    upstreamTrustedProxySecret: DEFAULT_UPSTREAM_TRUSTED_PROXY_SECRET,
    upstreamTrustedProxySecretFile: DEFAULT_UPSTREAM_TRUSTED_PROXY_SECRET_FILE,
    upstreamTrustedProxySecretHeader:
      DEFAULT_UPSTREAM_TRUSTED_PROXY_SECRET_HEADER,
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

    if (arg === '--browser-transport-protected') {
      config.browserTransportProtected = true
      continue
    }

    if (arg === '--backend-transport-protected') {
      config.backendTransportProtected = true
      continue
    }

    if (
      arg === '--trusted-proxy-secret' ||
      arg === '--upstream-trusted-proxy-secret'
    ) {
      throw new Error(
        `${arg} is disabled because process arguments are observable; use ${arg}-file`,
      )
    }

    if (arg === '--trusted-proxy-secret-file') {
      const value = argv[i + 1]
      if (!value)
        throw new Error('--trusted-proxy-secret-file requires a value')
      config.trustedProxySecretFile = value
      i += 1
      continue
    }

    if (arg === '--trusted-proxy-user-email-header') {
      const value = argv[i + 1]
      if (!value)
        throw new Error('--trusted-proxy-user-email-header requires a value')
      config.trustedProxyUserEmailHeader = value
      i += 1
      continue
    }

    if (arg === '--upstream-trusted-proxy-secret-file') {
      const value = argv[i + 1]
      if (!value)
        throw new Error('--upstream-trusted-proxy-secret-file requires a value')
      config.upstreamTrustedProxySecretFile = value
      i += 1
      continue
    }

    if (arg === '--upstream-trusted-proxy-secret-header') {
      const value = argv[i + 1]
      if (!value)
        throw new Error(
          '--upstream-trusted-proxy-secret-header requires a value',
        )
      config.upstreamTrustedProxySecretHeader = value
      i += 1
      continue
    }

    throw new Error(`unknown argument: ${arg}`)
  }

  config.trustedProxySecret = resolveSecret(
    config.trustedProxySecret,
    config.trustedProxySecretFile,
    'trusted proxy secret',
  )
  config.upstreamTrustedProxySecret = resolveSecret(
    config.upstreamTrustedProxySecret,
    config.upstreamTrustedProxySecretFile,
    'upstream trusted proxy secret',
  )
  config.trustedProxyUserEmailHeader = normalizeHeaderName(
    config.trustedProxyUserEmailHeader,
    'trusted proxy user email header',
  )
  config.upstreamTrustedProxySecretHeader = normalizeHeaderName(
    config.upstreamTrustedProxySecretHeader,
    'upstream trusted proxy secret header',
  )

  return config
}

export function validateServeConfig(config) {
  const backendUrl = parseBackendUrl(
    config.backendUrl,
    config.backendTransportProtected,
  )
  const listen = parseListen(config.listen, config.browserTransportProtected)
  const distDir = path.resolve(config.distDir)
  const indexPath = path.join(distDir, 'index.html')
  if (!fileExists(indexPath)) {
    throw new Error(
      `missing web assets at ${indexPath}. Reinstall or set --dist-dir.`,
    )
  }
  return { backendUrl, listen, distDir }
}

export function candidateValidationArgs(config, distDir) {
  const args = [
    'validate-config',
    '--listen',
    config.listen,
    '--backend-url',
    config.backendUrl,
    '--dist-dir',
    distDir,
    '--trusted-proxy-user-email-header',
    config.trustedProxyUserEmailHeader,
    '--upstream-trusted-proxy-secret-header',
    config.upstreamTrustedProxySecretHeader,
  ]
  if (config.browserTransportProtected) {
    args.push('--browser-transport-protected')
  }
  if (config.backendTransportProtected) {
    args.push('--backend-transport-protected')
  }
  if (config.trustedProxySecretFile) {
    args.push('--trusted-proxy-secret-file', config.trustedProxySecretFile)
  }
  if (config.upstreamTrustedProxySecretFile) {
    args.push(
      '--upstream-trusted-proxy-secret-file',
      config.upstreamTrustedProxySecretFile,
    )
  }
  return args
}

function parseUpdateArgs(argv) {
  const config = {
    channel: process.env.OORE_CHANNEL || '',
    repo: process.env.OORE_GITHUB_REPO || '',
    check: false,
    force: false,
  }

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i]
    if (arg === '--help' || arg === '-h') {
      printUpdateHelp()
      process.exit(0)
    }

    if (arg === '--channel') {
      const value = argv[i + 1]
      if (!value) throw new Error('--channel requires a value')
      config.channel = value
      i += 1
      continue
    }

    if (arg === '--repo') {
      const value = argv[i + 1]
      if (!value) throw new Error('--repo requires a value')
      config.repo = value
      i += 1
      continue
    }

    if (arg === '--check') {
      config.check = true
      continue
    }

    if (arg === '--force') {
      config.force = true
      continue
    }

    throw new Error(`unknown update argument: ${arg}`)
  }

  return config
}

function parseCommand(argv) {
  const first = argv[0]
  if (!first || first.startsWith('-')) {
    return { command: 'serve', args: argv }
  }

  if (first === '--version' || first === '-V') {
    return { command: 'version', args: [] }
  }

  return { command: first, args: argv.slice(1) }
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

function readTrimmedFile(filePath) {
  try {
    const value = fs.readFileSync(filePath, 'utf8').trim()
    return value || null
  } catch {
    return null
  }
}

function resolveInstallRoot() {
  const envRoot = process.env.OORE_INSTALL_ROOT?.trim()
  if (envRoot) return path.resolve(envRoot)

  const binDir = path.dirname(process.execPath)
  if (path.basename(binDir) === 'bin') {
    return path.dirname(binDir)
  }

  return path.join(os.homedir(), '.oore')
}

function parseChannel(raw) {
  const value = raw.trim().toLowerCase()
  if (value === 'stable' || value === 'prod' || value === 'production') {
    return 'stable'
  }
  if (value === 'beta') return 'beta'
  if (value === 'alpha') return 'alpha'
  throw new Error(`invalid channel '${raw}', expected: stable|beta|alpha`)
}

function parseVersion(raw) {
  const match = raw
    .trim()
    .match(/^v?(\d+)\.(\d+)\.(\d+)(?:-(alpha|beta)\.(\d+))?$/)
  if (!match) throw new Error(`invalid version: ${raw}`)
  return {
    major: Number(match[1]),
    minor: Number(match[2]),
    patch: Number(match[3]),
    pre: match[4] || '',
    preNumber: match[5] ? Number(match[5]) : 0,
    raw: raw.trim().replace(/^v/, ''),
  }
}

function compareVersions(a, b) {
  for (const key of ['major', 'minor', 'patch']) {
    if (a[key] !== b[key]) return a[key] - b[key]
  }

  if (a.pre === b.pre) return a.preNumber - b.preNumber
  if (!a.pre) return 1
  if (!b.pre) return -1
  if (a.pre === 'beta' && b.pre === 'alpha') return 1
  if (a.pre === 'alpha' && b.pre === 'beta') return -1
  return a.pre.localeCompare(b.pre)
}

function inferChannelFromVersion(version) {
  if (version.pre === 'alpha') return 'alpha'
  if (version.pre === 'beta') return 'beta'
  return 'stable'
}

function normalizeGitHubRepo(repo) {
  const value = repo.trim()
  return value === LEGACY_GITHUB_REPO ? DEFAULT_GITHUB_REPO : value
}

function githubHeaders() {
  const headers = {
    Accept: 'application/vnd.github+json',
    'X-GitHub-Api-Version': '2022-11-28',
    'User-Agent': `oore-web/${readInstalledVersion(resolveInstallRoot()) || 'unknown'}/update`,
  }
  const token =
    process.env.OORE_GITHUB_TOKEN ||
    process.env.GITHUB_TOKEN ||
    process.env.GH_TOKEN
  if (token?.trim()) {
    headers.Authorization = `Bearer ${token.trim()}`
  }
  return headers
}

async function fetchReleaseManifest(channel, repo) {
  const baseUrl = (
    process.env.OORE_RELEASE_INDEX_BASE_URL || DEFAULT_RELEASE_INDEX_BASE_URL
  ).replace(/\/$/, '')
  const url = `${baseUrl}/latest/${channel}.json`
  const response = await fetch(url, {
    headers: {
      Accept: 'application/json',
      'User-Agent': `oore-web/${readInstalledVersion(resolveInstallRoot()) || 'unknown'}/update`,
    },
    signal: AbortSignal.timeout(10_000),
  })
  if (!response.ok) {
    throw new Error(
      `release index request failed (${response.status}) for ${url}`,
    )
  }
  const release = await response.json()
  if (
    release.schema_version !== 1 ||
    release.channel !== channel ||
    typeof release.tag !== 'string' ||
    typeof release.version !== 'string' ||
    typeof release.download_base_url !== 'string' ||
    release.tag.replace(/^v/, '') !== release.version
  ) {
    throw new Error(`invalid ${channel} release index response from ${url}`)
  }
  const expectedDownloadBase = `https://github.com/${repo}/releases/download/${release.tag}`
  if (release.download_base_url.replace(/\/$/, '') !== expectedDownloadBase) {
    throw new Error(
      `release index asset source does not match GitHub repo ${repo}`,
    )
  }
  return release
}

async function fetchBytes(url) {
  const response = await fetch(url, {
    headers: githubHeaders(),
    signal: AbortSignal.timeout(120_000),
  })
  if (!response.ok) {
    throw new Error(`download failed (${response.status}) for ${url}`)
  }
  return Buffer.from(await response.arrayBuffer())
}

function findAssetUrl(release, name) {
  return `${release.download_base_url.replace(/\/$/, '')}/${name}`
}

function releasePlatform() {
  if (process.platform === 'darwin') return 'darwin'
  if (process.platform === 'linux') return 'linux'
  throw new Error(`unsupported platform: ${process.platform}`)
}

function releaseArch() {
  if (process.arch === 'arm64') return 'arm64'
  if (process.arch === 'x64') return 'x86_64'
  throw new Error(`unsupported architecture: ${process.arch}`)
}

function parseChecksum(text, filename) {
  for (const line of text.split(/\r?\n/)) {
    const parts = line.trim().split(/\s+/)
    if (parts.length >= 2 && parts[1] === filename) {
      return parts[0].toLowerCase()
    }
  }
  throw new Error(`checksum not found for ${filename}`)
}

function sha256(buffer) {
  return crypto.createHash('sha256').update(buffer).digest('hex')
}

function extractTarGz(bundlePath, extractDir) {
  fs.mkdirSync(extractDir, { recursive: true })
  const result = spawnSync('tar', ['-xzf', bundlePath, '-C', extractDir], {
    stdio: 'pipe',
    encoding: 'utf8',
  })
  if (result.status !== 0) {
    const details =
      result.stderr?.trim() || result.stdout?.trim() || 'tar failed'
    throw new Error(`failed to extract archive: ${details}`)
  }
}

function replaceFile(src, dst) {
  const next = `${dst}.new-${process.pid}`
  fs.copyFileSync(src, next)
  fs.chmodSync(next, 0o755)
  fs.renameSync(next, dst)
}

function replaceDirectory(src, dst) {
  const next = `${dst}.new-${process.pid}`
  const prev = `${dst}.old-${process.pid}`
  fs.rmSync(next, { recursive: true, force: true })
  fs.cpSync(src, next, { recursive: true })

  if (fs.existsSync(dst)) {
    fs.renameSync(dst, prev)
  }
  fs.renameSync(next, dst)
  fs.rmSync(prev, { recursive: true, force: true })
}

function readInstalledVersion(installRoot) {
  return readTrimmedFile(path.join(installRoot, 'VERSION'))
}

function readInstalledMetadata() {
  const installRoot = resolveInstallRoot()
  return {
    version: readInstalledVersion(installRoot) || 'unknown',
    channel: readTrimmedFile(path.join(installRoot, 'CHANNEL')),
    github_repo: normalizeGitHubRepo(
      readTrimmedFile(path.join(installRoot, 'GITHUB_REPO')) ||
        DEFAULT_GITHUB_REPO,
    ),
  }
}

function printVersion() {
  const installRoot = resolveInstallRoot()
  const version = readInstalledVersion(installRoot)
  console.log(version || 'unknown')
}

async function statusProbe(baseUrl, pathname) {
  let response
  try {
    response = await fetch(new URL(pathname, baseUrl), {
      headers: { Accept: 'application/json' },
      signal: AbortSignal.timeout(5000),
    })
  } catch {
    return { ok: false, error: 'connection_failed' }
  }

  let data
  try {
    data = await response.json()
  } catch {
    return { ok: false, status: response.status, error: 'invalid_json' }
  }

  return {
    ok: response.ok && data?.ok === true,
    status: response.status,
    proxied: response.headers.get('x-oore-web-proxy') === '1',
    data,
  }
}

function statusValue(value) {
  if (typeof value !== 'string') return 'unknown'
  return value.replace(/[^\x20-\x7e]/g, '').slice(0, 128) || 'unknown'
}

function probeFailure(probe) {
  if (probe.error === 'connection_failed') return 'connection failed'
  if (probe.status >= 400) return `HTTP ${probe.status}`
  if (probe.error === 'invalid_json') return 'invalid JSON response'
  if (probe.status) return `HTTP ${probe.status}`
  return 'unhealthy response'
}

async function getStatus(baseUrl) {
  const frontendProbe = await statusProbe(baseUrl, '/__oore_web_healthz')
  if (!frontendProbe.ok) {
    return {
      ok: false,
      url: baseUrl,
      frontend: {
        ok: false,
        version: 'unknown',
        error: `Frontend check failed (${probeFailure(frontendProbe)}). Check that oore-web is running and --url is correct.`,
      },
      backend: {
        ok: false,
        version: 'unknown',
        skipped: true,
        error: 'Backend check skipped because the frontend is unavailable.',
      },
    }
  }

  const [backendHealth, backendReady] = await Promise.all([
    statusProbe(baseUrl, '/healthz'),
    statusProbe(baseUrl, '/readyz'),
  ])
  const checks = {
    database:
      typeof backendReady.data?.database === 'boolean'
        ? backendReady.data.database
        : null,
    migrations:
      typeof backendReady.data?.migrations === 'boolean'
        ? backendReady.data.migrations
        : null,
    encryption:
      typeof backendReady.data?.encryption === 'boolean'
        ? backendReady.data.encryption
        : null,
  }

  let backendError
  if (!backendHealth.ok) {
    backendError = `Backend liveness check failed (${probeFailure(backendHealth)}). Check OORE_WEB_BACKEND_URL and that oored is running.`
  } else if (!backendHealth.proxied) {
    backendError =
      'Backend response did not pass through oore-web. Check that --url points to oore-web.'
  } else if (!backendReady.ok) {
    if (backendReady.proxied) {
      const failed = Object.entries(checks)
        .filter(([, ok]) => ok === false)
        .map(([name]) => name)
        .join(', ')
      backendError = `Backend is not ready${failed ? ` (${failed} failed)` : ''}. Check oored logs and dependencies.`
    } else {
      backendError = `Backend readiness check failed (${probeFailure(backendReady)}). Check OORE_WEB_BACKEND_URL and that oored is running.`
    }
  } else if (!backendReady.proxied) {
    backendError =
      'Backend response did not pass through oore-web. Check that --url points to oore-web.'
  }

  const backendOk = !backendError
  return {
    ok: backendOk,
    url: baseUrl,
    frontend: {
      ok: true,
      version: statusValue(frontendProbe.data?.version),
      channel: statusValue(frontendProbe.data?.channel),
    },
    backend: {
      ok: backendOk,
      version: statusValue(backendHealth.data?.version),
      channel: statusValue(backendHealth.data?.channel),
      ready: backendReady.ok,
      checks,
      ...(backendError ? { error: backendError } : {}),
    },
  }
}

function printStatus(report, json) {
  if (json) {
    console.log(JSON.stringify(report, null, 2))
    return
  }

  console.log(`URL:      ${report.url}`)
  console.log(
    `Frontend: ${report.frontend.ok ? 'ok' : 'failed'} (version ${report.frontend.version}, channel ${report.frontend.channel || 'unknown'})`,
  )
  if (report.frontend.error) console.log(`          ${report.frontend.error}`)

  if (report.backend.skipped) {
    console.log(`Backend:  skipped - ${report.backend.error}`)
    return
  }

  console.log(
    `Backend:  ${report.backend.ok ? 'ok' : 'failed'} (version ${report.backend.version}, channel ${report.backend.channel})`,
  )
  if (report.backend.error) console.log(`          ${report.backend.error}`)
  console.log(
    `Ready:    database=${report.backend.checks.database ?? 'unknown'} migrations=${report.backend.checks.migrations ?? 'unknown'} encryption=${report.backend.checks.encryption ?? 'unknown'}`,
  )
}

async function runStatus(config) {
  const report = await getStatus(config.url)
  printStatus(report, config.json)
  if (!report.ok) process.exitCode = 1
}

function validateUpdateCandidate(
  binaryPath,
  distDir,
  activeConfig,
  installRoot,
) {
  const binDir = path.join(installRoot, 'bin')
  const stagedBinary = path.join(
    binDir,
    `.oore-web.candidate-${process.pid}-${crypto.randomBytes(6).toString('hex')}`,
  )
  fs.mkdirSync(binDir, { recursive: true })
  try {
    fs.copyFileSync(binaryPath, stagedBinary)
    fs.chmodSync(stagedBinary, 0o755)
    const result = spawnSync(
      stagedBinary,
      candidateValidationArgs(activeConfig, distDir),
      {
        encoding: 'utf8',
        timeout: 5000,
      },
    )
    if (result.error) {
      throw new Error(
        `candidate launcher validation failed: ${result.error.message}`,
      )
    }
    if (result.status !== 0) {
      const reason = (result.stderr || result.stdout || 'unknown error')
        .trim()
        .slice(0, 1024)
      throw new Error(
        `candidate launcher rejected the active service configuration: ${reason}`,
      )
    }
  } finally {
    fs.rmSync(stagedBinary, { force: true })
  }
}

export function installUpdateCandidate({
  installRoot,
  extractedBinary,
  extractedDist,
  extractedVersion,
  extractedLicense,
  channel,
  repo,
  activeConfig = null,
}) {
  if (activeConfig) {
    validateUpdateCandidate(
      extractedBinary,
      extractedDist,
      activeConfig,
      installRoot,
    )
  }

  const binDir = path.join(installRoot, 'bin')
  fs.mkdirSync(binDir, { recursive: true })
  replaceFile(extractedBinary, path.join(binDir, 'oore-web'))
  replaceDirectory(extractedDist, path.join(installRoot, 'web-dist'))
  fs.copyFileSync(extractedVersion, path.join(installRoot, 'VERSION'))
  fs.writeFileSync(path.join(installRoot, 'CHANNEL'), `${channel}\n`)
  fs.writeFileSync(path.join(installRoot, 'GITHUB_REPO'), `${repo}\n`)
  if (fileExists(extractedLicense)) {
    fs.copyFileSync(extractedLicense, path.join(installRoot, 'LICENSE'))
  }
}

async function runUpdate(config, activeConfig = null) {
  const installRoot = resolveInstallRoot()
  const currentRaw = readInstalledVersion(installRoot) || '0.0.0'
  const current = parseVersion(currentRaw)
  const repo = normalizeGitHubRepo(
    config.repo ||
      readTrimmedFile(path.join(installRoot, 'GITHUB_REPO')) ||
      DEFAULT_GITHUB_REPO,
  )
  const channel = parseChannel(
    config.channel ||
      readTrimmedFile(path.join(installRoot, 'CHANNEL')) ||
      inferChannelFromVersion(current),
  )

  const release = await fetchReleaseManifest(channel, repo)
  const latestRaw = release.version
  const latest = parseVersion(latestRaw)

  console.log(`Channel:         ${channel}`)
  console.log(`GitHub repo:     ${repo}`)
  console.log(`Current version: ${current.raw}`)
  console.log(`Latest version:  ${latest.raw} (${release.tag})`)

  if (compareVersions(current, latest) >= 0 && !config.force) {
    console.log('Already up to date.')
    return { current: current.raw, latest: latest.raw, updated: false }
  }

  if (compareVersions(current, latest) < 0) {
    console.log(`Update available: ${current.raw} -> ${latest.raw}`)
  } else {
    console.log(`Reinstalling version ${latest.raw} (--force).`)
  }

  if (config.check) return

  const osName = releasePlatform()
  const arch = releaseArch()
  const archiveName = `oore-web_${latestRaw}_${osName}_${arch}.tar.gz`
  const checksumsName = `oore_${latestRaw}_checksums.txt`
  const archiveUrl = findAssetUrl(release, archiveName)
  const checksumsUrl = findAssetUrl(release, checksumsName)

  console.log(`Downloading ${archiveName}...`)
  const [archiveBytes, checksumsBytes] = await Promise.all([
    fetchBytes(archiveUrl),
    fetchBytes(checksumsUrl),
  ])

  const expectedHash = parseChecksum(
    checksumsBytes.toString('utf8'),
    archiveName,
  )
  const actualHash = sha256(archiveBytes)
  if (actualHash !== expectedHash) {
    throw new Error(
      `checksum mismatch for ${archiveName} (expected ${expectedHash}, got ${actualHash})`,
    )
  }
  console.log('Checksum verified (SHA-256).')

  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'oore-web-update-'))
  try {
    const bundlePath = path.join(tmpDir, archiveName)
    const extractDir = path.join(tmpDir, 'extract')
    fs.writeFileSync(bundlePath, archiveBytes)
    extractTarGz(bundlePath, extractDir)

    const extractedBinary = path.join(extractDir, 'bin', 'oore-web')
    const extractedDist = path.join(extractDir, 'web-dist')
    const extractedVersion = path.join(extractDir, 'VERSION')
    const extractedLicense = path.join(extractDir, 'LICENSE')

    if (!fileExists(extractedBinary))
      throw new Error('archive missing bin/oore-web')
    if (!isDirectory(extractedDist)) throw new Error('archive missing web-dist')
    if (!fileExists(extractedVersion))
      throw new Error('archive missing VERSION')

    installUpdateCandidate({
      installRoot,
      extractedBinary,
      extractedDist,
      extractedVersion,
      extractedLicense,
      channel,
      repo,
      activeConfig,
    })
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true })
  }

  console.log(`Updated oore-web to ${latest.raw}.`)
  console.log(
    'If oore-web is running as a service, restart the service to use the updated launcher binary.',
  )
  return { current: current.raw, latest: latest.raw, updated: true }
}

function hasManagedWebService() {
  return (
    fileExists(
      path.join(os.homedir(), '.config', 'systemd', 'user', 'oore-web.service'),
    ) ||
    fileExists(
      path.join(
        os.homedir(),
        'Library',
        'LaunchAgents',
        'build.oore.oore-web.plist',
      ),
    )
  )
}

export async function authorizeOwner(
  request,
  backendUrl,
  config,
  signal = AbortSignal.timeout(5000),
) {
  const headers = new Headers(request.headers)
  headers.delete('host')
  headers.delete('content-length')
  applyTrustedProxyHeaders(request, headers, config)
  const response = await fetch(new URL('/v1/users/me', backendUrl), {
    headers,
    ...(signal ? { signal } : {}),
  })
  if (!response.ok) return false
  const profile = await response.json()
  return profile?.user?.role === 'owner'
}

export async function getWebUpdateStatus(updateState, searchParams) {
  const metadata = readInstalledMetadata()
  const current = parseVersion(searchParams.get('current') || metadata.version)
  const channel = parseChannel(
    searchParams.get('channel') ||
      metadata.channel ||
      inferChannelFromVersion(current),
  )
  const repo = normalizeGitHubRepo(
    searchParams.get('repo') || metadata.github_repo,
  )
  const release = await fetchReleaseManifest(channel, repo)
  const latest = parseVersion(release.version)
  return {
    ...metadata,
    version: current.raw,
    channel,
    github_repo: repo,
    latest_version: latest.raw,
    update_available: compareVersions(current, latest) < 0,
    release_name: release.release_name || release.tag,
    release_notes: release.release_notes || '',
    release_url: release.release_url,
    changelog_url: release.changelog_url,
    managed_service: hasManagedWebService(),
    ...updateState,
  }
}

export function isApiPath(pathname) {
  return (
    pathname === '/healthz' ||
    pathname === '/readyz' ||
    pathname.startsWith('/install/') ||
    pathname === '/v1' ||
    pathname.startsWith('/v1/')
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

function headersToStripForTrustedProxy(config) {
  return new Set([
    BACKEND_TRUSTED_PROXY_SECRET_HEADER,
    config.upstreamTrustedProxySecretHeader,
    config.trustedProxyUserEmailHeader,
    ...CLIENT_CONTROLLED_IDENTITY_HEADERS,
  ])
}

export function applyTrustedProxyHeaders(request, headers, config) {
  const trustedProxySecret = config.trustedProxySecret.trim()
  const upstreamSecret = config.upstreamTrustedProxySecret.trim()
  const identityHeader = config.trustedProxyUserEmailHeader
  const upstreamSecretHeader = config.upstreamTrustedProxySecretHeader
  const inboundIdentity = request.headers.get(identityHeader)?.trim() || ''
  const inboundUpstreamSecret =
    request.headers.get(upstreamSecretHeader)?.trim() || ''

  for (const header of headersToStripForTrustedProxy(config)) {
    headers.delete(header)
  }

  if (!trustedProxySecret) return

  headers.set(BACKEND_TRUSTED_PROXY_SECRET_HEADER, trustedProxySecret)

  if (
    upstreamSecret &&
    inboundIdentity &&
    timingSafeStringEqual(inboundUpstreamSecret, upstreamSecret)
  ) {
    headers.set(identityHeader, inboundIdentity)
  }
}

async function proxyRequest(request, backendUrl, url, config) {
  const upstream = new URL(`${url.pathname}${url.search}`, backendUrl)
  const headers = new Headers(request.headers)
  headers.delete('host')
  headers.delete('content-length')
  applyTrustedProxyHeaders(request, headers, config)

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

export function spaCacheControl(pathname) {
  if (pathname.startsWith('/assets/')) {
    return 'public, max-age=31536000, immutable'
  }
  if (pathname === '/' || pathname.endsWith('.html')) {
    return 'public, max-age=0, must-revalidate'
  }
  return 'public, max-age=3600, must-revalidate'
}

export function spaResponseHeaders(pathname) {
  return {
    'Cache-Control': spaCacheControl(pathname),
    'Content-Security-Policy': "frame-ancestors 'none'",
    'X-Frame-Options': 'DENY',
  }
}

function spaFileResponse(filePath, pathname) {
  return new Response(Bun.file(filePath), {
    headers: spaResponseHeaders(pathname),
  })
}

function serveSpa(distDir, pathname, acceptHeader) {
  const assetPath = resolveAssetPath(distDir, pathname)
  if (!assetPath) {
    return new Response('Not found', { status: 404 })
  }

  if (isDirectory(assetPath)) {
    const indexPath = path.join(assetPath, 'index.html')
    if (fileExists(indexPath)) {
      return spaFileResponse(indexPath, '/')
    }
  }

  if (fileExists(assetPath)) {
    return spaFileResponse(assetPath, pathname)
  }

  const wantsHtml =
    acceptHeader.includes('text/html') || acceptHeader.includes('*/*')
  if (wantsHtml) {
    const indexPath = path.join(distDir, 'index.html')
    if (!fileExists(indexPath)) {
      return new Response('index.html not found', { status: 500 })
    }
    return spaFileResponse(indexPath, '/')
  }

  return new Response('Not found', { status: 404 })
}

async function main() {
  const parsedCommand = parseCommand(process.argv.slice(2))
  if (parsedCommand.command === 'help' || parsedCommand.command === '--help') {
    printHelp()
    return
  }
  if (parsedCommand.command === 'version') {
    printVersion()
    return
  }
  if (parsedCommand.command === 'validate-config') {
    try {
      validateServeConfig(parseServeArgs(parsedCommand.args))
    } catch (error) {
      console.error(
        `[oore-web] ${error instanceof Error ? error.message : 'invalid service configuration'}`,
      )
      process.exit(2)
    }
    return
  }
  if (parsedCommand.command === 'status') {
    let statusConfig
    try {
      statusConfig = parseStatusArgs(parsedCommand.args)
    } catch (error) {
      console.error(
        `[oore-web] ${error instanceof Error ? error.message : 'failed to parse status args'}`,
      )
      printStatusHelp()
      process.exit(2)
    }
    await runStatus(statusConfig)
    return
  }
  if (parsedCommand.command === 'update') {
    let updateConfig
    try {
      updateConfig = parseUpdateArgs(parsedCommand.args)
    } catch (error) {
      console.error(
        `[oore-web] ${error instanceof Error ? error.message : 'failed to parse update args'}`,
      )
      printUpdateHelp()
      process.exit(2)
    }
    await runUpdate(updateConfig)
    return
  }

  if (parsedCommand.command !== 'serve' && parsedCommand.command !== 'run') {
    console.error(`[oore-web] unknown command: ${parsedCommand.command}`)
    printHelp()
    process.exit(2)
  }

  let config
  try {
    config = parseServeArgs(parsedCommand.args)
  } catch (error) {
    console.error(
      `[oore-web] ${error instanceof Error ? error.message : 'failed to parse args'}`,
    )
    printHelp()
    process.exit(2)
  }

  let validated
  try {
    validated = validateServeConfig(config)
  } catch (error) {
    console.error(
      `[oore-web] ${error instanceof Error ? error.message : 'invalid service configuration'}`,
    )
    process.exit(2)
  }
  const { backendUrl, listen, distDir } = validated

  const updateState = { phase: 'idle', error: null }
  const server = Bun.serve({
    hostname: listen.hostname,
    port: listen.port,
    fetch: async (request) => {
      const url = new URL(request.url)

      if (url.pathname === '/__oore_web_healthz') {
        return Response.json(
          {
            ok: true,
            ...readInstalledMetadata(),
          },
          {
            headers: {
              'Cache-Control': 'no-store',
            },
          },
        )
      }

      if (url.pathname === '/__oore_web_update') {
        let owner = false
        try {
          owner = await authorizeOwner(request, backendUrl, config)
        } catch {
          return Response.json(
            { error: 'Could not verify the current owner session' },
            { status: 502 },
          )
        }
        if (!owner) {
          return Response.json(
            { error: 'Only the instance owner can manage runtime updates' },
            { status: 403 },
          )
        }

        if (request.method === 'GET') {
          try {
            return Response.json(
              await getWebUpdateStatus(updateState, url.searchParams),
              {
                headers: { 'Cache-Control': 'no-store' },
              },
            )
          } catch (error) {
            return Response.json(
              {
                error:
                  error instanceof Error
                    ? error.message
                    : 'Failed to check for frontend updates',
              },
              { status: 502 },
            )
          }
        }

        if (request.method !== 'POST') {
          return new Response('Method not allowed', { status: 405 })
        }
        if (!hasManagedWebService()) {
          return Response.json(
            { error: 'Frontend updates require a managed service' },
            { status: 409 },
          )
        }
        if (updateState.phase === 'updating') {
          return Response.json(
            { error: 'A frontend update is already in progress' },
            { status: 409 },
          )
        }

        updateState.phase = 'updating'
        updateState.error = null
        void runUpdate({ check: false, force: false }, config).then(
          (result) => {
            if (!result.updated) {
              updateState.phase = 'idle'
              return
            }
            updateState.phase = 'restarting'
            setTimeout(() => process.exit(75), 1000)
          },
          (error) => {
            updateState.phase = 'failed'
            updateState.error =
              error instanceof Error ? error.message : 'Frontend update failed'
          },
        )
        return Response.json(updateState, { status: 202 })
      }

      if (isApiPath(url.pathname)) {
        return proxyRequest(request, backendUrl, url, config)
      }

      const acceptHeader = request.headers.get('accept') || ''
      return serveSpa(distDir, url.pathname, acceptHeader)
    },
  })

  console.log(
    `[oore-web] listening on http://${listen.hostname}:${listen.port} (backend: ${backendUrl.toString()})`,
  )
  if (config.browserTransportProtected && !isLiteralLoopback(listen.hostname)) {
    console.warn(
      '[oore-web] non-loopback HTTP listener relies on separately protected browser transport',
    )
  }
  if (
    config.backendTransportProtected &&
    backendUrl.protocol === 'http:' &&
    backendUrl.hostname !== 'localhost' &&
    !isLiteralLoopback(backendUrl.hostname)
  ) {
    console.warn(
      '[oore-web] remote HTTP backend relies on separately protected backend transport',
    )
  }
  if (config.trustedProxySecret?.trim()) {
    console.log('[oore-web] trusted proxy shared secret injection enabled')
    if (config.upstreamTrustedProxySecret?.trim()) {
      console.log(
        `[oore-web] trusted proxy identity forwarding requires ${config.upstreamTrustedProxySecretHeader}`,
      )
    } else {
      console.log(
        '[oore-web] trusted proxy identity headers are stripped until an upstream proxy secret is configured',
      )
    }
  }

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

if (import.meta.main) {
  main().catch((error) => {
    console.error(
      `[oore-web] ${error instanceof Error ? error.message : error}`,
    )
    process.exit(1)
  })
}
