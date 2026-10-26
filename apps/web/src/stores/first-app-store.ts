import { create } from 'zustand'
import { createJSONStorage, persist } from 'zustand/middleware'
import type { CreateProjectRequest } from '@oore/client/models'
import { useAuthStore } from '@/stores/auth-store'
import { useActiveInstance } from '@/stores/instance-store'

interface FirstAppProgress {
  /** The project the setup checklist follows until its first app installs. */
  projectId?: string
  projectDraft?: CreateProjectRequest
}

export function useFirstAppScope() {
  const instance = useActiveInstance()
  const userId = useAuthStore((state) => state.user?.user_id)
  return `${instance?.id ?? ''}:${userId ?? ''}`
}

export const useFirstAppStore = create<{
  progress: Record<string, FirstAppProgress | undefined>
  update: (scope: string, values: Partial<FirstAppProgress>) => void
}>()(
  persist(
    (set) => ({
      progress: {},
      update: (scope, values) =>
        set((state) => ({
          progress: {
            ...state.progress,
            [scope]: { ...state.progress[scope], ...values },
          },
        })),
    }),
    {
      name: 'oore-first-app',
      storage: createJSONStorage(() => sessionStorage),
      partialize: (state) => ({ progress: state.progress }),
    },
  ),
)
