import { createSignal, Show } from 'solid-js'
import { createForm } from '@tanstack/solid-form'
import { createFileRoute, useNavigate } from '@tanstack/solid-router'
import z from 'zod'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { FormField } from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import {
  useSetupLocalOwnerCreate,
  useSetupOidcStart,
  useSetupStatus,
  useSetupTrustedProxyClaimOwner,
} from '@/hooks/use-setup'
import { useSetupStore } from '@/stores/setup-store'

export const Route = createFileRoute('/setup/owner')({
  component: SetupOwnerPage,
})

function SetupOwnerPage() {
  const navigate = useNavigate()
  const status = useSetupStatus()
  const createLocalOwner = useSetupLocalOwnerCreate()
  const startOidcOwnerFlow = useSetupOidcStart()
  const claimTrustedProxyOwner = useSetupTrustedProxyClaimOwner()

  const [error, setError] = createSignal<string | null>(null)

  const schema = z.object({
    email: z
      .string()
      .trim()
      .min(1, 'Owner email is required.')
      .email('Owner email must be valid.'),
  })

  const form = createForm(() => ({
    defaultValues: {
      email: '',
    },
    validators: {
      onSubmit: ({ value }) => {
        const parsed = schema.safeParse(value)
        if (parsed.success) return undefined
        return {
          fields: {
            email:
              parsed.error.flatten().fieldErrors.email?.[0] ??
              'Owner email is required.',
          },
        }
      },
    },
    onSubmit: async ({ value }) => {
      setError(null)
      try {
        await createLocalOwner.mutateAsync(value.email.trim())
        continueToComplete()
      } catch (submitError) {
        setError(
          submitError instanceof Error
            ? submitError.message
            : 'Failed to create owner',
        )
      }
    },
  }))

  const submissionAttempts = form.useStore((state) => state.submissionAttempts)

  const continueToComplete = () => {
    useSetupStore.getState().setCurrentStep(4)
    void navigate({ to: '/setup/complete' })
  }

  const handleLocalOwnerSubmit = (event: SubmitEvent) => {
    event.preventDefault()
    void form.handleSubmit()
  }

  const handleOidcOwner = async () => {
    setError(null)
      try {
        const redirectUri = `${window.location.origin}/auth/callback`
        const response = await startOidcOwnerFlow.mutateAsync(redirectUri)
      try {
        const sessionToken = useSetupStore.getState().sessionToken
        sessionStorage.setItem('oore_oidc_state', response.state)
        sessionStorage.setItem('oore_oidc_flow', 'setup_owner')
        if (sessionToken) {
          sessionStorage.setItem('oore_setup_session_token', sessionToken)
        }
      } catch {
        // ignore
      }
      window.location.href = response.authorization_url
    } catch (value) {
      setError(value instanceof Error ? value.message : 'Failed to start OIDC owner flow')
    }
  }

  const handleTrustedProxyOwner = async () => {
    setError(null)
    try {
      await claimTrustedProxyOwner.mutateAsync()
      continueToComplete()
    } catch (value) {
      setError(value instanceof Error ? value.message : 'Failed to claim owner')
    }
  }

  return (
    <form class="space-y-4" onSubmit={handleLocalOwnerSubmit}>
      <div>
        <h2 class="text-lg font-semibold">Create Owner</h2>
        <p class="text-sm text-muted-foreground">
          Finalize owner identity for this instance.
        </p>
      </div>

      {error() ? (
        <Alert variant="destructive">
          <AlertTitle>Owner setup failed</AlertTitle>
          <AlertDescription>{error()}</AlertDescription>
        </Alert>
      ) : null}

      <Show
        when={status.data?.runtime_mode === 'local'}
        fallback={
          <Show
            when={status.data?.remote_auth_mode === 'trusted_proxy'}
            fallback={
              <Button
                type="button"
                onClick={handleOidcOwner}
                disabled={startOidcOwnerFlow.isPending}
              >
                Continue with OIDC Owner Authentication
              </Button>
            }
          >
            <Button
              type="button"
              onClick={handleTrustedProxyOwner}
              disabled={claimTrustedProxyOwner.isPending}
            >
              Claim Owner via Trusted Proxy
            </Button>
          </Show>
        }
      >
        <form.Field name="email">
          {(field) => {
            const fieldError = () => {
              if (
                !field().state.meta.isTouched &&
                submissionAttempts() === 0
              ) {
                return null
              }
              return (field().state.meta.errors[0] as string | undefined) ?? null
            }

            return (
              <FormField label="Owner email" error={fieldError()}>
                <Input
                  value={field().state.value}
                  onInput={(event) => field().handleChange(event.currentTarget.value)}
                  onBlur={field().handleBlur}
                  placeholder="owner@example.com"
                />
              </FormField>
            )
          }}
        </form.Field>
        <Button type="submit" disabled={createLocalOwner.isPending}>
          Create Owner
        </Button>
      </Show>
    </form>
  )
}
