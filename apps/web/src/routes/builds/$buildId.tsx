import { createFileRoute } from '@tanstack/react-router'

import { BuildDetailRoute } from '@/components/build-details/build-detail-route'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { useAuthStore } from '@/stores/auth-store'

export const Route = createFileRoute('/builds/$buildId')({
  staticData: {
    breadcrumbLabel: 'Details',
    breadcrumbParent: { label: 'Builds', to: '/builds' },
  },
  beforeLoad: ({ search }) => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
    const isQaViewer = useAuthStore.getState().user?.role === 'qa_viewer'
    if (search.install || isQaViewer) {
      void import('@/components/build-details/artifact-install-page')
    } else {
      void import('@/components/build-details/build-detail-page')
    }
  },
  validateSearch: (search: Record<string, unknown>): { install?: string } => ({
    install: typeof search.install === 'string' ? search.install : undefined,
  }),
  component: BuildDetailRoute,
})
