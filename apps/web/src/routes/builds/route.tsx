import { createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute('/builds')({
  staticData: {
    breadcrumb: {
      title: 'Builds',
    },
  },
})
