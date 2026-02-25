import { createFileRoute, redirect } from '@tanstack/solid-router'

export const Route = createFileRoute('/settings/artifacts')({
  beforeLoad: () => {
    throw redirect({ to: '/settings/preferences' })
  },
})
