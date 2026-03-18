import type {
  RetentionCleanupSummary,
  RetentionPolicy,
} from '@/lib/types'
import { ago } from '../seed'

export const demoRetentionPolicy: RetentionPolicy = {
  enabled: true,
  max_age_days: 30,
  max_builds_per_project: 100,
  max_artifact_size_bytes: 5368709120,
  cleanup_target: 'artifacts_only',
  keep_statuses: ['succeeded'],
  dry_run: false,
  cleanup_interval_secs: 86400,
  updated_at: ago(86400 * 3),
}

export const demoLastCleanup: RetentionCleanupSummary = {
  builds_expired: 3,
  artifacts_deleted: 7,
  bytes_reclaimed: 1073741824,
  dry_run: false,
  ran_at: ago(3600),
}
