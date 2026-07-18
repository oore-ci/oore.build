import { lazy, Suspense } from 'react'
import { createFileRoute, redirect } from '@tanstack/react-router'

import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import { ArtifactStorageSettings } from '@/components/settings/preferences-artifact-storage-settings'
import {
  useArtifactStoragePageState,
  preloadArtifactFolderPicker,
} from '@/components/settings/use-artifact-storage-page-state'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { PageMeta } from '@/lib/seo'
import { useAuthStore } from '@/stores/auth-store'

const ArtifactFolderPicker = lazy(preloadArtifactFolderPicker)

export const Route = createFileRoute('/settings/artifacts')({
  staticData: {
    breadcrumb: {
      title: 'Artifact storage',
    },
  },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)

    const user = useAuthStore.getState().user
    if (!user || (user.role !== 'owner' && user.role !== 'admin')) {
      throw redirect({ to: '/' })
    }
  },
  component: ArtifactStoragePage,
})

function ArtifactStoragePage() {
  const state = useArtifactStoragePageState()

  return (
    <PageLayout>
      <PageMeta title="Artifact storage" noindex />
      <PageHeader
        title="Artifact storage"
        description="Choose where build artifact files are stored and manage provider credentials."
      />
      <ArtifactStorageSettings state={state} />
      {state.artifactDirPickerOpen ? (
        <Suspense fallback={null}>
          <ArtifactFolderPicker state={state} />
        </Suspense>
      ) : null}
    </PageLayout>
  )
}
