import { lazy, Suspense, useMemo, useState } from 'react'
import { createFileRoute } from '@tanstack/react-router'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'

import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import {
  ArtifactStorageSettings,
  artifactStorageSchema,
} from '@/components/settings/preferences-artifact-storage-settings'
import type {
  ArtifactStorageFormInput,
  ArtifactStorageFormValues,
} from '@/components/settings/preferences-artifact-storage-settings'
import {
  useArtifactStorageSettings,
  useUpdateArtifactStorageSettings,
} from '@/hooks/use-artifact-storage'
import { useHasPermission } from '@/hooks/use-permissions'
import { getApiErrorMessage } from '@/lib/api'
import { isLoopbackHostname, resolveUrlHostname } from '@/lib/connectivity'
import {
  getActiveInstanceOrRedirect,
  requireInstanceRoleOrRedirect,
} from '@/lib/instance-context'
import { resolveInstanceApiBaseUrl } from '@/lib/instance-url'
import { PageMeta } from '@/lib/seo'
import { toast } from '@/lib/toast'
import { useActiveInstance } from '@/stores/instance-store'

const loadArtifactFolderPicker = () =>
  import('@/components/LocalFolderPickerDialog')
const ArtifactFolderPicker = lazy(loadArtifactFolderPicker)

export const Route = createFileRoute('/settings/artifacts')({
  staticData: {
    breadcrumb: {
      title: 'Artifact storage',
    },
  },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireInstanceRoleOrRedirect(instance.id, ['owner', 'admin'])
  },
  component: ArtifactStoragePage,
})

function ArtifactStoragePage() {
  const [artifactDirPickerOpen, setArtifactDirPickerOpen] = useState(false)
  const canWrite = useHasPermission('instance_settings', 'write')
  const instance = useActiveInstance()
  const instanceApiBaseUrl = resolveInstanceApiBaseUrl(instance)
  const canBrowseLocalFs =
    isLoopbackHostname(window.location.hostname) &&
    isLoopbackHostname(resolveUrlHostname(instanceApiBaseUrl))
  const settingsQuery = useArtifactStorageSettings()
  const updateStorageMutation = useUpdateArtifactStorageSettings()
  const settings = settingsQuery.data

  const storageValues = useMemo<ArtifactStorageFormInput | undefined>(() => {
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

  const storageForm = useForm<
    ArtifactStorageFormInput,
    unknown,
    ArtifactStorageFormValues
  >({
    resolver: zodResolver(artifactStorageSchema),
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

  function onSubmitStorage(values: ArtifactStorageFormValues) {
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

  return (
    <PageLayout>
      <PageMeta title="Artifact storage" noindex />
      <PageHeader
        title="Artifact storage"
        description="Choose where build artifact files are stored and manage provider credentials."
      />
      <ArtifactStorageSettings
        backendKind={backendKind}
        canBrowseLocalFs={canBrowseLocalFs}
        canWrite={canWrite}
        error={settingsQuery.error}
        form={storageForm}
        isLoading={settingsQuery.isLoading}
        isSaving={updateStorageMutation.isPending}
        onOpenFolderPicker={() => setArtifactDirPickerOpen(true)}
        onPreloadFolderPicker={() => void loadArtifactFolderPicker()}
        onRetry={() => void settingsQuery.refetch()}
        onSubmit={onSubmitStorage}
        settings={settings}
      />
      {artifactDirPickerOpen ? (
        <Suspense fallback={null}>
          <ArtifactFolderPicker
            open
            onOpenChange={setArtifactDirPickerOpen}
            enabled={canBrowseLocalFs}
            initialPath={storageForm.getValues('local_base_dir')}
            title="Browse artifact folder"
            description="Select a folder on the daemon host where artifact files will be stored."
            selectCurrentLabel="Use current folder"
            selectDirectoryLabel="Select folder"
            onSelectPath={(path) => {
              storageForm.setValue('local_base_dir', path, {
                shouldDirty: true,
                shouldTouch: true,
                shouldValidate: true,
              })
            }}
          />
        </Suspense>
      ) : null}
    </PageLayout>
  )
}
