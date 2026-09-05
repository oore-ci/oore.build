import { createLazyFileRoute, useNavigate } from '@tanstack/react-router'
import { useEffect, useRef, useState } from 'react'
import { useForm, useWatch } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import * as z from 'zod'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import {
  Form,
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import {
  useConfigureOidc,
  useSetupStatus,
  useSetupSummary,
} from '@/hooks/use-setup'
import { useSetupStore } from '@/stores/setup-store'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import { PageMeta } from '@/lib/seo'
import { useSetupModeGuard } from '@/hooks/use-setup-route-transitions'
import { CopyableOidcRedirectUri } from '@/components/setup-oidc-components'
import { SetupStepError } from '@/components/setup-route-components'
import type { OidcConfigureRequest } from '@oore/client/models'

// ── Predefined OIDC providers ──────────────────────────────────

const PROVIDERS = [
  {
    id: 'google',
    label: 'Google',
    issuerUrl: 'https://accounts.google.com',
    locked: true,
    docsPath: '/team/access/oidc/google',
    placeholder: '',
  },
  {
    id: 'microsoft',
    label: 'Microsoft (Entra ID)',
    issuerUrl: '',
    locked: false,
    docsPath: '/team/access/oidc/entra',
    placeholder: 'https://login.microsoftonline.com/{tenant-id}/v2.0',
  },
  {
    id: 'okta',
    label: 'Okta',
    issuerUrl: '',
    locked: false,
    docsPath: '/team/access/oidc/okta',
    placeholder: 'https://{your-domain}.okta.com',
  },
  {
    id: 'auth0',
    label: 'Auth0',
    issuerUrl: '',
    locked: false,
    docsPath: '/team/access/oidc/auth0',
    placeholder: 'https://{your-domain}.auth0.com/',
  },
  {
    id: 'keycloak',
    label: 'Keycloak',
    issuerUrl: '',
    locked: false,
    docsPath: '/team/access/oidc/keycloak',
    placeholder: 'https://{host}/realms/{realm}',
  },
  {
    id: 'custom',
    label: 'Custom / Other',
    issuerUrl: '',
    locked: false,
    docsPath: '/team/access/oidc',
    placeholder: 'https://your-issuer.example.com',
  },
] as const

type ProviderId = (typeof PROVIDERS)[number]['id']

function providerStorageKey(instanceId: string): string {
  return `oore_setup_oidc_provider_${instanceId}`
}

function loadProviderId(instanceId: string): ProviderId | null {
  try {
    const value = sessionStorage.getItem(providerStorageKey(instanceId))
    return PROVIDERS.find((provider) => provider.id === value)?.id ?? null
  } catch {
    return null
  }
}

function saveProviderId(instanceId: string, providerId: ProviderId): void {
  try {
    sessionStorage.setItem(providerStorageKey(instanceId), providerId)
  } catch {
    // Setup still works when sessionStorage is unavailable.
  }
}

function providerIdForIssuer(issuerUrl: string): ProviderId {
  const normalizedIssuer = issuerUrl.replace(/\/+$/, '')
  if (normalizedIssuer === PROVIDERS[0].issuerUrl) return 'google'

  try {
    const issuer = new URL(issuerUrl)
    const hostname = issuer.hostname.toLowerCase()

    if (issuer.pathname.includes('/realms/')) return 'keycloak'
    if (hostname.endsWith('.auth0.com')) return 'auth0'
    if (
      hostname.endsWith('.okta.com') ||
      hostname.endsWith('.oktapreview.com') ||
      hostname.endsWith('.okta-emea.com')
    ) {
      return 'okta'
    }
    if (
      hostname === 'login.microsoftonline.com' ||
      hostname === 'login.microsoftonline.us' ||
      hostname === 'login.chinacloudapi.cn'
    ) {
      return 'microsoft'
    }
  } catch {
    return 'custom'
  }

  return 'custom'
}

// ── Form schema ────────────────────────────────────────────────

const oidcConfigSchema = z.object({
  issuerUrl: z.url('Please enter a valid URL'),
  clientId: z.string().min(1, 'Client ID is required'),
  clientSecret: z.string().optional(),
})

type OidcConfigForm = z.infer<typeof oidcConfigSchema>

// ── Route ──────────────────────────────────────────────────────

export const Route = createLazyFileRoute('/setup/oidc')({
  component: OidcConfigStep,
  errorComponent: SetupStepError,
})

// ── Component ──────────────────────────────────────────────────

function OidcConfigStep() {
  const navigate = useNavigate()
  const sessionToken = useSetupStore((s) => s.sessionToken)
  const configureMutation = useConfigureOidc()
  const { data: status } = useSetupStatus()
  const { data: summary } = useSetupSummary()
  const [selectedProvider, setSelectedProvider] = useState<ProviderId>('google')
  const [removeSavedSecret, setRemoveSavedSecret] = useState(false)
  const [usePublicAuth0Client, setUsePublicAuth0Client] = useState(false)
  const initializedFromSummary = useRef(false)

  const provider =
    PROVIDERS.find((p) => p.id === selectedProvider) ?? PROVIDERS[0]

  const form = useForm<OidcConfigForm>({
    resolver: zodResolver(oidcConfigSchema),
    defaultValues: {
      issuerUrl: PROVIDERS[0].issuerUrl,
      clientId: '',
      clientSecret: '',
    },
    mode: 'onBlur',
  })

  const clientSecret = useWatch({ control: form.control, name: 'clientSecret' })

  const isFormDisabled =
    configureMutation.isPending || configureMutation.isSuccess

  useEffect(() => {
    if (
      initializedFromSummary.current ||
      !summary?.issuer_url ||
      !summary.client_id
    ) {
      return
    }

    initializedFromSummary.current = true
    setSelectedProvider(
      loadProviderId(summary.instance_id) ??
        providerIdForIssuer(summary.issuer_url),
    )
    form.reset({
      issuerUrl: summary.issuer_url,
      clientId: summary.client_id,
      clientSecret: '',
    })
  }, [form, summary])

  const errorMessage = configureMutation.error
    ? getApiErrorMessage(configureMutation.error, {
        oidc_discovery_failed: `OIDC discovery failed: ${configureMutation.error.message}`,
        invalid_state:
          'OIDC settings can only be changed before owner verification is completed.',
        oidc_secret_reentry_required:
          'The issuer or client ID changed. Enter the new client secret, or remove the saved secret for a public client.',
        session_expired:
          'Your setup session has expired. Please go back and re-enter the bootstrap token.',
        invalid_session:
          'Your session is no longer valid. Please restart setup.',
      })
    : null

  const discoveredIssuer = configureMutation.data?.discovered_issuer ?? null

  useSetupModeGuard(status, 'oidc')

  function handleProviderChange(value: ProviderId) {
    setSelectedProvider(() => value)
    setUsePublicAuth0Client(false)
    const nextProvider = PROVIDERS.find((pr) => pr.id === value) ?? PROVIDERS[0]
    if (nextProvider.locked) {
      form.setValue('issuerUrl', nextProvider.issuerUrl, {
        shouldValidate: true,
      })
    } else {
      form.setValue('issuerUrl', '', { shouldValidate: false })
    }
  }

  function onSubmit(data: OidcConfigForm) {
    if (!sessionToken) return
    const issuerUrl = data.issuerUrl.trim()
    const clientId = data.clientId.trim()
    const clientSecret = data.clientSecret?.trim()
    const changedClientIdentity =
      !!summary?.has_client_secret &&
      (summary.issuer_url !== issuerUrl || summary.client_id !== clientId)

    if (changedClientIdentity && !clientSecret && !removeSavedSecret) {
      form.setError('clientSecret', {
        type: 'manual',
        message:
          'Enter the new client secret, or remove the saved secret for a public client.',
      })
      return
    }

    if (
      selectedProvider === 'auth0' &&
      !summary?.has_client_secret &&
      !clientSecret &&
      !usePublicAuth0Client
    ) {
      form.setError('clientSecret', {
        type: 'manual',
        message:
          'Enter the Auth0 client secret, or confirm that this is a public client.',
      })
      return
    }

    const instanceId = summary?.instance_id ?? status?.instance_id

    const oidcData: OidcConfigureRequest = {
      issuer_url: issuerUrl,
      client_id: clientId,
    }
    if (clientSecret) oidcData.client_secret = clientSecret
    if (removeSavedSecret) oidcData.clear_client_secret = true
    configureMutation.mutate(
      { sessionToken, data: oidcData },
      {
        onSuccess: () => {
          if (instanceId) saveProviderId(instanceId, selectedProvider)
          setTimeout(() => {
            void navigate({
              to: '/setup/owner',
              viewTransition: { types: ['setup-forward'] },
            })
          }, 1200)
        },
      },
    )
  }

  return (
    <Form {...form}>
      <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
        <PageMeta title="Setup OIDC" />
        <div className="space-y-1">
          <h2 className="text-lg font-medium">
            Connect your identity provider
          </h2>
          <p className="text-sm text-muted-foreground">
            Enter the issuer URL and client credentials from your OIDC
            application.
          </p>
        </div>

        {/* Redirect URI guidance */}
        <div className="space-y-2 border-b pb-4">
          <div className="space-y-0.5">
            <p className="text-sm font-medium">Redirect URI</p>
            <p className="text-xs text-muted-foreground">
              Allow this exact URL in your identity provider.
            </p>
          </div>
          <CopyableOidcRedirectUri
            uri={`${window.location.origin}/auth/callback`}
          />
        </div>

        {/* Provider selector */}
        <div className="space-y-2">
          <div className="flex items-baseline justify-between gap-3">
            <Label htmlFor="identity-provider">Identity provider</Label>
            <a
              href={`https://docs.oore.build${provider.docsPath}`}
              target="_blank"
              rel="noopener noreferrer"
              className="shrink-0 text-xs text-foreground underline underline-offset-2"
            >
              Setup guide
            </a>
          </div>
          <Select
            value={selectedProvider}
            onValueChange={(value) => {
              const providerId = PROVIDERS.find(
                (candidate) => candidate.id === value,
              )?.id
              if (providerId) handleProviderChange(providerId)
            }}
            disabled={isFormDisabled}
          >
            <SelectTrigger id="identity-provider" className="w-full">
              <SelectValue>{provider.label}</SelectValue>
            </SelectTrigger>
            <SelectContent>
              {PROVIDERS.map((p) => (
                <SelectItem key={p.id} value={p.id}>
                  {p.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <FormField
          control={form.control}
          name="issuerUrl"
          render={({ field }) => (
            <FormItem>
              <FormLabel>Issuer URL</FormLabel>
              <FormControl>
                <Input
                  type="url"
                  placeholder={
                    provider.placeholder || 'https://accounts.google.com'
                  }
                  disabled={isFormDisabled || provider.locked}
                  autoFocus={!provider.locked}
                  {...field}
                />
              </FormControl>
              <FormMessage />
            </FormItem>
          )}
        />

        <FormField
          control={form.control}
          name="clientId"
          render={({ field }) => (
            <FormItem>
              <FormLabel>Client ID</FormLabel>
              <FormControl>
                <Input
                  type="text"
                  placeholder="your-client-id"
                  disabled={isFormDisabled}
                  {...field}
                />
              </FormControl>
              <FormMessage />
            </FormItem>
          )}
        />

        <FormField
          control={form.control}
          name="clientSecret"
          render={({ field }) => (
            <FormItem>
              <FormLabel>Client secret</FormLabel>
              <FormControl>
                <Input
                  type="password"
                  placeholder={
                    summary?.has_client_secret
                      ? 'Enter a new secret to replace the saved one'
                      : 'your-client-secret'
                  }
                  disabled={
                    isFormDisabled || removeSavedSecret || usePublicAuth0Client
                  }
                  {...field}
                  onChange={(event) => {
                    field.onChange(event)
                    if (event.target.value.trim()) {
                      form.clearErrors('clientSecret')
                    }
                  }}
                />
              </FormControl>
              <FormDescription>
                {summary?.has_client_secret
                  ? 'A client secret is saved. Leave this blank to keep it, or enter a new secret to replace it.'
                  : selectedProvider === 'auth0'
                    ? 'Required for the recommended Auth0 Regular Web Application. Oore encrypts it at rest.'
                    : 'Leave this blank only when the provider uses a public client. Oore encrypts it at rest.'}
              </FormDescription>
              <FormMessage />
            </FormItem>
          )}
        />

        {summary?.has_client_secret ? (
          <div className="flex items-start gap-3 py-1">
            <Checkbox
              id="remove-saved-client-secret"
              checked={removeSavedSecret}
              onCheckedChange={(checked) => {
                const remove = checked === true
                setRemoveSavedSecret(remove)
                if (remove) {
                  form.setValue('clientSecret', '')
                  form.clearErrors('clientSecret')
                }
              }}
              disabled={isFormDisabled}
            />
            <div className="space-y-1">
              <Label htmlFor="remove-saved-client-secret">
                Remove the saved client secret
              </Label>
              <p className="text-xs text-muted-foreground">
                Use this only when the new OIDC client is public.
              </p>
            </div>
          </div>
        ) : null}

        {selectedProvider === 'auth0' && !summary?.has_client_secret ? (
          <div className="flex items-start gap-3 py-1">
            <Checkbox
              id="use-public-auth0-client"
              checked={usePublicAuth0Client}
              onCheckedChange={(checked) => {
                const usePublicClient = checked === true
                setUsePublicAuth0Client(usePublicClient)
                if (usePublicClient) form.clearErrors('clientSecret')
              }}
              disabled={isFormDisabled || !!clientSecret?.trim()}
            />
            <div className="space-y-1">
              <Label htmlFor="use-public-auth0-client">
                Use an Auth0 public client without a secret
              </Label>
              <p className="text-xs text-muted-foreground">
                Use this only when Client Authentication Method is None in
                Auth0.
              </p>
            </div>
          </div>
        ) : null}

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
              Verified issuer:{' '}
              <code className="text-xs">{discoveredIssuer}</code>. Proceeding to
              next step...
            </AlertDescription>
          </Alert>
        ) : null}

        <Button
          type="submit"
          disabled={configureMutation.isPending || configureMutation.isSuccess}
          className="w-full"
        >
          {configureMutation.isPending
            ? 'Discovering provider...'
            : configureMutation.isSuccess
              ? 'Configured'
              : 'Configure OIDC'}
        </Button>
      </form>
    </Form>
  )
}
