import { isMatch, Link, useMatches, useParentMatches } from '@tanstack/react-router'
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbSeparator,
  BreadcrumbPage,
} from '@/components/ui/breadcrumb'


export default function PageBreadcrumb() {
  const parentBreadcrumbs = useParentMatches({
    select: (matches) => matches
      .filter((m) => isMatch(m, 'staticData.breadcrumb'))
      .map((match) => ({
        href: match.pathname,
        label: match.staticData?.breadcrumb?.title,
        path: match.fullPath,
      })),
  })

  const currBreadcrumbs = useMatches({
    select: (matches) => matches
      .filter((m) => isMatch(m, 'staticData.breadcrumb'))
      .map((match) => ({
        href: match.pathname,
        label: match.staticData?.breadcrumb?.title,
        path: match.fullPath,
      })),
  })

  const breadcrumbs = [...parentBreadcrumbs, ...currBreadcrumbs];

  return (
    <Breadcrumb>
      <BreadcrumbList>
        {breadcrumbs.map((item, index) => (
          [
            index !== breadcrumbs.length - 1 && <BreadcrumbItem key={item.href}>
              <BreadcrumbLink render={<Link to={item.href} />}>
                {item.label}
              </BreadcrumbLink>
            </BreadcrumbItem>,
            index === breadcrumbs.length - 1 && <BreadcrumbPage key={item.href}>
              <BreadcrumbLink render={<Link to={item.href} />}>
                {item.label}
              </BreadcrumbLink>
            </BreadcrumbPage>,
            index < breadcrumbs.length - 1 && <BreadcrumbSeparator key={item.href + "_separator"} />]
        ))}
      </BreadcrumbList>
    </Breadcrumb>
  )
}
