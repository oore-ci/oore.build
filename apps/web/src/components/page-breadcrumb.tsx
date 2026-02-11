import { Link, useMatches } from '@tanstack/react-router'
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from '@/components/ui/breadcrumb'
import { useBreadcrumbStore } from '@/stores/breadcrumb-store'

interface BreadcrumbEntry {
  label: string
  to?: string
}

function humanize(segment: string): string {
  return segment.replace(/[-_]/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase())
}

export default function PageBreadcrumb() {
  const matches = useMatches()
  const dynamicLabels = useBreadcrumbStore((s) => s.labels)

  const crumbs: Array<BreadcrumbEntry> = []

  // Check if any match is under /settings — inject virtual "Settings" parent
  const isSettingsRoute = matches.some((m) =>
    m.fullPath.startsWith('/settings'),
  )

  for (const match of matches) {
    // Skip root layout
    if (match.id === '__root__') continue

    // Dynamic label from store takes priority over static data
    const staticData = match.staticData as
      | { breadcrumbLabel?: string }
      | undefined
    const label = dynamicLabels[match.id] || staticData?.breadcrumbLabel

    if (label) {
      crumbs.push({
        label,
        to: match.fullPath,
      })
    }
  }

  // If under settings and no "Settings" crumb exists, prepend it
  if (isSettingsRoute && !crumbs.some((c) => c.label === 'Settings')) {
    crumbs.unshift({ label: 'Settings' })
  }

  // Fallback: if no crumbs were generated, humanize the last path segment
  if (crumbs.length === 0) {
    const lastMatch = matches[matches.length - 1]
    const segments = lastMatch.fullPath.split('/').filter(Boolean)
    const lastSegment = segments[segments.length - 1]
    crumbs.push({
      label: lastSegment ? humanize(lastSegment) : 'Dashboard',
    })
  }

  // Last crumb is always the current page (no link)
  const lastIndex = crumbs.length - 1

  return (
    <Breadcrumb>
      <BreadcrumbList>
        {crumbs.map((crumb, i) => {
          const isLast = i === lastIndex

          return (
            <span key={i} className="contents">
              {i > 0 && <BreadcrumbSeparator className="hidden md:block" />}
              <BreadcrumbItem
                className={i < lastIndex ? 'hidden md:block' : ''}
              >
                {isLast ? (
                  <BreadcrumbPage>{crumb.label}</BreadcrumbPage>
                ) : (
                  <BreadcrumbLink
                    render={crumb.to ? <Link to={crumb.to} /> : <span />}
                  >
                    {crumb.label}
                  </BreadcrumbLink>
                )}
              </BreadcrumbItem>
            </span>
          )
        })}
      </BreadcrumbList>
    </Breadcrumb>
  )
}
