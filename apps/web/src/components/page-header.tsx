interface PageHeaderProps {
  title: string
  description?: string
  actions?: React.ReactNode
  meta?: React.ReactNode
}

export default function PageHeader({
  title,
  description,
  actions,
  meta,
}: PageHeaderProps) {
  return (
    <header className="space-y-3">
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
