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

interface InstanceStoreState {
  instances: Record<string, Instance>
  activeInstanceId: string | null
  addInstance: (label: string, url: string, icon?: string) => string
  setActiveInstance: (id: string) => void
  updateInstance: (
    id: string,
    fields: Partial<Pick<Instance, 'label' | 'url' | 'icon'>>,
  ) => void
}

export const useInstanceStore = create<InstanceStoreState>()(
  persist(
    (set, get) => ({
      instances: {},
      activeInstanceId: null,

      addInstance: (label, url, icon) => {
        const id = crypto.randomUUID()
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
    }),
    { name: 'oore_instances' },
  ),
)

export function useActiveInstance() {
  return useInstanceStore((s) => {
    return s.activeInstanceId ? s.instances[s.activeInstanceId] : null
  })
}
