import { describe, expect, test } from "bun:test";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";

import { buildReleaseIndex } from "./generate-release-index";

const release = (tag: string, prerelease: boolean, body = "") => ({
  tag_name: tag,
  name: tag,
  body,
  html_url: `https://github.com/oore-ci/oore.build/releases/tag/${tag}`,
  published_at: "2026-07-14T00:00:00Z",
  created_at: "2026-07-14T00:00:00Z",
  draft: false,
  prerelease,
});

describe("release index", () => {
  test("keeps each channel newest-first and derives latest from the same entries", () => {
    const index = buildReleaseIndex(
      [
        [
          release("v1.0.0-alpha.9", true),
          release(
            "v1.0.0-alpha.10",
            true,
            "**Full Changelog**: https://github.com/oore-ci/oore.build/compare/a...b",
          ),
          release("v1.0.0-beta.2", true),
          release("v1.0.0", false),
        ],
      ],
      "oore-ci/oore.build",
    );

    const alpha = index.find((item) => item.channel === "alpha")!;
    expect(alpha.entries.map((item) => item.tag)).toEqual([
      "v1.0.0-alpha.10",
      "v1.0.0-alpha.9",
    ]);
    expect(alpha.entries[0]).toMatchObject({
      channel: "alpha",
      version: "1.0.0-alpha.10",
      changelog_url: "https://github.com/oore-ci/oore.build/compare/a...b",
      download_base_url:
        "https://github.com/oore-ci/oore.build/releases/download/v1.0.0-alpha.10",
    });
  });

  test("writes the public latest and channel-history contract", async () => {
    const root = await mkdtemp(path.join(tmpdir(), "oore-release-index-"));
    const input = path.join(root, "releases.json");
    const output = path.join(root, "public");
    try {
      await writeFile(
        input,
        JSON.stringify([[release("v1.0.0-alpha.10", true)]]),
      );
      const result = Bun.spawnSync({
        cmd: [
          process.execPath,
          path.join(import.meta.dir, "generate-release-index.ts"),
          input,
          output,
        ],
        stdout: "pipe",
        stderr: "pipe",
      });
      expect(result.exitCode).toBe(0);

      const latest = JSON.parse(
        await readFile(path.join(output, "latest/alpha.json"), "utf8"),
      );
      const history = JSON.parse(
        await readFile(path.join(output, "alpha.json"), "utf8"),
      );
      const headers = await readFile(path.join(output, "_headers"), "utf8");

      expect(latest.tag).toBe("v1.0.0-alpha.10");
      expect(history.releases).toEqual([latest]);
      expect(headers).toContain("/latest/*.json");
      expect(headers).toContain("/alpha.json");
      expect(headers.split("\n")).not.toContain("/*.json");
      expect(headers.match(/Access-Control-Allow-Origin: \*/g)).toHaveLength(4);
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });
});
