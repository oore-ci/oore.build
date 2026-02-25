import { createSelectorStore } from '@/stores/store-utils'

interface BreadcrumbStoreState {
  labels: Record<string, string>
  setLabel: (routeId: string, label: string) => void
  clearLabel: (routeId: string) => void
}

export const useBreadcrumbStore = createSelectorStore<BreadcrumbStoreState>(
  (set, get) => ({
    labels: {},
    setLabel: (routeId, label) => {
      const state = get()
      set({ labels: { ...state.labels, [routeId]: label } })
    },
    clearLabel: (routeId) => {
      const state = get()
      const { [routeId]: _removed, ...rest } = state.labels
      set({ labels: rest })
    },
  }),
)
