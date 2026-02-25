import type { UserRole } from '@/lib/types'
import { createSelectorStore } from '@/stores/store-utils'

interface AuthUser {
  email: string
  oidc_subject: string
  user_id: string
  role: UserRole
  avatar_url?: string
}

interface AuthStoreState {
  instanceId: string | null
  token: string | null
  expiresAt: number | null
  user: AuthUser | null
  setInstanceContext: (instanceId: string | null) => void
  setAuth: (
    token: string,
    expiresAt: number,
    user: AuthUser,
    method?: LastAuthMethod,
  ) => void
  clearAuth: () => void
}

export type LastAuthMethod = 'oidc' | 'local' | 'trusted_proxy'

export interface LastAuthMeta {
  method: LastAuthMethod
  at: number
}

function tokenKey(instanceId: string | null): string {
  return instanceId ? `oore_auth_token_${instanceId}` : 'oore_auth_token'
}

function expiresKey(instanceId: string | null): string {
  return instanceId ? `oore_auth_expires_${instanceId}` : 'oore_auth_expires'
}

function userKey(instanceId: string | null): string {
  return instanceId ? `oore_auth_user_${instanceId}` : 'oore_auth_user'
}

function lastMethodKey(instanceId: string | null): string {
  return instanceId
    ? `oore_auth_last_method_${instanceId}`
    : 'oore_auth_last_method'
}

function lastAtKey(instanceId: string | null): string {
  return instanceId ? `oore_auth_last_at_${instanceId}` : 'oore_auth_last_at'
}

function loadToken(instanceId: string | null): string | null {
  try {
    return localStorage.getItem(tokenKey(instanceId)) ?? null
  } catch {
    return null
  }
}

function loadExpiresAt(instanceId: string | null): number | null {
  try {
    const value = localStorage.getItem(expiresKey(instanceId))
    return value ? Number(value) : null
  } catch {
    return null
  }
}

function loadUser(instanceId: string | null): AuthUser | null {
  try {
    const value = localStorage.getItem(userKey(instanceId))
    if (!value) return null
    return JSON.parse(value) as AuthUser
  } catch {
    return null
  }
}

function saveAuth(
  instanceId: string | null,
  token: string | null,
  expiresAt: number | null,
  user: AuthUser | null,
): void {
  try {
    if (token) {
      localStorage.setItem(tokenKey(instanceId), token)
    } else {
      localStorage.removeItem(tokenKey(instanceId))
    }

    if (expiresAt != null) {
      localStorage.setItem(expiresKey(instanceId), String(expiresAt))
    } else {
      localStorage.removeItem(expiresKey(instanceId))
    }

    if (user) {
      localStorage.setItem(userKey(instanceId), JSON.stringify(user))
    } else {
      localStorage.removeItem(userKey(instanceId))
    }
  } catch {
    // localStorage unavailable
  }
}

function saveLastAuthMeta(
  instanceId: string | null,
  method: LastAuthMethod,
  at: number,
): void {
  try {
    localStorage.setItem(lastMethodKey(instanceId), method)
    localStorage.setItem(lastAtKey(instanceId), String(at))
  } catch {
    // localStorage unavailable
  }
}

export function clearAuthStorageForInstance(instanceId: string): void {
  try {
    localStorage.removeItem(tokenKey(instanceId))
    localStorage.removeItem(expiresKey(instanceId))
    localStorage.removeItem(userKey(instanceId))
  } catch {
    // localStorage unavailable
  }
}

export function getLastAuthMetaForInstance(
  instanceId: string | null,
): LastAuthMeta | null {
  if (!instanceId) return null

  try {
    const method = localStorage.getItem(lastMethodKey(instanceId))
    const atRaw = localStorage.getItem(lastAtKey(instanceId))
    if (
      (method !== 'oidc' && method !== 'local' && method !== 'trusted_proxy') ||
      !atRaw
    )
      return null

    const at = Number(atRaw)
    if (!Number.isFinite(at) || at <= 0) return null
    return { method, at }
  } catch {
    return null
  }
}

export function isAuthenticated(): boolean {
  const { token, expiresAt } = useAuthStore.getState()
  if (!token || expiresAt == null) return false
  return expiresAt > Math.floor(Date.now() / 1000)
}

export const useAuthStore = createSelectorStore<AuthStoreState>((set, get) => ({
  instanceId: null,
  token: loadToken(null),
  expiresAt: loadExpiresAt(null),
  user: loadUser(null),

  setInstanceContext: (instanceId) => {
    set({
      instanceId,
      token: loadToken(instanceId),
      expiresAt: loadExpiresAt(instanceId),
      user: loadUser(instanceId),
    })
  },

  setAuth: (token, expiresAt, user, method = 'oidc') => {
    const { instanceId } = get()
    saveLastAuthMeta(instanceId, method, Math.floor(Date.now() / 1000))
    saveAuth(instanceId, token, expiresAt, user)
    set({ token, expiresAt, user })
  },

  clearAuth: () => {
    const { instanceId } = get()
    saveAuth(instanceId, null, null, null)
    set({ token: null, expiresAt: null, user: null })
  },
}))
