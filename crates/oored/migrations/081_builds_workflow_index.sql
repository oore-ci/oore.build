-- Filter a pipeline's builds by the v2 workflow they planned.
CREATE INDEX IF NOT EXISTS idx_builds_pipeline_workflow
    ON builds (pipeline_id, json_extract(config_snapshot, '$.workflow'));

-- A webhook delivery may start one build per workflow of a pipeline, so a
-- claim is per (delivery, pipeline, workflow). '' stands for no workflow.
CREATE TABLE webhook_build_claims_v2 (
    webhook_id TEXT NOT NULL REFERENCES integration_webhooks(id) ON DELETE CASCADE,
    pipeline_id TEXT NOT NULL REFERENCES pipelines(id) ON DELETE CASCADE,
    workflow TEXT NOT NULL DEFAULT '',
    build_id TEXT NOT NULL,
    PRIMARY KEY (webhook_id, pipeline_id, workflow)
) WITHOUT ROWID;

INSERT INTO webhook_build_claims_v2 (webhook_id, pipeline_id, workflow, build_id)
SELECT claims.webhook_id, claims.pipeline_id,
       COALESCE((SELECT json_extract(b.config_snapshot, '$.workflow')
                 FROM builds b WHERE b.id = claims.build_id), ''),
       claims.build_id
FROM webhook_build_claims claims;

DROP TABLE webhook_build_claims;
ALTER TABLE webhook_build_claims_v2 RENAME TO webhook_build_claims;
