import { useId, type ReactNode } from 'react'

import {
  SectionDescription,
  SettingsSection,
  SettingsSurface,
  warnLongDescription,
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
  id,
  title,
}: {
  actions?: ReactNode
  children: ReactNode
  description?: string
  errorCount?: number
  /** Anchor used by the section index and by deep links into the editor. */
  id?: string
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
      id={id}
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
  id,
  title,
}: {
  actions?: ReactNode
  children: ReactNode
  description?: string
  id?: string
  title: string
}) {
  const headingId = useId()
  warnLongDescription('PipelineFormSubsection', description)
  return (
    <div
      id={id}
      role="group"
      aria-labelledby={headingId}
      className="flex scroll-mt-[calc(var(--header-height)+4rem)] flex-col gap-4"
    >
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 flex-col gap-1">
          <h3 id={headingId} className="text-sm font-medium">
            {title}
          </h3>
          {description ? (
            <SectionDescription>{description}</SectionDescription>
          ) : null}
        </div>
        {actions ? <div className="shrink-0">{actions}</div> : null}
      </div>
      {children}
    </div>
  )
}
