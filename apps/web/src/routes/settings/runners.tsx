import { createFileRoute } from '@tanstack/react-router'
import { searchChoice, searchNumber, searchString } from '@/lib/search-input'
import type { SearchInput } from '@/lib/search-input'

import {
  getActiveInstanceOrRedirect,
  requireInstanceRoleOrRedirect,
} from '@/lib/instance-context'

export type RunnerSort = 'created_at' | 'last_heartbeat_at' | 'name' | 'status'

export interface RunnersSearch {
  direction?: 'asc' | 'desc'
  page?: number
  pageSize?: 20 | 50 | 100
  q?: string
  sort?: RunnerSort
}

const RUNNER_SORTS = new Set<RunnerSort>([
  'created_at',
  'last_heartbeat_at',
  'name',
  'status',
])

export function parseRunnersSearch(search: SearchInput): RunnersSearch {
  const page = searchNumber(search, 'page')
  const pageSize = searchNumber(search, 'pageSize')
  const q = searchString(search, 'q')?.trim() ?? ''
  const sort = searchChoice(search, 'sort', RUNNER_SORTS)

  return {
    q: q || undefined,
    sort,
    direction:
      searchString(search, 'direction') === 'desc' ? 'desc' : undefined,
    page: Number.isInteger(page) && page > 1 ? page : undefined,
    pageSize: pageSize === 50 || pageSize === 100 ? pageSize : undefined,
  }
}

export const Route = createFileRoute('/settings/runners')({
  staticData: {
    breadcrumb: {
      title: 'Runners',
    },
  },
  validateSearch: parseRunnersSearch,
  beforeLoad: (context) => {
    const instance = getActiveInstanceOrRedirect()
    requireInstanceRoleOrRedirect(
      instance.id,
      ['owner', 'admin', 'developer'],
      context,
    )
  },
})
