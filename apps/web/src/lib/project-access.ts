import type { ProjectRole } from '@oore/client/models'

export const PROJECT_ROLE_LABELS = {
  maintainer: 'Maintainer',
  developer: 'Developer',
  viewer: 'Viewer',
} satisfies Record<ProjectRole, string>

export const PROJECT_ROLE_OPTIONS: Array<ProjectRole> = [
  'maintainer',
  'developer',
  'viewer',
]

const EMAIL_PATTERN =
  /[^\s<>,;:"'()[\]]+@[^\s<>,;:"'()[\]]+\.[^\s<>,;:"'()[\]]+/g

/**
 * Pulls the email addresses out of pasted text: one per line, comma or
 * semicolon separated, or copied from a mail client as `Name <a@b.com>`.
 * Lowercased and deduplicated, in the order they appear.
 */
export function parseEmailList(text: string): Array<string> {
  const emails = (text.match(EMAIL_PATTERN) ?? []).map((email) =>
    email.toLowerCase(),
  )
  return [...new Set(emails)]
}

/** Owners and admins already reach every project; they need no membership. */
export function hasImplicitProjectAccess(instanceRole: string | undefined) {
  return instanceRole === 'owner' || instanceRole === 'admin'
}

/** QA viewers are read-only everywhere, so they always get Viewer. */
export function projectRoleFor(
  instanceRole: string | undefined,
  role: ProjectRole,
): ProjectRole {
  return instanceRole === 'qa_viewer' ? 'viewer' : role
}

export function plural(count: number, noun: string) {
  return `${count} ${noun}${count === 1 ? '' : 's'}`
}

export function people(count: number) {
  return count === 1 ? '1 person' : `${count} people`
}

/** Runs `task` over `items` with at most `limit` in flight at once. */
export async function mapWithLimit<T, R>(
  items: Array<T>,
  limit: number,
  task: (item: T) => Promise<R>,
): Promise<Array<PromiseSettledResult<R>>> {
  const results: Array<PromiseSettledResult<R>> = new Array(items.length)
  let next = 0
  async function worker() {
    while (next < items.length) {
      const index = next++
      try {
        results[index] = {
          status: 'fulfilled',
          value: await task(items[index]),
        }
      } catch (reason) {
        results[index] = { status: 'rejected', reason }
      }
    }
  }
  await Promise.all(
    Array.from({ length: Math.min(limit, items.length) }, worker),
  )
  return results
}
