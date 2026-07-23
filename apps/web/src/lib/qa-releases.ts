import type { Artifact, Build } from '@/lib/types'
import { getIosAppMetadata } from '@/lib/artifact-install'

export function changelogSummary(markdown: string): string {
  const firstLine = markdown
    .split('\n')
    .map((line) => line.trim())
    .find(Boolean)
  return (firstLine ?? '')
    .replace(/^[-*+]\s+/, '')
    .replace(/[*_`#>[\]]/g, '')
    .replace(/\s+/g, ' ')
    .trim()
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

function metadataString(
  value: Record<string, unknown>,
  ...keys: Array<string>
): string | null {
  for (const key of keys) {
    const candidate = value[key]
    if (typeof candidate === 'string' && candidate.trim())
      return candidate.trim()
    if (typeof candidate === 'number' && Number.isFinite(candidate))
      return String(candidate)
  }
  return null
}

function artifactVersion(
  artifact: Artifact,
): { name: string; number: string } | null {
  const ios = getIosAppMetadata(artifact)
  if (ios) return { name: ios.version, number: ios.buildNumber }

  const android = metadataObject(artifact.metadata, 'android_app')
  if (!android) return null
  const name = metadataString(android, 'version_name', 'version')
  const number = metadataString(android, 'version_code', 'build_number')
  return name && number ? { name, number } : null
}

export function qaProjectVersionBase(
  artifacts: Array<Artifact>,
): string | null {
  return (
    [...artifacts]
      .sort((left, right) => right.created_at - left.created_at)
      .map(artifactVersion)
      .find((version) => version !== null)?.name ?? null
  )
}

export function qaBuildVersion(
  build: Build,
  artifacts: Array<Artifact>,
  fallbackVersion: string | null,
): string {
  const exact = artifacts
    .map(artifactVersion)
    .find((version) => version !== null)
  return exact
    ? `${exact.name}+${exact.number}`
    : fallbackVersion
      ? `${fallbackVersion}+${build.build_number}`
      : `Build ${build.build_number}`
}
