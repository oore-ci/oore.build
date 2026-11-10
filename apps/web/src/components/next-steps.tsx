import { useState } from 'react'
import { Link } from '@tanstack/react-router'
import { useProject } from '@/hooks/use-projects'
import { hasProjectPermission } from '@/hooks/use-permissions'
import { nextSteps } from '@/lib/setup-checklist'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'
import { SetupGuideDrawer } from '@/components/setup-guide-drawer'

function hiddenKey(projectId: string) {
  return `oore-next-steps-hidden:${projectId}`
}

function readHidden(projectId: string) {
  try {
    return localStorage.getItem(hiddenKey(projectId)) === '1'
  } catch {
    return false
  }
}

/**
 * What to set up after the first install: signing a release, sending to a
 * store and inviting a tester. Each step leaves once it is done, and the
 * card once all three are, or when someone hides it.
 */
export function NextSteps({ projectId }: { projectId: string }) {
  const projectQuery = useProject(projectId)
  const [hidden, setHidden] = useState(() => readHidden(projectId))
  const data = projectQuery.data
  if (hidden || !data || data.build_count === 0) return null
  if (!hasProjectPermission(data.current_user_role, 'pipelines:write')) {
    return null
  }
  const steps = data.setup ? nextSteps(data.setup) : []
  if (steps.length === 0) return null

  function hide() {
    try {
      localStorage.setItem(hiddenKey(projectId), '1')
    } catch {
      // Hidden for this visit only.
    }
    setHidden(true)
  }

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>Next steps</CardTitle>
        <CardDescription>
          Your app installs. These take it to testers and the stores.
        </CardDescription>
        <CardAction>
          <Button variant="ghost" size="sm" onClick={hide}>
            Hide
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent>
        <ul className="flex flex-col" aria-label="Next steps">
          {steps.map((step) => (
            <li
              key={step.id}
              className="border-b py-3 first:pt-0 last:border-b-0 last:pb-0"
            >
              <Row gap={3} className="flex-wrap">
                <Stack gap={0.5} className="min-w-0 flex-1">
                  <span className="text-sm font-medium">{step.title}</span>
                  <Text variant="muted">{step.detail}</Text>
                </Stack>
                {step.id === 'testers' ? (
                  <Button
                    size="sm"
                    variant="outline"
                    render={
                      <Link
                        to="/projects/$projectId"
                        params={{ projectId }}
                        search={{ tab: 'settings' }}
                      />
                    }
                    nativeButton={false}
                  >
                    {step.action}
                  </Button>
                ) : (
                  <SetupGuideDrawer kind={step.id} projectId={projectId}>
                    <Button size="sm" variant="outline">
                      {step.action}
                    </Button>
                  </SetupGuideDrawer>
                )}
              </Row>
            </li>
          ))}
        </ul>
      </CardContent>
    </Card>
  )
}
