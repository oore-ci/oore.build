import { Suspense, lazy } from 'react'
import { useParams, useSearch } from '@tanstack/react-router'

import PageLayout from '@/components/page-layout'
import { Skeleton } from '@/components/ui/skeleton'
import { useAuthStore } from '@/stores/auth-store'

const ArtifactInstallPage = lazy(() =>
  import('./artifact-install-page').then((module) => ({
    default: module.ArtifactInstallPage,
  })),
)
const BuildDetailPage = lazy(() =>
  import('./build-detail-page').then((module) => ({
    default: module.BuildDetailPage,
  })),
)

function BuildDetailFallback() {
  return (
    <PageLayout width="full">
      <Skeleton className="h-8 w-56" />
      <Skeleton className="h-24 w-full" />
      <Skeleton className="h-64 w-full" />
    </PageLayout>
  )
}

export function BuildDetailRoute() {
  const { buildId } = useParams({ from: '/builds/$buildId' })
  const { install: artifactId } = useSearch({ from: '/builds/$buildId' })
  const isQaViewer = useAuthStore((state) => state.user?.role === 'qa_viewer')
  if (artifactId || isQaViewer) {
    return (
      <Suspense fallback={<BuildDetailFallback />}>
        <ArtifactInstallPage
          key={`${buildId}:${artifactId ?? 'auto'}`}
          buildId={buildId}
          artifactId={artifactId}
        />
      </Suspense>
    )
  }
  return (
    <Suspense fallback={<BuildDetailFallback />}>
      <BuildDetailPage key={buildId} buildId={buildId} />
    </Suspense>
  )
}
