import { isOoreApiError, type OoreApiError } from '@oore/client/client'

export type SessionEndReason = 'expired' | 'rejected'

interface SessionEnd {
  reason: SessionEndReason
  /** The token that ended. Listeners ignore it once a newer one is active. */
  token: string
}

type Listener = (end: SessionEnd) => void

const listeners = new Set<Listener>()

export function onSessionEnded(listener: Listener): () => void {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

export function reportSessionEnded(end: SessionEnd): void {
  for (const listener of listeners) listener(end)
}

/** True for an API error that says the request's credentials are no good. */
export function isUnauthorizedError(cause: unknown): cause is OoreApiError {
  return isOoreApiError(cause) && cause.status === 401
}

/**
 * Reports a 401 from an authenticated request. Requests sent without a
 * session, such as sign-in attempts, are left to their own error handling.
 */
export function reportApiError(cause: unknown): void {
  if (!isUnauthorizedError(cause)) return
  const token = cause.request?.headers
    .get('Authorization')
    ?.replace(/^Bearer\s+/i, '')
  if (token) reportSessionEnded({ reason: 'rejected', token })
}
