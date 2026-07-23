import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar'
import { useRepositoryAvatar } from '@/hooks/use-repository-avatar'
import type { ScmProvider } from '@/lib/types'
import { repositoryInitials } from '@/lib/repository-avatar'

export default function RepositoryAvatar({
  fullName,
  avatarUrl,
  repositoryId,
  provider,
  size = 'sm',
}: {
  fullName?: string
  avatarUrl?: string
  repositoryId?: string
  provider?: ScmProvider
  size?: 'sm' | 'default' | 'lg'
}) {
  const useGitLabProxy = provider === 'gitlab' && !!repositoryId && !!avatarUrl

  return (
    <Avatar size={size} aria-hidden="true">
      {useGitLabProxy ? (
        <GitLabAvatarImage repositoryId={repositoryId} />
      ) : avatarUrl ? (
        <AvatarImage src={avatarUrl} alt="" referrerPolicy="no-referrer" />
      ) : null}
      {fullName && (
        <AvatarFallback>{repositoryInitials(fullName)}</AvatarFallback>
      )}
    </Avatar>
  )
}

function GitLabAvatarImage({ repositoryId }: { repositoryId: string }) {
  const { data: avatarUrl } = useRepositoryAvatar(repositoryId)
  return avatarUrl ? <AvatarImage src={avatarUrl} alt="" /> : null
}
