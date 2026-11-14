import { describe, expect, test } from 'bun:test'
import path from 'node:path'

const script = path.join(import.meta.dir, 'previous-release-tag.sh')

function previous(tag: string, tags: Array<string>) {
  const result = Bun.spawnSync(['bash', script, tag], {
    stdin: new TextEncoder().encode(tags.join('\n') + '\n'),
  })
  expect(result.exitCode).toBe(0)
  return result.stdout.toString().trim()
}

describe('previous-release-tag', () => {
  test('ignores tags cut ahead of the release being published', () => {
    const tags = [
      'v0.1.50-alpha.5',
      'v0.1.50-alpha.6',
      'v0.1.50-alpha.7',
      'v0.1.50-alpha.8',
      'v0.1.50-alpha.9',
    ]
    expect(previous('v0.1.50-alpha.6', tags)).toBe('v0.1.50-alpha.5')
    expect(previous('v0.1.50-alpha.8', tags)).toBe('v0.1.50-alpha.7')
    expect(previous('v0.1.50-alpha.9', tags)).toBe('v0.1.50-alpha.8')
  })

  test('orders pre-release numbers numerically', () => {
    const tags = ['v0.1.50-alpha.9', 'v0.1.50-alpha.10', 'v0.1.50-alpha.11']
    expect(previous('v0.1.50-alpha.10', tags)).toBe('v0.1.50-alpha.9')
  })

  test('crosses into the previous version', () => {
    const tags = ['v0.1.49-alpha.3', 'v0.1.50-alpha.1', 'v0.1.50-alpha.2']
    expect(previous('v0.1.50-alpha.1', tags)).toBe('v0.1.49-alpha.3')
  })

  test('works for stable tags and when the tag itself is not listed', () => {
    expect(previous('v0.1.49', ['v0.1.47', 'v0.1.48', 'v0.1.50'])).toBe(
      'v0.1.48',
    )
  })

  test('prints nothing for the first release', () => {
    expect(previous('v0.1.0', ['v0.1.0', 'v0.2.0'])).toBe('')
    expect(previous('v0.1.0', [])).toBe('')
  })
})
