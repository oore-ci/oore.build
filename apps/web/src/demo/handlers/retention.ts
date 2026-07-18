import { HttpResponse, delay, http } from 'msw'
import type { ProjectRetentionOverride } from '@/lib/types'
import { requireDemoInstancePermission } from '../authorization'
import { demoState } from '../state'

function now(): number {
  return Math.floor(Date.now() / 1000)
}

function effectivePolicy(projectId: string) {
  const override = demoState.projectRetentionOverrides[projectId]
  if (!override) {
    return { effective: { ...demoState.retentionPolicy }, has_override: false }
  }
  return {
    effective: {
      enabled: override.enabled ?? demoState.retentionPolicy.enabled,
      max_age_days:
        override.max_age_days ?? demoState.retentionPolicy.max_age_days,
      max_builds_per_project:
        override.max_builds_per_project ??
        demoState.retentionPolicy.max_builds_per_project,
      max_artifact_size_bytes:
        override.max_artifact_size_bytes ??
        demoState.retentionPolicy.max_artifact_size_bytes,
      cleanup_target:
        override.cleanup_target ?? demoState.retentionPolicy.cleanup_target,
      keep_statuses:
        override.keep_statuses ?? demoState.retentionPolicy.keep_statuses,
      dry_run: demoState.retentionPolicy.dry_run,
      cleanup_interval_secs: demoState.retentionPolicy.cleanup_interval_secs,
      updated_at: override.updated_at ?? demoState.retentionPolicy.updated_at,
    },
    has_override: true,
    override_fields: override,
  }
}

export const retentionHandlers = [
  http.get('/v1/settings/retention', async () => {
    await delay(150)
    return HttpResponse.json({ policy: demoState.retentionPolicy })
  }),

  http.put('/v1/settings/retention', async ({ request }) => {
    await delay(300)
    const forbidden = requireDemoInstancePermission(
      request,
      'instance_settings:write',
    )
    if (forbidden) return forbidden
    const body = (await request.json()) as Record<string, unknown>
    demoState.retentionPolicy = {
      ...demoState.retentionPolicy,
      ...body,
      updated_at: now(),
    }
    return HttpResponse.json({ policy: demoState.retentionPolicy })
  }),

  http.get('/v1/settings/retention/last-cleanup', async () => {
    await delay(150)
    return HttpResponse.json({ last_cleanup: demoState.lastCleanup })
  }),

  http.get('/v1/projects/:id/retention', async ({ params }) => {
    await delay(150)
    return HttpResponse.json(effectivePolicy(params.id as string))
  }),

  http.put('/v1/projects/:id/retention', async ({ params, request }) => {
    await delay(300)
    const forbidden = requireDemoInstancePermission(
      request,
      'instance_settings:write',
    )
    if (forbidden) return forbidden
    const projectId = params.id as string
    const body = (await request.json()) as Record<string, unknown>

    const override: ProjectRetentionOverride = {
      ...(demoState.projectRetentionOverrides[projectId] ?? {
        project_id: projectId,
      }),
      ...body,
      project_id: projectId,
      updated_at: now(),
    }

    demoState.projectRetentionOverrides[projectId] = override
    return HttpResponse.json(effectivePolicy(projectId))
  }),

  http.delete('/v1/projects/:id/retention', async ({ params, request }) => {
    await delay(300)
    const forbidden = requireDemoInstancePermission(
      request,
      'instance_settings:write',
    )
    if (forbidden) return forbidden
    const projectId = params.id as string
    delete demoState.projectRetentionOverrides[projectId]
    return HttpResponse.json(effectivePolicy(projectId))
  }),
]
