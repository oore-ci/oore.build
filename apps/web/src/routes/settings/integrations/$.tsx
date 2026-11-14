import { createFileRoute, redirect } from '@tanstack/react-router'

/**
 * Sources used to live under /settings/integrations, and older daemons still
 * send people back to those addresses after connecting GitHub or GitLab.
 */
export const Route = createFileRoute('/settings/integrations/$')({
  beforeLoad: ({ params }) => {
    const rest = params._splat ? `/${params._splat}` : ''
    throw redirect({
      href: `/settings/sources${rest}${window.location.search}`,
    })
  },
})
