import * as z from 'zod'
import type { Artifact } from '@oore/client/models'

const jsonObjectSchema = z.record(z.string(), z.json())
type JsonObject = z.infer<typeof jsonObjectSchema>

export interface IosAppMetadata {
  bundleIdentifier: string
  displayName: string
  version: string
  buildNumber: string
}

export type InstallDevice =
  | 'iphone-safari'
  | 'iphone-other'
  | 'android'
  | 'other'

export interface ArtifactInstallReadiness {
  ready: boolean
  reason?: string
}

export function selectInstallArtifact(
  artifacts: Array<Artifact>,
  device: InstallDevice,
  requestedId?: string,
): Artifact | undefined {
  return (
    artifacts.find((artifact) => artifact.id === requestedId) ??
    artifacts.find(
      (artifact) =>
        (device.startsWith('iphone') && artifact.artifact_type === 'ipa') ||
        (device === 'android' && artifact.artifact_type === 'apk'),
    ) ??
    artifacts.find((artifact) => artifact.artifact_type === 'apk') ??
    artifacts.find((artifact) => artifact.artifact_type === 'ipa')
  )
}

function metadataString(value: JsonObject, key: string): string | null {
  const candidate = z.string().safeParse(value[key])
  return candidate.success && candidate.data.trim()
    ? candidate.data.trim()
    : null
}

function metadataObject(value: JsonObject, key: string): JsonObject | null {
  const candidate = jsonObjectSchema.safeParse(value[key])
  return candidate.success ? candidate.data : null
}

export function getIosAppMetadata(artifact: Artifact): IosAppMetadata | null {
  const metadata = jsonObjectSchema.safeParse(artifact.metadata)
  if (!metadata.success) return null
  const app = metadataObject(metadata.data, 'ios_app')
  if (!app) return null
  const bundleIdentifier = metadataString(app, 'bundle_identifier')
  const displayName = metadataString(app, 'display_name')
  const version = metadataString(app, 'version')
  const buildNumber = metadataString(app, 'build_number')
  if (!bundleIdentifier || !displayName || !version || !buildNumber) return null
  return { bundleIdentifier, displayName, version, buildNumber }
}

export function artifactInstallReadiness(
  artifact: Artifact,
): ArtifactInstallReadiness {
  if (artifact.state !== 'available') {
    return {
      ready: false,
      reason: 'This file is no longer available. Open a newer build.',
    }
  }
  if (
    artifact.expires_at != null &&
    artifact.expires_at <= Math.floor(Date.now() / 1000)
  ) {
    return {
      ready: false,
      reason: 'This file has expired. Ask a developer for a fresh build.',
    }
  }
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
  const metadata = jsonObjectSchema.safeParse(artifact.metadata)
  if (!metadata.success) {
    return { ready: false, reason: 'This IPA has invalid install metadata.' }
  }
  const signing = metadataObject(metadata.data, 'ios_signing')
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
