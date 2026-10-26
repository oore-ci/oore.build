import { resolveInstanceApiBaseUrl } from '@/lib/instance-url'
import { createWebOoreClient } from '@/lib/api-client/client'
import { useAuthStore } from '@/stores/auth-store'
import { useActiveInstance } from '@/stores/instance-store'
import { useTime } from './use-time'
import { useShallow } from 'zustand/react/shallow'

export function useApiContext() {
  const instance = useActiveInstance()
  const [token, expiresAt] = useAuthStore(
    useShallow((state) => [state.token, state.expiresAt]),
  )
  const validToken = useTime((time) =>
    !token ||
    expiresAt == null ||
    !Number.isFinite(expiresAt) ||
    expiresAt <= Math.floor(time / 1000)
      ? null
      : token,
  )
  const baseUrl = resolveInstanceApiBaseUrl(instance)

  return {
    baseUrl,
    client: createWebOoreClient({
      baseUrl: baseUrl ?? 'http://127.0.0.1',
      token: validToken ?? undefined,
    }),
    instance,
    instanceId: instance?.id ?? '__none__',
    token: validToken,
  }
}
