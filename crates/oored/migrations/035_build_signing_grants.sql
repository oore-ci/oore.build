ALTER TABLE builds ADD COLUMN signing_token_hash TEXT;

-- Retire stale assignments left by pre-v3 lifecycle behavior. Active jobs are
-- left fail-closed with no signing grant and must be requeued to receive one.
UPDATE builds
SET runner_id = NULL
WHERE status IN (
  'queued',
  'scheduled',
  'succeeded',
  'failed',
  'canceled',
  'timed_out',
  'expired'
);
