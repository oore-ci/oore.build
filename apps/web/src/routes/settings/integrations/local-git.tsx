import { createFileRoute, redirect } from '@tanstack/react-router'

/**
 * Local repositories are chosen while creating a project, so this old page
 * sends the user straight to that flow.
 */
export const Route = createFileRoute('/settings/integrations/local-git')({
  beforeLoad: () => {
    throw redirect({
      to: '/projects',
      search: { openCreate: '1' },
      replace: true,
    })
  },
})
