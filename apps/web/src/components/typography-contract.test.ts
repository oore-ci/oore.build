import { readdirSync, readFileSync } from 'node:fs'
import { extname, join, relative, resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

const SOURCE_ROOT = resolve(process.cwd(), 'src')
const FORBIDDEN_DECORATIVE_UTILITIES = [
  `upper${'case'}`,
  ['tracking', 'wider'].join('-'),
] as const

function userFacingTsxFiles(directory: string): Array<string> {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name)
    if (entry.isDirectory()) return userFacingTsxFiles(path)
    if (extname(entry.name) !== '.tsx' || entry.name.endsWith('.test.tsx')) {
      return []
    }
    return [path]
  })
}

describe('user-facing typography contract', () => {
  it('keeps decorative display casing out of TSX class names', () => {
    const violations = userFacingTsxFiles(SOURCE_ROOT).flatMap((path) => {
      const source = readFileSync(path, 'utf8')
      return source
        .split('\n')
        .flatMap((line, index) =>
          FORBIDDEN_DECORATIVE_UTILITIES.filter((utility) =>
            line.includes(utility),
          ).map(
            (utility) =>
              `${relative(SOURCE_ROOT, path)}:${index + 1} uses ${utility}`,
          ),
        )
    })

    expect(violations).toEqual([])
  })
})
