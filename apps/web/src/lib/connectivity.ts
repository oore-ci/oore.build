export type ConnectivityIssueKind = 'mixed_content' | 'network_unreachable'

export interface ConnectivityIssue {
  kind: ConnectivityIssueKind
  title: string
  description: string
}

function parseUrl(value: string): URL | null {
  try {
    return new URL(value)
  } catch {
    return null
  }
}

export function isLoopbackHostname(hostname: string): boolean {
  return (
    hostname === 'localhost' ||
    hostname === '127.0.0.1' ||
    hostname === '::1' ||
    hostname === '[::1]'
  )
}

export function resolveUrlHostname(value: string | null | undefined): string {
  const trimmed = value?.trim() ?? ''
  return trimmed ? (parseUrl(trimmed)?.hostname ?? '') : ''
}

export function isHostedUiOrigin(origin: string): boolean {
  const parsed = parseUrl(origin)
  return parsed?.hostname === 'ci.oore.build'
}

export function isLoopbackUrl(value: string): boolean {
  const parsed = parseUrl(value)
  if (!parsed) return false
  return parsed.hostname === 'localhost' || parsed.hostname === '127.0.0.1'
}

export function isLocalLauncherOrigin(origin: string): boolean {
  const parsed = parseUrl(origin)
  if (!parsed) return false
  if (isHostedUiOrigin(origin)) return false
  return (
    parsed.protocol === 'http:' &&
    (parsed.hostname === 'localhost' || parsed.hostname === '127.0.0.1') &&
    parsed.port === '4173'
  )
}

export function isMixedContentBlocked(
  frontendOrigin: string,
  backendUrl: string,
): boolean {
  const frontend = parseUrl(frontendOrigin)
  const backend = parseUrl(backendUrl)
  if (!frontend || !backend) return false
  return frontend.protocol === 'https:' && backend.protocol === 'http:'
}

export function isLikelyFetchNetworkError(error: unknown): boolean {
  if (!(error instanceof Error)) return false
  const message = error.message.toLowerCase()
  return (
    message.includes('failed to fetch') ||
    message.includes('network') ||
    message.includes('load failed')
  )
}

export function getConnectivityIssue(
  backendUrl: string,
  error: unknown,
  frontendOrigin: string,
): ConnectivityIssue | null {
  if (isMixedContentBlocked(frontendOrigin, backendUrl)) {
    return {
      kind: 'mixed_content',
      title: 'Browser blocked mixed content',
      description:
        'This UI is loaded over HTTPS but your backend URL uses HTTP. Browsers block this combination.',
    }
  }

  if (isLikelyFetchNetworkError(error)) {
    return {
      kind: 'network_unreachable',
      title: 'Cannot reach backend instance',
      description:
        'The backend did not respond from this browser session. It may not be publicly reachable from your current network.',
    }
  }

  return null
}
