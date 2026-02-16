import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { useEffect } from 'react'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { z } from 'zod'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import {
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { useSetupPreferences, useSetupStatus } from '@/hooks/use-setup'
import { getApiErrorMessage } from '@/lib/api'
import {
  getActiveInstanceOrRedirect,
  requireSetupSessionOrRedirect,
} from '@/lib/instance-context'
import { PageMeta } from '@/lib/seo'
import { useSetupStore } from '@/stores/setup-store'

const modeSchema = z.object({
  mode: z.enum(['local', 'remote_oidc', 'remote_trusted']),
})

type ModeForm = z.infer<typeof modeSchema>

export const Route = createFileRoute('/setup/mode')({
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireSetupSessionOrRedirect(instance.id)
  },
  component: SetupModeStep,
  errorComponent: SetupModeError,
})

function SetupModeError({ error }: { error: Error }) {
  return (
    <div className="space-y-4">
      <Alert variant="destructive">
        <AlertTitle>Something went wrong</AlertTitle>
        <AlertDescription>{error.message}</AlertDescription>
      </Alert>
    </div>
  )
}

function toModeValue(
  runtimeMode: 'local' | 'remote' | undefined,
  remoteAuthMode: 'oidc' | 'trusted_proxy' | undefined,
): ModeForm['mode'] {
  if (runtimeMode === 'local') return 'local'
  if (remoteAuthMode === 'trusted_proxy') return 'remote_trusted'
  return 'remote_oidc'
}

function SetupModeStep() {
  const navigate = useNavigate()
  const sessionToken = useSetupStore((s) => s.sessionToken)
  const setCurrentStep = useSetupStore((s) => s.setCurrentStep)
  const { data: status } = useSetupStatus()
  const setupModeMutation = useSetupPreferences()

  const form = useForm<ModeForm>({
    resolver: zodResolver(modeSchema),
    defaultValues: {
      mode: toModeValue(status?.runtime_mode, status?.remote_auth_mode),
    },
  })

  useEffect(() => {
    setCurrentStep(1)
  }, [setCurrentStep])

  useEffect(() => {
    if (!status) return
    form.setValue('mode', toModeValue(status.runtime_mode, status.remote_auth_mode))
  }, [status, form])

  const errorMessage = setupModeMutation.error
    ? getApiErrorMessage(setupModeMutation.error, {
        invalid_state: 'Setup mode cannot be changed after owner creation.',
        session_expired:
          'Your setup session has expired. Restart setup with a fresh bootstrap token.',
        invalid_session:
          'Your setup session is invalid. Restart setup from the token step.',
      })
    : null

  function onSubmit(values: ModeForm) {
    if (!sessionToken) return

    const runtimeMode = values.mode === 'local' ? 'local' : 'remote'
    const remoteAuthMode =
      values.mode === 'remote_trusted'
        ? 'trusted_proxy'
        : values.mode === 'remote_oidc'
          ? 'oidc'
          : undefined

    setupModeMutation.mutate(
      {
        sessionToken,
        runtimeMode,
        remoteAuthMode,
      },
      {
        onSuccess: () => {
          if (values.mode === 'local') {
            void navigate({ to: '/setup/owner' })
            return
          }
          if (values.mode === 'remote_trusted') {
            void navigate({ to: '/setup/trusted-proxy' })
            return
          }
          void navigate({ to: '/setup/oidc' })
        },
      },
    )
  }

  return (
    <div className="space-y-4">
      <PageMeta title="Setup Mode" />
      <div className="space-y-1">
        <h2 className="text-lg font-medium">Access Mode</h2>
        <p className="text-sm text-muted-foreground">
          Choose how users will authenticate when accessing this instance.
        </p>
      </div>

      <Form {...form}>
        <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
          <FormField
            control={form.control}
            name="mode"
            render={({ field }) => (
              <FormItem>
                <FormLabel>Authentication mode</FormLabel>
                <FormControl>
                  <Select
                    value={field.value}
                    onValueChange={(value) => field.onChange(value)}
                    disabled={setupModeMutation.isPending}
                  >
                    <SelectTrigger>
                      <SelectValue placeholder="Choose mode" />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="local">Local Only (loopback login)</SelectItem>
                      <SelectItem value="remote_oidc">Remote (OIDC)</SelectItem>
                      <SelectItem value="remote_trusted">
                        Remote (Trusted Proxy / Warpgate)
                      </SelectItem>
                    </SelectContent>
                  </Select>
                </FormControl>
                <FormMessage />
              </FormItem>
            )}
          />

          {errorMessage ? (
            <Alert variant="destructive">
              <AlertTitle>Failed to save setup mode</AlertTitle>
              <AlertDescription>{errorMessage}</AlertDescription>
            </Alert>
          ) : null}

          <Button
            type="submit"
            className="w-full"
            disabled={setupModeMutation.isPending}
          >
            {setupModeMutation.isPending ? 'Saving...' : 'Continue'}
          </Button>
        </form>
      </Form>
    </div>
  )
}
