import { lazy, Suspense } from 'react'
import { createFileRoute } from '@tanstack/react-router'

import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import { ArtifactStorageSettings } from '@/components/settings/preferences-artifact-storage-settings'
import {
  useArtifactStoragePageState,
  preloadArtifactFolderPicker,
} from '@/components/settings/use-artifact-storage-page-state'
import {
  getActiveInstanceOrRedirect,
  requireInstanceRoleOrRedirect,
} from '@/lib/instance-context'
import { PageMeta } from '@/lib/seo'

const ArtifactFolderPicker = lazy(preloadArtifactFolderPicker)

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
