import { afterAll, describe, expect, test } from 'bun:test'
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'

const script = path.join(import.meta.dir, 'require-current-release.sh')
const workdir = mkdtempSync(path.join(tmpdir(), 'require-current-release-'))
afterAll(() => rmSync(workdir, { recursive: true, force: true }))

const releases = [
  [
    {
      tag_name: 'v0.1.47-alpha.10',
      draft: false,
      published_at: '2026-10-20T10:00:00Z',
    },
    {
      tag_name: 'v0.1.47-alpha.9',
      draft: false,
      published_at: '2026-10-19T10:00:00Z',
    },
    {
      tag_name: 'v0.1.47-alpha.11',
      draft: true,
      published_at: null,
      created_at: '2026-10-21T10:00:00Z',
    },
  ],
  [{ tag_name: 'v0.1.46', draft: false, published_at: '2026-10-18T10:00:00Z' }],
]
const releasesFile = path.join(workdir, 'releases.json')
writeFileSync(releasesFile, JSON.stringify(releases))

function run(...args: Array<string>) {
  const result = Bun.spawnSync(['bash', script, ...args], {
    env: { ...process.env, GH_RELEASES_JSON: releasesFile },
  })
  return { code: result.exitCode, stderr: result.stderr.toString() }
}

function index(latest: Record<string, string>) {
  const dir = mkdtempSync(path.join(workdir, 'index-'))
  mkdirSync(path.join(dir, 'latest'))
  for (const [channel, tag] of Object.entries(latest)) {
    writeFileSync(
      path.join(dir, 'latest', `${channel}.json`),
      JSON.stringify({ tag }),
    )
  }
  return dir
}

describe('require-current-release', () => {
  test('accepts the newest published tag and a new tag', () => {
    expect(run('v0.1.47-alpha.10').code).toBe(0)
    expect(run('v0.1.47-alpha.11').code).toBe(0)
    expect(run('v0.1.46').code).toBe(0)
  })

  test('refuses a tag older than the newest published tag in its channel', () => {
    const result = run('v0.1.47-alpha.9')
    expect(result.code).toBe(1)
    expect(result.stderr).toContain('older than published v0.1.47-alpha.10')
  })

  test('--most-recent refuses a release published before another channel', () => {
    expect(run('v0.1.46', '--most-recent').code).toBe(1)
    expect(run('v0.1.47-alpha.10', '--most-recent').code).toBe(0)
  })

  test('--index requires every channel to name its newest release', () => {
    const current = index({ alpha: 'v0.1.47-alpha.10', stable: 'v0.1.46' })
    expect(run('v0.1.47-alpha.10', '--index', current).code).toBe(0)

    const staleStable = index({ alpha: 'v0.1.47-alpha.10', stable: 'v0.1.45' })
    expect(run('v0.1.47-alpha.10', '--index', staleStable).stderr).toContain(
      'latest stable release, but v0.1.46 is published',
    )

    const missingStable = index({ alpha: 'v0.1.47-alpha.10' })
    expect(run('v0.1.47-alpha.10', '--index', missingStable).code).toBe(1)
  })

  test('rejects an unsupported tag', () => {
    expect(run('v0.1.47-dev').code).toBe(1)
  })
})
