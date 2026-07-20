import { createBoundStore } from '@/lib/store'

interface UiStoreState {
  commandPaletteOpen: boolean
  setCommandPaletteOpen: (open: boolean) => void
  toggleCommandPalette: () => void
}

export const useUiStore = createBoundStore<UiStoreState>(
  (set) => ({
    commandPaletteOpen: false,
    setCommandPaletteOpen: (commandPaletteOpen) => set({ commandPaletteOpen }),
    toggleCommandPalette: () =>
      set((state) => ({ commandPaletteOpen: !state.commandPaletteOpen })),
  }),
  { name: 'oore-ui-preferences', partialize: () => ({}) },
)
