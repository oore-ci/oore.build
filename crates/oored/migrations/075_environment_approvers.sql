-- Who may approve promotions into an environment. With no approver role and
-- no approver rows, any project maintainer approves, as before.

-- Members with at least this project role may approve.
ALTER TABLE project_environments ADD COLUMN approver_role TEXT
    CHECK (approver_role IS NULL OR approver_role IN ('maintainer', 'developer'));

-- Project members named as approvers. Removing a user from the project
-- removes their rows, and only current members count.
CREATE TABLE environment_approvers (
    environment_id TEXT NOT NULL REFERENCES project_environments(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (environment_id, user_id)
);

CREATE INDEX idx_environment_approvers_user ON environment_approvers (user_id);
