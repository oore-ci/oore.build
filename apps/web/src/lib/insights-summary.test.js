import { describe, expect, it } from 'bun:test'
import {
  appHealthLine,
  buildHealthLine,
  storeHealthLine,
} from './insights-summary'

const totals = (finished, failed, rate) => ({
  finished,
  failed,
  succeeded: finished - failed,
  canceled: 0,
  expired: 0,
  timed_out: 0,
  success_rate: rate,
})

describe('buildHealthLine', () => {
  it('is quiet when builds go as usual', () => {
    expect(
      buildHealthLine({
        totals: totals(20, 1, 0.95),
        previous_totals: totals(20, 1, 0.95),
        window_days: 7,
        projects: [],
      }),
    ).toBeNull()
  })

  it('names a drop from last period', () => {
    expect(
      buildHealthLine({
        totals: totals(20, 6, 0.7),
        previous_totals: totals(20, 2, 0.9),
        window_days: 7,
        projects: [],
      }),
    ).toBe('70% of builds succeeded in the last 7 days, down from 90%.')
  })
})

describe('appHealthLine', () => {
  it('names an app failing most builds', () => {
    expect(
      appHealthLine({
        projects: [
          { project_name: 'Shop', totals: totals(6, 4, 0.33) },
          { project_name: 'Tiny', totals: totals(2, 2, 0) },
        ],
      }),
    ).toBe('Shop failed 4 of its last 6 builds.')
  })
})

describe('storeHealthLine', () => {
  it('prefers breaches over warnings', () => {
    expect(storeHealthLine({ breached: 2, warning: 1 })).toBe(
      "2 apps are past a store's crash or ANR threshold.",
    )
    expect(storeHealthLine({ breached: 0, warning: 0 })).toBeNull()
  })
})
