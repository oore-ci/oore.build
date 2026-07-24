import type { BadgeVariant } from '@/components/ui/badge'
import type { BuildStatus, RunnerPolicyBlockReason } from '@/lib/types'

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

const BUILD_STATUS_VARIANT: Record<string, BadgeVariant> = {
  succeeded: 'success',
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

const RUNNER_POLICY_BLOCK_LABEL: Record<RunnerPolicyBlockReason, string> = {
  instance_paused: 'Direct runner paused',
  repository_unavailable: 'Source unavailable',
}

export function getRunnerPolicyBlockLabel(
  reason: RunnerPolicyBlockReason,
): string {
  return RUNNER_POLICY_BLOCK_LABEL[reason]
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
