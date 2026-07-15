import type { Artifact } from '@/lib/types'

export interface IosAppMetadata {
  bundleIdentifier: string
  displayName: string
  version: string
  buildNumber: string
}

export type InstallDevice =
  'iphone-safari' | 'iphone-other' | 'android' | 'other'

function metadataString(
  value: Record<string, unknown>,
  key: string,
): string | null {
  const candidate = value[key]
  return typeof candidate === 'string' && candidate.trim()
    ? candidate.trim()
    : null
}

function metadataObject(
  value: Record<string, unknown>,
  key: string,
): Record<string, unknown> | null {
  const candidate = value[key]
  return candidate !== null &&
    typeof candidate === 'object' &&
    !Array.isArray(candidate)
    ? (candidate as Record<string, unknown>)
    : null
}

export function getIosAppMetadata(artifact: Artifact): IosAppMetadata | null {
  const app = metadataObject(artifact.metadata, 'ios_app')
  if (!app) return null
  const bundleIdentifier = metadataString(app, 'bundle_identifier')
  const displayName = metadataString(app, 'display_name')
  const version = metadataString(app, 'version')
  const buildNumber = metadataString(app, 'build_number')
  if (!bundleIdentifier || !displayName || !version || !buildNumber) return null
  return { bundleIdentifier, displayName, version, buildNumber }
}

export function artifactInstallReadiness(artifact: Artifact): {
  ready: boolean
  reason?: string
} {
  if (artifact.artifact_type === 'apk') return { ready: true }
  if (artifact.artifact_type !== 'ipa') {
    return {
      ready: false,
      reason:
        'Only APK and signed ad-hoc IPA artifacts support device installation.',
    }
  }

  const app = getIosAppMetadata(artifact)
  if (!app) {
    return {
      ready: false,
      reason:
        'This IPA predates install metadata. Rebuild it with the current runner, then install the new artifact.',
    }
  }
  const signing = metadataObject(artifact.metadata, 'ios_signing')
  const exportMethod = signing
    ? metadataString(signing, 'effective_export_method')
    : null
  const bundleIds = signing?.bundle_ids
  const profileMatches =
    Array.isArray(bundleIds) && bundleIds.includes(app.bundleIdentifier)
  if (
    !profileMatches ||
    (exportMethod !== 'ad-hoc' && exportMethod !== 'release-testing')
  ) {
    return {
      ready: false,
      reason: 'This IPA is not signed for registered-device installation.',
    }
  }
  return { ready: true }
}

export function detectInstallDevice(userAgent: string): InstallDevice {
  if (/iPhone|iPod/i.test(userAgent)) {
    const isSafari =
      /Version\/[\d.]+.*Mobile\/\S+.*Safari\//i.test(userAgent) &&
      !/CriOS|FxiOS|EdgiOS|OPiOS|DuckDuckGo|Ddg|GSA/i.test(userAgent)
    return isSafari ? 'iphone-safari' : 'iphone-other'
  }
  if (/Android/i.test(userAgent)) return 'android'
  return 'other'
}
