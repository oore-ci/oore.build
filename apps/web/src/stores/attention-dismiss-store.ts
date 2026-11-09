import { create } from 'zustand'
import { createJSONStorage, persist } from 'zustand/middleware'
import { useAuthStore } from '@/stores/auth-store'
import { useActiveInstance } from '@/stores/instance-store'

interface AttentionDismissState {
  /**
   * Dismissed Needs attention items by `<instance>:<user>` scope, then by
   * item key, holding the revision that was dismissed. A new revision (a
   * newer failed build, another blocked build) shows the item again.
   */
  dismissed: Record<string, Record<string, string | undefined>>
  dismiss: (scope: string, key: string, revision: string) => void
  /** Forget dismissals for items that are no longer listed. */
  prune: (scope: string, keys: ReadonlySet<string>) => void
}

export const useAttentionDismissStore = create<AttentionDismissState>()(
  persist(
    (set) => ({
      dismissed: {},
      dismiss: (scope, key, revision) =>
        set((state) => ({
          dismissed: {
            ...state.dismissed,
            [scope]: { ...state.dismissed[scope], [key]: revision },
          },
        })),
      prune: (scope, keys) =>
        set((state) => {
          const current = state.dismissed[scope]
          if (!current) return state
          const kept = Object.fromEntries(
            Object.entries(current).filter(([key]) => keys.has(key)),
          )
          if (Object.keys(kept).length === Object.keys(current).length)
            return state
          return { dismissed: { ...state.dismissed, [scope]: kept } }
        }),
    }),
    {
      name: 'oore-attention-dismissed',
      storage: createJSONStorage(() => localStorage),
      partialize: (state) => ({ dismissed: state.dismissed }),
    },
  ),
)

/** This viewer's dismissals on the active instance. */
export function useAttentionDismissals() {
  const instance = useActiveInstance()
  const userId = useAuthStore((state) => state.user?.user_id)
  const scope = `${instance?.id ?? ''}:${userId ?? ''}`
  const dismissed = useAttentionDismissStore((state) => state.dismissed[scope])
  const dismiss = useAttentionDismissStore((state) => state.dismiss)
  const prune = useAttentionDismissStore((state) => state.prune)
  return {
    dismissed: dismissed ?? {},
    dismiss: (key: string, revision: string) => dismiss(scope, key, revision),
    prune: (keys: ReadonlySet<string>) => prune(scope, keys),
  }
}
