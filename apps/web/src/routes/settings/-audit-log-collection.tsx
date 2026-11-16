import { useState, type ReactNode } from 'react'
import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ArrowDown01Icon,
  ArrowRight01Icon,
  Copy01Icon,
  FilterHorizontalIcon,
  SecurityBlockIcon,
  Settings02Icon,
} from '@hugeicons/core-free-icons'
import type { AuditLogCategory, AuditLogEntry } from '@oore/client/models'
import { toast } from 'sonner'

import { CollectionError, CollectionFrame } from '@/components/collection'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'
import { Avatar, AvatarFallback } from '@/components/ui/avatar'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import { Input } from '@/components/ui/input'
import { Skeleton } from '@/components/ui/skeleton'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import {
  auditDayLabel,
  auditInitials,
  describeAuditEntry,
  groupAuditEntries,
} from '@/lib/audit-log'
import type { AuditDescription, AuditRow } from '@/lib/audit-log'
import { formatAbsoluteTime } from '@/lib/format-utils'
import { PAGE_SEARCH } from '@/lib/keyboard-shortcuts'
import { rowVariants, textVariants } from '@/lib/recipes'
import { cn } from '@/lib/utils'

export type AuditKind = AuditLogCategory | 'all'

export const AUDIT_KINDS: ReadonlyArray<{ label: string; value: AuditKind }> = [
  { value: 'all', label: 'All' },
  { value: 'access', label: 'Access' },
  { value: 'builds', label: 'Builds and releases' },
  { value: 'settings', label: 'Settings' },
  { value: 'system', label: 'System' },
]

const KIND_LABELS = Object.fromEntries(
  AUDIT_KINDS.map((kind) => [kind.value, kind.label]),
)

const ROW_TRIGGER_CLASS =
  'grid w-full grid-cols-[1.75rem_minmax(0,1fr)_auto] items-start gap-x-3 gap-y-1 px-4 py-3 text-left outline-none hover:bg-muted/50 focus-visible:bg-muted/50 sm:grid-cols-[3rem_1.75rem_minmax(0,1fr)_auto_auto] sm:px-5'
const ROW_TIME_CLASS =
  'hidden pt-1 font-mono text-xs text-muted-foreground tabular-nums sm:block'
const ROW_BODY_CLASS = 'flex min-w-0 flex-col gap-0.5 sm:pt-0.5'
const ROW_SIDE_CLASS =
  'col-start-2 row-start-2 flex flex-wrap items-center gap-2 sm:col-start-4 sm:row-start-1 sm:pt-0.5'
const ROW_CHEVRON_CLASS =
  'col-start-3 row-start-1 pt-1 text-muted-foreground sm:col-start-5'
const DETAIL_CLASS =
  'flex flex-col gap-4 px-4 pt-1 pb-4 pl-14 sm:pr-5 sm:pl-[6.75rem]'
const FACTS_CLASS =
  'grid grid-cols-1 gap-x-8 gap-y-3 sm:grid-cols-2 lg:grid-cols-3'

function clock(epochSecs: number) {
  return new Date(epochSecs * 1000).toLocaleTimeString(undefined, {
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  })
}

function ActorAvatar({ description }: { description: AuditDescription }) {
  if (description.actorKind === 'person') {
    return (
      <Avatar size="sm" className="size-7" aria-hidden>
        <AvatarFallback className="text-[11px] font-semibold text-foreground">
          {auditInitials(description.actor)}
        </AvatarFallback>
      </Avatar>
    )
  }
  const blocked = description.actorKind === 'unknown'
  return (
    <span
      aria-hidden
      className={cn(
        'flex size-7 items-center justify-center rounded-full',
        blocked
          ? 'bg-destructive/10 text-destructive dark:bg-destructive/20'
          : 'border border-dashed border-input text-muted-foreground',
      )}
    >
      <HugeiconsIcon
        icon={blocked ? SecurityBlockIcon : Settings02Icon}
        className="size-3.5"
      />
    </span>
  )
}

function copyEntry(entries: Array<AuditLogEntry>) {
  const payload = entries.length === 1 ? entries[0] : entries
  void navigator.clipboard
    .writeText(JSON.stringify(payload, null, 2))
    .then(() => toast.success('Copied as JSON'))
    .catch(() => toast.error('Could not copy'))
}

function AuditRowDetail({
  description,
  onShowActor,
  onShowApp,
  row,
}: {
  description: AuditDescription
  onShowActor: (actorId: string) => void
  onShowApp: (name: string) => void
  row: AuditRow
}) {
  const { entry, repeats } = row
  const actorFilter =
    entry.actor_id ?? (description.actorKind === 'system' ? 'system' : null)
  return (
    <div className={DETAIL_CLASS}>
      <dl className={FACTS_CLASS}>
        {description.facts.map((fact) => (
          <div key={fact.label} className="min-w-0">
            <dt className={textVariants({ variant: 'caption' })}>
              {fact.label}
            </dt>
            <dd
              className={cn('wrap-anywhere', fact.mono && 'font-mono text-xs')}
            >
              {fact.value}
            </dd>
          </div>
        ))}
        {repeats.length > 0 ? (
          <div className="min-w-0">
            <dt className={textVariants({ variant: 'caption' })}>
              Also ran at
            </dt>
            <dd className="tabular-nums">
              {repeats.map((repeat) => clock(repeat.created_at)).join(', ')}
            </dd>
          </div>
        ) : null}
      </dl>
      <div className={cn(rowVariants({ gap: 2 }), 'flex-wrap')}>
        {actorFilter ? (
          <Button
            variant="outline"
            size="sm"
            onClick={() => onShowActor(actorFilter)}
          >
            Show only{' '}
            {description.actorKind === 'system'
              ? 'what Oore did'
              : description.actor}
          </Button>
        ) : null}
        {entry.project_name ? (
          <Button
            variant="outline"
            size="sm"
            onClick={() => onShowApp(entry.project_name!)}
          >
            Show only {entry.project_name}
          </Button>
        ) : null}
        {entry.build_id ? (
          <Button
            variant="outline"
            size="sm"
            render={
              <Link
                to="/builds/$buildId"
                params={{ buildId: entry.build_id }}
              />
            }
          >
            Open build
          </Button>
        ) : entry.project_id ? (
          <Button
            variant="outline"
            size="sm"
            render={
              <Link
                to="/projects/$projectId"
                params={{ projectId: entry.project_id }}
              />
            }
          >
            Open app
          </Button>
        ) : null}
        <Button
          variant="outline"
          size="sm"
          onClick={() => copyEntry([entry, ...repeats])}
        >
          <HugeiconsIcon
            icon={Copy01Icon}
            data-icon="inline-start"
            aria-hidden
          />
          Copy as JSON
        </Button>
      </div>
    </div>
  )
}

function AuditRowItem({
  onShowActor,
  onShowApp,
  row,
}: {
  onShowActor: (actorId: string) => void
  onShowApp: (name: string) => void
  row: AuditRow
}) {
  const [open, setOpen] = useState(false)
  const { entry, repeats } = row
  const description = describeAuditEntry(entry)
  return (
    <Collapsible
      open={open}
      onOpenChange={setOpen}
      render={<li data-audit-row className={cn(open && 'bg-muted/30')} />}
    >
      <CollapsibleTrigger className={ROW_TRIGGER_CLASS}>
        <time
          dateTime={new Date(entry.created_at * 1000).toISOString()}
          title={formatAbsoluteTime(entry.created_at)}
          className={ROW_TIME_CLASS}
        >
          {clock(entry.created_at)}
        </time>
        <span className="row-span-2 sm:row-span-1">
          <ActorAvatar description={description} />
        </span>
        <span className={ROW_BODY_CLASS}>
          <span className="text-muted-foreground">
            <span className="font-medium text-foreground">
              {description.actor}
            </span>{' '}
            {description.say.map((part, index) =>
              part.strong ? (
                // oxlint-disable-next-line react/no-array-index-key -- parts are fixed per entry
                <span key={index} className="font-medium text-foreground">
                  {part.text}
                </span>
              ) : (
                part.text
              ),
            )}
          </span>
          {description.meta ? (
            <Text variant="caption" render={<span />}>
              {description.meta}
            </Text>
          ) : null}
        </span>
        <span className={ROW_SIDE_CLASS}>
          <span className="font-mono text-xs text-muted-foreground tabular-nums sm:hidden">
            {clock(entry.created_at)}
          </span>
          {repeats.length > 0 ? (
            <Badge variant="secondary">{repeats.length + 1} runs</Badge>
          ) : null}
          {description.problem ? (
            <Badge variant="destructive">{description.problem}</Badge>
          ) : null}
          <Text variant="caption" render={<span />}>
            {KIND_LABELS[entry.category ?? 'settings']}
          </Text>
        </span>
        <span className={ROW_CHEVRON_CLASS} aria-hidden>
          <HugeiconsIcon
            icon={open ? ArrowDown01Icon : ArrowRight01Icon}
            className="size-4"
          />
        </span>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <AuditRowDetail
          description={description}
          onShowActor={onShowActor}
          onShowApp={onShowApp}
          row={row}
        />
      </CollapsibleContent>
    </Collapsible>
  )
}

function AuditDays({
  entries,
  onShowActor,
  onShowApp,
}: {
  entries: Array<AuditLogEntry>
  onShowActor: (actorId: string) => void
  onShowApp: (name: string) => void
}) {
  return (
    <div className="overflow-hidden rounded-md border">
      {groupAuditEntries(entries).map((day) => (
        <section key={day.date} aria-label={auditDayLabel(day.date)}>
          <h2 className="border-b bg-muted/40 px-4 py-2 text-xs font-medium sm:px-5">
            {auditDayLabel(day.date)}
          </h2>
          <ul className="divide-y border-b in-[section:last-child]:border-b-0">
            {day.rows.map((row) => (
              <AuditRowItem
                key={row.entry.id}
                row={row}
                onShowActor={onShowActor}
                onShowApp={onShowApp}
              />
            ))}
          </ul>
        </section>
      ))}
    </div>
  )
}

function AuditSkeleton() {
  return (
    <div className="rounded-md border" aria-label="Loading audit entries">
      {Array.from({ length: 8 }, (_, index) => (
        <Row key={index} gap={3} className="border-b px-5 py-3 last:border-b-0">
          <Skeleton className="h-4 w-10" />
          <Skeleton className="size-7 rounded-full" />
          <Skeleton className="h-4 w-full max-w-md" />
        </Row>
      ))}
    </div>
  )
}

export function AuditLogCollection({
  activeFilterCount,
  emptyState,
  entries,
  error,
  filters,
  isFiltered,
  isLoading,
  isRefreshing,
  kind,
  onClearFilters,
  onKindChange,
  onPageChange,
  onRetry,
  onSearch,
  onShowActor,
  onShowApp,
  page,
  pageSize,
  query,
  total,
}: {
  /** Person and date filters in effect, shown on the compact Filters button. */
  activeFilterCount: number
  emptyState: ReactNode
  entries: Array<AuditLogEntry>
  error: Error | null
  /** Person and date range controls. */
  filters: ReactNode
  isFiltered: boolean
  isLoading: boolean
  isRefreshing: boolean
  kind: AuditKind
  onClearFilters: () => void
  onKindChange: (kind: AuditKind) => void
  onPageChange: (page: number) => void
  onRetry: () => void
  onSearch: (query: string) => void
  onShowActor: (actorId: string) => void
  onShowApp: (name: string) => void
  page: number
  pageSize: number
  query: string
  total: number
}) {
  const [filtersOpen, setFiltersOpen] = useState(false)
  const pageCount = Math.max(1, Math.ceil(total / pageSize))
  const firstResult = (page - 1) * pageSize + 1
  const lastResult = Math.min(page * pageSize, total)
  const clearFilters = isFiltered ? (
    <Button variant="ghost" onClick={onClearFilters}>
      Clear filters
    </Button>
  ) : null

  return (
    <CollectionFrame
      ariaLabel="Audit activity"
      isBusy={isLoading || isRefreshing}
      className="@container"
    >
      {error ? (
        <CollectionError
          title="Audit log could not be loaded"
          description={error.message}
          onRetry={onRetry}
        />
      ) : null}

      {!isLoading && total === 0 && !error && !isFiltered ? (
        emptyState
      ) : (
        <>
          <Stack gap={2}>
            <Row gap={2}>
              <Input
                type="search"
                {...PAGE_SEARCH}
                value={query}
                placeholder="Search people, actions and apps"
                aria-label="Search people, actions and apps"
                onChange={(event) => onSearch(event.target.value)}
                className="min-w-0 flex-1 @4xl:max-w-xs"
              />
              <Row gap={2} className="ml-auto shrink-0">
                <div className="hidden items-center gap-2 @4xl:flex">
                  {clearFilters}
                  {filters}
                </div>
                <Button
                  variant={filtersOpen ? 'outline' : 'secondary'}
                  className="@4xl:hidden"
                  aria-expanded={filtersOpen}
                  onClick={() => setFiltersOpen((open) => !open)}
                >
                  <HugeiconsIcon
                    icon={FilterHorizontalIcon}
                    data-icon="inline-start"
                    aria-hidden
                  />
                  Filters
                  {activeFilterCount > 0 ? (
                    <span className="rounded-full bg-primary px-1.5 text-xs text-primary-foreground tabular-nums">
                      {activeFilterCount}
                    </span>
                  ) : null}
                </Button>
              </Row>
            </Row>
            {filtersOpen ? (
              <div
                className={cn(
                  rowVariants({ gap: 2 }),
                  'flex-wrap rounded-md border p-2 @4xl:hidden',
                )}
                role="group"
                aria-label="Filters"
              >
                {filters}
                {clearFilters}
              </div>
            ) : null}
            <div className="-mx-1 overflow-x-auto px-1 py-0.5">
              <ToggleGroup
                aria-label="Kind of activity"
                value={[kind]}
                variant="outline"
                size="sm"
                onValueChange={(value) => {
                  const next = AUDIT_KINDS.find(
                    (option) => option.value === value.at(0),
                  )
                  if (next) onKindChange(next.value)
                }}
              >
                {AUDIT_KINDS.map((option) => (
                  <ToggleGroupItem key={option.value} value={option.value}>
                    {option.label}
                  </ToggleGroupItem>
                ))}
              </ToggleGroup>
            </div>
          </Stack>

          {isLoading ? (
            <AuditSkeleton />
          ) : entries.length > 0 ? (
            <AuditDays
              entries={entries}
              onShowActor={onShowActor}
              onShowApp={onShowApp}
            />
          ) : (
            <Stack
              gap={3}
              className="items-center rounded-md border px-4 py-10 text-muted-foreground"
            >
              <span>No matching activity.</span>
              {isFiltered ? (
                <Button variant="outline" size="sm" onClick={onClearFilters}>
                  Clear filters
                </Button>
              ) : null}
            </Stack>
          )}

          {total > 0 ? (
            <Row gap={2} className="justify-between pb-4">
              <Text variant="muted" className="tabular-nums">
                {firstResult}–{lastResult} of {total}
              </Text>
              {pageCount > 1 ? (
                <Row gap={2}>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => onPageChange(page - 1)}
                    disabled={page <= 1}
                  >
                    Previous
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => onPageChange(page + 1)}
                    disabled={page >= pageCount}
                  >
                    Next
                  </Button>
                </Row>
              ) : null}
            </Row>
          ) : null}
        </>
      )}
    </CollectionFrame>
  )
}
