import { HugeiconsIcon } from '@hugeicons/react'
import { GithubIcon, GitlabIcon } from '@hugeicons/core-free-icons'

import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar'
import { useRepositoryAvatar } from '@/hooks/use-repository-avatar'
import type { ScmProvider } from '@oore/client/models'
import { repositoryInitials } from '@/lib/repository-avatar'
import { useObjectUrl } from '@/hooks/use-object-url'

export default function RepositoryAvatar({
  fullName,
  avatarUrl,
  repositoryId,
  provider,
  size = 'sm',
}: {
  fullName?: string | null
  avatarUrl?: string | null
  repositoryId?: string | null
  provider?: ScmProvider | null
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
      {provider === 'github' || provider === 'gitlab' ? (
        // Without a repository image the provider mark says more than initials.
        <AvatarFallback>
          <HugeiconsIcon
            icon={provider === 'github' ? GithubIcon : GitlabIcon}
            className="size-[60%]"
          />
        </AvatarFallback>
      ) : fullName ? (
        <AvatarFallback>{repositoryInitials(fullName)}</AvatarFallback>
      ) : null}
    </Avatar>
  )
}

function GitLabAvatarImage({ repositoryId }: { repositoryId: string }) {
  const { data: avatarBlob } = useRepositoryAvatar(repositoryId)
  const avatarUrl = useObjectUrl(avatarBlob)

  return avatarUrl ? <AvatarImage src={avatarUrl} alt="" /> : null
}
