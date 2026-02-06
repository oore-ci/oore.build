-- Setup state machine persistence table.
-- There is always exactly one row (id = 1).
CREATE TABLE IF NOT EXISTS setup_state (
    id                          INTEGER PRIMARY KEY CHECK (id = 1) DEFAULT 1,

    schema_version              INTEGER NOT NULL DEFAULT 1,
    instance_id                 TEXT    NOT NULL,
    setup_state                 TEXT    NOT NULL DEFAULT 'bootstrap_pending',

    -- Bootstrap token (nullable group)
    bootstrap_token_hash        TEXT,
    bootstrap_token_expires_at  INTEGER,
    bootstrap_token_consumed_at INTEGER,

    -- Setup session (nullable group)
    session_hash                TEXT,
    session_expires_at          INTEGER,

    -- OIDC config (nullable group)
    oidc_issuer_url             TEXT,
    oidc_client_id              TEXT,
    oidc_has_client_secret      INTEGER,
    oidc_authorization_endpoint TEXT,
    oidc_token_endpoint         TEXT,
    oidc_userinfo_endpoint      TEXT,
    oidc_jwks_uri               TEXT,
    oidc_configured_at          INTEGER,

    -- OIDC secret (nullable group)
    oidc_encrypted_client_secret TEXT,
    oidc_secret_stored_at        INTEGER,

    -- Owner (nullable group)
    owner_email                 TEXT,
    owner_oidc_subject          TEXT,
    owner_created_at            INTEGER,

    -- Timestamps
    created_at                  INTEGER NOT NULL,
    updated_at                  INTEGER NOT NULL
);
