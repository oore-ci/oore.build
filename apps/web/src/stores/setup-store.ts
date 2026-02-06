import { create } from 'zustand'

interface SetupStoreState {
  currentStep: number
  sessionToken: string | null
  sessionExpiresAt: number | null
  setCurrentStep: (step: number) => void
  setSessionToken: (token: string | null) => void
  setSessionExpiresAt: (expiresAt: number | null) => void
  reset: () => void
}

function loadSessionToken(): string | null {
  try {
    return sessionStorage.getItem('oore_setup_session') ?? null
  } catch {
    return null
  }
}

function saveSessionToken(token: string | null): void {
  try {
    if (token) {
      sessionStorage.setItem('oore_setup_session', token)
    } else {
      sessionStorage.removeItem('oore_setup_session')
    }
  } catch {
    // sessionStorage unavailable
  }
}

function loadSessionExpiresAt(): number | null {
  try {
    const val = sessionStorage.getItem('oore_setup_session_expires')
    return val ? Number(val) : null
  } catch {
    return null
  }
}

function saveSessionExpiresAt(expiresAt: number | null): void {
  try {
    if (expiresAt != null) {
      sessionStorage.setItem('oore_setup_session_expires', String(expiresAt))
    } else {
      sessionStorage.removeItem('oore_setup_session_expires')
    }
  } catch {
    // sessionStorage unavailable
  }
}

export const useSetupStore = create<SetupStoreState>((set) => ({
  currentStep: 0,
  sessionToken: loadSessionToken(),
  sessionExpiresAt: loadSessionExpiresAt(),
  setCurrentStep: (step) => set({ currentStep: step }),
  setSessionToken: (token) => {
    saveSessionToken(token)
    set({ sessionToken: token })
  },
  setSessionExpiresAt: (expiresAt) => {
    saveSessionExpiresAt(expiresAt)
    set({ sessionExpiresAt: expiresAt })
  },
  reset: () => {
    saveSessionToken(null)
    saveSessionExpiresAt(null)
    set({ currentStep: 0, sessionToken: null, sessionExpiresAt: null })
  },
}))
