import { createFileRoute, redirect } from '@tanstack/react-router'
import { useState } from 'react'

import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { DashboardBuildHealth } from '@/components/dashboard-build-health'
import { DashboardBuildTrends } from '@/components/dashboard-build-trends'
import { DashboardProjectHealth } from '@/components/dashboard-project-health'
import { DashboardStoreHealth } from '@/components/dashboard-store-health'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import { Stack } from '@/components/layout/stack'
import { canChangeApprovalPolicy } from '@/hooks/use-permissions'
import { useRunners } from '@/hooks/use-runners'
import type { BuildInsightsWindow } from '@/hooks/use-builds'
import { PageMeta } from '@/lib/seo'
import { useAuthStore } from '@/stores/auth-store'

export const Route = createFileRoute('/insights')({
  staticData: { breadcrumb: { title: 'Insights' } },
  beforeLoad: (context) => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id, context)
    if (useAuthStore.getState().user?.role === 'qa_viewer') {
      throw redirect({ to: '/' })
    }
  },
  component: InsightsPage,
})

/**
 * The slower look that used to sit at the bottom of Home: build health over
 * a period, trends across builds, health per app and the stores' crash
 * numbers.
 */
function InsightsPage() {
  const [days, setDays] = useState<BuildInsightsWindow>(7)
  const canApprove = canChangeApprovalPolicy(
    useAuthStore((s) => s.user?.role),
    undefined,
  )
  const runnersQuery = useRunners(
    { limit: 1 },
    { select: (data) => data.online_total },
  )

  return (
    <PageLayout width="wide">
      <PageMeta title="Insights" noindex />
      <Stack gap={10}>
        <PageHeader
          title="Insights"
          description="How builds, apps and store releases are going over time."
        />
        <DashboardBuildHealth
          canApprove={canApprove}
          days={days}
          onDaysChange={setDays}
          onlineRunners={runnersQuery.data}
        />
        <div id="trends" className="scroll-mt-20">
          <DashboardBuildTrends days={days} />
        </div>
        <div id="app-health" className="scroll-mt-20">
          <DashboardProjectHealth days={days} />
        </div>
        <div id="store-health" className="scroll-mt-20">
          <DashboardStoreHealth />
        </div>
      </Stack>
    </PageLayout>
  )
}
