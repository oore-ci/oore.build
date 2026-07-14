import type { Instance } from '@/lib/types'

type InstanceUrl = Pick<Instance, 'url'> | null | undefined

export function resolveInstanceApiBaseUrl(
  instance: InstanceUrl,
): string | null {
  const rawUrl = instance?.url.trim() ?? ''

  if (!rawUrl || rawUrl === 'local') {
    if (typeof window === 'undefined') return null
    return window.location.origin
  }

  return rawUrl.replace(/\/+$/, '')
}

export function resolveRequiredInstanceApiBaseUrl(
  instance: InstanceUrl,
): string {
  const baseUrl = resolveInstanceApiBaseUrl(instance)
  if (!baseUrl) {
    throw new Error('No active instance. Select or add an instance first.')
  }
  return baseUrl
}
