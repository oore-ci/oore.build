import { createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute('/settings/integrations')({
  staticData: {
    breadcrumb: {
      title: 'Sources',
    },
  },
})
