---
status: implemented
description: "Deploy Oore CI in production including launchd, reverse proxy, and TLS setup."
---

# Production Deployment

Checklist and guidance for deploying Oore CI in a production environment.

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

### 2. Configure the daemon

Set environment variables:

```bash
export OORED_LISTEN_ADDR=127.0.0.1:8787
export OORE_CORS_ORIGINS=https://ci.mycompany.com
export RUST_LOG=info
```

### 3. Set up a reverse proxy

Place a reverse proxy (nginx, Caddy, etc.) in front of the daemon to handle TLS termination:

```nginx
server {
    listen 443 ssl;
    server_name ci.mycompany.com;

    ssl_certificate /etc/ssl/certs/ci.mycompany.com.pem;
    ssl_certificate_key /etc/ssl/private/ci.mycompany.com.key;

    location /v1/ {
        proxy_pass http://127.0.0.1:8787;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
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

### 5. Configure artifact storage

For production, use S3 or R2 instead of local storage. See [Configure Storage](/guides/artifacts/configure-storage).

### 6. Verify

```bash
curl https://ci.mycompany.com/v1/public/setup-status
curl https://ci.mycompany.com/healthz
```

## Security hardening

- **TLS**: Always use HTTPS for production. The daemon itself doesn't handle TLS.
- **CORS**: Set `OORE_CORS_ORIGINS` to your production domain only.
- **Firewall**: The daemon should only be accessible through the reverse proxy.
- **Backups**: Schedule regular database backups (see [Backup and Restore](/operations/backup-restore)).
- **Monitoring**: Set up health checks and metrics collection (see [Monitoring](/operations/monitoring)).
