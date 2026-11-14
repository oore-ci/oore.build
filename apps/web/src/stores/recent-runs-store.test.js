import { describe, expect, test } from 'bun:test'
import { rememberRun } from './recent-runs-store'

const run = (pipelineId, branch) => ({
  projectId: 'p',
  projectName: 'Shop',
  pipelineId,
  pipelineName: pipelineId,
  branch,
})

describe('rememberRun', () => {
  test('puts the newest run first without duplicates', () => {
    const runs = rememberRun(
      [run('a', 'main'), run('b', 'main')],
      run('b', 'main'),
    )
    expect(runs.map((r) => r.pipelineId)).toEqual(['b', 'a'])
  })

  test('keeps five runs', () => {
    let runs = []
    for (const branch of ['1', '2', '3', '4', '5', '6']) {
      runs = rememberRun(runs, run('a', branch))
    }
    expect(runs.map((r) => r.branch)).toEqual(['6', '5', '4', '3', '2'])
  })
})
