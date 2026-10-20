import assert from 'node:assert/strict'
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { gunzipSync } from 'node:zlib'
import { test } from 'bun:test'
import {
  evaluateSystemdWebUnit,
  ownerCheckFailureReason,
  parseSavedWebUpdateState,
  parseSystemctlShow,
  restoreWebUpdateState,
  spaFileResponse,
} from './oore-web.js'

test('static assets negotiate gzip without changing bytes, type or cache policy', async () => {
  const directory = mkdtempSync(join(tmpdir(), 'oore-compression-'))
  try {
    const file = join(directory, 'app.js')
    const source = 'console.log("oore");\n'.repeat(100)
    writeFileSync(file, source)
    for (const encoding of ['gzip', 'br, gzip;q=0.5', '*;q=1']) {
      const response = spaFileResponse(file, '/assets/app.js', encoding)
      assert.equal(response.headers.get('Content-Encoding'), 'gzip')
      assert.equal(response.headers.get('Vary'), 'Accept-Encoding')
      assert.match(response.headers.get('Content-Type'), /javascript/)
      assert.match(response.headers.get('Cache-Control'), /immutable/)
      const bytes = await response.arrayBuffer()
      assert.ok(bytes.byteLength < source.length)
      assert.equal(gunzipSync(bytes).toString(), source)
    }
    for (const encoding of ['', 'br', 'gzip;q=0, *;q=1', 'gzip;q=invalid']) {
      const response = spaFileResponse(file, '/assets/app.js', encoding)
      assert.equal(response.headers.get('Content-Encoding'), null)
      assert.equal(await response.text(), source)
    }
  } finally {
    rmSync(directory, { recursive: true })
  }
})

test('a systemd unit is managed only when it restarts this process after an update', () => {
  const loaded = parseSystemctlShow(
    'LoadState=loaded\nRestart=on-failure\nMainPID=4242\n',
  )
  assert.equal(
    evaluateSystemdWebUnit(parseSystemctlShow('LoadState=not-found\n'), {
      scope: 'system',
    }),
    null,
  )
  assert.equal(
    evaluateSystemdWebUnit(loaded, { scope: 'system', pid: 4242 }).managed,
    true,
  )
  assert.equal(
    evaluateSystemdWebUnit(
      { ...loaded, Restart: 'always' },
      { scope: 'system' },
    ).managed,
    true,
  )
  assert.match(
    evaluateSystemdWebUnit({ ...loaded, Restart: 'no' }, { scope: 'system' })
      .reason,
    /Restart=no/,
  )
  assert.match(
    evaluateSystemdWebUnit(loaded, { scope: 'system', pid: 1 }).reason,
    /not the one/,
  )
  assert.match(
    evaluateSystemdWebUnit(loaded, { scope: 'user', linger: false }).reason,
    /enable-linger/,
  )
  assert.equal(
    evaluateSystemdWebUnit(loaded, { scope: 'user', linger: true }).managed,
    true,
  )
})

test('a failed frontend update keeps its reason across a restart', () => {
  const failed = parseSavedWebUpdateState(
    JSON.parse(
      '{"phase":"failed","error":"checksum mismatch","log_path":"/opt/oore/logs/oore-web-update.log","from_version":"0.1.46","target_version":null}',
    ),
  )
  assert.deepEqual(restoreWebUpdateState(failed, '0.1.46'), {
    phase: 'failed',
    error: 'checksum mismatch',
    log_path: '/opt/oore/logs/oore-web-update.log',
    from_version: '0.1.46',
  })
  assert.equal(restoreWebUpdateState(failed, '0.1.47').phase, 'idle')
  assert.equal(
    restoreWebUpdateState(
      parseSavedWebUpdateState({
        phase: 'restarting',
        target_version: '0.1.47',
      }),
      '0.1.47',
    ).phase,
    'idle',
  )
  assert.match(
    restoreWebUpdateState(
      parseSavedWebUpdateState({
        phase: 'restarting',
        target_version: '0.1.47',
      }),
      '0.1.46',
    ).error,
    /instead of 0\.1\.47/,
  )
  assert.match(
    restoreWebUpdateState(
      parseSavedWebUpdateState({ phase: 'updating' }),
      '0.1.46',
    ).error,
    /interrupted/,
  )
  assert.equal(
    restoreWebUpdateState(parseSavedWebUpdateState([]), '0.1.46').phase,
    'idle',
  )
})

test('the owner check explains identity headers stripped behind a trusted proxy', () => {
  const config = {
    trustedProxySecret: 'backend-proof',
    trustedProxyUserEmailHeader: 'x-oore-user-email',
    upstreamTrustedProxySecret: '',
    upstreamTrustedProxySecretHeader: 'x-oore-web-trusted-proxy-secret',
  }
  const request = (headers) =>
    new Request('http://127.0.0.1:4173/__oore_web_update', { headers })
  const identity = { 'x-oore-user-email': 'owner@example.com' }

  assert.match(
    ownerCheckFailureReason(request(identity), 401, config),
    /upstream trusted proxy secret/,
  )
  const withUpstream = { ...config, upstreamTrustedProxySecret: 'edge-proof' }
  assert.match(
    ownerCheckFailureReason(request({}), 401, withUpstream),
    /did not send the x-oore-user-email header/,
  )
  assert.match(
    ownerCheckFailureReason(
      request({ ...identity, 'x-oore-web-trusted-proxy-secret': 'wrong' }),
      401,
      withUpstream,
    ),
    /without a matching x-oore-web-trusted-proxy-secret/,
  )
  assert.match(
    ownerCheckFailureReason(request(identity), 401, {
      ...config,
      trustedProxySecret: '',
    }),
    /Sign in again/,
  )
  assert.match(
    ownerCheckFailureReason(request(identity), 500, config),
    /HTTP 500/,
  )
})
