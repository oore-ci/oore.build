import type { BuildPlatform } from '@oore/client/models'

export const ALL_PLATFORMS = 'all'

/**
 * The workflow and platform a plan request should use. A stored choice
 * counts only while the configuration still offers it, so removing the
 * chosen workflow or platform falls back to the defaults instead of
 * leaving the panel stuck on an error with its select hidden. The stored
 * choice is kept and applies again once it is offered.
 */
export function effectivePlanSelection({
  platform,
  platforms,
  workflow,
  workflows,
}: {
  platform: string
  /** Platforms the configuration builds. */
  platforms: ReadonlyArray<BuildPlatform>
  workflow: string | null
  /** Workflows the last plan reported. */
  workflows: ReadonlyArray<string>
}) {
  return {
    platform: platforms.find((candidate) => candidate === platform),
    workflow:
      workflow !== null && workflows.includes(workflow) ? workflow : undefined,
  }
}
