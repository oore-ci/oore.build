-- Slack member IDs to mention, per channel, as a JSON list of
-- {"identity", "member_id"} pairs. Lets incoming-webhook-only Slack channels
-- mention people, and maps Git usernames from push builds to Slack members.
ALTER TABLE notification_channels ADD COLUMN slack_mentions_json TEXT;
