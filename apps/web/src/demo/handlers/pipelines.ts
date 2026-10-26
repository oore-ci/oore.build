import { demoApi } from './api'
import { HttpResponse, delay } from 'msw'
import * as z from 'zod'
import { PIPELINE_IDS, ago } from '../seed'
import { getDemoPersonaFromRequest, getDemoProjectRole } from '../personas'
import {
  requireDemoInstancePermission,
  requireDemoProjectPermission,
} from '../authorization'
import { demoState } from '../state'
import { demoExportYaml } from '../pipeline-yaml'
import { demoImportYaml } from '../pipeline-yaml-import'
import { parseDemoJsonObject } from '../request'
import {
  DEMO_SHARED_KEY_ID,
  demoSharedKeySummary,
  setDemoSharedKeyUsage,
} from './store-credentials'
import type { JsonObject } from '@/lib/types'
import type {
  AppStorePublishConfig,
  BuildPlatform,
  CreatePipelineRequest,
  ExportPipelineYamlRequest,
  ImportPipelineYamlRequest,
  Pipeline,
  PipelineExecutionConfig,
  PipelinePlanStep,
  PlanPipelineRequest,
  PlanPipelineResponse,
  RegisterIosDeviceRequest,
  UpdatePipelineAndroidSigningRequest,
  UpdatePipelineRequest,
} from '@oore/client/models'

const stringListSchema = z.array(z.string())
const executionConfigSchema = z.object({
  platforms: z.array(z.enum(['android', 'ios', 'macos'])),
  flutter_version: z.string().nullish(),
  commands: z.object({
    pre_build: stringListSchema,
    build: stringListSchema,
    post_build: stringListSchema,
  }),
  platform_build_args: z
    .object({
      android: stringListSchema,
      ios: stringListSchema,
      macos: stringListSchema,
    })
    .optional(),
  platform_commands: z
    .object({
      android: z.string().nullish(),
      ios: z.string().nullish(),
      macos: z.string().nullish(),
    })
    .optional(),
  env: z.array(z.object({ key: z.string(), value: z.string() })).optional(),
  artifact_patterns: stringListSchema,
  build_number_offset: z.number().int().optional(),
  runs_on: stringListSchema.optional(),
  isolation: z.enum(['host', 'vm']).optional(),
})
const triggerConfigSchema = z.object({
  events: stringListSchema,
  branches: stringListSchema,
  tags: stringListSchema.optional(),
  paths: stringListSchema.optional(),
  paths_ignore: stringListSchema.optional(),
  schedules: z
    .array(z.object({ cron: z.string(), branch: z.string().nullish() }))
    .optional(),
})
const approvalPolicySchema = z.object({
  required: z.boolean(),
  events: stringListSchema.optional(),
  expires_after_hours: z.number().int().min(0).nullish(),
})
const sizeBudgetSchema = z.object({
  mode: z.enum(['warn', 'fail']),
  rules: z.array(
    z.object({
      kind: z.string(),
      max_bytes: z.number().nullish(),
      max_increase_bytes: z.number().nullish(),
      max_increase_percent: z.number().nullish(),
    }),
  ),
})
const concurrencySchema = z.object({
  cancel_previous: z.boolean(),
  max_concurrent: z.number().nullish(),
})
const createPipelineSchema = z.toZod<CreatePipelineRequest>()(
  z.object({
    name: z.string(),
    config_path: z.string().nullish(),
    config_path_explicit: z.boolean().nullish(),
    execution_config: executionConfigSchema.nullish(),
    trigger_config: triggerConfigSchema.nullish(),
    concurrency: concurrencySchema.nullish(),
    timeout_minutes: z.number().int().nullish(),
    approval: approvalPolicySchema.nullish(),
    size_budget: sizeBudgetSchema.nullish(),
    workflow: z.string().nullish(),
  }),
)
const updatePipelineSchema = z.toZod<UpdatePipelineRequest>()(
  z.object({
    name: z.string().nullish(),
    config_path: z.string().nullish(),
    config_path_explicit: z.boolean().nullish(),
    execution_config: executionConfigSchema.nullish(),
    trigger_config: triggerConfigSchema.nullish(),
    concurrency: concurrencySchema.nullish(),
    timeout_minutes: z.number().int().nullish(),
    approval: approvalPolicySchema.nullish(),
    size_budget: sizeBudgetSchema.nullish(),
    workflow: z.string().nullish(),
    enabled: z.boolean().nullish(),
  }),
)
const androidSigningInputSchema = z.object({
  enabled: z.boolean().optional(),
  keystore_filename: z.string().nullish(),
  keystore_base64: z.string().nullish(),
  store_password: z.string().nullish(),
  key_alias: z.string().nullish(),
  key_password: z.string().nullish(),
})
const androidSigningRequestSchema =
  z.toZod<UpdatePipelineAndroidSigningRequest>()(
    z.object({
      debug: androidSigningInputSchema.nullish(),
      release: androidSigningInputSchema.nullish(),
    }),
  )
const jsonObjectSchema = z.record(z.string(), z.json())
const iosDeviceRequestSchema = z.toZod<RegisterIosDeviceRequest>()(
  z.object({
    name: z.string(),
    udid: z.string(),
    platform: z.string().nullish(),
  }),
)

interface IosSigningFixtures {
  [pipelineId: string]: JsonObject | undefined
}

interface IosDeviceFixtures {
  [pipelineId: string]: Array<JsonObject> | undefined
}

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

const iosSigningByPipeline: IosSigningFixtures = {
  [PIPELINE_IDS.shopIos]: {
    enabled: true,
    mode: 'api',
    export_method: 'ad_hoc',
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
    export_method: 'app_store',
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
    shared_api_key_id: DEMO_SHARED_KEY_ID,
    provisioning_profiles: [
      {
        bundle_id: 'com.example.nativepayments',
        has_profile: true,
        profile_filename: 'NativePayments_AppStore.mobileprovision',
        profile_uuid: 'B2C3D4E5-F6A7-8901-BCDE-F12345678901',
        profile_name: 'NativePayments App Store',
        expires_at: ago(-365 * 86400),
      },
      {
        bundle_id: 'com.example.nativepayments.share',
        has_profile: true,
        profile_filename: 'NativePayments_Share_AppStore.mobileprovision',
        profile_uuid: 'C3D4E5F6-A7B8-9012-CDEF-123456789012',
        profile_name: 'NativePayments Share App Store',
        expires_at: ago(-365 * 86400),
      },
    ],
    updated_at: ago(86400 * 7),
  },
}

const demoAppStoreConfigs = new Map<
  string,
  { config: AppStorePublishConfig; updated_at: number }
>([
  [
    PIPELINE_IDS.paymentsAll,
    {
      config: {
        enabled: true,
        destination: 'testflight',
        testflight_groups: ['Internal QA', 'Payments beta'],
        release_notes: [
          {
            locale: 'en-US',
            whats_new: 'Checks the new card entry flow and saved cards.',
          },
        ],
        uses_non_exempt_encryption: false,
        submit_for_review: false,
        phased_release: false,
      },
      updated_at: ago(86400 * 2),
    },
  ],
])

const appStoreConfigSchema = z.object({
  enabled: z.boolean(),
  destination: z.enum(['testflight', 'app_store']),
  testflight_groups: z.array(z.string().min(1).max(128)).max(20),
  release_notes: z
    .array(z.object({ locale: z.string().min(1), whats_new: z.string() }))
    .max(40),
  uses_non_exempt_encryption: z.boolean(),
  submit_for_review: z.boolean(),
  phased_release: z.boolean(),
})

const iosSigningSummarySchema = z.object({
  enabled: z.boolean().optional(),
  export_method: z.string().optional(),
  has_api_key: z.boolean().optional(),
  shared_api_key_id: z.string().optional(),
})

function storedIosSigning(pipelineId: string) {
  return iosSigningSummarySchema.parse(
    demoState.iosSigning[pipelineId] ?? iosSigningByPipeline[pipelineId] ?? {},
  )
}

setDemoSharedKeyUsage(
  (keyId) =>
    demoState.pipelines.filter(
      (pipeline) => storedIosSigning(pipeline.id).shared_api_key_id === keyId,
    ).length,
)

/** The daemon returns the selected shared key, not its id. */
function iosSigningResponse(data: JsonObject) {
  const { shared_api_key_id: sharedKeyId, ...rest } = data
  const id = z.string().safeParse(sharedKeyId).data
  const sharedKey = id ? demoSharedKeySummary(id) : undefined
  return sharedKey ? { ...rest, shared_api_key: sharedKey } : rest
}

function appStoreResponse(pipelineId: string) {
  const signing = storedIosSigning(pipelineId)
  const saved = demoAppStoreConfigs.get(pipelineId)
  return {
    pipeline_id: pipelineId,
    config: saved?.config ?? {
      enabled: false,
      destination: 'testflight' as const,
      testflight_groups: [],
      release_notes: [],
      uses_non_exempt_encryption: false,
      submit_for_review: false,
      phased_release: false,
    },
    updated_at: saved?.updated_at,
    has_api_key: !!signing.has_api_key || !!signing.shared_api_key_id,
    app_store_export:
      (signing.enabled ?? false) && signing.export_method === 'app_store',
  }
}

function invalidPublishConfig(message: string): Response {
  return HttpResponse.json(
    { error: message, code: 'invalid_publish_config' },
    { status: 400 },
  )
}

const iosDevicesByPipeline: IosDeviceFixtures = {
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
  demoApi.listPipelines(async ({ params, request }) => {
    await delay(150)
    const persona = getDemoPersonaFromRequest(request)
    if (!getDemoProjectRole(persona, String(params.project_id))) {
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
      (p) => p.project_id === params.project_id,
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

  demoApi.discoverRepositoryWorkflows(async ({ params, request }) => {
    await delay(150)
    const projectId = String(params.project_id)
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
  }),

  demoApi.getPipeline(async ({ params, request }) => {
    await delay(150)
    const persona = getDemoPersonaFromRequest(request)
    const pipeline = demoState.pipelines.find(
      (p) => p.id === params.pipeline_id,
    )
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

  demoApi.createPipeline(async ({ params, request }) => {
    await delay(300)
    const forbidden = requireDemoProjectPermission(
      request,
      String(params.project_id),
      'pipelines:write',
    )
    if (forbidden) return forbidden
    const body = createPipelineSchema.parse(await request.json())
    const pipeline: Pipeline = {
      id: `pipe-demo-new-${crypto.randomUUID().slice(0, 8)}`,
      project_id: String(params.project_id),
      name: body.name,
      config_path: body.config_path ?? '.oore/pipeline.yaml',
      config_path_explicit: body.config_path_explicit ?? false,
      execution_config: body.execution_config ?? {
        platforms: ['android'],
        commands: { pre_build: [], build: [], post_build: [] },
        artifact_patterns: [],
      },
      trigger_config: body.trigger_config ?? { events: [], branches: [] },
      concurrency: body.concurrency ?? {
        cancel_previous: false,
        max_concurrent: null,
      },
      timeout_minutes: body.timeout_minutes ?? 60,
      approval: body.approval ?? { required: false, events: [] },
      size_budget: body.size_budget ?? { mode: 'warn', rules: [] },
      workflow: body.workflow ?? null,
      enabled: true,
      created_at: ago(0),
      updated_at: ago(0),
    }
    demoState.pipelines.unshift(pipeline)
    return HttpResponse.json({ pipeline }, { status: 201 })
  }),

  demoApi.updatePipeline(async ({ params, request }) => {
    await delay(200)
    const forbidden = requirePipelinePermission(
      request,
      String(params.pipeline_id),
      'pipelines:write',
    )
    if (forbidden) return forbidden
    const body = updatePipelineSchema.parse(await request.json())
    const pipeline = demoState.pipelines.find(
      (p) => p.id === params.pipeline_id,
    )
    if (!pipeline) {
      return HttpResponse.json(
        { error: 'Pipeline not found', code: 'not_found' },
        { status: 404 },
      )
    }
    Object.assign(pipeline, body, { updated_at: ago(0) })
    return HttpResponse.json({ pipeline })
  }),

  demoApi.deletePipeline(async ({ params, request }) => {
    await delay(200)
    const forbidden = requirePipelinePermission(
      request,
      String(params.pipeline_id),
      'pipelines:delete',
    )
    if (forbidden) return forbidden
    const pipelineId = String(params.pipeline_id)
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
    return HttpResponse.json({ ok: true })
  }),

  demoApi.validatePipeline(async ({ request }) => {
    await delay(200)
    const forbidden = requireDemoInstancePermission(request, 'pipelines:write')
    if (forbidden) return forbidden
    return HttpResponse.json({ valid: true })
  }),

  demoApi.planPipeline(async ({ request }) => {
    await delay(150)
    const forbidden = requireDemoInstancePermission(request, 'pipelines:read')
    if (forbidden) return forbidden
    // SAFETY: demo requests come from the app's own typed client.
    const body = (await request.json()) as PlanPipelineRequest
    if (body.execution_config) {
      return HttpResponse.json(
        demoFormPlan(
          body.execution_config,
          body.platforms ?? [],
          body.build_play_bundle ?? false,
        ),
      )
    }
    return HttpResponse.json(
      demoFilePlan(
        body.repository_yaml ?? '',
        body.workflow ?? null,
        body.platforms ?? [],
      ),
    )
  }),

  demoApi.exportPipelineYaml(async ({ request }) => {
    await delay(150)
    const forbidden = requireDemoInstancePermission(request, 'pipelines:read')
    if (forbidden) return forbidden
    // SAFETY: demo requests come from the app's own typed client.
    const body = (await request.json()) as ExportPipelineYamlRequest
    return HttpResponse.json(demoExportYaml(body))
  }),

  demoApi.importPipelineYaml(async ({ request }) => {
    await delay(150)
    const forbidden = requireDemoInstancePermission(request, 'pipelines:read')
    if (forbidden) return forbidden
    // SAFETY: demo requests come from the app's own typed client.
    const body = (await request.json()) as ImportPipelineYamlRequest
    return HttpResponse.json(demoImportYaml(body))
  }),

  demoApi.getPipelineAndroidSigning(async ({ params }) => {
    await delay(150)
    const id = String(params.pipeline_id)
    demoState.androidSigning[id] ??= {
      pipeline_id: params.pipeline_id,
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

  demoApi.updatePipelineAndroidSigning(async ({ params, request }) => {
    await delay(300)
    const forbidden = requirePipelinePermission(
      request,
      String(params.pipeline_id),
      'pipelines:write',
    )
    if (forbidden) return forbidden
    const body = androidSigningRequestSchema.parse(await request.json())
    const existing = demoState.androidSigning[String(params.pipeline_id)] ?? {}
    const existingRelease = jsonObjectSchema.safeParse(existing.release)
    const release = {
      build_type: 'release',
      enabled: true,
      has_keystore: true,
      keystore_filename: 'release.keystore',
      has_store_password: true,
      has_key_password: true,
      updated_at: ago(0),
    }
    if (existingRelease.success) Object.assign(release, existingRelease.data)
    if (body.release) Object.assign(release, body.release)
    const signing = {
      pipeline_id: params.pipeline_id,
      debug: {
        build_type: 'debug',
        enabled: false,
        has_keystore: false,
        has_store_password: false,
        has_key_password: false,
        ...body.debug,
      },
      release,
    }
    demoState.androidSigning[String(params.pipeline_id)] = signing
    return HttpResponse.json(signing)
  }),

  demoApi.getPipelineIosSigning(async ({ params }) => {
    await delay(150)
    const id = String(params.pipeline_id)
    const data =
      demoState.iosSigning[id] ??
      (iosSigningByPipeline[id]
        ? structuredClone(iosSigningByPipeline[id])
        : undefined)
    if (data) {
      demoState.iosSigning[id] = data
      return HttpResponse.json(iosSigningResponse({ pipeline_id: id, ...data }))
    }
    return HttpResponse.json({
      pipeline_id: id,
      enabled: false,
      mode: 'manual',
      export_method: 'ad_hoc',
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

  demoApi.updatePipelineIosSigning(async ({ params, request }) => {
    await delay(300)
    const id = String(params.pipeline_id)
    const forbidden = requirePipelinePermission(request, id, 'pipelines:write')
    if (forbidden) return forbidden
    const body = await parseDemoJsonObject(request)
    const existing = demoState.iosSigning[id] ?? iosSigningByPipeline[id] ?? {}
    // Switching the export method discards profiles, like the daemon does.
    const methodChanged =
      body.export_method != null &&
      body.export_method !== (existing.export_method ?? 'ad_hoc')
    const { shared_api_key_id: rawSharedKeyId, ...changes } = body
    const sharedKeyId = z.string().optional().parse(rawSharedKeyId)
    const merged: JsonObject = {
      pipeline_id: id,
      ...existing,
      ...changes,
      updated_at: ago(0),
    }
    // Omitted keeps the shared key, an empty string switches to the own key.
    if (sharedKeyId === '') delete merged.shared_api_key_id
    else if (sharedKeyId) merged.shared_api_key_id = sharedKeyId
    if (methodChanged) merged.provisioning_profiles = []
    demoState.iosSigning[id] = merged
    return HttpResponse.json(iosSigningResponse(merged))
  }),

  demoApi.getPipelineAppStorePublish(async ({ params }) => {
    await delay(120)
    return HttpResponse.json(appStoreResponse(String(params.pipeline_id)))
  }),

  demoApi.updatePipelineAppStorePublish(async ({ params, request }) => {
    await delay(200)
    const id = String(params.pipeline_id)
    const forbidden = requirePipelinePermission(request, id, 'pipelines:write')
    if (forbidden) return forbidden
    const parsed = appStoreConfigSchema.safeParse(await request.json())
    if (!parsed.success) {
      return invalidPublishConfig(parsed.error.message)
    }
    const current = appStoreResponse(id)
    if (parsed.data.enabled && !current.app_store_export) {
      return invalidPublishConfig(
        'App Store publishing needs iOS signing with the App Store export method.',
      )
    }
    if (parsed.data.enabled && !current.has_api_key) {
      return invalidPublishConfig(
        'App Store publishing needs an App Store Connect API key in iOS signing.',
      )
    }
    demoAppStoreConfigs.set(id, {
      config: parsed.data,
      updated_at: ago(0),
    })
    return HttpResponse.json(appStoreResponse(id))
  }),

  demoApi.syncPipelineIosSigning(async ({ params, request }) => {
    await delay(500)
    const forbidden = requirePipelinePermission(
      request,
      String(params.pipeline_id),
      'pipelines:write',
    )
    if (forbidden) return forbidden
    return HttpResponse.json({
      pipeline_id: params.pipeline_id,
      updated_profiles: 1,
      warnings: [],
    })
  }),

  demoApi.listPipelineIosDevices(async ({ params }) => {
    await delay(150)
    const id = String(params.pipeline_id)
    demoState.iosDevices[id] ??= structuredClone(iosDevicesByPipeline[id] ?? [])
    const devices = demoState.iosDevices[id]
    return HttpResponse.json({ devices })
  }),

  demoApi.registerPipelineIosDevice(async ({ params, request }) => {
    await delay(400)
    const forbidden = requirePipelinePermission(
      request,
      String(params.pipeline_id),
      'pipelines:write',
    )
    if (forbidden) return forbidden
    const body = iosDeviceRequestSchema.parse(await request.json())
    const newDevice = {
      id: `iosdev-new-${Date.now()}`,
      name: body.name,
      udid: body.udid,
      platform: body.platform,
      status: 'registered',
      added_at: ago(0),
    }
    const id = String(params.pipeline_id)
    demoState.iosDevices[id] ??= structuredClone(iosDevicesByPipeline[id] ?? [])
    demoState.iosDevices[id].push(newDevice)
    return HttpResponse.json({
      device: newDevice,
      profile_sync_triggered: true,
    })
  }),
]

// ── Plan previews ───────────────────────────────────────────────

const DEMO_PLATFORM_BUILDS = {
  android: 'flutter build apk --release',
  ios: 'flutter build ios --release --no-codesign',
  macos: 'flutter build macos --release',
} satisfies Record<BuildPlatform, string>

/** Mirrors the runner's lowering of form settings (plan_execution_config). */
function demoFormPlan(
  config: PipelineExecutionConfig,
  selected: Array<BuildPlatform>,
  playBundle: boolean,
): PlanPipelineResponse {
  const available = config.platforms
  if (available.length === 0) {
    return {
      valid: false,
      errors: ['execution_config.platforms must include at least one platform'],
      notices: [],
      workflows: [],
      available_platforms: [],
    }
  }
  const platforms =
    selected.length > 0
      ? available.filter((platform) => selected.includes(platform))
      : available
  const steps: Array<PipelinePlanStep> = []
  // Sources match the daemon: `default` for commands Oore adds.
  const push = (
    phase: PipelinePlanStep['phase'],
    run: string,
    source: string,
    platform?: BuildPlatform,
  ) => {
    const index = steps.filter((step) => step.phase === phase).length
    steps.push({ name: `${phase}-${index + 1}`, phase, run, source, platform })
  }
  const pushUser = (phase: PipelinePlanStep['phase'], runs: Array<string>) =>
    runs.forEach((run, index) =>
      push(phase, run, `commands.${phase}[${index}]`),
    )
  push('pre_build', 'flutter pub get', 'default')
  pushUser('pre_build', config.commands.pre_build)
  for (const platform of platforms) {
    const args = config.platform_build_args?.[platform] ?? []
    const override = config.platform_commands?.[platform]
    push(
      'build',
      override || [DEMO_PLATFORM_BUILDS[platform], ...args].join(' '),
      override ? `platform_commands.${platform}` : 'default',
      platform,
    )
    if (platform === 'android' && playBundle && !override) {
      push(
        'build',
        // `--split-per-abi` is APK-only; the runner drops it too.
        [
          'flutter build appbundle --release',
          ...args.filter((arg) => arg !== '--split-per-abi'),
        ].join(' '),
        'default',
        platform,
      )
    }
  }
  pushUser('build', config.commands.build)
  pushUser('post_build', config.commands.post_build)
  return {
    valid: true,
    errors: [],
    notices: [],
    workflows: [],
    available_platforms: available,
    plan: {
      platforms,
      flutter_version: config.flutter_version ?? undefined,
      steps,
      env_keys: (config.env ?? []).map((entry) => entry.key),
      artifact_patterns: config.artifact_patterns,
      build_number_offset: config.build_number_offset ?? 0,
      runs_on: config.runs_on ?? [],
      isolation: config.isolation ?? 'host',
    },
  }
}

const DEMO_WORKFLOWS = {
  nightly: { platforms: ['android', 'ios'], flavor: 'staging' },
  release: { platforms: ['android', 'ios'], flavor: 'production' },
} satisfies Record<string, { platforms: Array<BuildPlatform>; flavor: string }>

/** A canned version 2 plan; the demo has no YAML parser. */
function demoFilePlan(
  yaml: string,
  requested: string | null,
  selected: Array<BuildPlatform>,
): PlanPipelineResponse {
  const workflows = Object.keys(DEMO_WORKFLOWS)
  if (!/^version:\s*[12]\s*$/m.test(yaml)) {
    return {
      valid: false,
      errors: ['missing field `version`; start the file with `version: 2`'],
      notices: [],
      workflows: [],
      available_platforms: [],
    }
  }
  const name = requested ?? workflows[0]
  if (!(name in DEMO_WORKFLOWS)) {
    return {
      valid: false,
      errors: [`workflow '${name}' is not defined`],
      notices: [],
      version: 2,
      workflows,
      available_platforms: [],
    }
  }
  // SAFETY: `name in DEMO_WORKFLOWS` was checked above.
  const workflow = DEMO_WORKFLOWS[name as keyof typeof DEMO_WORKFLOWS]
  const available: Array<BuildPlatform> = workflow.platforms
  const platforms =
    selected.length > 0
      ? available.filter((platform) => selected.includes(platform))
      : available
  const { flavor } = workflow
  const steps: Array<PipelinePlanStep> = [
    {
      name: 'Get packages',
      phase: 'pre_build',
      run: 'flutter pub get',
      source: 'steps[0]',
    },
    {
      name: 'Test',
      phase: 'pre_build',
      run: 'flutter test --machine > build/test-results.json',
      source: 'steps[1]',
      test_report: 'build/test-results.json',
    },
    ...platforms.map((platform): PipelinePlanStep => ({
      name: `Build ${platform === 'ios' ? 'iOS' : 'Android'} (${flavor})`,
      phase: 'build',
      run:
        platform === 'ios'
          ? `flutter build ipa --release --flavor ${flavor} --export-options-plist ios/ExportOptions.plist`
          : `flutter build appbundle --release --flavor ${flavor}`,
      source: `targets.${platform}.build`,
      platform,
      flavor,
    })),
    {
      name: 'Upload symbols',
      phase: 'post_build',
      run: 'sentry-cli debug-files upload --auth-token ${{ secrets.SENTRY_TOKEN }} build/',
      source: 'steps[3]',
    },
  ]
  return {
    valid: true,
    errors: [],
    notices: [
      `this file defines workflows [${workflows.join(', ')}], but pipelines cannot select a workflow yet, so every build from this file fails with \`choose one for the pipeline\`; keep one workflow until workflow selection lands`,
    ],
    version: 2,
    workflows,
    available_platforms: available,
    plan: {
      workflow: name,
      platforms,
      flutter_version: '3.24.5',
      steps,
      env_keys: ['BUILD_FLAVOR', 'SENTRY_DSN'],
      artifact_patterns: platforms.map((platform) =>
        platform === 'ios'
          ? 'build/ios/ipa/*.ipa'
          : `build/app/outputs/bundle/${flavor}Release/*.aab`,
      ),
      build_number_offset: 0,
      runs_on: [],
      isolation: 'host',
    },
  }
}
