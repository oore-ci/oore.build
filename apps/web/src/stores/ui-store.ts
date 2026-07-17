import { create } from 'zustand'
import { persist } from 'zustand/middleware'

interface UiStoreState {
  commandPaletteOpen: boolean
  directRunnerTrustNoticeAcknowledgements: Record<string, true>
  sidebarOpen: boolean
  acknowledgeDirectRunnerTrustNotice: (noticeKey: string) => void
  setCommandPaletteOpen: (open: boolean) => void
  setSidebarOpen: (open: boolean) => void
  toggleCommandPalette: () => void
}

export const useUiStore = create<UiStoreState>()(
  persist(
    (set) => ({
      commandPaletteOpen: false,
      directRunnerTrustNoticeAcknowledgements: {},
      sidebarOpen: true,
      acknowledgeDirectRunnerTrustNotice: (noticeKey) =>
        set((state) => ({
          directRunnerTrustNoticeAcknowledgements: {
            ...state.directRunnerTrustNoticeAcknowledgements,
            [noticeKey]: true,
          },
        })),
      setCommandPaletteOpen: (commandPaletteOpen) =>
        set({ commandPaletteOpen }),
      setSidebarOpen: (sidebarOpen) => set({ sidebarOpen }),
      toggleCommandPalette: () =>
        set((state) => ({ commandPaletteOpen: !state.commandPaletteOpen })),
    }),
    {
      name: 'oore-ui-preferences',
      partialize: (state) => ({
        directRunnerTrustNoticeAcknowledgements:
          state.directRunnerTrustNoticeAcknowledgements,
        sidebarOpen: state.sidebarOpen,
      }),
    },
  ),
)
