import { describe, expect, it } from 'vitest'

import type { BuildLogChunk } from '@/lib/types'
import {
  mergeBuildLogChunks,
  mergeBuildLogSnapshots,
} from '@/lib/log-stream-utils'

function chunk(sequence: number, content: string): BuildLogChunk {
  return { sequence, content, stream: 'stdout' }
}

describe('mergeBuildLogChunks', () => {
  it('appends monotonic chunks without resorting the full log set', () => {
    const logsBySequence = new Map<number, BuildLogChunk>([
      [1, chunk(1, 'one')],
      [2, chunk(2, 'two')],
    ])
    const currentLogs = [chunk(1, 'one'), chunk(2, 'two')]

    const result = mergeBuildLogChunks(currentLogs, logsBySequence, [
      chunk(3, 'three'),
      chunk(4, 'four'),
    ])

    expect(result.changed).toBe(true)
    expect(result.logs.map((log) => log.sequence)).toEqual([1, 2, 3, 4])
    expect(result.lastSequence).toBe(4)
  })

  it('inserts out-of-order backfill chunks in sequence order', () => {
    const one = chunk(1, 'one')
    const three = chunk(3, 'three')
    const logsBySequence = new Map<number, BuildLogChunk>([
      [one.sequence, one],
      [three.sequence, three],
    ])

    const result = mergeBuildLogChunks([one, three], logsBySequence, [
      chunk(2, 'two'),
    ])

    expect(result.logs.map((log) => log.sequence)).toEqual([1, 2, 3])
    expect(result.lastSequence).toBe(3)
  })

  it('replaces changed duplicate sequence chunks in place', () => {
    const oldTwo = chunk(2, 'old')
    const logsBySequence = new Map<number, BuildLogChunk>([
      [1, chunk(1, 'one')],
      [2, oldTwo],
      [3, chunk(3, 'three')],
    ])

    const result = mergeBuildLogChunks(
      [...logsBySequence.values()],
      logsBySequence,
      [chunk(2, 'new')],
    )

    expect(result.logs.map((log) => log.content)).toEqual([
      'one',
      'new',
      'three',
    ])
    expect(result.lastSequence).toBe(3)
  })

  it('ignores exact duplicate chunks and preserves the existing array', () => {
    const one = chunk(1, 'one')
    const currentLogs = [one]
    const logsBySequence = new Map([[one.sequence, one]])

    const result = mergeBuildLogChunks(currentLogs, logsBySequence, [
      chunk(1, 'one'),
    ])

    expect(result.changed).toBe(false)
    expect(result.logs).toBe(currentLogs)
    expect(result.lastSequence).toBe(1)
  })

  it('keeps streamed lines while the terminal snapshot catches up', () => {
    const result = mergeBuildLogSnapshots(
      [chunk(1, 'one'), chunk(2, 'two'), chunk(3, 'three')],
      [chunk(1, 'one'), chunk(2, 'two')],
    )

    expect(result.map((log) => log.content)).toEqual(['one', 'two', 'three'])
  })
})
