-- Allow token-protected artifact delivery to bypass an interactive auth proxy.

ALTER TABLE external_access_network_settings
    ADD COLUMN artifact_delivery_url TEXT;
