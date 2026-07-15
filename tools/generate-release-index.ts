import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import path from "node:path";

type Channel = "alpha" | "beta" | "stable";

interface GitHubRelease {
  tag_name: string;
  name?: string | null;
  body?: string | null;
  html_url: string;
  published_at?: string | null;
  created_at: string;
  draft: boolean;
  prerelease: boolean;
}

export interface ReleaseEntry {
  schema_version: 1;
  channel: Channel;
  version: string;
  tag: string;
  published_at: string;
  release_name: string;
  release_notes: string;
  release_url: string;
  changelog_url: string;
  download_base_url: string;
}

const TAG_PATTERN = /^v?(\d+)\.(\d+)\.(\d+)(?:-(alpha|beta)\.(\d+))?$/;

function parseTag(tag: string) {
  const match = TAG_PATTERN.exec(tag);
  if (!match) return null;
  const prerelease = match[4] as Exclude<Channel, "stable"> | undefined;
  return {
    channel: prerelease ?? "stable",
    version: [match[1], match[2], match[3], match[5] ?? "-1"].map(Number),
  };
}

function compareReleases(a: GitHubRelease, b: GitHubRelease) {
  const left = parseTag(a.tag_name)!.version;
  const right = parseTag(b.tag_name)!.version;
  for (let index = 0; index < left.length; index += 1) {
    if (left[index] !== right[index]) return right[index]! - left[index]!;
  }
  return 0;
}

function changelogUrl(release: GitHubRelease) {
  return (
    release.body?.match(
      /\*\*Full Changelog\*\*:\s*(https:\/\/github\.com\/\S+)/,
    )?.[1] ?? release.html_url
  );
}

export function buildReleaseIndex(
  source: Array<GitHubRelease> | Array<Array<GitHubRelease>>,
  repository: string,
) {
  const releases = source.flat().filter((release) => {
    const parsed = parseTag(release.tag_name);
    if (!parsed || release.draft) return false;
    return parsed.channel === "stable"
      ? !release.prerelease
      : release.prerelease;
  });

  return (["alpha", "beta", "stable"] as const).map((channel) => {
    const entries = releases
      .filter((release) => parseTag(release.tag_name)?.channel === channel)
      .sort(compareReleases)
      .map<ReleaseEntry>((release) => ({
        schema_version: 1,
        channel,
        version: release.tag_name.replace(/^v/, ""),
        tag: release.tag_name.startsWith("v")
          ? release.tag_name
          : `v${release.tag_name}`,
        published_at: release.published_at ?? release.created_at,
        release_name: release.name || release.tag_name,
        release_notes: release.body || "",
        release_url: release.html_url,
        changelog_url: changelogUrl(release),
        download_base_url: `https://github.com/${repository}/releases/download/${release.tag_name}`,
      }));
    return { channel, entries };
  });
}

async function main() {
  const [input, output, repository = "oore-ci/oore.build"] =
    process.argv.slice(2);
  if (!input || !output) {
    throw new Error(
      "usage: bun tools/generate-release-index.ts <github-releases.json> <output-dir> [owner/repo]",
    );
  }

  const source = JSON.parse(await readFile(input, "utf8")) as
    Array<GitHubRelease> | Array<Array<GitHubRelease>>;
  const index = buildReleaseIndex(source, repository);

  await rm(output, { recursive: true, force: true });
  await mkdir(path.join(output, "latest"), { recursive: true });
  const historyHeaderRules = index
    .map(
      ({ channel }) =>
        `/${channel}.json\n  Access-Control-Allow-Origin: *\n  Cache-Control: public, max-age=300, s-maxage=300`,
    )
    .join("\n\n");
  await writeFile(
    path.join(output, "_headers"),
    `/latest/*.json\n  Access-Control-Allow-Origin: *\n  Cache-Control: public, max-age=60, s-maxage=300\n\n${historyHeaderRules}\n`,
  );

  for (const { channel, entries } of index) {
    await writeFile(
      path.join(output, `${channel}.json`),
      `${JSON.stringify({ schema_version: 1, channel, releases: entries }, null, 2)}\n`,
    );
    if (entries[0]) {
      await writeFile(
        path.join(output, "latest", `${channel}.json`),
        `${JSON.stringify(entries[0], null, 2)}\n`,
      );
    }
  }
}

if (import.meta.main) await main();
