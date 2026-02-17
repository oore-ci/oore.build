import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import type { Instance } from '@/lib/types'
import { queryClient } from '@/lib/query-client'
import { clearAuthStorageForInstance } from '@/stores/auth-store'

function generateInstanceId(): string {
  // crypto can be missing in some test environments / older runtimes
  const webCrypto = (globalThis as unknown as { crypto?: Crypto }).crypto
  if (webCrypto && typeof webCrypto.randomUUID === 'function') {
    return webCrypto.randomUUID()
  }

  if (webCrypto && typeof webCrypto.getRandomValues === 'function') {
    const bytes = new Uint8Array(16)
    webCrypto.getRandomValues(bytes)
    bytes[6] = (bytes[6] & 0x0f) | 0x40
    bytes[8] = (bytes[8] & 0x3f) | 0x80

    const hex = Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join(
      '',
    )
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

export const useInstanceStore = create<InstanceStoreState>()(
  persist(
    (set, get) => ({
      instances: {},
      activeInstanceId: null,

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
        set({
          instances: { ...state.instances, [id]: instance },
          ...(isFirst ? { activeInstanceId: id } : {}),
        })
        return id
      },

      removeInstance: (id) => {
        const state = get()
        const { [id]: _, ...rest } = state.instances

        // Clear namespaced sessionStorage keys for this instance
        try {
          sessionStorage.removeItem(`oore_setup_session_${id}`)
          sessionStorage.removeItem(`oore_setup_session_expires_${id}`)
        } catch {
          // sessionStorage unavailable
        }

        // Clear auth (localStorage) keys for this instance
        clearAuthStorageForInstance(id)

        // Evict query cache entries scoped to this instance
        queryClient.removeQueries({ queryKey: [id] })

        // Auto-select next instance or null
        let nextActiveId: string | null = state.activeInstanceId
        if (state.activeInstanceId === id) {
          const remaining = Object.keys(rest)
          nextActiveId = remaining.length > 0 ? remaining[0] : null
        }

        set({ instances: rest, activeInstanceId: nextActiveId })
      },

      setActiveInstance: (id) => {
        const state = get()
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- id may not exist in record
        if (state.instances[id]) {
          set({ activeInstanceId: id })
        }
      },

      updateInstance: (id, fields) => {
        const state = get()
        const instance = state.instances[id]
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- id may not exist in record
        if (instance) {
          set({
            instances: {
              ...state.instances,
              [id]: { ...instance, ...fields },
            },
          })
        }
      },

      updateInstanceLabel: (id, label) => {
        const state = get()
        const instance = state.instances[id]
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- id may not exist in record
        if (instance) {
          set({
            instances: {
              ...state.instances,
              [id]: { ...instance, label },
            },
          })
        }
      },

      updateInstanceIcon: (id, icon) => {
        const state = get()
        const instance = state.instances[id]
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- id may not exist in record
        if (instance) {
          set({
            instances: {
              ...state.instances,
              [id]: { ...instance, icon },
            },
          })
        }
      },
    }),
    {
      name: 'oore_instances',
    },
  ),
)

export function useActiveInstance(): Instance | null {
  const activeId = useInstanceStore((s) => s.activeInstanceId)
  const instances = useInstanceStore((s) => s.instances)
  if (!activeId) return null
  return instances[activeId] ?? null
}
