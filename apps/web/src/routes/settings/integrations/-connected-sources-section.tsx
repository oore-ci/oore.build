import {
  Info as InformationCircleIcon,
  Link2 as Link04Icon,
  Search as Search01Icon,
} from 'lucide-react'

import type { Integration } from '@/lib/types'
import type { SortDirection } from '@/components/collection-controls'
import { CollectionSearchInput } from '@/components/collection-search-input'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import { NativeSelect, NativeSelectOption } from '@/components/ui/native-select'
import { SourceInventory } from './-source-inventory'
import type { IntegrationSort } from './-source-inventory'

const sortOptions: Record<IntegrationSort, string> = {
  name: 'Name',
  provider: 'Provider',
  status: 'Status',
  updated_at: 'Recently updated',
}

export function ConnectedSourcesSection({
  canWrite,
  direction,
  error,
  integrations,
  isLoading,
  onClearSearch,
  onPageChange,
  onPageSizeChange,
  onRetry,
  onSearch,
  onSortChange,
  page,
  pageSize,
  search,
  sort,
  total,
}: {
  canWrite: boolean
  direction: SortDirection
  error: Error | null
  integrations: Array<Integration>
  isLoading: boolean
  onClearSearch: () => void
  onPageChange: (page: number) => void
  onPageSizeChange: (size: number) => void
  onRetry: () => void
  onSearch: (query: string) => void
  onSortChange: (sort: IntegrationSort, direction: SortDirection) => void
  page: number
  pageSize: number
  search?: string
  sort: IntegrationSort
  total: number
}) {
  const isEmpty = !isLoading && !error && total === 0
  return (
    <section
      aria-label="Connected sources"
      className="flex min-w-0 flex-col gap-4"
    >
      {isLoading || total > 0 || search ? (
        <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <CollectionSearchInput
            initialValue={search ?? ''}
            onSearch={onSearch}
            placeholder="Search connected sources"
            ariaLabel="Search connected sources"
          />
          <NativeSelect
            className="w-full sm:hidden"
            aria-label="Sort connected sources"
            value={sort}
            onChange={(event) =>
              onSortChange(event.target.value as IntegrationSort, direction)
            }
          >
            {Object.entries(sortOptions).map(([value, label]) => (
              <NativeSelectOption key={value} value={value}>
                {label}
              </NativeSelectOption>
            ))}
          </NativeSelect>
        </div>
      ) : null}
      {error ? (
        <Alert variant="destructive">
          <InformationCircleIcon size={16} />
          <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <span>Failed to load sources: {error.message}</span>
            <Button variant="outline" size="sm" onClick={onRetry}>
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      ) : null}
      {isEmpty && !search ? (
        <Empty className="bg-card">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <Link04Icon />
            </EmptyMedia>
            <EmptyTitle>No connected sources</EmptyTitle>
            <EmptyDescription>
              {canWrite
                ? 'Choose GitHub or GitLab below to discover repositories.'
                : 'An owner or admin can connect the first source.'}
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : null}
      {isEmpty && search ? (
        <Empty className="bg-card">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <Search01Icon />
            </EmptyMedia>
            <EmptyTitle>No matching sources</EmptyTitle>
            <EmptyDescription>
              Try a different search or clear the current query.
            </EmptyDescription>
          </EmptyHeader>
          <EmptyContent>
            <Button variant="outline" onClick={onClearSearch}>
              Clear search
            </Button>
          </EmptyContent>
        </Empty>
      ) : null}
      {!error && (isLoading || total > 0) ? (
        <SourceInventory
          direction={direction}
          integrations={integrations}
          isLoading={isLoading}
          onPageChange={onPageChange}
          onPageSizeChange={onPageSizeChange}
          onSortChange={onSortChange}
          page={page}
          pageSize={pageSize}
          sort={sort}
          total={total}
        />
      ) : null}
    </section>
  )
}
