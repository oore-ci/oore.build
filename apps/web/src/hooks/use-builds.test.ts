import { describe, expect, it } from 'vitest'

import type { Build, BuildStatus } from '@/lib/types'
import { hasActiveBuilds } from '@/hooks/use-builds'

function build(status: BuildStatus): Build {
  return {
    id: 'build-1',
    project_id: 'project-1',
    pipeline_id: 'pipeline-1',
    build_number: 1,
    status,
    trigger_type: 'manual',
    config_snapshot: {},
    queued_at: 0,
    created_at: 0,
    updated_at: 0,
  }
}

describe('hasActiveBuilds', () => {
  it('only polls lists that contain a non-terminal build', () => {
    expect(hasActiveBuilds({ builds: [build('running')], total: 1 })).toBe(true)
    expect(hasActiveBuilds({ builds: [build('succeeded')], total: 1 })).toBe(
      false,
    )
  })
})
