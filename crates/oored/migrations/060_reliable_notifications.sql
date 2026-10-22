-- Build notifications come from a scan, like commit checks: a finished build
-- is due while `notified_status` differs from its status, so every terminal
-- write path notifies. Existing builds start notified so history stays quiet.
ALTER TABLE builds ADD COLUMN notified_status TEXT;
UPDATE builds SET notified_status = status;

CREATE INDEX IF NOT EXISTS idx_builds_notification_due
ON builds(updated_at) WHERE notified_status IS NOT status;

-- A delivery keeps the notification it sends so a failed send can be
-- retried. Pending deliveries are due at `next_attempt_at`.
ALTER TABLE notification_deliveries ADD COLUMN notification_json TEXT;
ALTER TABLE notification_deliveries ADD COLUMN next_attempt_at INTEGER;

CREATE INDEX IF NOT EXISTS idx_notification_deliveries_due
ON notification_deliveries(next_attempt_at) WHERE status = 'pending';
