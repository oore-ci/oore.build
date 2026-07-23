import { create } from 'zustand'

interface SetupStoreState {
  instanceId: string | null
  currentStep: number
  sessionToken: string | null
  sessionExpiresAt: number | null
  setInstanceContext: (instanceId: string | null) => void
  setCurrentStep: (step: number) => void
  setSessionToken: (token: string | null) => void
  setSessionExpiresAt: (expiresAt: number | null) => void
  reset: () => void
}

function tokenKey(instanceId: string | null): string {
  return instanceId ? `oore_setup_session_${instanceId}` : 'oore_setup_session'
}

function expiresKey(instanceId: string | null): string {
  return instanceId
    ? `oore_setup_session_expires_${instanceId}`
    : 'oore_setup_session_expires'
}

function loadSessionToken(instanceId: string | null): string | null {
  try {
    return sessionStorage.getItem(tokenKey(instanceId)) ?? null
  } catch {
    return null
  }
}

function saveSessionToken(
  instanceId: string | null,
  token: string | null,
): void {
  try {
    if (token) {
      sessionStorage.setItem(tokenKey(instanceId), token)
    } else {
      sessionStorage.removeItem(tokenKey(instanceId))
    }
  } catch {
    // sessionStorage unavailable
  }
}

function loadSessionExpiresAt(instanceId: string | null): number | null {
  try {
    const val = sessionStorage.getItem(expiresKey(instanceId))
    return val ? Number(val) : null
  } catch {
    return null
  }
}

function saveSessionExpiresAt(
  instanceId: string | null,
  expiresAt: number | null,
): void {
  try {
    if (expiresAt != null) {
      sessionStorage.setItem(expiresKey(instanceId), String(expiresAt))
    } else {
      sessionStorage.removeItem(expiresKey(instanceId))
    }
  } catch {
    // sessionStorage unavailable
  }
}

export const useSetupStore = create<SetupStoreState>()((set, get) => ({
  instanceId: null,
  currentStep: 0,
  sessionToken: loadSessionToken(null),
  sessionExpiresAt: loadSessionExpiresAt(null),

  setInstanceContext: (instanceId) => {
    set({
      instanceId,
      sessionToken: loadSessionToken(instanceId),
      sessionExpiresAt: loadSessionExpiresAt(instanceId),
    })
  },

  setCurrentStep: (step) => set({ currentStep: step }),

  setSessionToken: (token) => {
    const { instanceId } = get()
    saveSessionToken(instanceId, token)
    set({ sessionToken: token })
  },

  setSessionExpiresAt: (expiresAt) => {
    const { instanceId } = get()
    saveSessionExpiresAt(instanceId, expiresAt)
    set({ sessionExpiresAt: expiresAt })
  },

  reset: () => {
    const { instanceId } = get()
    saveSessionToken(instanceId, null)
    saveSessionExpiresAt(instanceId, null)
    set({ currentStep: 0, sessionToken: null, sessionExpiresAt: null })
  },
}))
