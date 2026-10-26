import { demoApi } from './api'
import { HttpResponse, delay } from 'msw'
import * as z from 'zod'
import type {
  ConfigureExternalAccessOidcRequest,
  ExternalAccessPreflightCheck,
  TestOidcConnectionRequest,
  UpdateArtifactStorageSettingsRequest,
  UpdateExternalAccessNetworkSettingsRequest,
  UpdateInstancePreferencesRequest,
  UpdateTrustedProxySettingsRequest,
} from '@oore/client/models'
import { requireDemoInstancePermission } from '../authorization'
import { demoState } from '../state'

const DEMO_OIDC_ISSUER = 'https://accounts.google.com'

const artifactStorageRequestSchema =
  z.toZod<UpdateArtifactStorageSettingsRequest>()(
    z.object({
      provider: z.enum(['disabled', 'local', 's3', 'r2']),
      local_base_dir: z.string().nullish(),
      s3_bucket: z.string().nullish(),
      s3_region: z.string().nullish(),
      s3_endpoint: z.string().nullish(),
      access_key_id: z.string().nullish(),
      secret_access_key: z.string().nullish(),
    }),
  )

const preferencesRequestSchema = z.toZod<UpdateInstancePreferencesRequest>()(
  z.object({
    key_storage_mode: z.enum(['keychain', 'file']),
    runtime_mode: z.enum(['local', 'remote']).nullish(),
    remote_auth_mode: z.enum(['oidc', 'trusted_proxy']).nullish(),
    direct_macos_runner_paused: z.boolean().nullish(),
  }),
)

const networkRequestSchema =
  z.toZod<UpdateExternalAccessNetworkSettingsRequest>()(
    z.object({
      public_url: z.string().nullish(),
      artifact_delivery_url: z.string().nullish(),
      allowed_origins: z.array(z.string()),
    }),
  )

const trustedProxyRequestSchema = z.toZod<UpdateTrustedProxySettingsRequest>()(
  z.object({
    user_email_header: z.string().nullish(),
    trusted_proxy_cidrs: z.array(z.string()).optional(),
    shared_secret: z.string().nullish(),
    warpgate_ticket: z.string().nullish(),
  }),
)

const oidcRequestSchema = z.toZod<ConfigureExternalAccessOidcRequest>()(
  z.object({
    issuer_url: z.string(),
    client_id: z.string(),
    client_secret: z.string().nullish(),
  }),
)

const testOidcRequestSchema = z.toZod<TestOidcConnectionRequest>()(
  z.object({ issuer_url: z.string() }),
)

function now(): number {
  return Math.floor(Date.now() / 1000)
}

function ensureExternalAccessDefaults(origin: string) {
  if (demoState.externalAccessNetwork.allowed_origins.length === 0) {
    demoState.externalAccessNetwork = {
      public_url: origin,
      allowed_origins: [origin],
      source: 'database',
      updated_at: now(),
    }
  }
}

function buildPreflight(origin: string) {
  ensureExternalAccessDefaults(origin)

  const checks: Array<ExternalAccessPreflightCheck> = [
    {
      id: 'setup_ready',
      label: 'Setup complete',
      ok: true,
      message: 'Instance is ready.',
    },
    {
      id: 'public_url_https',
      label: 'Public URL is HTTPS',
      ok: (demoState.externalAccessNetwork.public_url ?? '').startsWith(
        'https://',
      ),
      message: 'Public URL uses HTTPS.',
      failure_code: 'external_access_https_required',
    },
    {
      id: 'public_origin_allowed',
      label: 'Frontend origin is allowed',
      ok: demoState.externalAccessNetwork.allowed_origins.includes(origin),
      message: 'Frontend origin is present in allowed origins.',
      failure_code: 'external_access_origin_not_allowed',
    },
  ]

  if (demoState.preferences.remote_auth_mode === 'trusted_proxy') {
    checks.push({
      id: 'trusted_proxy_configured',
      label: 'Trusted proxy configured',
      ok:
        !!demoState.trustedProxy.user_email_header &&
        demoState.trustedProxy.has_shared_secret,
      message: demoState.trustedProxy.has_shared_secret
        ? 'Trusted Proxy settings are configured.'
        : 'Configure Trusted Proxy to enable External Access.',
      failure_code: 'external_access_trusted_proxy_not_configured',
    })
  } else {
    checks.push(
      {
        id: 'redirect_policy_consistent',
        label: 'Redirect policy is consistent',
        ok: true,
        message: 'Redirect URI policy matches configured origins.',
      },
      {
        id: 'oidc_configured',
        label: 'OIDC configured',
        ok: demoState.oidc.configured,
        message: demoState.oidc.configured
          ? `OIDC is configured (${demoState.oidc.issuer}).`
          : 'Configure OIDC to enable External Access.',
        failure_code: 'external_access_oidc_not_configured',
      },
    )
  }

  return {
    ready: checks.every((check) => check.ok),
    checks,
  }
}

export const settingsHandlers = [
  demoApi.getArtifactStorageSettings(async () => {
    await delay(150)
    return HttpResponse.json({ settings: demoState.artifactStorage })
  }),

  demoApi.updateArtifactStorageSettings(async ({ request }) => {
    await delay(300)
    const forbidden = requireDemoInstancePermission(
      request,
      'instance_settings:write',
    )
    if (forbidden) return forbidden
    const body = artifactStorageRequestSchema.parse(await request.json())

    const accessKeyId = body.access_key_id?.trim()
    const secretAccessKey = body.secret_access_key?.trim()

    // Persist non-secret configuration in demo state; never echo secrets.
    demoState.artifactStorage = {
      ...demoState.artifactStorage,
      provider: body.provider,
      local_base_dir: body.local_base_dir,
      s3_bucket: body.s3_bucket,
      s3_region: body.s3_region,
      s3_endpoint: body.s3_endpoint,
      has_access_key_id:
        demoState.artifactStorage.has_access_key_id || !!accessKeyId,
      has_secret_access_key:
        demoState.artifactStorage.has_secret_access_key || !!secretAccessKey,
      updated_at: now(),
    }
    return HttpResponse.json({
      settings: {
        ...demoState.artifactStorage,
      },
    })
  }),

  demoApi.getInstancePreferences(async ({ request }) => {
    await delay(150)
    const forbidden = requireDemoInstancePermission(
      request,
      'instance_settings:read',
    )
    if (forbidden) return forbidden
    return HttpResponse.json({ preferences: demoState.preferences })
  }),

  demoApi.updateInstancePreferences(async ({ request }) => {
    await delay(300)
    const forbidden = requireDemoInstancePermission(
      request,
      'instance_settings:write',
    )
    if (forbidden) return forbidden
    const body = preferencesRequestSchema.parse(await request.json())

    demoState.preferences = {
      ...demoState.preferences,
      key_storage_mode: body.key_storage_mode,
      runtime_mode: body.runtime_mode ?? demoState.preferences.runtime_mode,
      remote_auth_mode:
        body.remote_auth_mode ?? demoState.preferences.remote_auth_mode,
      direct_macos_runner_paused:
        body.direct_macos_runner_paused ??
        demoState.preferences.direct_macos_runner_paused,
      updated_at: now(),
    }

    return HttpResponse.json({
      preferences: {
        ...demoState.preferences,
      },
    })
  }),

  demoApi.getExternalAccessPreflight(async ({ request }) => {
    await delay(200)
    const origin = new URL(request.url).origin
    return HttpResponse.json(buildPreflight(origin))
  }),

  demoApi.getExternalAccessNetworkSettings(async ({ request }) => {
    await delay(150)
    ensureExternalAccessDefaults(new URL(request.url).origin)
    return HttpResponse.json({ settings: demoState.externalAccessNetwork })
  }),

  demoApi.updateExternalAccessNetworkSettings(async ({ request }) => {
    await delay(250)
    const forbidden = requireDemoInstancePermission(
      request,
      'instance_settings:write',
    )
    if (forbidden) return forbidden
    const body = networkRequestSchema.parse(await request.json())
    demoState.externalAccessNetwork = {
      ...demoState.externalAccessNetwork,
      public_url: body.public_url,
      artifact_delivery_url: body.artifact_delivery_url,
      allowed_origins: body.allowed_origins,
      source: 'database',
      updated_at: now(),
    }
    return HttpResponse.json({ settings: demoState.externalAccessNetwork })
  }),

  demoApi.getExternalAccessTrustedProxySettings(async () => {
    await delay(150)
    return HttpResponse.json({ settings: demoState.trustedProxy })
  }),

  demoApi.updateExternalAccessTrustedProxySettings(async ({ request }) => {
    await delay(250)
    const forbidden = requireDemoInstancePermission(
      request,
      'instance_settings:write',
    )
    if (forbidden) return forbidden
    const body = trustedProxyRequestSchema.parse(await request.json())
    demoState.trustedProxy = {
      ...demoState.trustedProxy,
      user_email_header:
        body.user_email_header ?? demoState.trustedProxy.user_email_header,
      trusted_proxy_cidrs:
        body.trusted_proxy_cidrs ?? demoState.trustedProxy.trusted_proxy_cidrs,
      has_shared_secret:
        demoState.trustedProxy.has_shared_secret || !!body.shared_secret,
      has_warpgate_ticket:
        body.warpgate_ticket === ''
          ? false
          : demoState.trustedProxy.has_warpgate_ticket ||
            !!body.warpgate_ticket,
      warpgate_ticket_source:
        body.warpgate_ticket === ''
          ? undefined
          : body.warpgate_ticket
            ? 'database'
            : demoState.trustedProxy.warpgate_ticket_source,
      updated_at: now(),
    }
    return HttpResponse.json({ settings: demoState.trustedProxy })
  }),

  demoApi.getExternalAccessOidc(async () => {
    await delay(150)
    if (!demoState.oidc.configured) {
      return new HttpResponse(
        JSON.stringify({
          code: 'oidc_not_configured',
          message: 'No OIDC provider is configured',
        }),
        { status: 404 },
      )
    }
    const base = demoState.oidc.issuer.replace(/\/$/, '')
    return HttpResponse.json({
      issuer_url: demoState.oidc.issuer,
      client_id: 'demo-client-id',
      has_client_secret: demoState.oidc.hasClientSecret,
      authorization_endpoint: `${base}/o/oauth2/v2/auth`,
      token_endpoint: `${base}/token`,
      userinfo_endpoint: `${base}/userinfo`,
      jwks_uri: `${base}/jwks`,
      configured_at: demoState.oidc.configuredAt,
    })
  }),

  demoApi.configureExternalAccessOidc(async ({ request }) => {
    await delay(250)
    const forbidden = requireDemoInstancePermission(
      request,
      'instance_settings:write',
    )
    if (forbidden) return forbidden
    const body = oidcRequestSchema.parse(await request.json())

    demoState.oidc.configured = true
    demoState.oidc.issuer = body.issuer_url ?? demoState.oidc.issuer
    demoState.oidc.hasClientSecret =
      demoState.oidc.hasClientSecret || !!body.client_secret
    demoState.oidc.configuredAt = now()

    return HttpResponse.json({
      discovered_issuer: demoState.oidc.issuer,
      has_client_secret: demoState.oidc.hasClientSecret,
      configured_at: demoState.oidc.configuredAt,
    })
  }),

  demoApi.testOidcConnection(async ({ request }) => {
    await delay(500)
    const forbidden = requireDemoInstancePermission(
      request,
      'instance_settings:write',
    )
    if (forbidden) return forbidden
    const body = testOidcRequestSchema.parse(await request.json())
    const issuer = (body.issuer_url ?? DEMO_OIDC_ISSUER).replace(/\/$/, '')
    return HttpResponse.json({
      success: true,
      discovered_issuer: issuer,
      authorization_endpoint: `${issuer}/o/oauth2/v2/auth`,
      token_endpoint: `${issuer}/token`,
      userinfo_endpoint: `${issuer}/userinfo`,
      jwks_uri: `${issuer}/jwks`,
      scopes_supported: ['openid', 'email', 'profile'],
    })
  }),
]
