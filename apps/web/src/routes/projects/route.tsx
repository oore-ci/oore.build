import { createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute('/projects')({
  staticData: {
    breadcrumb: {
      title: 'Projects',
    },
  },
})
