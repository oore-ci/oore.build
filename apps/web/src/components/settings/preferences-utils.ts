import type { RemoteAuthMode } from '@/lib/types'

export function guidanceForPreflight(
  checkId: string,
  failureCode?: string,
): string {
  if (failureCode === 'external_access_public_url_missing')
    return 'Set a non-loopback HTTPS Public URL in External Access network settings.'
  if (failureCode === 'external_access_https_required')
    return 'Public URL must use HTTPS before enabling External Access.'
  if (failureCode === 'external_access_origin_not_allowed')
    return 'Add the Public URL origin to allowed origins in External Access network settings.'
  if (checkId === 'setup_ready')
    return 'Finish setup until the instance reaches ready state.'
  if (checkId === 'oidc_configured')
    return 'Configure OIDC and verify runtime auth settings.'
  if (checkId === 'trusted_proxy_configured')
    return 'Configure Trusted Proxy identity settings and shared secret.'
  if (checkId === 'redirect_policy_consistent')
    return 'Ensure redirect URI policy matches your configured public origin.'
  return 'Resolve this check before enabling External Access.'
}

export function authModeLabel(mode: RemoteAuthMode | undefined): string {
  return mode === 'trusted_proxy' ? 'Trusted Proxy' : 'OIDC'
}

export function authModeDescription(mode: RemoteAuthMode | undefined): string {
  return mode === 'trusted_proxy'
    ? 'Sign-in is delegated to a trusted upstream proxy.'
    : 'Sign-in uses your configured OIDC provider.'
}

export function runtimeUpdateActive(phase?: string): boolean {
  return phase === 'updating' || phase === 'restarting'
}
