import { createFileRoute } from '@tanstack/react-router'
import { searchChoice, searchNumber, searchString } from '@/lib/search-input'
import type { SearchInput } from '@/lib/search-input'

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

export function parseApiTokensSearch(search: SearchInput): ApiTokensSearch {
  const page = searchNumber(search, 'page')
  const pageSize = searchNumber(search, 'pageSize')
  const q = searchString(search, 'q')?.trim() ?? ''
  const sort = searchChoice(search, 'sort', API_TOKEN_SORTS)

  return {
    q: q || undefined,
    sort,
    direction: searchString(search, 'direction') === 'asc' ? 'asc' : undefined,
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
  beforeLoad: (context) => {
    const instance = getActiveInstanceOrRedirect()
    requireInstanceRoleOrRedirect(
      instance.id,
      ['owner', 'admin', 'developer'],
      context,
    )
  },
})
