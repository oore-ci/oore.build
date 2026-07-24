import { createFileRoute } from '@tanstack/react-router'

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

export function parseRunnersSearch(
  search: Record<string, unknown>,
): RunnersSearch {
  const page = Number(search.page)
  const pageSize = Number(search.pageSize)
  const q = typeof search.q === 'string' ? search.q.trim() : ''
  const sort = search.sort as RunnerSort

  return {
    q: q || undefined,
    sort: RUNNER_SORTS.has(sort) ? sort : undefined,
    direction: search.direction === 'asc' ? 'asc' : undefined,
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
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireInstanceRoleOrRedirect(instance.id, ['owner', 'admin', 'developer'])
  },
})
