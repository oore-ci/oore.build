-- Apps a person pinned; pinned apps sort first and their releases reach
-- the person's notifications.
CREATE TABLE user_favorite_projects (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, project_id)
);

CREATE INDEX idx_user_favorite_projects_project ON user_favorite_projects (project_id);

-- One person's notification choices, as NotificationPreferences JSON.
CREATE TABLE user_notification_preferences (
    user_id TEXT PRIMARY KEY NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    preferences_json TEXT NOT NULL,
    last_digest_at INTEGER,
    updated_at INTEGER NOT NULL
);

-- Personal email waiting to be sent. Rows go once sent or after the last
-- attempt fails.
CREATE TABLE personal_emails (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    subject TEXT NOT NULL,
    html_body TEXT NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_personal_emails_due ON personal_emails (next_attempt_at);
