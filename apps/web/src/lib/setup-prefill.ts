export type TrustedProxySetupPreset = 'generic' | 'warpgate' | 'custom'

export interface TrustedProxySetupPrefill {
  ownerEmail?: string
  proxyPreset?: TrustedProxySetupPreset
  userEmailHeader?: string
}

function keyForInstance(instanceId: string): string {
  return `oore_setup_trusted_proxy_prefill_${instanceId}`
}

export function normalizeTrustedProxySetupPreset(
  value: string | null,
): TrustedProxySetupPreset | undefined {
  if (value === 'generic' || value === 'warpgate' || value === 'custom') {
    return value
  }
  return undefined
}

export function saveTrustedProxySetupPrefill(
  instanceId: string,
  prefill: TrustedProxySetupPrefill,
): void {
  const normalized: TrustedProxySetupPrefill = {}
  const ownerEmail = prefill.ownerEmail?.trim().toLowerCase()
  const userEmailHeader = prefill.userEmailHeader?.trim().toLowerCase()

  if (ownerEmail) normalized.ownerEmail = ownerEmail
  if (prefill.proxyPreset) normalized.proxyPreset = prefill.proxyPreset
  if (userEmailHeader) normalized.userEmailHeader = userEmailHeader

  if (Object.keys(normalized).length === 0) return

  try {
    sessionStorage.setItem(
      keyForInstance(instanceId),
      JSON.stringify(normalized),
    )
  } catch {
    // sessionStorage unavailable
  }
}

export function loadTrustedProxySetupPrefill(
  instanceId: string | null,
): TrustedProxySetupPrefill | null {
  if (!instanceId) return null

  try {
    const raw = sessionStorage.getItem(keyForInstance(instanceId))
    if (!raw) return null

    const parsed = JSON.parse(raw) as TrustedProxySetupPrefill
    return {
      ownerEmail: parsed.ownerEmail,
      proxyPreset: normalizeTrustedProxySetupPreset(parsed.proxyPreset ?? null),
      userEmailHeader: parsed.userEmailHeader,
    }
  } catch {
    return null
  }
}
