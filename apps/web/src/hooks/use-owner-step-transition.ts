import { useEffect } from 'react'
import { useNavigate } from '@tanstack/react-router'

import type { SetupStatus } from '@/lib/types'
import { useSetupStore } from '@/stores/setup-store'

export function useOwnerStepTransition(status?: SetupStatus) {
  const navigate = useNavigate()
  const setCurrentStep = useSetupStore((state) => state.setCurrentStep)

  useEffect(() => {
    if (!status) return
    setCurrentStep(status.runtime_mode === 'local' ? 2 : 3)
    if (status.state === 'owner_created') {
      setCurrentStep(status.runtime_mode === 'local' ? 3 : 4)
      void navigate({ to: '/setup/complete' })
    }
  }, [status, setCurrentStep, navigate])
}
