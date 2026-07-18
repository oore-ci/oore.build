import { HttpResponse, delay, http } from 'msw'
import { PIPELINE_IDS, ago } from '../seed'
import { getDemoPersonaFromRequest, getDemoProjectRole } from '../personas'
import {
  requireDemoInstancePermission,
  requireDemoProjectPermission,
} from '../authorization'
import { demoState } from '../state'

function pipelineProjectId(pipelineId: string): string | null {
  return (
    demoState.pipelines.find((pipeline) => pipeline.id === pipelineId)
      ?.project_id ?? null
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

function invalidListQuery(message: string): Response {
  return HttpResponse.json(
    { error: message, code: 'invalid_input' },
    { status: 400 },
  )
}

function parseIntegerQuery(value: string | null): number | null {
  if (value === null) return null
  if (!/^-?\d+$/.test(value)) return Number.NaN
  return Number(value)
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
    const url = new URL(request.url)
    const sort = url.searchParams.get('sort') ?? 'created_at'
    if (sort !== 'created_at' && sort !== 'name') {
      return invalidListQuery('sort must be created_at or name')
    }
    const direction = url.searchParams.get('direction') ?? 'desc'
    if (direction !== 'asc' && direction !== 'desc') {
      return invalidListQuery('direction must be asc or desc')
    }
    const requestedLimit = parseIntegerQuery(url.searchParams.get('limit'))
    const requestedOffset = parseIntegerQuery(url.searchParams.get('offset'))
    if (Number.isNaN(requestedLimit) || Number.isNaN(requestedOffset)) {
      return invalidListQuery('limit and offset must be integers')
    }

    const search = url.searchParams.get('search')?.trim().toLowerCase()
    const pipelines = demoState.pipelines.filter(
      (p) => p.project_id === params.projectId,
    )
    const filtered = search
      ? pipelines.filter((pipeline) =>
          pipeline.name.toLowerCase().includes(search),
        )
      : pipelines
    const directionFactor = direction === 'asc' ? 1 : -1
    const sorted = [...filtered].sort((left, right) => {
      const primary =
        sort === 'name'
          ? left.name.localeCompare(right.name, undefined, {
              sensitivity: 'base',
            })
          : left.created_at - right.created_at
      return (primary || left.id.localeCompare(right.id)) * directionFactor
    })
    const offset = Math.max(0, requestedOffset ?? 0)
    const limit = Math.min(requestedLimit ?? 50, 200)
    const page =
      limit < 0 ? sorted.slice(offset) : sorted.slice(offset, offset + limit)

    return HttpResponse.json({ pipelines: page, total: filtered.length })
  }),

  http.get(
    '/v1/projects/:projectId/repository-workflows',
    async ({ params, request }) => {
      await delay(150)
      const projectId = String(params.projectId)
      const persona = getDemoPersonaFromRequest(request)
      const project = demoState.projects.find((item) => item.id === projectId)
      if (!project || !getDemoProjectRole(persona, projectId)) {
        return HttpResponse.json(
          { error: 'Project not found', code: 'not_found' },
          { status: 404 },
        )
      }

      const url = new URL(request.url)
      const requestedPath = url.searchParams.get('path')
      const discovered = demoState.repositoryWorkflows[projectId]
      const workflows = (discovered ?? []).filter(
        (workflow) => !requestedPath || workflow.path === requestedPath,
      )
      const integrationId = Object.entries(demoState.repositories).find(
        ([, repositories]) =>
          repositories?.some(
            (repository) => repository.id === project.repository_id,
          ),
      )?.[0]
      const provider = demoState.integrations.find(
        (integration) => integration.id === integrationId,
      )?.provider

      return HttpResponse.json({
        project_id: projectId,
        provider: provider === 'gitlab' ? 'gitlab' : 'github',
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
    const pipeline = demoState.pipelines.find((p) => p.id === params.pipelineId)
    if (!pipeline || !getDemoProjectRole(persona, pipeline.project_id)) {
      return HttpResponse.json(
        { error: 'Pipeline not found', code: 'not_found' },
        { status: 404 },
      )
    }
    return HttpResponse.json({
      pipeline,
      build_count: demoState.builds.filter((b) => b.pipeline_id === pipeline.id)
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
      const pipeline = {
        id: `pipe-demo-new-${crypto.randomUUID().slice(0, 8)}`,
        project_id: String(params.projectId),
        config_path: '.oore/pipeline.yaml',
        config_path_explicit: false,
        enabled: true,
        created_at: ago(0),
        updated_at: ago(0),
        ...body,
      } as (typeof demoState.pipelines)[number]
      demoState.pipelines.unshift(pipeline)
      return HttpResponse.json({ pipeline })
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
    const pipeline = demoState.pipelines.find((p) => p.id === params.pipelineId)
    if (!pipeline) {
      return HttpResponse.json(
        { error: 'Pipeline not found', code: 'not_found' },
        { status: 404 },
      )
    }
    Object.assign(pipeline, body, { updated_at: ago(0) })
    return HttpResponse.json({ pipeline })
  }),

  http.delete('/v1/pipelines/:pipelineId', async ({ params, request }) => {
    await delay(200)
    const forbidden = requirePipelinePermission(
      request,
      String(params.pipelineId),
      'pipelines:delete',
    )
    if (forbidden) return forbidden
    const pipelineId = String(params.pipelineId)
    const buildIds = new Set(
      demoState.builds
        .filter((build) => build.pipeline_id === pipelineId)
        .map((build) => build.id),
    )
    demoState.pipelines = demoState.pipelines.filter(
      (pipeline) => pipeline.id !== pipelineId,
    )
    demoState.builds = demoState.builds.filter(
      (build) => build.pipeline_id !== pipelineId,
    )
    for (const buildId of buildIds) {
      delete demoState.buildEvents[buildId]
      delete demoState.buildLogs[buildId]
      delete demoState.artifacts[buildId]
    }
    delete demoState.androidSigning[pipelineId]
    delete demoState.iosSigning[pipelineId]
    delete demoState.iosDevices[pipelineId]
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
    const id = String(params.pipelineId)
    demoState.androidSigning[id] ??= {
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
    }
    return HttpResponse.json(demoState.androidSigning[id])
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
      const existing = demoState.androidSigning[String(params.pipelineId)] ?? {}
      const signing = {
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
          ...((existing.release as Record<string, unknown> | undefined) ?? {}),
          ...(body.release ?? {}),
        },
      }
      demoState.androidSigning[String(params.pipelineId)] = signing
      return HttpResponse.json(signing)
    },
  ),

  http.get('/v1/pipelines/:pipelineId/ios-signing', async ({ params }) => {
    await delay(150)
    const id = params.pipelineId as string
    const data =
      demoState.iosSigning[id] ??
      (iosSigningByPipeline[id]
        ? structuredClone(iosSigningByPipeline[id])
        : undefined)
    if (data) {
      demoState.iosSigning[id] = data
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
      const existing =
        demoState.iosSigning[id] ?? iosSigningByPipeline[id] ?? {}
      const merged = {
        pipeline_id: id,
        ...existing,
        ...body,
        updated_at: ago(0),
      }
      demoState.iosSigning[id] = merged
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
      const id = params.pipelineId as string
      demoState.iosDevices[id] ??= structuredClone(
        iosDevicesByPipeline[id] ?? [],
      )
      const devices = demoState.iosDevices[id]
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
      demoState.iosDevices[id] ??= structuredClone(
        iosDevicesByPipeline[id] ?? [],
      )
      demoState.iosDevices[id].push(newDevice)
      return HttpResponse.json({
        device: newDevice,
        profile_sync_triggered: true,
      })
    },
  ),
]
