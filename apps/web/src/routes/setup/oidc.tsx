import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { useCallback, useState } from 'react'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import z from 'zod'
import { HugeiconsIcon } from '@hugeicons/react'
import { Copy01Icon, Tick02Icon } from '@hugeicons/core-free-icons'
import { useMountEffect } from '@/hooks/use-mount-effect'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { useConfigureOidc, useSetupStatus } from '@/hooks/use-setup'
import { useSetupStore } from '@/stores/setup-store'
import { getApiErrorMessage } from '@/lib/api'
import { PageMeta } from '@/lib/seo'
import {
  getActiveInstanceOrRedirect,
  requireSetupSessionOrRedirect,
} from '@/lib/instance-context'

// ── Predefined OIDC providers ──────────────────────────────────

const PROVIDERS = [
  {
    id: 'google',
    label: 'Google',
    issuerUrl: 'https://accounts.google.com',
    locked: true,
    docsPath: '/guides/oidc/google',
    placeholder: '',
  },
  {
    id: 'microsoft',
    label: 'Microsoft (Entra ID)',
    issuerUrl: '',
    locked: false,
    docsPath: '/guides/oidc/azure-ad',
    placeholder: 'https://login.microsoftonline.com/{tenant-id}/v2.0',
  },
  {
    id: 'okta',
    label: 'Okta',
    issuerUrl: '',
    locked: false,
    docsPath: '/guides/oidc/okta',
    placeholder: 'https://{your-domain}.okta.com',
  },
  {
    id: 'auth0',
    label: 'Auth0',
    issuerUrl: '',
    locked: false,
    docsPath: '/guides/oidc/auth0',
    placeholder: 'https://{your-domain}.auth0.com',
  },
  {
    id: 'keycloak',
    label: 'Keycloak',
    issuerUrl: '',
    locked: false,
    docsPath: '/guides/oidc/keycloak',
    placeholder: 'https://{host}/realms/{realm}',
  },
  {
    id: 'custom',
    label: 'Custom / Other',
    issuerUrl: '',
    locked: false,
    docsPath: '/guides/oidc',
    placeholder: 'https://your-issuer.example.com',
  },
] as const

type ProviderId = (typeof PROVIDERS)[number]['id']

// ── Form schema ────────────────────────────────────────────────

const oidcConfigSchema = z.object({
  issuerUrl: z.url('Please enter a valid URL'),
  clientId: z.string().min(1, 'Client ID is required'),
  clientSecret: z.string().optional(),
})

type OidcConfigForm = z.infer<typeof oidcConfigSchema>

// ── Route ──────────────────────────────────────────────────────

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

// ── CopyableUri ────────────────────────────────────────────────

function CopyableUri({ uri }: { uri: string }) {
  const [copied, setCopied] = useState(false)

  const handleCopy = useCallback(() => {
    void navigator.clipboard.writeText(uri).then(() => {
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    })
  }, [uri])

  return (
    <div className="flex items-center gap-2 bg-muted px-3 py-2">
      <code className="flex-1 text-xs font-mono break-all">{uri}</code>
      <button
        type="button"
        onClick={handleCopy}
        className="shrink-0 text-muted-foreground hover:text-foreground transition-colors"
        aria-label="Copy redirect URI"
      >
        {copied ? (
          <HugeiconsIcon icon={Tick02Icon} size={14} className="text-primary" />
        ) : (
          <HugeiconsIcon icon={Copy01Icon} size={14} />
        )}
      </button>
    </div>
  )
}

// ── Component ──────────────────────────────────────────────────

function OidcConfigStep() {
  const navigate = useNavigate()
  const sessionToken = useSetupStore((s) => s.sessionToken)
  const setCurrentStep = useSetupStore((s) => s.setCurrentStep)
  const configureMutation = useConfigureOidc()
  const { data: status } = useSetupStatus()
  const [selectedProvider, setSelectedProvider] = useState<ProviderId>('google')

  const provider =
    PROVIDERS.find((p) => p.id === selectedProvider) ?? PROVIDERS[0]

  const {
    register,
    handleSubmit,
    setValue,
    formState: { errors, isValid },
  } = useForm<OidcConfigForm>({
    resolver: zodResolver(oidcConfigSchema),
    defaultValues: {
      issuerUrl: PROVIDERS[0].issuerUrl,
      clientId: '',
      clientSecret: '',
    },
    mode: 'onBlur',
  })

  const isFormDisabled =
    configureMutation.isPending || configureMutation.isSuccess

  const errorMessage = configureMutation.error
    ? getApiErrorMessage(configureMutation.error, {
        oidc_discovery_failed: `OIDC discovery failed: ${configureMutation.error.message}`,
        invalid_state:
          'OIDC settings can only be changed before owner verification is completed.',
        session_expired:
          'Your setup session has expired. Please go back and re-enter the bootstrap token.',
        invalid_session:
          'Your session is no longer valid. Please restart setup.',
      })
    : null

  const discoveredIssuer = configureMutation.data?.discovered_issuer ?? null

  useMountEffect(() => {
    setCurrentStep(2)
  })

  useMountEffect(() => {
    if (!status) return
    if (
      status.runtime_mode !== 'remote' ||
      status.remote_auth_mode !== 'oidc'
    ) {
      void navigate({ to: '/setup/mode' })
    }
  })

  function handleProviderChange(value: ProviderId) {
    setSelectedProvider(value)
    const nextProvider = PROVIDERS.find((pr) => pr.id === value) ?? PROVIDERS[0]
    if (nextProvider.locked) {
      setValue('issuerUrl', nextProvider.issuerUrl, { shouldValidate: true })
    } else {
      setValue('issuerUrl', '', { shouldValidate: false })
    }
  }

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
      <PageMeta title="Setup OIDC" />
      <div className="space-y-1">
        <h2 className="text-lg font-medium">OIDC Provider</h2>
        <p className="text-sm text-muted-foreground">
          Configure the OpenID Connect provider for authentication. You will
          need a client ID (and optionally secret) from your identity provider.
        </p>
        <p className="text-xs text-amber-600 dark:text-amber-400">
          You can edit this until owner verification is completed. After owner
          verification, setup can only move forward to finalize.
        </p>
      </div>

      {/* Redirect URI guidance */}
      <Alert>
        <AlertTitle>
          Configure this redirect URI in your identity provider
        </AlertTitle>
        <AlertDescription>
          <p className="text-sm text-muted-foreground mb-2">
            Add this as an authorized redirect URI when creating your OAuth app:
          </p>
          <CopyableUri uri={`${window.location.origin}/auth/callback`} />
        </AlertDescription>
      </Alert>

      {/* Provider selector */}
      <div className="space-y-2">
        <Label>Identity Provider</Label>
        <Select
          value={selectedProvider}
          onValueChange={(v) => handleProviderChange(v as ProviderId)}
          disabled={isFormDisabled}
        >
          <SelectTrigger className="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {PROVIDERS.map((p) => (
              <SelectItem key={p.id} value={p.id}>
                {p.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <a
          href={`https://docs.oore.build${provider.docsPath}`}
          target="_blank"
          rel="noopener noreferrer"
          className="text-xs text-primary underline underline-offset-2"
        >
          How to set up {provider.label} for Oore CI
        </a>
      </div>

      <div className="space-y-2">
        <Label htmlFor="issuer-url">Issuer URL</Label>
        <Input
          id="issuer-url"
          type="url"
          placeholder={provider.placeholder || 'https://accounts.google.com'}
          {...register('issuerUrl')}
          disabled={isFormDisabled || provider.locked}
          autoFocus={!provider.locked}
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
          disabled={isFormDisabled}
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
          disabled={isFormDisabled}
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
