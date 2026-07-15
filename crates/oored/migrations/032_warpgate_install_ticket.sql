-- Optional Warpgate access ticket for non-interactive iOS OTA downloads.

ALTER TABLE trusted_proxy_settings
    ADD COLUMN encrypted_warpgate_ticket TEXT;
