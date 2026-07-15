import { createFileRoute } from '@tanstack/react-router'

import {
  getActiveInstanceOrRedirect,
  requireSetupSessionOrRedirect,
} from '@/lib/instance-context'

export const Route = createFileRoute('/setup/mode')({
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireSetupSessionOrRedirect(instance.id)
  },
})
