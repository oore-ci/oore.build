import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { useEffect } from 'react'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { useSetupStatus } from '@/hooks/use-setup'

export const Route = createFileRoute('/')({
  component: IndexPage,
})

function IndexPage() {
  const { data: status, isLoading, error } = useSetupStatus()
  const navigate = useNavigate()

  useEffect(() => {
    document.title = 'oore.build'
  }, [])

  useEffect(() => {
    if (status?.setup_mode) {
      void navigate({ to: '/setup' })
    }
  }, [status?.setup_mode, navigate])

  if (isLoading) {
    return (
      <div className="min-h-screen flex items-center justify-center bg-background">
        <p className="text-muted-foreground text-sm">
          Connecting to backend...
        </p>
      </div>
    )
  }

  if (error) {
    return (
      <div className="min-h-screen flex items-center justify-center bg-background p-4">
        <div className="max-w-md w-full">
          <Alert variant="destructive">
            <AlertTitle>Connection failed</AlertTitle>
            <AlertDescription>
              Unable to reach the oore daemon. Make sure{' '}
              <code className="bg-muted px-1 py-0.5 text-xs">oored</code> is
              running and accessible.
            </AlertDescription>
          </Alert>
        </div>
      </div>
    )
  }

  if (status?.is_configured) {
    return (
      <div className="min-h-screen bg-background">
        <div className="max-w-4xl mx-auto px-6 py-16 space-y-8">
          <div className="space-y-2">
            <h1 className="text-2xl font-semibold tracking-tight">
              Dashboard
            </h1>
            <p className="text-muted-foreground text-sm">
              Instance{' '}
              <code className="bg-muted px-1.5 py-0.5 text-xs font-mono">
                {status.instance_id}
              </code>
            </p>
          </div>
          <div className="border border-border bg-card p-6">
            <p className="text-sm text-muted-foreground">
              Your oore.build instance is configured and ready. Build pipelines
              and runner management are coming in the next release.
            </p>
          </div>
        </div>
      </div>
    )
  }

  return (
    <div className="min-h-screen flex items-center justify-center bg-background">
      <p className="text-muted-foreground text-sm">Loading...</p>
    </div>
  )
}
