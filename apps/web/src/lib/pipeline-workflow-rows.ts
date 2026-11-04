import type {
  BuildPlatform,
  PipelineWorkflowSettings,
  PipelineWorkflowSummary,
  PipelineWorkflowsResponse,
} from '@oore/client/models'

export interface WorkflowRow {
  name: string
  /** What the file plans for it; absent when the file no longer defines it. */
  summary?: PipelineWorkflowSummary
  settings?: PipelineWorkflowSettings
  isDefault: boolean
}

/**
 * The file's workflows, then any with stored settings that the file no
 * longer defines, so their settings can still be cleared.
 */
export function workflowRows(
  listing: PipelineWorkflowsResponse | undefined,
  settings: Array<PipelineWorkflowSettings>,
): Array<WorkflowRow> {
  const byName = new Map(settings.map((entry) => [entry.workflow, entry]))
  const rows: Array<WorkflowRow> = (listing?.workflows ?? []).map(
    (summary) => ({
      name: summary.name,
      summary,
      settings: byName.get(summary.name),
      isDefault: listing?.default_workflow === summary.name,
    }),
  )
  for (const entry of settings) {
    if (rows.some((row) => row.name === entry.workflow)) continue
    rows.push({ name: entry.workflow, settings: entry, isDefault: false })
  }
  return rows
}

/** Platforms the workflow signs for: what it builds, or both when unknown. */
export function signingPlatforms(row: WorkflowRow): Array<BuildPlatform> {
  const built = (row.summary?.platforms ?? []).filter(
    (platform) => platform === 'android' || platform === 'ios',
  )
  return built.length > 0 ? built : ['android', 'ios']
}

/**
 * The signing identity the file names for the workflow on `platform`,
 * which applies when the pipeline's workflow settings pick none.
 */
export function fileSigningName(
  row: WorkflowRow,
  platform: BuildPlatform,
): string | undefined {
  if (platform !== 'android' && platform !== 'ios') return undefined
  return row.summary?.signing?.[platform]?.trim() || undefined
}
