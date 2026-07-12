import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { ArrowLeft02Icon } from '@hugeicons/core-free-icons'

interface PageHeaderProps {
  title: string
  description?: string
  actions?: React.ReactNode
  back?: { to: string; label: string }
  meta?: React.ReactNode
}

export default function PageHeader({
  title,
  description,
  actions,
  back,
  meta,
}: PageHeaderProps) {
  return (
    <header className="space-y-3">
      {back && (
        <Link
          to={back.to}
          className="inline-flex items-center gap-1.5 text-xs font-medium uppercase tracking-wider text-muted-foreground transition-colors hover:text-foreground"
        >
          <HugeiconsIcon icon={ArrowLeft02Icon} size={14} />
          {back.label}
        </Link>
      )}

      <div className="flex flex-col gap-4 lg:flex-row lg:items-end lg:justify-between">
        <div className="min-w-0 space-y-1">
          <h1 className="text-3xl font-bold tracking-tight">{title}</h1>
          {description ? (
            <p className="text-sm text-muted-foreground">{description}</p>
          ) : null}
        </div>
        {actions ? (
          <div className="flex flex-wrap items-center gap-2">{actions}</div>
        ) : null}
      </div>

      {meta ? (
        <div className="flex flex-wrap items-center gap-2 pt-2 text-xs text-muted-foreground">
          {meta}
        </div>
      ) : null}
    </header>
  )
}
