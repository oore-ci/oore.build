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
  const setupState = status?.state
  const runtimeMode = status?.runtime_mode

  useEffect(() => {
    if (!setupState || !sessionToken) {
      setCurrentStep(0)
      return
    }

    const step = stateToStep(setupState, runtimeMode)
    setCurrentStep(step)

    if (setupState === 'bootstrap_pending' || setupState === 'uninitialized') {
      void navigate({ to: '/setup/mode' })
    } else if (setupState === 'idp_configured') {
      void navigate({ to: '/setup/owner' })
    } else if (setupState === 'owner_created') {
      void navigate({ to: '/setup/complete' })
    }
  }, [navigate, runtimeMode, sessionToken, setCurrentStep, setupState])
}

export function useSetupModeGuard(
  status: SetupStatus | undefined,
  expectedAuthMode: 'oidc' | 'trusted_proxy',
) {
  const navigate = useNavigate()
  const runtimeMode = status?.runtime_mode
  const remoteAuthMode = status?.remote_auth_mode

  useEffect(() => {
    if (
      runtimeMode &&
      (runtimeMode !== 'remote' || remoteAuthMode !== expectedAuthMode)
    ) {
      void navigate({ to: '/setup/mode' })
    }
  }, [expectedAuthMode, navigate, remoteAuthMode, runtimeMode])
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
  const setupState = status?.state
  const runtimeMode = status?.runtime_mode

  useEffect(() => {
    if (!setupState || !runtimeMode) return

    setCurrentStep(runtimeMode === 'local' ? 2 : 3)
    if (setupState === 'owner_created') {
      setCurrentStep(runtimeMode === 'local' ? 3 : 4)
      void navigate({ to: '/setup/complete' })
    }
  }, [navigate, runtimeMode, setCurrentStep, setupState])
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
