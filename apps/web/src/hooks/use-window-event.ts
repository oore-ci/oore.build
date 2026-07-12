import { useEffect, useEffectEvent } from 'react'

export function useWindowEvent<TEvent extends keyof WindowEventMap>(
  type: TEvent,
  listener: (event: WindowEventMap[TEvent]) => void,
) {
  const onEvent = useEffectEvent(listener)

  useEffect(() => {
    const handleEvent = (event: WindowEventMap[TEvent]) => onEvent(event)
    window.addEventListener(type, handleEvent)
    return () => window.removeEventListener(type, handleEvent)
  }, [type])
}
