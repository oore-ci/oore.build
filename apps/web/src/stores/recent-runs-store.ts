import { create } from 'zustand'
import { createJSONStorage, persist } from 'zustand/middleware'
import { useAuthStore } from '@/stores/auth-store'
import { useActiveInstance } from '@/stores/instance-store'

/** One pipeline and branch a person ran from the Run build drawer. */
export interface RecentRun {
  projectId: string
  projectName: string
  pipelineId: string
  pipelineName: string
  branch: string
}

const MAX_RECENT_RUNS = 5

/**
 * Adds a run to the front of the list, dropping an older copy of the same
 * project, pipeline and branch, and keeps the newest few.
 */
export function rememberRun(
  runs: ReadonlyArray<RecentRun>,
  run: RecentRun,
): Array<RecentRun> {
  return [
    run,
    ...runs.filter(
      (candidate) =>
        candidate.projectId !== run.projectId ||
        candidate.pipelineId !== run.pipelineId ||
        candidate.branch !== run.branch,
    ),
  ].slice(0, MAX_RECENT_RUNS)
}

export const useRecentRunsStore = create<{
  runs: Record<string, Array<RecentRun> | undefined>
  remember: (scope: string, run: RecentRun) => void
}>()(
  persist(
    (set) => ({
      runs: {},
      remember: (scope, run) =>
        set((state) => ({
          runs: {
            ...state.runs,
            [scope]: rememberRun(state.runs[scope] ?? [], run),
          },
        })),
    }),
    {
      name: 'oore-recent-runs',
      storage: createJSONStorage(() => localStorage),
      partialize: (state) => ({ runs: state.runs }),
    },
  ),
)

const NO_RUNS: Array<RecentRun> = []

/** The `<instance>:<user>` key recent runs are kept under. */
export function useRecentRunsScope() {
  const instance = useActiveInstance()
  const userId = useAuthStore((state) => state.user?.user_id)
  return `${instance?.id ?? ''}:${userId ?? ''}`
}

/** The person's recent runs on this instance, newest first. */
export function useRecentRuns() {
  const scope = useRecentRunsScope()
  const runs = useRecentRunsStore((state) => state.runs[scope] ?? NO_RUNS)
  const remember = useRecentRunsStore((state) => state.remember)
  return [runs, (run: RecentRun) => remember(scope, run)] as const
}
