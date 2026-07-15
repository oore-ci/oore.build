import { describe, expect, it } from 'vitest'

import type { Artifact, Build } from '@/lib/types'
import {
  changelogSummary,
  qaBuildVersion,
  qaProjectVersionBase,
  selectQaProjectReleases,
} from '@/lib/qa-releases'

function build(id: string, projectId: string, createdAt: number): Build {
  return {
    id,
    project_id: projectId,
    pipeline_id: 'pipeline-1',
    build_number: createdAt,
    status: 'succeeded',
    trigger_type: 'manual',
    config_snapshot: {},
    queued_at: createdAt,
    created_at: createdAt,
    updated_at: createdAt,
  }
}

function artifact(
  id: string,
  buildId: string,
  type: 'apk' | 'ipa',
  name: string,
  createdAt: number,
  buildNumber = '12',
): Artifact {
  return {
    id,
    build_id: buildId,
    name,
    artifact_type: type,
    file_path: name,
    metadata:
      type === 'ipa'
        ? {
            ios_app: {
              bundle_identifier: 'build.oore.app',
              display_name: 'Oore App',
              version: '1.2.0',
              build_number: buildNumber,
            },
            ios_signing: {
              bundle_ids: ['build.oore.app'],
              effective_export_method: 'release-testing',
            },
          }
        : {},
    created_at: createdAt,
  }
}

describe('QA release selection', () => {
  it('reduces Markdown changelogs to a quiet one-line summary', () => {
    expect(
      changelogSummary('- **Faster checkout** — Alex\n- Fixed receipts — Sam'),
    ).toBe('Faster checkout — Alex')
  })

  it('keeps every artifact-bearing build and groups its installable platforms under one version', () => {
    const builds = [build('new', 'kite', 13), build('older', 'kite', 12)]
    const artifacts = [
      artifact('new-ipa', 'new', 'ipa', 'Kite.ipa', 13, '13'),
      artifact('apk-32', 'new', 'apk', 'app-armeabi-v7a.apk', 13),
      artifact('apk-64', 'new', 'apk', 'app-arm64-v8a.apk', 13),
      artifact('older-ipa', 'older', 'ipa', 'Kite.ipa', 12),
    ]
    const releases = selectQaProjectReleases('kite', builds, artifacts, 200)

    expect(qaProjectVersionBase(artifacts)).toBe('1.2.0')
    expect(qaBuildVersion(builds[0], artifacts.slice(0, 2), '1.2.0')).toBe(
      '1.2.0+13',
    )
    expect(releases.map((release) => release.version)).toEqual([
      '1.2.0+13',
      '1.2.0+12',
    ])
    expect(releases[0].artifacts.map((candidate) => candidate.id)).toEqual([
      'new-ipa',
      'apk-64',
    ])
    expect(releases[1].artifacts.map((candidate) => candidate.id)).toEqual([
      'older-ipa',
    ])
  })
})
