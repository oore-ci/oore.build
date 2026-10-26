import { expect, test } from 'bun:test'
import { isDemoMutationAllowed } from '@/lib/demo-mode'

test('the hosted demo allows dry runs and blocks writes', () => {
  const host = 'demo.oore.build'
  expect(isDemoMutationAllowed('POST', '/v1/pipelines/plan', host)).toBe(true)
  expect(isDemoMutationAllowed('POST', '/v1/pipelines/export-yaml', host)).toBe(
    true,
  )
  expect(isDemoMutationAllowed('POST', '/v1/pipelines/import-yaml', host)).toBe(
    true,
  )
  expect(isDemoMutationAllowed('POST', '/v1/projects/p/pipelines', host)).toBe(
    false,
  )
  expect(isDemoMutationAllowed('POST', '/v1/pipelines/plan', 'localhost')).toBe(
    true,
  )
})
