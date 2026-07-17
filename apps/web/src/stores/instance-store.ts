import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import { resolveInstanceApiBaseUrl } from '@/lib/instance-url'
import type { Instance } from '@/lib/types'
import { queryClient } from '@/lib/query-client'
import { clearAuthStorageForInstance, useAuthStore } from '@/stores/auth-store'
import { useSetupStore } from '@/stores/setup-store'

function backendOrigin(url: string): string | null {
  const baseUrl = resolveInstanceApiBaseUrl({ url })
  if (!baseUrl) return null

  try {
    return new URL(baseUrl).origin
  } catch {
    return baseUrl
  }
}

function clearInstanceScopedState(id: string): void {
  try {
    sessionStorage.removeItem(`oore_setup_session_${id}`)
    sessionStorage.removeItem(`oore_setup_session_expires_${id}`)
    sessionStorage.removeItem(`oore_setup_trusted_proxy_prefill_${id}`)
  } catch {
    // sessionStorage unavailable
  }

  clearAuthStorageForInstance(id)
  queryClient.removeQueries({ queryKey: [id] })
}

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
        clearInstanceScopedState(id)

        // Auto-select next instance or null
        let nextActiveId: string | null = state.activeInstanceId
        if (state.activeInstanceId === id) {
          const remaining = Object.keys(rest)
          nextActiveId = remaining[0] ?? null
        }

        set({ instances: rest, activeInstanceId: nextActiveId })
        useSetupStore.getState().setInstanceContext(nextActiveId)
        useAuthStore.getState().setInstanceContext(nextActiveId)
      },

      setActiveInstance: (id) => {
        const state = get()
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- id may not exist in record
        if (state.instances[id]) {
          set({ activeInstanceId: id })
          useSetupStore.getState().setInstanceContext(id)
          useAuthStore.getState().setInstanceContext(id)
        }
      },

      updateInstance: (id, fields) => {
        const state = get()
        const instance = state.instances[id]
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- id may not exist in record
        if (instance) {
          const next = { ...instance, ...fields }
          const authorityChanged =
            fields.url !== undefined &&
            backendOrigin(instance.url) !== backendOrigin(fields.url)

          if (authorityChanged) {
            clearInstanceScopedState(id)

            if (useSetupStore.getState().instanceId === id) {
              useSetupStore.getState().setInstanceContext(id)
            }
            if (useAuthStore.getState().instanceId === id) {
              useAuthStore.getState().setInstanceContext(id)
            }
          }

          set({
            instances: {
              ...state.instances,
              [id]: next,
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
