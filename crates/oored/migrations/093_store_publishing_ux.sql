-- Release notes written for one build's store uploads. They replace the
-- pipeline's Google Play and App Store notes for that build.
ALTER TABLE builds ADD COLUMN store_notes TEXT;

-- TestFlight groups a workflow's builds go to, as a JSON array. NULL follows
-- the file, then the pipeline.
ALTER TABLE pipeline_workflow_settings ADD COLUMN testflight_groups TEXT;

-- Slack channel the bot token posts release updates to, so each release has
-- one message Oore edits instead of a new post per change.
ALTER TABLE notification_channels ADD COLUMN slack_channel_id TEXT;

-- The Slack message that stands for a release in a channel.
CREATE TABLE IF NOT EXISTS slack_release_messages (
    channel_id TEXT NOT NULL REFERENCES notification_channels(id) ON DELETE CASCADE,
    release_id TEXT NOT NULL REFERENCES releases(id) ON DELETE CASCADE,
    slack_channel_id TEXT NOT NULL,
    ts TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (channel_id, release_id)
) WITHOUT ROWID;
