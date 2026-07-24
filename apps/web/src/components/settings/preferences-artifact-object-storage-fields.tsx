import type {
  ArtifactStorageForm,
  ArtifactStorageFormInput,
} from '@/components/settings/preferences-artifact-storage-settings'
import type { ArtifactStorageSettings } from '@/lib/types'
import {
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

export function ArtifactObjectStorageFields({
  canWrite,
  form,
  isSaving,
  objectService,
  onObjectServiceChange,
  settings,
}: {
  canWrite: boolean
  form: ArtifactStorageForm
  isSaving: boolean
  objectService: ArtifactStorageFormInput['object_service']
  onObjectServiceChange: (
    service: ArtifactStorageFormInput['object_service'],
  ) => void
  settings: ArtifactStorageSettings
}) {
  return (
    <>
      <FormField
        control={form.control}
        name="object_service"
        render={({ field }) => (
          <FormItem>
            <FormLabel>Service</FormLabel>
            <Select
              value={field.value ?? 'aws_s3'}
              onValueChange={(value) => {
                field.onChange(value)
                onObjectServiceChange(value ?? undefined)
              }}
              disabled={!canWrite || isSaving}
              items={{
                aws_s3: 'AWS S3',
                cloudflare_r2: 'Cloudflare R2',
                minio: 'MinIO',
                custom: 'Custom S3-compatible',
              }}
            >
              <FormControl>
                <SelectTrigger>
                  <SelectValue placeholder="Select a service" />
                </SelectTrigger>
              </FormControl>
              <SelectContent>
                <SelectItem value="aws_s3">AWS S3</SelectItem>
                <SelectItem value="cloudflare_r2">Cloudflare R2</SelectItem>
                <SelectItem value="minio">MinIO</SelectItem>
                <SelectItem value="custom">Custom S3-compatible</SelectItem>
              </SelectContent>
            </Select>
            <FormDescription>
              Presets apply sane defaults (R2 uses region `auto` and requires an
              endpoint).
            </FormDescription>
            <FormMessage />
          </FormItem>
        )}
      />

      <FormField
        control={form.control}
        name="s3_bucket"
        render={({ field }) => (
          <FormItem>
            <FormLabel>Bucket</FormLabel>
            <FormControl>
              <Input
                placeholder="oore-artifacts"
                {...field}
                disabled={!canWrite || isSaving}
              />
            </FormControl>
            <FormDescription>
              Keep this bucket private. Oore serves files via time-limited
              signed URLs.
            </FormDescription>
            <FormMessage />
          </FormItem>
        )}
      />

      <FormField
        control={form.control}
        name="s3_region"
        render={({ field }) => (
          <FormItem>
            <FormLabel>Region</FormLabel>
            <FormControl>
              <Input
                placeholder={
                  objectService === 'cloudflare_r2' ? 'auto' : 'us-east-1'
                }
                {...field}
                disabled={
                  objectService === 'cloudflare_r2' || !canWrite || isSaving
                }
              />
            </FormControl>
            <FormDescription>
              {objectService === 'cloudflare_r2'
                ? 'Cloudflare R2 uses region `auto` for signing.'
                : 'AWS region used for request signing.'}
            </FormDescription>
            <FormMessage />
          </FormItem>
        )}
      />

      <FormField
        control={form.control}
        name="s3_endpoint"
        render={({ field }) => (
          <FormItem>
            <FormLabel>
              {objectService === 'aws_s3'
                ? 'Endpoint (optional)'
                : 'Endpoint (required)'}
            </FormLabel>
            <FormControl>
              <Input
                placeholder={
                  objectService === 'cloudflare_r2'
                    ? 'https://<account-id>.r2.cloudflarestorage.com'
                    : objectService === 'minio'
                      ? 'http://127.0.0.1:9000'
                      : objectService === 'custom'
                        ? 'https://s3.example.com'
                        : 'Leave empty for AWS S3'
                }
                {...field}
                disabled={!canWrite || isSaving}
              />
            </FormControl>
            <FormDescription>
              {objectService === 'aws_s3'
                ? 'Leave empty to use AWS defaults for the selected region.'
                : 'Base URL for the S3-compatible API endpoint.'}
            </FormDescription>
            <FormMessage />
          </FormItem>
        )}
      />

      <FormField
        control={form.control}
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
                disabled={!canWrite || isSaving}
              />
            </FormControl>
            <FormDescription>
              Leave empty to keep the current stored value.
            </FormDescription>
            <FormMessage />
          </FormItem>
        )}
      />

      <FormField
        control={form.control}
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
                disabled={!canWrite || isSaving}
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
  )
}
