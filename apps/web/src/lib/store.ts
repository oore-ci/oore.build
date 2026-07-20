import { Store, useSelector } from '@tanstack/react-store'

type StateUpdate<T> = Partial<T> | ((state: T) => Partial<T>)
type SetState<T> = (update: StateUpdate<T>) => void

interface PersistOptions<T> {
  name: string
  partialize?: (state: T) => Partial<T>
}

export interface BoundStore<T> {
  (): T
  <Selected>(selector: (state: T) => Selected): Selected
  getState: () => T
  setState: SetState<T>
  subscribe: (listener: (state: T, previousState: T) => void) => () => void
}

function loadPersistedState<T>(name: string): Partial<T> {
  try {
    const value = localStorage.getItem(name)
    if (!value) return {}
    const state = (JSON.parse(value) as { state?: unknown }).state
    return state && typeof state === 'object' ? (state as Partial<T>) : {}
  } catch {
    return {}
  }
}

export function createBoundStore<T>(
  initializer: (set: SetState<T>, get: () => T) => T,
  persist?: PersistOptions<T>,
): BoundStore<T> {
  let store: Store<T>
  const getState = () => store.state
  const setState: SetState<T> = (update) =>
    store.setState((state) => ({
      ...state,
      ...(typeof update === 'function' ? update(state) : update),
    }))

  const initialState = initializer(setState, getState)
  store = new Store(
    persist
      ? { ...initialState, ...loadPersistedState<T>(persist.name) }
      : initialState,
  )

  if (persist) {
    store.subscribe((state) => {
      try {
        localStorage.setItem(
          persist.name,
          JSON.stringify({
            state: persist.partialize?.(state) ?? state,
            version: 0,
          }),
        )
      } catch {
        // localStorage unavailable
      }
    })
  }

  function useBoundStore<Selected = T>(
    selector: (state: T) => Selected = (state) => state as unknown as Selected,
  ): Selected {
    return useSelector(store, selector)
  }

  return Object.assign(useBoundStore, {
    getState,
    setState,
    subscribe: (listener: (state: T, previousState: T) => void) => {
      let previousState = store.state
      const subscription = store.subscribe((state) => {
        listener(state, previousState)
        previousState = state
      })
      return subscription.unsubscribe
    },
  })
}
