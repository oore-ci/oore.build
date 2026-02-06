import { create } from 'zustand'
import type { UserRole } from '@/lib/types'

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
  setAuth: (token: string, expiresAt: number, user: AuthUser) => void
  clearAuth: () => void
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

function loadToken(instanceId: string | null): string | null {
  try {
    return localStorage.getItem(tokenKey(instanceId)) ?? null
  } catch {
    return null
  }
}

function loadExpiresAt(instanceId: string | null): number | null {
  try {
    const val = localStorage.getItem(expiresKey(instanceId))
    return val ? Number(val) : null
  } catch {
    return null
  }
}

function loadUser(instanceId: string | null): AuthUser | null {
  try {
    const val = localStorage.getItem(userKey(instanceId))
    if (!val) return null
    return JSON.parse(val) as AuthUser
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

export function clearAuthStorageForInstance(instanceId: string): void {
  try {
    localStorage.removeItem(tokenKey(instanceId))
    localStorage.removeItem(expiresKey(instanceId))
    localStorage.removeItem(userKey(instanceId))
  } catch {
    // localStorage unavailable
  }
}

/** Returns true only when we have a non-expired token. */
export function isAuthenticated(): boolean {
  const { token, expiresAt } = useAuthStore.getState()
  if (!token || expiresAt == null) return false
  return expiresAt > Math.floor(Date.now() / 1000)
}

export const useAuthStore = create<AuthStoreState>((set, get) => ({
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

  setAuth: (token, expiresAt, user) => {
    const { instanceId } = get()
    saveAuth(instanceId, token, expiresAt, user)
    set({ token, expiresAt, user })
  },

  clearAuth: () => {
    const { instanceId } = get()
    saveAuth(instanceId, null, null, null)
    set({ token: null, expiresAt: null, user: null })
  },
}))
