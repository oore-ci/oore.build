import { describe, expect, it } from 'vitest'

import {
  artifactInstallReadiness,
  detectInstallDevice,
  selectInstallArtifact,
} from './artifact-install'
import type { Artifact } from '@/lib/types'

function ipa(metadata: Record<string, unknown>): Artifact {
  return {
    id: 'artifact-1',
    build_id: 'build-1',
    name: 'Kite.ipa',
    artifact_type: 'ipa',
    file_path: 'artifacts/Kite.ipa',
    metadata,
    created_at: 1,
  }
}

describe('artifact install readiness', () => {
  it('accepts an ad-hoc IPA with matching app and profile metadata', () => {
    expect(
      artifactInstallReadiness(
        ipa({
          ios_app: {
            bundle_identifier: 'com.example.kite',
            display_name: 'Kite',
            version: '3.2.1',
            build_number: '42',
          },
          ios_signing: {
            bundle_ids: ['com.example.kite'],
            effective_export_method: 'release-testing',
          },
        }),
      ),
    ).toEqual({ ready: true })
  })

  it('keeps old IPAs downloadable but not install-ready', () => {
    const readiness = artifactInstallReadiness(
      ipa({
        ios_signing: {
          bundle_ids: ['com.example.kite'],
          effective_export_method: 'ad-hoc',
        },
      }),
    )
    expect(readiness.ready).toBe(false)
    expect(readiness.reason).toContain('predates install metadata')
  })
})

describe('install device detection', () => {
  const iphone =
    'Mozilla/5.0 (iPhone; CPU iPhone OS 18_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.5 Mobile/15E148 Safari/604.1'

  it('recognizes native iPhone Safari', () => {
    expect(detectInstallDevice(iphone)).toBe('iphone-safari')
  })

  it('rejects Chrome and Firefox on iPhone', () => {
    expect(
      detectInstallDevice(iphone.replace('Version/18.5', 'CriOS/138.0.0.0')),
    ).toBe('iphone-other')
    expect(
      detectInstallDevice(iphone.replace('Version/18.5', 'FxiOS/140.0')),
    ).toBe('iphone-other')
  })

  it('distinguishes Android and desktop browsers', () => {
    expect(
      detectInstallDevice(
        'Mozilla/5.0 (Linux; Android 16; Pixel 9) AppleWebKit/537.36 Chrome/138 Mobile Safari/537.36',
      ),
    ).toBe('android')
    expect(detectInstallDevice('Mozilla/5.0 (Macintosh; Intel Mac OS X)')).toBe(
      'other',
    )
  })
})

describe('combined install artifact selection', () => {
  const apk = { ...ipa({}), id: 'apk', artifact_type: 'apk' as const }
  const ios = ipa({})

  it('prefers the current phone and otherwise keeps the intended artifact', () => {
    expect(selectInstallArtifact([ios, apk], 'android', ios.id)?.id).toBe('apk')
    expect(selectInstallArtifact([ios, apk], 'iphone-safari', apk.id)?.id).toBe(
      ios.id,
    )
    expect(selectInstallArtifact([ios, apk], 'other', ios.id)?.id).toBe(ios.id)
    expect(selectInstallArtifact([ios, apk], 'other')?.id).toBe('apk')
  })
})
