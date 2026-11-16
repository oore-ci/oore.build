import type {
  BuildPlatform,
  PipelineWorkflowSettings,
  PipelineWorkflowSummary,
  PipelineWorkflowsResponse,
  ScmProvider,
} from '@oore/client/models'

/**
 * Where a value in force comes from, in the order the daemon resolves
 * signing, publishing and approval (concepts/files-pipelines-workflows):
 * the pipeline's workflow settings, then the file, then the pipeline.
 */
export type ValueSource =
  | { kind: 'file'; path: string }
  | { kind: 'here' }
  | { kind: 'workflow'; workflow: string }

/** One value in force and where it comes from. */
export interface SourcedValue {
  value: string
  source: ValueSource
  /**
   * The workflows it applies to. Empty when it applies to every build,
   * including when the file defines no workflows.
   */
  workflows: Array<string>
}

/** The pipeline's file when the daemon could read it, with its workflows. */
export interface PipelineFile {
  path: string
  reference?: string
  workflows: Array<PipelineWorkflowSummary>
  defaultWorkflow?: string
}

export function pipelineFile(
  listing: PipelineWorkflowsResponse | undefined,
): PipelineFile | null {
  if (!listing || listing.state !== 'valid' || !listing.path) return null
  return {
    path: listing.path,
    reference: listing.reference ?? undefined,
    workflows: listing.workflows,
    defaultWorkflow: listing.default_workflow ?? undefined,
  }
}

/** Workflows that build `platform`; an unplanned workflow counts. */
function workflowsBuilding(
  file: PipelineFile,
  platform: BuildPlatform,
): Array<PipelineWorkflowSummary> {
  return file.workflows.filter(
    (workflow) =>
      workflow.platforms.length === 0 || workflow.platforms.includes(platform),
  )
}

function sameSource(a: ValueSource, b: ValueSource) {
  if (a.kind !== b.kind) return false
  if (a.kind === 'file' && b.kind === 'file') return a.path === b.path
  if (a.kind === 'workflow' && b.kind === 'workflow')
    return a.workflow === b.workflow
  return true
}

/**
 * One entry per workflow, merged when value and source match. A merged
 * entry that covers every workflow applies to all builds.
 */
function merge(
  perWorkflow: Array<{ workflow: string; value: string; source: ValueSource }>,
): Array<SourcedValue> {
  const entries: Array<SourcedValue> = []
  for (const item of perWorkflow) {
    const existing = entries.find(
      (entry) =>
        entry.value === item.value && sameSource(entry.source, item.source),
    )
    if (existing) existing.workflows.push(item.workflow)
    else
      entries.push({
        value: item.value,
        source: item.source,
        workflows: [item.workflow],
      })
  }
  if (entries.length === 1) entries[0].workflows = []
  return entries
}

function settingsByWorkflow(settings: Array<PipelineWorkflowSettings>) {
  return new Map(settings.map((entry) => [entry.workflow, entry]))
}

/** Whether any entry falls through to what is set on the pipeline. */
export function usesPipeline(entries: Array<SourcedValue>) {
  return entries.some((entry) => entry.source.kind === 'here')
}

/**
 * The signing identity each workflow uses on `platform`: its workflow
 * setting, then the file's `signing`, then the pipeline's own.
 */
export function signingInForce({
  file,
  identityName,
  pipelineValue,
  platform,
  settings,
}: {
  file: PipelineFile | null
  /** The library name of an identity picked in workflow settings. */
  identityName: (id: string) => string
  /** What the pipeline's own signing says, such as `Not signed`. */
  pipelineValue: string
  platform: 'android' | 'ios'
  settings: Array<PipelineWorkflowSettings>
}): Array<SourcedValue> {
  if (!file || file.workflows.length === 0)
    return [{ value: pipelineValue, source: { kind: 'here' }, workflows: [] }]
  const byName = settingsByWorkflow(settings)
  return merge(
    workflowsBuilding(file, platform).map((workflow) => {
      const picked =
        platform === 'android'
          ? byName.get(workflow.name)?.android_identity_id
          : byName.get(workflow.name)?.ios_identity_id
      if (picked)
        return {
          workflow: workflow.name,
          value: identityName(picked),
          source: { kind: 'workflow', workflow: workflow.name },
        }
      const named = workflow.signing?.[platform]?.trim()
      if (named)
        return {
          workflow: workflow.name,
          value: named,
          source: { kind: 'file', path: file.path },
        }
      return {
        workflow: workflow.name,
        value: pipelineValue,
        source: { kind: 'here' },
      }
    }),
  )
}

/** `android publishes to the Google Play internal track (workflows…)` → `the Google Play internal track`. */
export function describePublishLine(line: string) {
  return line
    .replace(/^(android|ios) publishes to /, '')
    .replace(/ \([^()]*\)$/, '')
}

/**
 * Where each workflow's builds for `platform` go: off in its workflow
 * settings, then the file's `publish`, then the pipeline's Publishing.
 */
export function publishingInForce({
  file,
  pipelineValue,
  platform,
  settings,
}: {
  file: PipelineFile | null
  pipelineValue: string
  platform: 'android' | 'ios'
  settings: Array<PipelineWorkflowSettings>
}): Array<SourcedValue> {
  if (!file || file.workflows.length === 0)
    return [{ value: pipelineValue, source: { kind: 'here' }, workflows: [] }]
  const byName = settingsByWorkflow(settings)
  return merge(
    workflowsBuilding(file, platform).map((workflow) => {
      if (byName.get(workflow.name)?.publish_enabled === false)
        return {
          workflow: workflow.name,
          value: 'Off',
          source: { kind: 'workflow', workflow: workflow.name },
        }
      const line = workflow.publishes.find((entry) =>
        entry.startsWith(`${platform} `),
      )
      if (line)
        return {
          workflow: workflow.name,
          value: `Uploads to ${describePublishLine(line)}`,
          source: { kind: 'file', path: file.path },
        }
      return {
        workflow: workflow.name,
        value: pipelineValue,
        source: { kind: 'here' },
      }
    }),
  )
}

/**
 * What starts each workflow on its own: the workflow's `on` (or the
 * file's), else the pipeline's trigger settings. With a default workflow,
 * webhooks start only that one.
 */
export function triggersInForce({
  file,
  pipelineValue,
}: {
  file: PipelineFile | null
  pipelineValue: string
}): Array<SourcedValue> {
  if (!file || file.workflows.length === 0)
    return [{ value: pipelineValue, source: { kind: 'here' }, workflows: [] }]
  const started = file.defaultWorkflow
    ? file.workflows.filter(
        (workflow) => workflow.name === file.defaultWorkflow,
      )
    : file.workflows
  if (started.length === 0)
    return [{ value: pipelineValue, source: { kind: 'here' }, workflows: [] }]
  const entries = merge(
    started.map((workflow) =>
      workflow.uses_pipeline_triggers
        ? {
            workflow: workflow.name,
            value: pipelineValue,
            source: { kind: 'here' },
          }
        : {
            workflow: workflow.name,
            value: workflow.triggers.join(', ') || 'Only when run by hand',
            source: { kind: 'file', path: file.path },
          },
    ),
  )
  // A default workflow is named even when it is the only one started.
  if (file.defaultWorkflow && entries.length === 1)
    entries[0].workflows = [file.defaultWorkflow]
  return entries
}

/** Approval per workflow: its workflow setting, else the pipeline's policy. */
export function approvalInForce({
  file,
  pipelineValue,
  settings,
}: {
  file: PipelineFile | null
  pipelineValue: string
  settings: Array<PipelineWorkflowSettings>
}): Array<SourcedValue> {
  if (!file || file.workflows.length === 0)
    return [{ value: pipelineValue, source: { kind: 'here' }, workflows: [] }]
  const byName = settingsByWorkflow(settings)
  return merge(
    file.workflows.map((workflow) => {
      const required = byName.get(workflow.name)?.approval_required
      if (required === true || required === false)
        return {
          workflow: workflow.name,
          value: required ? 'Required' : 'Not required',
          source: { kind: 'workflow', workflow: workflow.name },
        }
      return {
        workflow: workflow.name,
        value: pipelineValue,
        source: { kind: 'here' },
      }
    }),
  )
}

/**
 * One line for a row header: the file, "set here" and how many workflow
 * overrides, in that order, each named once.
 */
export function sourceSummary(entries: Array<SourcedValue>): Array<string> {
  const parts: Array<string> = []
  const file = entries.find((entry) => entry.source.kind === 'file')
  if (file && file.source.kind === 'file')
    parts.push(`from ${file.source.path}`)
  if (usesPipeline(entries)) parts.push('set here')
  const overrides = new Set(
    entries.flatMap((entry) =>
      entry.source.kind === 'workflow' ? [entry.source.workflow] : [],
    ),
  )
  if (overrides.size === 1)
    parts.push(`overridden for workflow ${[...overrides][0]}`)
  else if (overrides.size > 1)
    parts.push(`overridden for ${overrides.size} workflows`)
  return parts
}

const DEFAULT_HOSTS = {
  github: 'https://github.com',
  gitlab: 'https://gitlab.com',
} as const

/**
 * The web page of a file in the repository, or null for local Git and
 * unknown repositories.
 */
export function repositoryFileUrl({
  fullName,
  host,
  path,
  provider,
  reference,
}: {
  fullName?: string | null
  host?: string | null
  path: string
  provider?: ScmProvider | null
  reference?: string | null
}): string | null {
  if (
    !fullName ||
    !reference ||
    (provider !== 'github' && provider !== 'gitlab')
  )
    return null
  const base = (host || DEFAULT_HOSTS[provider])
    .replace(/\/+$/, '')
    .replace('://api.github.com', '://github.com')
  const file = path.replace(/^\.?\/+/, '')
  const ref = encodeURIComponent(reference).replaceAll('%2F', '/')
  return provider === 'gitlab'
    ? `${base}/${fullName}/-/blob/${ref}/${file}`
    : `${base}/${fullName}/blob/${ref}/${file}`
}
