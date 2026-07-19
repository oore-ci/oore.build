import * as React from 'react'
import { useMountEffect } from './use-mount-effect'

const MOBILE_BREAKPOINT = 768

export function useIsBelowBreakpoint(breakpoint: number) {
  const [isMobile, setIsMobile] = React.useState<boolean | undefined>(undefined)

  useMountEffect(() => {
    const mql = window.matchMedia(`(max-width: ${breakpoint - 1}px)`)
    const onChange = () => {
      setIsMobile(window.innerWidth < breakpoint)
    }
    mql.addEventListener('change', onChange)
    setIsMobile(window.innerWidth < breakpoint)
    return () => mql.removeEventListener('change', onChange)
  })

  return !!isMobile
}

export function useIsMobile() {
  return useIsBelowBreakpoint(MOBILE_BREAKPOINT)
}
