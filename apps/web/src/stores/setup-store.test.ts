import { beforeEach, describe, expect, it } from 'vitest'
import { useSetupStore } from '@/stores/setup-store'

// ── sessionStorage mock (jsdom provides one, but reset between tests) ──

beforeEach(() => {
  sessionStorage.clear()
  useSetupStore.setState({
    instanceId: null,
    currentStep: 0,
    sessionToken: null,
    sessionExpiresAt: null,
  })
})

describe('useSetupStore', () => {
  it('has correct initial state after reset', () => {
    const state = useSetupStore.getState()
    expect(state.instanceId).toBeNull()
    expect(state.currentStep).toBe(0)
    expect(state.sessionToken).toBeNull()
    expect(state.sessionExpiresAt).toBeNull()
  })

  it('setCurrentStep updates step', () => {
    useSetupStore.getState().setCurrentStep(3)
    expect(useSetupStore.getState().currentStep).toBe(3)
  })

  // ── Without instance context (legacy-compatible) ─────────────

  it('setSessionToken persists to non-namespaced key when no instance', () => {
    useSetupStore.getState().setSessionToken('abc-123')
    expect(useSetupStore.getState().sessionToken).toBe('abc-123')
    expect(sessionStorage.getItem('oore_setup_session')).toBe('abc-123')
  })

  it('setSessionToken(null) removes non-namespaced key', () => {
    useSetupStore.getState().setSessionToken('temp')
    useSetupStore.getState().setSessionToken(null)
    expect(useSetupStore.getState().sessionToken).toBeNull()
    expect(sessionStorage.getItem('oore_setup_session')).toBeNull()
  })

  it('setSessionExpiresAt persists to non-namespaced key when no instance', () => {
    useSetupStore.getState().setSessionExpiresAt(1700000000)
    expect(useSetupStore.getState().sessionExpiresAt).toBe(1700000000)
    expect(sessionStorage.getItem('oore_setup_session_expires')).toBe(
      '1700000000',
    )
  })

  it('setSessionExpiresAt(null) removes non-namespaced key', () => {
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

  // ── With instance context ────────────────────────────────────

  it('setInstanceContext hydrates token from namespaced sessionStorage', () => {
    const id = 'inst-abc'
    sessionStorage.setItem(`oore_setup_session_${id}`, 'tok-abc')
    sessionStorage.setItem(`oore_setup_session_expires_${id}`, '9999')

    useSetupStore.getState().setInstanceContext(id)

    const state = useSetupStore.getState()
    expect(state.instanceId).toBe(id)
    expect(state.sessionToken).toBe('tok-abc')
    expect(state.sessionExpiresAt).toBe(9999)
  })

  it('setInstanceContext returns null when no stored data', () => {
    useSetupStore.getState().setInstanceContext('inst-new')
    const state = useSetupStore.getState()
    expect(state.instanceId).toBe('inst-new')
    expect(state.sessionToken).toBeNull()
    expect(state.sessionExpiresAt).toBeNull()
  })

  it('setSessionToken persists to namespaced key when instance is set', () => {
    const id = 'inst-xyz'
    useSetupStore.getState().setInstanceContext(id)
    useSetupStore.getState().setSessionToken('tok-xyz')

    expect(sessionStorage.getItem(`oore_setup_session_${id}`)).toBe('tok-xyz')
    // Non-namespaced key should not be set
    expect(sessionStorage.getItem('oore_setup_session')).toBeNull()
  })

  it('setSessionExpiresAt persists to namespaced key when instance is set', () => {
    const id = 'inst-xyz'
    useSetupStore.getState().setInstanceContext(id)
    useSetupStore.getState().setSessionExpiresAt(5555)

    expect(sessionStorage.getItem(`oore_setup_session_expires_${id}`)).toBe(
      '5555',
    )
    expect(sessionStorage.getItem('oore_setup_session_expires')).toBeNull()
  })

  it('reset clears namespaced keys when instance is set', () => {
    const id = 'inst-reset'
    useSetupStore.getState().setInstanceContext(id)
    useSetupStore.getState().setSessionToken('tok')
    useSetupStore.getState().setSessionExpiresAt(1234)

    useSetupStore.getState().reset()

    expect(sessionStorage.getItem(`oore_setup_session_${id}`)).toBeNull()
    expect(
      sessionStorage.getItem(`oore_setup_session_expires_${id}`),
    ).toBeNull()
    expect(useSetupStore.getState().sessionToken).toBeNull()
    expect(useSetupStore.getState().sessionExpiresAt).toBeNull()
  })

  it('switching instance context loads different session data', () => {
    const id1 = 'inst-1'
    const id2 = 'inst-2'
    sessionStorage.setItem(`oore_setup_session_${id1}`, 'tok-1')
    sessionStorage.setItem(`oore_setup_session_${id2}`, 'tok-2')

    useSetupStore.getState().setInstanceContext(id1)
    expect(useSetupStore.getState().sessionToken).toBe('tok-1')

    useSetupStore.getState().setInstanceContext(id2)
    expect(useSetupStore.getState().sessionToken).toBe('tok-2')
  })
})
