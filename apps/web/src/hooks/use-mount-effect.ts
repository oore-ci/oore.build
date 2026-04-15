import { useEffect } from 'react'

/**
 * Runs an effect exactly once on mount. Accepts an optional cleanup function.
 * This is the ONLY sanctioned way to call useEffect directly.
 *
 * For effects that need to re-run when deps change, use a different pattern:
 * - Derive state inline (Rule 1)
 * - Use TanStack Query for data fetching (Rule 2)
 * - Use event handlers (Rule 3)
 * - Use key-based remounting (Rule 5)
 */
export function useMountEffect(effect: () => void | (() => void)) {
  useEffect(effect, [])
}
