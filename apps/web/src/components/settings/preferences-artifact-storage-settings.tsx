import { Folder as Folder02Icon } from 'lucide-react'
import type { UseFormReturn } from 'react-hook-form'
import * as z from 'zod'

import type { ArtifactStorageSettings as ArtifactStorageSettingsValue } from '@/lib/types'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
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
import { Skeleton } from '@/components/ui/skeleton'
import { Spinner } from '@/components/ui/spinner'
import { ArtifactObjectStorageFields } from '@/components/settings/preferences-artifact-object-storage-fields'

export const artifactStorageSchema = z
  .object({
    backend_kind: z.enum(['disabled', 'local', 'object']),
    object_service: z
      .enum(['aws_s3', 'cloudflare_r2', 'minio', 'custom'])
      .optional(),
    local_base_dir: z.string().optional(),
    s3_bucket: z.string().optional(),
    s3_region: z.string().optional(),
    s3_endpoint: z.string().optional(),
    access_key_id: z.string().optional(),
    secret_access_key: z.string().optional(),
  })
  .superRefine((value, context) => {
    const localDir = (value.local_base_dir ?? '').trim()
    const bucket = (value.s3_bucket ?? '').trim()
    const endpoint = (value.s3_endpoint ?? '').trim()
    const service = value.object_service

    if (value.backend_kind === 'local' && !localDir) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['local_base_dir'],
        message: 'Local base directory is required.',
      })
    }

    if (value.backend_kind === 'object') {
      if (!service) {
        context.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['object_service'],
          message: 'Choose an object storage service.',
        })
      }
      if (!bucket) {
        context.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['s3_bucket'],
          message: 'Bucket is required for object storage.',
        })
      }
      if (
        (service === 'cloudflare_r2' ||
          service === 'minio' ||
          service === 'custom') &&
        !endpoint
      ) {
        context.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['s3_endpoint'],
          message: 'Endpoint is required for this service preset.',
        })
      }
    }
  })

export type ArtifactStorageFormInput = z.input<typeof artifactStorageSchema>
export type ArtifactStorageFormValues = z.output<typeof artifactStorageSchema>
export type ArtifactStorageForm = UseFormReturn<
  ArtifactStorageFormInput,
  unknown,
  ArtifactStorageFormValues
>

export function ArtifactStorageSettings({
  backendKind,
  canBrowseLocalFs,
  canWrite,
  error,
  form,
  isLoading,
  isSaving,
  onOpenFolderPicker,
  onPreloadFolderPicker,
  onRetry,
  onSubmit,
  settings,
}: {
  backendKind: ArtifactStorageFormInput['backend_kind']
  canBrowseLocalFs: boolean
  canWrite: boolean
  error: Error | null
  form: ArtifactStorageForm
  isLoading: boolean
  isSaving: boolean
  onOpenFolderPicker: () => void
  onPreloadFolderPicker: () => void
  onRetry: () => void
  onSubmit: (values: ArtifactStorageFormValues) => void
  settings: ArtifactStorageSettingsValue | undefined
}) {
  const objectService = form.watch('object_service')

  function applyRegionDefault(
    service: ArtifactStorageFormInput['object_service'],
  ) {
    const region = form.getValues('s3_region')
    if (service === 'cloudflare_r2' && region !== 'auto') {
      form.setValue('s3_region', 'auto', { shouldDirty: true })
    } else if (service === 'aws_s3' && region === 'auto') {
      form.setValue('s3_region', 'us-east-1', { shouldDirty: true })
    }
  }

  return (
    <section aria-label="Artifact storage configuration" className="space-y-4">
      {isLoading ? (
        <div className="space-y-3 border bg-card p-4">
          <Skeleton className="h-10 w-full" />
          <Skeleton className="h-10 w-full" />
          <Skeleton className="h-10 w-full" />
        </div>
      ) : null}

      {error ? (
        <Alert variant="destructive">
          <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <span>Failed to load artifact settings: {error.message}</span>
            <Button type="button" variant="outline" size="sm" onClick={onRetry}>
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      ) : null}

      {!isLoading && !error && !settings ? (
        <Alert variant="destructive">
          <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <span>The response did not include artifact storage settings.</span>
            <Button type="button" variant="outline" size="sm" onClick={onRetry}>
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      ) : null}

      {!isLoading && !error && settings ? (
        <div className="border bg-card">
          <div className="border-b px-4 py-3">
            <h2 className="text-sm font-semibold">Storage provider</h2>
            <p className="mt-0.5 text-xs text-muted-foreground">
              Files remain unavailable when storage is disabled.
            </p>
          </div>
          <div className="space-y-4 p-4">
            {!canWrite ? (
              <Alert>
                <AlertDescription>
                  You have read access only. Owner/Admin write permission is
                  required to update settings.
                </AlertDescription>
              </Alert>
            ) : null}

            <Form {...form}>
              <form
                onSubmit={form.handleSubmit(onSubmit)}
                className="space-y-4"
              >
                <FormField
                  control={form.control}
                  name="backend_kind"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Backend</FormLabel>
                      <Select
                        value={field.value}
                        onValueChange={(value) => {
                          field.onChange(value)
                          if (value === 'object') {
                            applyRegionDefault(objectService)
                          }
                        }}
                        disabled={!canWrite || isSaving}
                        items={{
                          disabled: 'Disabled',
                          local: 'Local filesystem',
                          object: 'Object storage (S3-compatible)',
                        }}
                      >
                        <FormControl>
                          <SelectTrigger>
                            <SelectValue placeholder="Select backend" />
                          </SelectTrigger>
                        </FormControl>
                        <SelectContent>
                          <SelectItem value="disabled">Disabled</SelectItem>
                          <SelectItem value="local">
                            Local filesystem
                          </SelectItem>
                          <SelectItem value="object">
                            Object storage (S3-compatible)
                          </SelectItem>
                        </SelectContent>
                      </Select>
                      <FormDescription>
                        {backendKind === 'disabled'
                          ? 'Artifacts are recorded as metadata only. Files are not stored, so downloads are unavailable.'
                          : backendKind === 'local'
                            ? 'Artifacts are uploaded to the daemon and stored on local disk (best for single-host setups).'
                            : 'Artifacts are stored in an S3-compatible bucket and served via time-limited download links.'}
                      </FormDescription>
                      <FormMessage />
                    </FormItem>
                  )}
                />

                {backendKind === 'local' ? (
                  <FormField
                    control={form.control}
                    name="local_base_dir"
                    render={({ field }) => (
                      <FormItem>
                        <FormLabel>Local base directory</FormLabel>
                        <div className="flex flex-col gap-2 md:flex-row">
                          <FormControl>
                            <Input
                              placeholder="/absolute/path/to/artifacts"
                              className="font-mono text-xs"
                              {...field}
                              disabled={!canWrite || isSaving}
                            />
                          </FormControl>
                          {canBrowseLocalFs ? (
                            <div className="flex gap-2">
                              <Button
                                type="button"
                                variant="outline"
                                size="icon"
                                aria-label="Browse local base directory"
                                title="Browse local base directory"
                                onMouseEnter={() => onPreloadFolderPicker()}
                                onFocus={() => onPreloadFolderPicker()}
                                onClick={onOpenFolderPicker}
                                disabled={!canWrite || isSaving}
                              >
                                <Folder02Icon size={16} />
                              </Button>
                            </div>
                          ) : null}
                        </div>
                        <FormDescription>
                          Absolute path on the daemon host where artifact files
                          are stored.
                          {!canBrowseLocalFs ? (
                            <>
                              {' '}
                              For security, folder browsing is only available
                              from localhost.
                            </>
                          ) : null}
                        </FormDescription>
                        <FormMessage />
                      </FormItem>
                    )}
                  />
                ) : null}

                {backendKind === 'object' ? (
                  <ArtifactObjectStorageFields
                    canWrite={canWrite}
                    form={form}
                    isSaving={isSaving}
                    objectService={objectService}
                    onObjectServiceChange={applyRegionDefault}
                    settings={settings}
                  />
                ) : null}

                <div className="flex justify-end">
                  <Button type="submit" disabled={!canWrite || isSaving}>
                    {isSaving ? (
                      <>
                        <Spinner className="size-4" />
                        Saving...
                      </>
                    ) : (
                      'Save'
                    )}
                  </Button>
                </div>
              </form>
            </Form>
          </div>
        </div>
      ) : null}
    </section>
  )
}
