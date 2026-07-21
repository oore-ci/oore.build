import { create } from 'zustand'
import { persist } from 'zustand/middleware'

interface UiStoreState {
  commandPaletteOpen: boolean
  sidebarOpen: boolean
  setCommandPaletteOpen: (open: boolean) => void
  setSidebarOpen: (open: boolean) => void
  toggleCommandPalette: () => void
}

export const useUiStore = create<UiStoreState>()(
  persist(
    (set) => ({
      commandPaletteOpen: false,
      sidebarOpen: true,
      setCommandPaletteOpen: (commandPaletteOpen) =>
        set({ commandPaletteOpen }),
      setSidebarOpen: (sidebarOpen) => set({ sidebarOpen }),
      toggleCommandPalette: () =>
        set((state) => ({ commandPaletteOpen: !state.commandPaletteOpen })),
    }),
    {
      name: 'oore-ui-preferences',
      partialize: (state) => ({
        sidebarOpen: state.sidebarOpen,
      }),
    },
  ),
)
