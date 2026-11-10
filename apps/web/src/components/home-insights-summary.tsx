import { useMemo } from 'react'
import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { ChevronRightIcon } from '@hugeicons/core-free-icons'

import {
  DashboardSection,
  DashboardViewAll,
} from '@/components/dashboard-section'
import { storeHealthProblems } from '@/components/dashboard-store-health'
import { useBuildInsights } from '@/hooks/use-builds'
import { useStoreVitals } from '@/hooks/use-releases'
import {
  appHealthLine,
  buildHealthLine,
  storeHealthLine,
} from '@/lib/insights-summary'

/**
 * Home's slower look: build, app and store health live on Insights, and Home
 * only says one line for each that is off. Nothing off, nothing shown.
 */
export function HomeInsightsSummary() {
  const insightsQuery = useBuildInsights({ days: 7 })
  const vitalsQuery = useStoreVitals()
  const lines = useMemo(() => {
    const insights = insightsQuery.data
    return [
      {
        key: 'builds',
        text: insights ? buildHealthLine(insights) : null,
        hash: undefined,
      },
      {
        key: 'apps',
        text: insights ? appHealthLine(insights) : null,
        hash: 'app-health',
      },
      {
        key: 'stores',
        text: storeHealthLine(
          storeHealthProblems(vitalsQuery.data?.apps ?? []),
        ),
        hash: 'store-health',
      },
    ].filter((line) => line.text)
  }, [insightsQuery.data, vitalsQuery.data?.apps])

  if (lines.length === 0) return null

  return (
    <DashboardSection
      title="Worth a look"
      action={<DashboardViewAll to="/insights" />}
    >
      <ul className="divide-y overflow-hidden rounded-xl border bg-card">
        {lines.map((line) => (
          <li key={line.key}>
            <Link
              to="/insights"
              hash={line.hash}
              className="flex items-center justify-between gap-3 px-4 py-3 text-sm hover:bg-muted/50"
            >
              <span className="min-w-0">{line.text}</span>
              <HugeiconsIcon
                icon={ChevronRightIcon}
                className="size-4 shrink-0 text-muted-foreground"
              />
            </Link>
          </li>
        ))}
      </ul>
    </DashboardSection>
  )
}
