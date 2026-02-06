import { createFileRoute, isRedirect, Outlet, redirect, useNavigate } from '@tanstack/react-router'
import { useEffect } from 'react'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent } from '@/components/ui/card'
import { useSetupStore } from '@/stores/setup-store'
import { useSessionCountdown } from '@/hooks/use-session-countdown'
import { getSetupStatus } from '@/lib/api'

export const Route = createFileRoute('/setup')({
  beforeLoad: async () => {
    try {
      const status = await getSetupStatus()
      if (status.is_configured) {
        throw redirect({ to: '/' })
      }
    } catch (e) {
      if (isRedirect(e)) throw e
      throw e
    }
  },
  component: SetupLayout,
})

const STEPS = ['Token', 'OIDC', 'Owner', 'Complete'] as const

function StepIndicator({ currentStep }: { currentStep: number }) {
  return (
    <div className="flex items-center justify-center gap-3">
      {STEPS.map((label, index) => {
        const isActive = index === currentStep
        const isCompleted = index < currentStep

        return (
          <div key={label} className="flex items-center gap-3">
            {index > 0 ? (
              <div
                className={`h-px w-6 ${isCompleted ? 'bg-primary' : 'bg-muted'}`}
              />
            ) : null}
            <Badge
              variant={isActive ? 'default' : isCompleted ? 'secondary' : 'outline'}
              className="text-xs"
            >
              {isCompleted ? <><span className="mr-0.5">✓</span>{label}</> : label}
            </Badge>
          </div>
        )
      })}
    </div>
  )
}

function SetupLayout() {
  const currentStep = useSetupStore((s) => s.currentStep)
  const { formatted, isWarning, isExpired } = useSessionCountdown()
  const navigate = useNavigate()

  useEffect(() => {
    document.title = 'Setup — oore.build'
  }, [])

  useEffect(() => {
    if (isExpired) {
      useSetupStore.getState().reset()
      void navigate({ to: '/setup' })
    }
  }, [isExpired, navigate])

  return (
    <div className="min-h-screen bg-background flex flex-col items-center justify-center p-4">
      <div className="w-full max-w-lg space-y-6">
        <div className="text-center space-y-2">
          <h1 className="text-2xl font-semibold tracking-tight">
            oore.build Setup
          </h1>
          <p className="text-muted-foreground text-sm">
            Configure your self-hosted CI instance
          </p>
        </div>

        <StepIndicator currentStep={currentStep} />

        {formatted && !isExpired ? (
          <div className="text-center">
            <p className={`text-sm font-mono ${isWarning ? 'text-destructive font-semibold' : 'text-muted-foreground'}`}>
              Session expires in {formatted}
            </p>
          </div>
        ) : null}

        <Card>
          <CardContent>
            <Outlet />
          </CardContent>
        </Card>
      </div>
    </div>
  )
}
