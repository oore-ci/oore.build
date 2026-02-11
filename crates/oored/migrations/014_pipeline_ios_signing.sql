-- Pipeline-scoped iOS signing settings and assets.
-- Sensitive values are encrypted at rest using the instance encryption key.

CREATE TABLE IF NOT EXISTS pipeline_ios_signing_settings (
    id TEXT PRIMARY KEY,
    pipeline_id TEXT NOT NULL UNIQUE,
    enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
    mode TEXT NOT NULL DEFAULT 'manual' CHECK (mode IN ('manual', 'api', 'hybrid')),
    team_id TEXT,
    export_method TEXT NOT NULL DEFAULT 'ad_hoc',
    bundle_ids_json TEXT NOT NULL DEFAULT '[]',
    p12_filename TEXT,
    p12_encrypted TEXT,
    p12_password_encrypted TEXT,
    p12_fingerprint TEXT,
    p12_expires_at INTEGER,
    api_key_id TEXT,
    api_issuer_id TEXT,
    api_private_key_encrypted TEXT,
    created_by TEXT,
    updated_by TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (pipeline_id) REFERENCES pipelines(id) ON DELETE CASCADE,
    FOREIGN KEY (created_by) REFERENCES users(id) ON DELETE SET NULL,
    FOREIGN KEY (updated_by) REFERENCES users(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_pipeline_ios_signing_settings_pipeline
ON pipeline_ios_signing_settings(pipeline_id);

CREATE TABLE IF NOT EXISTS pipeline_ios_provisioning_profiles (
    id TEXT PRIMARY KEY,
    pipeline_id TEXT NOT NULL,
    bundle_id TEXT NOT NULL,
    profile_filename TEXT,
    profile_encrypted TEXT,
    profile_uuid TEXT,
    profile_name TEXT,
    team_id TEXT,
    expires_at INTEGER,
    checksum TEXT,
    created_by TEXT,
    updated_by TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (pipeline_id) REFERENCES pipelines(id) ON DELETE CASCADE,
    FOREIGN KEY (created_by) REFERENCES users(id) ON DELETE SET NULL,
    FOREIGN KEY (updated_by) REFERENCES users(id) ON DELETE SET NULL,
    UNIQUE (pipeline_id, bundle_id)
);

CREATE INDEX IF NOT EXISTS idx_pipeline_ios_profiles_pipeline
ON pipeline_ios_provisioning_profiles(pipeline_id);

CREATE TABLE IF NOT EXISTS pipeline_ios_signing_devices (
    id TEXT PRIMARY KEY,
    pipeline_id TEXT NOT NULL,
    device_id TEXT,
    udid TEXT NOT NULL,
    name TEXT NOT NULL,
    platform TEXT NOT NULL DEFAULT 'ios',
    status TEXT NOT NULL DEFAULT 'enabled',
    added_at INTEGER NOT NULL,
    last_synced_at INTEGER,
    created_by TEXT,
    updated_by TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (pipeline_id) REFERENCES pipelines(id) ON DELETE CASCADE,
    FOREIGN KEY (created_by) REFERENCES users(id) ON DELETE SET NULL,
    FOREIGN KEY (updated_by) REFERENCES users(id) ON DELETE SET NULL,
    UNIQUE (pipeline_id, udid)
);

CREATE INDEX IF NOT EXISTS idx_pipeline_ios_devices_pipeline
ON pipeline_ios_signing_devices(pipeline_id);
