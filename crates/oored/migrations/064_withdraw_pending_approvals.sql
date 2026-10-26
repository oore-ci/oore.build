-- A build that ends while waiting for approval (canceled, superseded, failed
-- by a credential preflight, or removed with its source) is no longer
-- waiting for anyone. Clear its approval state on every path that ends it.
-- approval_requested_at stays, so a rerun of a gated build stays gated.
CREATE TRIGGER builds_withdraw_pending_approval
AFTER UPDATE OF status ON builds
WHEN NEW.approval_state = 'pending' AND NEW.status <> 'queued'
BEGIN
    UPDATE builds SET approval_state = NULL WHERE id = NEW.id;
END;

UPDATE builds SET approval_state = NULL
WHERE approval_state = 'pending' AND status <> 'queued';
