import { Link, useMatches } from '@tanstack/react-router'
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from '@/components/ui/breadcrumb'

const ROUTE_LABELS: Record<string, string> = {
  '/': 'Dashboard',
  '/settings/users': 'Users',
}

const ROUTE_PARENTS: Record<string, { path: string; label: string }> = {
  '/settings/users': { path: '/', label: 'Settings' },
}

export default function PageBreadcrumb() {
  const matches = useMatches()
  const lastMatch = matches[matches.length - 1]
  const currentPath = lastMatch.fullPath
  const label = ROUTE_LABELS[currentPath] ?? 'Page'
  const parent = ROUTE_PARENTS[currentPath] as
    | { path: string; label: string }
    | undefined

  return (
    <Breadcrumb>
      <BreadcrumbList>
        {parent ? (
          <>
            <BreadcrumbItem className="hidden md:block">
              <BreadcrumbLink render={<Link to={parent.path} />}>
                {parent.label}
              </BreadcrumbLink>
            </BreadcrumbItem>
            <BreadcrumbSeparator className="hidden md:block" />
          </>
        ) : null}
        <BreadcrumbItem>
          <BreadcrumbPage>{label}</BreadcrumbPage>
        </BreadcrumbItem>
      </BreadcrumbList>
    </Breadcrumb>
  )
}
