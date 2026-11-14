import { create } from 'zustand'
import { createJSONStorage, persist } from 'zustand/middleware'
import { useAuthStore } from '@/stores/auth-store'
import { useActiveInstance } from '@/stores/instance-store'
import { useProjects } from '@/hooks/use-projects'

export type BuildScope = 'mine' | 'all'

/** With more apps than this, Builds and Home start on your own builds. */
export const MINE_BY_DEFAULT_ABOVE_APPS = 5

export function defaultBuildScope(appCount: number): BuildScope {
  return appCount > MINE_BY_DEFAULT_ABOVE_APPS ? 'mine' : 'all'
}

interface BuildScopeState {
  /** Chosen scope by `<instance>:<user>`; unset follows the app count. */
  scopes: Record<string, BuildScope | undefined>
  setScope: (key: string, scope: BuildScope) => void
}

export const useBuildScopeStore = create<BuildScopeState>()(
  persist(
    (set) => ({
      scopes: {},
      setScope: (key, scope) =>
        set((state) => ({ scopes: { ...state.scopes, [key]: scope } })),
    }),
    {
      name: 'oore-build-scope',
      storage: createJSONStorage(() => localStorage),
      partialize: (state) => ({ scopes: state.scopes }),
    },
  ),
)

/**
 * Whether lists of builds show yours (builds you started and builds of apps
 * you pinned) or everyone's. `resolved` is false until the app count is
 * known, so callers can wait instead of loading the wrong list first.
 */
export function useBuildScope() {
  const instance = useActiveInstance()
  const userId = useAuthStore((state) => state.user?.user_id)
  const key = `${instance?.id ?? ''}:${userId ?? ''}`
  const chosen = useBuildScopeStore((state) => state.scopes[key])
  const setScope = useBuildScopeStore((state) => state.setScope)
  const projectsQuery = useProjects({ limit: 1 }, { enabled: !chosen })
  const appCount = projectsQuery.data?.total
  const scope =
    chosen ?? (appCount === undefined ? 'all' : defaultBuildScope(appCount))
  return {
    scope,
    resolved: !!chosen || appCount !== undefined || !!projectsQuery.error,
    setScope: (next: BuildScope) => setScope(key, next),
  }
}
