import type { Instance } from '@/lib/types'
import { queryClient } from '@/lib/query-client'
import { clearAuthStorageForInstance } from '@/stores/auth-store'
import { createSelectorStore } from '@/stores/store-utils'
import { createMemo, type Accessor } from 'solid-js'

interface PersistedInstanceState {
  state?: {
    instances?: Record<string, Instance>
    activeInstanceId?: string | null
  }
}

function loadPersistedState(): {
  instances: Record<string, Instance>
  activeInstanceId: string | null
} {
  try {
    const raw = localStorage.getItem('oore_instances')
    if (!raw) return { instances: {}, activeInstanceId: null }

    const parsed = JSON.parse(raw) as PersistedInstanceState
    const instances = parsed.state?.instances ?? {}
    const activeInstanceId = parsed.state?.activeInstanceId ?? null

    if (activeInstanceId && !instances[activeInstanceId]) {
      return { instances, activeInstanceId: null }
    }

    return { instances, activeInstanceId }
  } catch {
    return { instances: {}, activeInstanceId: null }
  }
}

function savePersistedState(
  instances: Record<string, Instance>,
  activeInstanceId: string | null,
): void {
  try {
    localStorage.setItem(
      'oore_instances',
      JSON.stringify({
        state: {
          instances,
          activeInstanceId,
        },
        version: 0,
      }),
    )
  } catch {
    // localStorage unavailable
  }
}

function generateInstanceId(): string {
  const webCrypto = (globalThis as unknown as { crypto?: Crypto }).crypto
  if (webCrypto && typeof webCrypto.randomUUID === 'function') {
    return webCrypto.randomUUID()
  }

  if (webCrypto && typeof webCrypto.getRandomValues === 'function') {
    const bytes = new Uint8Array(16)
    webCrypto.getRandomValues(bytes)
    bytes[6] = (bytes[6] & 0x0f) | 0x40
    bytes[8] = (bytes[8] & 0x3f) | 0x80

    const hex = Array.from(bytes, (value) =>
      value.toString(16).padStart(2, '0'),
    ).join('')
    return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20, 32)}`
  }

  return `instance-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`
}

interface InstanceStoreState {
  instances: Record<string, Instance>
  activeInstanceId: string | null
  addInstance: (label: string, url: string, icon?: string) => string
  removeInstance: (id: string) => void
  setActiveInstance: (id: string) => void
  updateInstance: (
    id: string,
    fields: Partial<Pick<Instance, 'label' | 'url' | 'icon'>>,
  ) => void
  updateInstanceLabel: (id: string, label: string) => void
  updateInstanceIcon: (id: string, icon: string) => void
}

const persisted = loadPersistedState()

export const useInstanceStore = createSelectorStore<InstanceStoreState>(
  (set, get) => ({
    instances: persisted.instances,
    activeInstanceId: persisted.activeInstanceId,

    addInstance: (label, url, icon) => {
      const id = generateInstanceId()
      const instance: Instance = {
        id,
        label,
        url,
        ...(icon ? { icon } : {}),
        addedAt: Date.now(),
      }
      const state = get()
      const isFirst = state.activeInstanceId === null
      const instances = { ...state.instances, [id]: instance }
      const activeInstanceId = isFirst ? id : state.activeInstanceId
      set({ instances, activeInstanceId })
      savePersistedState(instances, activeInstanceId)
      return id
    },

    removeInstance: (id) => {
      const state = get()
      const { [id]: _removed, ...rest } = state.instances

      try {
        sessionStorage.removeItem(`oore_setup_session_${id}`)
        sessionStorage.removeItem(`oore_setup_session_expires_${id}`)
      } catch {
        // sessionStorage unavailable
      }

      clearAuthStorageForInstance(id)
      queryClient.removeQueries({ queryKey: [id] })

      let nextActiveId: string | null = state.activeInstanceId
      if (state.activeInstanceId === id) {
        const remaining = Object.keys(rest)
        nextActiveId = remaining.length > 0 ? remaining[0] : null
      }

      set({ instances: rest, activeInstanceId: nextActiveId })
      savePersistedState(rest, nextActiveId)
    },

    setActiveInstance: (id) => {
      const state = get()
      if (!state.instances[id]) return
      set({ activeInstanceId: id })
      savePersistedState(state.instances, id)
    },

    updateInstance: (id, fields) => {
      const state = get()
      const instance = state.instances[id]
      if (!instance) return

      const instances = {
        ...state.instances,
        [id]: { ...instance, ...fields },
      }
      set({ instances })
      savePersistedState(instances, state.activeInstanceId)
    },

    updateInstanceLabel: (id, label) => {
      const state = get()
      const instance = state.instances[id]
      if (!instance) return

      const instances = {
        ...state.instances,
        [id]: { ...instance, label },
      }
      set({ instances })
      savePersistedState(instances, state.activeInstanceId)
    },

    updateInstanceIcon: (id, icon) => {
      const state = get()
      const instance = state.instances[id]
      if (!instance) return

      const instances = {
        ...state.instances,
        [id]: { ...instance, icon },
      }
      set({ instances })
      savePersistedState(instances, state.activeInstanceId)
    },
  }),
)

export function useActiveInstance(): Accessor<Instance | null> {
  const activeId = useInstanceStore((state) => state.activeInstanceId)
  const instances = useInstanceStore((state) => state.instances)
  return createMemo(() => {
    const id = activeId()
    if (!id) return null
    return instances()[id] ?? null
  })
}
