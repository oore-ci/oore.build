import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar'

export function repositoryInitials(fullName: string): string {
  const repositoryName = fullName.split('/').filter(Boolean).at(-1) ?? fullName
  return (
    repositoryName
      .replaceAll(/[^a-z0-9]/gi, '')
      .slice(0, 2)
      .toUpperCase() || 'R'
  )
}

export default function RepositoryAvatar({
  fullName,
  avatarUrl,
  size = 'sm',
}: {
  fullName: string
  avatarUrl?: string
  size?: 'sm' | 'default' | 'lg'
}) {
  return (
    <Avatar size={size} aria-hidden="true">
      {avatarUrl ? (
        <AvatarImage src={avatarUrl} alt="" referrerPolicy="no-referrer" />
      ) : null}
      <AvatarFallback>{repositoryInitials(fullName)}</AvatarFallback>
    </Avatar>
  )
}
