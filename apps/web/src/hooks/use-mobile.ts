import * as React from 'react'
import { useMountEffect } from '@/hooks/use-mount-effect'

const MOBILE_BREAKPOINT = 768

export function useIsBelowBreakpoint(breakpoint: number) {
  const [matches, setMatches] = React.useState(
    () => window.innerWidth < breakpoint,
  )

  useMountEffect(() => {
    const mql = window.matchMedia(`(max-width: ${breakpoint - 1}px)`)
    const onChange = () => {
      setMatches(mql.matches)
    }
    mql.addEventListener('change', onChange)
    setMatches(mql.matches)
    return () => mql.removeEventListener('change', onChange)
  })

  return matches
}

export function useIsMobile() {
  return useIsBelowBreakpoint(MOBILE_BREAKPOINT)
}
