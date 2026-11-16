import { describe, expect, test } from 'bun:test'
import {
  approvalInForce,
  pipelineFile,
  publishingInForce,
  repositoryFileUrl,
  signingInForce,
  sourceSummary,
  triggersInForce,
} from '@/lib/pipeline-sources'

const listing = {
  pipeline_id: 'p1',
  state: 'valid',
  path: '.oore/release.yaml',
  reference: 'main',
  workflows: [
    {
      name: 'adhoc',
      platforms: ['android', 'ios'],
      publishes: [
        'android publishes to the Google Play internal track (workflows.adhoc.targets.android.publish.play)',
      ],
      triggers: ['pull requests into main'],
      uses_pipeline_triggers: false,
      signing: { android: 'coin-upload', ios: 'coin-adhoc' },
    },
    {
      name: 'release',
      platforms: ['android', 'ios'],
      publishes: [
        'android publishes to the Google Play internal track (workflows.release.targets.android.publish.play)',
      ],
      triggers: ['push to main'],
      uses_pipeline_triggers: false,
      signing: { android: 'coin-upload' },
    },
    {
      name: 'nightly',
      platforms: ['ios'],
      publishes: [],
      triggers: [],
      uses_pipeline_triggers: true,
    },
  ],
}
const file = pipelineFile(listing)
const names = new Map([['id-1', 'coin-app-store']])
const identityName = (id) => names.get(id) ?? 'Deleted identity'

describe('pipelineFile', () => {
  test('is null unless the file was read', () => {
    expect(pipelineFile(undefined)).toBeNull()
    expect(pipelineFile({ ...listing, state: 'missing' })).toBeNull()
    expect(file?.path).toBe('.oore/release.yaml')
  })
})

describe('signingInForce', () => {
  test('without a file, the pipeline decides every build', () => {
    expect(
      signingInForce({
        file: null,
        identityName,
        pipelineValue: 'upload-key',
        platform: 'android',
        settings: [],
      }),
    ).toEqual([
      { value: 'upload-key', source: { kind: 'here' }, workflows: [] },
    ])
  })

  test('the file names one identity for every workflow', () => {
    expect(
      signingInForce({
        file,
        identityName,
        pipelineValue: 'Not signed',
        platform: 'android',
        settings: [],
      }),
    ).toEqual([
      {
        value: 'coin-upload',
        source: { kind: 'file', path: '.oore/release.yaml' },
        workflows: [],
      },
    ])
  })

  test('workflow settings beat the file, which beats the pipeline', () => {
    const entries = signingInForce({
      file,
      identityName,
      pipelineValue: 'Pipeline identity',
      platform: 'ios',
      settings: [{ workflow: 'release', ios_identity_id: 'id-1' }],
    })
    expect(entries).toEqual([
      {
        value: 'coin-adhoc',
        source: { kind: 'file', path: '.oore/release.yaml' },
        workflows: ['adhoc'],
      },
      {
        value: 'coin-app-store',
        source: { kind: 'workflow', workflow: 'release' },
        workflows: ['release'],
      },
      {
        value: 'Pipeline identity',
        source: { kind: 'here' },
        workflows: ['nightly'],
      },
    ])
    expect(sourceSummary(entries)).toEqual([
      'from .oore/release.yaml',
      'set here',
      'overridden for workflow release',
    ])
  })
})

describe('publishingInForce', () => {
  test('uploads turned off for a workflow override the file', () => {
    const entries = publishingInForce({
      file,
      pipelineValue: 'Off',
      platform: 'android',
      settings: [{ workflow: 'adhoc', publish_enabled: false }],
    })
    expect(entries).toEqual([
      {
        value: 'Off',
        source: { kind: 'workflow', workflow: 'adhoc' },
        workflows: ['adhoc'],
      },
      {
        value: 'Uploads to the Google Play internal track',
        source: { kind: 'file', path: '.oore/release.yaml' },
        workflows: ['release'],
      },
    ])
  })
})

describe('triggersInForce', () => {
  test('a default workflow is the only one webhooks start', () => {
    expect(
      triggersInForce({
        file: { ...file, defaultWorkflow: 'release' },
        pipelineValue: 'Pushes to main',
      }),
    ).toEqual([
      {
        value: 'push to main',
        source: { kind: 'file', path: '.oore/release.yaml' },
        workflows: ['release'],
      },
    ])
  })

  test('a workflow without `on` follows the pipeline', () => {
    const entries = triggersInForce({ file, pipelineValue: 'Pushes to main' })
    expect(entries.map((entry) => entry.source.kind)).toEqual([
      'file',
      'file',
      'here',
    ])
  })
})

describe('approvalInForce', () => {
  test('names the workflows that set their own approval', () => {
    const entries = approvalInForce({
      file,
      pipelineValue: 'Not required',
      settings: [{ workflow: 'release', approval_required: true }],
    })
    expect(entries).toEqual([
      {
        value: 'Not required',
        source: { kind: 'here' },
        workflows: ['adhoc', 'nightly'],
      },
      {
        value: 'Required',
        source: { kind: 'workflow', workflow: 'release' },
        workflows: ['release'],
      },
    ])
  })
})

describe('repositoryFileUrl', () => {
  test('links to the file on the branch', () => {
    expect(
      repositoryFileUrl({
        fullName: 'acme/app',
        path: '.oore/release.yaml',
        provider: 'github',
        reference: 'release/3.2',
      }),
    ).toBe('https://github.com/acme/app/blob/release/3.2/.oore/release.yaml')
    expect(
      repositoryFileUrl({
        fullName: 'acme/app',
        host: 'https://git.acme.dev/',
        path: '.oore.yaml',
        provider: 'gitlab',
        reference: 'main',
      }),
    ).toBe('https://git.acme.dev/acme/app/-/blob/main/.oore.yaml')
  })

  test('is null for local repositories', () => {
    expect(
      repositoryFileUrl({
        fullName: 'app',
        path: '.oore.yaml',
        provider: 'local_git',
        reference: 'main',
      }),
    ).toBeNull()
  })
})
