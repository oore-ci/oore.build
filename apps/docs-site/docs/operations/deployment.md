---
status: implemented
description: 'Deploy Oore CI in production including launchd, reverse proxy, and TLS setup.'
---

# Production Deployment

Checklist and guidance for deploying Oore CI in a production environment.

For separate backend and frontend hosts, see [Split Backend and Frontend](/operations/split-roles).
For one internal-only macOS rollout example behind NetBird + Warpgate, see [Mac Studio + NetBird + Warpgate](/operations/mac-studio-netbird-warpgate).

## Prerequisites

- macOS host with [all prerequisites](/getting-started/prerequisites) installed
- A domain name for your instance (e.g., `ci.mycompany.com`)
- TLS certificate for the domain
- Remote auth configured (OIDC default via [OIDC guides](/guides/oidc/), or `trusted_proxy` if deployed behind an identity-aware proxy)

## Deployment checklist

### 1. Build from source

```bash
git clone https://github.com/devaryakjha/oore.build.git
cd Oore CI
cargo build --release -p oored
cargo build --release -p oore
```

The release binaries are at `target/release/oored` and `target/release/oore`.

### 2. Install the daemon service

Keep the daemon bound to loopback and run it as a macOS launchd user service:

```bash
./target/release/oored install-service \
  --listen 127.0.0.1:8787 \
  --env OORE_PUBLIC_URL=https://ci.mycompany.com \
  --env OORE_CORS_ORIGINS=https://ci.mycompany.com \
  --env RUST_LOG=info
```

If you installed release binaries with the installer, use `oored install-service`
instead of `./target/release/oored install-service`.

The service plist is written to
`~/Library/LaunchAgents/build.oore.oored.plist`, and daemon logs are written to
`~/.oore/logs/oored.log`.

### 3. Set up a reverse proxy

Place a reverse proxy (nginx, Caddy, etc.) in front of the daemon to handle TLS termination and serve the built web UI. Keep the daemon on loopback and expose only the HTTPS proxy to users.

```nginx
server {
    listen 443 ssl;
    server_name ci.mycompany.com;

    root /absolute/path/to/oore.build/apps/web/dist;
    index index.html;

    ssl_certificate /etc/ssl/certs/ci.mycompany.com.pem;
    ssl_certificate_key /etc/ssl/private/ci.mycompany.com.key;

    location / {
        try_files $uri $uri/ /index.html;
    }

    location /v1/ {
        proxy_pass http://127.0.0.1:8787;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_set_header X-Oore-User-Email $http_x_oore_user_email;
    }

    location /healthz {
        proxy_pass http://127.0.0.1:8787;
    }

    location /metrics {
        proxy_pass http://127.0.0.1:8787;
        # Restrict to internal monitoring network
        allow 10.0.0.0/8;
        deny all;
    }
}
```

### 4. Run the setup wizard

```bash
./target/release/oore setup token --ttl 15m
./target/release/oore setup --daemon-url http://127.0.0.1:8787
```

If your browser reaches the UI through an identity-aware proxy, choose `Remote (Trusted Proxy)` during setup instead of OIDC, enter the initial owner email, and configure the header your proxy forwards.

### 5. Configure artifact storage

For production, use S3 or R2 instead of local storage. See [Configure Storage](/guides/artifacts/configure-storage).

### 6. Verify

```bash
curl https://ci.mycompany.com/v1/public/setup-status
curl https://ci.mycompany.com/healthz
launchctl print gui/$(id -u)/build.oore.oored
```

## Security hardening

- **TLS**: Always use HTTPS for the browser-visible origin. Internal-only VPN HTTPS is fine; it does not need to be public internet reachable.
- **CORS**: Set `OORE_CORS_ORIGINS` to your production domain only.
- **Firewall**: The daemon should only be accessible through the reverse proxy.
- **Backups**: Schedule regular database backups (see [Backup and Restore](/operations/backup-restore)).
- **Monitoring**: Set up health checks and metrics collection (see [Monitoring](/operations/monitoring)).
