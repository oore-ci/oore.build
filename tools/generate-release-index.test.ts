import { describe, expect, test } from 'bun:test'
import { mkdtemp, readFile, readdir, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import path from 'node:path'

import { buildReleaseIndex, type GitHubRelease } from './generate-release-index'

function release(
  tag_name: string,
  overrides: Partial<GitHubRelease> = {},
): GitHubRelease {
  return {
    tag_name,
    name: tag_name,
    body: '',
    html_url: `https://github.com/oore-ci/oore.build/releases/tag/${tag_name}`,
    published_at: '2026-10-01T00:00:00Z',
    created_at: '2026-09-30T00:00:00Z',
    draft: false,
    prerelease: tag_name.includes('-'),
    ...overrides,
  }
}

function channel(
  index: ReturnType<typeof buildReleaseIndex>,
  name: 'alpha' | 'beta' | 'stable',
) {
  return index.find((entry) => entry.channel === name)!.entries
}

describe('buildReleaseIndex', () => {
  test('sorts each channel newest first by semantic version', () => {
    const index = buildReleaseIndex(
      [
        release('v0.1.9'),
        release('v0.1.10'),
        release('v0.1.10-alpha.2'),
        release('v0.1.10-alpha.10'),
        release('v0.2.0-beta.1'),
      ],
      'oore-ci/oore.build',
    )

    expect(channel(index, 'stable').map((entry) => entry.tag)).toEqual([
      'v0.1.10',
      'v0.1.9',
    ])
    expect(channel(index, 'alpha').map((entry) => entry.tag)).toEqual([
      'v0.1.10-alpha.10',
      'v0.1.10-alpha.2',
    ])
    expect(channel(index, 'beta').map((entry) => entry.version)).toEqual([
      '0.2.0-beta.1',
    ])
  })

  test('drops drafts, foreign tags and releases flagged for the wrong channel', () => {
    const index = buildReleaseIndex(
      [
        release('v0.1.11', { draft: true }),
        release('v0.1.12', { prerelease: true }),
        release('v0.1.12-alpha.1', { prerelease: false }),
        release('v0.1.12-rc.1'),
        release('nightly'),
        release('v0.1.10'),
      ],
      'oore-ci/oore.build',
    )

    expect(index.flatMap((entry) => entry.entries).map((e) => e.tag)).toEqual([
      'v0.1.10',
    ])
  })

  test('flattens paginated sources and fills entry details', () => {
    const notes = `Fixes.\n\n**Full Changelog**: https://github.com/oore-ci/oore.build/compare/v0.1.9...v0.1.10`
    const index = buildReleaseIndex(
      [
        [release('0.1.10', { body: notes, name: '', published_at: null })],
        [release('v0.1.9', { body: null })],
      ],
      'example/fork',
    )

    expect(channel(index, 'stable')).toEqual([
      {
        schema_version: 1,
        channel: 'stable',
        version: '0.1.10',
        tag: 'v0.1.10',
        published_at: '2026-09-30T00:00:00Z',
        release_name: '0.1.10',
        release_notes: notes,
        release_url:
          'https://github.com/oore-ci/oore.build/releases/tag/0.1.10',
        changelog_url:
          'https://github.com/oore-ci/oore.build/compare/v0.1.9...v0.1.10',
        download_base_url:
          'https://github.com/example/fork/releases/download/0.1.10',
      },
      expect.objectContaining({
        tag: 'v0.1.9',
        release_notes: '',
        changelog_url:
          'https://github.com/oore-ci/oore.build/releases/tag/v0.1.9',
      }),
    ])
  })
})

describe('generate-release-index CLI', () => {
  test('writes channel histories, latest manifests and headers', async () => {
    const directory = await mkdtemp(path.join(tmpdir(), 'oore-release-index-'))
    try {
      const source = path.join(directory, 'releases.json')
      const output = path.join(directory, 'index')
      await Bun.write(
        source,
        JSON.stringify([release('v0.1.10'), release('v0.1.11-beta.1')]),
      )

      const result = Bun.spawnSync([
        process.execPath,
        path.join(import.meta.dir, 'generate-release-index.ts'),
        source,
        output,
      ])
      expect(result.stderr.toString()).toBe('')
      expect(result.exitCode).toBe(0)

      expect((await readdir(output)).sort()).toEqual([
        '_headers',
        'alpha.json',
        'beta.json',
        'latest',
        'stable.json',
      ])
      // Channels without a release publish an empty history and no latest manifest.
      expect((await readdir(path.join(output, 'latest'))).sort()).toEqual([
        'beta.json',
        'stable.json',
      ])
      expect(
        JSON.parse(await readFile(path.join(output, 'alpha.json'), 'utf8')),
      ).toEqual({ schema_version: 1, channel: 'alpha', releases: [] })
      expect(
        JSON.parse(
          await readFile(path.join(output, 'latest/stable.json'), 'utf8'),
        ),
      ).toMatchObject({
        tag: 'v0.1.10',
        download_base_url:
          'https://github.com/oore-ci/oore.build/releases/download/v0.1.10',
      })
      const headers = await readFile(path.join(output, '_headers'), 'utf8')
      for (const rule of ['/latest/*', '/*.json.sig', '/stable.json']) {
        expect(headers).toContain(`${rule}\n  Access-Control-Allow-Origin: *`)
      }
    } finally {
      await rm(directory, { recursive: true, force: true })
    }
  })

  test('fails without input and output paths', () => {
    const result = Bun.spawnSync([
      process.execPath,
      path.join(import.meta.dir, 'generate-release-index.ts'),
    ])
    expect(result.exitCode).not.toBe(0)
    expect(result.stderr.toString()).toContain('usage:')
  })
})
