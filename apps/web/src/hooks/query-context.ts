import { createMemo, type Accessor } from 'solid-js'
import { useActiveInstance } from '@/stores/instance-store'
import { useAuthStore } from '@/stores/auth-store'

export function useAuthToken(): Accessor<string | null> {
  const token = useAuthStore((state) => state.token)
  const expiresAt = useAuthStore((state) => state.expiresAt)
  return createMemo(() => {
    const nextToken = token()
    const expiry = expiresAt()
    if (!nextToken || expiry == null) return null
    if (expiry <= Math.floor(Date.now() / 1000)) return null
    return nextToken
  })
}

export function useBaseUrl(): Accessor<string | null> {
  const instance = useActiveInstance()
  return createMemo(() => instance()?.url ?? null)
}

export function useInstanceQueryPrefix(): Accessor<string> {
  const instance = useActiveInstance()
  return createMemo(() => instance()?.id ?? '__none__')
}
