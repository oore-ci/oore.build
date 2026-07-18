import { createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute(
  '/projects/$projectId/pipelines/$pipelineId',
)({
  staticData: {
    breadcrumb: {
      title: 'Pipeline',
    },
  },
})
