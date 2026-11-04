import type {
  PipelineWorkflowSettings,
  SigningIdentity,
} from '@oore/client/models'
import { PIPELINE_IDS, PROJECT_IDS, ago } from '../seed'

export const SIGNING_IDENTITY_IDS = {
  coinUpload: 'sid-demo-coin-upload',
  coinAdhoc: 'sid-demo-coin-adhoc',
  coinAppStore: 'sid-demo-coin-app-store',
} as const

const DAY = 86_400
const TEAM_ID = 'A1B2C3D4E5'

/** NativePayments' library; `used_by` is filled in when listed. */
export const demoSigningIdentities: Array<SigningIdentity> = [
  {
    id: SIGNING_IDENTITY_IDS.coinUpload,
    project_id: PROJECT_IDS.nativePayments,
    name: 'coin-upload',
    kind: 'android_keystore',
    filename: 'coin-upload.jks',
    key_alias: 'upload',
    created_at: ago(60 * DAY),
    updated_at: ago(60 * DAY),
  },
  {
    id: SIGNING_IDENTITY_IDS.coinAdhoc,
    project_id: PROJECT_IDS.nativePayments,
    name: 'coin-adhoc',
    kind: 'ios_distribution',
    filename: 'coin-distribution.p12',
    team_id: TEAM_ID,
    export_method: 'ad_hoc',
    expires_at: ago(-300 * DAY),
    profiles: [
      {
        bundle_id: 'com.acme.coin',
        profile_name: 'Coin Ad Hoc',
        profile_uuid: '8f1c2a7e-4b1d-4d7e-9c55-1a2b3c4d5e01',
        expires_at: ago(-21 * DAY),
      },
      {
        bundle_id: 'com.acme.coin.widgets',
        profile_name: 'Coin Widgets Ad Hoc',
        profile_uuid: '8f1c2a7e-4b1d-4d7e-9c55-1a2b3c4d5e02',
        expires_at: ago(-21 * DAY),
      },
    ],
    created_at: ago(45 * DAY),
    updated_at: ago(12 * DAY),
  },
  {
    id: SIGNING_IDENTITY_IDS.coinAppStore,
    project_id: PROJECT_IDS.nativePayments,
    name: 'coin-app-store',
    kind: 'ios_distribution',
    filename: 'coin-distribution.p12',
    team_id: TEAM_ID,
    export_method: 'app_store',
    expires_at: ago(-300 * DAY),
    profiles: [
      {
        bundle_id: 'com.acme.coin',
        profile_name: 'Coin App Store',
        profile_uuid: '8f1c2a7e-4b1d-4d7e-9c55-1a2b3c4d5e03',
        expires_at: ago(-300 * DAY),
      },
      {
        bundle_id: 'com.acme.coin.widgets',
        profile_name: 'Coin Widgets App Store',
        profile_uuid: '8f1c2a7e-4b1d-4d7e-9c55-1a2b3c4d5e04',
        expires_at: ago(-300 * DAY),
      },
    ],
    created_at: ago(45 * DAY),
    updated_at: ago(45 * DAY),
  },
]

/** The Multi-platform Release pipeline's per-workflow settings. */
export const demoWorkflowSettings = {
  [PIPELINE_IDS.paymentsAll]: [
    {
      workflow: 'adhoc',
      ios_identity_id: SIGNING_IDENTITY_IDS.coinAdhoc,
      publish_enabled: false,
      updated_at: ago(3 * DAY),
    },
    {
      workflow: 'release',
      approval_required: true,
      updated_at: ago(3 * DAY),
    },
  ],
} satisfies Partial<Record<string, Array<PipelineWorkflowSettings>>>
