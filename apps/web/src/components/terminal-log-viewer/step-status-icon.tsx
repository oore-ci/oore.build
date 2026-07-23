import {
  CircleAlert as AlertCircleIcon,
  CircleCheck as CheckmarkCircle02Icon,
  LoaderCircle as Loading03Icon,
} from 'lucide-react'
export function StepStatusIcon({ status }: { status: string }) {
  const normalized = status.trim().toLowerCase()
  if (normalized === 'running') {
    return (
      <Loading03Icon size={16} className="shrink-0 animate-spin text-info" />
    )
  }
  if (normalized === 'succeeded') {
    return <CheckmarkCircle02Icon size={16} className="shrink-0 text-success" />
  }
  if (
    normalized === 'failed' ||
    normalized === 'canceled' ||
    normalized === 'timed_out'
  ) {
    return <AlertCircleIcon size={16} className="shrink-0 text-destructive" />
  }
  return (
    <span className="flex size-3 shrink-0 items-center justify-center">
      <span className="size-1.5 bg-muted-foreground" />
    </span>
  )
}
