ALTER TABLE instance_preferences
  ADD COLUMN runtime_mode TEXT NOT NULL DEFAULT 'local'
  CHECK (runtime_mode IN ('local', 'remote'));

UPDATE instance_preferences
SET runtime_mode = 'local'
WHERE runtime_mode IS NULL OR runtime_mode = '';
