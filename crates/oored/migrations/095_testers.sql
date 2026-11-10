-- A link that always installs the newest succeeded build of a pipeline.
CREATE TABLE latest_install_links (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    pipeline_id TEXT NOT NULL REFERENCES pipelines(id) ON DELETE CASCADE,
    branch TEXT,
    platform TEXT NOT NULL CHECK (platform IN ('android', 'ios')),
    token_hash TEXT NOT NULL UNIQUE,
    token_prefix TEXT NOT NULL,
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    last_used_at INTEGER,
    use_count INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_latest_install_links_pipeline ON latest_install_links (pipeline_id);

-- The one pull request comment Oore keeps up to date per pipeline.
CREATE TABLE scm_pr_comments (
    pipeline_id TEXT NOT NULL REFERENCES pipelines(id) ON DELETE CASCADE,
    repository_full_name TEXT NOT NULL,
    pr_number INTEGER NOT NULL,
    external_id TEXT NOT NULL,
    build_id TEXT,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (pipeline_id, repository_full_name, pr_number)
);

-- Named groups of people a build can be sent to.
CREATE TABLE tester_groups (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE tester_group_members (
    group_id TEXT NOT NULL REFERENCES tester_groups(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    PRIMARY KEY (group_id, user_id)
);

CREATE TABLE build_shares (
    id TEXT PRIMARY KEY NOT NULL,
    build_id TEXT NOT NULL REFERENCES builds(id) ON DELETE CASCADE,
    group_id TEXT NOT NULL REFERENCES tester_groups(id) ON DELETE CASCADE,
    message TEXT,
    sent_by TEXT,
    sent_at INTEGER NOT NULL
);

CREATE INDEX idx_build_shares_build ON build_shares (build_id);

-- Who sent a build to whom, frozen when it was sent.
CREATE TABLE build_share_recipients (
    share_id TEXT NOT NULL REFERENCES build_shares(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    no_access INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (share_id, user_id)
);

-- Each install of an artifact: through the install page (signed in), a
-- share link, or a latest link.
CREATE TABLE artifact_installs (
    id TEXT PRIMARY KEY NOT NULL,
    artifact_id TEXT NOT NULL REFERENCES artifacts(id) ON DELETE CASCADE,
    build_id TEXT NOT NULL REFERENCES builds(id) ON DELETE CASCADE,
    user_id TEXT,
    via TEXT NOT NULL,
    -- One install per download token, however often the file is fetched.
    token_hash TEXT UNIQUE,
    installed_at INTEGER NOT NULL
);

-- What a download token is for: `share_link`, `install_page` or `latest_link`.
ALTER TABLE artifact_download_tokens ADD COLUMN purpose TEXT NOT NULL DEFAULT 'share_link';

CREATE INDEX idx_artifact_installs_build ON artifact_installs (build_id, installed_at);

-- An iPhone registering itself for a pipeline's ad hoc builds.
CREATE TABLE ios_device_registrations (
    id TEXT PRIMARY KEY NOT NULL,
    pipeline_id TEXT NOT NULL REFERENCES pipelines(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    device_name TEXT,
    state TEXT NOT NULL DEFAULT 'pending',
    udid TEXT,
    error TEXT,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    completed_at INTEGER
);

-- Builds someone chose to keep; retention never removes them.
ALTER TABLE builds ADD COLUMN kept_at INTEGER;
ALTER TABLE builds ADD COLUMN kept_by TEXT;

-- Browser push subscriptions for the installed tester portal.
CREATE TABLE web_push_subscriptions (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    endpoint TEXT NOT NULL UNIQUE,
    p256dh TEXT NOT NULL,
    auth TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    last_success_at INTEGER
);

CREATE INDEX idx_web_push_subscriptions_user ON web_push_subscriptions (user_id);

-- The instance's VAPID key for web push, encrypted with the instance key.
CREATE TABLE web_push_keys (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    private_key_enc TEXT NOT NULL,
    public_key TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

-- When subscribed testers were told about a succeeded build.
ALTER TABLE builds ADD COLUMN tester_pushed_at INTEGER;
