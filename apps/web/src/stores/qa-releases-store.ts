import { create } from 'zustand'
import { createJSONStorage, persist } from 'zustand/middleware'
import { useAuthStore } from '@/stores/auth-store'
import { useActiveInstance } from '@/stores/instance-store'

interface QaReleasesStoreState {
  selectedProjects: Record<string, string | undefined>
  setSelectedProjectId: (scope: string, projectId: string | null) => void
}

export const useQaReleasesStore = create<QaReleasesStoreState>()(
  persist(
    (set) => ({
      selectedProjects: {},
      setSelectedProjectId: (scope, projectId) =>
        set((state) => ({
          selectedProjects: {
            ...state.selectedProjects,
            [scope]: projectId ?? undefined,
          },
        })),
    }),
    {
      name: 'oore-qa-releases',
      storage: createJSONStorage(() => localStorage),
      partialize: (state) => ({ selectedProjects: state.selectedProjects }),
    },
  ),
)

/** The QA app selection, kept separately for each instance and user. */
export function useQaSelectedProject() {
  const instance = useActiveInstance()
  const userId = useAuthStore((state) => state.user?.user_id)
  const scope = `${instance?.id ?? ''}:${userId ?? ''}`
  const selectedProjectId = useQaReleasesStore(
    (state) => state.selectedProjects[scope] ?? null,
  )
  const setSelectedProjectId = useQaReleasesStore(
    (state) => state.setSelectedProjectId,
  )
  return {
    selectedProjectId,
    setSelectedProjectId: (projectId: string | null) =>
      setSelectedProjectId(scope, projectId),
  }
}
