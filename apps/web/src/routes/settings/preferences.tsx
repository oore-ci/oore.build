import { For, Show, createMemo } from 'solid-js'
import { createFileRoute } from '@tanstack/solid-router'

import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { PageHeader } from '@/components/page-header'
import { PageLayout } from '@/components/page-layout'
import {
  useArtifactStorageSettings,
  useExternalAccessNetworkSettings,
  useExternalAccessPreflight,
  useInstancePreferences,
  useUpdateInstancePreferences,
} from '@/hooks/use-artifact-storage'
import { useHasPermission } from '@/hooks/use-permissions'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { PageMeta } from '@/lib/seo'
import { toast } from '@/components/ui/sonner'

export const Route = createFileRoute('/settings/preferences')({
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  staticData: { breadcrumbLabel: 'Preferences' },
  component: PreferencesPage,
})

function titleCase(input: string): string {
  return input
    .replace(/_/g, ' ')
    .replace(/\b\w/g, (char) => char.toUpperCase())
}

function PreferencesPage() {
  const canWrite = useHasPermission('instance_settings', 'write')

  const preferencesQuery = useInstancePreferences()
  const artifactSettingsQuery = useArtifactStorageSettings()
  const networkSettingsQuery = useExternalAccessNetworkSettings()
  const preflightQuery = useExternalAccessPreflight()
  const updatePreferencesMutation = useUpdateInstancePreferences()

  const runtimeMode = createMemo(
    () => preferencesQuery.data?.preferences.runtime_mode ?? 'local',
  )

  const checks = createMemo(() => preflightQuery.data?.checks ?? [])
  const readyChecks = createMemo(() => checks().filter((check) => check.ok).length)

  const networkCheck = createMemo(() => {
    const candidates = checks().filter((check) =>
      check.id.includes('redirect') ||
      check.id.includes('external_access') ||
      check.id.includes('network'),
    )
    return candidates[0]
  })

  const identityCheck = createMemo(() => {
    const candidates = checks().filter(
      (check) => check.id.includes('oidc') || check.id.includes('identity'),
    )
    return candidates[0]
  })

  const externalAccessEnabled = createMemo(() => runtimeMode() === 'remote')

  const handleEnableExternalAccess = () => {
    const current = preferencesQuery.data?.preferences
    if (!current) return

    updatePreferencesMutation.mutate(
      {
        key_storage_mode: current.key_storage_mode,
        runtime_mode: 'remote',
        remote_auth_mode: current.remote_auth_mode,
      },
      {
        onSuccess: () => {
          toast.success('External Access enabled')
        },
        onError: (error) => {
          toast.error(
            error instanceof Error
              ? error.message
              : 'Failed to enable External Access',
          )
        },
      },
    )
  }

  const externalAccessSummary = createMemo(() => {
    if (externalAccessEnabled()) {
      const url = networkSettingsQuery.data?.settings.public_url
      return url ? `External access active at ${url}` : 'External access is enabled.'
    }

    return 'Local Only is active. Sign-in is limited to localhost on this machine.'
  })

  const artifactBackend = createMemo(() => {
    const provider = artifactSettingsQuery.data?.settings.provider
    if (!provider || provider === 'disabled') return 'Disabled'
    if (provider === 'local') return 'Local filesystem'
    if (provider === 'r2') return 'Cloudflare R2'
    if (provider === 's3') return 'S3-compatible object storage'
    return titleCase(provider)
  })

  return (
    <PageLayout width="wide" class="space-y-4">
      <PageMeta title="Preferences" noindex />
      <PageHeader
        title="Preferences"
        description="Manage artifact storage and External Access policy for this instance."
      />

      <Card>
        <CardHeader>
          <div class="flex items-center justify-between">
            <CardTitle class="text-sm font-medium uppercase tracking-wider text-muted-foreground">
              External Access
            </CardTitle>
            <Badge variant={externalAccessEnabled() ? 'success' : 'outline'}>
              {externalAccessEnabled() ? 'Enabled' : 'Local Only'}
            </Badge>
          </div>
        </CardHeader>

        <CardContent class="space-y-3">
          <div class="border p-3">
            <div class="flex items-center justify-between gap-3">
              <div>
                <p class="text-sm font-medium">Current access</p>
                <p class="text-sm text-muted-foreground">{externalAccessSummary()}</p>
              </div>

              <Show when={!externalAccessEnabled() && canWrite}>
                <Button
                  onClick={handleEnableExternalAccess}
                  disabled={updatePreferencesMutation.isPending}
                >
                  {updatePreferencesMutation.isPending
                    ? 'Applying...'
                    : 'Turn On External Access'}
                </Button>
              </Show>
            </div>
          </div>

          <div class="border p-3">
            <div class="mb-3 flex items-center justify-between">
              <p class="text-sm font-medium uppercase tracking-wider text-muted-foreground">
                Setup Steps
              </p>
              <Badge variant="outline">{readyChecks()}/{checks().length} ready</Badge>
            </div>

            <div class="grid gap-3 md:grid-cols-2">
              <div class="border p-3">
                <div class="flex items-center justify-between gap-2">
                  <p class="text-sm font-semibold">1. Network</p>
                  <Badge variant={networkCheck()?.ok ? 'success' : 'warning'}>
                    {networkCheck()?.ok ? 'Ready' : 'Setup'}
                  </Badge>
                </div>
                <p class="mt-2 text-xs text-muted-foreground">
                  {networkSettingsQuery.data?.settings.public_url ??
                    'http://127.0.0.1:4300'}
                </p>
                <p class="text-xs text-muted-foreground">
                  {(networkSettingsQuery.data?.settings.allowed_origins ?? []).length}{' '}
                  allowed origins
                </p>
                <p class="mt-2 text-xs text-primary">Configure ›</p>
              </div>

              <div class="border p-3">
                <div class="flex items-center justify-between gap-2">
                  <p class="text-sm font-semibold">2. Identity</p>
                  <Badge variant={identityCheck()?.ok ? 'success' : 'warning'}>
                    {identityCheck()?.ok ? 'Ready' : 'Setup'}
                  </Badge>
                </div>
                <p class="mt-2 text-xs text-muted-foreground">
                  {preferencesQuery.data?.preferences.remote_auth_mode === 'oidc'
                    ? 'OIDC provider configured.'
                    : 'Trusted proxy auth configured.'}
                </p>
                <p class="mt-2 text-xs text-primary">Configure ›</p>
              </div>
            </div>
          </div>

          <div class="border p-3">
            <div class="flex items-center justify-between gap-3">
              <div>
                <p class="text-sm font-medium uppercase tracking-wider text-muted-foreground">
                  Technical checks
                </p>
                <p class="text-sm text-muted-foreground">
                  {checks().filter((check) => !check.ok).length} check need attention.
                </p>
              </div>
              <p class="text-xs text-muted-foreground">Show checks</p>
            </div>
            <Show when={checks().length > 0}>
              <div class="mt-3 space-y-2">
                <For each={checks().slice(0, 3)}>
                  {(check) => (
                    <div class="flex items-center justify-between gap-3 border px-2 py-1.5 text-xs">
                      <span>{check.label}</span>
                      <Badge variant={check.ok ? 'success' : 'warning'}>
                        {check.ok ? 'OK' : 'Needs action'}
                      </Badge>
                    </div>
                  )}
                </For>
              </div>
            </Show>
          </div>
        </CardContent>
      </Card>

      <section class="grid gap-3 md:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle class="text-sm font-medium uppercase tracking-wider text-muted-foreground">
              Artifact backend
            </CardTitle>
          </CardHeader>
          <CardContent>
            <p class="text-3xl font-semibold tracking-tight">{artifactBackend()}</p>
            <p class="mt-1 text-sm text-muted-foreground">
              Where build artifacts are stored
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle class="text-sm font-medium uppercase tracking-wider text-muted-foreground">
              Config source
            </CardTitle>
          </CardHeader>
          <CardContent>
            <p class="text-3xl font-semibold tracking-tight">
              {titleCase(artifactSettingsQuery.data?.settings.source ?? 'default')}
            </p>
            <p class="mt-1 text-sm text-muted-foreground">
              Effective settings source
            </p>
          </CardContent>
        </Card>
      </section>

      <Card>
        <CardHeader>
          <CardTitle class="text-sm font-medium uppercase tracking-wider text-muted-foreground">
            Artifact storage
          </CardTitle>
        </CardHeader>
        <CardContent>
          <Alert>
            <AlertDescription>
              Advanced storage and credential editing is preserved in the API. UI controls for this section are being aligned to the legacy multi-step workflow.
            </AlertDescription>
          </Alert>
        </CardContent>
      </Card>
    </PageLayout>
  )
}
