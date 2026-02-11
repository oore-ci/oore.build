import { create } from 'zustand'

interface BreadcrumbStoreState {
  labels: Record<string, string>
  setLabel: (routeId: string, label: string) => void
  clearLabel: (routeId: string) => void
}

export const useBreadcrumbStore = create<BreadcrumbStoreState>()((set) => ({
  labels: {},
  setLabel: (routeId, label) =>
    set((state) => ({
      labels: { ...state.labels, [routeId]: label },
    })),
  clearLabel: (routeId) =>
    set((state) => {
      const { [routeId]: _, ...rest } = state.labels
      return { labels: rest }
    }),
}))
