const READ_ONLY_REASON = 'Action not allowed on demo.'

export const isDemoMode = import.meta.env.VITE_DEMO_MODE === 'true'

export function isDemoMutationAllowed(method: string, path: string): boolean {
  if (method === 'GET' || method === 'HEAD') return true
  return (
    path.startsWith('/v1/auth/') ||
    path.includes('/validate') ||
    path.endsWith('/download-link') ||
    path.endsWith('/install-link')
  )
}

export function isDemoMutationBlocked(method: string, path: string): boolean {
  return isDemoMode && !isDemoMutationAllowed(method, path)
}

export { READ_ONLY_REASON }
