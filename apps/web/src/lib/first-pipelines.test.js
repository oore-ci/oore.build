import { expect, test } from 'bun:test'
import { proposedPipelines } from '@/lib/first-pipelines'

const flutter = {
  reference: 'main',
  flutter: true,
  app_name: 'coin',
  platforms: ['android', 'ios'],
  config_files: [],
}

test('a Flutter app gets one starter per platform, Android first', () => {
  const proposals = proposedPipelines(flutter, { manualOnlyTriggers: false })
  expect(
    proposals.map((p) => [p.key, p.selected, p.runsNow, p.request.name]),
  ).toEqual([
    ['android', true, true, 'Android test app'],
    ['ios', false, false, 'iOS test app'],
  ])
  const android = proposals[0].request
  expect(android.execution_config.platforms).toEqual(['android'])
  expect(android.trigger_config.events).toEqual([])
})

test('an iOS-only app has its one pipeline ticked', () => {
  const [ios] = proposedPipelines(
    { ...flutter, platforms: ['ios'] },
    { manualOnlyTriggers: true },
  )
  expect([ios.key, ios.selected]).toEqual(['ios', true])
})

test('a checked-in Oore file wins over the starters', () => {
  const proposals = proposedPipelines(
    { ...flutter, config_files: ['.oore.yaml', '.oore.release.yaml'] },
    { manualOnlyTriggers: false },
  )
  expect(proposals.map((p) => [p.request.config_path, p.selected])).toEqual([
    ['.oore.yaml', true],
    ['.oore.release.yaml', false],
  ])
})

test('a repository without Flutter gets no proposal', () => {
  expect(
    proposedPipelines(
      { ...flutter, flutter: false },
      { manualOnlyTriggers: false },
    ),
  ).toEqual([])
})
