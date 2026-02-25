import { createSignal } from 'solid-js'
import { createForm } from '@tanstack/solid-form'
import { createFileRoute, useNavigate } from '@tanstack/solid-router'
import z from 'zod'
import { ApiClientError } from '@/lib/api'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { FormField } from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import { Spinner } from '@/components/ui/spinner'
import { useVerifyBootstrapToken } from '@/hooks/use-setup'
import { useSetupStore } from '@/stores/setup-store'

export const Route = createFileRoute('/setup/')({
  component: SetupTokenPage,
})

function SetupTokenPage() {
  const navigate = useNavigate()
  const verifyToken = useVerifyBootstrapToken()
  const [error, setError] = createSignal<string | null>(null)

  const tokenSchema = z.string().trim().min(1, 'Bootstrap token is required.')

  const form = createForm(() => ({
    defaultValues: {
      token: '',
    },
    validators: {
      onSubmit: ({ value }) => {
        const parsed = z
          .object({
            token: tokenSchema,
          })
          .safeParse(value)

        if (parsed.success) return undefined

        return {
          fields: {
            token:
              parsed.error.flatten().fieldErrors.token?.[0] ??
              'Bootstrap token is required.',
          },
        }
      },
    },
    onSubmit: async ({ value }) => {
      setError(null)

      try {
        await verifyToken.mutateAsync(value.token.trim())
        useSetupStore.getState().setCurrentStep(1)
        void navigate({ to: '/setup/mode' })
      } catch (submitError) {
        if (submitError instanceof ApiClientError) {
          setError(submitError.message)
        } else {
          setError(
            submitError instanceof Error
              ? submitError.message
              : 'Token verification failed',
          )
        }
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
        <h2 class="text-lg font-semibold">Bootstrap Token</h2>
        <p class="text-sm text-muted-foreground">
          Paste the setup bootstrap token from the backend host.
        </p>
      </div>

      {error() ? (
        <Alert variant="destructive">
          <AlertTitle>Verification failed</AlertTitle>
          <AlertDescription>{error()}</AlertDescription>
        </Alert>
      ) : null}

      <form.Field name="token">
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
            <FormField error={fieldError()}>
              <Input
                value={field().state.value}
                onInput={(event) => field().handleChange(event.currentTarget.value)}
                onBlur={field().handleBlur}
                placeholder="oot_xxx"
              />
            </FormField>
          )
        }}
      </form.Field>

      <Button type="submit" disabled={verifyToken.isPending}>
        {verifyToken.isPending ? <Spinner class="size-4" /> : null}
        Verify Token
      </Button>
    </form>
  )
}
