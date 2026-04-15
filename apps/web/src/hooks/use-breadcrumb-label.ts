import { useEffect } from 'react'

export function useBreadcrumbLabel(
  setLabel: (routeId: string, label: string) => void,
  routeId: string,
  label: string | undefined,
) {
  useEffect(() => {
    if (label) setLabel(routeId, label)
  }, [routeId, label, setLabel])
}
