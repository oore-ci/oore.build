import { HttpResponse, delay, http } from 'msw'
import * as z from 'zod'

import type {
  GooglePlayCredentialsTest,
  GooglePlayPublishConfig,
  GooglePlayPublishResponse,
} from '@/hooks/use-pipeline-publish'
import type { SecretSummary } from '@/hooks/use-secrets'
import {
  requireDemoInstancePermission,
  requireDemoProjectPermission,
} from '../authorization'
import { getDemoPersonaFromRequest, getDemoProjectRole } from '../personas'
import { ago, PIPELINE_IDS, PROJECT_IDS } from '../seed'
import { demoState } from '../state'

// One set of resolvers serves all three secret scopes, so these match the
// daemon routes directly instead of using one generated handler per scope.

const demoSecrets: Array<SecretSummary> = [
  {
    name: 'SENTRY_AUTH_TOKEN',
    scope: 'instance',
    created_at: ago(30 * 86_400),
    updated_at: ago(12 * 86_400),
  },
  {
    name: 'PLAY_SERVICE_ACCOUNT',
    scope: 'project',
    project_id: PROJECT_IDS.flutterShop,
    created_at: ago(9 * 86_400),
    updated_at: ago(9 * 86_400),
  },
  {
    name: 'SENTRY_AUTH_TOKEN',
    scope: 'pipeline',
    project_id: PROJECT_IDS.flutterShop,
    pipeline_id: PIPELINE_IDS.shopAndroid,
    created_at: ago(2 * 86_400),
    updated_at: ago(3_600),
  },
]

const demoPlayConfigs = new Map<
  string,
  { config: GooglePlayPublishConfig; updated_at: number }
>([
  [
    PIPELINE_IDS.shopAndroid,
    {
      config: {
        enabled: true,
        service_account_secret: 'PLAY_SERVICE_ACCOUNT',
        track: 'production',
        rollout_percent: 5,
        draft: false,
        release_notes: 'changelog',
        release_notes_locale: 'en-US',
        upload_mapping: true,
      },
      updated_at: ago(5 * 86_400),
    },
  ],
])

// The latest credentials check per pipeline, as the daemon keeps it.
const demoPlayChecks = new Map<string, GooglePlayCredentialsTest>([
  [
    PIPELINE_IDS.shopAndroid,
    {
      ok: true,
      service_account_secret: 'PLAY_SERVICE_ACCOUNT',
      message:
        'oore-publisher@oore-demo.iam.gserviceaccount.com can publish build.oore.demo.shop on Google Play.',
      package_name: 'build.oore.demo.shop',
      service_account_email: 'oore-publisher@oore-demo.iam.gserviceaccount.com',
      checked_at: ago(2 * 3_600),
    },
  ],
])

const secretInputSchema = z.object({
  name: z.string().optional(),
  value: z.string().min(1),
})

function pipelineProjectId(pipelineId: string) {
  return demoState.pipelines.find((pipeline) => pipeline.id === pipelineId)
    ?.project_id
}

// Anyone with a role on the project can list its secret names.
function requireDemoProjectRead(request: Request, projectId: string) {
  const persona = getDemoPersonaFromRequest(request)
  if (persona.role === 'owner' || persona.role === 'admin') return null
  return getDemoProjectRole(persona, projectId)
    ? null
    : HttpResponse.json(
        { error: 'Project not found', code: 'not_found' },
        { status: 404 },
      )
}

function notFound(message: string) {
  return HttpResponse.json(
    { error: message, code: 'not_found' },
    { status: 404 },
  )
}

type Scope =
  | { scope: 'instance' }
  | { scope: 'project'; projectId: string }
  | { scope: 'pipeline'; pipelineId: string; projectId: string }

function matches(secret: SecretSummary, scope: Scope) {
  if (secret.scope !== scope.scope) return false
  if (scope.scope === 'project') return secret.project_id === scope.projectId
  if (scope.scope === 'pipeline') {
    return secret.pipeline_id === scope.pipelineId
  }
  return true
}

function authorize(request: Request, scope: Scope, write: boolean) {
  if (scope.scope === 'instance') {
    return requireDemoInstancePermission(
      request,
      write ? 'instance_settings:write' : 'instance_settings:read',
    )
  }
  return write
    ? requireDemoProjectPermission(request, scope.projectId, 'pipelines:write')
    : requireDemoProjectRead(request, scope.projectId)
}

function resolveScope(params: {
  pipelineId?: string | ReadonlyArray<string>
  projectId?: string | ReadonlyArray<string>
}): Scope | null {
  const projectId = z.string().safeParse(params.projectId).data
  const pipelineId = z.string().safeParse(params.pipelineId).data
  if (pipelineId) {
    const owner = pipelineProjectId(pipelineId)
    return owner ? { scope: 'pipeline', pipelineId, projectId: owner } : null
  }
  if (projectId) return { scope: 'project', projectId }
  return { scope: 'instance' }
}

function secretHandlers(base: string) {
  return [
    http.get(`*${base}`, async ({ params, request }) => {
      await delay(120)
      const scope = resolveScope(params)
      if (!scope) return notFound('Pipeline not found')
      const denied = authorize(request, scope, false)
      if (denied) return denied
      return HttpResponse.json({
        secrets: demoSecrets.filter((secret) => matches(secret, scope)),
      })
    }),
    http.post(`*${base}`, async ({ params, request }) => {
      await delay(200)
      const scope = resolveScope(params)
      if (!scope) return notFound('Pipeline not found')
      const denied = authorize(request, scope, true)
      if (denied) return denied
      const body = secretInputSchema.parse(await request.json())
      const name = body.name?.trim() ?? ''
      if (demoSecrets.some((s) => s.name === name && matches(s, scope))) {
        return HttpResponse.json(
          {
            error: `Secret '${name}' already exists in this scope; rotate it instead`,
            code: 'secret_exists',
          },
          { status: 409 },
        )
      }
      const now = Math.floor(Date.now() / 1000)
      const secret: SecretSummary = {
        name,
        scope: scope.scope,
        project_id: scope.scope === 'instance' ? undefined : scope.projectId,
        pipeline_id: scope.scope === 'pipeline' ? scope.pipelineId : undefined,
        created_at: now,
        updated_at: now,
      }
      demoSecrets.push(secret)
      return HttpResponse.json({ secret }, { status: 201 })
    }),
    http.put(`*${base}/:name`, async ({ params, request }) => {
      await delay(200)
      const scope = resolveScope(params)
      if (!scope) return notFound('Pipeline not found')
      const denied = authorize(request, scope, true)
      if (denied) return denied
      secretInputSchema.parse(await request.json())
      const secret = demoSecrets.find(
        (s) => s.name === params.name && matches(s, scope),
      )
      if (!secret) return notFound('Secret not found')
      secret.updated_at = Math.floor(Date.now() / 1000)
      return HttpResponse.json({ secret })
    }),
    http.delete(`*${base}/:name`, async ({ params, request }) => {
      await delay(150)
      const scope = resolveScope(params)
      if (!scope) return notFound('Pipeline not found')
      const denied = authorize(request, scope, true)
      if (denied) return denied
      const index = demoSecrets.findIndex(
        (s) => s.name === params.name && matches(s, scope),
      )
      if (index < 0) return notFound('Secret not found')
      demoSecrets.splice(index, 1)
      return HttpResponse.json({ ok: true })
    }),
  ]
}

function secretVisible(name: string | undefined, pipelineId: string) {
  const projectId = pipelineProjectId(pipelineId)
  return demoSecrets.some(
    (secret) =>
      secret.name === name &&
      (secret.scope === 'instance' ||
        (secret.scope === 'project' && secret.project_id === projectId) ||
        secret.pipeline_id === pipelineId),
  )
}

function playResponse(pipelineId: string): GooglePlayPublishResponse {
  const saved = demoPlayConfigs.get(pipelineId)
  return {
    pipeline_id: pipelineId,
    config: saved?.config,
    updated_at: saved?.updated_at,
    service_account_secret_present: secretVisible(
      saved?.config.service_account_secret,
      pipelineId,
    ),
    last_credentials_check: demoPlayChecks.get(pipelineId),
  }
}

const playCredentialsTestSchema = z.object({
  service_account_secret: z.string(),
  package_name: z.string().nullish(),
})

/** Mirrors the daemon's checks against the demo secrets. */
function playCredentialsTest(
  pipelineId: string,
  body: z.infer<typeof playCredentialsTestSchema>,
): GooglePlayCredentialsTest {
  const secret = body.service_account_secret.trim()
  const packageName =
    body.package_name?.trim() ||
    demoPlayConfigs.get(pipelineId)?.config.package_name ||
    (pipelineId === PIPELINE_IDS.shopAndroid
      ? 'build.oore.demo.shop'
      : undefined)
  const email = 'oore-publisher@oore-demo.iam.gserviceaccount.com'
  const checked_at = Math.floor(Date.now() / 1000)
  const service_account_secret = secret
  if (!secretVisible(secret, pipelineId)) {
    return {
      ok: false,
      service_account_secret,
      message: `No secret named ${secret} is visible to this pipeline. Add it to the pipeline, its project or the instance.`,
      package_name: packageName,
      checked_at,
    }
  }
  // Only this demo secret holds a service account key.
  if (secret !== 'PLAY_SERVICE_ACCOUNT') {
    return {
      ok: false,
      service_account_secret,
      message: `${secret} does not hold a Google service account JSON key. Store the whole key file downloaded from Google Cloud.`,
      package_name: packageName,
      checked_at,
    }
  }
  if (!packageName) {
    return {
      ok: false,
      service_account_secret,
      message:
        'Google accepted the key, but Oore does not know which app to check yet. Set the expected package under Advanced, then test again.',
      service_account_email: email,
      checked_at,
    }
  }
  return {
    ok: true,
    service_account_secret,
    message: `${email} can publish ${packageName} on Google Play.`,
    package_name: packageName,
    service_account_email: email,
    checked_at,
  }
}

const playConfigSchema = z.object({
  enabled: z.boolean(),
  service_account_secret: z.string().min(1),
  package_name: z.string().nullish(),
  track: z.string().min(1),
  rollout_percent: z.number().nullish(),
  draft: z.boolean(),
  release_notes: z.enum(['none', 'changelog', 'repo_files']),
  release_notes_locale: z.string(),
  release_notes_dir: z.string().nullish(),
  upload_mapping: z.boolean(),
  in_app_update_priority: z.number().int().min(0).max(5).nullish(),
})

export const secretHandlersList = [
  ...secretHandlers('/v1/secrets'),
  ...secretHandlers('/v1/projects/:projectId/secrets'),
  ...secretHandlers('/v1/pipelines/:pipelineId/secrets'),
  http.get(
    '*/v1/pipelines/:pipelineId/publish/google-play',
    async ({ params, request }) => {
      await delay(120)
      const pipelineId = z.string().parse(params.pipelineId)
      const projectId = pipelineProjectId(pipelineId)
      if (!projectId) return notFound('Pipeline not found')
      const denied = requireDemoProjectRead(request, projectId)
      if (denied) return denied
      return HttpResponse.json(playResponse(pipelineId))
    },
  ),
  http.put(
    '*/v1/pipelines/:pipelineId/publish/google-play',
    async ({ params, request }) => {
      await delay(200)
      const pipelineId = z.string().parse(params.pipelineId)
      const projectId = pipelineProjectId(pipelineId)
      if (!projectId) return notFound('Pipeline not found')
      const denied = requireDemoProjectPermission(
        request,
        projectId,
        'pipelines:write',
      )
      if (denied) return denied
      const parsed = playConfigSchema.safeParse(await request.json())
      if (!parsed.success) {
        return HttpResponse.json(
          { error: parsed.error.message, code: 'invalid_publish_config' },
          { status: 400 },
        )
      }
      demoPlayConfigs.set(pipelineId, {
        config: parsed.data,
        updated_at: Math.floor(Date.now() / 1000),
      })
      return HttpResponse.json(playResponse(pipelineId))
    },
  ),
  http.post(
    '*/v1/pipelines/:pipelineId/publish/google-play/test-credentials',
    async ({ params, request }) => {
      await delay(600)
      const pipelineId = z.string().parse(params.pipelineId)
      const projectId = pipelineProjectId(pipelineId)
      if (!projectId) return notFound('Pipeline not found')
      const denied = requireDemoProjectPermission(
        request,
        projectId,
        'pipelines:write',
      )
      if (denied) return denied
      const body = playCredentialsTestSchema.parse(await request.json())
      const result = playCredentialsTest(pipelineId, body)
      if (secretVisible(result.service_account_secret, pipelineId)) {
        demoPlayChecks.set(pipelineId, result)
      }
      return HttpResponse.json(result)
    },
  ),
]
