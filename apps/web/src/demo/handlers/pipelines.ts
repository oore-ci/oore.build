import { HttpResponse, delay, http } from 'msw'
import { demoPipelines } from '../data/pipelines'
import { demoBuilds } from '../data/builds'
import { demoProjects } from '../data/projects'
import { INTEGRATION_IDS, PIPELINE_IDS, ago } from '../seed'
import { getDemoPersonaFromRequest, getDemoProjectRole } from '../personas'
import {
  requireDemoInstancePermission,
  requireDemoProjectPermission,
} from '../authorization'

function pipelineProjectId(pipelineId: string): string | null {
  return (
    demoPipelines.find((pipeline) => pipeline.id === pipelineId)?.project_id ??
    null
  )
}

function requirePipelinePermission(
  request: Request,
  pipelineId: string,
  permission: string,
): Response | null {
  const projectId = pipelineProjectId(pipelineId)
  if (!projectId) {
    return HttpResponse.json(
      { error: 'Pipeline not found', code: 'not_found' },
      { status: 404 },
    )
  }
  return requireDemoProjectPermission(request, projectId, permission)
}

const iosSigningByPipeline: Partial<Record<string, Record<string, unknown>>> = {
  [PIPELINE_IDS.shopIos]: {
    enabled: true,
    mode: 'api',
    team_id: 'A1B2C3D4E5',
    bundle_ids: ['com.example.fluttershop'],
    has_p12: false,
    has_p12_password: false,
    has_api_key: true,
    api_key_id: 'K9X7Y2Z1AB',
    api_issuer_id: '57246542-96fe-1a63-e053-0824d011072a',
    provisioning_profiles: [
      {
        bundle_id: 'com.example.fluttershop',
        has_profile: true,
        profile_filename: 'FlutterShop_AdHoc.mobileprovision',
        profile_uuid: 'A1B2C3D4-E5F6-7890-ABCD-EF1234567890',
        profile_name: 'FlutterShop Ad Hoc',
        expires_at: ago(-365 * 86400),
      },
    ],
    updated_at: ago(86400 * 3),
  },
  [PIPELINE_IDS.paymentsAll]: {
    enabled: true,
    mode: 'hybrid',
    team_id: 'F6G7H8I9J0',
    bundle_ids: [
      'com.example.nativepayments',
      'com.example.nativepayments.share',
    ],
    has_p12: true,
    p12_filename: 'distribution.p12',
    has_p12_password: true,
    has_api_key: true,
    api_key_id: 'M3N4P5Q6RS',
    api_issuer_id: '69d2de96-0000-47e3-e053-0824d011072a',
    provisioning_profiles: [
      {
        bundle_id: 'com.example.nativepayments',
        has_profile: true,
        profile_filename: 'NativePayments_AdHoc.mobileprovision',
        profile_uuid: 'B2C3D4E5-F6A7-8901-BCDE-F12345678901',
        profile_name: 'NativePayments Ad Hoc',
        expires_at: ago(-365 * 86400),
      },
      {
        bundle_id: 'com.example.nativepayments.share',
        has_profile: true,
        profile_filename: 'NativePayments_Share_AdHoc.mobileprovision',
        profile_uuid: 'C3D4E5F6-A7B8-9012-CDEF-123456789012',
        profile_name: 'NativePayments Share Ad Hoc',
        expires_at: ago(-365 * 86400),
      },
    ],
    updated_at: ago(86400 * 7),
  },
}

const iosDevicesByPipeline: Partial<
  Record<string, Array<Record<string, unknown>>>
> = {
  [PIPELINE_IDS.shopIos]: [
    {
      id: 'iosdev-001',
      name: "Alex's iPhone 15 Pro",
      udid: '00008110-000A1234ABCD5678',
      platform: 'IOS',
      status: 'registered',
      added_at: ago(86400 * 14),
    },
    {
      id: 'iosdev-002',
      name: 'QA iPad Air',
      udid: '00008103-000B5678EFGH9012',
      platform: 'IOS',
      status: 'registered',
      added_at: ago(86400 * 7),
    },
  ],
  [PIPELINE_IDS.paymentsAll]: [
    {
      id: 'iosdev-003',
      name: "Alex's iPhone 15 Pro",
      udid: '00008110-000A1234ABCD5678',
      platform: 'IOS',
      status: 'registered',
      added_at: ago(86400 * 10),
    },
  ],
}

export const pipelineHandlers = [
  http.get('/v1/projects/:projectId/pipelines', async ({ params, request }) => {
    await delay(150)
    const persona = getDemoPersonaFromRequest(request)
    if (!getDemoProjectRole(persona, String(params.projectId))) {
      return HttpResponse.json(
        { error: 'Project not found', code: 'not_found' },
        { status: 404 },
      )
    }
    const pipelines = demoPipelines.filter(
      (p) => p.project_id === params.projectId,
    )
    return HttpResponse.json({ pipelines, total: pipelines.length })
  }),

  http.get(
    '/v1/projects/:projectId/repository-workflows',
    async ({ params, request }) => {
      await delay(150)
      const projectId = String(params.projectId)
      const persona = getDemoPersonaFromRequest(request)
      const project = demoProjects.find((item) => item.id === projectId)
      if (!project || !getDemoProjectRole(persona, projectId)) {
        return HttpResponse.json(
          { error: 'Project not found', code: 'not_found' },
          { status: 404 },
        )
      }

      const url = new URL(request.url)
      const requestedPath = url.searchParams.get('path')
      const workflows = demoPipelines
        .filter(
          (pipeline) =>
            pipeline.project_id === projectId &&
            (!requestedPath || pipeline.config_path === requestedPath),
        )
        .map((pipeline) => ({
          path: pipeline.config_path,
          valid: true,
          errors: [],
          execution: {
            platforms: pipeline.execution_config.platforms,
            flutter_version: pipeline.execution_config.flutter_version,
            commands: pipeline.execution_config.commands,
            platform_build_args:
              pipeline.execution_config.platform_build_args ?? {},
            platform_commands:
              pipeline.execution_config.platform_commands ?? {},
            env_keys:
              pipeline.execution_config.env?.map((variable) => variable.key) ??
              [],
            artifact_patterns: pipeline.execution_config.artifact_patterns,
          },
        }))

      return HttpResponse.json({
        project_id: projectId,
        provider: project.repository_id?.startsWith(INTEGRATION_IDS.gitlab)
          ? 'gitlab'
          : 'github',
        reference:
          url.searchParams.get('ref') ?? project.default_branch ?? 'main',
        workflows,
        truncated: false,
      })
    },
  ),

  http.get('/v1/pipelines/:pipelineId', async ({ params, request }) => {
    await delay(150)
    const persona = getDemoPersonaFromRequest(request)
    const pipeline = demoPipelines.find((p) => p.id === params.pipelineId)
    if (!pipeline || !getDemoProjectRole(persona, pipeline.project_id)) {
      return HttpResponse.json(
        { error: 'Pipeline not found', code: 'not_found' },
        { status: 404 },
      )
    }
    return HttpResponse.json({
      pipeline,
      build_count: demoBuilds.filter((b) => b.pipeline_id === pipeline.id)
        .length,
    })
  }),

  http.post(
    '/v1/projects/:projectId/pipelines',
    async ({ params, request }) => {
      await delay(300)
      const forbidden = requireDemoProjectPermission(
        request,
        String(params.projectId),
        'pipelines:write',
      )
      if (forbidden) return forbidden
      const body = (await request.json()) as Record<string, unknown>
      return HttpResponse.json({
        pipeline: {
          id: `pipe-demo-new-${Date.now()}`,
          project_id: params.projectId,
          config_path: '.oore/pipeline.yaml',
          config_path_explicit: false,
          enabled: true,
          created_at: ago(0),
          updated_at: ago(0),
          ...body,
        },
      })
    },
  ),

  http.patch('/v1/pipelines/:pipelineId', async ({ params, request }) => {
    await delay(200)
    const forbidden = requirePipelinePermission(
      request,
      String(params.pipelineId),
      'pipelines:write',
    )
    if (forbidden) return forbidden
    const body = (await request.json()) as Record<string, unknown>
    const pipeline = demoPipelines.find((p) => p.id === params.pipelineId)
    return HttpResponse.json({
      pipeline: pipeline
        ? { ...pipeline, ...body, updated_at: ago(0) }
        : { id: params.pipelineId, ...body, updated_at: ago(0) },
    })
  }),

  http.delete('/v1/pipelines/:pipelineId', async ({ params, request }) => {
    await delay(200)
    const forbidden = requirePipelinePermission(
      request,
      String(params.pipelineId),
      'pipelines:delete',
    )
    if (forbidden) return forbidden
    return new HttpResponse(null, { status: 204 })
  }),

  http.post('/v1/pipelines/validate', async ({ request }) => {
    await delay(200)
    const forbidden = requireDemoInstancePermission(request, 'pipelines:write')
    if (forbidden) return forbidden
    return HttpResponse.json({ valid: true })
  }),

  http.get('/v1/pipelines/:pipelineId/android-signing', async ({ params }) => {
    await delay(150)
    return HttpResponse.json({
      pipeline_id: params.pipelineId,
      debug: {
        build_type: 'debug',
        enabled: false,
        has_keystore: false,
        has_store_password: false,
        has_key_password: false,
      },
      release: {
        build_type: 'release',
        enabled: true,
        has_keystore: true,
        keystore_filename: 'release.keystore',
        keystore_checksum: 'sha256:abc123...',
        key_alias: 'upload',
        has_store_password: true,
        has_key_password: true,
        updated_at: ago(86400 * 10),
      },
    })
  }),

  http.put(
    '/v1/pipelines/:pipelineId/android-signing',
    async ({ params, request }) => {
      await delay(300)
      const forbidden = requirePipelinePermission(
        request,
        String(params.pipelineId),
        'pipelines:write',
      )
      if (forbidden) return forbidden
      const body = (await request.json()) as {
        debug?: Record<string, unknown>
        release?: Record<string, unknown>
      }
      return HttpResponse.json({
        pipeline_id: params.pipelineId,
        debug: {
          build_type: 'debug',
          enabled: false,
          has_keystore: false,
          has_store_password: false,
          has_key_password: false,
          ...(body.debug ?? {}),
        },
        release: {
          build_type: 'release',
          enabled: true,
          has_keystore: true,
          keystore_filename: 'release.keystore',
          has_store_password: true,
          has_key_password: true,
          updated_at: ago(0),
          ...(body.release ?? {}),
        },
      })
    },
  ),

  http.get('/v1/pipelines/:pipelineId/ios-signing', async ({ params }) => {
    await delay(150)
    const id = params.pipelineId as string
    const data = iosSigningByPipeline[id]
    if (data) {
      return HttpResponse.json({ pipeline_id: id, ...data })
    }
    return HttpResponse.json({
      pipeline_id: id,
      enabled: false,
      mode: 'manual',
      team_id: null,
      bundle_ids: [],
      has_p12: false,
      has_p12_password: false,
      has_api_key: false,
      api_key_id: null,
      api_issuer_id: null,
      provisioning_profiles: [],
    })
  }),

  http.put(
    '/v1/pipelines/:pipelineId/ios-signing',
    async ({ params, request }) => {
      await delay(300)
      const id = params.pipelineId as string
      const forbidden = requirePipelinePermission(
        request,
        id,
        'pipelines:write',
      )
      if (forbidden) return forbidden
      const body = (await request.json()) as Record<string, unknown>
      const existing = iosSigningByPipeline[id] ?? {}
      const merged = {
        pipeline_id: id,
        ...existing,
        ...body,
        updated_at: ago(0),
      }
      iosSigningByPipeline[id] = merged
      return HttpResponse.json(merged)
    },
  ),

  http.post(
    '/v1/pipelines/:pipelineId/ios-signing/sync',
    async ({ params, request }) => {
      await delay(500)
      const forbidden = requirePipelinePermission(
        request,
        String(params.pipelineId),
        'pipelines:write',
      )
      if (forbidden) return forbidden
      return HttpResponse.json({
        pipeline_id: params.pipelineId,
        updated_profiles: 1,
        warnings: [],
      })
    },
  ),

  http.get(
    '/v1/pipelines/:pipelineId/ios-signing/devices',
    async ({ params }) => {
      await delay(150)
      const devices = iosDevicesByPipeline[params.pipelineId as string] ?? []
      return HttpResponse.json({ devices })
    },
  ),

  http.post(
    '/v1/pipelines/:pipelineId/ios-signing/devices/register',
    async ({ params, request }) => {
      await delay(400)
      const forbidden = requirePipelinePermission(
        request,
        String(params.pipelineId),
        'pipelines:write',
      )
      if (forbidden) return forbidden
      const body = (await request.json()) as {
        name: string
        udid: string
        platform: string
      }
      const newDevice = {
        id: `iosdev-new-${Date.now()}`,
        name: body.name,
        udid: body.udid,
        platform: body.platform,
        status: 'registered',
        added_at: ago(0),
      }
      const id = params.pipelineId as string
      if (!iosDevicesByPipeline[id]) iosDevicesByPipeline[id] = []
      iosDevicesByPipeline[id].push(newDevice)
      return HttpResponse.json({
        device: newDevice,
        profile_sync_triggered: true,
      })
    },
  ),
]
