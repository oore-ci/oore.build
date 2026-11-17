## Oore CI

This app builds on [Oore](https://oore.build), a self-hosted CI for Flutter apps. The pipeline is defined in `.oore.yaml` at the repository root. Drive Oore with the `oore` CLI. Pass `--json` to any command to get JSON on standard output.

### Signing in

- Read the token from the `OORE_TOKEN` environment variable and the instance from `OORE_URL`. Never print the token, write it to a file, or put it in a commit, log or message.
- If no token is set, ask the person for one. They create it with `oore token create --name <name>` or in Settings > API tokens.

### Change the pipeline file

1. Check the file: `oore pipeline validate`. It needs no sign-in.
2. See what a build would run: `oore pipeline plan`.
3. Commit and push. Oore builds the file from the commit it builds.

### Build and wait

- Start a build and wait for its result: `oore build run --pipeline <pipeline> --wait --json`.
- Follow a build that is already running: `oore build watch <build-id> --json`.
- Find pipelines and builds: `oore pipeline list --json`, `oore build list --json`.

### When a build fails

- Ask why: `oore build why <build-id> --json`. It names the failed step, its last log lines, failed tests and a hint.
- Read more of the log: `oore build logs <build-id> --step <step>`.
- Fix the cause in the code or in `.oore.yaml`, push, and build again. Do not retry an unchanged build more than once.

### Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Done. |
| 1 | Failed: the build did not succeed, or another error. |
| 2 | Blocked: a person must act, for example to approve. Stop and tell the person. |
| 3 | The store refused the release. |
| 4 | Not signed in, or the token may not do this. |
| 5 | Not found. |
| 64 | The command was called wrongly. |

### Safety rules

- Never start, raise, resume, complete, approve or promote a store release, or change release freezes, unless the person asked for exactly that.
- Never change secrets, signing identities, tester groups or API tokens unless the person asked for exactly that.
- Destructive commands need `--yes` when no terminal is attached. Pass it only for the action the person asked for.
- Use `--dry-run` first on release commands to see what would change.

### Docs

- Everything about Oore, for agents: https://docs.oore.build/llms.txt
- The pipeline file: https://docs.oore.build/reference/config/oore-yaml-v2
- The CLI: https://docs.oore.build/reference/cli
