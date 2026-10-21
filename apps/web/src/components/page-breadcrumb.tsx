import { isMatch, Link, useMatches, useParams } from '@tanstack/react-router'
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

/**
 * Breadcrumb leaves name the entity, not its type: the project's name, the
 * pipeline's name, the build number. The route's static title is the fallback
 * while the entity loads. Every hook here reads the query the page itself
 * already populated, so this adds no requests.
 */
function useEntityCrumbLabels(): Map<string, string> {
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
  return labels
}

export default function PageBreadcrumb() {
  const entityLabels = useEntityCrumbLabels()
  const breadcrumbs = useMatches({
    select: (matches) =>
      matches
        .filter((m) => isMatch(m, 'staticData.breadcrumb'))
        .map((match) => ({
          href: match.pathname,
          label: match.staticData?.breadcrumb?.title,
        })),
  })

  return (
    <Breadcrumb>
      <BreadcrumbList>
        {breadcrumbs.map((item, index) => {
          const label =
            entityLabels.get(item.href.replace(/\/$/, '')) ?? item.label
          const isLeaf = index === breadcrumbs.length - 1
          return [
            <BreadcrumbItem key={item.href}>
              {isLeaf ? (
                <BreadcrumbPage>{label}</BreadcrumbPage>
              ) : (
                <BreadcrumbLink render={<Link to={item.href} />}>
                  {label}
                </BreadcrumbLink>
              )}
            </BreadcrumbItem>,
            isLeaf ? null : (
              <BreadcrumbSeparator key={item.href + '_separator'} />
            ),
          ]
        })}
      </BreadcrumbList>
    </Breadcrumb>
  )
}
