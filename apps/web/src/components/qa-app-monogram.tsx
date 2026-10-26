import { Avatar, AvatarFallback } from '@/components/ui/avatar'
import { repositoryInitials } from '@/lib/repository-avatar'

/** App identity for QA viewers: a monogram, never the Git provider's logo. */
export default function QaAppMonogram({ name }: { name: string }) {
  return (
    <Avatar size="sm" className="rounded-md after:rounded-md" aria-hidden>
      <AvatarFallback className="rounded-md bg-primary/10 font-medium text-primary">
        {repositoryInitials(name)}
      </AvatarFallback>
    </Avatar>
  )
}
