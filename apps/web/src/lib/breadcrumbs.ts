export function resolveBreadcrumbPath(
  path: string,
  params: Record<string, string>,
): string | undefined {
  const keys = [...path.matchAll(/\$([A-Za-z0-9_]+)/g)].map((match) => match[1])
  if (keys.some((key) => !params[key])) return undefined
  return path.replace(/\$([A-Za-z0-9_]+)/g, (_, key: string) =>
    encodeURIComponent(params[key]),
  )
}
