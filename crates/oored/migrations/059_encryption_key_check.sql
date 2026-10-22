-- A known value encrypted with the instance encryption key. The daemon
-- decrypts it on startup so a missing or replaced key is caught before
-- anything else is encrypted with the wrong key.
CREATE TABLE IF NOT EXISTS encryption_key_check (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    check_ciphertext TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
