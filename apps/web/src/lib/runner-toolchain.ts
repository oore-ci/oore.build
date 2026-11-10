import type {
  BuildPlatform,
  Runner,
  RunnerToolchainCheck,
} from '@oore/client/models'

export const PLATFORM_LABEL = {
  android: 'Android',
  ios: 'iOS',
  macos: 'macOS',
} satisfies Record<BuildPlatform, string>

/** Platforms that share their checks show as one group. */
export const PLATFORM_GROUPS: ReadonlyArray<{
  label: string
  platform: BuildPlatform
}> = [
  { label: 'Android', platform: 'android' },
  { label: 'iOS and macOS', platform: 'ios' },
]

/** The checks a build for `platform` depends on. */
export function checksFor(
  runner: Pick<Runner, 'toolchain'>,
  platform: BuildPlatform,
): Array<RunnerToolchainCheck> {
  return (runner.toolchain ?? []).filter(
    (check) =>
      (check.platforms ?? []).length === 0 ||
      (check.platforms ?? []).includes(platform),
  )
}

/**
 * Whether a runner can build `platform`: `unknown` when it reports no
 * checks, as runners from before this version do.
 */
export function runnerPlatformState(
  runner: Pick<Runner, 'toolchain'>,
  platform: BuildPlatform,
): 'ready' | 'missing' | 'unknown' {
  if (!runner.toolchain?.length) return 'unknown'
  return checksFor(runner, platform).some((check) => check.status === 'missing')
    ? 'missing'
    : 'ready'
}

export type PlatformReadiness =
  | { state: 'ready'; runner: Runner; warnings: Array<RunnerToolchainCheck> }
  | { state: 'missing'; runner: Runner; problems: Array<RunnerToolchainCheck> }
  | { state: 'offline' }
  | { state: 'unknown' }

/**
 * Whether any connected runner can build `platform` now, and when none can,
 * what the first one is missing.
 */
export function platformReadiness(
  runners: ReadonlyArray<Runner>,
  platform: BuildPlatform,
): PlatformReadiness {
  const online = runners.filter((runner) => runner.status !== 'offline')
  if (online.length === 0) return { state: 'offline' }
  const ready = online.find(
    (runner) => runnerPlatformState(runner, platform) === 'ready',
  )
  if (ready) {
    return {
      state: 'ready',
      runner: ready,
      warnings: checksFor(ready, platform).filter(
        (check) => check.status === 'warning',
      ),
    }
  }
  const reporting = online.find((runner) => runner.toolchain?.length)
  if (!reporting) return { state: 'unknown' }
  return {
    state: 'missing',
    runner: reporting,
    problems: checksFor(reporting, platform).filter(
      (check) => check.status === 'missing',
    ),
  }
}
