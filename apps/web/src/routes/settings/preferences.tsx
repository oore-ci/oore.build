import { useEffect } from 'react'
import { createFileRoute, redirect } from '@tanstack/react-router'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { z } from 'zod'
import { toast } from 'sonner'

import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { useAuthStore } from '@/stores/auth-store'
import { webPageTitle } from '@/lib/seo'
import { useHasPermission } from '@/hooks/use-permissions'
import {
  useArtifactStorageSettings,
  useInstancePreferences,
  useUpdateArtifactStorageSettings,
  useUpdateInstancePreferences,
} from '@/hooks/use-artifact-storage'
import PageLayout from '@/components/page-layout'
import PageHeader from '@/components/page-header'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import {
  Form,
  FormControl,
  FormDescription,
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
import { Checkbox } from '@/components/ui/checkbox'
import { Skeleton } from '@/components/ui/skeleton'
import { Spinner } from '@/components/ui/spinner'

export const Route = createFileRoute('/settings/preferences')({
  staticData: { breadcrumbLabel: 'Preferences' },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)

    const user = useAuthStore.getState().user
    if (!user || (user.role !== 'owner' && user.role !== 'admin')) {
      throw redirect({ to: '/' })
    }
  },
  component: PreferencesPage,
})

const storageSchema = z
  .object({
    provider: z.enum(['disabled', 'local', 's3', 'r2']),
    local_base_dir: z.string().optional(),
    s3_bucket: z.string().optional(),
    s3_region: z.string().optional(),
    s3_endpoint: z.string().optional(),
    access_key_id: z.string().optional(),
    secret_access_key: z.string().optional(),
  })
  .superRefine((value, ctx) => {
    const localDir = (value.local_base_dir ?? '').trim()
    const bucket = (value.s3_bucket ?? '').trim()
    const endpoint = (value.s3_endpoint ?? '').trim()

    if (value.provider === 'local' && !localDir) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['local_base_dir'],
        message: 'Local base directory is required for local storage.',
      })
    }

    if ((value.provider === 's3' || value.provider === 'r2') && !bucket) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['s3_bucket'],
        message: 'Bucket is required for S3/R2 storage.',
      })
    }

    if (value.provider === 'r2' && !endpoint) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['s3_endpoint'],
        message: 'Endpoint is required for R2 storage.',
      })
    }
  })

const keyModeSchema = z.object({
  use_keychain: z.boolean(),
})

type StorageFormValues = z.infer<typeof storageSchema>
type KeyModeFormValues = z.infer<typeof keyModeSchema>

function PreferencesPage() {
  const canWrite = useHasPermission('instance_settings', 'write')
  const settingsQuery = useArtifactStorageSettings()
  const preferencesQuery = useInstancePreferences()
  const updateStorageMutation = useUpdateArtifactStorageSettings()
  const updatePreferencesMutation = useUpdateInstancePreferences()

  const storageForm = useForm<StorageFormValues>({
    resolver: zodResolver(storageSchema),
    defaultValues: {
      provider: 'disabled',
      local_base_dir: '',
      s3_bucket: '',
      s3_region: 'us-east-1',
      s3_endpoint: '',
      access_key_id: '',
      secret_access_key: '',
    },
  })

  const keyModeForm = useForm<KeyModeFormValues>({
    resolver: zodResolver(keyModeSchema),
    defaultValues: {
      use_keychain: true,
    },
  })

  const provider = storageForm.watch('provider')

  useEffect(() => {
    document.title = webPageTitle('Preferences')
  }, [])

  useEffect(() => {
    const settings = settingsQuery.data?.settings
    if (!settings) return

    storageForm.reset({
      provider: settings.provider,
      local_base_dir: settings.local_base_dir ?? '',
      s3_bucket: settings.s3_bucket ?? '',
      s3_region: settings.s3_region ?? 'us-east-1',
      s3_endpoint: settings.s3_endpoint ?? '',
      access_key_id: '',
      secret_access_key: '',
    })
  }, [settingsQuery.data, storageForm])

  useEffect(() => {
    const preferences = preferencesQuery.data?.preferences
    if (!preferences) return
    keyModeForm.reset({
      use_keychain: preferences.key_storage_mode === 'keychain',
    })
  }, [preferencesQuery.data, keyModeForm])

  function onSubmitStorage(values: StorageFormValues) {
    const payload = {
      provider: values.provider,
      local_base_dir: values.provider === 'local' ? values.local_base_dir?.trim() : undefined,
      s3_bucket:
        values.provider === 's3' || values.provider === 'r2'
          ? values.s3_bucket?.trim()
          : undefined,
      s3_region:
        values.provider === 's3' || values.provider === 'r2'
          ? (values.s3_region?.trim() || 'us-east-1')
          : undefined,
      s3_endpoint:
        values.provider === 's3' || values.provider === 'r2'
          ? values.s3_endpoint?.trim() || undefined
          : undefined,
      access_key_id:
        values.provider === 's3' || values.provider === 'r2'
          ? values.access_key_id?.trim() || undefined
          : undefined,
      secret_access_key:
        values.provider === 's3' || values.provider === 'r2'
          ? values.secret_access_key?.trim() || undefined
          : undefined,
    } as const

    updateStorageMutation.mutate(payload, {
      onSuccess: (res) => {
        toast.success(`Artifact storage updated to ${res.settings.provider}`)
        storageForm.setValue('access_key_id', '')
        storageForm.setValue('secret_access_key', '')
      },
      onError: (error) => {
        toast.error(
          error instanceof Error
            ? error.message
            : 'Failed to update artifact storage settings',
        )
      },
    })
  }

  function onSubmitPreferences(values: KeyModeFormValues) {
    updatePreferencesMutation.mutate(
      {
        key_storage_mode: values.use_keychain ? 'keychain' : 'file',
      },
      {
      onSuccess: (res) => {
        toast.success(
          `Security preference updated (${res.preferences.key_storage_mode}). Restart daemon to apply startup mode.`,
        )
      },
      onError: (error) => {
        toast.error(
          error instanceof Error
            ? error.message
            : 'Failed to update instance preferences',
        )
      },
    },
    )
  }

  const settings = settingsQuery.data?.settings
  const preferences = preferencesQuery.data?.preferences

  return (
    <PageLayout width="wide">
      <PageHeader
        title="Preferences"
        description="Manage artifact storage and security defaults for this instance."
      />

      <section className="grid gap-4 md:grid-cols-3">
        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium">Artifact provider</CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-2xl font-semibold tracking-tight">
              {settings?.provider ?? 'disabled'}
            </p>
            <p className="text-xs text-muted-foreground">Current artifact backend</p>
          </CardContent>
        </Card>
        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium">Config source</CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-2xl font-semibold tracking-tight">
              {settings?.source ?? 'default'}
            </p>
            <p className="text-xs text-muted-foreground">Database, environment, or default</p>
          </CardContent>
        </Card>
        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium">Key storage mode</CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-2xl font-semibold tracking-tight">
              {preferences?.key_storage_mode ?? 'keychain'}
            </p>
            <p className="text-xs text-muted-foreground">
              Startup secret-key source for encryption-at-rest
            </p>
          </CardContent>
        </Card>
      </section>

      {settingsQuery.isLoading || preferencesQuery.isLoading ? (
        <Card>
          <CardContent className="space-y-3">
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
          </CardContent>
        </Card>
      ) : null}

      {settingsQuery.error ? (
        <Alert variant="destructive">
          <AlertDescription>
            Failed to load artifact settings: {settingsQuery.error.message}
          </AlertDescription>
        </Alert>
      ) : null}

      {preferencesQuery.error ? (
        <Alert variant="destructive">
          <AlertDescription>
            Failed to load instance preferences: {preferencesQuery.error.message}
          </AlertDescription>
        </Alert>
      ) : null}

      {!settingsQuery.isLoading && !settingsQuery.error ? (
        <Card>
          <CardHeader>
            <CardTitle className="text-base">Artifact Storage</CardTitle>
          </CardHeader>
          <CardContent>
            {!canWrite ? (
              <Alert>
                <AlertDescription>
                  You have read access only. Owner/Admin write permission is required to update
                  settings.
                </AlertDescription>
              </Alert>
            ) : null}

            <Form {...storageForm}>
              <form
                onSubmit={storageForm.handleSubmit(onSubmitStorage)}
                className="space-y-4"
              >
                <FormField
                  control={storageForm.control}
                  name="provider"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Provider</FormLabel>
                      <Select
                        value={field.value}
                        onValueChange={field.onChange}
                        disabled={!canWrite || updateStorageMutation.isPending}
                      >
                        <FormControl>
                          <SelectTrigger>
                            <SelectValue placeholder="Select provider" />
                          </SelectTrigger>
                        </FormControl>
                        <SelectContent>
                          <SelectItem value="disabled">Disabled</SelectItem>
                          <SelectItem value="local">Local filesystem</SelectItem>
                          <SelectItem value="s3">S3-compatible (AWS/MinIO)</SelectItem>
                          <SelectItem value="r2">Cloudflare R2</SelectItem>
                        </SelectContent>
                      </Select>
                      <FormDescription>
                        {provider === 'disabled'
                          ? 'Artifacts remain in metadata only. Binary downloads are unavailable.'
                          : provider === 'local'
                            ? 'Runner uploads artifact binaries to this daemon and stores files on local disk.'
                            : 'Runner uploads binaries using pre-signed S3-compatible URLs.'}
                      </FormDescription>
                      <FormMessage />
                    </FormItem>
                  )}
                />

                {provider === 'local' ? (
                  <FormField
                    control={storageForm.control}
                    name="local_base_dir"
                    render={({ field }) => (
                      <FormItem>
                        <FormLabel>Local base directory</FormLabel>
                        <FormControl>
                          <Input
                            placeholder="/Users/arya/Library/Application Support/oore/artifacts"
                            {...field}
                            disabled={!canWrite || updateStorageMutation.isPending}
                          />
                        </FormControl>
                        <FormDescription>
                          Absolute path on the daemon host where artifact files are stored.
                        </FormDescription>
                        <FormMessage />
                      </FormItem>
                    )}
                  />
                ) : null}

                {provider === 's3' || provider === 'r2' ? (
                  <>
                    <FormField
                      control={storageForm.control}
                      name="s3_bucket"
                      render={({ field }) => (
                        <FormItem>
                          <FormLabel>Bucket</FormLabel>
                          <FormControl>
                            <Input
                              placeholder="oore-artifacts"
                              {...field}
                              disabled={!canWrite || updateStorageMutation.isPending}
                            />
                          </FormControl>
                          <FormMessage />
                        </FormItem>
                      )}
                    />

                    <FormField
                      control={storageForm.control}
                      name="s3_region"
                      render={({ field }) => (
                        <FormItem>
                          <FormLabel>Region</FormLabel>
                          <FormControl>
                            <Input
                              placeholder="us-east-1"
                              {...field}
                              disabled={!canWrite || updateStorageMutation.isPending}
                            />
                          </FormControl>
                          <FormMessage />
                        </FormItem>
                      )}
                    />

                    <FormField
                      control={storageForm.control}
                      name="s3_endpoint"
                      render={({ field }) => (
                        <FormItem>
                          <FormLabel>Endpoint (optional for S3, required for R2)</FormLabel>
                          <FormControl>
                            <Input
                              placeholder="https://<account-id>.r2.cloudflarestorage.com"
                              {...field}
                              disabled={!canWrite || updateStorageMutation.isPending}
                            />
                          </FormControl>
                          <FormMessage />
                        </FormItem>
                      )}
                    />

                    <FormField
                      control={storageForm.control}
                      name="access_key_id"
                      render={({ field }) => (
                        <FormItem>
                          <FormLabel>Access key ID</FormLabel>
                          <FormControl>
                            <Input
                              placeholder={
                                settings?.has_access_key_id
                                  ? 'Stored (leave empty to keep current key)'
                                  : 'Enter access key'
                              }
                              {...field}
                              disabled={!canWrite || updateStorageMutation.isPending}
                            />
                          </FormControl>
                          <FormDescription>
                            Leave empty to keep current stored value.
                          </FormDescription>
                          <FormMessage />
                        </FormItem>
                      )}
                    />

                    <FormField
                      control={storageForm.control}
                      name="secret_access_key"
                      render={({ field }) => (
                        <FormItem>
                          <FormLabel>Secret access key</FormLabel>
                          <FormControl>
                            <Input
                              type="password"
                              placeholder={
                                settings?.has_secret_access_key
                                  ? 'Stored (leave empty to keep current key)'
                                  : 'Enter secret key'
                              }
                              {...field}
                              disabled={!canWrite || updateStorageMutation.isPending}
                            />
                          </FormControl>
                          <FormDescription>
                            Stored encrypted at rest using the daemon encryption key.
                          </FormDescription>
                          <FormMessage />
                        </FormItem>
                      )}
                    />
                  </>
                ) : null}

                <div className="flex justify-end">
                  <Button
                    type="submit"
                    disabled={!canWrite || updateStorageMutation.isPending}
                  >
                    {updateStorageMutation.isPending ? (
                      <>
                        <Spinner className="size-4" />
                        Saving...
                      </>
                    ) : (
                      'Save artifact settings'
                    )}
                  </Button>
                </div>
              </form>
            </Form>
          </CardContent>
        </Card>
      ) : null}

      {!preferencesQuery.isLoading && !preferencesQuery.error ? (
        <Card>
          <CardHeader>
            <CardTitle className="text-base">Security Preferences</CardTitle>
          </CardHeader>
          <CardContent>
            <Form {...keyModeForm}>
              <form
                onSubmit={keyModeForm.handleSubmit(onSubmitPreferences)}
                className="space-y-4"
              >
                <FormField
                  control={keyModeForm.control}
                  name="use_keychain"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Encryption key storage mode</FormLabel>
                      <FormControl>
                        <label className="flex items-center gap-3 rounded-md border p-3">
                          <Checkbox
                            checked={field.value}
                            onCheckedChange={(checked) =>
                              field.onChange(Boolean(checked))
                            }
                            disabled={!canWrite || updatePreferencesMutation.isPending}
                          />
                          <span className="text-sm">
                            Use macOS Keychain for encryption key storage (recommended)
                          </span>
                        </label>
                      </FormControl>
                      <FormDescription>
                        Turn off to use legacy file mode instead.
                      </FormDescription>
                      <FormMessage />
                    </FormItem>
                  )}
                />

                <Alert>
                  <AlertDescription>
                    Restart the daemon after saving to ensure startup uses the selected key storage
                    mode.
                  </AlertDescription>
                </Alert>

                <div className="flex justify-end">
                  <Button
                    type="submit"
                    disabled={!canWrite || updatePreferencesMutation.isPending}
                  >
                    {updatePreferencesMutation.isPending ? (
                      <>
                        <Spinner className="size-4" />
                        Saving...
                      </>
                    ) : (
                      'Save security preferences'
                    )}
                  </Button>
                </div>
              </form>
            </Form>
          </CardContent>
        </Card>
      ) : null}
    </PageLayout>
  )
}
