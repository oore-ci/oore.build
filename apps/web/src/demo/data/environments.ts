import type {
  EnvironmentPromotion,
  ProjectEnvironment,
} from '@oore/client/models'
import { BUILD_IDS, PROJECT_IDS, USER_IDS, ago } from '../seed'
import { ENVIRONMENT_IDS } from './environment-ids'
import { RELEASE_IDS } from './releases'

export { ENVIRONMENT_IDS }

/** Stored environments; the handlers add latest_release. */
export type DemoEnvironment = Omit<ProjectEnvironment, 'latest_release'>

/** Stored promotions; the handlers add names, emails and can_decide. */
export interface DemoPromotion extends Omit<
  EnvironmentPromotion,
  | 'can_decide'
  | 'environment_name'
  | 'source_environment_name'
  | 'requested_by_email'
  | 'decided_by_email'
> {
  requested_by?: string
  decided_by?: string
}

export const demoEnvironments: Array<DemoEnvironment> = [
  {
    id: ENVIRONMENT_IDS.shopInternal,
    project_id: PROJECT_IDS.flutterShop,
    key: 'internal',
    name: 'Internal',
    position: 0,
    store: 'google_play',
    track: 'internal',
    approval_required: false,
    rollout_policy: { steps: [1], interval_hours: 24, approval: 'none' },
    promotable: true,
    created_at: ago(86400 * 30),
    updated_at: ago(86400 * 30),
  },
  {
    id: ENVIRONMENT_IDS.shopBeta,
    project_id: PROJECT_IDS.flutterShop,
    key: 'beta',
    name: 'Beta',
    position: 1,
    store: 'google_play',
    track: 'open',
    approval_required: true,
    rollout_policy: {
      steps: [0.5, 1],
      interval_hours: 12,
      crash_rate_threshold: 0.02,
      approval: 'none',
    },
    promotable: true,
    created_at: ago(86400 * 30),
    updated_at: ago(86400 * 30),
  },
  {
    id: ENVIRONMENT_IDS.shopProduction,
    project_id: PROJECT_IDS.flutterShop,
    key: 'production',
    name: 'Production',
    position: 2,
    store: 'google_play',
    track: 'production',
    approval_required: true,
    rollout_policy: {
      steps: [0.01, 0.05, 0.2, 0.5, 1],
      interval_hours: 24,
      crash_rate_threshold: 0.02,
      approval: 'none',
    },
    promotable: true,
    created_at: ago(86400 * 30),
    updated_at: ago(86400 * 30),
  },
  {
    id: ENVIRONMENT_IDS.shopTestflight,
    project_id: PROJECT_IDS.flutterShop,
    key: 'testflight',
    name: 'TestFlight',
    position: 3,
    store: 'app_store',
    track: 'Internal',
    approval_required: false,
    rollout_policy: { steps: [1], interval_hours: 24, approval: 'none' },
    promotable: true,
    created_at: ago(86400 * 20),
    updated_at: ago(86400 * 20),
  },
  {
    id: ENVIRONMENT_IDS.shopAppStore,
    project_id: PROJECT_IDS.flutterShop,
    key: 'app-store',
    name: 'App Store',
    position: 4,
    store: 'app_store',
    track: 'app_store',
    approval_required: true,
    rollout_policy: {
      steps: [0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1],
      interval_hours: 24,
      approval: 'none',
    },
    submit_for_review: true,
    promotable: true,
    created_at: ago(86400 * 20),
    updated_at: ago(86400 * 20),
  },
]

export const demoPromotions: Array<DemoPromotion> = [
  {
    id: 'promo-demo-003',
    project_id: PROJECT_IDS.flutterShop,
    build_id: BUILD_IDS.succeeded7,
    build_number: 145,
    version_name: '2.5.0',
    version_code: 145,
    environment_id: ENVIRONMENT_IDS.shopBeta,
    source_release_id: RELEASE_IDS.shopInternalNext,
    source_track: 'internal',
    state: 'pending',
    reason: 'Order tracking passed internal QA',
    requested_by: USER_IDS.developer,
    created_at: ago(1800),
    updated_at: ago(1800),
  },
  {
    id: 'promo-demo-002',
    project_id: PROJECT_IDS.flutterShop,
    build_id: BUILD_IDS.succeeded1,
    build_number: 141,
    version_name: '2.4.0',
    version_code: 141,
    environment_id: ENVIRONMENT_IDS.shopProduction,
    source_release_id: RELEASE_IDS.shopBeta,
    source_track: 'open',
    release_id: RELEASE_IDS.shopProduction,
    state: 'completed',
    reason: 'Beta crash rate stayed under 0.3%',
    requested_by: USER_IDS.developer,
    decided_by: USER_IDS.owner,
    decided_at: ago(86400 * 2 + 3600),
    created_at: ago(86400 * 2 + 7200),
    updated_at: ago(86400 * 2 + 3600),
  },
  {
    id: 'promo-demo-001',
    project_id: PROJECT_IDS.flutterShop,
    build_id: BUILD_IDS.succeeded1,
    build_number: 141,
    version_name: '2.4.0',
    version_code: 141,
    environment_id: ENVIRONMENT_IDS.shopBeta,
    source_release_id: RELEASE_IDS.shopInternal,
    source_track: 'internal',
    release_id: RELEASE_IDS.shopBeta,
    state: 'completed',
    requested_by: USER_IDS.owner,
    decided_by: USER_IDS.owner,
    decided_at: ago(86400 * 4),
    created_at: ago(86400 * 4 + 600),
    updated_at: ago(86400 * 4),
  },
]
