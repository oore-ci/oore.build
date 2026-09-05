import { isMatch, Link, useMatches, useParams } from '@tanstack/react-router'
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbSeparator,
  BreadcrumbPage,
} from '@/components/ui/breadcrumb'
import { useProject } from '@/hooks/use-projects'

export default function PageBreadcrumb() {
  const { projectId } = useParams({ strict: false })
  const project = useProject(projectId ?? '')
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
        {breadcrumbs.map((item, index) => [
          index !== breadcrumbs.length - 1 && (
            <BreadcrumbItem key={item.href}>
              <BreadcrumbLink render={<Link to={item.href} />}>
                {projectId &&
                item.href.replace(/\/$/, '') === `/projects/${projectId}`
                  ? (project.data?.project.name ?? item.label)
                  : item.label}
              </BreadcrumbLink>
            </BreadcrumbItem>
          ),
          index === breadcrumbs.length - 1 && (
            <BreadcrumbItem key={item.href}>
              <BreadcrumbPage>
                {projectId &&
                item.href.replace(/\/$/, '') === `/projects/${projectId}`
                  ? (project.data?.project.name ?? item.label)
                  : item.label}
              </BreadcrumbPage>
            </BreadcrumbItem>
          ),
          index < breadcrumbs.length - 1 && (
            <BreadcrumbSeparator key={item.href + '_separator'} />
          ),
        ])}
      </BreadcrumbList>
    </Breadcrumb>
  )
}
