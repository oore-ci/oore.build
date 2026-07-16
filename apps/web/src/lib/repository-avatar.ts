export function repositoryInitials(fullName: string): string {
  const repositoryName = fullName.split('/').filter(Boolean).at(-1) ?? fullName
  return (
    repositoryName
      .replaceAll(/[^a-z0-9]/gi, '')
      .slice(0, 2)
      .toUpperCase() || 'R'
  )
}
