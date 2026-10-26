-- Artifact size limits a pipeline's commit checks enforce, as JSON
-- (`{"mode":"warn"|"fail","rules":[...]}`). An empty object has no rules.
ALTER TABLE pipelines ADD COLUMN size_budget TEXT NOT NULL DEFAULT '{}';
