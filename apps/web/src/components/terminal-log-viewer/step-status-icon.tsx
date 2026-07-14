import {
  AlertCircleIcon,
  CheckmarkCircle02Icon,
  Loading03Icon,
} from '@hugeicons/core-free-icons'
import { HugeiconsIcon } from '@hugeicons/react'

export function StepStatusIcon({ status }: { status: string }) {
  const normalized = status.trim().toLowerCase()
  if (normalized === 'running') {
    return (
      <HugeiconsIcon
        icon={Loading03Icon}
        size={16}
        className="shrink-0 animate-spin text-info"
      />
    )
  }
  if (normalized === 'succeeded') {
    return (
      <HugeiconsIcon
        icon={CheckmarkCircle02Icon}
        size={16}
        className="shrink-0 text-success"
      />
    )
  }
  if (
    normalized === 'failed' ||
    normalized === 'canceled' ||
    normalized === 'timed_out'
  ) {
    return (
      <HugeiconsIcon
        icon={AlertCircleIcon}
        size={16}
        className="shrink-0 text-destructive"
      />
    )
  }
  return (
    <span className="flex size-3 shrink-0 items-center justify-center">
      <span className="size-1.5 bg-muted-foreground" />
    </span>
  )
}
