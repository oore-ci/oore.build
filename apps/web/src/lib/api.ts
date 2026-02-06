import type {
  ApiError,
  BootstrapTokenVerifyResponse,
  InviteUserRequest,
  InviteUserResponse,
  ListUsersResponse,
  LogoutResponse,
  OidcConfigureRequest,
  OidcConfigureResponse,
  ReEnableUserResponse,
  SetupCompleteResponse,
  SetupOidcStartResponse,
  SetupOidcVerifyResponse,
  SetupStatus,
  UpdateUserRoleRequest,
  UpdateUserRoleResponse,
  UserProfileResponse,
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

async function request<T>(
  baseUrl: string,
  path: string,
  options: RequestInit = {},
): Promise<T> {
  const res = await fetch(`${baseUrl}${path}`, {
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

export function getSetupStatus(baseUrl: string): Promise<SetupStatus> {
  return request<SetupStatus>(baseUrl, '/v1/public/setup-status')
}

export function verifyBootstrapToken(
  baseUrl: string,
  token: string,
): Promise<BootstrapTokenVerifyResponse> {
  return request<BootstrapTokenVerifyResponse>(
    baseUrl,
    '/v1/setup/bootstrap-token/verify',
    {
      method: 'POST',
      body: JSON.stringify({ token }),
    },
  )
}

export function configureOidc(
  baseUrl: string,
  sessionToken: string,
  data: OidcConfigureRequest,
): Promise<OidcConfigureResponse> {
  return request<OidcConfigureResponse>(baseUrl, '/v1/setup/oidc/configure', {
    method: 'POST',
    headers: authHeaders(sessionToken),
    body: JSON.stringify(data),
  })
}

export function setupOidcStart(
  baseUrl: string,
  sessionToken: string,
  redirectUri: string,
): Promise<SetupOidcStartResponse> {
  return request<SetupOidcStartResponse>(
    baseUrl,
    '/v1/setup/owner/start-oidc',
    {
      method: 'POST',
      headers: authHeaders(sessionToken),
      body: JSON.stringify({ redirect_uri: redirectUri }),
    },
  )
}

export function setupOidcVerify(
  baseUrl: string,
  sessionToken: string,
  code: string,
  state: string,
): Promise<SetupOidcVerifyResponse> {
  return request<SetupOidcVerifyResponse>(
    baseUrl,
    '/v1/setup/owner/verify-oidc',
    {
      method: 'POST',
      headers: authHeaders(sessionToken),
      body: JSON.stringify({ code, state }),
    },
  )
}

export function completeSetup(
  baseUrl: string,
  sessionToken: string,
): Promise<SetupCompleteResponse> {
  return request<SetupCompleteResponse>(baseUrl, '/v1/setup/complete', {
    method: 'POST',
    headers: authHeaders(sessionToken),
  })
}

// ── User management API ─────────────────────────────────────────

export function getMe(
  baseUrl: string,
  token: string,
): Promise<UserProfileResponse> {
  return request<UserProfileResponse>(baseUrl, '/v1/users/me', {
    headers: authHeaders(token),
  })
}

export function listUsers(
  baseUrl: string,
  token: string,
): Promise<ListUsersResponse> {
  return request<ListUsersResponse>(baseUrl, '/v1/users', {
    headers: authHeaders(token),
  })
}

export function inviteUser(
  baseUrl: string,
  token: string,
  data: InviteUserRequest,
): Promise<InviteUserResponse> {
  return request<InviteUserResponse>(baseUrl, '/v1/users/invite', {
    method: 'POST',
    headers: authHeaders(token),
    body: JSON.stringify(data),
  })
}

export function updateUserRole(
  baseUrl: string,
  token: string,
  userId: string,
  data: UpdateUserRoleRequest,
): Promise<UpdateUserRoleResponse> {
  return request<UpdateUserRoleResponse>(baseUrl, `/v1/users/${userId}/role`, {
    method: 'PATCH',
    headers: authHeaders(token),
    body: JSON.stringify(data),
  })
}

export function reEnableUser(
  baseUrl: string,
  token: string,
  userId: string,
): Promise<ReEnableUserResponse> {
  return request<ReEnableUserResponse>(baseUrl, `/v1/users/${userId}/enable`, {
    method: 'POST',
    headers: authHeaders(token),
  })
}

export function deleteUser(
  baseUrl: string,
  token: string,
  userId: string,
): Promise<{ ok: boolean }> {
  return request<{ ok: boolean }>(baseUrl, `/v1/users/${userId}`, {
    method: 'DELETE',
    headers: authHeaders(token),
  })
}

export function logout(
  baseUrl: string,
  token: string,
): Promise<LogoutResponse> {
  return request<LogoutResponse>(baseUrl, '/v1/auth/logout', {
    method: 'POST',
    headers: authHeaders(token),
  })
}
