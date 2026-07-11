import type { BuildPlatform, Pipeline } from '@/lib/types'
import type { PipelineFormValues } from '@/lib/pipeline-schema'

export function parseMultiline(raw?: string): Array<string> {
  if (!raw) return []
  return raw
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean)
}

export function parseCsv(raw?: string): Array<string> {
  if (!raw) return []
  return raw
    .split(',')
    .map((value) => value.trim())
    .filter(Boolean)
}

export function parseEnvVars(
  raw?: string,
): Array<{ key: string; value: string }> {
  if (!raw) return []
  return raw
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => {
      const eq = line.indexOf('=')
      if (eq <= 0) {
        return { key: line.trim(), value: '' }
      }
      return {
        key: line.slice(0, eq).trim(),
        value: line.slice(eq + 1).trim(),
      }
    })
}

export function selectedPlatforms(
  data: PipelineFormValues,
): Array<BuildPlatform> {
  const platforms: Array<BuildPlatform> = []
  if (data.platform_android) platforms.push('android')
  if (data.platform_ios) platforms.push('ios')
  if (data.platform_macos) platforms.push('macos')
  return platforms
}

export function applyArgs(base: string, args: Array<string>): string {
  if (args.length === 0) return base
  return `${base} ${args.join(' ')}`
}

export function previewPlatformCommands(
  data: PipelineFormValues,
): Array<string> {
  const platforms = selectedPlatforms(data)
  const commands: Array<string> = []
  for (const platform of platforms) {
    if (platform === 'android') {
      commands.push(
        data.android_command_override?.trim() ||
          applyArgs(
            'flutter build apk --release',
            parseMultiline(data.android_build_args),
          ),
      )
    }
    if (platform === 'ios') {
      commands.push(
        data.ios_command_override?.trim() ||
          applyArgs(
            'flutter build ios --release --no-codesign',
            parseMultiline(data.ios_build_args),
          ),
      )
    }
    if (platform === 'macos') {
      commands.push(
        data.macos_command_override?.trim() ||
          applyArgs(
            'flutter build macos --release',
            parseMultiline(data.macos_build_args),
          ),
      )
    }
  }
  return commands.filter(Boolean)
}

export function defaultArtifactPatterns(
  platforms: Array<BuildPlatform>,
): Array<string> {
  const patterns = new Set<string>()
  if (platforms.includes('android'))
    patterns.add('build/app/outputs/flutter-apk/*.apk')
  if (platforms.includes('ios')) patterns.add('build/ios/ipa/*.ipa')
  if (platforms.includes('macos'))
    patterns.add('build/macos/Build/Products/Release/*.app')
  if (patterns.size === 0) patterns.add('build/app/outputs/flutter-apk/*.apk')
  return [...patterns]
}

export function trimToUndefined(value?: string): string | undefined {
  const trimmed = value?.trim()
  return trimmed ? trimmed : undefined
}

export async function fileToBase64(file: File): Promise<string> {
  const arrayBuffer = await file.arrayBuffer()
  const bytes = new Uint8Array(arrayBuffer)
  let binary = ''
  for (const b of bytes) {
    binary += String.fromCharCode(b)
  }
  return btoa(binary)
}

export function parseBundleIdsInput(raw?: string): Array<string> {
  if (!raw) return []
  const seen = new Set<string>()
  const out: Array<string> = []
  for (const part of raw.split(/[\n,]/g)) {
    const trimmed = part.trim()
    if (!trimmed || seen.has(trimmed)) continue
    seen.add(trimmed)
    out.push(trimmed)
  }
  return out
}

export async function fileToUtf8(file: File): Promise<string> {
  return await file.text()
}

export function toMultiline(values: Array<string>): string {
  return values.join('\n')
}

export function hasCustomFallback(pipeline: Pipeline): boolean {
  const commands = pipeline.execution_config.commands
  if (commands.pre_build.length > 0) return true
  if (commands.build.length > 0) return true
  if (commands.post_build.length > 0) return true
  if ((pipeline.execution_config.env?.length ?? 0) > 0) return true

  const args = pipeline.execution_config.platform_build_args
  if ((args?.android.length ?? 0) > 0) return true
  if ((args?.ios.length ?? 0) > 0) return true
  if ((args?.macos.length ?? 0) > 0) return true

  const overrides = pipeline.execution_config.platform_commands
  if (overrides?.android?.trim()) return true
  if (overrides?.ios?.trim()) return true
  if (overrides?.macos?.trim()) return true

  const defaults = defaultArtifactPatterns(pipeline.execution_config.platforms)
  const current = [...pipeline.execution_config.artifact_patterns].sort()
  const expected = [...defaults].sort()

  return JSON.stringify(current) !== JSON.stringify(expected)
}
