import { demoApi } from './api'
import { HttpResponse, delay } from 'msw'
import * as z from 'zod'
import type {
  ProjectRetentionOverride,
  UpdateProjectRetentionOverrideRequest,
  UpdateRetentionPolicyRequest,
} from '@oore/client/models'
import { requireDemoInstancePermission } from '../authorization'
import { demoState } from '../state'

const retentionFields = {
  enabled: z.boolean().nullish(),
  max_age_days: z.number().nullish(),
  max_builds_per_project: z.number().nullish(),
  max_artifact_size_bytes: z.number().nullish(),
  cleanup_target: z.enum(['artifacts_only', 'full']).nullish(),
  keep_statuses: z.array(z.string()).nullish(),
  artifact_ttl_days: z.number().nullish(),
}
const retentionPolicyRequestSchema = z.toZod<UpdateRetentionPolicyRequest>()(
  z.object({
    ...retentionFields,
    enabled: z.boolean(),
    cleanup_target: z.enum(['artifacts_only', 'full']),
    keep_statuses: z.array(z.string()).optional(),
    dry_run: z.boolean().optional(),
    cleanup_interval_secs: z.number().optional(),
  }),
)
const retentionOverrideRequestSchema =
  z.toZod<UpdateProjectRetentionOverrideRequest>()(z.object(retentionFields))

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
  demoApi.getRetentionPolicy(async () => {
    await delay(150)
    return HttpResponse.json({ policy: demoState.retentionPolicy })
  }),

  demoApi.updateRetentionPolicy(async ({ request }) => {
    await delay(300)
    const forbidden = requireDemoInstancePermission(
      request,
      'instance_settings:write',
    )
    if (forbidden) return forbidden
    const body = retentionPolicyRequestSchema.parse(await request.json())
    demoState.retentionPolicy = {
      ...demoState.retentionPolicy,
      ...body,
      updated_at: now(),
    }
    return HttpResponse.json({ policy: demoState.retentionPolicy })
  }),

  demoApi.getRetentionLastCleanup(async () => {
    await delay(150)
    return HttpResponse.json({ last_cleanup: demoState.lastCleanup })
  }),

  demoApi.getProjectRetention(async ({ params }) => {
    await delay(150)
    return HttpResponse.json(effectivePolicy(String(params.project_id)))
  }),

  demoApi.updateProjectRetention(async ({ params, request }) => {
    await delay(300)
    const forbidden = requireDemoInstancePermission(
      request,
      'instance_settings:write',
    )
    if (forbidden) return forbidden
    const projectId = String(params.project_id)
    const body = retentionOverrideRequestSchema.parse(await request.json())

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

  demoApi.deleteProjectRetention(async ({ params, request }) => {
    await delay(300)
    const forbidden = requireDemoInstancePermission(
      request,
      'instance_settings:write',
    )
    if (forbidden) return forbidden
    const projectId = String(params.project_id)
    delete demoState.projectRetentionOverrides[projectId]
    return HttpResponse.json(effectivePolicy(projectId))
  }),
]
