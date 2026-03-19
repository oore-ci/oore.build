import { HttpResponse, delay, http } from 'msw'
import {
  demoArtifactStorageSettings,
  demoInstancePreferences,
} from '../data/settings'
import { ago } from '../seed'
import type {
  ArtifactStorageSettings,
  ExternalAccessNetworkSettings,
  ExternalAccessPreflightCheck,
  InstancePreferences,
  TrustedProxySettingsPublic,
} from '@/lib/types'

const DEMO_OIDC_ISSUER = 'https://accounts.google.com'

function now(): number {
  return Math.floor(Date.now() / 1000)
}

// Mutate module-scoped state so demo changes survive refresh-free navigation.
let artifactStorageSettings: ArtifactStorageSettings = {
  ...demoArtifactStorageSettings,
}
let instancePreferences: InstancePreferences = { ...demoInstancePreferences }

let externalAccessNetworkSettings: ExternalAccessNetworkSettings = {
  public_url: undefined,
  allowed_origins: [],
  source: 'default',
  updated_at: ago(86400 * 30),
}

let trustedProxySettings: TrustedProxySettingsPublic = {
  user_email_header: 'x-warpgate-username',
  trusted_proxy_cidrs: [],
  has_shared_secret: false,
  updated_at: ago(86400 * 30),
}

let oidcConfigured = true
let oidcIssuer = DEMO_OIDC_ISSUER
let oidcHasClientSecret = false
let oidcConfiguredAt = ago(86400 * 30)

function ensureExternalAccessDefaults(origin: string) {
  if (externalAccessNetworkSettings.allowed_origins.length === 0) {
    externalAccessNetworkSettings = {
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
      ok: (externalAccessNetworkSettings.public_url ?? '').startsWith(
        'https://',
      ),
      message: 'Public URL uses HTTPS.',
      failure_code: 'external_access_https_required',
    },
    {
      id: 'public_origin_allowed',
      label: 'Frontend origin is allowed',
      ok: externalAccessNetworkSettings.allowed_origins.includes(origin),
      message: 'Frontend origin is present in allowed origins.',
      failure_code: 'external_access_origin_not_allowed',
    },
    {
      id: 'redirect_policy_consistent',
      label: 'Redirect policy is consistent',
      ok: true,
      message: 'Redirect URI policy matches configured origins.',
    },
    {
      id: 'oidc_configured',
      label: 'OIDC configured',
      ok: oidcConfigured,
      message: oidcConfigured
        ? `OIDC is configured (${oidcIssuer}).`
        : 'Configure OIDC to enable External Access.',
      failure_code: 'external_access_oidc_not_configured',
    },
  ]

  return {
    ready: checks.every((check) => check.ok),
    checks,
  }
}

export const settingsHandlers = [
  http.get('/v1/settings/artifact-storage', async () => {
    await delay(150)
    return HttpResponse.json({ settings: artifactStorageSettings })
  }),

  http.put('/v1/settings/artifact-storage', async ({ request }) => {
    await delay(300)
    const body = (await request.json()) as Record<string, unknown>

    const accessKeyId = (body.access_key_id as string | undefined)?.trim()
    const secretAccessKey = (
      body.secret_access_key as string | undefined
    )?.trim()

    // Persist non-secret configuration in demo state; never echo secrets.
    artifactStorageSettings = {
      ...artifactStorageSettings,
      ...body,
      has_access_key_id:
        artifactStorageSettings.has_access_key_id || !!accessKeyId,
      has_secret_access_key:
        artifactStorageSettings.has_secret_access_key || !!secretAccessKey,
      updated_at: now(),
    }
    delete (artifactStorageSettings as any).access_key_id
    delete (artifactStorageSettings as any).secret_access_key

    return HttpResponse.json({
      settings: {
        ...artifactStorageSettings,
      },
    })
  }),

  http.get('/v1/settings/preferences', async () => {
    await delay(150)
    return HttpResponse.json({ preferences: instancePreferences })
  }),

  http.put('/v1/settings/preferences', async ({ request }) => {
    await delay(300)
    const body = (await request.json()) as Record<string, unknown>

    instancePreferences = {
      ...instancePreferences,
      ...body,
      updated_at: now(),
    } as InstancePreferences

    return HttpResponse.json({
      preferences: {
        ...instancePreferences,
      },
    })
  }),

  http.get('/v1/settings/external-access/preflight', async ({ request }) => {
    await delay(200)
    const origin = new URL(request.url).origin
    return HttpResponse.json(buildPreflight(origin))
  }),

  http.get('/v1/settings/external-access/network', async ({ request }) => {
    await delay(150)
    ensureExternalAccessDefaults(new URL(request.url).origin)
    return HttpResponse.json({ settings: externalAccessNetworkSettings })
  }),

  http.put('/v1/settings/external-access/network', async ({ request }) => {
    await delay(250)
    const body = (await request.json()) as {
      public_url?: string
      allowed_origins: Array<string>
    }
    externalAccessNetworkSettings = {
      ...externalAccessNetworkSettings,
      public_url: body.public_url,
      allowed_origins: body.allowed_origins,
      source: 'database',
      updated_at: now(),
    }
    return HttpResponse.json({ settings: externalAccessNetworkSettings })
  }),

  http.get('/v1/settings/external-access/trusted-proxy', async () => {
    await delay(150)
    return HttpResponse.json({ settings: trustedProxySettings })
  }),

  http.put(
    '/v1/settings/external-access/trusted-proxy',
    async ({ request }) => {
      await delay(250)
      const body = (await request.json()) as {
        user_email_header?: string
        trusted_proxy_cidrs: Array<string>
        shared_secret?: string
      }
      trustedProxySettings = {
        ...trustedProxySettings,
        user_email_header:
          body.user_email_header ?? trustedProxySettings.user_email_header,
        trusted_proxy_cidrs: body.trusted_proxy_cidrs,
        has_shared_secret:
          trustedProxySettings.has_shared_secret || !!body.shared_secret,
        updated_at: now(),
      }
      return HttpResponse.json({ settings: trustedProxySettings })
    },
  ),

  http.get('/v1/settings/external-access/oidc', async () => {
    await delay(150)
    if (!oidcConfigured) {
      return new HttpResponse(
        JSON.stringify({
          code: 'oidc_not_configured',
          message: 'No OIDC provider is configured',
        }),
        { status: 404 },
      )
    }
    const base = oidcIssuer.replace(/\/$/, '')
    return HttpResponse.json({
      issuer_url: oidcIssuer,
      client_id: 'demo-client-id',
      has_client_secret: oidcHasClientSecret,
      authorization_endpoint: `${base}/o/oauth2/v2/auth`,
      token_endpoint: `${base}/token`,
      userinfo_endpoint: `${base}/userinfo`,
      jwks_uri: `${base}/jwks`,
      configured_at: oidcConfiguredAt,
    })
  }),

  http.put('/v1/settings/external-access/oidc', async ({ request }) => {
    await delay(250)
    const body = (await request.json()) as {
      issuer_url?: string
      client_id?: string
      client_secret?: string
    }

    oidcConfigured = true
    oidcIssuer = body.issuer_url ?? oidcIssuer
    oidcHasClientSecret = oidcHasClientSecret || !!body.client_secret
    oidcConfiguredAt = now()

    return HttpResponse.json({
      discovered_issuer: oidcIssuer,
      has_client_secret: oidcHasClientSecret,
      configured_at: oidcConfiguredAt,
    })
  }),

  http.post(
    '/v1/settings/external-access/oidc/test-connection',
    async ({ request }) => {
      await delay(500)
      const body = (await request.json()) as { issuer_url?: string }
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
    },
  ),
]
