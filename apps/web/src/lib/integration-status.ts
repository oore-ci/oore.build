import type { Integration } from '@oore/client/models'

// The daemon reports why a source is in error, but the published
// @oore/client models predate these fields. Drop this once the client is
// regenerated.
export type IntegrationWithStatus = Integration & {
  status_reason?: string | null
  status_checked_at?: number | null
}

export function integrationStatusReason(
  integration: IntegrationWithStatus,
): string | null {
  if (integration.status !== 'error') return null
  switch (integration.status_reason) {
    case 'expired':
      return 'The saved access token expired.'
    case 'rejected':
      return 'The provider rejected the saved access token. It may have been revoked.'
    case 'refresh_failed':
      return 'Oore could not refresh the authorization with the provider.'
    default:
      return 'Oore could not use the saved credentials for this source.'
  }
}
