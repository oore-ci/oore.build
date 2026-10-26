-- Runner labels for `runs_on` scheduling and heartbeat utilisation.
-- `custom_labels` holds operator labels; `labels` holds those plus the labels
-- detected from capabilities, recomputed whenever either changes.
ALTER TABLE runners ADD COLUMN custom_labels TEXT NOT NULL DEFAULT '[]';
ALTER TABLE runners ADD COLUMN labels TEXT NOT NULL DEFAULT '[]';
ALTER TABLE runners ADD COLUMN metrics TEXT;
