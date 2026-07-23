import { HttpResponse, delay, http } from 'msw'
import type { ExternalAccessPreflightCheck } from '@/lib/types'
import { requireDemoInstancePermission } from '../authorization'
import { demoState } from '../state'

const DEMO_OIDC_ISSUER = 'https://accounts.google.com'

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
  http.get('/v1/settings/artifact-storage', async () => {
    await delay(150)
    return HttpResponse.json({ settings: demoState.artifactStorage })
  }),

  http.put('/v1/settings/artifact-storage', async ({ request }) => {
    await delay(300)
    const forbidden = requireDemoInstancePermission(
      request,
      'instance_settings:write',
    )
    if (forbidden) return forbidden
    const body = (await request.json()) as Record<string, unknown>

    const accessKeyId = (body.access_key_id as string | undefined)?.trim()
    const secretAccessKey = (
      body.secret_access_key as string | undefined
    )?.trim()

    // Persist non-secret configuration in demo state; never echo secrets.
    demoState.artifactStorage = {
      ...demoState.artifactStorage,
      ...body,
      has_access_key_id:
        demoState.artifactStorage.has_access_key_id || !!accessKeyId,
      has_secret_access_key:
        demoState.artifactStorage.has_secret_access_key || !!secretAccessKey,
      updated_at: now(),
    }
    delete (demoState.artifactStorage as any).access_key_id
    delete (demoState.artifactStorage as any).secret_access_key

    return HttpResponse.json({
      settings: {
        ...demoState.artifactStorage,
      },
    })
  }),

  http.get('/v1/settings/preferences', async ({ request }) => {
    await delay(150)
    const forbidden = requireDemoInstancePermission(
      request,
      'instance_settings:read',
    )
    if (forbidden) return forbidden
    return HttpResponse.json({ preferences: demoState.preferences })
  }),

  http.put('/v1/settings/preferences', async ({ request }) => {
    await delay(300)
    const forbidden = requireDemoInstancePermission(
      request,
      'instance_settings:write',
    )
    if (forbidden) return forbidden
    const body = (await request.json()) as Record<string, unknown>

    demoState.preferences = {
      ...demoState.preferences,
      ...body,
      updated_at: now(),
    }

    return HttpResponse.json({
      preferences: {
        ...demoState.preferences,
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
    return HttpResponse.json({ settings: demoState.externalAccessNetwork })
  }),

  http.put('/v1/settings/external-access/network', async ({ request }) => {
    await delay(250)
    const forbidden = requireDemoInstancePermission(
      request,
      'instance_settings:write',
    )
    if (forbidden) return forbidden
    const body = (await request.json()) as {
      public_url?: string
      artifact_delivery_url?: string
      allowed_origins: Array<string>
    }
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

  http.get('/v1/settings/external-access/trusted-proxy', async () => {
    await delay(150)
    return HttpResponse.json({ settings: demoState.trustedProxy })
  }),

  http.put(
    '/v1/settings/external-access/trusted-proxy',
    async ({ request }) => {
      await delay(250)
      const forbidden = requireDemoInstancePermission(
        request,
        'instance_settings:write',
      )
      if (forbidden) return forbidden
      const body = (await request.json()) as {
        user_email_header?: string
        trusted_proxy_cidrs: Array<string>
        shared_secret?: string
        warpgate_ticket?: string
      }
      demoState.trustedProxy = {
        ...demoState.trustedProxy,
        user_email_header:
          body.user_email_header ?? demoState.trustedProxy.user_email_header,
        trusted_proxy_cidrs: body.trusted_proxy_cidrs,
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
    },
  ),

  http.get('/v1/settings/external-access/oidc', async () => {
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

  http.put('/v1/settings/external-access/oidc', async ({ request }) => {
    await delay(250)
    const forbidden = requireDemoInstancePermission(
      request,
      'instance_settings:write',
    )
    if (forbidden) return forbidden
    const body = (await request.json()) as {
      issuer_url?: string
      client_id?: string
      client_secret?: string
    }

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

  http.post(
    '/v1/settings/external-access/oidc/test-connection',
    async ({ request }) => {
      await delay(500)
      const forbidden = requireDemoInstancePermission(
        request,
        'instance_settings:write',
      )
      if (forbidden) return forbidden
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
