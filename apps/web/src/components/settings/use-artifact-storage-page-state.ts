import { useMemo, useState } from 'react'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import * as z from 'zod'

import { useMountEffect } from '@/hooks/use-mount-effect'
import { useHasPermission } from '@/hooks/use-permissions'
import {
  useArtifactStorageSettings,
  useUpdateArtifactStorageSettings,
} from '@/hooks/use-artifact-storage'
import { getApiErrorMessage } from '@/lib/api'
import { resolveInstanceApiBaseUrl } from '@/lib/instance-url'
import { toast } from '@/lib/toast'
import { useActiveInstance } from '@/stores/instance-store'

export const preloadArtifactFolderPicker = () =>
  import('@/components/settings/preferences-artifact-folder-picker')

const storageSchema = z
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

type StorageFormInput = z.input<typeof storageSchema>
type StorageFormValues = z.output<typeof storageSchema>

function isLoopbackHostname(hostname: string): boolean {
  return (
    hostname === 'localhost' ||
    hostname === '127.0.0.1' ||
    hostname === '::1' ||
    hostname === '[::1]'
  )
}

function resolveHostname(rawUrl: string | null | undefined): string {
  const trimmed = rawUrl?.trim() ?? ''
  if (!trimmed) return ''
  try {
    return new URL(trimmed).hostname
  } catch {
    return ''
  }
}

export function useArtifactStoragePageState() {
  const [artifactDirPickerOpen, setArtifactDirPickerOpen] = useState(false)
  const canWrite = useHasPermission('instance_settings', 'write')
  const instance = useActiveInstance()
  const instanceApiBaseUrl = resolveInstanceApiBaseUrl(instance)
  const canBrowseLocalFs =
    isLoopbackHostname(window.location.hostname) &&
    isLoopbackHostname(resolveHostname(instanceApiBaseUrl))
  const settingsQuery = useArtifactStorageSettings()
  const updateStorageMutation = useUpdateArtifactStorageSettings()
  const settings = settingsQuery.data

  const storageValues = useMemo<StorageFormInput | undefined>(() => {
    if (!settings) return undefined
    const backend_kind =
      settings.provider === 'disabled'
        ? 'disabled'
        : settings.provider === 'local'
          ? 'local'
          : 'object'
    const object_service =
      settings.provider === 'r2'
        ? 'cloudflare_r2'
        : settings.provider === 's3'
          ? settings.s3_endpoint
            ? 'custom'
            : 'aws_s3'
          : 'aws_s3'

    return {
      backend_kind,
      object_service,
      local_base_dir: settings.local_base_dir ?? '',
      s3_bucket: settings.s3_bucket ?? '',
      s3_region:
        settings.provider === 'r2'
          ? (settings.s3_region ?? 'auto')
          : (settings.s3_region ?? 'us-east-1'),
      s3_endpoint: settings.s3_endpoint ?? '',
      access_key_id: '',
      secret_access_key: '',
    }
  }, [settings])

  const storageForm = useForm<StorageFormInput, unknown, StorageFormValues>({
    resolver: zodResolver(storageSchema),
    defaultValues: {
      backend_kind: 'disabled',
      object_service: 'aws_s3',
      local_base_dir: '',
      s3_bucket: '',
      s3_region: 'us-east-1',
      s3_endpoint: '',
      access_key_id: '',
      secret_access_key: '',
    },
    values: storageValues,
    mode: 'onBlur',
  })

  const backendKind = storageForm.watch('backend_kind')
  const objectService = storageForm.watch('object_service')

  useMountEffect(() => {
    const subscription = storageForm.watch((values, { name }) => {
      if (name !== 'object_service' && name !== 'backend_kind') return
      if (values.backend_kind !== 'object') return
      if (values.object_service === 'cloudflare_r2') {
        if (values.s3_region !== 'auto') {
          storageForm.setValue('s3_region', 'auto', { shouldDirty: true })
        }
      } else if (
        values.object_service === 'aws_s3' &&
        values.s3_region === 'auto'
      ) {
        storageForm.setValue('s3_region', 'us-east-1', { shouldDirty: true })
      }
    })
    return () => subscription.unsubscribe()
  })

  function onSubmitStorage(values: StorageFormValues) {
    const provider =
      values.backend_kind === 'disabled'
        ? 'disabled'
        : values.backend_kind === 'local'
          ? 'local'
          : values.object_service === 'cloudflare_r2'
            ? 'r2'
            : 's3'
    const region =
      values.backend_kind === 'object'
        ? values.object_service === 'cloudflare_r2'
          ? 'auto'
          : values.s3_region?.trim() || 'us-east-1'
        : undefined

    updateStorageMutation.mutate(
      {
        provider,
        local_base_dir:
          values.backend_kind === 'local'
            ? values.local_base_dir?.trim()
            : undefined,
        s3_bucket:
          values.backend_kind === 'object'
            ? values.s3_bucket?.trim()
            : undefined,
        s3_region: values.backend_kind === 'object' ? region : undefined,
        s3_endpoint:
          values.backend_kind === 'object'
            ? values.s3_endpoint?.trim() || undefined
            : undefined,
        access_key_id:
          values.backend_kind === 'object'
            ? values.access_key_id?.trim() || undefined
            : undefined,
        secret_access_key:
          values.backend_kind === 'object'
            ? values.secret_access_key?.trim() || undefined
            : undefined,
      },
      {
        onSuccess: (response) => {
          const label =
            response.settings.provider === 'local'
              ? 'Local filesystem'
              : response.settings.provider === 's3'
                ? 'Object storage'
                : response.settings.provider === 'r2'
                  ? 'Object storage (R2)'
                  : 'Disabled'
          toast.success(`Artifact storage updated: ${label}`)
          storageForm.setValue('access_key_id', '')
          storageForm.setValue('secret_access_key', '')
        },
        onError: (error) => {
          toast.error(
            getApiErrorMessage(error, {
              invalid_local_base_dir:
                'Local base directory must be an absolute path.',
              invalid_s3_bucket: 'Bucket is required for object storage.',
              invalid_s3_endpoint:
                'Endpoint is required for this service preset.',
              missing_s3_credentials:
                'Access key ID and secret access key are required for object storage.',
              encryption_error: 'Failed to store credentials securely.',
              store_error: 'Failed to update artifact storage settings.',
            }),
          )
        },
      },
    )
  }

  return {
    artifactDirPickerOpen,
    backendKind,
    canBrowseLocalFs,
    canWrite,
    objectService,
    onSubmitStorage,
    preloadArtifactFolderPicker,
    setArtifactDirPickerOpen,
    settings,
    settingsQuery,
    storageForm,
    updateStorageMutation,
  }
}

export type ArtifactStoragePageState = ReturnType<
  typeof useArtifactStoragePageState
>
