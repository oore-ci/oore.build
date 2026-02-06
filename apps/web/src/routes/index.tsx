import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { useEffect, useState } from 'react'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import { Spinner } from '@/components/ui/spinner'
import { useSetupStatus } from '@/hooks/use-setup'
import { useActiveInstance } from '@/stores/instance-store'
import { useAuthStore } from '@/stores/auth-store'
import AddInstanceDialog from '@/components/AddInstanceDialog'

export const Route = createFileRoute('/')({
  component: IndexPage,
})

function IndexPage() {
  const instance = useActiveInstance()
  const { data: status, isLoading, error } = useSetupStatus()
  const navigate = useNavigate()
  const [showAddInstance, setShowAddInstance] = useState(false)
  const authToken = useAuthStore((s) => s.token)
  const authExpiresAt = useAuthStore((s) => s.expiresAt)
  const clearAuth = useAuthStore((s) => s.clearAuth)

  useEffect(() => {
    document.title = 'oore.build'
  }, [])

  useEffect(() => {
    if (status?.setup_mode) {
      void navigate({ to: '/setup' })
    }
  }, [status?.setup_mode, navigate])

  // When configured but not authenticated (or token expired), redirect to login
  useEffect(() => {
    if (status?.is_configured) {
      const now = Math.floor(Date.now() / 1000)
      const valid = !!authToken && authExpiresAt != null && authExpiresAt > now
      if (!valid) {
        clearAuth()
        void navigate({ to: '/login' })
      }
    }
  }, [status?.is_configured, authToken, authExpiresAt, clearAuth, navigate])

  // No active instance — show onboarding
  if (!instance) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center p-6">
        <div className="w-full max-w-md space-y-6">
          <div className="text-center space-y-2">
            <h1 className="text-2xl font-semibold tracking-tight">
              Welcome to oore.build
            </h1>
            <p className="text-muted-foreground text-sm">
              Connect to your first backend instance to get started.
            </p>
          </div>

          <Card>
            <CardContent className="space-y-4">
              <p className="text-sm text-muted-foreground">
                Add a backend instance to begin setup or connect to an
                already-configured server.
              </p>
              <Button
                onClick={() => setShowAddInstance(true)}
                className="w-full"
              >
                Add Instance
              </Button>
            </CardContent>
          </Card>
        </div>

        <AddInstanceDialog
          open={showAddInstance}
          onOpenChange={setShowAddInstance}
        />
      </div>
    )
  }

  if (isLoading) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <div className="flex items-center gap-3">
          <Spinner className="size-5" />
          <p className="text-muted-foreground text-sm">
            Connecting to backend...
          </p>
        </div>
      </div>
    )
  }

  if (error) {
    return (
      <div className="flex-1 flex items-center justify-center p-6">
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
      <div className="max-w-4xl mx-auto w-full px-6 py-8 space-y-6">
        <div className="grid grid-cols-3 gap-4">
          <Card>
            <CardContent className="space-y-1">
              <p className="text-xs text-muted-foreground uppercase tracking-wider">Instance</p>
              <p className="text-sm font-medium font-mono truncate">{status.instance_id}</p>
            </CardContent>
          </Card>
          <Card>
            <CardContent className="space-y-1">
              <p className="text-xs text-muted-foreground uppercase tracking-wider">Status</p>
              <p className="text-sm font-medium text-green-600">Ready</p>
            </CardContent>
          </Card>
          <Card>
            <CardContent className="space-y-1">
              <p className="text-xs text-muted-foreground uppercase tracking-wider">Builds</p>
              <p className="text-sm font-medium text-muted-foreground">Coming soon</p>
            </CardContent>
          </Card>
        </div>

        <Card>
          <CardContent>
            <p className="text-sm text-muted-foreground">
              Build pipelines and runner management are coming in the next
              release. Once available, your build history and active runners
              will appear here.
            </p>
          </CardContent>
        </Card>
      </div>
    )
  }

  return (
    <div className="flex-1 flex items-center justify-center">
      <div className="flex items-center gap-3">
        <Spinner className="size-5" />
        <p className="text-muted-foreground text-sm">Loading...</p>
      </div>
    </div>
  )
}
