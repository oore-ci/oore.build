#!/usr/bin/env bun

import fs from 'node:fs'
import crypto from 'node:crypto'
import net from 'node:net'
import os from 'node:os'
import path from 'node:path'
import { spawn, spawnSync } from 'node:child_process'
import releaseSigningPublicKey from '../../../tools/release-signing-key.pub' with { type: 'text' }

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
const RELEASE_SIGNER_IDENTITY = 'release@oore.build'
const RELEASE_INDEX_NAMESPACE = 'oore-release-index@oore.build'
const RELEASE_MANIFEST_NAMESPACE = 'oore-release-manifest@oore.build'
const MAX_RELEASE_METADATA_BYTES = 1024 * 1024
const MAX_RELEASE_SIGNATURE_BYTES = 64 * 1024
const MAX_RELEASE_ARCHIVE_BYTES = 512 * 1024 * 1024
const MAX_RELEASE_EXTRACTED_BYTES = 1024 * 1024 * 1024

function parsedString(value) {
  return Object.prototype.toString.call(value) === '[object String]'
    ? String(value)
    : null
}

function parsedBoolean(value) {
  return value === true || value === false ? value : null
}
const MAX_RELEASE_ARCHIVE_ENTRIES = 50_000
const MAX_RELEASE_ARCHIVE_LIST_BYTES = 16 * 1024 * 1024
const RELEASE_ARCHIVE_COMMAND_TIMEOUT_MS = 120_000
const WEB_UPDATE_TRANSACTION_NAME = '.oore-web-update-transaction'
const OORE_LIFECYCLE_LOCK_SUFFIX = '.oore-lifecycle.lock'
const WEB_UPDATE_LOCK_SUFFIX = '.oore-web-update.lock'
const WEB_UPDATE_LOCK_READY = 'oore-web-update-lock-ready'
const WEB_UPDATE_LOCK_TIMEOUT_MS = 5_000
const WEB_UPDATE_TRANSACTION_SCHEMA_VERSION = 1
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
  oore-web recover-update
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
  --channel   Release channel. Defaults to the installed frontend channel, then current version.
  --repo      GitHub repo. Defaults to the installed frontend repository, then ${DEFAULT_GITHUB_REPO}.
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

export function validateManagedUpdateTransition(
  installed,
  targetVersion,
  targetChannel,
  targetRepo,
) {
  if (installed.version === 'unknown') return
  const current = parseVersion(installed.version)
  if (compareVersions(targetVersion, current) < 0) {
    throw new Error(
      `frontend updates cannot downgrade from ${current.raw} to ${targetVersion.raw}`,
    )
  }
  const installedChannel = parseChannel(
    installed.channel || inferChannelFromVersion(current),
  )
  if (targetRepo !== installed.github_repo) {
    throw new Error(
      `frontend updates must stay on the recorded ${installed.github_repo} repository`,
    )
  }
  if (targetChannel === installedChannel) return
  const prereleaseToStable =
    ['alpha', 'beta'].includes(installedChannel) &&
    targetChannel === 'stable' &&
    !targetVersion.pre &&
    current.major === targetVersion.major &&
    current.minor === targetVersion.minor &&
    current.patch === targetVersion.patch
  if (!prereleaseToStable) {
    throw new Error(
      `frontend updates must stay on the recorded ${installedChannel} channel, except for a same-version-line alpha or beta promotion to stable`,
    )
  }
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

function releaseIndexHeaders() {
  return {
    Accept: 'application/json',
    'User-Agent': `oore-web/${readInstalledVersion(resolveInstallRoot()) || 'unknown'}/update`,
  }
}

function normalizedReleaseSigningPublicKey() {
  for (const line of releaseSigningPublicKey.split(/\r?\n/)) {
    const [type, key] = line.trim().split(/\s+/)
    if (type === 'ssh-ed25519' && key) return `${type} ${key}`
  }
  throw new Error(
    'this oore-web binary has no configured Ed25519 release signing key',
  )
}

function resolveSshKeygen() {
  for (const candidate of [
    '/usr/bin/ssh-keygen',
    '/usr/local/bin/ssh-keygen',
  ]) {
    try {
      fs.accessSync(candidate, fs.constants.X_OK)
      return candidate
    } catch {
      // Fall back to PATH after checking the standard Darwin and Linux path.
    }
  }
  return 'ssh-keygen'
}

function openSshRequirementError() {
  return new Error(
    'OpenSSH ssh-keygen with -Y signature verification is required. Install OpenSSH (openssh-client on Debian or Ubuntu) and retry.',
  )
}

function verifySignedReleaseMetadata(
  payload,
  signature,
  namespace,
  description,
) {
  const publicKey = normalizedReleaseSigningPublicKey()
  const tmpDir = fs.mkdtempSync(
    path.join(os.tmpdir(), 'oore-web-signature-verify-'),
  )
  try {
    const allowedSigners = path.join(tmpDir, 'allowed-signers')
    const signaturePath = path.join(tmpDir, 'metadata.sig')
    fs.writeFileSync(
      allowedSigners,
      `${RELEASE_SIGNER_IDENTITY} namespaces="${namespace}" ${publicKey}\n`,
      { mode: 0o600 },
    )
    fs.writeFileSync(signaturePath, signature, { mode: 0o600 })

    const result = spawnSync(
      resolveSshKeygen(),
      [
        '-Y',
        'verify',
        '-f',
        allowedSigners,
        '-I',
        RELEASE_SIGNER_IDENTITY,
        '-n',
        namespace,
        '-s',
        signaturePath,
      ],
      {
        input: payload,
        encoding: 'utf8',
        timeout: 10_000,
        maxBuffer: 64 * 1024,
      },
    )
    if (result.error) {
      if (result.error.code === 'ENOENT' || result.error.code === 'EACCES') {
        throw openSshRequirementError()
      }
      throw new Error(
        `failed to run OpenSSH release signature verification: ${result.error.message}`,
      )
    }
    if (result.status !== 0) {
      const stderr = result.stderr?.toLowerCase() || ''
      if (
        stderr.includes('unknown option') ||
        stderr.includes('illegal option') ||
        stderr.includes('invalid option')
      ) {
        throw openSshRequirementError()
      }
      throw new Error(`${description} signature verification failed`)
    }
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true })
  }
}

function decodeReleaseMetadata(payload, description) {
  try {
    return new TextDecoder('utf-8', { fatal: true }).decode(payload)
  } catch {
    throw new Error(`${description} is not valid UTF-8`)
  }
}

async function fetchReleaseManifest(channel, repo) {
  const baseUrl = (
    process.env.OORE_RELEASE_INDEX_BASE_URL || DEFAULT_RELEASE_INDEX_BASE_URL
  ).replace(/\/$/, '')
  const url = `${baseUrl}/latest/${channel}.json`
  const payload = await downloadVerifiedReleaseMetadata(
    url,
    RELEASE_INDEX_NAMESPACE,
    `${channel} release index`,
    releaseIndexHeaders(),
  )
  let release
  try {
    release = JSON.parse(decodeReleaseMetadata(payload, 'release index'))
  } catch (error) {
    if (error instanceof SyntaxError) {
      throw new Error(`failed to parse ${channel} release index JSON`)
    }
    throw error
  }
  if (
    release.schema_version !== 1 ||
    release.channel !== channel ||
    parsedString(release.tag) === null ||
    parsedString(release.version) === null ||
    parsedString(release.download_base_url) === null ||
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

async function fetchBytes(
  url,
  {
    headers = githubHeaders(),
    timeout = 120_000,
    limit = MAX_RELEASE_ARCHIVE_BYTES,
    description = 'release asset',
  } = {},
) {
  const response = await fetch(url, {
    headers,
    signal: AbortSignal.timeout(timeout),
  })
  if (!response.ok) {
    throw new Error(
      `${description} download failed (${response.status}) for ${url}`,
    )
  }

  const contentLength = response.headers.get('content-length')
  if (
    contentLength !== null &&
    Number.isFinite(Number(contentLength)) &&
    Number(contentLength) > limit
  ) {
    throw new Error(`${description} exceeds the size limit`)
  }

  if (!response.body) return Buffer.alloc(0)
  const reader = response.body.getReader()
  const chunks = []
  let total = 0
  try {
    while (true) {
      const { done, value } = await reader.read()
      if (done) break
      total += value.byteLength
      if (total > limit) {
        await reader.cancel()
        throw new Error(`${description} exceeds the size limit`)
      }
      chunks.push(Buffer.from(value))
    }
  } finally {
    reader.releaseLock()
  }
  return Buffer.concat(chunks, total)
}

async function downloadFileLimited(
  url,
  filePath,
  {
    headers = githubHeaders(),
    timeout = 120_000,
    limit = MAX_RELEASE_ARCHIVE_BYTES,
    description = 'release asset',
  } = {},
) {
  const response = await fetch(url, {
    headers,
    signal: AbortSignal.timeout(timeout),
  })
  if (!response.ok) {
    throw new Error(
      `${description} download failed (${response.status}) for ${url}`,
    )
  }
  const contentLength = response.headers.get('content-length')
  if (
    contentLength !== null &&
    Number.isFinite(Number(contentLength)) &&
    Number(contentLength) > limit
  ) {
    throw new Error(`${description} exceeds the size limit`)
  }

  const descriptor = fs.openSync(filePath, 'wx', 0o600)
  const digest = crypto.createHash('sha256')
  let total = 0
  try {
    if (response.body) {
      const reader = response.body.getReader()
      try {
        while (true) {
          const { done, value } = await reader.read()
          if (done) break
          total += value.byteLength
          if (total > limit) {
            await reader.cancel()
            throw new Error(`${description} exceeds the size limit`)
          }
          let offset = 0
          while (offset < value.byteLength) {
            const written = fs.writeSync(
              descriptor,
              value,
              offset,
              value.byteLength - offset,
            )
            if (written <= 0) {
              throw new Error(`failed to write ${description}`)
            }
            offset += written
          }
          digest.update(value)
        }
      } finally {
        reader.releaseLock()
      }
    }
    fs.fsyncSync(descriptor)
  } catch (error) {
    fs.closeSync(descriptor)
    fs.rmSync(filePath, { force: true })
    throw error
  }
  fs.closeSync(descriptor)
  return digest.digest('hex')
}

async function downloadVerifiedReleaseMetadata(
  url,
  namespace,
  description,
  headers = githubHeaders(),
) {
  const payload = await fetchBytes(url, {
    headers,
    timeout: 10_000,
    limit: MAX_RELEASE_METADATA_BYTES,
    description,
  })
  const signature = await fetchBytes(`${url}.sig`, {
    headers,
    timeout: 10_000,
    limit: MAX_RELEASE_SIGNATURE_BYTES,
    description: `${description} signature`,
  })
  verifySignedReleaseMetadata(payload, signature, namespace, description)
  return payload
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

async function fetchVerifiedWebRelease(channel, repo) {
  const release = await fetchReleaseManifest(channel, repo)
  const latest = parseVersion(release.version)
  const osName = releasePlatform()
  const arch = releaseArch()
  const archiveName = `oore-web_${latest.raw}_${osName}_${arch}.tar.gz`
  const checksumsName = `oore_${latest.raw}_checksums.txt`
  const checksumsUrl = findAssetUrl(release, checksumsName)
  const checksumsBytes = await downloadVerifiedReleaseMetadata(
    checksumsUrl,
    RELEASE_MANIFEST_NAMESPACE,
    'release checksum manifest',
  )
  const expectedHash = parseChecksum(
    decodeReleaseMetadata(checksumsBytes, 'release checksum manifest'),
    archiveName,
  )
  if (!/^[a-f0-9]{64}$/.test(expectedHash)) {
    throw new Error(
      'release archive checksum must contain 64 hexadecimal characters',
    )
  }
  return { release, latest, archiveName, expectedHash }
}

function tarOutputLines(output, description) {
  if (output.includes('\uFFFD')) {
    throw new Error(`${description} contains a non-UTF-8 path`)
  }
  const withoutTrailingNewline = output.endsWith('\n')
    ? output.slice(0, -1)
    : output
  if (!withoutTrailingNewline) return []
  const lines = withoutTrailingNewline.split('\n')
  if (lines.some((line) => line.includes('\r') || line.length === 0)) {
    throw new Error(`${description} contains an unsupported path`)
  }
  return lines
}

function listTarArchive(bundlePath, verbose) {
  const result = spawnSync('tar', [verbose ? '-tvzf' : '-tzf', bundlePath], {
    stdio: ['ignore', 'pipe', 'pipe'],
    encoding: 'utf8',
    env: { ...process.env, LC_ALL: 'C' },
    timeout: RELEASE_ARCHIVE_COMMAND_TIMEOUT_MS,
    maxBuffer: MAX_RELEASE_ARCHIVE_LIST_BYTES,
  })
  if (result.error) {
    const reason =
      result.error.code === 'ETIMEDOUT'
        ? 'archive inspection timed out'
        : result.error.message
    throw new Error(`failed to inspect release archive: ${reason}`)
  }
  if (result.status !== 0) {
    const details =
      result.stderr?.trim() || result.stdout?.trim() || 'tar failed'
    throw new Error(`failed to inspect release archive: ${details}`)
  }
  return tarOutputLines(
    result.stdout || '',
    verbose ? 'verbose release archive listing' : 'release archive listing',
  )
}

function normalizeReleaseArchiveMember(rawName) {
  if (rawName === '.' || rawName === './') return ''
  let value = rawName
  if (value.startsWith('./')) value = value.slice(2)
  if (value.endsWith('/')) value = value.slice(0, -1)
  if (!value || value.startsWith('/') || value.includes('\\')) {
    throw new Error(`release archive contains an unsafe path: ${rawName}`)
  }
  const parts = value.split('/')
  if (parts.some((part) => !part || part === '.' || part === '..')) {
    throw new Error(`release archive contains an unsafe path: ${rawName}`)
  }
  return parts.join('/')
}

function releaseArchivePathAllowed(relative, kind) {
  if (!relative) return kind === 'directory'
  const fixed = new Map([
    ['LICENSE', 'file'],
    ['VERSION', 'file'],
    ['bin', 'directory'],
    ['bin/oore-web', 'file'],
    ['web-dist', 'directory'],
  ])
  const expected = fixed.get(relative)
  if (expected) return kind === expected
  return relative.startsWith('web-dist/')
}

function inspectReleaseArchive(bundlePath) {
  const names = listTarArchive(bundlePath, false)
  const verbose = listTarArchive(bundlePath, true)
  if (names.length !== verbose.length) {
    throw new Error('release archive contains an unsupported path')
  }
  if (names.length > MAX_RELEASE_ARCHIVE_ENTRIES) {
    throw new Error('release archive contains too many entries')
  }

  const seen = new Map()
  for (let index = 0; index < names.length; index += 1) {
    const type = verbose[index]?.[0]
    const kind = type === '-' ? 'file' : type === 'd' ? 'directory' : null
    if (!kind) {
      throw new Error(
        `release archive contains an unsupported entry: ${names[index]}`,
      )
    }
    const relative = normalizeReleaseArchiveMember(names[index])
    if (!releaseArchivePathAllowed(relative, kind)) {
      throw new Error(
        `release archive contains an unexpected path: ${names[index]}`,
      )
    }
    if (seen.has(relative)) {
      throw new Error(
        `release archive contains a duplicate path: ${names[index]}`,
      )
    }
    seen.set(relative, kind)
  }

  for (const [relative, kind] of [
    ['LICENSE', 'file'],
    ['VERSION', 'file'],
    ['bin/oore-web', 'file'],
    ['web-dist', 'directory'],
    ['web-dist/index.html', 'file'],
  ]) {
    if (seen.get(relative) !== kind) {
      throw new Error(`release archive is missing ${relative}`)
    }
  }
}

async function measureReleaseArchivePayload(bundlePath) {
  await new Promise((resolve, reject) => {
    const child = spawn('tar', ['-xOzf', bundlePath], {
      stdio: ['ignore', 'pipe', 'pipe'],
      env: { ...process.env, LC_ALL: 'C' },
    })
    let total = 0
    let stderr = ''
    let exceeded = false
    let timedOut = false
    let settled = false
    const timeout = setTimeout(() => {
      timedOut = true
      child.kill('SIGKILL')
    }, RELEASE_ARCHIVE_COMMAND_TIMEOUT_MS)

    const finish = (error) => {
      if (settled) return
      settled = true
      clearTimeout(timeout)
      if (error) reject(error)
      else resolve()
    }

    child.stdout.on('data', (chunk) => {
      total += chunk.length
      if (total > MAX_RELEASE_EXTRACTED_BYTES && !exceeded) {
        exceeded = true
        child.kill('SIGKILL')
      }
    })
    child.stderr.on('data', (chunk) => {
      if (stderr.length < 4096) stderr += chunk.toString('utf8')
    })
    child.once('error', (error) => {
      finish(new Error(`failed to inspect release archive: ${error.message}`))
    })
    child.once('close', (status) => {
      if (exceeded) {
        finish(new Error('release archive exceeds the extracted size limit'))
      } else if (timedOut) {
        finish(new Error('release archive inspection timed out'))
      } else if (status !== 0) {
        finish(
          new Error(
            `failed to inspect release archive payload: ${stderr.trim() || 'tar failed'}`,
          ),
        )
      } else {
        finish()
      }
    })
  })
}

function requireRegularFile(filePath, description, executable = false) {
  const stat = fs.lstatSync(filePath)
  if (!stat.isFile()) throw new Error(`${description} is not a regular file`)
  if (executable && (stat.mode & 0o111) === 0) {
    throw new Error(`${description} is not executable`)
  }
  return stat
}

function requireRegularDirectory(directory, description) {
  const stat = fs.lstatSync(directory)
  if (!stat.isDirectory()) {
    throw new Error(`${description} is not a regular directory`)
  }
  return stat
}

function validateRegularTree(root, description, allowPath = null) {
  requireRegularDirectory(root, description)
  const stack = [{ absolute: root, relative: '' }]
  let entries = 0
  let bytes = 0
  const seen = new Map([['', 'directory']])
  while (stack.length > 0) {
    const current = stack.pop()
    const names = fs.readdirSync(current.absolute)
    for (const name of names) {
      const absolute = path.join(current.absolute, name)
      const relative = current.relative ? `${current.relative}/${name}` : name
      const stat = fs.lstatSync(absolute)
      const kind = stat.isFile()
        ? 'file'
        : stat.isDirectory()
          ? 'directory'
          : null
      if (!kind)
        throw new Error(`${description} contains an unsafe path: ${relative}`)
      if (allowPath && !allowPath(relative, kind)) {
        throw new Error(
          `${description} contains an unexpected path: ${relative}`,
        )
      }
      entries += 1
      if (entries > MAX_RELEASE_ARCHIVE_ENTRIES) {
        throw new Error(`${description} contains too many entries`)
      }
      if (kind === 'file') {
        bytes += stat.size
        if (bytes > MAX_RELEASE_EXTRACTED_BYTES) {
          throw new Error(`${description} exceeds the extracted size limit`)
        }
      } else {
        stack.push({ absolute, relative })
      }
      seen.set(relative, kind)
    }
  }
  return seen
}

function readCandidateVersion(filePath) {
  const stat = requireRegularFile(filePath, 'release archive VERSION')
  if (stat.size > 4096) throw new Error('release archive VERSION is too large')
  const value = decodeReleaseMetadata(
    fs.readFileSync(filePath),
    'release archive VERSION',
  )
  const trimmed = value.endsWith('\n') ? value.slice(0, -1) : value
  if (!trimmed || trimmed.includes('\n') || trimmed.includes('\r')) {
    throw new Error('release archive VERSION must contain one line')
  }
  parseVersion(trimmed)
  return trimmed
}

function validateExtractedWebRelease(extractDir, expectedVersion) {
  const seen = validateRegularTree(
    extractDir,
    'release archive',
    releaseArchivePathAllowed,
  )
  for (const [relative, kind] of [
    ['LICENSE', 'file'],
    ['VERSION', 'file'],
    ['bin/oore-web', 'file'],
    ['web-dist', 'directory'],
    ['web-dist/index.html', 'file'],
  ]) {
    if (seen.get(relative) !== kind) {
      throw new Error(`release archive is missing ${relative}`)
    }
  }
  requireRegularFile(
    path.join(extractDir, 'bin', 'oore-web'),
    'release archive launcher',
    true,
  )
  const version = readCandidateVersion(path.join(extractDir, 'VERSION'))
  if (version !== expectedVersion) {
    throw new Error(
      `release archive VERSION does not match ${expectedVersion}: ${version}`,
    )
  }
}

async function extractTarGz(bundlePath, extractDir, expectedVersion) {
  inspectReleaseArchive(bundlePath)
  await measureReleaseArchivePayload(bundlePath)
  fs.mkdirSync(extractDir, { recursive: true })
  const result = spawnSync('tar', ['-xzf', bundlePath, '-C', extractDir], {
    stdio: 'pipe',
    encoding: 'utf8',
    env: { ...process.env, LC_ALL: 'C' },
    timeout: RELEASE_ARCHIVE_COMMAND_TIMEOUT_MS,
    maxBuffer: 1024 * 1024,
  })
  if (result.error) {
    const reason =
      result.error.code === 'ETIMEDOUT'
        ? 'archive extraction timed out'
        : result.error.message
    throw new Error(`failed to extract archive: ${reason}`)
  }
  if (result.status !== 0) {
    const details =
      result.stderr?.trim() || result.stdout?.trim() || 'tar failed'
    throw new Error(`failed to extract archive: ${details}`)
  }
  validateExtractedWebRelease(extractDir, expectedVersion)
}

function updateTransactionPath(installRoot) {
  return path.join(installRoot, WEB_UPDATE_TRANSACTION_NAME)
}

function siblingLockPath(installRoot, suffix) {
  const rootName = path.basename(installRoot)
  if (!rootName || rootName === '.' || rootName === '..') {
    throw new Error(`invalid Oore install root: ${installRoot}`)
  }
  return path.join(path.dirname(installRoot), `.${rootName}${suffix}`)
}

function lifecycleLockPath(installRoot) {
  return siblingLockPath(installRoot, OORE_LIFECYCLE_LOCK_SUFFIX)
}

function updateLockPath(installRoot) {
  return siblingLockPath(installRoot, WEB_UPDATE_LOCK_SUFFIX)
}

function openPrivateLockFile(lockPath) {
  const lockDirectory = path.dirname(lockPath)
  const directory = requireRegularDirectory(
    lockDirectory,
    'frontend lifecycle lock directory',
  )
  const expectedUid =
    process.getuid instanceof Function ? process.getuid() : directory.uid
  if (directory.uid !== expectedUid || (directory.mode & 0o022) !== 0) {
    throw new Error(
      `frontend lifecycle lock directory has unsafe ownership or permissions: ${lockDirectory}`,
    )
  }
  const flags =
    fs.constants.O_CREAT | fs.constants.O_RDWR | (fs.constants.O_NOFOLLOW || 0)
  const descriptor = fs.openSync(lockPath, flags, 0o600)
  try {
    fs.fchmodSync(descriptor, 0o600)
    const opened = fs.fstatSync(descriptor)
    const linked = fs.lstatSync(lockPath)
    if (
      !opened.isFile() ||
      !linked.isFile() ||
      linked.isSymbolicLink() ||
      opened.dev !== linked.dev ||
      opened.ino !== linked.ino ||
      opened.uid !== expectedUid ||
      (opened.mode & 0o077) !== 0
    ) {
      throw new Error(`frontend update lock is unsafe: ${lockPath}`)
    }
    fs.fsyncSync(descriptor)
    fsyncDirectory(lockDirectory)
    return { descriptor, lockPath }
  } catch (error) {
    fs.closeSync(descriptor)
    throw error
  }
}

async function acquireFileLock(lockPath, busyMessage) {
  const { descriptor } = openPrivateLockFile(lockPath)
  let acquireCommand
  if (process.platform === 'darwin') {
    acquireCommand = '/usr/bin/lockf -s -t 0 3'
  } else if (process.platform === 'linux') {
    const flock = fs.existsSync('/usr/bin/flock')
      ? '/usr/bin/flock'
      : '/bin/flock'
    if (!fs.existsSync(flock)) {
      fs.closeSync(descriptor)
      throw new Error('frontend updates require flock on Linux')
    }
    acquireCommand = `${flock} -n 3`
  } else {
    fs.closeSync(descriptor)
    throw new Error(`frontend updates are not supported on ${process.platform}`)
  }
  const holdScript = `${acquireCommand} || exit 75
printf '%s\\n' '${WEB_UPDATE_LOCK_READY}'
IFS= read -r _`

  let child
  try {
    child = spawn('/bin/sh', ['-c', holdScript], {
      stdio: ['pipe', 'pipe', 'pipe', descriptor],
      env: { ...process.env, LC_ALL: 'C' },
    })
  } finally {
    fs.closeSync(descriptor)
  }

  let stderr = ''
  child.stderr.on('data', (chunk) => {
    if (stderr.length < 4096) stderr += chunk.toString('utf8')
  })
  const exit = new Promise((resolve) => {
    child.once('exit', (code, signal) => resolve({ code, signal }))
  })

  await new Promise((resolve, reject) => {
    let output = ''
    let settled = false
    const finish = (error) => {
      if (settled) return
      settled = true
      clearTimeout(timeout)
      child.stdout.off('data', onData)
      child.off('error', onError)
      if (error) reject(error)
      else resolve()
    }
    const onData = (chunk) => {
      output += chunk.toString('utf8')
      if (output.includes(`${WEB_UPDATE_LOCK_READY}\n`)) finish()
    }
    const onError = (error) =>
      finish(
        new Error(`failed to start frontend update lock: ${error.message}`),
      )
    const onExit = (code, signal) => {
      const details = stderr.trim()
      finish(
        new Error(
          details ||
            (code === 75
              ? busyMessage
              : `frontend update lock helper failed (${signal ? `signal ${signal}` : `exit ${code}`})`),
        ),
      )
    }
    const timeout = setTimeout(() => {
      child.kill('SIGKILL')
      finish(
        new Error(
          `timed out while acquiring frontend update lock: ${lockPath}`,
        ),
      )
    }, WEB_UPDATE_LOCK_TIMEOUT_MS)
    child.stdout.on('data', onData)
    child.once('error', onError)
    void exit.then(({ code, signal }) => onExit(code, signal))
  })

  let released = false
  return {
    async release() {
      if (released) return
      released = true
      child.stdin.end('\n')
      const result = await exit
      if (result.code !== 0) {
        throw new Error(
          `frontend update lock helper failed (${result.signal ? `signal ${result.signal}` : `exit ${result.code}`})`,
        )
      }
    },
  }
}

async function withFileLock(lockPath, busyMessage, operation) {
  const lock = await acquireFileLock(lockPath, busyMessage)
  let value
  let operationError = null
  try {
    value = await operation()
  } catch (error) {
    operationError = error
  }

  let releaseError = null
  try {
    await lock.release()
  } catch (error) {
    releaseError = error
  }
  if (operationError && releaseError) {
    throw new Error(
      `${errorMessage(operationError)}; releasing the frontend update lock also failed: ${errorMessage(releaseError)}`,
    )
  }
  if (operationError) throw operationError
  if (releaseError) throw releaseError
  return value
}

function withUpdateLock(installRoot, operation) {
  return withFileLock(
    updateLockPath(installRoot),
    'another frontend update or recovery is active',
    operation,
  )
}

function withLifecycleLock(installRoot, operation) {
  return withFileLock(
    lifecycleLockPath(installRoot),
    'another Oore install, setup, update, or uninstall operation is active',
    operation,
  )
}

function recoverUpdateWithLocks(installRoot) {
  return withLifecycleLock(installRoot, () =>
    withUpdateLock(installRoot, () => recoverInterruptedUpdate(installRoot)),
  )
}

function updateTransactionManifestPath(transactionRoot) {
  return path.join(transactionRoot, 'manifest.json')
}

function updateTransactionOwnerPath(transactionRoot) {
  return path.join(transactionRoot, 'owner.json')
}

function fsyncDirectory(directory) {
  const descriptor = fs.openSync(directory, fs.constants.O_RDONLY)
  try {
    fs.fsyncSync(descriptor)
  } finally {
    fs.closeSync(descriptor)
  }
}

function fsyncFile(filePath) {
  const descriptor = fs.openSync(filePath, fs.constants.O_RDONLY)
  try {
    fs.fsyncSync(descriptor)
  } finally {
    fs.closeSync(descriptor)
  }
}

function ensureDirectoryDurable(directory) {
  try {
    requireRegularDirectory(directory, directory)
    return
  } catch (error) {
    if (error?.code !== 'ENOENT') throw error
  }

  const parent = path.dirname(directory)
  if (parent === directory) {
    throw new Error(`cannot create update directory: ${directory}`)
  }
  ensureDirectoryDurable(parent)
  fs.mkdirSync(directory)
  fsyncDirectory(directory)
  fsyncDirectory(parent)
}

function removeUpdatePath(filePath) {
  const kind = updatePathKind(filePath)
  if (!kind) return false
  fs.rmSync(filePath, { recursive: kind.kind === 'directory', force: false })
  fsyncDirectory(path.dirname(filePath))
  return true
}

function renameUpdatePath(source, destination) {
  const sourceParent = path.dirname(source)
  const destinationParent = path.dirname(destination)
  ensureDirectoryDurable(destinationParent)
  fs.renameSync(source, destination)
  fsyncDirectory(destinationParent)
  if (sourceParent !== destinationParent) fsyncDirectory(sourceParent)
}

function chmodUpdatePath(filePath, mode) {
  fs.chmodSync(filePath, mode)
  const kind = updatePathKind(filePath)
  if (kind?.kind === 'directory') fsyncDirectory(filePath)
  else fsyncFile(filePath)
  fsyncDirectory(path.dirname(filePath))
}

function writeFileDurable(filePath, contents, mode = 0o600) {
  ensureDirectoryDurable(path.dirname(filePath))
  fs.writeFileSync(filePath, contents, { mode })
  fsyncFile(filePath)
  fsyncDirectory(path.dirname(filePath))
}

function writeJsonAtomic(filePath, value) {
  const next = `${filePath}.new-${process.pid}`
  try {
    writeFileDurable(next, `${JSON.stringify(value)}\n`, 0o600)
    renameUpdatePath(next, filePath)
  } catch (error) {
    try {
      removeUpdatePath(next)
    } catch {
      // Preserve the primary write failure.
    }
    throw error
  }
}

function errorMessage(error) {
  return error instanceof Error ? error.message : String(error)
}

function readJsonFile(filePath, description) {
  try {
    return JSON.parse(fs.readFileSync(filePath, 'utf8'))
  } catch (error) {
    throw new Error(
      `failed to read ${description} at ${filePath}: ${errorMessage(error)}`,
    )
  }
}

function processIsRunning(pid) {
  if (!Number.isInteger(pid) || pid <= 0) return false
  try {
    process.kill(pid, 0)
    return true
  } catch (error) {
    return error?.code !== 'ESRCH'
  }
}

function copyUpdatePath(source, destination, kind) {
  const sourceKind = updatePathKind(source)
  if (sourceKind?.kind !== kind) {
    throw new Error(`update source has an unexpected type: ${source}`)
  }
  ensureDirectoryDurable(path.dirname(destination))
  if (updatePathKind(destination)) removeUpdatePath(destination)
  if (kind === 'directory') {
    fs.cpSync(source, destination, {
      recursive: true,
      preserveTimestamps: true,
    })
    fsyncUpdateTree(destination)
    fsyncDirectory(path.dirname(destination))
    return
  }
  fs.copyFileSync(source, destination)
  fsyncFile(destination)
  fsyncDirectory(path.dirname(destination))
}

function fsyncUpdateTree(root) {
  const stack = [{ filePath: root, visited: false }]
  while (stack.length > 0) {
    const current = stack.pop()
    const kind = updatePathKind(current.filePath)
    if (!kind) throw new Error(`update path disappeared: ${current.filePath}`)
    if (kind.kind === 'file') {
      fsyncFile(current.filePath)
      continue
    }
    if (current.visited) {
      fsyncDirectory(current.filePath)
      continue
    }
    stack.push({ filePath: current.filePath, visited: true })
    for (const name of fs.readdirSync(current.filePath)) {
      stack.push({
        filePath: path.join(current.filePath, name),
        visited: false,
      })
    }
  }
}

function updatePathKind(filePath) {
  try {
    const stat = fs.lstatSync(filePath)
    if (stat.isFile()) return { kind: 'file', mode: stat.mode & 0o777 }
    if (stat.isDirectory()) {
      return { kind: 'directory', mode: stat.mode & 0o777 }
    }
    throw new Error(`unsupported file type at ${filePath}`)
  } catch (error) {
    if (error?.code === 'ENOENT') return null
    throw error
  }
}

function validateUpdateManifest(manifest) {
  if (
    manifest?.schema_version !== WEB_UPDATE_TRANSACTION_SCHEMA_VERSION ||
    ![
      'prepared',
      'committing',
      'committed',
      'rolling_back',
      'rolled_back',
    ].includes(manifest.phase) ||
    !Array.isArray(manifest.entries)
  ) {
    throw new Error('invalid frontend update transaction manifest')
  }

  const allowed = new Map([
    ['launcher', { relative: 'bin/oore-web', kind: 'file' }],
    ['dist', { relative: 'web-dist', kind: 'directory' }],
    ['version', { relative: 'WEB_VERSION', kind: 'file' }],
    ['channel', { relative: 'WEB_CHANNEL', kind: 'file' }],
    ['repo', { relative: 'WEB_GITHUB_REPO', kind: 'file' }],
    ['license', { relative: 'LICENSE', kind: 'file' }],
  ])
  const seen = new Set()
  for (const entry of manifest.entries) {
    if (entry && entry.state === undefined) {
      entry.state =
        manifest.phase === 'prepared'
          ? 'pending'
          : manifest.phase === 'committed'
            ? 'published'
            : 'publishing'
    }
    const expected = allowed.get(entry?.key)
    if (
      !expected ||
      seen.has(entry.key) ||
      entry.relative !== expected.relative ||
      entry.kind !== expected.kind ||
      !['pending', 'publishing', 'published', 'restoring', 'restored'].includes(
        entry.state,
      ) ||
      parsedBoolean(entry.had_original) === null ||
      (entry.had_original &&
        (!['file', 'directory'].includes(entry.original_kind) ||
          !Number.isInteger(entry.original_mode) ||
          entry.original_mode < 0 ||
          entry.original_mode > 0o777)) ||
      (!entry.had_original &&
        (entry.original_kind !== null || entry.original_mode !== null))
    ) {
      throw new Error('invalid frontend update transaction entry')
    }
    seen.add(entry.key)
  }

  for (const required of ['launcher', 'dist', 'version', 'channel', 'repo']) {
    if (!seen.has(required)) {
      throw new Error(`frontend update transaction omitted ${required}`)
    }
  }
  return manifest
}

function publishUpdatePath(transactionRoot, installRoot, entry) {
  const staged = path.join(transactionRoot, 'staged', entry.relative)
  const target = path.join(installRoot, entry.relative)
  const displaced = path.join(transactionRoot, 'displaced', entry.relative)
  ensureDirectoryDurable(path.dirname(target))

  const stagedKind = updatePathKind(staged)
  if (stagedKind?.kind !== entry.kind) {
    throw new Error(`staged frontend path is invalid: ${entry.relative}`)
  }
  const current = updatePathKind(target)
  if (
    (current !== null) !== entry.had_original ||
    (current && current.kind !== entry.original_kind)
  ) {
    throw new Error(`installed frontend path changed: ${entry.relative}`)
  }
  if (entry.kind === 'file' && current?.kind !== 'directory') {
    renameUpdatePath(staged, target)
    return
  }

  ensureDirectoryDurable(path.dirname(displaced))
  removeUpdatePath(displaced)
  if (current) renameUpdatePath(target, displaced)
  try {
    renameUpdatePath(staged, target)
  } catch (error) {
    if (current && updatePathKind(displaced) && !updatePathKind(target)) {
      renameUpdatePath(displaced, target)
    }
    throw error
  }
}

function replaceTargetFromPreserved(preserved, target, transactionRoot, entry) {
  const preservedKind = updatePathKind(preserved)
  if (preservedKind?.kind !== entry.original_kind) {
    throw new Error(`frontend rollback source is invalid: ${entry.relative}`)
  }
  const current = updatePathKind(target)
  if (entry.original_kind === 'file' && current?.kind !== 'directory') {
    renameUpdatePath(preserved, target)
  } else {
    const discarded = path.join(transactionRoot, 'discarded', entry.relative)
    ensureDirectoryDurable(path.dirname(discarded))
    removeUpdatePath(discarded)
    if (current) renameUpdatePath(target, discarded)
    try {
      renameUpdatePath(preserved, target)
    } catch (error) {
      if (current && updatePathKind(discarded) && !updatePathKind(target)) {
        renameUpdatePath(discarded, target)
      }
      throw error
    }
  }
  chmodUpdatePath(target, entry.original_mode)
}

function restoreUpdatePath(transactionRoot, installRoot, entry, wasRestoring) {
  const target = path.join(installRoot, entry.relative)
  const staged = path.join(transactionRoot, 'staged', entry.relative)
  const displaced = path.join(transactionRoot, 'displaced', entry.relative)
  const backup = path.join(transactionRoot, 'backup', entry.relative)
  const stagedKind = updatePathKind(staged)
  const displacedKind = updatePathKind(displaced)
  const backupKind = updatePathKind(backup)

  if (!entry.had_original) {
    if (stagedKind) {
      if (updatePathKind(target)) {
        throw new Error(
          `frontend rollback found an unexpected path: ${entry.relative}`,
        )
      }
    } else {
      removeUpdatePath(target)
    }
    return
  }

  if (displacedKind) {
    replaceTargetFromPreserved(displaced, target, transactionRoot, entry)
    return
  }
  if (stagedKind) {
    const current = updatePathKind(target)
    if (current?.kind !== entry.original_kind) {
      throw new Error(
        `frontend rollback cannot verify the original path: ${entry.relative}`,
      )
    }
    return
  }
  if (backupKind) {
    replaceTargetFromPreserved(backup, target, transactionRoot, entry)
    return
  }
  if (!wasRestoring) {
    throw new Error(`frontend rollback source is missing: ${entry.relative}`)
  }
  const current = updatePathKind(target)
  if (current?.kind !== entry.original_kind) {
    throw new Error(`frontend rollback is incomplete: ${entry.relative}`)
  }
  chmodUpdatePath(target, entry.original_mode)
}

function rollbackUpdateTransaction(transactionRoot, installRoot, manifest) {
  manifest.phase = 'rolling_back'
  writeJsonAtomic(updateTransactionManifestPath(transactionRoot), manifest)
  for (const entry of [...manifest.entries].reverse()) {
    if (entry.state === 'pending' || entry.state === 'restored') continue
    const wasRestoring = entry.state === 'restoring'
    if (!wasRestoring) {
      entry.state = 'restoring'
      writeJsonAtomic(updateTransactionManifestPath(transactionRoot), manifest)
    }
    restoreUpdatePath(transactionRoot, installRoot, entry, wasRestoring)
    entry.state = 'restored'
    writeJsonAtomic(updateTransactionManifestPath(transactionRoot), manifest)
  }
  manifest.phase = 'rolled_back'
  writeJsonAtomic(updateTransactionManifestPath(transactionRoot), manifest)
}

function recoverInterruptedUpdate(installRoot) {
  const transactionRoot = updateTransactionPath(installRoot)
  const transactionKind = updatePathKind(transactionRoot)
  if (!transactionKind) return false
  if (transactionKind.kind !== 'directory') {
    throw new Error(`frontend update transaction is unsafe: ${transactionRoot}`)
  }

  const ownerPath = updateTransactionOwnerPath(transactionRoot)
  let owner = null
  const ownerKind = updatePathKind(ownerPath)
  if (ownerKind?.kind === 'file') {
    owner = readJsonFile(ownerPath, 'frontend update transaction owner')
  } else if (ownerKind) {
    throw new Error('frontend update transaction owner is unsafe')
  }
  if (owner?.pid !== process.pid && processIsRunning(owner?.pid)) {
    throw new Error(
      `another frontend update is in progress (process ${owner.pid})`,
    )
  }

  const manifestPath = updateTransactionManifestPath(transactionRoot)
  const manifestKind = updatePathKind(manifestPath)
  if (!manifestKind) {
    removeUpdatePath(transactionRoot)
    return false
  }
  if (manifestKind.kind !== 'file') {
    throw new Error('frontend update transaction manifest is unsafe')
  }

  const manifest = validateUpdateManifest(
    readJsonFile(manifestPath, 'frontend update transaction manifest'),
  )
  const interrupted = ['committing', 'rolling_back'].includes(manifest.phase)
  if (interrupted) {
    rollbackUpdateTransaction(transactionRoot, installRoot, manifest)
  }
  removeUpdatePath(transactionRoot)
  return interrupted
}

function prepareUpdateTransaction(installRoot, candidates) {
  const transactionRoot = updateTransactionPath(installRoot)
  ensureDirectoryDurable(installRoot)
  try {
    fs.mkdirSync(transactionRoot, { mode: 0o700 })
    fsyncDirectory(transactionRoot)
    fsyncDirectory(installRoot)
  } catch (error) {
    if (error?.code === 'EEXIST') {
      throw new Error('another frontend update transaction already exists')
    }
    throw error
  }

  try {
    writeJsonAtomic(updateTransactionOwnerPath(transactionRoot), {
      pid: process.pid,
      started_at: new Date().toISOString(),
    })
    const entries = []
    for (const candidate of candidates) {
      const staged = path.join(transactionRoot, 'staged', candidate.relative)
      if (candidate.contents !== undefined) {
        writeFileDurable(staged, candidate.contents, 0o644)
      } else {
        copyUpdatePath(candidate.source, staged, candidate.kind)
      }
      if (candidate.executable) chmodUpdatePath(staged, 0o755)

      const target = path.join(installRoot, candidate.relative)
      const original = updatePathKind(target)
      if (original && candidate.kind === 'file' && original.kind === 'file') {
        const backup = path.join(transactionRoot, 'backup', candidate.relative)
        copyUpdatePath(target, backup, original.kind)
        chmodUpdatePath(backup, original.mode)
      }
      entries.push({
        key: candidate.key,
        relative: candidate.relative,
        kind: candidate.kind,
        had_original: original !== null,
        original_kind: original?.kind ?? null,
        original_mode: original?.mode ?? null,
        state: 'pending',
      })
    }

    const manifest = {
      schema_version: WEB_UPDATE_TRANSACTION_SCHEMA_VERSION,
      phase: 'prepared',
      entries,
    }
    writeJsonAtomic(updateTransactionManifestPath(transactionRoot), manifest)
    return { transactionRoot, manifest }
  } catch (error) {
    try {
      removeUpdatePath(transactionRoot)
    } catch {
      // Preserve the primary preparation failure.
    }
    throw error
  }
}

function commitUpdateTransaction(installRoot, transactionRoot, manifest) {
  manifest.phase = 'committing'
  writeJsonAtomic(updateTransactionManifestPath(transactionRoot), manifest)
  try {
    for (const entry of manifest.entries) {
      entry.state = 'publishing'
      writeJsonAtomic(updateTransactionManifestPath(transactionRoot), manifest)
      publishUpdatePath(transactionRoot, installRoot, entry)
      entry.state = 'published'
      writeJsonAtomic(updateTransactionManifestPath(transactionRoot), manifest)
    }
    manifest.phase = 'committed'
    writeJsonAtomic(updateTransactionManifestPath(transactionRoot), manifest)
  } catch (error) {
    try {
      rollbackUpdateTransaction(transactionRoot, installRoot, manifest)
    } catch (rollbackError) {
      throw new Error(
        `frontend update failed: ${errorMessage(error)}; restoring the previous frontend also failed: ${errorMessage(rollbackError)}; recovery files remain at ${transactionRoot}`,
      )
    }
    removeUpdatePath(transactionRoot)
    throw error
  }

  try {
    removeUpdatePath(transactionRoot)
  } catch (error) {
    console.warn(
      `[oore-web] update completed, but transaction cleanup failed: ${errorMessage(error)}`,
    )
  }
}

function readInstalledVersion(installRoot) {
  return (
    readTrimmedFile(path.join(installRoot, 'WEB_VERSION')) ||
    readTrimmedFile(path.join(installRoot, 'VERSION'))
  )
}

function readInstalledChannel(installRoot) {
  return (
    readTrimmedFile(path.join(installRoot, 'WEB_CHANNEL')) ||
    readTrimmedFile(path.join(installRoot, 'CHANNEL'))
  )
}

function readInstalledRepo(installRoot) {
  return normalizeGitHubRepo(
    readTrimmedFile(path.join(installRoot, 'WEB_GITHUB_REPO')) ||
      readTrimmedFile(path.join(installRoot, 'GITHUB_REPO')) ||
      DEFAULT_GITHUB_REPO,
  )
}

export function readInstalledMetadata(installRoot = resolveInstallRoot()) {
  return {
    version: readInstalledVersion(installRoot) || 'unknown',
    channel: readInstalledChannel(installRoot),
    github_repo: readInstalledRepo(installRoot),
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
  const parsed = parsedString(value)
  if (parsed === null) return 'unknown'
  return parsed.replace(/[^\x20-\x7e]/g, '').slice(0, 128) || 'unknown'
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
    database: parsedBoolean(backendReady.data?.database),
    migrations: parsedBoolean(backendReady.data?.migrations),
    encryption: parsedBoolean(backendReady.data?.encryption),
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
  const backend = {
    ok: backendOk,
    version: statusValue(backendHealth.data?.version),
    channel: statusValue(backendHealth.data?.channel),
    ready: backendReady.ok,
    checks,
  }
  if (backendError) backend.error = backendError

  return {
    ok: backendOk,
    url: baseUrl,
    frontend: {
      ok: true,
      version: statusValue(frontendProbe.data?.version),
      channel: statusValue(frontendProbe.data?.channel),
    },
    backend,
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

function validateUpdateCandidate(binaryPath, distDir, activeConfig) {
  const result = spawnSync(
    binaryPath,
    candidateValidationArgs(activeConfig, distDir),
    {
      encoding: 'utf8',
      timeout: 5000,
      maxBuffer: 1024 * 1024,
      env: {
        ...process.env,
        OORE_INSTALL_ROOT: path.dirname(path.dirname(binaryPath)),
      },
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
}

function validateCandidateLauncherVersion(binaryPath, expectedVersion) {
  const result = spawnSync(binaryPath, ['version'], {
    encoding: 'utf8',
    timeout: 5000,
    maxBuffer: 1024 * 1024,
    env: {
      ...process.env,
      OORE_INSTALL_ROOT: path.dirname(path.dirname(binaryPath)),
    },
  })
  if (result.error) {
    throw new Error(
      `candidate launcher version probe failed: ${result.error.message}`,
    )
  }
  if (result.status !== 0 || result.stdout.trim() !== expectedVersion) {
    const reason = (result.stderr || result.stdout || 'unknown error')
      .trim()
      .slice(0, 1024)
    throw new Error(
      `candidate launcher does not report version ${expectedVersion}: ${reason}`,
    )
  }
}

function validateUpdateCandidateSources({
  extractedBinary,
  extractedDist,
  extractedVersion,
  extractedLicense,
  expectedVersion,
  requireLicense,
}) {
  requireRegularFile(extractedBinary, 'candidate launcher', true)
  validateRegularTree(extractedDist, 'candidate web-dist')
  requireRegularFile(
    path.join(extractedDist, 'index.html'),
    'candidate web-dist/index.html',
  )
  const version = readCandidateVersion(extractedVersion)
  if (expectedVersion && version !== expectedVersion) {
    throw new Error(
      `candidate VERSION does not match ${expectedVersion}: ${version}`,
    )
  }
  const licenseKind = updatePathKind(extractedLicense)
  if (requireLicense && licenseKind?.kind !== 'file') {
    throw new Error('candidate LICENSE is not a regular file')
  }
  if (licenseKind && licenseKind.kind !== 'file') {
    throw new Error('candidate LICENSE is not a regular file')
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
  expectedVersion = null,
  requireLicense = false,
}) {
  recoverInterruptedUpdate(installRoot)
  validateUpdateCandidateSources({
    extractedBinary,
    extractedDist,
    extractedVersion,
    extractedLicense,
    expectedVersion,
    requireLicense,
  })
  if (expectedVersion) {
    validateCandidateLauncherVersion(extractedBinary, expectedVersion)
  }
  if (activeConfig) {
    validateUpdateCandidate(extractedBinary, extractedDist, activeConfig)
  }

  const candidates = [
    {
      key: 'dist',
      relative: 'web-dist',
      kind: 'directory',
      source: extractedDist,
    },
    {
      key: 'version',
      relative: 'WEB_VERSION',
      kind: 'file',
      source: extractedVersion,
    },
    {
      key: 'channel',
      relative: 'WEB_CHANNEL',
      kind: 'file',
      contents: `${channel}\n`,
    },
    {
      key: 'repo',
      relative: 'WEB_GITHUB_REPO',
      kind: 'file',
      contents: `${repo}\n`,
    },
  ]
  if (updatePathKind(extractedLicense)?.kind === 'file') {
    candidates.push({
      key: 'license',
      relative: 'LICENSE',
      kind: 'file',
      source: extractedLicense,
    })
  }
  candidates.push({
    key: 'launcher',
    relative: 'bin/oore-web',
    kind: 'file',
    source: extractedBinary,
    executable: true,
  })

  const { transactionRoot, manifest } = prepareUpdateTransaction(
    installRoot,
    candidates,
  )
  commitUpdateTransaction(installRoot, transactionRoot, manifest)
}

async function runUpdate(config, activeConfig = null) {
  if (config.check) return runUpdateWithAcquiredLock(config, activeConfig)
  const installRoot = resolveInstallRoot()
  return withLifecycleLock(installRoot, () =>
    withUpdateLock(installRoot, () =>
      runUpdateWithAcquiredLock(config, activeConfig),
    ),
  )
}

async function runUpdateWithAcquiredLock(config, activeConfig = null) {
  const installRoot = resolveInstallRoot()
  if (!config.check && recoverInterruptedUpdate(installRoot)) {
    console.warn(
      '[oore-web] restored the previous frontend after an interrupted update',
    )
  }
  const installed = readInstalledMetadata(installRoot)
  const currentRaw =
    installed.version === 'unknown' ? '0.0.0' : installed.version
  const current = parseVersion(currentRaw)
  const repo = normalizeGitHubRepo(config.repo || installed.github_repo)
  const channel = parseChannel(
    config.channel || installed.channel || inferChannelFromVersion(current),
  )

  const { release, latest, archiveName, expectedHash } =
    await fetchVerifiedWebRelease(channel, repo)
  validateManagedUpdateTransition(installed, latest, channel, repo)

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

  const archiveUrl = findAssetUrl(release, archiveName)

  if (config.check) {
    return { current: current.raw, latest: latest.raw, updated: false }
  }

  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'oore-web-update-'))
  try {
    const bundlePath = path.join(tmpDir, archiveName)
    const extractDir = path.join(tmpDir, 'extract')
    console.log(`Downloading ${archiveName}...`)
    const actualHash = await downloadFileLimited(archiveUrl, bundlePath, {
      limit: MAX_RELEASE_ARCHIVE_BYTES,
      description: 'release archive',
    })
    if (actualHash !== expectedHash) {
      throw new Error(
        `checksum mismatch for ${archiveName} (expected ${expectedHash}, got ${actualHash})`,
      )
    }
    console.log('Checksum verified (SHA-256).')
    await extractTarGz(bundlePath, extractDir, latest.raw)

    const extractedBinary = path.join(extractDir, 'bin', 'oore-web')
    const extractedDist = path.join(extractDir, 'web-dist')
    const extractedVersion = path.join(extractDir, 'VERSION')
    const extractedLicense = path.join(extractDir, 'LICENSE')

    installUpdateCandidate({
      installRoot,
      extractedBinary,
      extractedDist,
      extractedVersion,
      extractedLicense,
      channel,
      repo,
      activeConfig,
      expectedVersion: latest.raw,
      requireLicense: true,
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
  const requestInit = { headers }
  if (signal) requestInit.signal = signal
  const response = await fetch(new URL('/v1/users/me', backendUrl), requestInit)
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
  const { release, latest } = await fetchVerifiedWebRelease(channel, repo)
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
    Vary: 'Accept-Encoding',
    'Content-Security-Policy': "frame-ancestors 'none'",
    'X-Frame-Options': 'DENY',
  }
}

export function spaFileResponse(filePath, pathname, acceptEncoding = '') {
  const file = Bun.file(filePath)
  const headers = { ...spaResponseHeaders(pathname), 'Content-Type': file.type }
  const encodings = new Map(
    acceptEncoding
      .toLowerCase()
      .split(',')
      .map((part) => {
        const [name, ...parameters] = part.trim().split(';')
        const quality = parameters.find((parameter) =>
          parameter.trim().startsWith('q='),
        )
        return [name.trim(), quality ? Number(quality.trim().slice(2)) : 1]
      }),
  )
  const gzipQuality = encodings.get('gzip') ?? encodings.get('*') ?? 0
  if (
    gzipQuality > 0 &&
    gzipQuality <= 1 &&
    file.size >= 1024 &&
    /\.(?:html|js|css|json|svg|txt|xml)$/.test(filePath)
  ) {
    headers['Content-Encoding'] = 'gzip'
    return new Response(
      file.stream().pipeThrough(new CompressionStream('gzip')),
      { headers },
    )
  }
  return new Response(file, { headers })
}

function serveSpa(distDir, pathname, acceptHeader, acceptEncoding) {
  const assetPath = resolveAssetPath(distDir, pathname)
  if (!assetPath) {
    return new Response('Not found', { status: 404 })
  }

  if (isDirectory(assetPath)) {
    const indexPath = path.join(assetPath, 'index.html')
    if (fileExists(indexPath)) {
      return spaFileResponse(indexPath, '/', acceptEncoding)
    }
  }

  if (fileExists(assetPath)) {
    return spaFileResponse(assetPath, pathname, acceptEncoding)
  }

  const wantsHtml =
    acceptHeader.includes('text/html') || acceptHeader.includes('*/*')
  if (wantsHtml) {
    const indexPath = path.join(distDir, 'index.html')
    if (!fileExists(indexPath)) {
      return new Response('index.html not found', { status: 500 })
    }
    return spaFileResponse(indexPath, '/', acceptEncoding)
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

  if (parsedCommand.command === 'recover-update') {
    if (parsedCommand.args.length > 0) {
      console.error('[oore-web] recover-update does not accept arguments')
      process.exit(2)
    }
    const recovered = await recoverUpdateWithLocks(resolveInstallRoot())
    console.log(
      recovered
        ? 'Restored the previous frontend after an interrupted update.'
        : 'Cleared the frontend update recovery state.',
    )
    return
  }

  if (parsedCommand.command !== 'serve' && parsedCommand.command !== 'run') {
    console.error(`[oore-web] unknown command: ${parsedCommand.command}`)
    printHelp()
    process.exit(2)
  }

  const installRoot = resolveInstallRoot()
  const recoveredUpdate = fs.existsSync(updateTransactionPath(installRoot))
    ? await recoverUpdateWithLocks(installRoot)
    : false
  if (recoveredUpdate) {
    console.warn(
      '[oore-web] restored the previous frontend after an interrupted update',
    )
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
      return serveSpa(
        distDir,
        url.pathname,
        acceptHeader,
        request.headers.get('accept-encoding') || '',
      )
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
