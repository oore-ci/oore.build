import {
  Outlet,
  createFileRoute,
  isRedirect,
  redirect,
  useNavigate,
} from '@tanstack/react-router'
import { useEffect } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { Tick02Icon } from '@hugeicons/core-free-icons'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent } from '@/components/ui/card'
import { useSetupStore } from '@/stores/setup-store'
import { useSessionCountdown } from '@/hooks/use-session-countdown'
import { getSetupStatus } from '@/lib/api'
import { getActiveInstanceOrRedirect } from '@/lib/instance-context'
import { webPageTitle } from '@/lib/seo'

export const Route = createFileRoute('/setup')({
  beforeLoad: async () => {
    const instance = getActiveInstanceOrRedirect()
    try {
      const status = await getSetupStatus(instance.url)
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
    <div className="flex items-center justify-center gap-1">
      {STEPS.map((label, index) => {
        const isActive = index === currentStep
        const isCompleted = index < currentStep

        return (
          <div key={label} className="flex items-center gap-1">
            {index > 0 ? (
              <div
                className={`h-px w-8 ${isCompleted ? 'bg-primary' : 'bg-border'}`}
              />
            ) : null}
            <Badge
              variant={
                isActive ? 'default' : isCompleted ? 'secondary' : 'outline'
              }
              className="text-xs"
            >
              {isCompleted ? (
                <>
                  <HugeiconsIcon
                    icon={Tick02Icon}
                    size={12}
                    className="mr-0.5"
                  />
                  {label}
                </>
              ) : (
                label
              )}
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
    document.title = webPageTitle('Setup')
  }, [])

  useEffect(() => {
    if (isExpired) {
      useSetupStore.getState().reset()
      void navigate({ to: '/setup' })
    }
  }, [isExpired, navigate])

  return (
    <div className="flex-1 flex flex-col items-center justify-center p-6">
      <div className="w-full max-w-lg space-y-8">
        <div className="text-center space-y-4">
          <div className="mx-auto flex size-14 items-center justify-center border-2 border-primary/20 bg-primary/5">
            <img src="/logo.svg" alt="oore.build logo" className="size-7" />
          </div>
          <div className="space-y-1">
            <h1 className="text-3xl font-bold tracking-tight">
              Instance Setup
            </h1>
            <p className="text-muted-foreground text-sm">
              Configure your self-hosted CI instance
            </p>
          </div>
        </div>

        <StepIndicator currentStep={currentStep} />

        {formatted && !isExpired ? (
          <div className="text-center">
            <p
              className={`text-xs font-mono ${isWarning ? 'text-destructive font-semibold' : 'text-muted-foreground'}`}
            >
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
