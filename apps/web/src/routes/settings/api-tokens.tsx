import { createFileRoute } from '@tanstack/react-router'

import {
  getActiveInstanceOrRedirect,
  requireInstanceRoleOrRedirect,
} from '@/lib/instance-context'

export type ApiTokenSort =
  | 'created_at'
  | 'last_used_at'
  | 'name'
  | 'role'
  | 'status'

export interface ApiTokensSearch {
  direction?: 'asc' | 'desc'
  page?: number
  pageSize?: 20 | 50 | 100
  q?: string
  sort?: ApiTokenSort
}

const API_TOKEN_SORTS = new Set<ApiTokenSort>([
  'created_at',
  'last_used_at',
  'name',
  'role',
  'status',
])

export function parseApiTokensSearch(
  search: Record<string, unknown>,
): ApiTokensSearch {
  const page = Number(search.page)
  const pageSize = Number(search.pageSize)
  const q = typeof search.q === 'string' ? search.q.trim() : ''
  const sort = search.sort as ApiTokenSort

  return {
    q: q || undefined,
    sort: API_TOKEN_SORTS.has(sort) ? sort : undefined,
    direction: search.direction === 'asc' ? 'asc' : undefined,
    page: Number.isInteger(page) && page > 1 ? page : undefined,
    pageSize: pageSize === 50 || pageSize === 100 ? pageSize : undefined,
  }
}

export const Route = createFileRoute('/settings/api-tokens')({
  staticData: {
    breadcrumb: {
      title: 'API tokens',
    },
  },
  validateSearch: parseApiTokensSearch,
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireInstanceRoleOrRedirect(instance.id, ['owner', 'admin', 'developer'])
  },
})
