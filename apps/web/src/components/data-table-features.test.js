import { expect, test } from 'bun:test'
import {
  dataTableColumnSortDirection,
  dataTableSortingState,
  resolveDataTableSorting,
} from '@/components/data-table-features'

const SORTS = ['created_at', 'name']

// Mirrors one DataTableColumnHeader click against controlled sorting state,
// while the table atom still reports no sort, as it does one render late.
function clickHeader(state, columnId) {
  const sorting = dataTableSortingState(state.sort, state.direction)
  const direction = dataTableColumnSortDirection(sorting, columnId, () => false)
  const updater = () => [{ id: columnId, desc: direction === 'asc' }]
  return resolveDataTableSorting(updater, sorting, SORTS) ?? state
}

test('reads the header direction from controlled sorting', () => {
  const sorting = dataTableSortingState('name', 'desc')
  const stale = () => 'asc'

  expect(dataTableColumnSortDirection(sorting, 'name', stale)).toBe('desc')
  expect(dataTableColumnSortDirection(sorting, 'created_at', stale)).toBe(false)
  expect(dataTableColumnSortDirection(undefined, 'name', stale)).toBe('asc')
})

test('repeated header clicks alternate ascending and descending', () => {
  let state = { sort: 'created_at', direction: 'desc' }

  const seen = []
  for (let click = 0; click < 4; click += 1) {
    state = clickHeader(state, 'name')
    seen.push(`${state.sort}:${state.direction}`)
  }

  expect(seen).toEqual(['name:asc', 'name:desc', 'name:asc', 'name:desc'])
  expect(clickHeader(state, 'created_at')).toEqual({
    sort: 'created_at',
    direction: 'asc',
  })
})
