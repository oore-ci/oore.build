import { useParams, useSearch } from '@tanstack/react-router'

import { ArtifactInstallPage } from './artifact-install-page'
import { BuildDetailPage } from './build-detail-page'
import { useAuthStore } from '@/stores/auth-store'

export function BuildDetailRoute() {
  const { buildId } = useParams({ from: '/builds/$buildId' })
  const { install: artifactId } = useSearch({ from: '/builds/$buildId' })
  const isQaViewer = useAuthStore((state) => state.user?.role === 'qa_viewer')
  if (artifactId || isQaViewer) {
    return (
      <ArtifactInstallPage
        key={`${buildId}:${artifactId ?? 'auto'}`}
        buildId={buildId}
        artifactId={artifactId}
      />
    )
  }
  return <BuildDetailPage key={buildId} buildId={buildId} />
}
