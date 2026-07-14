import { useParams } from '@tanstack/react-router'

import { BuildDetailPage } from './build-detail-page'

export function BuildDetailRoute() {
  const { buildId } = useParams({ from: '/builds/$buildId' })
  return <BuildDetailPage key={buildId} buildId={buildId} />
}
