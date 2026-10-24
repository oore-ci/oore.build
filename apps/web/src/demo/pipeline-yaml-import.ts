import type {
  BuildPlatform,
  ImportPipelineYamlRequest,
  ImportPipelineYamlResponse,
  PipelineExecutionConfig,
  TriggerConfig,
} from '@oore/client/models'
import { onLines } from './pipeline-yaml'

/**
 * The demo's stand-in for `POST /v1/pipelines/import-yaml`. It reads files
 * shaped like the generated one and follows the daemon's rules for what the
 * form can hold; the daemon parses any YAML and proves the round trip.
 */

type Node =
  | { kind: 'text'; text: string }
  | { kind: 'list'; items: Array<Node> }
  | { kind: 'map'; map: Record<string, Node> }

const text = (value: string): Node => ({ kind: 'text', text: value })
const mapNode = (map: Record<string, Node>): Node => ({ kind: 'map', map })

const DEFAULT_BUILDS = {
  android: 'flutter build apk --release',
  ios: 'flutter build ios --release --no-codesign',
  macos: 'flutter build macos --release',
} satisfies Record<BuildPlatform, string>

const DEFAULT_ARTIFACTS = {
  android: [
    'build/app/outputs/flutter-apk/*.apk',
    'build/app/outputs/bundle/**/*.aab',
  ],
  ios: ['build/ios/ipa/*.ipa'],
  macos: ['build/macos/Build/Products/Release/*.app'],
} satisfies Record<BuildPlatform, Array<string>>

const PLATFORMS: Array<BuildPlatform> = ['android', 'ios', 'macos']
const PUB_GET = 'flutter pub get'
const TITLES = {
  android: 'Android',
  ios: 'iOS',
  macos: 'macOS',
} satisfies Record<BuildPlatform, string>
const KNOWN_STEPS = new Map([
  ['flutter pub get', 'Get packages'],
  ['flutter analyze', 'Analyze'],
  ['flutter test', 'Run tests'],
])

class YamlError extends Error {}

/** Drop a trailing ` # comment` outside quotes. */
function stripComment(line: string): string {
  let quote: string | null = null
  for (let i = 0; i < line.length; i++) {
    const char = line[i]
    if (quote) {
      if (char === quote) quote = null
    } else if (char === "'" || char === '"') {
      quote = char
    } else if (char === '#' && (i === 0 || line[i - 1] === ' ')) {
      return line.slice(0, i).trimEnd()
    }
  }
  return line.trimEnd()
}

function unquote(raw: string): string {
  const value = raw.trim()
  if (value.startsWith("'") && value.endsWith("'") && value.length > 1) {
    return value.slice(1, -1).replaceAll("''", "'")
  }
  if (value.startsWith('"') && value.endsWith('"') && value.length > 1) {
    return value.slice(1, -1).replaceAll('\\n', '\n').replaceAll('\\"', '"')
  }
  return value
}

function flowList(raw: string): Array<Node> {
  const inner = raw.trim().slice(1, -1).trim()
  return inner ? inner.split(',').map((item) => text(unquote(item))) : []
}

interface Line {
  indent: number
  text: string
}

/** A small reader for the block YAML the export writes. */
export function parseDemoYaml(source: string): Node {
  const lines: Array<Line> = []
  const raw = source.split('\n')
  const blocks = new Map<number, string>()
  for (let i = 0; i < raw.length; i++) {
    const text = stripComment(raw[i])
    if (!text.trim()) continue
    const indent = text.length - text.trimStart().length
    // A literal block scalar keeps the lines indented under it.
    if (/:\s*\|-?$/.test(text)) {
      const body: Array<string> = []
      let j = i + 1
      let blockIndent = -1
      while (j < raw.length) {
        const next = raw[j]
        const nextIndent = next.length - next.trimStart().length
        if (next.trim() && nextIndent <= indent) break
        if (blockIndent < 0 && next.trim()) blockIndent = nextIndent
        body.push(next.slice(Math.max(blockIndent, 0)))
        j++
      }
      blocks.set(lines.length, body.join('\n').trimEnd())
      lines.push({ indent, text: text.replace(/\|-?$/, '').trimEnd() })
      i = j - 1
      continue
    }
    lines.push({ indent, text: text.trimStart() })
  }
  const anchors = new Map<string, Node>()
  let index = 0

  function value(rest: string, indent: number, at: number): Node {
    const anchor = /^&(\w+)\s*(.*)$/.exec(rest)
    if (anchor) {
      const node = value(anchor[2], indent, at)
      anchors.set(anchor[1], node)
      return node
    }
    if (rest.startsWith('*')) {
      const node = anchors.get(rest.slice(1))
      if (!node) throw new YamlError(`unknown alias ${rest}`)
      return node
    }
    const literal = blocks.get(at)
    if (literal !== undefined) return text(literal)
    if (rest === '{}') return mapNode({})
    if (rest.startsWith('[')) return { kind: 'list', items: flowList(rest) }
    if (rest.startsWith('{')) {
      const map: Record<string, Node> = {}
      for (const pair of rest.slice(1, -1).split(',')) {
        const [key, ...tail] = pair.split(':')
        if (key.trim()) map[key.trim()] = text(unquote(tail.join(':')))
      }
      return mapNode(map)
    }
    if (rest) return text(unquote(rest))
    if (index < lines.length && lines[index].indent > indent) {
      return block(lines[index].indent)
    }
    // `- ` lists may sit at the key's own indent.
    if (index < lines.length && lines[index].text.startsWith('- ')) {
      return block(lines[index].indent)
    }
    return text('')
  }

  function block(indent: number): Node {
    if (lines[index].text.startsWith('- ') || lines[index].text === '-') {
      const list: Array<Node> = []
      while (
        index < lines.length &&
        lines[index].indent === indent &&
        lines[index].text.startsWith('-')
      ) {
        const at = index
        const item = lines[index].text.slice(1).trim()
        index++
        if (/^[\w.-]+:(\s|$)/.test(item)) {
          // A mapping item: its first key is on the dash line.
          const map: Record<string, Node> = {}
          const [key, ...tail] = item.split(':')
          map[key] = value(tail.join(':').trim(), indent + 2, at)
          while (index < lines.length && lines[index].indent === indent + 2) {
            const [name, node] = entry(indent + 2)
            map[name] = node
          }
          list.push(mapNode(map))
        } else {
          list.push(value(item, indent, at))
        }
      }
      return { kind: 'list', items: list }
    }
    const map: Record<string, Node> = {}
    while (index < lines.length && lines[index].indent === indent) {
      if (lines[index].text.startsWith('-')) break
      const [name, node] = entry(indent)
      map[name] = node
    }
    if (index < lines.length && lines[index].indent > indent) {
      throw new YamlError(
        `line ${index + 1}: unexpected indentation; check the spaces before it`,
      )
    }
    return mapNode(map)
  }

  function entry(indent: number): [string, Node] {
    const at = index
    const match = /^([^:]+):(?:\s+(.*))?$/.exec(lines[index].text)
    if (!match) {
      throw new YamlError(
        `YAML parse error: expected \`key: value\`, found \`${lines[index].text}\``,
      )
    }
    index++
    return [match[1].trim(), value((match[2] ?? '').trim(), indent, at)]
  }

  if (lines.length === 0) return mapNode({})
  const root = block(lines[0].indent)
  if (index < lines.length) {
    throw new YamlError(`YAML parse error near \`${lines[index].text}\``)
  }
  return root
}

function mapOf(node: Node | undefined): Record<string, Node> | undefined {
  return node?.kind === 'map' ? node.map : undefined
}

function textOf(node: Node | undefined): string | undefined {
  return node?.kind === 'text' ? node.text : undefined
}

function strings(node: Node | undefined): Array<string> {
  if (node?.kind === 'text') return node.text ? [node.text] : []
  if (node?.kind === 'list') {
    return node.items.flatMap((item) =>
      item.kind === 'text' ? [item.text] : [],
    )
  }
  return []
}

function stepName(run: string): string {
  return KNOWN_STEPS.get(run) ?? run.split('\n')[0].slice(0, 60)
}

function invalid(errors: Array<string>): ImportPipelineYamlResponse {
  return {
    valid: false,
    representable: false,
    errors,
    unsupported: [],
    notes: [],
  }
}

const TOP_LEVEL = new Set([
  'version',
  'app',
  'targets',
  'steps',
  'env',
  'vars',
  'on',
  'workflows',
  'runs_on',
  'isolation',
  'build_number_offset',
])

function isPlatform(key: string): key is BuildPlatform {
  return PLATFORMS.some((platform) => platform === key)
}

export function demoImportYaml(
  body: ImportPipelineYamlRequest,
): ImportPipelineYamlResponse {
  let parsed: Node
  try {
    parsed = parseDemoYaml(body.yaml)
  } catch (error) {
    return invalid([error instanceof Error ? error.message : String(error)])
  }
  const root = mapOf(parsed)
  if (!root) return invalid(['the file must be a mapping'])
  const unknown = Object.keys(root).filter((key) => !TOP_LEVEL.has(key))
  if (unknown.length > 0) {
    return invalid(
      unknown.map(
        (key) =>
          `YAML parse error: unknown field \`${key}\`, expected one of ${[...TOP_LEVEL].map((k) => `\`${k}\``).join(', ')}`,
      ),
    )
  }
  const version = textOf(root.version)
  if (version === '1') {
    return {
      ...invalid([]),
      valid: true,
      unsupported: [
        'This is a version 1 file. The pane edits version 2: run `oore pipeline migrate` on it, or start from the generated file',
      ],
    }
  }
  if (version !== '2') {
    return invalid(['unsupported config version, expected 1 or 2'])
  }

  const unsupported: Array<string> = []
  const notes: Array<string> = []
  const app = mapOf(root.app) ?? {}
  const workflows = Object.keys(mapOf(root.workflows) ?? {})
  if (workflows.length > 0) {
    unsupported.push(
      `workflows (${workflows.join(', ')}): the form edits one set of build settings, so a file with workflows is edited as YAML only`,
    )
  }
  const vars = Object.keys(mapOf(root.vars) ?? {})
  if (vars.length > 0) {
    unsupported.push(
      `vars (${vars.join(', ')}): the form has no variables; write the values where they are used`,
    )
  }
  const flavors = strings(app.flavors)
  if (flavors.length > 0) {
    unsupported.push(
      `app.flavors (${flavors.join(', ')}): the form builds each target once, without a flavor matrix`,
    )
  }
  if (unsupported.length > 0) {
    return { valid: true, representable: false, errors: [], unsupported, notes }
  }

  // Targets.
  const targets = mapOf(root.targets) ?? {}
  const errors: Array<string> = []
  const platforms: Array<BuildPlatform> = []
  for (const key of Object.keys(targets)) {
    if (isPlatform(key)) platforms.push(key)
    else errors.push(`targets.${key}: expected android, ios or macos`)
  }
  if (platforms.length === 0) errors.push('targets must define a target')
  const base = body.execution_config
  const play = body.google_play?.enabled ? body.google_play : undefined
  const store = body.app_store?.enabled ? body.app_store : undefined
  const commands: PipelineExecutionConfig['platform_commands'] = {
    ...base?.platform_commands,
  }
  const args: NonNullable<PipelineExecutionConfig['platform_build_args']> = {
    android: base?.platform_build_args?.android ?? [],
    ios: base?.platform_build_args?.ios ?? [],
    macos: base?.platform_build_args?.macos ?? [],
  }
  const artifacts: Array<string> = []
  let filePlay: ImportPipelineYamlResponse['google_play']
  let fileGroup: string | undefined
  for (const platform of platforms) {
    const target = mapOf(targets[platform])
    const build = textOf(target?.build)?.trim()
    if (!target || !build) {
      errors.push(`targets.${platform}.build is required`)
      continue
    }
    if (build.includes('\n')) {
      unsupported.push(
        `targets.${platform}.build spans several lines; the form's build command override is one line`,
      )
    }
    const bundle = platform === 'android' && !!play
    const render = (list: Array<string>) => {
      const command = [DEFAULT_BUILDS[platform], ...list].join(' ')
      return bundle
        ? `${command} && ${[
            'flutter build appbundle --release',
            ...list.filter((arg) => arg !== '--split-per-abi'),
          ].join(' ')}`
        : command
    }
    const head = DEFAULT_BUILDS[platform]
    const rest = build.startsWith(head)
      ? build.slice(head.length).split(' && ')[0]
      : null
    const listed =
      rest !== null && (rest === '' || rest.startsWith(' '))
        ? rest.split(/\s+/).filter(Boolean)
        : null
    if (listed && render(listed) === build) {
      commands[platform] = undefined
      args[platform] = listed
    } else {
      commands[platform] = build
      args[platform] = []
    }
    const patterns = strings(target.artifacts)
    for (const pattern of patterns.length > 0
      ? patterns
      : DEFAULT_ARTIFACTS[platform]) {
      if (!artifacts.includes(pattern)) artifacts.push(pattern)
    }
    const publish = mapOf(target.publish)
    const playSpec = mapOf(publish?.play)
    if (playSpec) {
      const rollout = textOf(playSpec.rollout)
      const draft = textOf(playSpec.draft)
      filePlay = {
        track: textOf(playSpec.track) ?? '',
        rollout_percent:
          rollout === undefined
            ? undefined
            : Math.round(Number(rollout) * 1e8) / 1e6,
        draft: draft === undefined ? undefined : draft === 'true',
      }
      if (!play) {
        unsupported.push(
          "publish.play: turn on Google Play upload in the pipeline's Publish settings; the form cannot",
        )
      } else {
        const percent = Math.min(play.rollout_percent ?? 100, 100)
        const same =
          filePlay.track === play.track.trim() &&
          (filePlay.rollout_percent === undefined ||
            filePlay.rollout_percent === percent) &&
          (filePlay.draft === undefined || filePlay.draft === play.draft)
        if (!same) {
          unsupported.push(
            `publish.play: the pipeline's Google Play settings upload to the ${play.track} track${percent < 100 ? ` at ${percent}%` : ''}; change the track, rollout or draft in the Publish settings`,
          )
        }
      }
    }
    const testflight = mapOf(publish?.testflight)
    if (testflight) {
      const group = textOf(testflight.group) ?? ''
      fileGroup = group
      const groups = store?.testflight_groups ?? []
      if (!store) {
        unsupported.push(
          "publish.testflight: turn on App Store Connect upload in the pipeline's Publish settings; the form cannot",
        )
      } else if (
        store.destination === 'app_store' ||
        groups.length !== 1 ||
        groups[0].trim() !== group
      ) {
        unsupported.push(
          "publish.testflight: the pipeline's App Store Connect settings upload elsewhere; change the TestFlight group in the Publish settings",
        )
      }
    }
  }
  if (errors.length > 0) return invalid(errors)

  // Steps.
  const steps = root.steps?.kind === 'list' ? root.steps.items : []
  const pre: Array<string> = []
  const after: Array<string> = []
  const names: Array<string> = []
  let built = false
  for (const entry of steps) {
    if (textOf(entry) === 'build') {
      built = true
      continue
    }
    const step = mapOf(entry)
    const run = textOf(step?.run)?.trim()
    if (!step || !run) {
      return invalid([
        'steps entries need `name` and `run`, or the word `build`',
      ])
    }
    const name = (textOf(step.name) ?? '').trim()
    if (step.reports) {
      unsupported.push(
        `steps: '${name}' reports test results (reports.tests), which the form cannot hold`,
      )
    }
    if (run.includes('\n')) {
      unsupported.push(
        `steps: '${name}' runs several lines; the form takes one command per line, so split it into steps or move it into a script`,
      )
    }
    names.push(name)
    ;(built ? after : pre).push(run)
  }
  if (pre[0] === PUB_GET) {
    pre.shift()
  } else {
    unsupported.push(
      `steps: the form always runs \`${PUB_GET}\` before its own commands; keep a \`${PUB_GET}\` step first`,
    )
  }
  const baseBuild = base?.commands.build ?? []
  const keepsBuild =
    baseBuild.length > 0 &&
    baseBuild.every((command, index) => after[index] === command.trim())
  if (baseBuild.length > 0 && !keepsBuild) {
    notes.push(
      "The pipeline's Build commands now run after the target builds, as the file runs them",
    )
  }
  // Names the export gives: after the command, unique among the builds.
  const taken = new Set(platforms.map((p) => `Build ${TITLES[p]}`))
  const derivedNames = [PUB_GET, ...pre, ...after].map((run) => {
    const base = stepName(run)
    let name = base
    for (let n = 2; taken.has(name); n++) name = `${base} (${n})`
    taken.add(name)
    return name
  })
  const renamed = names.flatMap((name, index) => {
    const derived = derivedNames[index] ?? ''
    return name && name !== derived ? [`'${name}' becomes '${derived}'`] : []
  })
  if (renamed.length > 0) {
    notes.push(
      `Step names are not kept: the form names steps after their commands (${renamed.join(', ')})`,
    )
  }

  // Env.
  const env = Object.entries(mapOf(root.env) ?? {}).map(([key, value]) => ({
    key,
    value: textOf(value) ?? '',
  }))
  for (const entry of env) {
    if (entry.value.includes('\n')) {
      unsupported.push(
        `env.${entry.key} spans several lines; the form takes one line per variable`,
      )
    }
  }

  // Triggers.
  let triggerConfig: TriggerConfig | undefined
  if (root.on) {
    const result = demoTriggers(root.on, body.trigger_config ?? undefined)
    if (result.ok) triggerConfig = result.config
    else unsupported.push(`on: ${result.reason}`)
  }

  const response = {
    valid: true,
    errors: [],
    notes,
    google_play: filePlay,
    testflight_group: fileGroup,
  }
  if (unsupported.length > 0) {
    return { ...response, representable: false, unsupported }
  }
  const baseArtifacts = base?.artifact_patterns ?? []
  return {
    ...response,
    representable: true,
    unsupported: [],
    execution_config: {
      platforms,
      flutter_version: textOf(app.flutter) || undefined,
      commands: {
        pre_build: pre,
        build: keepsBuild ? baseBuild : [],
        post_build: keepsBuild ? after.slice(baseBuild.length) : after,
      },
      platform_build_args: args,
      platform_commands: commands,
      env,
      artifact_patterns:
        baseArtifacts.join('\n') === artifacts.join('\n')
          ? baseArtifacts
          : artifacts,
      build_number_offset: Number(textOf(root.build_number_offset) ?? 0) || 0,
      runs_on: strings(root.runs_on).map((label) => label.toLowerCase()),
      isolation: textOf(root.isolation) === 'vm' ? 'vm' : 'host',
    },
    trigger_config: triggerConfig,
  }
}

type TriggerResult =
  | { ok: true; config: TriggerConfig }
  | { ok: false; reason: string }

/** Trigger settings for `on`, or why there are none, as the daemon decides. */
function demoTriggers(
  node: Node,
  base: TriggerConfig | undefined,
): TriggerResult {
  const fail = (reason: string): TriggerResult => ({ ok: false, reason })
  if (!base) {
    return fail(
      'this repository only builds manually, so the form has no trigger settings for it',
    )
  }
  const on = mapOf(node)
  if (!on) return fail('expected a mapping')
  const branchesOf = (value: Node | undefined): Array<string> | null => {
    if (textOf(value) === 'true') return []
    const filter = mapOf(value)
    return filter ? strings(filter.branches) : null
  }
  if (mapOf(on.pull_request)) {
    return fail(
      "on.pull_request.branches matches the branch a pull request targets, while the trigger settings' branch filter matches its source branch",
    )
  }
  const push = branchesOf(on.push)
  const pullRequest = textOf(on.pull_request) === 'true'
  const tag = strings(on.tag)
  const everyTag = tag.length === 1 && tag[0] === '*'
  const events: Array<string> = []
  let branches: Array<string> = []
  let tags: Array<string> = []
  if (pullRequest) {
    if (push && push.length > 0) {
      return fail(
        "with pull requests on, the trigger settings' branch filter would also filter pull requests by their source branch; use `push: true` or turn pull requests off",
      )
    }
    if (tag.length > 0 && !everyTag) {
      return fail(
        "with pull requests on, the trigger settings' tag filter turns off branch events unless branches are listed",
      )
    }
    if (push) events.push('push')
    events.push('pull_request')
    if (everyTag) events.push('tag_push')
  } else if (push && push.length > 0) {
    events.push('push')
    branches = push
    if (tag.length > 0) {
      events.push('tag_push')
      tags = tag
    }
  } else if (push) {
    if (tag.length > 0 && !everyTag) {
      return fail(
        "with `push: true`, the trigger settings' tag filter turns off branch events unless branches are listed, so list the branches under `push.branches`",
      )
    }
    events.push('push')
    if (everyTag) events.push('tag_push')
  } else if (tag.length > 0) {
    events.push('tag_push')
    if (!everyTag) tags = tag
  } else {
    return fail(
      "it starts no webhook builds, which the trigger settings cannot express; remove `on` to keep the pipeline's trigger settings",
    )
  }
  const derived: TriggerConfig = {
    events,
    branches,
    tags,
    paths: push || pullRequest ? strings(on.paths) : [],
    paths_ignore: [],
    schedules: base.schedules ?? [],
  }
  // Settings that already say the same stay as they are.
  const before = onLines(base)
  const now = onLines(derived)
  const same =
    before.ok && now.ok && before.lines.join('\n') === now.lines.join('\n')
  return { ok: true, config: same ? base : derived }
}
