import { createSignal, Show } from 'solid-js'
import { createForm } from '@tanstack/solid-form'
import { createFileRoute, useNavigate } from '@tanstack/solid-router'
import z from 'zod'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { useSetupPreferences, useSetupStatus } from '@/hooks/use-setup'
import { useSetupStore } from '@/stores/setup-store'

export const Route = createFileRoute('/setup/mode')({
  component: SetupModePage,
})

function SetupModePage() {
  const navigate = useNavigate()
  const status = useSetupStatus()
  const savePreferences = useSetupPreferences()

  const [error, setError] = createSignal<string | null>(null)

  const schema = z.object({
    runtimeMode: z.enum(['local', 'remote']),
    remoteAuthMode: z.enum(['oidc', 'trusted_proxy']),
  })

  const form = createForm(() => ({
    defaultValues: {
      runtimeMode: 'local' as 'local' | 'remote',
      remoteAuthMode: 'oidc' as 'oidc' | 'trusted_proxy',
    },
    validators: {
      onSubmit: ({ value }) => {
        const parsed = schema.safeParse(value)
        if (parsed.success) return undefined
        return {
          form: 'Invalid runtime mode selection.',
        }
      },
    },
    onSubmit: async ({ value }) => {
      setError(null)
      try {
        await savePreferences.mutateAsync({
          runtime_mode: value.runtimeMode,
          remote_auth_mode:
            value.runtimeMode === 'remote' ? value.remoteAuthMode : undefined,
        })

        useSetupStore.getState().setCurrentStep(2)
        if (value.runtimeMode === 'local') {
          void navigate({ to: '/setup/owner' })
        } else if (value.remoteAuthMode === 'trusted_proxy') {
          void navigate({ to: '/setup/trusted-proxy' })
        } else {
          void navigate({ to: '/setup/oidc' })
        }
      } catch (submitError) {
        setError(
          submitError instanceof Error
            ? submitError.message
            : 'Failed to update mode',
        )
      }
    },
  }))

  const values = form.useStore((state) => state.values)

  const handleSubmit = (event: SubmitEvent) => {
    event.preventDefault()
    void form.handleSubmit()
  }

  return (
    <form class="space-y-4" onSubmit={handleSubmit}>
      <div>
        <h2 class="text-lg font-semibold">Runtime Mode</h2>
        <p class="text-sm text-muted-foreground">
          Choose how this instance is accessed.
        </p>
      </div>

      <Show when={status.data}>
        <Badge variant="outline">
          Current: {status.data?.runtime_mode} / {status.data?.remote_auth_mode}
        </Badge>
      </Show>

      {error() ? (
        <Alert variant="destructive">
          <AlertTitle>Unable to save</AlertTitle>
          <AlertDescription>{error()}</AlertDescription>
        </Alert>
      ) : null}

      <div class="grid gap-2 md:grid-cols-2">
        <button
          type="button"
          class={`border px-3 py-2 text-left text-sm ${
            values().runtimeMode === 'local' ? 'border-primary bg-primary/10' : 'hover:bg-accent'
          }`}
          onClick={() => form.setFieldValue('runtimeMode', 'local')}
        >
          <div class="font-medium">Local Only</div>
          <div class="text-xs text-muted-foreground">Loopback-only login on host machine.</div>
        </button>
        <button
          type="button"
          class={`border px-3 py-2 text-left text-sm ${
            values().runtimeMode === 'remote' ? 'border-primary bg-primary/10' : 'hover:bg-accent'
          }`}
          onClick={() => form.setFieldValue('runtimeMode', 'remote')}
        >
          <div class="font-medium">Remote Access</div>
          <div class="text-xs text-muted-foreground">Expose this instance to non-loopback clients.</div>
        </button>
      </div>

      <Show when={values().runtimeMode === 'remote'}>
        <div class="space-y-2 border p-3">
          <p class="text-xs font-medium text-muted-foreground">Remote authentication mode</p>
          <div class="grid gap-2 md:grid-cols-2">
            <button
              type="button"
              class={`border px-3 py-2 text-left text-sm ${
                values().remoteAuthMode === 'oidc' ? 'border-primary bg-primary/10' : 'hover:bg-accent'
              }`}
              onClick={() => form.setFieldValue('remoteAuthMode', 'oidc')}
            >
              OIDC Provider
            </button>
            <button
              type="button"
              class={`border px-3 py-2 text-left text-sm ${
                values().remoteAuthMode === 'trusted_proxy'
                  ? 'border-primary bg-primary/10'
                  : 'hover:bg-accent'
              }`}
              onClick={() => form.setFieldValue('remoteAuthMode', 'trusted_proxy')}
            >
              Trusted Proxy
            </button>
          </div>
        </div>
      </Show>

      <Button type="submit" disabled={savePreferences.isPending}>
        Continue
      </Button>
    </form>
  )
}
