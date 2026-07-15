import { useParams, useSearch } from '@tanstack/react-router'

import { ArtifactInstallPage } from './artifact-install-page'
import { BuildDetailPage } from './build-detail-page'

export function BuildDetailRoute() {
  const { buildId } = useParams({ from: '/builds/$buildId' })
  const { install: artifactId } = useSearch({ from: '/builds/$buildId' })
  if (artifactId) {
    return (
      <ArtifactInstallPage
        key={`${buildId}:${artifactId}`}
        buildId={buildId}
        artifactId={artifactId}
      />
    )
  }
  return <BuildDetailPage key={buildId} buildId={buildId} />
}
