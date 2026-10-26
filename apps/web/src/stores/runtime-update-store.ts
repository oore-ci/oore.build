import { create } from 'zustand'

export type CoordinatedUpdateStep =
  | 'idle'
  | 'backend'
  | 'frontend'
  | 'reloading'
  | 'failed'

interface RuntimeUpdateStoreState {
  step: CoordinatedUpdateStep
  target: string | null
  error: string | null
  setStep: (step: CoordinatedUpdateStep, target?: string | null) => void
  fail: (error: string) => void
}

export const useRuntimeUpdateStore = create<RuntimeUpdateStoreState>((set) => ({
  step: 'idle',
  target: null,
  error: null,
  setStep: (step, target) =>
    set((state) => ({
      step,
      target: target === undefined ? state.target : target,
      error: null,
    })),
  fail: (error) => set({ step: 'failed', error }),
}))
