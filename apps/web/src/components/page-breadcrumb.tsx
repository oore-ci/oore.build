import { isMatch, Link, useMatches } from '@tanstack/react-router'
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbSeparator,
  BreadcrumbPage,
} from '@/components/ui/breadcrumb'

export default function PageBreadcrumb() {
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
                {item.label}
              </BreadcrumbLink>
            </BreadcrumbItem>
          ),
          index === breadcrumbs.length - 1 && (
            <BreadcrumbPage key={item.href}>
              <BreadcrumbLink render={<Link to={item.href} />}>
                {item.label}
              </BreadcrumbLink>
            </BreadcrumbPage>
          ),
          index < breadcrumbs.length - 1 && (
            <BreadcrumbSeparator key={item.href + '_separator'} />
          ),
        ])}
      </BreadcrumbList>
    </Breadcrumb>
  )
}
