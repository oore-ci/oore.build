export interface OidcCallbackPrecheck {
  ok: boolean
  target: '/login' | '/setup/owner'
  flow: 'auth' | 'setup_owner'
  code?: string
  state?: string
  message?: string
  hint?: string
}

export function precheckOidcCallback(
  params: URLSearchParams,
  storedState: string | null,
  flow: string | null,
): OidcCallbackPrecheck {
  const normalizedFlow = flow === 'setup_owner' ? 'setup_owner' : 'auth'
  const target = normalizedFlow === 'setup_owner' ? '/setup/owner' : '/login'

  const providerError = params.get('error')
  const providerErrorDescription = params.get('error_description')
  if (providerError) {
    return {
      ok: false,
      target,
      flow: normalizedFlow,
      message: providerErrorDescription
        ? `Identity provider returned an error: ${providerErrorDescription}`
        : `Identity provider returned an error: ${providerError}`,
      hint: providerError,
    }
  }

  const code = params.get('code')
  const state = params.get('state')
  if (!code || !state) {
    return {
      ok: false,
      target,
      flow: normalizedFlow,
      message: 'Missing authorization code or state parameter in callback URL.',
      hint: 'missing_callback_params',
    }
  }

  if (!storedState) {
    return {
      ok: false,
      target,
      flow: normalizedFlow,
      message: 'Missing OIDC session state. Restart sign-in from the app.',
      hint: 'missing_stored_state',
    }
  }

  if (storedState !== state) {
    return {
      ok: false,
      target,
      flow: normalizedFlow,
      message: 'OIDC state mismatch. Please try again.',
      hint: 'state_mismatch',
    }
  }

  return {
    ok: true,
    target,
    flow: normalizedFlow,
    code,
    state,
  }
}
