import { create } from 'zustand'
import { persist } from 'zustand/middleware'

const MAX_RECENT = 5

interface RecentProject {
  id: string
  name: string
  visitedAt: number
}

interface RecentProjectsStoreState {
  projects: Array<RecentProject>
  trackVisit: (id: string, name: string) => void
  remove: (id: string) => void
  clear: () => void
}

export const useRecentProjectsStore = create<RecentProjectsStoreState>()(
  persist(
    (set) => ({
      projects: [],
      trackVisit: (id, name) =>
        set((state) => {
          const filtered = state.projects.filter((p) => p.id !== id)
          return {
            projects: [{ id, name, visitedAt: Date.now() }, ...filtered].slice(
              0,
              MAX_RECENT,
            ),
          }
        }),
      remove: (id) =>
        set((state) => ({
          projects: state.projects.filter((p) => p.id !== id),
        })),
      clear: () => set({ projects: [] }),
    }),
    { name: 'oore-recent-projects' },
  ),
)
