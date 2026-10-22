import { createFileRoute } from '@tanstack/react-router'

import { BuildDetailRoute } from '@/components/build-details/build-detail-route'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { useAuthStore } from '@/stores/auth-store'
import { searchString } from '@/lib/search-input'
import type { SearchInput } from '@/lib/search-input'

interface BuildDetailSearch {
  install?: string
  release?: string
  tab?: string
}

export const Route = createFileRoute('/builds/$buildId')({
  staticData: {
    breadcrumb: {
      title: 'Details',
    },
  },
  beforeLoad: ({ search, ...context }) => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id, context)
    const isQaViewer = useAuthStore.getState().user?.role === 'qa_viewer'
    if (search.install || isQaViewer) {
      void import('@/components/build-details/artifact-install-page')
    } else {
      void import('@/components/build-details/build-detail-page')
    }
  },
  validateSearch: (search: SearchInput): BuildDetailSearch => ({
    install: searchString(search, 'install'),
    release: searchString(search, 'release'),
    tab: searchString(search, 'tab'),
  }),
  component: BuildDetailRoute,
})
