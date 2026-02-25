import { createSignal } from 'solid-js'
import { createForm } from '@tanstack/solid-form'
import { createFileRoute, useNavigate } from '@tanstack/solid-router'
import z from 'zod'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { FormField } from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import { useSetupStore } from '@/stores/setup-store'
import { useSetupTrustedProxyConfigure } from '@/hooks/use-setup'

export const Route = createFileRoute('/setup/trusted-proxy')({
  component: SetupTrustedProxyPage,
})

function SetupTrustedProxyPage() {
  const navigate = useNavigate()
  const configure = useSetupTrustedProxyConfigure()
  const [error, setError] = createSignal<string | null>(null)

  const schema = z.object({
    userEmailHeader: z.string().trim().min(1, 'User email header is required.'),
    trustedCidrs: z
      .string()
      .trim()
      .min(1, 'At least one trusted CIDR is required.')
      .refine(
        (value) =>
          value
            .split(',')
            .map((entry) => entry.trim())
            .filter(Boolean).length > 0,
        'At least one trusted CIDR is required.',
      ),
    sharedSecret: z.string().optional(),
  })

  const form = createForm(() => ({
    defaultValues: {
      userEmailHeader: 'x-auth-request-email',
      trustedCidrs: '127.0.0.1/32',
      sharedSecret: '',
    },
    validators: {
      onSubmit: ({ value }) => {
        const parsed = schema.safeParse(value)
        if (parsed.success) return undefined
        const fields = parsed.error.flatten().fieldErrors
        return {
          fields: {
            userEmailHeader: fields.userEmailHeader?.[0],
            trustedCidrs: fields.trustedCidrs?.[0],
          },
        }
      },
    },
    onSubmit: async ({ value }) => {
      setError(null)
      try {
        await configure.mutateAsync({
          user_email_header: value.userEmailHeader.trim() || undefined,
          trusted_proxy_cidrs: value.trustedCidrs
            .split(',')
            .map((entry) => entry.trim())
            .filter(Boolean),
          shared_secret: value.sharedSecret.trim() || undefined,
        })

        useSetupStore.getState().setCurrentStep(3)
        void navigate({ to: '/setup/owner' })
      } catch (submitError) {
        setError(
          submitError instanceof Error
            ? submitError.message
            : 'Trusted proxy setup failed',
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
        <h2 class="text-lg font-semibold">Trusted Proxy</h2>
        <p class="text-sm text-muted-foreground">
          Configure trusted peers and identity header mapping.
        </p>
      </div>

      {error() ? (
        <Alert variant="destructive">
          <AlertTitle>Trusted proxy configuration failed</AlertTitle>
          <AlertDescription>{error()}</AlertDescription>
        </Alert>
      ) : null}

      <form.Field name="userEmailHeader">
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
            <FormField label="User email header" error={fieldError()}>
              <Input
                value={field().state.value}
                onInput={(event) => field().handleChange(event.currentTarget.value)}
                onBlur={field().handleBlur}
              />
            </FormField>
          )
        }}
      </form.Field>

      <form.Field name="trustedCidrs">
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
            <FormField
              label="Trusted CIDRs (comma separated)"
              error={fieldError()}
            >
              <Input
                value={field().state.value}
                onInput={(event) => field().handleChange(event.currentTarget.value)}
                onBlur={field().handleBlur}
              />
            </FormField>
          )
        }}
      </form.Field>

      <form.Field name="sharedSecret">
        {(field) => (
          <FormField label="Shared secret (optional)">
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
        Save Trusted Proxy Settings
      </Button>
    </form>
  )
}
