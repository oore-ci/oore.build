import type {
  PipelineHealth,
  RepositoryConfigFile,
  RepositoryConfigFilesResponse,
} from '@oore/client/models'

/** Config files in the repository that no pipeline builds from yet. */
export function unusedConfigFiles(
  response: RepositoryConfigFilesResponse | undefined,
): Array<RepositoryConfigFile> {
  return (response?.files ?? []).filter((file) => file.pipelines.length === 0)
}

/** Unused files that can become pipelines: the valid ones. */
export function addableConfigFiles(
  response: RepositoryConfigFilesResponse | undefined,
): Array<RepositoryConfigFile> {
  return unusedConfigFiles(response).filter((file) => file.valid)
}

/** One line about a config file for its row. */
export function describeConfigFile(file: RepositoryConfigFile): string {
  if (!file.valid) return file.errors[0] ?? 'The file is invalid'
  if (file.workflows.length === 0) return 'Builds as one workflow'
  return `Workflows: ${file.workflows.join(', ')}`
}

/**
 * The banner a pipeline page shows for its health, or `null` when the file
 * is fine. A missing file is information, not a failure: the pipeline then
 * builds from the settings saved in Oore.
 */
export function pipelineHealthBanner(
  health: PipelineHealth | null | undefined,
): {
  title: string
  description: string
  variant: 'default' | 'destructive'
} | null {
  if (!health || health.state === 'ok') return null
  const description = health.message ?? ''
  switch (health.state) {
    case 'file_missing':
      return { title: 'No config file', description, variant: 'default' }
    case 'file_invalid':
      return {
        title: 'Config file is invalid',
        description,
        variant: 'destructive',
      }
    case 'workflow_missing':
      return {
        title: 'Workflow is missing',
        description,
        variant: 'destructive',
      }
  }
}
