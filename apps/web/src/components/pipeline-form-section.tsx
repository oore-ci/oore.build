import { useId, type ReactNode } from 'react'

import {
  SettingsSection,
  SettingsSurface,
} from '@/components/settings/settings-section'
import { Badge } from '@/components/ui/badge'

/**
 * One top-level block of the pipeline editor: a heading outside one bordered
 * surface, following the settings anatomy in DESIGN.md. Sections are always
 * open; the editor has five of them (Start, Basics, Build, Signing,
 * Triggers) instead of nine equal collapsibles.
 */
export function PipelineFormSection({
  actions,
  children,
  description,
  errorCount,
  title,
}: {
  actions?: ReactNode
  children: ReactNode
  description?: string
  errorCount?: number
  title: string
}) {
  const errorBadge =
    errorCount && errorCount > 0 ? (
      <Badge variant="destructive">
        {errorCount} {errorCount === 1 ? 'error' : 'errors'}
      </Badge>
    ) : null
  return (
    <SettingsSection
      description={description}
      title={title}
      actions={
        errorBadge || actions ? (
          <div className="flex items-center gap-2">
            {errorBadge}
            {actions}
          </div>
        ) : undefined
      }
    >
      <SettingsSurface className="flex flex-col gap-5">
        {children}
      </SettingsSurface>
    </SettingsSection>
  )
}

/**
 * A titled group inside a section surface. Divide consecutive subsections
 * with `SettingsSurfaceSeparator`.
 */
export function PipelineFormSubsection({
  actions,
  children,
  description,
  title,
}: {
  actions?: ReactNode
  children: ReactNode
  description?: ReactNode
  title: string
}) {
  const headingId = useId()
  return (
    <div
      role="group"
      aria-labelledby={headingId}
      className="flex flex-col gap-4"
    >
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 flex-col gap-1">
          <h3 id={headingId} className="text-sm font-medium">
            {title}
          </h3>
          {description ? (
            <p className="max-w-[65ch] text-sm text-muted-foreground">
              {description}
            </p>
          ) : null}
        </div>
        {actions ? <div className="shrink-0">{actions}</div> : null}
      </div>
      {children}
    </div>
  )
}
