import { createFileRoute, redirect } from '@tanstack/react-router'

/** Store credentials are a section of the Secrets and credentials page now. */
export const Route = createFileRoute('/settings/store-credentials')({
  beforeLoad: () => {
    throw redirect({
      to: '/settings/secrets',
      hash: 'store-credentials',
      replace: true,
    })
  },
})
