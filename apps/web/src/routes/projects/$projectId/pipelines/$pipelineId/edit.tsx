import { createFileRoute, redirect } from '@tanstack/react-router'

import { rowForAnchor } from './-pipeline-row'

/**
 * The pipeline had a separate Edit page. Its settings now open in place on
 * the pipeline page, so old links land on the row that holds the section
 * they pointed at, such as `edit#ios-signing` on Signing.
 */
export const Route = createFileRoute(
  '/projects/$projectId/pipelines/$pipelineId/edit',
)({
  beforeLoad: ({ location, params }) => {
    const hash = location.hash || undefined
    throw redirect({
      to: '/projects/$projectId/pipelines/$pipelineId',
      params,
      search: { row: rowForAnchor(hash) ?? 'source' },
      hash,
      replace: true,
    })
  },
})
