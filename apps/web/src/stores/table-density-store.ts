import { create } from 'zustand'
import { createJSONStorage, persist } from 'zustand/middleware'
import { useAuthStore } from '@/stores/auth-store'
import { useActiveInstance } from '@/stores/instance-store'

export type TableDensity = 'comfortable' | 'compact'

interface TableDensityState {
  /** Density by `<instance>:<user>` scope, then by table. */
  densities: Record<string, Record<string, TableDensity | undefined>>
  setDensity: (scope: string, table: string, density: TableDensity) => void
}

export const useTableDensityStore = create<TableDensityState>()(
  persist(
    (set) => ({
      densities: {},
      setDensity: (scope, table, density) =>
        set((state) => ({
          densities: {
            ...state.densities,
            [scope]: { ...state.densities[scope], [table]: density },
          },
        })),
    }),
    {
      name: 'oore-table-density',
      storage: createJSONStorage(() => localStorage),
      partialize: (state) => ({ densities: state.densities }),
    },
  ),
)

/** A table's row density, remembered for each instance and user. */
export function useTableDensity(table: string) {
  const instance = useActiveInstance()
  const userId = useAuthStore((state) => state.user?.user_id)
  const scope = `${instance?.id ?? ''}:${userId ?? ''}`
  const density = useTableDensityStore(
    (state) => state.densities[scope]?.[table] ?? 'comfortable',
  )
  const setDensity = useTableDensityStore((state) => state.setDensity)
  return [
    density,
    (next: TableDensity) => setDensity(scope, table, next),
  ] as const
}
