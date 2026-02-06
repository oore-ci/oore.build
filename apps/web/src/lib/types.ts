// ── Setup state machine ─────────────────────────────────────────

export type SetupState =
  | 'uninitialized'
  | 'bootstrap_pending'
  | 'idp_configured'
  | 'owner_created'
  | 'ready'

// ── Public setup status (non-sensitive) ─────────────────────────

export interface SetupStatus {
  instance_id: string
  state: SetupState
  setup_mode: boolean
  is_configured: boolean
}

// ── API request/response types ──────────────────────────────────

export interface BootstrapTokenVerifyResponse {
  session_token: string
  expires_at: number
}

export interface OidcConfigureRequest {
  issuer_url: string
  client_id: string
  client_secret?: string
}

export interface OidcConfigureResponse {
  state: SetupState
  discovered_issuer: string
  session_expires_at?: number
}

export interface SetupOidcStartRequest {
  redirect_uri: string
}

export interface SetupOidcStartResponse {
  authorization_url: string
  state: string
}

export interface SetupOidcVerifyRequest {
  code: string
  state: string
}

export interface SetupOidcVerifyResponse {
  state: SetupState
  owner_email: string
  oidc_subject: string
  session_expires_at?: number
}

export interface SetupCompleteResponse {
  state: SetupState
  instance_id: string
}

// ── Structured API error ────────────────────────────────────────

export interface ApiError {
  error: string
  code: string
  details?: string
}
