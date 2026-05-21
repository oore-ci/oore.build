export type LoginFlowStatus = {
  runtime_mode: 'local' | 'remote'
  remote_auth_mode: 'oidc' | 'trusted_proxy'
}

export type LoginFlow = 'local' | 'trusted_proxy' | 'oidc'

export function resolveLoginFlow(
  status: LoginFlowStatus,
  canUseLoopbackLocalLogin: boolean,
): LoginFlow {
  if (status.runtime_mode === 'local') {
    return 'local'
  }

  if (status.remote_auth_mode === 'trusted_proxy') {
    return 'trusted_proxy'
  }

  if (canUseLoopbackLocalLogin) {
    return 'local'
  }

  return 'oidc'
}
