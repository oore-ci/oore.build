import { createFileRoute, redirect } from '@tanstack/react-router'

/** Sources used to live at /settings/integrations; keep old links working. */
export const Route = createFileRoute('/settings/integrations/')({
  beforeLoad: () => {
    throw redirect({ href: `/settings/sources${window.location.search}` })
  },
})
