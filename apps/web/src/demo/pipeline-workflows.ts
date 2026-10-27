import type {
  Pipeline,
  PipelineWorkflowSummary,
  PipelineWorkflowsResponse,
  RepositoryWorkflowPreview,
} from '@oore/client/models'
import { pipelineRepositoryFile } from '@/lib/pipeline-form-utils'
import { PROJECT_IDS } from './seed'
import { demoState } from './state'

interface DemoSigningNames {
  android?: string
  ios?: string
}

/** Overrides for one file's workflows, by workflow name. */
interface DemoWorkflowFile {
  projectId: string
  path: string
  workflows: Record<
    string,
    Partial<Omit<PipelineWorkflowSummary, 'name'>> | undefined
  >
  /** The file's top-level `signing`, and each workflow's replacement. */
  signing?: DemoSigningNames
  workflowSigning?: Record<string, DemoSigningNames | undefined>
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
        platforms: ['android', 'ios'],
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
      nightly: { platforms: ['ios'] },
    },
    signing: { android: 'coin-upload' },
    workflowSigning: {
      adhoc: { ios: 'coin-adhoc' },
      release: { ios: 'coin-app-store' },
      // Not in the library: a manual build is refused with invalid_signing.
      nightly: { ios: 'coin-nightly' },
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

const SIGNING_KINDS = {
  android: 'android_keystore',
  ios: 'ios_distribution',
} as const

/**
 * Like the daemon's `resolve_workflow_binding`: the pipeline's workflow
 * settings first, then the names the file gives under `signing`. Returns
 * the 400 `invalid_signing` message, or undefined when the library can
 * sign the build.
 */
export function demoSigningError(
  pipeline: Pipeline,
  reference: string,
  workflow?: string,
): string | undefined {
  const listing = demoPipelineWorkflows(pipeline, reference)
  const file = DEMO_WORKFLOW_FILES.find(
    (entry) =>
      entry.projectId === pipeline.project_id && entry.path === listing.path,
  )
  const settings = workflow
    ? demoState.workflowSettings[pipeline.id]?.find(
        (entry) => entry.workflow === workflow,
      )
    : undefined
  const fileSigning: Partial<DemoWorkflowFile> = file ?? {}
  const names: DemoSigningNames = {
    ...fileSigning.signing,
    ...(workflow ? fileSigning.workflowSigning?.[workflow] : undefined),
  }
  const scope = workflow ? `workflow ${workflow}` : 'this build'
  const publishDisabled = settings?.publish_enabled === false
  const uploadsToAppStore = (
    listing.workflows.find((entry) => entry.name === workflow)?.publishes ?? []
  ).some((line) => line.startsWith('ios'))
  for (const platform of ['android', 'ios'] as const) {
    const picked =
      platform === 'android'
        ? settings?.android_identity_id
        : settings?.ios_identity_id
    const named = names[platform]
    if (!picked && !named) continue
    const identity = demoState.signingIdentities.find(
      (candidate) =>
        candidate.project_id === pipeline.project_id &&
        (picked ? candidate.id === picked : candidate.name === named),
    )
    if (!identity) {
      return !picked && named
        ? `${scope} signs ${platform} with '${named}', which is not in the project's signing identities; add it under Project settings > Signing, or fix signing.${platform}`
        : `${scope}'s ${platform} signing identity was deleted; pick another in the pipeline's workflow settings`
    }
    if (identity.kind !== SIGNING_KINDS[platform]) {
      return `${scope} signs ${platform} with '${named ?? 'the picked identity'}', which is an ${identity.kind} identity`
    }
    if (
      platform === 'ios' &&
      uploadsToAppStore &&
      !publishDisabled &&
      identity.export_method === 'ad_hoc'
    ) {
      return `${scope} uploads to App Store Connect, but its iOS signing identity holds ad hoc profiles; use an identity with App Store profiles`
    }
  }
  return undefined
}
