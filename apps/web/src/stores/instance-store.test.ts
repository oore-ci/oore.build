import { beforeEach, describe, expect, it } from 'vitest'
import { queryClient } from '@/lib/query-client'
import { useAuthStore } from '@/stores/auth-store'
import { useInstanceStore } from '@/stores/instance-store'
import { useSetupStore } from '@/stores/setup-store'

const authUser = {
  email: 'owner@example.com',
  oidc_subject: 'owner-subject',
  user_id: 'owner-id',
  role: 'owner' as const,
}

beforeEach(() => {
  sessionStorage.clear()
  localStorage.clear()
  queryClient.clear()
  useInstanceStore.setState({
    instances: {},
    activeInstanceId: null,
  })
  useAuthStore.setState({
    instanceId: null,
    token: null,
    expiresAt: null,
    user: null,
  })
  useSetupStore.setState({
    instanceId: null,
    sessionToken: null,
    sessionExpiresAt: null,
  })
})

describe('useInstanceStore', () => {
  it('starts with empty instances and no active', () => {
    const state = useInstanceStore.getState()
    expect(state.instances).toEqual({})
    expect(state.activeInstanceId).toBeNull()
  })

  it('addInstance creates an instance and returns its id', () => {
    const id = useInstanceStore
      .getState()
      .addInstance('My Server', 'https://ci.example.com')
    const state = useInstanceStore.getState()
    expect(state.instances[id]).toBeDefined()
    expect(state.instances[id].label).toBe('My Server')
    expect(state.instances[id].url).toBe('https://ci.example.com')
    expect(state.instances[id].addedAt).toBeGreaterThan(0)
  })

  it('addInstance auto-selects first instance as active', () => {
    const id = useInstanceStore
      .getState()
      .addInstance('First', 'https://one.example.com')
    expect(useInstanceStore.getState().activeInstanceId).toBe(id)
  })

  it('addInstance does not change active when one already exists', () => {
    const first = useInstanceStore
      .getState()
      .addInstance('First', 'https://one.example.com')
    useInstanceStore.getState().addInstance('Second', 'https://two.example.com')
    expect(useInstanceStore.getState().activeInstanceId).toBe(first)
  })

  it('setActiveInstance switches active', () => {
    useInstanceStore.getState().addInstance('First', 'https://one.example.com')
    const second = useInstanceStore
      .getState()
      .addInstance('Second', 'https://two.example.com')
    useInstanceStore.getState().setActiveInstance(second)
    expect(useInstanceStore.getState().activeInstanceId).toBe(second)
  })

  it('setActiveInstance ignores unknown id', () => {
    const id = useInstanceStore
      .getState()
      .addInstance('First', 'https://one.example.com')
    useInstanceStore.getState().setActiveInstance('nonexistent')
    expect(useInstanceStore.getState().activeInstanceId).toBe(id)
  })

  it('updateInstanceLabel changes label', () => {
    const id = useInstanceStore
      .getState()
      .addInstance('Old', 'https://ci.example.com')
    useInstanceStore.getState().updateInstanceLabel(id, 'New Label')
    expect(useInstanceStore.getState().instances[id].label).toBe('New Label')
  })

  it('removeInstance removes and auto-selects next', () => {
    const first = useInstanceStore
      .getState()
      .addInstance('First', 'https://one.example.com')
    const second = useInstanceStore
      .getState()
      .addInstance('Second', 'https://two.example.com')
    useInstanceStore.getState().removeInstance(first)
    expect(useInstanceStore.getState().instances[first]).toBeUndefined()
    expect(useInstanceStore.getState().activeInstanceId).toBe(second)
  })

  it('removeInstance sets null when last instance removed', () => {
    const id = useInstanceStore
      .getState()
      .addInstance('Only', 'https://ci.example.com')
    useInstanceStore.getState().removeInstance(id)
    expect(useInstanceStore.getState().activeInstanceId).toBeNull()
    expect(Object.keys(useInstanceStore.getState().instances)).toHaveLength(0)
  })

  it('removeInstance clears namespaced sessionStorage keys', () => {
    const id = useInstanceStore
      .getState()
      .addInstance('Test', 'https://ci.example.com')
    sessionStorage.setItem(`oore_setup_session_${id}`, 'tok')
    sessionStorage.setItem(`oore_setup_session_expires_${id}`, '999')
    useInstanceStore.getState().removeInstance(id)
    expect(sessionStorage.getItem(`oore_setup_session_${id}`)).toBeNull()
    expect(
      sessionStorage.getItem(`oore_setup_session_expires_${id}`),
    ).toBeNull()
  })

  it('removeInstance does not change active when removing non-active', () => {
    const first = useInstanceStore
      .getState()
      .addInstance('First', 'https://one.example.com')
    const second = useInstanceStore
      .getState()
      .addInstance('Second', 'https://two.example.com')
    useInstanceStore.getState().removeInstance(second)
    expect(useInstanceStore.getState().activeInstanceId).toBe(first)
  })

  it('addInstance stores icon when provided', () => {
    const id = useInstanceStore
      .getState()
      .addInstance('Staging', 'https://staging.example.com', 'rocket-01')
    expect(useInstanceStore.getState().instances[id].icon).toBe('rocket-01')
  })

  it('addInstance omits icon when not provided', () => {
    const id = useInstanceStore
      .getState()
      .addInstance('Default', 'https://ci.example.com')
    expect(useInstanceStore.getState().instances[id].icon).toBeUndefined()
  })

  it('updateInstanceIcon changes icon', () => {
    const id = useInstanceStore
      .getState()
      .addInstance('Test', 'https://ci.example.com', 'cloud-server')
    useInstanceStore.getState().updateInstanceIcon(id, 'shield-01')
    expect(useInstanceStore.getState().instances[id].icon).toBe('shield-01')
  })

  it('updateInstance changes multiple fields at once', () => {
    const id = useInstanceStore
      .getState()
      .addInstance('Old', 'https://old.example.com', 'cloud-server')
    useInstanceStore.getState().updateInstance(id, {
      label: 'New',
      url: 'https://new.example.com',
      icon: 'rocket',
    })
    const inst = useInstanceStore.getState().instances[id]
    expect(inst.label).toBe('New')
    expect(inst.url).toBe('https://new.example.com')
    expect(inst.icon).toBe('rocket')
  })

  it('updateInstance changes a single field without affecting others', () => {
    const id = useInstanceStore
      .getState()
      .addInstance('Server', 'https://ci.example.com', 'shield')
    useInstanceStore.getState().updateInstance(id, { label: 'Renamed' })
    const inst = useInstanceStore.getState().instances[id]
    expect(inst.label).toBe('Renamed')
    expect(inst.url).toBe('https://ci.example.com')
    expect(inst.icon).toBe('shield')
  })

  it('clears active credentials before publishing a changed backend authority', () => {
    const id = useInstanceStore
      .getState()
      .addInstance('Server', 'https://original.example.com')
    useAuthStore.getState().setInstanceContext(id)
    useAuthStore.getState().setAuth('prior-bearer', 9_999_999_999, authUser)
    useSetupStore.getState().setInstanceContext(id)
    useSetupStore.getState().setSessionToken('prior-setup-session')
    useSetupStore.getState().setSessionExpiresAt(9_999_999_999)
    sessionStorage.setItem(
      `oore_setup_trusted_proxy_prefill_${id}`,
      '{"ownerEmail":"owner@example.com"}',
    )
    queryClient.setQueryData([id, 'users'], { users: ['prior-backend'] })

    let stateWhenUrlChanged:
      | { authToken: string | null; setupToken: string | null }
      | undefined
    const unsubscribe = useInstanceStore.subscribe((state, previous) => {
      if (state.instances[id]?.url !== previous.instances[id]?.url) {
        stateWhenUrlChanged = {
          authToken: useAuthStore.getState().token,
          setupToken: useSetupStore.getState().sessionToken,
        }
      }
    })

    useInstanceStore.getState().updateInstance(id, {
      url: 'https://replacement.example.com',
    })
    unsubscribe()

    expect(stateWhenUrlChanged).toEqual({
      authToken: null,
      setupToken: null,
    })
    expect(localStorage.getItem(`oore_auth_token_${id}`)).toBeNull()
    expect(sessionStorage.getItem(`oore_setup_session_${id}`)).toBeNull()
    expect(
      sessionStorage.getItem(`oore_setup_session_expires_${id}`),
    ).toBeNull()
    expect(
      sessionStorage.getItem(`oore_setup_trusted_proxy_prefill_${id}`),
    ).toBeNull()
    expect(queryClient.getQueryData([id, 'users'])).toBeUndefined()
  })

  it('clears only the edited inactive instance credentials', () => {
    const activeId = useInstanceStore
      .getState()
      .addInstance('Active', 'https://active.example.com')
    const editedId = useInstanceStore
      .getState()
      .addInstance('Edited', 'https://original.example.com')
    useAuthStore.getState().setInstanceContext(activeId)
    useAuthStore.getState().setAuth('active-bearer', 9_999_999_999, authUser)
    useSetupStore.getState().setInstanceContext(activeId)
    useSetupStore.getState().setSessionToken('active-setup-session')
    localStorage.setItem(`oore_auth_token_${editedId}`, 'prior-bearer')
    sessionStorage.setItem(
      `oore_setup_session_${editedId}`,
      'prior-setup-session',
    )
    queryClient.setQueryData([activeId, 'users'], ['active'])
    queryClient.setQueryData([editedId, 'users'], ['edited'])

    useInstanceStore.getState().updateInstance(editedId, {
      url: 'https://replacement.example.com',
    })

    expect(useAuthStore.getState().token).toBe('active-bearer')
    expect(useSetupStore.getState().sessionToken).toBe('active-setup-session')
    expect(localStorage.getItem(`oore_auth_token_${editedId}`)).toBeNull()
    expect(sessionStorage.getItem(`oore_setup_session_${editedId}`)).toBeNull()
    expect(queryClient.getQueryData([activeId, 'users'])).toEqual(['active'])
    expect(queryClient.getQueryData([editedId, 'users'])).toBeUndefined()

    useInstanceStore.getState().setActiveInstance(editedId)
    expect(useAuthStore.getState().token).toBeNull()
    expect(useSetupStore.getState().sessionToken).toBeNull()
  })

  it('preserves credentials for non-authority edits', () => {
    const id = useInstanceStore
      .getState()
      .addInstance('Server', 'https://ci.example.com')
    useAuthStore.getState().setInstanceContext(id)
    useAuthStore.getState().setAuth('current-bearer', 9_999_999_999, authUser)
    useSetupStore.getState().setInstanceContext(id)
    useSetupStore.getState().setSessionToken('current-setup-session')
    queryClient.setQueryData([id, 'users'], ['current'])

    useInstanceStore.getState().updateInstance(id, {
      label: 'Renamed',
      url: 'https://CI.EXAMPLE.COM:443/',
    })

    expect(useAuthStore.getState().token).toBe('current-bearer')
    expect(useSetupStore.getState().sessionToken).toBe('current-setup-session')
    expect(queryClient.getQueryData([id, 'users'])).toEqual(['current'])
  })

  it('updateInstance ignores unknown id', () => {
    const id = useInstanceStore
      .getState()
      .addInstance('Server', 'https://ci.example.com')
    useInstanceStore.getState().updateInstance('nonexistent', { label: 'Nope' })
    expect(useInstanceStore.getState().instances[id].label).toBe('Server')
  })
})
