import { useSyncExternalStore } from 'react'

let time = Date.now()
let timer: ReturnType<typeof setInterval>
const listeners = new Set<() => void>()

function subscribe(listener: () => void) {
  listeners.add(listener)
  if (listeners.size === 1) {
    time = Date.now()
    timer = setInterval(() => {
      time = Date.now()
      for (const notify of listeners) notify()
    }, 1000)
  }
  return () => {
    listeners.delete(listener)
    if (listeners.size === 0) clearInterval(timer)
  }
}

export function useTime(): number
export function useTime<T>(select: (time: number) => T): T
export function useTime<T>(select?: (time: number) => T) {
  return useSyncExternalStore(subscribe, () => (select ? select(time) : time))
}
