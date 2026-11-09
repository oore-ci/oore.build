import type { Build } from '@oore/client/models'

/** Branch, or the tag or ref that triggered a build with no branch. */
export function buildSourceRef(
  build: Pick<Build, 'branch' | 'trigger_ref'>,
): string | null {
  return (
    build.branch ??
    build.trigger_ref?.replace(/^refs\/(?:heads|tags)\//, '') ??
    null
  )
}
