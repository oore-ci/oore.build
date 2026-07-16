import { useEffect } from 'react'

import { useBreadcrumbStore } from '@/stores/breadcrumb-store'

export function useBreadcrumbLabel(routeId: string, label: string | undefined) {
  useEffect(() => {
    if (label) useBreadcrumbStore.getState().setLabel(routeId, label)
  }, [routeId, label])
}
