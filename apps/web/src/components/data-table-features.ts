import {
  columnFilteringFeature,
  columnVisibilityFeature,
  functionalUpdate,
  rowPaginationFeature,
  rowSelectionFeature,
  rowSortingFeature,
  tableFeatures,
  type SortingState,
  type Updater,
} from '@tanstack/react-table'
// New in v9: declare the features this table uses — anything you don't
// register is tree-shaken out of the bundle.
export const features = tableFeatures({
  columnFilteringFeature,
  columnVisibilityFeature,
  rowPaginationFeature,
  rowSelectionFeature,
  rowSortingFeature,
})

// Pass this as the first generic argument to `ColumnDef`, `Column`, `Table`,
// and `Row` so each type knows which feature APIs are available.
export type DataTableFeatures = typeof features

export type SortDirection = 'asc' | 'desc'

export function dataTableSortingState(
  sort: string,
  direction: SortDirection,
): SortingState {
  return [{ id: sort, desc: direction === 'desc' }]
}

// The direction a column header shows and toggles from. Controlled sorting is
// read straight from options: v9 only publishes it to the table atoms in a
// layout effect, so `column.getIsSorted()` lags one render behind and a header
// rendered from it keeps re-sending the first click's direction.
export function dataTableColumnSortDirection(
  controlled: SortingState | undefined,
  columnId: string,
  fallback: () => SortDirection | false,
): SortDirection | false {
  if (!controlled) return fallback()
  const entry = controlled.find((sort) => sort.id === columnId)
  if (!entry) return false
  return entry.desc ? 'desc' : 'asc'
}

export function resolveDataTableSorting<TSort extends string>(
  updater: Updater<SortingState>,
  current: SortingState,
  sortKeys: ReadonlyArray<TSort>,
): { direction: SortDirection; sort: TSort } | null {
  const next = functionalUpdate(updater, current)[0]
  if (!next) return null

  const sort = sortKeys.find((key) => key === next.id)
  return sort ? { sort, direction: next.desc ? 'desc' : 'asc' } : null
}
