import { createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute('/settings/notifications')({
  staticData: {
    breadcrumb: {
      title: 'Notifications',
    },
  },
})
