import type { ReactNode } from 'react'

export function LocalOnlySourcesNotice({ actions }: { actions?: ReactNode }) {
  return (
    <section
      className="space-y-3 border-t pt-4"
      aria-labelledby="local-only-sources-title"
    >
      <div>
        <h2 id="local-only-sources-title" className="text-sm font-semibold">
          Local Only mode
        </h2>
        <p className="mt-2 text-sm text-muted-foreground">
          Create projects from paths on this Mac. GitHub and GitLab sources
          become available when External Access is configured.
        </p>
      </div>
      {actions ? <div className="flex flex-wrap gap-2">{actions}</div> : null}
    </section>
  )
}
