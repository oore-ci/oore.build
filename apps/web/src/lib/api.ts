import type {
  ApiError,
  BootstrapTokenVerifyResponse,
  OidcConfigureRequest,
  OidcConfigureResponse,
  SetupCompleteResponse,
  SetupOidcStartResponse,
  SetupOidcVerifyResponse,
  SetupStatus,
} from '@/lib/types'

// ── Error class ─────────────────────────────────────────────────

export class ApiClientError extends Error {
  readonly status: number
  readonly code: string
  readonly details: string | undefined

  constructor(status: number, body: ApiError) {
    super(body.error)
    this.name = 'ApiClientError'
    this.status = status
    this.code = body.code
    this.details = body.details
  }
}

// ── Fetch wrapper ───────────────────────────────────────────────

const BASE_URL = ''

async function request<T>(
  path: string,
  options: RequestInit = {},
): Promise<T> {
  const res = await fetch(`${BASE_URL}${path}`, {
    ...options,
    headers: {
      'Content-Type': 'application/json',
      ...options.headers,
    },
  })

  if (!res.ok) {
    let body: ApiError
    try {
      body = (await res.json()) as ApiError
    } catch {
      body = {
        error: `Request failed with status ${res.status}`,
        code: 'unknown_error',
      }
    }
    throw new ApiClientError(res.status, body)
  }

  return (await res.json()) as T
}

function authHeaders(token: string): Record<string, string> {
  return { Authorization: `Bearer ${token}` }
}

// ── Error helpers ───────────────────────────────────────────────

export function getApiErrorMessage(
  error: unknown,
  codeMap: Record<string, string>,
): string {
  if (error instanceof ApiClientError) {
    return codeMap[error.code] ?? error.message
  }
  if (error instanceof Error) {
    return error.message
  }
  return 'An unexpected error occurred. Please try again.'
}

// ── API functions ───────────────────────────────────────────────

export function getSetupStatus(): Promise<SetupStatus> {
  return request<SetupStatus>('/v1/public/setup-status')
}

export function verifyBootstrapToken(
  token: string,
): Promise<BootstrapTokenVerifyResponse> {
  return request<BootstrapTokenVerifyResponse>(
    '/v1/setup/bootstrap-token/verify',
    {
      method: 'POST',
      body: JSON.stringify({ token }),
    },
  )
}

export function configureOidc(
  sessionToken: string,
  data: OidcConfigureRequest,
): Promise<OidcConfigureResponse> {
  return request<OidcConfigureResponse>('/v1/setup/oidc/configure', {
    method: 'POST',
    headers: authHeaders(sessionToken),
    body: JSON.stringify(data),
  })
}

export function setupOidcStart(
  sessionToken: string,
  redirectUri: string,
): Promise<SetupOidcStartResponse> {
  return request<SetupOidcStartResponse>('/v1/setup/owner/start-oidc', {
    method: 'POST',
    headers: authHeaders(sessionToken),
    body: JSON.stringify({ redirect_uri: redirectUri }),
  })
}

export function setupOidcVerify(
  sessionToken: string,
  code: string,
  state: string,
): Promise<SetupOidcVerifyResponse> {
  return request<SetupOidcVerifyResponse>('/v1/setup/owner/verify-oidc', {
    method: 'POST',
    headers: authHeaders(sessionToken),
    body: JSON.stringify({ code, state }),
  })
}

export function completeSetup(
  sessionToken: string,
): Promise<SetupCompleteResponse> {
  return request<SetupCompleteResponse>('/v1/setup/complete', {
    method: 'POST',
    headers: authHeaders(sessionToken),
  })
}
