type Health = { version?: unknown };
type UpdateStatus = { managed_service?: unknown };

export type Runner = {
  id: string;
  name: string;
  status: string;
  capabilities?: Record<string, unknown>;
  last_heartbeat_at?: number;
};

type RunnerList = { runners?: Runner[] };
type TriggerMode = "api" | "observe";

const delay = (milliseconds: number) =>
  new Promise((resolve) => setTimeout(resolve, milliseconds));

export function parseTriggerMode(value?: string): TriggerMode {
  const normalized = value?.trim().toLowerCase() || "api";
  if (normalized === "api" || normalized === "observe") return normalized;
  throw new Error("OORE_UPGRADE_SMOKE_TRIGGER must be api or observe");
}

export function selectRunner(runners: Runner[], requestedId?: string): Runner {
  if (requestedId) {
    const runner = runners.find(({ id }) => id === requestedId);
    if (!runner) throw new Error(`Runner ${requestedId} was not found`);
    return runner;
  }

  const online = runners.filter(({ status }) => status === "online");
  if (online.length !== 1) {
    throw new Error(
      `Expected exactly one online runner, found ${online.length}; set OORE_UPGRADE_SMOKE_RUNNER_ID`,
    );
  }
  return online[0];
}

export function runnerMatchesUpgrade(
  runner: Runner,
  previousHeartbeat: number,
  expectedVersion: string,
): boolean {
  return (
    runner.status === "online" &&
    (runner.last_heartbeat_at ?? 0) > previousHeartbeat &&
    runner.capabilities?.version === expectedVersion &&
    runner.capabilities?.protocol_version === 4
  );
}

async function responseJson<T>(
  response: Response,
  operation: string,
): Promise<T> {
  if (!response.ok) {
    const detail = (await response.text()).trim().slice(0, 500);
    throw new Error(
      `${operation} failed (${response.status})${detail ? `: ${detail}` : ""}`,
    );
  }
  return response.json() as Promise<T>;
}

async function main() {
  const baseUrl = (
    process.env.OORE_UPGRADE_SMOKE_URL ?? "http://127.0.0.1:8787"
  ).replace(/\/$/, "");
  const token = process.env.OORE_UPGRADE_SMOKE_SESSION_TOKEN?.trim();
  if (!token) throw new Error("OORE_UPGRADE_SMOKE_SESSION_TOKEN is required");
  const expectedVersion =
    process.env.OORE_UPGRADE_SMOKE_EXPECTED_VERSION?.trim();
  if (!expectedVersion)
    throw new Error("OORE_UPGRADE_SMOKE_EXPECTED_VERSION is required");

  const timeoutSeconds = Number(
    process.env.OORE_UPGRADE_SMOKE_TIMEOUT_SECONDS ?? "300",
  );
  if (!Number.isFinite(timeoutSeconds) || timeoutSeconds <= 0) {
    throw new Error(
      "OORE_UPGRADE_SMOKE_TIMEOUT_SECONDS must be a positive number",
    );
  }
  const auth = { Authorization: `Bearer ${token}` };
  const triggerMode = parseTriggerMode(process.env.OORE_UPGRADE_SMOKE_TRIGGER);

  const oldHealth = await responseJson<Health>(
    await fetch(`${baseUrl}/healthz`),
    "health check",
  );
  if (typeof oldHealth.version !== "string" || !oldHealth.version) {
    throw new Error("The backend health response did not include a version");
  }
  const oldVersion = oldHealth.version;
  if (oldVersion === expectedVersion) {
    throw new Error(
      `Backend is already on expected version ${expectedVersion}`,
    );
  }

  const initialList = await responseJson<RunnerList>(
    await fetch(`${baseUrl}/v1/runners`, { headers: auth }),
    "runner list",
  );
  const initialRunner = selectRunner(
    initialList.runners ?? [],
    process.env.OORE_UPGRADE_SMOKE_RUNNER_ID?.trim() || undefined,
  );
  const oldHeartbeat = initialRunner.last_heartbeat_at;
  if (typeof oldHeartbeat !== "number") {
    throw new Error(`Runner ${initialRunner.name} has no recorded heartbeat`);
  }

  const updateStatus = await responseJson<UpdateStatus>(
    await fetch(`${baseUrl}/v1/system/update`, { headers: auth }),
    "runtime update status",
  );
  if (updateStatus.managed_service !== true) {
    throw new Error("The backend is not running as a UI-managed service");
  }

  console.log(
    `[direct-runner-upgrade-smoke] Updating ${oldVersion}; runner ${initialRunner.name} heartbeat ${oldHeartbeat}`,
  );
  if (triggerMode === "api") {
    await responseJson(
      await fetch(`${baseUrl}/v1/system/update`, {
        method: "POST",
        headers: auth,
      }),
      "runtime update",
    );
  } else {
    console.log(
      "[direct-runner-upgrade-smoke] Observation armed. Trigger the backend update from the Oore UI now.",
    );
  }

  let newVersion: string | undefined;
  const backendDeadline = Date.now() + timeoutSeconds * 1_000;
  while (Date.now() < backendDeadline) {
    try {
      const health = await responseJson<Health>(
        await fetch(`${baseUrl}/healthz`),
        "health check after restart",
      );
      if (health.version === expectedVersion) {
        newVersion = expectedVersion;
        break;
      }
    } catch {
      // The managed daemon is expected to disappear briefly while launchd restarts it.
    }
    await delay(2_000);
  }
  if (!newVersion) {
    throw new Error(
      `Backend did not advance from ${oldVersion} to ${expectedVersion} within ${timeoutSeconds}s`,
    );
  }

  const runnerDeadline = Date.now() + timeoutSeconds * 1_000;
  while (Date.now() < runnerDeadline) {
    try {
      const list = await responseJson<RunnerList>(
        await fetch(`${baseUrl}/v1/runners`, { headers: auth }),
        "runner list after restart",
      );
      const runner = (list.runners ?? []).find(
        ({ id }) => id === initialRunner.id,
      );
      if (runner && runnerMatchesUpgrade(runner, oldHeartbeat, newVersion)) {
        console.log(
          `[direct-runner-upgrade-smoke] Passed: backend and runner ${newVersion}, protocol 4, heartbeat ${runner.last_heartbeat_at}`,
        );
        return;
      }
    } catch {
      // Session and runner queries can race the managed daemon restart.
    }
    await delay(2_000);
  }

  throw new Error(
    `Runner ${initialRunner.name} did not return online on ${newVersion} with protocol 4 and a newer heartbeat within ${timeoutSeconds}s`,
  );
}

if (import.meta.main) {
  main().catch((error) => {
    console.error(
      `[direct-runner-upgrade-smoke] ${error instanceof Error ? error.message : error}`,
    );
    process.exit(1);
  });
}
