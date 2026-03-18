import { HttpResponse, delay, http } from 'msw'
import { demoLastCleanup, demoRetentionPolicy } from '../data/retention'
import type {
  ProjectRetentionOverride,
  RetentionCleanupSummary,
  RetentionPolicy,
} from '@/lib/types'

function now(): number {
  return Math.floor(Date.now() / 1000)
}

let globalPolicy: RetentionPolicy = { ...demoRetentionPolicy }
const lastCleanup: RetentionCleanupSummary = { ...demoLastCleanup }
const projectOverrides = new Map<string, ProjectRetentionOverride>()

function effectivePolicy(projectId: string) {
  const override = projectOverrides.get(projectId)
  if (!override) {
    return { effective: { ...globalPolicy }, has_override: false }
  }
  return {
    effective: {
      enabled: override.enabled ?? globalPolicy.enabled,
      max_age_days: override.max_age_days ?? globalPolicy.max_age_days,
      max_builds_per_project:
        override.max_builds_per_project ?? globalPolicy.max_builds_per_project,
      max_artifact_size_bytes:
        override.max_artifact_size_bytes ??
        globalPolicy.max_artifact_size_bytes,
      cleanup_target:
        override.cleanup_target ?? globalPolicy.cleanup_target,
      keep_statuses: override.keep_statuses ?? globalPolicy.keep_statuses,
      dry_run: globalPolicy.dry_run,
      cleanup_interval_secs: globalPolicy.cleanup_interval_secs,
      updated_at: override.updated_at ?? globalPolicy.updated_at,
    } as RetentionPolicy,
    has_override: true,
    override_fields: override,
  }
}

export const retentionHandlers = [
  http.get('/v1/settings/retention', async () => {
    await delay(150)
    return HttpResponse.json({ policy: globalPolicy })
  }),

  http.put('/v1/settings/retention', async ({ request }) => {
    await delay(300)
    const body = (await request.json()) as Record<string, unknown>
    globalPolicy = {
      ...globalPolicy,
      ...body,
      updated_at: now(),
    } as RetentionPolicy
    return HttpResponse.json({ policy: globalPolicy })
  }),

  http.get('/v1/settings/retention/last-cleanup', async () => {
    await delay(150)
    return HttpResponse.json({ last_cleanup: lastCleanup })
  }),

  http.get('/v1/projects/:id/retention', async ({ params }) => {
    await delay(150)
    return HttpResponse.json(effectivePolicy(params.id as string))
  }),

  http.put('/v1/projects/:id/retention', async ({ params, request }) => {
    await delay(300)
    const projectId = params.id as string
    const body = (await request.json()) as Record<string, unknown>

    const override: ProjectRetentionOverride = {
      ...(projectOverrides.get(projectId) ?? { project_id: projectId }),
      ...body,
      project_id: projectId,
      updated_at: now(),
    } as ProjectRetentionOverride

    projectOverrides.set(projectId, override)
    return HttpResponse.json(effectivePolicy(projectId))
  }),

  http.delete('/v1/projects/:id/retention', async ({ params }) => {
    await delay(300)
    const projectId = params.id as string
    projectOverrides.delete(projectId)
    return HttpResponse.json(effectivePolicy(projectId))
  }),
]
