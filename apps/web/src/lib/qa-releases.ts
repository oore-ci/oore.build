import type { Artifact, Build } from '@/lib/types'
import {
  artifactInstallReadiness,
  getIosAppMetadata,
} from '@/lib/artifact-install'

export interface QaRelease {
  artifacts: Array<Artifact>
  build: Build
  version: string
}

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

function bestArtifact(
  artifacts: Array<Artifact>,
  type: 'apk' | 'ipa',
  now: number,
): Artifact | undefined {
  return artifacts
    .filter(
      (artifact) =>
        artifact.artifact_type === type &&
        (artifact.expires_at == null || artifact.expires_at > now) &&
        artifactInstallReadiness(artifact).ready,
    )
    .sort((left, right) => {
      if (type !== 'apk') return right.created_at - left.created_at
      const rank = (name: string) =>
        /universal/i.test(name) ? 2 : /arm64[-_]v8a|arm64/i.test(name) ? 1 : 0
      return rank(right.name) - rank(left.name)
    })[0]
}

export function selectQaProjectReleases(
  projectId: string,
  builds: Array<Build>,
  projectArtifacts: Array<Artifact>,
  now = Math.floor(Date.now() / 1000),
): Array<QaRelease> {
  const fallbackVersion = qaProjectVersionBase(projectArtifacts)
  const artifactsByBuild = new Map<string, Array<Artifact>>()
  for (const artifact of projectArtifacts) {
    const artifacts = artifactsByBuild.get(artifact.build_id) ?? []
    artifacts.push(artifact)
    artifactsByBuild.set(artifact.build_id, artifacts)
  }

  return builds
    .filter(
      (build) => build.project_id === projectId && build.status === 'succeeded',
    )
    .sort((left, right) => right.created_at - left.created_at)
    .flatMap((build) => {
      const buildArtifacts = artifactsByBuild.get(build.id) ?? []
      const artifacts = (['ipa', 'apk'] as const)
        .map((type) => bestArtifact(buildArtifacts, type, now))
        .filter((artifact): artifact is Artifact => artifact !== undefined)
      return artifacts.length > 0
        ? [
            {
              artifacts,
              build,
              version: qaBuildVersion(build, buildArtifacts, fallbackVersion),
            },
          ]
        : []
    })
}
