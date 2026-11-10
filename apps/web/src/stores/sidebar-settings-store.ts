import { create } from 'zustand'
import { createJSONStorage, persist } from 'zustand/middleware'
import { useAuthStore } from '@/stores/auth-store'
import { useActiveInstance } from '@/stores/instance-store'

interface SidebarSettingsState {
  /** Whether the Settings group is open, by `<instance>:<user>` scope. */
  open: Record<string, boolean | undefined>
  setOpen: (scope: string, open: boolean) => void
}

export const useSidebarSettingsStore = create<SidebarSettingsState>()(
  persist(
    (set) => ({
      open: {},
      setOpen: (scope, open) =>
        set((state) => ({ open: { ...state.open, [scope]: open } })),
    }),
    {
      name: 'oore-sidebar-settings',
      storage: createJSONStorage(() => localStorage),
      partialize: (state) => ({ open: state.open }),
    },
  ),
)

/**
 * The remembered open state of the sidebar's Settings group for this instance
 * and user, or `undefined` when the person has never toggled it.
 */
export function useSidebarSettingsOpen() {
  const instance = useActiveInstance()
  const userId = useAuthStore((state) => state.user?.user_id)
  const scope = `${instance?.id ?? ''}:${userId ?? ''}`
  const open = useSidebarSettingsStore((state) => state.open[scope])
  const setOpen = useSidebarSettingsStore((state) => state.setOpen)
  return [open, (next: boolean) => setOpen(scope, next)] as const
}
