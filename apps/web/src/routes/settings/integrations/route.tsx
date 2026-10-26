import { createFileRoute, Link, Outlet } from '@tanstack/react-router'
import { Button } from '@/components/ui/button'
import { useFirstAppScope, useFirstAppStore } from '@/stores/first-app-store'
import { Row } from '@/components/layout/row'

export const Route = createFileRoute('/settings/integrations')({
  staticData: {
    breadcrumb: {
      title: 'Sources',
    },
  },
  component: SourceSetupLayout,
})

function SourceSetupLayout() {
  const scope = useFirstAppScope()
  const hasDraft = useFirstAppStore(
    (state) => !!state.progress[scope]?.projectDraft,
  )
  return (
    <>
      {hasDraft ? (
        <Row
          gap={3}
          className="flex-wrap justify-between border-b px-4 py-3 sm:px-6"
        >
          <p className="text-sm">
            Your project details are saved. Connect and sync a source, then
            choose its repository.
          </p>
          <Button
            variant="outline"
            size="sm"
            render={<Link to="/projects" search={{ openCreate: 1 }} />}
            nativeButton={false}
          >
            Continue setup
          </Button>
        </Row>
      ) : null}
      <Outlet />
    </>
  )
}
