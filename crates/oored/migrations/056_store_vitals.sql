-- Latest store health read for each app a project has released: Google Play
-- crash and ANR rates, App Store hang rate and launch time. The daemon
-- refreshes rows in the background and Home reads them as they are.
CREATE TABLE store_app_vitals (
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    store TEXT NOT NULL CHECK (store IN ('google_play', 'app_store')),
    app_identifier TEXT NOT NULL,
    -- Pipeline whose publishing credentials read the store.
    pipeline_id TEXT NOT NULL REFERENCES pipelines(id) ON DELETE CASCADE,
    -- App Store Connect app id, looked up once from the bundle id.
    asc_app_id TEXT,
    vitals_json TEXT,
    issues_json TEXT,
    error TEXT,
    refreshed_at INTEGER,
    next_refresh_at INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (project_id, store, app_identifier)
);

CREATE INDEX idx_store_app_vitals_due ON store_app_vitals (next_refresh_at);
