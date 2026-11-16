import { useState, type ReactNode } from 'react'
import { Link } from '@tanstack/react-router'
import { useForm, useFormState, useWatch } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import * as z from 'zod'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Add01Icon,
  AlertCircleIcon,
  CheckmarkCircle02Icon,
} from '@hugeicons/core-free-icons'

import { getApiErrorMessage } from '@/lib/api-client/api-error'
import { relativeTime } from '@/lib/format-utils'
import { toast } from '@/lib/toast'
import { useAppStoreConnectKeys } from '@/hooks/use-app-store-connect-keys'
import { useHasPermissions } from '@/hooks/use-permissions'
import {
  useConnectAppStore,
  useConnectGooglePlay,
  useDisconnectAppStore,
  useDisconnectGooglePlay,
  useProjectStores,
  useTestAppStoreConnection,
  useTestGooglePlayConnection,
  type ProjectAppStoreConnection,
  type ProjectGooglePlayConnection,
  type ProjectStoreOverride,
} from '@/hooks/use-project-stores'
import { useInheritedSecrets, type SecretTarget } from '@/hooks/use-secrets'
import ConfirmDialog from '@/components/ConfirmDialog'
import { CollectionError } from '@/components/collection'
import { StoreTracks } from '@/components/environments/store-tracks'
import { SecretValueDialog } from '@/components/secrets/secret-value-dialog'
import {
  SettingsSection,
  SettingsSurface,
  SettingsSurfaceSeparator,
} from '@/components/settings/settings-section'
import { AppStoreConnectKeyDialog } from '@/components/store-credentials/app-store-connect-key-dialog'
import { PLAY_SERVICE_ACCOUNT_SECRET } from '@/components/store-credentials/google-play-service-account-panel'
import { UnsavedChangesGuard } from '@/components/unsaved-changes-guard'
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
import { Input } from '@/components/ui/input'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { Spinner } from '@/components/ui/spinner'
import { TabsContent } from '@/components/ui/tabs'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'

const PACKAGE_PATTERN = /^[A-Za-z][A-Za-z0-9_]*(\.[A-Za-z][A-Za-z0-9_]*)+$/
const BUNDLE_PATTERN = /^[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)+$/

const playSchema = z.object({
  service_account_secret: z
    .string()
    .min(1, 'Choose the secret that holds the service account JSON key.'),
  package_name: z
    .string()
    .trim()
    .refine((value) => !value || PACKAGE_PATTERN.test(value), {
      message: 'Enter an application id such as com.example.app.',
    }),
})

const appStoreSchema = z.object({
  app_store_connect_key_id: z
    .string()
    .min(1, 'Choose the App Store Connect key.'),
  bundle_id: z
    .string()
    .trim()
    .refine((value) => !value || BUNDLE_PATTERN.test(value), {
      message: 'Enter a bundle ID such as com.example.app.',
    }),
})

interface CheckResult {
  ok: boolean
  message: string
  checked_at: number
}

/** The latest test: quiet when it passed, red when it failed. */
function CheckLine({
  check,
  error,
}: {
  check: CheckResult | null | undefined
  error: Error | null
}) {
  if (error) {
    return (
      <Alert variant="destructive" aria-live="polite">
        <HugeiconsIcon icon={AlertCircleIcon} />
        <AlertTitle>The test did not run</AlertTitle>
        <AlertDescription>{getApiErrorMessage(error, {})}</AlertDescription>
      </Alert>
    )
  }
  if (!check) return null
  if (!check.ok) {
    return (
      <Alert variant="destructive" aria-live="polite">
        <HugeiconsIcon icon={AlertCircleIcon} />
        <AlertTitle>Connection test failed</AlertTitle>
        <AlertDescription>
          {check.message} Tested {relativeTime(check.checked_at)}.
        </AlertDescription>
      </Alert>
    )
  }
  return (
    <Row gap={2} className="items-start text-sm" aria-live="polite">
      <HugeiconsIcon
        icon={CheckmarkCircle02Icon}
        className="mt-0.5 size-4 shrink-0 text-muted-foreground"
        aria-hidden
      />
      <Text variant="muted">
        {check.message} Tested {relativeTime(check.checked_at)}.
      </Text>
    </Row>
  )
}

/** Pipelines that keep their own credentials, as a footnote. */
function OverridesNote({
  overrides,
  projectId,
}: {
  overrides: Array<ProjectStoreOverride>
  projectId: string
}) {
  if (overrides.length === 0) return null
  return (
    <Text variant="muted" className="text-[13px]">
      Kept their own credentials:{' '}
      {overrides.map((item, index) => (
        <span key={item.pipeline_id}>
          {index > 0 ? ', ' : null}
          <Link
            to="/projects/$projectId/pipelines/$pipelineId"
            params={{ projectId, pipelineId: item.pipeline_id }}
            search={{ tab: 'publishing' }}
            className="underline underline-offset-4"
          >
            {item.pipeline_name}
          </Link>{' '}
          ({item.detail.charAt(0).toLowerCase() + item.detail.slice(1)})
        </span>
      ))}
      .
    </Text>
  )
}

function StoreFooter({
  canWrite,
  connected,
  dirty,
  disconnecting,
  onDisconnect,
  saving,
  test,
}: {
  canWrite: boolean
  connected: boolean
  dirty: boolean
  disconnecting: boolean
  onDisconnect: () => void
  saving: boolean
  test: ReactNode
}) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-2">
      <Row gap={2}>{test}</Row>
      {canWrite ? (
        <Row gap={2} className="ms-auto">
          {connected ? (
            <Button
              type="button"
              variant="ghost"
              disabled={disconnecting}
              onClick={onDisconnect}
            >
              Disconnect
            </Button>
          ) : null}
          <Button type="submit" disabled={saving || (connected && !dirty)}>
            {saving ? <Spinner data-icon="inline-start" /> : null}
            {connected ? 'Save' : 'Connect'}
          </Button>
        </Row>
      ) : null}
    </div>
  )
}

function GooglePlayStore({
  canManageTracks,
  canReadInstanceSecrets,
  canWrite,
  connection,
  overrides,
  projectId,
}: {
  canManageTracks: boolean
  canReadInstanceSecrets: boolean
  canWrite: boolean
  connection: ProjectGooglePlayConnection | undefined
  overrides: Array<ProjectStoreOverride>
  projectId: string
}) {
  const connect = useConnectGooglePlay(projectId)
  const disconnect = useDisconnectGooglePlay(projectId)
  const test = useTestGooglePlayConnection(projectId)
  const [canShareSecrets] = useHasPermissions(['instance_settings:write'])
  const [addSecretOpen, setAddSecretOpen] = useState(false)
  const [confirmOpen, setConfirmOpen] = useState(false)
  const targets: Array<SecretTarget> = [
    { scope: 'project', projectId },
    ...(canReadInstanceSecrets ? [{ scope: 'instance' as const }] : []),
  ]
  const secrets = useInheritedSecrets(targets)
  const names = [...new Set(secrets.map((secret) => secret.name))]
  const defaultSecret = names.includes(PLAY_SERVICE_ACCOUNT_SECRET)
    ? PLAY_SERVICE_ACCOUNT_SECRET
    : ''
  const form = useForm<
    z.input<typeof playSchema>,
    unknown,
    z.output<typeof playSchema>
  >({
    resolver: zodResolver(playSchema),
    values: {
      service_account_secret:
        connection?.service_account_secret ?? defaultSecret,
      package_name: connection?.package_name ?? '',
    },
    resetOptions: { keepDirtyValues: true },
  })
  const { isDirty } = useFormState({ control: form.control })
  const secret = useWatch({
    control: form.control,
    name: 'service_account_secret',
  })
  if (secret && !names.includes(secret)) names.unshift(secret)
  const items = Object.fromEntries(names.map((name) => [name, name]))
  const check = test.data ?? connection?.last_check

  function values() {
    const current = form.getValues()
    return {
      service_account_secret: current.service_account_secret,
      package_name: current.package_name.trim() || null,
    }
  }

  return (
    <SettingsSection
      id="google-play"
      title="Google Play"
      meta={connection ? 'Connected' : 'Not connected'}
      hint="Pipelines of this app upload with this key unless they name their own."
    >
      <Form {...form}>
        <form
          noValidate
          onSubmit={(event) =>
            void form.handleSubmit((next) =>
              connect.mutate(
                {
                  service_account_secret: next.service_account_secret,
                  package_name: next.package_name || null,
                },
                {
                  onSuccess: () => {
                    form.reset()
                    toast.success('Google Play connected')
                  },
                },
              ),
            )(event)
          }
        >
          <fieldset disabled={!canWrite} className="min-w-0">
            <SettingsSurface className="flex flex-col gap-5">
              <div className="grid gap-4 sm:grid-cols-2">
                <FormField
                  control={form.control}
                  name="service_account_secret"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel hint="A secret holding the whole service account JSON key file.">
                        Service account key
                      </FormLabel>
                      <Row gap={2} className="flex-nowrap">
                        <Select
                          value={field.value || null}
                          onValueChange={(next) => field.onChange(next ?? '')}
                          items={items}
                        >
                          <FormControl>
                            <SelectTrigger className="min-w-0 flex-1 font-mono">
                              <SelectValue
                                placeholder={
                                  names.length > 0
                                    ? 'Choose a secret'
                                    : 'No secrets yet'
                                }
                              />
                            </SelectTrigger>
                          </FormControl>
                          <SelectContent>
                            {names.map((name) => (
                              <SelectItem
                                key={name}
                                value={name}
                                className="font-mono"
                              >
                                {name}
                              </SelectItem>
                            ))}
                          </SelectContent>
                        </Select>
                        {canWrite ? (
                          <Button
                            type="button"
                            variant="outline"
                            size="icon"
                            aria-label="Add secret"
                            onClick={() => setAddSecretOpen(true)}
                          >
                            <HugeiconsIcon icon={Add01Icon} />
                          </Button>
                        ) : null}
                      </Row>
                      {connection &&
                      connection.service_account_secret === field.value &&
                      !connection.service_account_secret_present ? (
                        <p className="text-sm text-destructive">
                          No secret named {field.value} is set. Uploads fail
                          until you add it.
                        </p>
                      ) : null}
                      <FormMessage />
                    </FormItem>
                  )}
                />
                <FormField
                  control={form.control}
                  name="package_name"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel hint="A bundle for any other package fails the upload. Empty reads it from the App Bundle.">
                        Package name
                      </FormLabel>
                      <FormControl>
                        <Input
                          className="font-mono"
                          placeholder="com.example.app"
                          autoComplete="off"
                          spellCheck={false}
                          {...field}
                        />
                      </FormControl>
                      <FormMessage />
                    </FormItem>
                  )}
                />
              </div>
              <CheckLine check={check} error={test.error} />
              {connect.error ? (
                <Alert variant="destructive">
                  <HugeiconsIcon icon={AlertCircleIcon} />
                  <AlertTitle>Not saved</AlertTitle>
                  <AlertDescription>
                    {getApiErrorMessage(connect.error, {})}
                  </AlertDescription>
                </Alert>
              ) : null}
              {canWrite ? (
                <>
                  <SettingsSurfaceSeparator />
                  <StoreFooter
                    canWrite={canWrite}
                    connected={!!connection}
                    dirty={isDirty}
                    disconnecting={disconnect.isPending}
                    onDisconnect={() => setConfirmOpen(true)}
                    saving={connect.isPending}
                    test={
                      <Button
                        type="button"
                        variant="outline"
                        disabled={!secret || test.isPending}
                        onClick={() => test.mutate(values())}
                      >
                        {test.isPending ? (
                          <Spinner data-icon="inline-start" />
                        ) : null}
                        Test connection
                      </Button>
                    }
                  />
                </>
              ) : null}
            </SettingsSurface>
          </fieldset>
        </form>
      </Form>
      <OverridesNote overrides={overrides} projectId={projectId} />
      <StoreTracks
        canManage={canManageTracks}
        projectId={projectId}
        store="google_play"
      />
      <UnsavedChangesGuard
        when={isDirty && !connect.isPending}
        title="Discard Google Play changes?"
        description="Your changes to the Google Play connection have not been saved."
      />
      <SecretValueDialog
        open={addSecretOpen}
        onOpenChange={setAddSecretOpen}
        defaultName={PLAY_SERVICE_ACCOUNT_SECRET}
        sharedTarget={canShareSecrets ? { scope: 'instance' } : undefined}
        target={{ scope: 'project', projectId }}
      />
      <ConfirmDialog
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        title="Disconnect Google Play?"
        description="Pipelines that upload with this connection must name their own key first. The secret itself is kept."
        confirmLabel="Disconnect"
        confirmVariant="destructive"
        isPending={disconnect.isPending}
        onConfirm={() =>
          disconnect.mutate(undefined, {
            onSuccess: () => toast.success('Google Play disconnected'),
            onError: (error) => toast.error(getApiErrorMessage(error, {})),
          })
        }
      />
    </SettingsSection>
  )
}

function AppStoreStore({
  canManageTracks,
  canWrite,
  connection,
  overrides,
  projectId,
}: {
  canManageTracks: boolean
  canWrite: boolean
  connection: ProjectAppStoreConnection | undefined
  overrides: Array<ProjectStoreOverride>
  projectId: string
}) {
  const connect = useConnectAppStore(projectId)
  const disconnect = useDisconnectAppStore(projectId)
  const test = useTestAppStoreConnection(projectId)
  const keysQuery = useAppStoreConnectKeys()
  const [canManageKeys] = useHasPermissions(['instance_settings:write'])
  const [addKeyOpen, setAddKeyOpen] = useState(false)
  const [confirmOpen, setConfirmOpen] = useState(false)
  const keys = keysQuery.data ?? []
  const form = useForm<
    z.input<typeof appStoreSchema>,
    unknown,
    z.output<typeof appStoreSchema>
  >({
    resolver: zodResolver(appStoreSchema),
    values: {
      app_store_connect_key_id:
        connection?.key.id ?? (keys.length === 1 ? keys[0]!.id : ''),
      bundle_id: connection?.bundle_id ?? '',
    },
    resetOptions: { keepDirtyValues: true },
  })
  const { isDirty } = useFormState({ control: form.control })
  const keyId = useWatch({
    control: form.control,
    name: 'app_store_connect_key_id',
  })
  const keyLabels = new Map<string, string>()
  if (connection) {
    keyLabels.set(
      connection.key.id,
      `${connection.key.name} (${connection.key.key_id})`,
    )
  }
  for (const key of keys) keyLabels.set(key.id, `${key.name} (${key.key_id})`)
  const items = Object.fromEntries(keyLabels)
  const check = test.data ?? connection?.last_check

  function values() {
    const current = form.getValues()
    return {
      app_store_connect_key_id: current.app_store_connect_key_id,
      bundle_id: current.bundle_id.trim() || null,
    }
  }

  return (
    <SettingsSection
      id="app-store"
      title="App Store Connect"
      meta={connection ? 'Connected' : 'Not connected'}
      hint="Pipelines of this app upload with this key unless they use their signing key."
    >
      <Form {...form}>
        <form
          noValidate
          onSubmit={(event) =>
            void form.handleSubmit((next) =>
              connect.mutate(
                {
                  app_store_connect_key_id: next.app_store_connect_key_id,
                  bundle_id: next.bundle_id || null,
                },
                {
                  onSuccess: () => {
                    form.reset()
                    toast.success('App Store Connect connected')
                  },
                },
              ),
            )(event)
          }
        >
          <fieldset disabled={!canWrite} className="min-w-0">
            <SettingsSurface className="flex flex-col gap-5">
              <div className="grid gap-4 sm:grid-cols-2">
                <FormField
                  control={form.control}
                  name="app_store_connect_key_id"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel hint="Stored once for the instance under Secrets and credentials.">
                        API key
                      </FormLabel>
                      <Row gap={2} className="flex-nowrap">
                        <Select
                          value={field.value || null}
                          onValueChange={(next) => field.onChange(next ?? '')}
                          items={items}
                        >
                          <FormControl>
                            <SelectTrigger className="min-w-0 flex-1">
                              <SelectValue
                                placeholder={
                                  keys.length > 0
                                    ? 'Choose a key'
                                    : 'No keys yet'
                                }
                              />
                            </SelectTrigger>
                          </FormControl>
                          <SelectContent>
                            {[...keyLabels].map(([value, label]) => (
                              <SelectItem key={value} value={value}>
                                {label}
                              </SelectItem>
                            ))}
                          </SelectContent>
                        </Select>
                        {canWrite && canManageKeys ? (
                          <Button
                            type="button"
                            variant="outline"
                            size="icon"
                            aria-label="Add key"
                            onClick={() => setAddKeyOpen(true)}
                          >
                            <HugeiconsIcon icon={Add01Icon} />
                          </Button>
                        ) : null}
                      </Row>
                      {keysQuery.isError ? (
                        <p className="text-sm text-destructive">
                          Could not load the stored keys.
                        </p>
                      ) : null}
                      <FormMessage />
                    </FormItem>
                  )}
                />
                <FormField
                  control={form.control}
                  name="bundle_id"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel hint="The app record the test looks up in App Store Connect.">
                        Bundle ID
                      </FormLabel>
                      <FormControl>
                        <Input
                          className="font-mono"
                          placeholder="com.example.app"
                          autoComplete="off"
                          spellCheck={false}
                          {...field}
                        />
                      </FormControl>
                      <FormMessage />
                    </FormItem>
                  )}
                />
              </div>
              <CheckLine check={check} error={test.error} />
              {connect.error ? (
                <Alert variant="destructive">
                  <HugeiconsIcon icon={AlertCircleIcon} />
                  <AlertTitle>Not saved</AlertTitle>
                  <AlertDescription>
                    {getApiErrorMessage(connect.error, {})}
                  </AlertDescription>
                </Alert>
              ) : null}
              {canWrite ? (
                <>
                  <SettingsSurfaceSeparator />
                  <StoreFooter
                    canWrite={canWrite}
                    connected={!!connection}
                    dirty={isDirty}
                    disconnecting={disconnect.isPending}
                    onDisconnect={() => setConfirmOpen(true)}
                    saving={connect.isPending}
                    test={
                      <Button
                        type="button"
                        variant="outline"
                        disabled={!keyId || test.isPending}
                        onClick={() => test.mutate(values())}
                      >
                        {test.isPending ? (
                          <Spinner data-icon="inline-start" />
                        ) : null}
                        Test connection
                      </Button>
                    }
                  />
                </>
              ) : null}
            </SettingsSurface>
          </fieldset>
        </form>
      </Form>
      <OverridesNote overrides={overrides} projectId={projectId} />
      <StoreTracks
        canManage={canManageTracks}
        projectId={projectId}
        store="app_store"
      />
      <UnsavedChangesGuard
        when={isDirty && !connect.isPending}
        title="Discard App Store Connect changes?"
        description="Your changes to the App Store Connect connection have not been saved."
      />
      <AppStoreConnectKeyDialog
        open={addKeyOpen}
        onOpenChange={setAddKeyOpen}
      />
      <ConfirmDialog
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        title="Disconnect App Store Connect?"
        description="Pipelines go back to the key in their iOS signing settings. The stored key is kept."
        confirmLabel="Disconnect"
        confirmVariant="destructive"
        isPending={disconnect.isPending}
        onConfirm={() =>
          disconnect.mutate(undefined, {
            onSuccess: () => toast.success('App Store Connect disconnected'),
            onError: (error) => toast.error(getApiErrorMessage(error, {})),
          })
        }
      />
    </SettingsSection>
  )
}

/**
 * Where the app's builds go: each store is connected once here with the
 * tracks it releases to, and pipelines only pick a track on their
 * Publishing tab.
 */
export function ProjectStoresTab({
  canManageTracks,
  canReadInstanceSecrets,
  canWrite,
  projectId,
}: {
  /** Maintainers set up the tracks each store releases to. */
  canManageTracks: boolean
  canReadInstanceSecrets: boolean
  canWrite: boolean
  projectId: string
}) {
  const storesQuery = useProjectStores(projectId)
  const stores = storesQuery.data
  const overrides = stores?.overrides ?? []

  return (
    <TabsContent value="stores">
      <Stack gap={8} className="pt-2">
        {storesQuery.error ? (
          <CollectionError
            title="Failed to load the app’s stores"
            description={storesQuery.error.message}
            onRetry={() => void storesQuery.refetch()}
          />
        ) : stores ? (
          <>
            <GooglePlayStore
              canManageTracks={canManageTracks}
              canReadInstanceSecrets={canReadInstanceSecrets}
              canWrite={canWrite}
              connection={stores.google_play ?? undefined}
              overrides={overrides.filter(
                (item) => item.store === 'google_play',
              )}
              projectId={projectId}
            />
            <AppStoreStore
              canManageTracks={canManageTracks}
              canWrite={canWrite}
              connection={stores.app_store ?? undefined}
              overrides={overrides.filter((item) => item.store === 'app_store')}
              projectId={projectId}
            />
          </>
        ) : (
          <Skeleton className="h-64 w-full" />
        )}
      </Stack>
    </TabsContent>
  )
}
