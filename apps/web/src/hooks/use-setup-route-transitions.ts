import { useEffect, useRef } from 'react'
import { useNavigate } from '@tanstack/react-router'

import type { SetupStatus } from '@/lib/types'
import { useSetupStore } from '@/stores/setup-store'

function stateToStep(
  state: string,
  runtimeMode: 'local' | 'remote' | undefined,
): number {
  if (state === 'bootstrap_pending' || state === 'uninitialized') return 1

  if (runtimeMode === 'local') {
    if (state === 'idp_configured') return 2
    if (state === 'owner_created') return 3
    return 0
  }

  if (state === 'idp_configured') return 3
  if (state === 'owner_created') return 4
  return 0
}

export function useBootstrapStepTransition(
  status: SetupStatus | undefined,
  sessionToken: string | null,
) {
  const navigate = useNavigate()
  const setCurrentStep = useSetupStore((state) => state.setCurrentStep)

  useEffect(() => {
    if (!status || !sessionToken) {
      setCurrentStep(0)
      return
    }

    const step = stateToStep(status.state, status.runtime_mode)
    setCurrentStep(step)

    if (
      status.state === 'bootstrap_pending' ||
      status.state === 'uninitialized'
    ) {
      void navigate({ to: '/setup/mode' })
    } else if (status.state === 'idp_configured') {
      void navigate({ to: '/setup/owner' })
    } else if (status.state === 'owner_created') {
      void navigate({ to: '/setup/complete' })
    }
  }, [
    navigate,
    sessionToken,
    setCurrentStep,
    status?.runtime_mode,
    status?.state,
  ])
}

export function useSetupModeGuard(
  status: SetupStatus | undefined,
  expectedAuthMode: 'oidc' | 'trusted_proxy',
) {
  const navigate = useNavigate()

  useEffect(() => {
    if (
      status &&
      (status.runtime_mode !== 'remote' ||
        status.remote_auth_mode !== expectedAuthMode)
    ) {
      void navigate({ to: '/setup/mode' })
    }
  }, [
    expectedAuthMode,
    navigate,
    status?.remote_auth_mode,
    status?.runtime_mode,
  ])
}

export function useSetupCurrentStep(step: number | null) {
  const setCurrentStep = useSetupStore((state) => state.setCurrentStep)

  useEffect(() => {
    if (step != null) setCurrentStep(step)
  }, [setCurrentStep, step])
}

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
  }, [navigate, setCurrentStep, status?.runtime_mode, status?.state])
}

export function useExpiredSetupSessionRedirect(isExpired: boolean) {
  const navigate = useNavigate()
  const reset = useSetupStore((state) => state.reset)
  const handledRef = useRef(false)

  useEffect(() => {
    if (!isExpired) {
      handledRef.current = false
      return
    }
    if (handledRef.current) return

    handledRef.current = true
    reset()
    void navigate({ to: '/setup' })
  }, [isExpired, navigate, reset])
}
