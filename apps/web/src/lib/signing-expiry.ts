import type { SigningIdentity } from '@oore/client/models'

const DAY = 86_400

/** How close a certificate or profile is to expiring, or null when far off. */
export type ExpiryBand = 'expired' | 'week' | 'month'

export function expiryBand(
  expiresAt: number,
  nowSeconds: number,
): ExpiryBand | null {
  const left = expiresAt - nowSeconds
  if (left <= 0) return 'expired'
  if (left <= 7 * DAY) return 'week'
  if (left <= 30 * DAY) return 'month'
  return null
}

export interface SigningExpiry {
  key: string
  projectId: string
  identityName: string
  /** "Distribution certificate" or "Profile for com.example.app". */
  what: string
  expiresAt: number
  band: ExpiryBand
}

/**
 * Certificates and provisioning profiles that expire within 30 days or
 * already have, soonest first.
 */
export function signingExpiries(
  identities: ReadonlyArray<SigningIdentity>,
  nowSeconds: number,
): Array<SigningExpiry> {
  const found: Array<SigningExpiry> = []
  for (const identity of identities) {
    const dates = [
      ...(identity.expires_at
        ? [
            {
              key: `${identity.id}:certificate`,
              what: 'Distribution certificate',
              expiresAt: identity.expires_at,
            },
          ]
        : []),
      ...(identity.profiles ?? []).flatMap((profile) =>
        profile.expires_at
          ? [
              {
                key: `${identity.id}:profile:${profile.bundle_id}`,
                what: `Profile for ${profile.bundle_id}`,
                expiresAt: profile.expires_at,
              },
            ]
          : [],
      ),
    ]
    for (const date of dates) {
      const band = expiryBand(date.expiresAt, nowSeconds)
      if (!band) continue
      found.push({
        ...date,
        band,
        projectId: identity.project_id,
        identityName: identity.name,
      })
    }
  }
  return found.sort((a, b) => a.expiresAt - b.expiresAt)
}
