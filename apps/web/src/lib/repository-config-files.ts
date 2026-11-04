import type {
  PipelineHealth,
  RepositoryConfigFile,
  RepositoryConfigFilesResponse,
} from '@oore/client/models'
import type { PipelineWorkflowProblem } from '@/lib/pipeline-form-utils'

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

/** The one banner a pipeline page shows about its config file. */
export interface PipelineBanner {
  title: string
  description: string
  variant: 'default' | 'destructive'
}

/**
 * The banner a pipeline page shows for its health, or `null` when the file
 * is fine. A missing file is information, not a failure: the pipeline then
 * builds from the settings saved in Oore. It is a failure when the pipeline
 * names a default workflow, which only a file can define.
 */
export function pipelineHealthBanner(
  health: PipelineHealth | null | undefined,
  pipeline?: { config_path: string; workflow?: string | null },
): PipelineBanner | null {
  if (!health || health.state === 'ok') return null
  const description = health.message ?? ''
  switch (health.state) {
    case 'file_missing': {
      const workflow = pipeline?.workflow?.trim()
      if (workflow && pipeline) {
        return pipelineProblemBanner(
          {
            kind: 'missing-file',
            path: health.path ?? pipeline.config_path.trim(),
            workflow,
          },
          health.branch,
        )
      }
      return { title: 'No config file', description, variant: 'default' }
    }
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

/**
 * The same banner from a live read of the repository, for a pipeline the
 * daemon has no health for yet.
 */
export function pipelineProblemBanner(
  problem: PipelineWorkflowProblem,
  branch: string,
): PipelineBanner {
  switch (problem.kind) {
    case 'missing-file':
      return {
        title: `No workflow file on ${branch}`,
        description: `The default workflow ${problem.workflow} comes from ${problem.path}, which is not on ${branch}. Builds of ${branch} will fail.`,
        variant: 'destructive',
      }
    case 'invalid-file':
      return {
        title: 'Config file is invalid',
        description: `${problem.path} on ${branch} has errors, so builds from it fail: ${problem.errors.join('; ')}`,
        variant: 'destructive',
      }
    case 'unknown-workflow':
      return {
        title: 'Workflow is missing',
        description: `${problem.path} on ${branch} no longer defines workflow ${problem.workflow}. Builds that do not name a workflow will fail until you change the default.`,
        variant: 'destructive',
      }
  }
}
