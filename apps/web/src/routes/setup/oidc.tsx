import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { useEffect } from 'react'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { z } from 'zod'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { useConfigureOidc } from '@/hooks/use-setup'
import { useSetupStore } from '@/stores/setup-store'
import { getApiErrorMessage } from '@/lib/api'
import { webPageTitle } from '@/lib/seo'
import {
  getActiveInstanceOrRedirect,
  requireSetupSessionOrRedirect,
} from '@/lib/instance-context'

const oidcConfigSchema = z.object({
  issuerUrl: z.url('Please enter a valid URL'),
  clientId: z.string().min(1, 'Client ID is required'),
  clientSecret: z.string().optional(),
})

type OidcConfigForm = z.infer<typeof oidcConfigSchema>

export const Route = createFileRoute('/setup/oidc')({
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireSetupSessionOrRedirect(instance.id)
  },
  component: OidcConfigStep,
  errorComponent: OidcConfigError,
})

function OidcConfigError({ error }: { error: Error }) {
  return (
    <div className="space-y-4">
      <Alert variant="destructive">
        <AlertTitle>Something went wrong</AlertTitle>
        <AlertDescription>{error.message}</AlertDescription>
      </Alert>
    </div>
  )
}

function OidcConfigStep() {
  const navigate = useNavigate()
  const sessionToken = useSetupStore((s) => s.sessionToken)
  const setCurrentStep = useSetupStore((s) => s.setCurrentStep)
  const configureMutation = useConfigureOidc()

  const {
    register,
    handleSubmit,
    formState: { errors, isValid },
  } = useForm<OidcConfigForm>({
    resolver: zodResolver(oidcConfigSchema),
    defaultValues: { issuerUrl: '', clientId: '', clientSecret: '' },
    mode: 'onBlur',
  })

  const errorMessage = configureMutation.error
    ? getApiErrorMessage(configureMutation.error, {
        oidc_discovery_failed: `OIDC discovery failed: ${configureMutation.error.message}`,
        invalid_state: 'OIDC has already been configured for this instance.',
        session_expired:
          'Your setup session has expired. Please go back and re-enter the bootstrap token.',
        invalid_session:
          'Your session is no longer valid. Please restart setup.',
      })
    : null

  const discoveredIssuer = configureMutation.data?.discovered_issuer ?? null

  useEffect(() => {
    document.title = webPageTitle('Setup OIDC')
  }, [])

  useEffect(() => {
    setCurrentStep(1)
  }, [setCurrentStep])

  function onSubmit(data: OidcConfigForm) {
    if (!sessionToken) return
    configureMutation.mutate(
      {
        sessionToken,
        data: {
          issuer_url: data.issuerUrl.trim(),
          client_id: data.clientId.trim(),
          ...(data.clientSecret?.trim()
            ? { client_secret: data.clientSecret.trim() }
            : {}),
        },
      },
      {
        onSuccess: () => {
          // Small delay so the user sees the discovery success message
          setTimeout(() => {
            setCurrentStep(2)
            void navigate({ to: '/setup/owner' })
          }, 1200)
        },
      },
    )
  }

  return (
    <form onSubmit={handleSubmit(onSubmit)} className="space-y-4">
      <div className="space-y-1">
        <h2 className="text-lg font-medium">OIDC Provider</h2>
        <p className="text-sm text-muted-foreground">
          Configure the OpenID Connect provider for authentication. You will
          need a client ID (and optionally secret) from your identity provider.
        </p>
        <p className="text-xs text-amber-600 dark:text-amber-400">
          Double-check your values before submitting — this step cannot be
          undone.
        </p>
      </div>

      <div className="space-y-2">
        <Label htmlFor="issuer-url">Issuer URL</Label>
        <Input
          id="issuer-url"
          type="url"
          placeholder="https://accounts.google.com"
          {...register('issuerUrl')}
          disabled={configureMutation.isPending || configureMutation.isSuccess}
          autoFocus
        />
        {errors.issuerUrl ? (
          <p className="text-sm text-destructive">{errors.issuerUrl.message}</p>
        ) : null}
      </div>

      <div className="space-y-2">
        <Label htmlFor="client-id">Client ID</Label>
        <Input
          id="client-id"
          type="text"
          placeholder="your-client-id"
          {...register('clientId')}
          disabled={configureMutation.isPending || configureMutation.isSuccess}
        />
        {errors.clientId ? (
          <p className="text-sm text-destructive">{errors.clientId.message}</p>
        ) : null}
      </div>

      <div className="space-y-2">
        <Label htmlFor="client-secret">
          Client Secret{' '}
          <span className="text-muted-foreground font-normal">(optional)</span>
        </Label>
        <Input
          id="client-secret"
          type="password"
          placeholder="your-client-secret"
          {...register('clientSecret')}
          disabled={configureMutation.isPending || configureMutation.isSuccess}
        />
      </div>

      {errorMessage ? (
        <Alert variant="destructive">
          <AlertTitle>Configuration failed</AlertTitle>
          <AlertDescription>{errorMessage}</AlertDescription>
        </Alert>
      ) : null}

      {discoveredIssuer ? (
        <Alert>
          <AlertTitle>Discovery successful</AlertTitle>
          <AlertDescription>
            Verified issuer: <code className="text-xs">{discoveredIssuer}</code>
            . Proceeding to next step...
          </AlertDescription>
        </Alert>
      ) : null}

      <Button
        type="submit"
        disabled={
          !isValid || configureMutation.isPending || configureMutation.isSuccess
        }
        className="w-full"
      >
        {configureMutation.isPending
          ? 'Discovering provider...'
          : configureMutation.isSuccess
            ? 'Configured'
            : 'Configure OIDC'}
      </Button>
    </form>
  )
}
