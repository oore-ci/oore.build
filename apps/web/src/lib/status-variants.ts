import type { VariantProps } from 'class-variance-authority'
import type { badgeVariants } from '@/components/ui/badge-variants'
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
  succeeded: 'success',
  active: 'success',
  running: 'info',
  failed: 'destructive',
  error: 'destructive',
  timed_out: 'warning',
  queued: 'outline',
  scheduled: 'outline',
  assigned: 'secondary',
  canceled: 'secondary',
  expired: 'secondary',
  inactive: 'secondary',
}

export function getStatusVariant(status: string): BadgeVariant {
  return BUILD_STATUS_VARIANT[status] ?? 'outline'
}

export const INTEGRATION_STATUS_VARIANT: Record<string, BadgeVariant> = {
  active: 'success',
  inactive: 'secondary',
  error: 'destructive',
}

export function getIntegrationStatusVariant(status: string): BadgeVariant {
  return INTEGRATION_STATUS_VARIANT[status] ?? 'outline'
}

export function getPipelineStatusVariant(enabled: boolean): BadgeVariant {
  return enabled ? 'success' : 'secondary'
}

const RUNNER_STATUS_VARIANT: Record<string, BadgeVariant> = {
  online: 'success',
  busy: 'warning',
  draining: 'secondary',
  offline: 'outline',
}

export function getRunnerStatusVariant(status: string): BadgeVariant {
  return RUNNER_STATUS_VARIANT[status] ?? 'outline'
}
