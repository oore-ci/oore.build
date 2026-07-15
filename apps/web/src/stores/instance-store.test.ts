import { beforeEach, describe, expect, it } from 'vitest'
import { useInstanceStore } from '@/stores/instance-store'

beforeEach(() => {
  sessionStorage.clear()
  localStorage.clear()
  useInstanceStore.setState({
    instances: {},
    activeInstanceId: null,
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

  it('updateInstance ignores unknown id', () => {
    const id = useInstanceStore
      .getState()
      .addInstance('Server', 'https://ci.example.com')
    useInstanceStore.getState().updateInstance('nonexistent', { label: 'Nope' })
    expect(useInstanceStore.getState().instances[id].label).toBe('Server')
  })
})
