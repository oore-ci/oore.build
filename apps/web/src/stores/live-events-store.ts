import { create } from 'zustand'

interface LiveEventsState {
  /** True while the `/v1/events` stream is open and authorized. */
  connected: boolean
  setConnected: (connected: boolean) => void
}

export const useLiveEventsStore = create<LiveEventsState>((set) => ({
  connected: false,
  setConnected: (connected) => set({ connected }),
}))
