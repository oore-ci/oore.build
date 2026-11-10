import { createFileRoute, redirect } from '@tanstack/react-router'

/** The General settings page used to live here; keep old links working. */
export const Route = createFileRoute('/settings/preferences')({
  beforeLoad: () => {
    throw redirect({ href: `/settings/general${window.location.search}` })
  },
})
