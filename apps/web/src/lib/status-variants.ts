import type { VariantProps } from 'class-variance-authority'
import type { badgeVariants } from '@/components/ui/badge'
import type { BuildStatus } from '@/lib/types'

type BadgeVariant = NonNullable<VariantProps<typeof badgeVariants>['variant']>

export const BUILD_STATUS_FILTER_OPTIONS = {
  all: 'All statuses',
  queued: 'Queued',
  scheduled: 'Scheduled',
  assigned: 'Assigned',
  running: 'Running',
  succeeded: 'Succeeded',
  failed: 'Failed',
  timed_out: 'Timed out',
  canceled: 'Canceled',
  expired: 'Expired',
} as const satisfies Record<'all' | BuildStatus, string>

export type BuildStatusFilter = keyof typeof BUILD_STATUS_FILTER_OPTIONS

const BUILD_STATUS_VARIANT: Record<string, BadgeVariant> = {
  succeeded: 'secondary',
  active: 'secondary',
  running: 'outline',
  failed: 'destructive',
  error: 'destructive',
  timed_out: 'outline',
  queued: 'outline',
  scheduled: 'outline',
  assigned: 'outline',
  canceled: 'outline',
  expired: 'outline',
  inactive: 'outline',
}

export function getStatusVariant(status: string): BadgeVariant {
  return BUILD_STATUS_VARIANT[status] ?? 'outline'
}

export const INTEGRATION_STATUS_VARIANT: Record<string, BadgeVariant> = {
  active: 'secondary',
  inactive: 'outline',
  error: 'destructive',
}

export function getIntegrationStatusVariant(status: string): BadgeVariant {
  return INTEGRATION_STATUS_VARIANT[status] ?? 'outline'
}

export function getPipelineStatusVariant(enabled: boolean): BadgeVariant {
  return enabled ? 'secondary' : 'outline'
}

const RUNNER_STATUS_VARIANT: Record<string, BadgeVariant> = {
  online: 'secondary',
  busy: 'outline',
  draining: 'outline',
  offline: 'outline',
}

export function getRunnerStatusVariant(status: string): BadgeVariant {
  return RUNNER_STATUS_VARIANT[status] ?? 'outline'
}
