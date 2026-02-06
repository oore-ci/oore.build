import { describe, it, expect, beforeEach } from 'vitest'
import { useSetupStore } from '@/stores/setup-store'

// ── sessionStorage mock (jsdom provides one, but reset between tests) ──

beforeEach(() => {
  sessionStorage.clear()
  useSetupStore.getState().reset()
})

describe('useSetupStore', () => {
  it('has correct initial state after reset', () => {
    const state = useSetupStore.getState()
    expect(state.currentStep).toBe(0)
    expect(state.sessionToken).toBeNull()
    expect(state.sessionExpiresAt).toBeNull()
  })

  it('setCurrentStep updates step', () => {
    useSetupStore.getState().setCurrentStep(3)
    expect(useSetupStore.getState().currentStep).toBe(3)
  })

  it('setSessionToken updates token and persists to sessionStorage', () => {
    useSetupStore.getState().setSessionToken('abc-123')
    expect(useSetupStore.getState().sessionToken).toBe('abc-123')
    expect(sessionStorage.getItem('oore_setup_session')).toBe('abc-123')
  })

  it('setSessionToken(null) removes from sessionStorage', () => {
    useSetupStore.getState().setSessionToken('temp')
    useSetupStore.getState().setSessionToken(null)
    expect(useSetupStore.getState().sessionToken).toBeNull()
    expect(sessionStorage.getItem('oore_setup_session')).toBeNull()
  })

  it('setSessionExpiresAt updates expiry and persists', () => {
    useSetupStore.getState().setSessionExpiresAt(1700000000)
    expect(useSetupStore.getState().sessionExpiresAt).toBe(1700000000)
    expect(sessionStorage.getItem('oore_setup_session_expires')).toBe(
      '1700000000',
    )
  })

  it('setSessionExpiresAt(null) removes from sessionStorage', () => {
    useSetupStore.getState().setSessionExpiresAt(123)
    useSetupStore.getState().setSessionExpiresAt(null)
    expect(useSetupStore.getState().sessionExpiresAt).toBeNull()
    expect(sessionStorage.getItem('oore_setup_session_expires')).toBeNull()
  })

  it('reset clears all state and sessionStorage', () => {
    useSetupStore.getState().setCurrentStep(5)
    useSetupStore.getState().setSessionToken('tok')
    useSetupStore.getState().setSessionExpiresAt(999)

    useSetupStore.getState().reset()

    const state = useSetupStore.getState()
    expect(state.currentStep).toBe(0)
    expect(state.sessionToken).toBeNull()
    expect(state.sessionExpiresAt).toBeNull()
    expect(sessionStorage.getItem('oore_setup_session')).toBeNull()
    expect(sessionStorage.getItem('oore_setup_session_expires')).toBeNull()
  })
})
