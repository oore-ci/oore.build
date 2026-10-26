import { isMatch, Link, useMatches, useParams } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { ArrowLeft01Icon } from '@hugeicons/core-free-icons'
import { isOoreApiError } from '@oore/client/client'
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbSeparator,
  BreadcrumbPage,
} from '@/components/ui/breadcrumb'
import { useBuild } from '@/hooks/use-builds'
import { useIntegration } from '@/hooks/use-integrations'
import { useNotificationChannel } from '@/hooks/use-notification-channels'
import { usePipeline } from '@/hooks/use-pipelines'
import { useProject } from '@/hooks/use-projects'
import { Separator } from '@/components/ui/separator'
import { rowVariants, textVariants } from '@/lib/recipes'
import { cn } from '@/lib/utils'

/**
 * Breadcrumb leaves name the entity, not its type: the project's name, the
 * pipeline's name, the build number. The route's static title is the fallback
 * while the entity loads. Every hook here reads the query the page itself
 * already populated, so this adds no requests.
 */
function useEntityCrumbLabels() {
  const { buildId, channelId, integrationId, pipelineId, projectId } =
    useParams({ strict: false })
  const project = useProject(projectId ?? '')
  const pipeline = usePipeline(pipelineId ?? '')
  const build = useBuild(buildId ?? '')
  const channel = useNotificationChannel(channelId ?? '')
  const integration = useIntegration(integrationId ?? '')

  const labels = new Map<string, string>()
  if (projectId && project.data) {
    labels.set(`/projects/${projectId}`, project.data.project.name)
  }
  if (projectId && pipelineId && pipeline.data) {
    labels.set(
      `/projects/${projectId}/pipelines/${pipelineId}`,
      pipeline.data.pipeline.name,
    )
  }
  if (buildId && build.data) {
    labels.set(`/builds/${buildId}`, `Build #${build.data.build.build_number}`)
  }
  if (channelId && channel.data) {
    labels.set(
      `/settings/notifications/${channelId}`,
      channel.data.channel.name,
    )
  }
  if (integrationId && integration.data) {
    const source = integration.data.integration
    labels.set(
      `/settings/integrations/${integrationId}`,
      source.display_name ?? source.provider,
    )
  }
  const missing = [project, pipeline, build, channel, integration].some(
    ({ error }) => isOoreApiError(error) && error.status === 404,
  )
  return { labels, missing }
}

/**
 * A trail is shown only when it adds hierarchy the page title lacks: one crumb
 * repeats the H1, and a not-found page has no hierarchy to show. Phones get a
 * single back link to the nearest parent so the header never wraps.
 */
export default function PageBreadcrumb() {
  const { labels: entityLabels, missing } = useEntityCrumbLabels()
  const routeNotFound = useMatches({
    select: (matches) => matches.some((m) => m.status === 'notFound'),
  })
  const breadcrumbs = useMatches({
    select: (matches) =>
      matches
        .filter((m) => isMatch(m, 'staticData.breadcrumb'))
        .map((match) => ({
          href: match.pathname,
          label: match.staticData?.breadcrumb?.title,
          linkable: match.staticData?.breadcrumb?.link !== false,
        })),
  }).map((item) => ({
    ...item,
    label: entityLabels.get(item.href.replace(/\/$/, '')) ?? item.label,
  }))

  if (breadcrumbs.length < 2 || routeNotFound || missing) return null

  const parent = breadcrumbs
    .slice(0, -1)
    .reverse()
    .find((item) => item.linkable)

  return (
    <>
      <Separator
        orientation="vertical"
        className="mx-2 hidden h-4 self-center! sm:block"
      />
      {parent ? (
        <Link
          to={parent.href}
          data-slot="breadcrumb-link"
          className={cn(
            textVariants({ variant: 'muted' }),
            rowVariants({ gap: 1 }),
            'min-w-0 transition-colors hover:text-foreground sm:hidden',
          )}
        >
          <HugeiconsIcon
            icon={ArrowLeft01Icon}
            strokeWidth={2}
            className="size-4 shrink-0"
          />
          <span className="truncate">{parent.label}</span>
        </Link>
      ) : null}
      <Breadcrumb className="hidden min-w-0 sm:block">
        <BreadcrumbList className="flex-nowrap">
          {breadcrumbs.map((item, index) => {
            const isLeaf = index === breadcrumbs.length - 1
            return [
              <BreadcrumbItem
                key={item.href}
                className={isLeaf ? 'min-w-0' : 'shrink-0'}
              >
                {isLeaf ? (
                  <BreadcrumbPage
                    className="block max-w-[32ch] truncate"
                    title={item.label}
                  >
                    {item.label}
                  </BreadcrumbPage>
                ) : item.linkable ? (
                  <BreadcrumbLink
                    render={<Link to={item.href} />}
                    className="block max-w-[24ch] truncate"
                    title={item.label}
                  >
                    {item.label}
                  </BreadcrumbLink>
                ) : (
                  <span>{item.label}</span>
                )}
              </BreadcrumbItem>,
              isLeaf ? null : (
                <BreadcrumbSeparator key={item.href + '_separator'} />
              ),
            ]
          })}
        </BreadcrumbList>
      </Breadcrumb>
    </>
  )
}
