import { HttpResponse, delay } from 'msw'
import * as z from 'zod'

import type {
  AppStoreConnectionTestResponse,
  GooglePlayCredentialsTestResponse,
  ProjectStoreOverride,
  ProjectStoresResponse,
} from '@oore/client/models'
import { requireDemoProjectPermission } from '../authorization'
import { PROJECT_IDS, ago } from '../seed'
import { demoState } from '../state'
import { demoApi } from './api'
import { requireDemoProjectRead } from './secrets'
import { DEMO_SHARED_KEY_ID, demoSharedKeySummary } from './store-credentials'

const PLAY_EMAIL = 'oore-publisher@oore-demo.iam.gserviceaccount.com'

interface DemoPlayConnection {
  service_account_secret: string
  package_name?: string | null
  last_check?: GooglePlayCredentialsTestResponse
  updated_at: number
}

interface DemoAppStoreConnection {
  key_id: string
  bundle_id?: string | null
  last_check?: AppStoreConnectionTestResponse
  updated_at: number
}

// The shop app is connected to both stores; the others are not yet.
const playConnections = new Map<string, DemoPlayConnection>([
  [
    PROJECT_IDS.flutterShop,
    {
      service_account_secret: 'PLAY_SERVICE_ACCOUNT',
      package_name: 'build.oore.demo.shop',
      last_check: {
        ok: true,
        service_account_secret: 'PLAY_SERVICE_ACCOUNT',
        message: `${PLAY_EMAIL} can publish build.oore.demo.shop on Google Play.`,
        package_name: 'build.oore.demo.shop',
        service_account_email: PLAY_EMAIL,
        checked_at: ago(2 * 3_600),
      },
      updated_at: ago(5 * 86_400),
    },
  ],
])

const appStoreConnections = new Map<string, DemoAppStoreConnection>([
  [
    PROJECT_IDS.flutterShop,
    {
      key_id: DEMO_SHARED_KEY_ID,
      bundle_id: 'build.oore.demo.shop',
      last_check: {
        ok: true,
        message:
          'Key T8K2L4M6NP can reach build.oore.demo.shop in App Store Connect.',
        bundle_id: 'build.oore.demo.shop',
        checked_at: ago(3 * 3_600),
      },
      updated_at: ago(5 * 86_400),
    },
  ],
])

function projectOf(pipelineId: string) {
  return demoState.pipelines.find((pipeline) => pipeline.id === pipelineId)
    ?.project_id
}

/** The Play secret the app's pipelines follow, when the app has one. */
export function demoAppPlaySecret(pipelineId: string) {
  const projectId = projectOf(pipelineId)
  return projectId
    ? playConnections.get(projectId)?.service_account_secret
    : undefined
}

/** Whether the pipeline's app has an App Store Connect key. */
export function demoAppHasAppStoreKey(pipelineId: string) {
  const projectId = projectOf(pipelineId)
  return !!projectId && appStoreConnections.has(projectId)
}

function storesResponse(projectId: string): ProjectStoresResponse {
  const play = playConnections.get(projectId)
  const appStore = appStoreConnections.get(projectId)
  const key = appStore ? demoSharedKeySummary(appStore.key_id) : undefined
  return {
    project_id: projectId,
    google_play: play
      ? {
          service_account_secret: play.service_account_secret,
          service_account_secret_present: true,
          package_name: play.package_name,
          last_check: play.last_check,
          updated_at: play.updated_at,
        }
      : null,
    app_store:
      appStore && key
        ? {
            key,
            bundle_id: appStore.bundle_id,
            last_check: appStore.last_check,
            updated_at: appStore.updated_at,
          }
        : null,
    overrides: [] satisfies Array<ProjectStoreOverride>,
  }
}

const PACKAGE = /^[A-Za-z][A-Za-z0-9_]*(\.[A-Za-z][A-Za-z0-9_]*)+$/
const BUNDLE = /^[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)+$/

const playSchema = z.object({
  service_account_secret: z.string().trim().min(1),
  package_name: z
    .string()
    .trim()
    .nullish()
    .refine((value) => !value || PACKAGE.test(value)),
})

const appStoreSchema = z.object({
  app_store_connect_key_id: z.string().trim().min(1),
  bundle_id: z
    .string()
    .trim()
    .nullish()
    .refine((value) => !value || BUNDLE.test(value)),
})

function invalid(message: string, code = 'invalid_input') {
  return HttpResponse.json({ error: message, code }, { status: 400 })
}

function playTest(
  body: z.infer<typeof playSchema>,
): GooglePlayCredentialsTestResponse {
  const secret = body.service_account_secret
  const checked_at = Math.floor(Date.now() / 1000)
  if (secret !== 'PLAY_SERVICE_ACCOUNT') {
    return {
      ok: false,
      service_account_secret: secret,
      message: `${secret} does not hold a Google service account JSON key. Store the whole key file downloaded from Google Cloud.`,
      checked_at,
    }
  }
  if (!body.package_name) {
    return {
      ok: false,
      service_account_secret: secret,
      message:
        'Google accepted the key, but Oore does not know which app to check yet. Set the package name, then test again.',
      service_account_email: PLAY_EMAIL,
      checked_at,
    }
  }
  return {
    ok: true,
    service_account_secret: secret,
    message: `${PLAY_EMAIL} can publish ${body.package_name} on Google Play.`,
    package_name: body.package_name,
    service_account_email: PLAY_EMAIL,
    checked_at,
  }
}

export const storeHandlers = [
  demoApi.getProjectStores(async ({ params, request }) => {
    await delay(120)
    const projectId = String(params.project_id)
    const forbidden = requireDemoProjectRead(request, projectId)
    if (forbidden) return forbidden
    return HttpResponse.json(storesResponse(projectId))
  }),

  demoApi.updateProjectGooglePlayConnection(async ({ params, request }) => {
    await delay(200)
    const projectId = String(params.project_id)
    const forbidden = requireDemoProjectPermission(
      request,
      projectId,
      'pipelines:write',
    )
    if (forbidden) return forbidden
    const parsed = playSchema.safeParse(await request.json())
    if (!parsed.success)
      return invalid('Check the secret name and package name')
    const previous = playConnections.get(projectId)
    const same =
      previous?.service_account_secret === parsed.data.service_account_secret &&
      (previous.package_name ?? null) === (parsed.data.package_name || null)
    playConnections.set(projectId, {
      service_account_secret: parsed.data.service_account_secret,
      package_name: parsed.data.package_name || null,
      last_check: same ? previous.last_check : undefined,
      updated_at: ago(0),
    })
    return HttpResponse.json(storesResponse(projectId))
  }),

  demoApi.deleteProjectGooglePlayConnection(async ({ params, request }) => {
    await delay(200)
    const projectId = String(params.project_id)
    const forbidden = requireDemoProjectPermission(
      request,
      projectId,
      'pipelines:write',
    )
    if (forbidden) return forbidden
    if (projectId === PROJECT_IDS.flutterShop) {
      return HttpResponse.json(
        {
          error:
            'Android Release uploads to Google Play with this connection. Turn its uploads off first.',
          code: 'store_connection_in_use',
        },
        { status: 409 },
      )
    }
    playConnections.delete(projectId)
    return HttpResponse.json(storesResponse(projectId))
  }),

  demoApi.testProjectGooglePlayConnection(async ({ params, request }) => {
    await delay(700)
    const projectId = String(params.project_id)
    const forbidden = requireDemoProjectPermission(
      request,
      projectId,
      'pipelines:write',
    )
    if (forbidden) return forbidden
    const parsed = playSchema.safeParse(await request.json())
    if (!parsed.success)
      return invalid('Check the secret name and package name')
    const result = playTest(parsed.data)
    const saved = playConnections.get(projectId)
    if (
      saved &&
      saved.service_account_secret === parsed.data.service_account_secret &&
      (saved.package_name ?? null) === (parsed.data.package_name || null)
    ) {
      saved.last_check = result
    }
    return HttpResponse.json(result)
  }),

  demoApi.updateProjectAppStoreConnection(async ({ params, request }) => {
    await delay(200)
    const projectId = String(params.project_id)
    const forbidden = requireDemoProjectPermission(
      request,
      projectId,
      'pipelines:write',
    )
    if (forbidden) return forbidden
    const parsed = appStoreSchema.safeParse(await request.json())
    if (!parsed.success) return invalid('Check the key and bundle ID')
    if (!demoSharedKeySummary(parsed.data.app_store_connect_key_id)) {
      return invalid(
        'The selected App Store Connect key no longer exists',
        'invalid_api_key',
      )
    }
    const previous = appStoreConnections.get(projectId)
    const same =
      previous?.key_id === parsed.data.app_store_connect_key_id &&
      (previous.bundle_id ?? null) === (parsed.data.bundle_id || null)
    appStoreConnections.set(projectId, {
      key_id: parsed.data.app_store_connect_key_id,
      bundle_id: parsed.data.bundle_id || null,
      last_check: same ? previous.last_check : undefined,
      updated_at: ago(0),
    })
    return HttpResponse.json(storesResponse(projectId))
  }),

  demoApi.deleteProjectAppStoreConnection(async ({ params, request }) => {
    await delay(200)
    const projectId = String(params.project_id)
    const forbidden = requireDemoProjectPermission(
      request,
      projectId,
      'pipelines:write',
    )
    if (forbidden) return forbidden
    appStoreConnections.delete(projectId)
    return HttpResponse.json(storesResponse(projectId))
  }),

  demoApi.testProjectAppStoreConnection(async ({ params, request }) => {
    await delay(700)
    const projectId = String(params.project_id)
    const forbidden = requireDemoProjectPermission(
      request,
      projectId,
      'pipelines:write',
    )
    if (forbidden) return forbidden
    const parsed = appStoreSchema.safeParse(await request.json())
    if (!parsed.success) return invalid('Check the key and bundle ID')
    const key = demoSharedKeySummary(parsed.data.app_store_connect_key_id)
    const bundle = parsed.data.bundle_id || null
    const result: AppStoreConnectionTestResponse = {
      ok: !!key && !!bundle,
      message: !key
        ? 'The selected App Store Connect key no longer exists.'
        : bundle
          ? `Key ${key.key_id} can reach ${bundle} in App Store Connect.`
          : 'Oore does not know which app to check yet. Set the bundle ID, then test again.',
      bundle_id: bundle,
      checked_at: Math.floor(Date.now() / 1000),
    }
    const saved = appStoreConnections.get(projectId)
    if (
      saved &&
      saved.key_id === parsed.data.app_store_connect_key_id &&
      (saved.bundle_id ?? null) === bundle
    ) {
      saved.last_check = result
    }
    return HttpResponse.json(result)
  }),
]
