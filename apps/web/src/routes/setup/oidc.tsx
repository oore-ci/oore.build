import { createFileRoute } from '@tanstack/react-router'

import {
  getActiveInstanceOrRedirect,
  requireSetupSessionOrRedirect,
} from '@/lib/instance-context'

export const Route = createFileRoute('/setup/oidc')({
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireSetupSessionOrRedirect(instance.id)
  },
})
