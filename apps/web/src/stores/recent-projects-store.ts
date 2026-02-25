import { createSelectorStore } from '@/stores/store-utils'

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
}

function loadRecentProjects(): Array<RecentProject> {
  try {
    const raw = localStorage.getItem('oore-recent-projects')
    if (!raw) return []
    const parsed = JSON.parse(raw) as {
      state?: { projects?: Array<RecentProject> }
    }
    return parsed.state?.projects ?? []
  } catch {
    return []
  }
}

function saveRecentProjects(projects: Array<RecentProject>): void {
  try {
    localStorage.setItem(
      'oore-recent-projects',
      JSON.stringify({
        state: { projects },
        version: 0,
      }),
    )
  } catch {
    // localStorage unavailable
  }
}

const initialProjects = loadRecentProjects()

export const useRecentProjectsStore = createSelectorStore<RecentProjectsStoreState>(
  (set, get) => ({
    projects: initialProjects,
    trackVisit: (id, name) => {
      const state = get()
      const filtered = state.projects.filter((project) => project.id !== id)
      const projects = [{ id, name, visitedAt: Date.now() }, ...filtered].slice(
        0,
        MAX_RECENT,
      )
      set({ projects })
      saveRecentProjects(projects)
    },
    remove: (id) => {
      const state = get()
      const projects = state.projects.filter((project) => project.id !== id)
      set({ projects })
      saveRecentProjects(projects)
    },
  }),
)
