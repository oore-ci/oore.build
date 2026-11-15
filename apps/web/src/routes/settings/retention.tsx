import { createFileRoute, redirect } from '@tanstack/react-router'

/** Retention is a section of the Storage page now. */
export const Route = createFileRoute('/settings/retention')({
  beforeLoad: () => {
    throw redirect({
      to: '/settings/artifacts',
      hash: 'retention',
      replace: true,
    })
  },
})
