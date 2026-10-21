import type { BadgeVariant } from '@/components/ui/badge'
import type {
  BuildStatus,
  IntegrationStatus,
  NotificationDeliveryStatus,
  RunnerPolicyBlockReason,
  RunnerStatus,
  TriggerType,
  UserStatus,
} from '@oore/client/models'
import { formatEnumLabel } from '@/lib/format-utils'

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

export const TRIGGER_TYPE_LABELS = {
  manual: 'Manual',
  api: 'API',
  webhook: 'Webhook',
  schedule: 'Schedule',
} as const satisfies Record<TriggerType, string>

const BUILD_STATUS_VARIANT = new Map<string, BadgeVariant>([
  ['succeeded', 'success'],
  ['active', 'secondary'],
  ['failed', 'destructive'],
  ['error', 'destructive'],
])

export function getStatusVariant(status: string): BadgeVariant {
  return BUILD_STATUS_VARIANT.get(status) ?? 'outline'
}

const RUNNER_POLICY_BLOCK_LABEL = {
  instance_paused: 'Direct runner paused',
  repository_unavailable: 'Source unavailable',
} satisfies Record<RunnerPolicyBlockReason, string>

export function getRunnerPolicyBlockLabel(
  reason: RunnerPolicyBlockReason,
): string {
  return RUNNER_POLICY_BLOCK_LABEL[reason]
}

export function getIntegrationStatusVariant(status: string): BadgeVariant {
  if (status === 'active') return 'secondary'
  return status === 'error' ? 'destructive' : 'outline'
}

export function getPipelineStatusVariant(enabled: boolean): BadgeVariant {
  return enabled ? 'secondary' : 'outline'
}

export function getRunnerStatusVariant(status: string): BadgeVariant {
  if (status === 'online' || status === 'busy') return 'secondary'
  return status === 'draining' ? 'warning' : 'outline'
}

/** Which product enum a `StatusBadge` is rendering. */
export type StatusKind =
  | 'build'
  | 'delivery'
  | 'enabled'
  | 'integration'
  | 'runner'
  | 'user'

export interface StatusPresentation {
  label: string
  variant: BadgeVariant
}

const INTEGRATION_STATUS_LABEL = {
  active: 'Active',
  inactive: 'Inactive',
  error: 'Error',
} satisfies Record<IntegrationStatus, string>

const RUNNER_STATUS_LABEL = {
  online: 'Online',
  offline: 'Offline',
  busy: 'Busy',
  draining: 'Draining',
} satisfies Record<RunnerStatus, string>

const USER_STATUS_LABEL = {
  active: 'Active',
  disabled: 'Disabled',
  invited: 'Invited',
} satisfies Record<UserStatus, string>

const USER_STATUS_VARIANT = {
  active: 'secondary',
  disabled: 'destructive',
  invited: 'outline',
} satisfies Record<UserStatus, BadgeVariant>

const DELIVERY_STATUS_LABEL = {
  pending: 'Pending',
  delivered: 'Delivered',
  failed: 'Failed',
} satisfies Record<NotificationDeliveryStatus, string>

const DELIVERY_STATUS_VARIANT = {
  pending: 'outline',
  delivered: 'secondary',
  failed: 'destructive',
} satisfies Record<NotificationDeliveryStatus, BadgeVariant>

function lookupLabel(
  labels: Readonly<Record<string, string>>,
  value: string,
): string {
  return labels[value] ?? formatEnumLabel(value)
}

function lookupVariant(
  variants: Readonly<Record<string, BadgeVariant>>,
  value: string,
): BadgeVariant {
  return variants[value] ?? 'outline'
}

/** Status value for an on/off entity such as a pipeline or channel. */
export function enabledStatus(enabled: boolean): 'enabled' | 'disabled' {
  return enabled ? 'enabled' : 'disabled'
}

/**
 * Label and badge variant for a raw status value. Labels are sentence case
 * and never the raw enum; unknown values fall back to `formatEnumLabel`.
 */
export function getStatusPresentation(
  kind: StatusKind,
  value: string,
): StatusPresentation {
  switch (kind) {
    case 'build':
      return {
        label: lookupLabel(BUILD_STATUS_FILTER_OPTIONS, value),
        variant: getStatusVariant(value),
      }
    case 'delivery':
      return {
        label: lookupLabel(DELIVERY_STATUS_LABEL, value),
        variant: lookupVariant(DELIVERY_STATUS_VARIANT, value),
      }
    case 'enabled':
      return {
        label: value === 'enabled' ? 'Enabled' : 'Disabled',
        variant: getPipelineStatusVariant(value === 'enabled'),
      }
    case 'integration':
      return {
        label: lookupLabel(INTEGRATION_STATUS_LABEL, value),
        variant: getIntegrationStatusVariant(value),
      }
    case 'runner':
      return {
        label: lookupLabel(RUNNER_STATUS_LABEL, value),
        variant: getRunnerStatusVariant(value),
      }
    case 'user':
      return {
        label: lookupLabel(USER_STATUS_LABEL, value),
        variant: lookupVariant(USER_STATUS_VARIANT, value),
      }
  }
}
