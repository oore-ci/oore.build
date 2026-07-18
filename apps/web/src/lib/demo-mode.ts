const READ_ONLY_REASON = 'Action not allowed on demo.'

export const isDemoMode = import.meta.env.VITE_DEMO_MODE === 'true'

export function isLocalDemoHost(hostname: string): boolean {
  return (
    hostname === 'localhost' ||
    hostname === '127.0.0.1' ||
    hostname === '0.0.0.0' ||
    hostname === '[::1]' ||
    hostname === '::1'
  )
}

export function isDemoMutationAllowed(
  method: string,
  path: string,
  hostname = typeof window === 'undefined' ? '' : window.location.hostname,
): boolean {
  if (method === 'GET' || method === 'HEAD') return true
  if (isLocalDemoHost(hostname)) return true
  return (
    path.startsWith('/v1/auth/') ||
    (method === 'POST' && path === '/v1/telemetry/web-performance') ||
    path === '/v1/artifacts/query' ||
    path.includes('/validate') ||
    path.endsWith('/download-link') ||
    path.endsWith('/install-link')
  )
}

export function isDemoMutationBlocked(method: string, path: string): boolean {
  return isDemoMode && !isDemoMutationAllowed(method, path)
}

export { READ_ONLY_REASON }
