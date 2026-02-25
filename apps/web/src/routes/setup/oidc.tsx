import { createSignal } from 'solid-js'
import { createForm } from '@tanstack/solid-form'
import { createFileRoute, useNavigate } from '@tanstack/solid-router'
import z from 'zod'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { FormField } from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import { Spinner } from '@/components/ui/spinner'
import { useConfigureOidc } from '@/hooks/use-setup'
import { useSetupStore } from '@/stores/setup-store'

export const Route = createFileRoute('/setup/oidc')({
  component: SetupOidcPage,
})

function SetupOidcPage() {
  const navigate = useNavigate()
  const configure = useConfigureOidc()

  const [error, setError] = createSignal<string | null>(null)

  const schema = z.object({
    issuerUrl: z
      .string()
      .trim()
      .min(1, 'Issuer URL is required.')
      .url('Issuer URL must be a valid URL.'),
    clientId: z.string().trim().min(1, 'Client ID is required.'),
    clientSecret: z.string().optional(),
  })

  const form = createForm(() => ({
    defaultValues: {
      issuerUrl: '',
      clientId: '',
      clientSecret: '',
    },
    validators: {
      onSubmit: ({ value }) => {
        const parsed = schema.safeParse(value)
        if (parsed.success) return undefined
        const fields = parsed.error.flatten().fieldErrors

        return {
          fields: {
            issuerUrl: fields.issuerUrl?.[0],
            clientId: fields.clientId?.[0],
          },
        }
      },
    },
    onSubmit: async ({ value }) => {
      setError(null)
      try {
        await configure.mutateAsync({
          issuer_url: value.issuerUrl.trim(),
          client_id: value.clientId.trim(),
          client_secret: value.clientSecret.trim() || undefined,
        })

        useSetupStore.getState().setCurrentStep(3)
        void navigate({ to: '/setup/owner' })
      } catch (submitError) {
        setError(
          submitError instanceof Error
            ? submitError.message
            : 'OIDC configuration failed',
        )
      }
    },
  }))

  const submissionAttempts = form.useStore((state) => state.submissionAttempts)

  const handleSubmit = (event: SubmitEvent) => {
    event.preventDefault()
    void form.handleSubmit()
  }

  return (
    <form class="space-y-4" onSubmit={handleSubmit}>
      <div>
        <h2 class="text-lg font-semibold">Configure OIDC</h2>
        <p class="text-sm text-muted-foreground">
          Connect your identity provider for remote authentication.
        </p>
      </div>

      {error() ? (
        <Alert variant="destructive">
          <AlertTitle>OIDC configuration failed</AlertTitle>
          <AlertDescription>{error()}</AlertDescription>
        </Alert>
      ) : null}

      <form.Field name="issuerUrl">
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
            <FormField label="Issuer URL" error={fieldError()}>
              <Input
                value={field().state.value}
                onInput={(event) => field().handleChange(event.currentTarget.value)}
                onBlur={field().handleBlur}
                placeholder="https://accounts.example.com"
              />
            </FormField>
          )
        }}
      </form.Field>

      <form.Field name="clientId">
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
            <FormField label="Client ID" error={fieldError()}>
              <Input
                value={field().state.value}
                onInput={(event) => field().handleChange(event.currentTarget.value)}
                onBlur={field().handleBlur}
                placeholder="oore-web"
              />
            </FormField>
          )
        }}
      </form.Field>

      <form.Field name="clientSecret">
        {(field) => (
          <FormField
            label="Client Secret (optional)"
          >
            <Input
              type="password"
              value={field().state.value}
              onInput={(event) => field().handleChange(event.currentTarget.value)}
              onBlur={field().handleBlur}
            />
          </FormField>
        )}
      </form.Field>

      <Button type="submit" disabled={configure.isPending}>
        {configure.isPending ? <Spinner class="size-4" /> : null}
        Save OIDC Configuration
      </Button>
    </form>
  )
}
