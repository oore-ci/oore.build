import { resolveInstanceApiBaseUrl } from '@/lib/instance-url'
import { useAuthStore } from '@/stores/auth-store'
import { useActiveInstance } from '@/stores/instance-store'

export function useApiContext() {
  const instance = useActiveInstance()
  const token = useAuthStore((state) => state.token)
  const expiresAt = useAuthStore((state) => state.expiresAt)
  const validToken =
    !token ||
    expiresAt == null ||
    !Number.isFinite(expiresAt) ||
    expiresAt <= Math.floor(Date.now() / 1000)
      ? null
      : token

  return {
    baseUrl: resolveInstanceApiBaseUrl(instance),
    instance,
    token: validToken,
  }
}
