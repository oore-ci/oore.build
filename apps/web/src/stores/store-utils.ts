import { createSignal, onCleanup, type Accessor } from 'solid-js'

type Listener = () => void

export type StoreSetter<T extends object> = (
  partial: Partial<T> | ((state: T) => Partial<T>),
) => void

export type StoreGetter<T extends object> = () => T

export interface SelectorStore<T extends object> {
  <R>(selector: (state: T) => R): Accessor<R>
  getState: StoreGetter<T>
  setState: StoreSetter<T>
  subscribe: (listener: Listener) => () => void
}

export function createSelectorStore<T extends object>(
  initializer: (set: StoreSetter<T>, get: StoreGetter<T>) => T,
): SelectorStore<T> {
  const listeners = new Set<Listener>()

  let state: T

  const notify = () => {
    for (const listener of listeners) listener()
  }

  const setState: StoreSetter<T> = (partial) => {
    const patch =
      typeof partial === 'function'
        ? (partial as (value: T) => Partial<T>)(state)
        : partial
    state = { ...state, ...patch }
    notify()
  }

  const getState: StoreGetter<T> = () => state

  state = initializer(setState, getState)

  const subscribe = (listener: Listener) => {
    listeners.add(listener)
    return () => {
      listeners.delete(listener)
    }
  }

  const useStore = (<R>(selector: (value: T) => R) => {
    const [selected, setSelected] = createSignal(selector(state))
    const unsubscribe = subscribe(() => {
      setSelected(() => selector(state))
    })
    onCleanup(unsubscribe)
    return selected
  }) as SelectorStore<T>

  useStore.getState = getState
  useStore.setState = setState
  useStore.subscribe = subscribe

  return useStore
}
