import { useState, type ReactElement } from 'react'
import { Link } from '@tanstack/react-router'
import type { Pipeline } from '@oore/client/models'
import { usePipelines } from '@/hooks/use-pipelines'
import { useIsMobile } from '@/hooks/use-mobile'
import { Button } from '@/components/ui/button'
import {
  Drawer,
  DrawerContent,
  DrawerDescription,
  DrawerHeader,
  DrawerTitle,
  DrawerTrigger,
} from '@/components/ui/drawer'
import { Skeleton } from '@/components/ui/skeleton'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'

export type SetupGuideKind = 'signing' | 'store'

interface GuideTarget {
  /** Shown on the button for each pipeline. */
  label: string
  platform: 'android' | 'ios'
  hash: string
  /** The pipeline's Signing row, or its Publishing row. */
  page: 'edit' | 'publishing'
}

interface GuideStep {
  title: string
  items: ReadonlyArray<string>
}

interface Guide {
  title: string
  description: string
  steps: ReadonlyArray<{
    platform: 'android' | 'ios'
    heading: string
    steps: ReadonlyArray<GuideStep>
  }>
  targets: ReadonlyArray<GuideTarget>
  after: string
  /** The files go to the app's signing library before a pipeline picks them. */
  library?: boolean
}

const GUIDES = {
  signing: {
    title: 'Sign a release',
    description:
      'Add your keystore or certificate to the app’s signing library once, then pick it on each pipeline.',
    library: true,
    steps: [
      {
        platform: 'android',
        heading: 'Android',
        steps: [
          {
            title: 'Have ready',
            items: [
              'The upload keystore (.jks or .keystore) you sign Play releases with.',
              'Its password, the key alias and the key password.',
            ],
          },
          {
            title: 'Add it',
            items: [
              'Add it to the signing library as an Android keystore.',
              'Pick it for release builds under the pipeline’s Signing. No Gradle changes are needed.',
            ],
          },
        ],
      },
      {
        platform: 'ios',
        heading: 'iOS',
        steps: [
          {
            title: 'Have ready',
            items: [
              'A distribution certificate (.p12) with its password and a provisioning profile per bundle ID.',
              'Or an App Store Connect API key (.p8) with the App Manager role, so Oore fetches them itself.',
            ],
          },
          {
            title: 'Add it',
            items: [
              'Add the certificate and its profiles to the signing library.',
              'Pick it under the pipeline’s iOS signing, or choose App Store Connect there.',
            ],
          },
        ],
      },
    ],
    targets: [
      {
        label: 'Android signing',
        platform: 'android',
        hash: 'android-signing',
        page: 'edit',
      },
      {
        label: 'iOS signing',
        platform: 'ios',
        hash: 'ios-signing',
        page: 'edit',
      },
    ],
    after: 'Then start a release build from that pipeline.',
  },
  store: {
    title: 'Send to a store',
    description:
      'A pipeline can upload each build to Google Play or TestFlight. Oore checks the access before it saves, and again before every upload.',
    steps: [
      {
        platform: 'android',
        heading: 'Google Play',
        steps: [
          {
            title: 'Have ready',
            items: [
              'The app already exists in Play Console, with one release uploaded by hand.',
              'A service account JSON key from Google Cloud, with the Google Play Android Developer API turned on.',
              'In Play Console, Users and permissions: invite the service account email and give it Release to testing tracks (and Release to production for production).',
            ],
          },
          {
            title: 'Add it and test',
            items: [
              'Choose the key as the service account secret, then Test credentials. A failed check names the permission that is missing.',
              'Pick the track and save. Android releases need release signing too.',
            ],
          },
        ],
      },
      {
        platform: 'ios',
        heading: 'TestFlight and the App Store',
        steps: [
          {
            title: 'Have ready',
            items: [
              'The app record in App Store Connect, with the same bundle ID.',
              'An App Store Connect API key (.p8) with the App Manager role, its key ID and issuer ID.',
            ],
          },
          {
            title: 'Add it and check',
            items: [
              'Add the key, pick TestFlight groups or App Store review, and save.',
              'The readiness check lists anything App Store Connect would refuse before the first upload.',
            ],
          },
        ],
      },
    ],
    targets: [
      {
        label: 'Google Play',
        platform: 'android',
        hash: 'google-play',
        page: 'publishing',
      },
      {
        label: 'TestFlight',
        platform: 'ios',
        hash: 'app-store',
        page: 'publishing',
      },
    ],
    after:
      'The next build from that pipeline uploads on its own. Home shows the store status.',
  },
} satisfies Record<SetupGuideKind, Guide>

function TargetLink({
  pipeline,
  projectId,
  target,
}: {
  pipeline: Pipeline
  projectId: string
  target: GuideTarget
}) {
  const params = { projectId, pipelineId: pipeline.id }
  const link = (
    <Link
      to="/projects/$projectId/pipelines/$pipelineId"
      params={params}
      search={{ row: target.page === 'edit' ? 'signing' : 'publishing' }}
      hash={target.hash}
    />
  )
  return (
    <Button size="sm" variant="outline" nativeButton={false} render={link}>
      {target.label}
    </Button>
  )
}

function PipelinePicker({
  guide,
  projectId,
}: {
  guide: Guide
  projectId: string
}) {
  const pipelinesQuery = usePipelines(projectId, { limit: 200 })
  if (pipelinesQuery.isLoading) return <Skeleton className="h-16 w-full" />
  const pipelines = (pipelinesQuery.data?.pipelines ?? []).filter((pipeline) =>
    guide.targets.some((target) =>
      pipeline.execution_config.platforms.includes(target.platform),
    ),
  )
  if (pipelines.length === 0) {
    return (
      <Text variant="muted">
        No pipeline builds Android or iOS yet. Add one from the Pipelines tab
        first.
      </Text>
    )
  }
  return (
    <ul className="flex flex-col rounded-lg border" aria-label="Pipelines">
      {pipelines.map((pipeline) => (
        <li key={pipeline.id} className="border-b px-3 py-2.5 last:border-b-0">
          <Row gap={2} className="flex-wrap justify-between">
            <span className="min-w-0 truncate text-sm font-medium">
              {pipeline.name}
            </span>
            <Row gap={2}>
              {guide.targets
                .filter((target) =>
                  pipeline.execution_config.platforms.includes(target.platform),
                )
                .map((target) => (
                  <TargetLink
                    key={target.hash}
                    pipeline={pipeline}
                    projectId={projectId}
                    target={target}
                  />
                ))}
            </Row>
          </Row>
        </li>
      ))}
    </ul>
  )
}

/**
 * A guide from the Next steps card: what to have ready, where it goes and
 * how Oore checks it, ending at the pipeline section that takes it.
 */
export function SetupGuideDrawer({
  children,
  kind,
  projectId,
}: {
  children: ReactElement
  kind: SetupGuideKind
  projectId: string
}) {
  const [open, setOpen] = useState(false)
  const isMobile = useIsMobile()
  const guide: Guide = GUIDES[kind]
  return (
    <Drawer
      open={open}
      onOpenChange={setOpen}
      showSwipeHandle={isMobile}
      swipeDirection={isMobile ? 'down' : 'right'}
    >
      <DrawerTrigger render={children} />
      <DrawerContent>
        <DrawerHeader>
          <DrawerTitle>{guide.title}</DrawerTitle>
          <DrawerDescription>{guide.description}</DrawerDescription>
        </DrawerHeader>
        <div className="flex-1 scroll-fade overflow-y-auto p-4">
          <Stack gap={6}>
            {guide.steps.map((platform) => (
              <section key={platform.platform} aria-label={platform.heading}>
                <Stack gap={3}>
                  <h3 className="text-sm font-semibold">{platform.heading}</h3>
                  <ol className="flex flex-col gap-3">
                    {platform.steps.map((step, index) => (
                      <li key={step.title}>
                        <Row gap={3} className="items-start">
                          <Row
                            className="size-5 shrink-0 justify-center rounded-full bg-muted text-xs font-medium"
                            render={<span />}
                          >
                            {index + 1}
                          </Row>
                          <Stack gap={1} className="min-w-0">
                            <span className="text-sm font-medium">
                              {step.title}
                            </span>
                            <ul className="list-disc space-y-1 pl-4 text-sm text-muted-foreground">
                              {step.items.map((item) => (
                                <li key={item}>{item}</li>
                              ))}
                            </ul>
                          </Stack>
                        </Row>
                      </li>
                    ))}
                  </ol>
                </Stack>
              </section>
            ))}
            {guide.library ? (
              <section aria-label="Signing library">
                <Stack gap={3}>
                  <h3 className="text-sm font-semibold">Signing library</h3>
                  <Row>
                    <Button
                      size="sm"
                      nativeButton={false}
                      render={
                        <Link
                          to="/projects/$projectId"
                          params={{ projectId }}
                          search={{ tab: 'settings' }}
                          hash="signing"
                          onClick={() => setOpen(false)}
                        />
                      }
                    >
                      Open signing library
                    </Button>
                  </Row>
                </Stack>
              </section>
            ) : null}
            <section aria-label="Choose a pipeline">
              <Stack gap={3}>
                <h3 className="text-sm font-semibold">Choose a pipeline</h3>
                <PipelinePicker guide={guide} projectId={projectId} />
                <Text variant="muted">{guide.after}</Text>
              </Stack>
            </section>
          </Stack>
        </div>
      </DrawerContent>
    </Drawer>
  )
}
