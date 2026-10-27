import type {
  Pipeline,
  PipelineWorkflowSummary,
  PipelineWorkflowsResponse,
  RepositoryWorkflowPreview,
} from '@oore/client/models'
import { pipelineRepositoryFile } from '@/lib/pipeline-form-utils'
import { PROJECT_IDS } from './seed'
import { demoState } from './state'

/** Overrides for one file's workflows, by workflow name. */
interface DemoWorkflowFile {
  projectId: string
  path: string
  workflows: Record<
    string,
    Partial<Omit<PipelineWorkflowSummary, 'name'>> | undefined
  >
}

/** The workflow a demo build plans, or the daemon's refusal. */
interface DemoBuildWorkflow {
  workflow?: string
  error?: string
}

/**
 * What the daemon would plan for each workflow of a demo file, by project
 * and path. Workflows not listed build the file's platforms and follow the
 * pipeline's trigger settings.
 */
const DEMO_WORKFLOW_FILES = [
  {
    projectId: PROJECT_IDS.nativePayments,
    path: '.oore/release.yaml',
    workflows: {
      adhoc: {
        platforms: ['android'],
        triggers: ['pull requests into main'],
        uses_pipeline_triggers: false,
      },
      release: {
        platforms: ['android', 'ios'],
        triggers: ['push to main', 'tag v*'],
        uses_pipeline_triggers: false,
        publishes: [
          'android publishes to the Google Play internal track (workflows.release.targets.android.publish.play)',
          'ios publishes to the TestFlight group Internal (workflows.release.targets.ios.publish.testflight)',
        ],
      },
    },
  },
  {
    projectId: PROJECT_IDS.flutterShop,
    path: '.oore/ios-release.yaml',
    workflows: {
      testflight: {
        triggers: ['push to develop'],
        uses_pipeline_triggers: false,
        publishes: [
          'ios publishes to the TestFlight group Beta testers (workflows.testflight.targets.ios.publish.testflight)',
        ],
      },
      'app-store': { triggers: ['tag v*'], uses_pipeline_triggers: false },
    },
  },
] satisfies Array<DemoWorkflowFile>

/** A full commit SHA or a short one, which the demo reads from the default branch. */
const COMMIT_SHA = /^[0-9a-f]{7,40}$/i

/**
 * The repository files of a demo project at `reference`, or undefined when
 * no such branch exists. Branches other than the default carry their own
 * files over the default branch's.
 */
export function demoFilesAt(
  projectId: string,
  reference: string,
): Array<RepositoryWorkflowPreview> | undefined {
  const project = demoState.projects.find((item) => item.id === projectId)
  const defaultBranch = project?.default_branch ?? 'main'
  const onDefault = demoState.repositoryWorkflows[projectId] ?? []
  const onBranch =
    reference === defaultBranch || COMMIT_SHA.test(reference)
      ? []
      : demoState.repositoryWorkflowBranches[projectId]?.[reference]
  if (!onBranch) return undefined
  return [
    ...onDefault.filter(
      (file) => !onBranch.some((other) => other.path === file.path),
    ),
    ...onBranch,
  ]
}

function summaries(
  projectId: string,
  file: RepositoryWorkflowPreview,
): Array<PipelineWorkflowSummary> {
  const details: DemoWorkflowFile['workflows'] =
    DEMO_WORKFLOW_FILES.find(
      (entry) => entry.projectId === projectId && entry.path === file.path,
    )?.workflows ?? {}
  return file.workflows.map((name) => ({
    name,
    platforms: file.execution?.platforms ?? [],
    triggers: [],
    uses_pipeline_triggers: true,
    publishes: [],
    errors: [],
    ...details[name],
  }))
}

/** `GET /v1/pipelines/{id}/workflows` for a demo pipeline. */
export function demoPipelineWorkflows(
  pipeline: Pipeline,
  requestedReference?: string | null,
): PipelineWorkflowsResponse {
  const project = demoState.projects.find(
    (item) => item.id === pipeline.project_id,
  )
  const reference =
    requestedReference?.trim() || project?.default_branch || 'main'
  const response: PipelineWorkflowsResponse = {
    pipeline_id: pipeline.id,
    reference,
    state: 'missing',
    errors: [],
    default_workflow: pipeline.workflow ?? undefined,
    workflows: [],
  }
  const files = demoFilesAt(pipeline.project_id, reference)
  if (!files) {
    return {
      ...response,
      state: 'unavailable',
      errors: [`Branch or commit '${reference}' was not found`],
    }
  }
  const file = pipelineRepositoryFile(files, pipeline)
  if (!file) return response
  if (!file.valid) {
    return {
      ...response,
      state: 'invalid',
      path: file.path,
      errors: file.errors,
    }
  }
  return {
    ...response,
    state: 'valid',
    path: file.path,
    workflows: summaries(pipeline.project_id, file),
  }
}

/**
 * The workflow a build plans, like the daemon: the request's, then the
 * pipeline's default, then the file's only workflow. `error` is the
 * daemon's 400 `invalid_workflow` message.
 */
export function demoBuildWorkflow(
  pipeline: Pipeline,
  reference: string,
  requested?: string | null,
): DemoBuildWorkflow {
  const listing = demoPipelineWorkflows(pipeline, reference)
  const wanted = requested?.trim() || pipeline.workflow?.trim() || undefined
  // Missing, invalid or unreadable files are left for the runner to report.
  if (listing.state !== 'valid') return { workflow: wanted }
  const names = listing.workflows.map((workflow) => workflow.name)
  const path = listing.path ?? '.oore.yaml'
  if (wanted) {
    if (names.includes(wanted)) return { workflow: wanted }
    return {
      error:
        names.length === 0
          ? `workflow '${wanted}' is not defined; ${path} defines no workflows (read at ${reference})`
          : `workflow '${wanted}' is not defined; ${path} defines [${names.join(', ')}] (read at ${reference})`,
    }
  }
  if (names.length > 1) {
    return {
      error: `${path} defines workflows [${names.join(', ')}]; name one when starting the build, or set a default workflow on the pipeline (read at ${reference})`,
    }
  }
  return { workflow: names[0] }
}
