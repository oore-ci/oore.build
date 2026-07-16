import { HugeiconsIcon } from '@hugeicons/react'
import { Search01Icon, UserMultiple02Icon } from '@hugeicons/core-free-icons'

import { Button } from '@/components/ui/button'
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import { InviteUserAction } from './-invite-user-action'

export function UsersEmptyState({
  onClearSearch,
  state,
}: {
  onClearSearch: () => void
  state: 'empty' | 'no-results' | null
}) {
  if (!state) return null
  return (
    <Empty className="bg-card">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <HugeiconsIcon
            icon={state === 'empty' ? UserMultiple02Icon : Search01Icon}
          />
        </EmptyMedia>
        <EmptyTitle>
          {state === 'empty' ? 'No users yet' : 'No matching users'}
        </EmptyTitle>
        <EmptyDescription>
          {state === 'empty'
            ? 'Invite the first person who needs access to this instance.'
            : 'Try a different search or clear the current query.'}
        </EmptyDescription>
      </EmptyHeader>
      <EmptyContent>
        {state === 'empty' ? (
          <InviteUserAction />
        ) : (
          <Button variant="outline" onClick={onClearSearch}>
            Clear search
          </Button>
        )}
      </EmptyContent>
    </Empty>
  )
}
