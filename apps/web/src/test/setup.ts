// Provide a localStorage mock for jsdom test environment.
// jsdom throws SecurityError on any localStorage access without --localstorage-file.
function needsLocalStorageMock(): boolean {
  try {
    globalThis.localStorage.getItem('__test__')
    return false
  } catch {
    return true
  }
}

if (needsLocalStorageMock()) {
  const store = new Map<string, string>()
  const localStorageMock: Storage = {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => {
      store.set(key, String(value))
    },
    removeItem: (key: string) => {
      store.delete(key)
    },
    clear: () => {
      store.clear()
    },
    get length() {
      return store.size
    },
    key: (index: number) => [...store.keys()][index] ?? null,
  }
  Object.defineProperty(globalThis, 'localStorage', {
    value: localStorageMock,
    writable: true,
  })
}
