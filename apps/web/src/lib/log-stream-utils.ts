import type { BuildLogChunk } from '@/lib/types'

export interface MergeBuildLogChunksResult {
  changed: boolean
  logs: Array<BuildLogChunk>
  lastSequence: number
}

export function createLogFrameBatcher(
  onFlush: (chunks: Array<BuildLogChunk>) => void,
  schedule: (callback: () => void) => number = requestAnimationFrame,
  cancel: (handle: number) => void = cancelAnimationFrame,
) {
  let frame: number | null = null
  let queued: Array<BuildLogChunk> = []

  const flush = () => {
    if (frame !== null) cancel(frame)
    frame = null
    if (queued.length === 0) return
    const chunks = queued
    queued = []
    onFlush(chunks)
  }

  return {
    enqueue(chunk: BuildLogChunk) {
      queued.push(chunk)
      if (frame === null) {
        frame = schedule(() => {
          frame = null
          flush()
        })
      }
    },
    flush,
    cancel() {
      if (frame !== null) cancel(frame)
      frame = null
      queued = []
    },
  }
}

function findLogInsertIndex(
  logs: Array<BuildLogChunk>,
  sequence: number,
): number {
  let low = 0
  let high = logs.length

  while (low < high) {
    const mid = Math.floor((low + high) / 2)
    if (logs[mid].sequence < sequence) {
      low = mid + 1
    } else {
      high = mid
    }
  }

  return low
}

export function mergeBuildLogChunks(
  currentLogs: Array<BuildLogChunk>,
  logsBySequence: Map<number, BuildLogChunk>,
  chunks: Array<BuildLogChunk>,
): MergeBuildLogChunksResult {
  let nextLogs = currentLogs
  let changed = false

  for (const chunk of chunks) {
    const existing = logsBySequence.get(chunk.sequence)
    if (
      existing &&
      existing.content === chunk.content &&
      existing.stream === chunk.stream
    ) {
      continue
    }

    logsBySequence.set(chunk.sequence, chunk)
    if (nextLogs === currentLogs) {
      nextLogs = [...currentLogs]
    }

    const index =
      nextLogs.length > 0 &&
      chunk.sequence > nextLogs[nextLogs.length - 1].sequence
        ? nextLogs.length
        : findLogInsertIndex(nextLogs, chunk.sequence)

    if (nextLogs[index]?.sequence === chunk.sequence) {
      nextLogs[index] = chunk
    } else {
      nextLogs.splice(index, 0, chunk)
    }

    changed = true
  }

  return {
    changed,
    logs: nextLogs,
    lastSequence:
      nextLogs.length > 0 ? nextLogs[nextLogs.length - 1].sequence : -1,
  }
}

export function mergeBuildLogSnapshots(
  streamedLogs: Array<BuildLogChunk>,
  finalLogs: Array<BuildLogChunk>,
): Array<BuildLogChunk> {
  if (streamedLogs.length === 0) return finalLogs
  if (finalLogs.length === 0) return streamedLogs

  const logsBySequence = new Map(
    streamedLogs.map((chunk) => [chunk.sequence, chunk]),
  )
  return mergeBuildLogChunks(streamedLogs, logsBySequence, finalLogs).logs
}
