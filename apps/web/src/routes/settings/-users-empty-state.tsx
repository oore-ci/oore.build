import {
  Search as Search01Icon,
  Users as UserMultiple02Icon,
} from 'lucide-react'

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
  const Icon = state === 'empty' ? UserMultiple02Icon : Search01Icon

  return (
    <Empty className="bg-card">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <Icon />
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
