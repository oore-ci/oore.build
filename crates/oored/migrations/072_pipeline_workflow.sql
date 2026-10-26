-- The `.oore.yaml` version 2 workflow a pipeline builds. NULL plans the
-- file's only workflow, or its top level when it defines none.
ALTER TABLE pipelines ADD COLUMN workflow TEXT;
