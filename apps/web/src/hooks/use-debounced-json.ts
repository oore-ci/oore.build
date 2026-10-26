import { useEffect, useRef } from 'react'

/**
 * Hands `value` to `apply` once edits settle. The first value goes out at
 * once so a panel does not start empty. Values are compared by their JSON,
 * so a new but equal object does not restart the timer.
 */
export function useDebouncedJson<T>(
  value: T,
  delay: number,
  apply: (value: T) => void,
) {
  const key = JSON.stringify(value)
  const first = useRef(true)
  const applyRef = useRef(apply)
  useEffect(() => {
    applyRef.current = apply
  }, [apply])
  useEffect(() => {
    const wait = first.current ? 0 : delay
    first.current = false
    // SAFETY: `key` is the JSON of a `T` built in this render.
    const timer = setTimeout(() => applyRef.current(JSON.parse(key) as T), wait)
    return () => clearTimeout(timer)
  }, [key, delay])
}
