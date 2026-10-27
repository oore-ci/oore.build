import type {
  Pipeline,
  PipelineHealth,
  RepositoryConfigFile,
  RepositoryConfigFilesResponse,
} from '@oore/client/models'
import { ago } from './seed'
import { demoState } from './state'

/**
 * What discovery lists: `.oore.yaml`, `.oore.yml`, root `.oore.<name>.yaml`,
 * and `.oore/*.yaml` one directory deep. Other files are read only when
 * their path is asked for.
 */
export const DISCOVERED_PATH =
  /^(\.oore(\.[^/]+)?\.ya?ml|\.oore\/([^/]+\/)?[^/]+\.ya?ml)$/

const DEMO_COMMIT = '4f9c2ab7d1e35c08b6a2f417e9d0c3b5a8e61f22'
const scannedAt = new Map<string, number>()

function lastScan(projectId: string): number {
  let at = scannedAt.get(projectId)
  if (at === undefined) {
    at = ago(60 * 12)
    scannedAt.set(projectId, at)
  }
  return at
}

/** Mirrors the daemon: `.oore.release.yaml` is `Release`. */
export function demoSuggestedName(path: string): string {
  const stem = path.replace(/\.ya?ml$/, '')
  const rest = stem.startsWith('.oore/')
    ? stem.slice('.oore/'.length)
    : stem.startsWith('.oore.')
      ? stem.slice('.oore.'.length)
      : stem === '.oore'
        ? ''
        : stem
  const words = rest
    .split(/[/.\-_ ]/)
    .filter(Boolean)
    .join(' ')
  return words ? words[0].toUpperCase() + words.slice(1) : 'Default'
}

function candidates(configPath: string): Array<string> {
  const path = configPath.trim()
  return path === '.oore.yaml' ? [path, '.oore.yml'] : [path]
}

function resolve(
  files: ReadonlyArray<{ path: string }>,
  configPath: string,
): string | undefined {
  return candidates(configPath).find((path) =>
    files.some((file) => file.path === path),
  )
}

function defaultBranch(projectId: string): string | undefined {
  return (
    demoState.projects.find((project) => project.id === projectId)
      ?.default_branch ?? undefined
  )
}

/** The cached config files for a demo project, as the daemon lists them. */
export function demoRepositoryConfigFiles(
  projectId: string,
): RepositoryConfigFilesResponse {
  const branch = defaultBranch(projectId)
  const discovered = demoState.repositoryWorkflows[projectId]
  if (!branch || !discovered) {
    return { project_id: projectId, branch, truncated: false, files: [] }
  }
  const pipelines = demoState.pipelines.filter(
    (pipeline) => pipeline.project_id === projectId,
  )
  const files: Array<RepositoryConfigFile> = discovered
    .filter((file) => DISCOVERED_PATH.test(file.path))
    .map((file) => ({
    path: file.path,
    valid: file.valid,
    errors: file.errors,
    workflows: file.workflows,
    suggested_name: demoSuggestedName(file.path),
    pipelines: pipelines
      .filter(
        (pipeline) => resolve(discovered, pipeline.config_path) === file.path,
      )
      .map((pipeline) => ({ id: pipeline.id, name: pipeline.name })),
    seen_at: ago(86400 * 2),
  }))
  return {
    project_id: projectId,
    branch,
    commit_sha: DEMO_COMMIT,
    scanned_at: lastScan(projectId),
    truncated: false,
    files,
  }
}

export function refreshDemoRepositoryConfigFiles(
  projectId: string,
): RepositoryConfigFilesResponse {
  scannedAt.set(projectId, ago(0))
  return demoRepositoryConfigFiles(projectId)
}

function demoHealth(pipeline: Pipeline): PipelineHealth | undefined {
  const branch = defaultBranch(pipeline.project_id)
  const discovered = demoState.repositoryWorkflows[pipeline.project_id]
  if (!branch || !discovered) return undefined
  const base = {
    branch,
    commit_sha: DEMO_COMMIT,
    checked_at: lastScan(pipeline.project_id),
  }
  const path = resolve(discovered, pipeline.config_path)
  const file = discovered.find((candidate) => candidate.path === path)
  if (!file) {
    return {
      ...base,
      state: 'file_missing',
      message: `${pipeline.config_path} is not on ${branch}, so builds use the settings saved in Oore`,
    }
  }
  if (!file.valid) {
    return {
      ...base,
      state: 'file_invalid',
      path: file.path,
      message: `${file.path} on ${branch} is invalid: ${file.errors[0] ?? 'unknown error'}`,
    }
  }
  if (pipeline.workflow && !file.workflows.includes(pipeline.workflow)) {
    return {
      ...base,
      state: 'workflow_missing',
      path: file.path,
      message: `${file.path} on ${branch} no longer defines workflow ${pipeline.workflow}`,
    }
  }
  return { ...base, state: 'ok', path: file.path }
}

/** A pipeline as the daemon returns it, with health from the cache. */
export function withDemoHealth(pipeline: Pipeline): Pipeline {
  const health = demoHealth(pipeline)
  return health ? { ...pipeline, health } : pipeline
}
