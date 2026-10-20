import type { PipelineFormValues } from '@/lib/pipeline-schema'
import type {
  PipelineAndroidSigningResponse,
  PipelineIosSigningResponse,
  UpdatePipelineAndroidSigningRequest,
  UpdatePipelineIosSigningRequest,
} from '@oore/client/models'
import {
  fileToBase64,
  parseBundleIdsInput,
  trimToUndefined,
} from '@/lib/pipeline-form-utils'

export interface AndroidSigningFiles {
  release: File | null
  debug: File | null
}

export interface IosSigningFiles {
  p12File: File | null
  apiKeyFile: File | null
  profileFiles: Record<string, File | null>
}

interface SigningResult<T> {
  payload: T | null
  errors: Array<string>
}

function profileHasInput(
  enabled: boolean,
  file: File | null,
  alias?: string,
  storePassword?: string,
  keyPassword?: string,
): boolean {
  return enabled || !!file || !!alias || !!storePassword || !!keyPassword
}

export async function buildAndroidSigningPayload(
  values: PipelineFormValues,
  files: AndroidSigningFiles,
  previous?: PipelineAndroidSigningResponse,
): Promise<SigningResult<UpdatePipelineAndroidSigningRequest>> {
  if (!values.platform_android) return { payload: null, errors: [] }

  const release = {
    enabled: values.android_signing_release_enabled,
    alias: trimToUndefined(values.android_signing_release_key_alias),
    storePassword: trimToUndefined(
      values.android_signing_release_store_password,
    ),
    keyPassword: trimToUndefined(values.android_signing_release_key_password),
  }
  const debug = {
    enabled: values.android_signing_debug_enabled,
    alias: trimToUndefined(values.android_signing_debug_key_alias),
    storePassword: trimToUndefined(values.android_signing_debug_store_password),
    keyPassword: trimToUndefined(values.android_signing_debug_key_password),
  }

  const releaseHasInput = profileHasInput(
    release.enabled,
    files.release,
    release.alias,
    release.storePassword,
    release.keyPassword,
  )
  const debugHasInput = profileHasInput(
    debug.enabled,
    files.debug,
    debug.alias,
    debug.storePassword,
    debug.keyPassword,
  )
  if (!previous && !releaseHasInput && !debugHasInput) {
    return { payload: null, errors: [] }
  }

  const errors: Array<string> = []
  if (release.enabled) {
    if (!files.release && !previous?.release.has_keystore) {
      errors.push(
        'Release signing is enabled but no release keystore is configured',
      )
    }
    if (!release.alias) errors.push('Release signing key alias is required')
    if (!release.storePassword && !previous?.release.has_store_password) {
      errors.push('Release store password is required')
    }
    if (!release.keyPassword && !previous?.release.has_key_password) {
      errors.push('Release key password is required')
    }
  }
  if (debug.enabled) {
    if (!files.debug && !previous?.debug.has_keystore) {
      errors.push(
        'Debug signing is enabled but no debug keystore is configured',
      )
    }
    if (!debug.alias) errors.push('Debug signing key alias is required')
    if (!debug.storePassword && !previous?.debug.has_store_password) {
      errors.push('Debug store password is required')
    }
    if (!debug.keyPassword && !previous?.debug.has_key_password) {
      errors.push('Debug key password is required')
    }
  }
  if (errors.length > 0) return { payload: null, errors }

  const releaseTouched = previous
    ? release.enabled !== previous.release.enabled ||
      !!files.release ||
      release.alias !== trimToUndefined(previous.release.key_alias) ||
      !!release.storePassword ||
      !!release.keyPassword
    : releaseHasInput
  const debugTouched = previous
    ? debug.enabled !== previous.debug.enabled ||
      !!files.debug ||
      debug.alias !== trimToUndefined(previous.debug.key_alias) ||
      !!debug.storePassword ||
      !!debug.keyPassword
    : debugHasInput
  if (!releaseTouched && !debugTouched) return { payload: null, errors: [] }

  return {
    errors: [],
    payload: {
      release: releaseTouched
        ? {
            enabled: release.enabled,
            keystore_filename: files.release?.name,
            keystore_base64: files.release
              ? await fileToBase64(files.release)
              : undefined,
            key_alias: release.alias,
            store_password: release.storePassword,
            key_password: release.keyPassword,
          }
        : undefined,
      debug: debugTouched
        ? {
            enabled: debug.enabled,
            keystore_filename: files.debug?.name,
            keystore_base64: files.debug
              ? await fileToBase64(files.debug)
              : undefined,
            key_alias: debug.alias,
            store_password: debug.storePassword,
            key_password: debug.keyPassword,
          }
        : undefined,
    },
  }
}

export async function buildIosSigningPayload(
  values: PipelineFormValues,
  files: IosSigningFiles,
  previous?: PipelineIosSigningResponse,
): Promise<SigningResult<UpdatePipelineIosSigningRequest>> {
  if (!values.platform_ios) return { payload: null, errors: [] }

  const bundleIds = parseBundleIdsInput(values.ios_signing_bundle_ids)
  const teamId = trimToUndefined(values.ios_signing_team_id)
  const p12Password = trimToUndefined(values.ios_signing_p12_password)
  const apiKeyId = trimToUndefined(values.ios_signing_api_key_id)
  const apiIssuerId = trimToUndefined(values.ios_signing_api_issuer_id)
  const hasFiles =
    !!files.p12File ||
    !!files.apiKeyFile ||
    Object.values(files.profileFiles).some(Boolean)
  const changed = previous
    ? values.ios_signing_enabled !== previous.enabled ||
      values.ios_signing_mode !== previous.mode ||
      values.ios_signing_export_method !== previous.export_method ||
      bundleIds.join(',') !== previous.bundle_ids.join(',') ||
      teamId !== trimToUndefined(previous.team_id) ||
      apiKeyId !== trimToUndefined(previous.api_key_id) ||
      apiIssuerId !== trimToUndefined(previous.api_issuer_id) ||
      !!p12Password ||
      hasFiles
    : values.ios_signing_enabled ||
      values.ios_signing_export_method !== 'ad_hoc' ||
      bundleIds.length > 0 ||
      !!teamId ||
      !!p12Password ||
      !!apiKeyId ||
      !!apiIssuerId ||
      hasFiles
  if (!changed) return { payload: null, errors: [] }

  const errors: Array<string> = []
  if (values.ios_signing_enabled) {
    if (!teamId) errors.push('iOS signing requires Team ID')
    if (bundleIds.length === 0) {
      errors.push('iOS signing requires at least one bundle identifier')
    }
  }

  const needsManual =
    values.ios_signing_enabled &&
    (values.ios_signing_mode === 'manual' ||
      values.ios_signing_mode === 'hybrid')
  if (needsManual) {
    if (!files.p12File && !previous?.has_p12) {
      errors.push('Manual/Hybrid iOS signing requires a .p12 certificate')
    }
    if (!p12Password && !previous?.has_p12_password) {
      errors.push('Manual/Hybrid iOS signing requires p12 password')
    }
    // The daemon discards stored profiles when the export method changes.
    const keepsStoredProfiles =
      values.ios_signing_export_method === previous?.export_method
    for (const bundleId of bundleIds) {
      const hasStoredProfile =
        keepsStoredProfiles &&
        previous?.provisioning_profiles.some(
          (profile) => profile.bundle_id === bundleId && profile.has_profile,
        )
      if (!files.profileFiles[bundleId] && !hasStoredProfile) {
        errors.push(
          `Manual/Hybrid iOS signing requires provisioning profile for ${bundleId}`,
        )
      }
    }
  }

  const needsApi =
    values.ios_signing_enabled &&
    (values.ios_signing_mode === 'api' || values.ios_signing_mode === 'hybrid')
  if (needsApi) {
    if (!apiKeyId && !previous?.api_key_id) {
      errors.push('API/Hybrid iOS signing requires API key ID')
    }
    if (!apiIssuerId && !previous?.api_issuer_id) {
      errors.push('API/Hybrid iOS signing requires API issuer ID')
    }
    if (!files.apiKeyFile && !previous?.has_api_key) {
      errors.push('API/Hybrid iOS signing requires a .p8 private key file')
    }
  }
  if (errors.length > 0) return { payload: null, errors }

  const [provisioningProfiles, apiPrivateKey, p12Base64] = await Promise.all([
    Promise.all(
      bundleIds.flatMap((bundleId) => {
        const file = files.profileFiles[bundleId]
        return file
          ? [
              fileToBase64(file).then((profileBase64) => ({
                bundle_id: bundleId,
                profile_filename: file.name,
                profile_base64: profileBase64,
              })),
            ]
          : []
      }),
    ),
    files.apiKeyFile ? files.apiKeyFile.text() : undefined,
    files.p12File ? fileToBase64(files.p12File) : undefined,
  ])

  return {
    errors: [],
    payload: {
      enabled: values.ios_signing_enabled,
      mode: values.ios_signing_mode,
      export_method: values.ios_signing_export_method,
      team_id: teamId,
      bundle_ids: bundleIds,
      certificate:
        files.p12File || p12Password
          ? {
              p12_filename: files.p12File?.name,
              p12_base64: p12Base64,
              p12_password: p12Password,
            }
          : undefined,
      provisioning_profiles: provisioningProfiles,
      api_credentials:
        apiKeyId || apiIssuerId || apiPrivateKey
          ? {
              key_id: apiKeyId ?? previous?.api_key_id,
              issuer_id: apiIssuerId ?? previous?.api_issuer_id,
              private_key_base64: apiPrivateKey
                ? btoa(apiPrivateKey)
                : undefined,
            }
          : undefined,
    },
  }
}
