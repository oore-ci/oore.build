---
status: implemented
---

# oore setup

The `setup` command configures a fresh oore.build instance. It can be run interactively (default) or used to generate bootstrap tokens with the `token` subcommand.

## Interactive setup

```bash
oore setup [--daemon-url <url>]
```

Runs the full 4-step interactive setup flow from the terminal. This is the CLI equivalent of the web-based setup wizard.

### Flags

| Flag | Default | Env var | Description |
|---|---|---|---|
| `--daemon-url` | `http://127.0.0.1:8787` | `OORE_DAEMON_URL` | URL of the oored daemon |

### Interactive flow

The interactive setup walks through 4 steps. At each step, the command checks the current state and skips already-completed steps.

#### Step 1: Bootstrap token verification

```
[Step 1/4] Bootstrap token verification
  Database: ~/Library/Application Support/oore/oore.db
  Generating bootstrap token (TTL: 15m)...
  Verifying token with daemon...
  > Bootstrap verified. Session token acquired.
```

The CLI:
1. Checks the daemon state via `GET /v1/public/setup-status`
2. Opens the local SQLite database
3. Generates a bootstrap token with a 15-minute TTL
4. Verifies the token via `POST /v1/setup/bootstrap-token/verify`
5. Stores the returned session token in memory for subsequent steps

#### Step 2: OIDC configuration

```
[Step 2/4] OIDC provider configuration
  OIDC Issuer URL: https://accounts.google.com
  Client ID: your-client-id.apps.googleusercontent.com
  Client Secret (optional, press Enter to skip): ****
  > OIDC provider configured. Issuer: https://accounts.google.com
```

The CLI prompts for:
- **OIDC Issuer URL** — text input (required)
- **Client ID** — text input (required)
- **Client Secret** — password input (optional, hidden)

Submits to `POST /v1/setup/oidc/configure` with Bearer auth.

To obtain these values, see the [OIDC setup guides](/guides/oidc/).

#### Step 3: Owner account

```
[Step 3/4] Owner account setup
  You'll authenticate via your OIDC provider to prove your identity.

  Before continuing, ensure this redirect URI is whitelisted
  in your OIDC provider's allowed callback URLs:

    http://localhost:52341

  Continue with OIDC authentication? [y/N]
```

The CLI:
1. Binds a TCP listener on a random free port on `127.0.0.1`
2. Displays the redirect URI for the operator to add to their IdP's allowed callback URLs
3. Calls `POST /v1/setup/owner/start-oidc` with the loopback redirect URI
4. Opens the authorization URL in the default browser (`open` on macOS)
5. Waits for the IdP to redirect back to the loopback listener
6. Extracts the `code` and `state` query parameters
7. Submits to `POST /v1/setup/owner/verify-oidc`
8. Sends an HTML success page to the browser

If the browser cannot be opened automatically, the authorization URL is printed for manual navigation.

#### Step 4: Finalize

```
[Step 4/4] Finalize setup
  Complete setup? This will lock all setup endpoints. [y/N]
  Completing setup...
  > Setup complete! Instance ID: 550e8400-e29b-41d4-a716-446655440000

Your oore.build instance is ready.
```

Calls `POST /v1/setup/complete` with Bearer auth.

::: danger
This step is irreversible. Once confirmed, all setup endpoints are permanently disabled.
:::

### State resumption

If the daemon is already past certain steps (e.g., OIDC was configured in a previous partial setup), the CLI detects this and skips completed steps automatically.

### Error handling

| Situation | Behavior |
|---|---|
| Cannot reach daemon | Prints a message suggesting to start `oored run` |
| Setup already complete | Exits confirming `ready` state |
| Session expired | Automatically re-acquires a session token and continues |
| OIDC configuration error | Offers retry with fresh inputs |
| OIDC authentication error | Displays the IdP error in the browser and exits |

---

## Setup Token {#setup-token}

```bash
oore setup token [--ttl <duration>] [--json] [--state-file <path>]
```

Generate a one-time bootstrap token for initializing an oore.build instance. This token is used as the first step in the setup process.

### Flags

| Flag | Default | Env var | Description |
|---|---|---|---|
| `--ttl` | `15m` | — | Token time-to-live (e.g., `5m`, `1h`, `30s`) |
| `--json` | `false` | — | Output in machine-readable JSON format |
| `--state-file` | Platform default | `OORE_SETUP_STATE_FILE` | Override the database path |

### TTL format

The `--ttl` flag accepts [humantime](https://docs.rs/humantime/) duration strings:

| Example | Duration |
|---|---|
| `15m` | 15 minutes |
| `1h` | 1 hour |
| `30s` | 30 seconds |
| `1h30m` | 1 hour 30 minutes |

### Output

#### Default (human-readable)

```
Bootstrap token generated.

Token:   a1b2c3d4e5f6...
Expires: 2026-02-06 14:30:00 (15m from now)
State:   bootstrap_pending
DB:      /Users/you/Library/Application Support/oore/oore.db

To complete setup, either:
  1. Open https://ci.oore.build/setup and add your backend instance
  2. Run: oore setup
```

#### JSON (`--json`)

```json
{
  "token": "a1b2c3d4e5f6...",
  "expires_at": 1738800000,
  "state": "bootstrap_pending",
  "database": "/Users/you/Library/Application Support/oore/oore.db",
  "instance_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

### Behavior

1. Resolves the database path (flag > env var > platform default)
2. Connects to the SQLite database, creating it if necessary
3. Loads or creates the initial setup state (starts as `bootstrap_pending`)
4. If the state is already `ready`, exits with an error
5. Generates a 32-byte random token using `OsRng`
6. Stores the SHA-256 hash and expiry in the database
7. Prints the plaintext token (never stored)

::: warning
The plaintext bootstrap token is displayed only once. If lost, generate a new one with `oore setup token`.
:::

### Examples

```bash
# Generate with default 15-minute TTL
oore setup token

# Generate with 1-hour TTL
oore setup token --ttl 1h

# Generate with JSON output for scripting
oore setup token --json

# Use a custom database path
oore setup token --state-file /tmp/oore-test.db
```
