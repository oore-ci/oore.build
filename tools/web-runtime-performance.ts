import { performance } from "node:perf_hooks";

import { groupLogs } from "../apps/web/src/components/terminal-log-viewer/log-model";
import { mergeBuildLogChunks } from "../apps/web/src/lib/log-stream-utils";
import type { BuildLogChunk } from "../apps/web/src/lib/types";

const LONG_TASK_MS = 50;

function measure(name: string, run: () => void): number {
  const startedAt = performance.now();
  run();
  const duration = performance.now() - startedAt;
  console.log(`${name}: ${duration.toFixed(2)} ms / ${LONG_TASK_MS} ms`);
  if (duration >= LONG_TASK_MS) {
    throw new Error(`${name} exceeded the long-task budget`);
  }
  return duration;
}

const projects = Array.from({ length: 200 }, (_, index) => ({
  id: `project-${index}`,
  name: `Project ${index}`,
}));
const builds = Array.from({ length: 20 }, (_, index) => ({
  projectId: `project-${index}`,
}));

measure("Build list project resolution (20 builds / 200 projects)", () => {
  for (const build of builds) {
    projects.find((project) => project.id === build.projectId);
  }
});

const logs: Array<BuildLogChunk> = Array.from(
  { length: 10_000 },
  (_, sequence) => ({
    sequence,
    content:
      sequence % 997 === 0
        ? `error: generated failure ${sequence}`
        : `build output line ${sequence}`,
    stream: "stdout",
  }),
);

let currentLogs: Array<BuildLogChunk> = [];
const logsBySequence = new Map<number, BuildLogChunk>();
let worstFrame = 0;
const streamStartedAt = performance.now();
for (let offset = 0; offset < logs.length; offset += 50) {
  const startedAt = performance.now();
  currentLogs = mergeBuildLogChunks(
    currentLogs,
    logsBySequence,
    logs.slice(offset, offset + 50),
  ).logs;
  groupLogs(currentLogs, []);
  worstFrame = Math.max(worstFrame, performance.now() - startedAt);
}
const streamDuration = performance.now() - streamStartedAt;
console.log(
  `Live log processing (10,000 lines, 50-line bursts): ${streamDuration.toFixed(2)} ms total; ${worstFrame.toFixed(2)} ms worst frame / ${LONG_TASK_MS} ms`,
);
if (worstFrame >= LONG_TASK_MS) {
  throw new Error("Live log frame exceeded the long-task budget");
}

measure("Terminal log grouping and search (10,000 lines)", () => {
  const grouped = groupLogs(currentLogs, []);
  grouped.allVisibleLogs.filter((entry) =>
    entry.content.toLowerCase().includes("failure"),
  );
});

const users = Array.from({ length: 1_000 }, (_, index) => ({
  email: `user${index}@example.com`,
  status: index % 3 === 0 ? "disabled" : "active",
}));

measure("Admin user filter and sort (1,000 users)", () => {
  users
    .filter((user) => user.email.includes("99") || user.status === "disabled")
    .sort((left, right) => left.email.localeCompare(right.email));
});
