import { redirect } from '@tanstack/solid-router'
import type { Instance } from '@/lib/types'
import { useAuthStore } from '@/stores/auth-store'
import { useInstanceStore } from '@/stores/instance-store'
import { useSetupStore } from '@/stores/setup-store'

function readActiveInstanceFromStorage(): Instance | null {
  try {
    const raw = localStorage.getItem('oore_instances')
    if (!raw) return null
    const parsed = JSON.parse(raw) as {
      state?: {
        instances?: Record<string, Instance>
        activeInstanceId?: string | null
      }
    }
    const state = parsed.state
    if (!state?.activeInstanceId || !state.instances?.[state.activeInstanceId]) {
      return null
    }
    return state.instances[state.activeInstanceId]
  } catch {
    return null
  }
}

export function getActiveInstanceOrRedirect(): Instance {
  const { activeInstanceId, instances } = useInstanceStore.getState()
  if (activeInstanceId && instances[activeInstanceId]) {
    return instances[activeInstanceId]
  }

  const fromStorage = readActiveInstanceFromStorage()
  if (fromStorage) return fromStorage

  throw redirect({ to: '/' })
}

export function syncSetupStoreContext(instanceId: string): void {
  const current = useSetupStore.getState().instanceId
  if (current !== instanceId) {
    useSetupStore.getState().setInstanceContext(instanceId)
  }
}

export function getSetupSessionTokenForInstance(instanceId: string): string | null {
  try {
    return sessionStorage.getItem(`oore_setup_session_${instanceId}`) ?? null
  } catch {
    return null
  }
}

export function requireSetupSessionOrRedirect(instanceId: string): string {
  const token = getSetupSessionTokenForInstance(instanceId)
  if (!token) {
    throw redirect({ to: '/setup' })
  }
  return token
}

export function getAuthTokenForInstance(instanceId: string): string | null {
  try {
    return localStorage.getItem(`oore_auth_token_${instanceId}`) ?? null
  } catch {
    return null
  }
}

function getAuthExpiresForInstance(instanceId: string): number | null {
  try {
    const value = localStorage.getItem(`oore_auth_expires_${instanceId}`)
    return value ? Number(value) : null
  } catch {
    return null
  }
}

export function requireAuthOrRedirect(instanceId: string): string {
  const token = getAuthTokenForInstance(instanceId)
  if (!token) {
    throw redirect({ to: '/login' })
  }

  const expiresAt = getAuthExpiresForInstance(instanceId)
  if (expiresAt != null && expiresAt <= Math.floor(Date.now() / 1000)) {
    throw redirect({ to: '/login' })
  }

  const current = useAuthStore.getState().instanceId
  if (current !== instanceId) {
    useAuthStore.getState().setInstanceContext(instanceId)
  }

  return token
}
