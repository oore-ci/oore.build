import { useState } from 'react'

import type { Release } from '@oore/client/models'
import { useProjectEnvironments } from '@/hooks/use-environments'
import { useProject } from '@/hooks/use-projects'
import { nextEnvironment, promotionSource } from '@/lib/environments'
import { PromoteDialog } from '@/components/environments/promote-dialog'
import { Button } from '@/components/ui/button'

/**
 * "Promote to Beta" wherever a release shows, not only on the Environments
 * tab. It appears only when the release is the one its environment would
 * promote and the person may promote or ask for it.
 */
export function PromoteReleaseButton({
  release,
  size = 'sm',
}: {
  release: Release
  size?: 'sm' | 'default'
}) {
  const [open, setOpen] = useState(false)
  const projectQuery = useProject(
    release.environment_id ? release.project_id : '',
  )
  const environmentsQuery = useProjectEnvironments(release.project_id, {
    enabled: !!release.environment_id,
  })
  const role =
    projectQuery.data?.current_user_role ??
    projectQuery.data?.project.current_user_role
  const canManage = role === 'maintainer'
  const canOperate = canManage || role === 'developer'
  const environments = environmentsQuery.data?.environments ?? []
  const source = promotionSource(environments, release)
  if (!source || !canOperate) return null
  const target = nextEnvironment(environments, source)
  // Only offer a step forward: from the last environment (production) the
  // dialog would default to the first one, which nobody means to do here.
  if (!target || !(canManage || target.approval_required)) return null

  return (
    <>
      <Button variant="outline" size={size} onClick={() => setOpen(true)}>
        Promote to {target.name}
      </Button>
      {open ? (
        <PromoteDialog
          open
          onOpenChange={setOpen}
          canPromoteDirectly={canManage}
          environments={environments}
          projectId={release.project_id}
          release={release}
          source={source}
        />
      ) : null}
    </>
  )
}
