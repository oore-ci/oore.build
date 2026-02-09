import { createFileRoute, redirect } from '@tanstack/react-router'

export const Route = createFileRoute('/settings/artifacts')({
  beforeLoad: () => {
    throw redirect({ to: '/settings/preferences' })
  },
})
