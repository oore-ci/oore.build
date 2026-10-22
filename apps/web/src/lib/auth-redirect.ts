const OIDC_REDIRECT_KEY = 'oore_auth_redirect'

// Pages that are part of signing in never make sense as a destination.
const AUTH_FLOW_PATHS = ['/login', '/auth', '/setup']

function isAuthFlowPath(pathname: string): boolean {
  return AUTH_FLOW_PATHS.some(
    (path) => pathname === path || pathname.startsWith(`${path}/`),
  )
}

/**
 * Returns `value` as a same-origin path when it is safe to navigate to after
 * sign-in, otherwise null. Rejects absolute URLs, protocol-relative URLs and
 * the sign-in pages themselves.
 */
export function safeRedirectPath(
  value: string | null | undefined,
  origin: string = window.location.origin,
): string | null {
  if (!value?.startsWith('/')) return null
  if (value.startsWith('//') || value.startsWith('/\\')) return null

  let url: URL
  try {
    url = new URL(value, origin)
  } catch {
    return null
  }
  if (url.origin !== new URL(origin).origin) return null
  if (isAuthFlowPath(url.pathname)) return null

  const path = `${url.pathname}${url.search}${url.hash}`
  return path === '/' ? null : path
}

/** Search params for `/login` that bring the person back to `path`. */
export function loginSearchFor(path: string | null | undefined): {
  redirect?: string
} {
  const redirect = safeRedirectPath(path)
  return redirect ? { redirect } : {}
}

/** Short description of where sign-in will continue to, for the login page. */
export function describeRedirectPath(path: string): string {
  const [first, second] = new URL(path, 'http://x').pathname
    .split('/')
    .filter(Boolean)

  switch (first) {
    case 'builds':
      return second ? 'this build' : 'Builds'
    case 'releases':
      return 'this release'
    case 'projects':
      return second ? 'this project' : 'Projects'
    case 'settings':
      return 'Settings'
    default:
      return 'the page you opened'
  }
}

/** Keeps the redirect across the OIDC round trip, which leaves the app. */
export function storeOidcRedirect(path: string | null): void {
  try {
    if (path) sessionStorage.setItem(OIDC_REDIRECT_KEY, path)
    else sessionStorage.removeItem(OIDC_REDIRECT_KEY)
  } catch {
    // sessionStorage unavailable
  }
}

/** Reads and forgets the redirect saved before the OIDC round trip. */
export function takeOidcRedirect(): string | null {
  try {
    const value = sessionStorage.getItem(OIDC_REDIRECT_KEY)
    sessionStorage.removeItem(OIDC_REDIRECT_KEY)
    return safeRedirectPath(value)
  } catch {
    return null
  }
}
