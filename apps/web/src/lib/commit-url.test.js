import { expect, test } from 'bun:test'
import { commitUrl } from '@/lib/commit-url'

test('commitUrl links GitHub and GitLab commits on their own hosts', () => {
  expect(
    commitUrl(
      { repository_provider: 'github', repository_full_name: 'acme/app' },
      'abc123',
    ),
  ).toBe('https://github.com/acme/app/commit/abc123')
  expect(
    commitUrl(
      {
        repository_provider: 'gitlab',
        repository_full_name: 'group/sub/app',
        repository_host_url: 'https://git.example.com/',
      },
      'abc123',
    ),
  ).toBe('https://git.example.com/group/sub/app/-/commit/abc123')
})

test('commitUrl has no link for local repositories or missing context', () => {
  expect(
    commitUrl(
      { repository_provider: 'local_git', repository_full_name: 'app' },
      'abc123',
    ),
  ).toBeNull()
  expect(commitUrl(undefined, 'abc123')).toBeNull()
})
