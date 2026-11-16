import {
  PIPELINE_OWN_FILES,
  usesOwnApiKey,
  type PipelineFormValues,
} from '@/lib/pipeline-schema'
import type {
  AndroidSigningProfile,
  AndroidSigningProfileInput,
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

/**
 * One Android build type's pick: a library identity signs it, the files the
 * pipeline still holds keep signing it, or nothing does. Undefined when
 * nothing changed.
 */
function androidProfileInput(
  picked: string | undefined,
  previous: AndroidSigningProfile | undefined,
): AndroidSigningProfileInput | undefined {
  const identity = picked ?? ''
  const enabled = identity !== ''
  const ownFiles = identity === PIPELINE_OWN_FILES
  if (previous) {
    const unchanged =
      enabled === previous.enabled &&
      (ownFiles || identity === (previous.identity_id ?? ''))
    if (unchanged) return undefined
  } else if (!enabled) {
    return undefined
  }
  // Own files keep the stored choice; turning signing off keeps the pick
  // too, so turning it back on needs no new pick.
  return {
    enabled,
    identity_id: ownFiles || !enabled ? undefined : identity,
  }
}

/**
 * Android signing picks a library identity per build type. The files
 * argument is kept for callers from before the library; uploads now go to
 * the library.
 */
export async function buildAndroidSigningPayload(
  values: PipelineFormValues,
  _files?: AndroidSigningFiles,
  previous?: PipelineAndroidSigningResponse,
): Promise<SigningResult<UpdatePipelineAndroidSigningRequest>> {
  await Promise.resolve()
  if (!values.platform_android) return { payload: null, errors: [] }
  const release = androidProfileInput(
    values.android_signing_release_identity_id,
    previous?.release,
  )
  const debug = androidProfileInput(
    values.android_signing_debug_identity_id,
    previous?.debug,
  )
  if (!release && !debug) return { payload: null, errors: [] }
  return { errors: [], payload: { release, debug } }
}

/** The picker value for a stored Android profile. */
export function androidIdentityValue(profile?: AndroidSigningProfile): string {
  if (!profile?.enabled) return profile?.identity_id ?? ''
  if (profile.identity_id) return profile.identity_id
  return profile.has_keystore ? PIPELINE_OWN_FILES : ''
}

/** The picker value for stored iOS signing. */
export function iosIdentityValue(signing?: PipelineIosSigningResponse): string {
  if (signing?.identity_id) return signing.identity_id
  return signing?.mode === 'manual' && signing.enabled && signing.has_p12
    ? PIPELINE_OWN_FILES
    : ''
}

/**
 * Whether saving `payload` makes the daemon discard the stored provisioning
 * profiles (the distribution changed) while App Store Connect can generate
 * replacements (API or hybrid mode). The caller then runs a profile sync, so
 * the next build does not fail on a pipeline with no profiles.
 */
export function iosProfilesNeedResync(
  payload: UpdatePipelineIosSigningRequest,
  previous?: PipelineIosSigningResponse,
): boolean {
  if (!previous || !payload.enabled) return false
  if (payload.mode !== 'api' && payload.mode !== 'hybrid') return false
  return (
    payload.export_method !== undefined &&
    payload.export_method !== previous.export_method
  )
}

/**
 * Manual mode signs with a library identity, which sets the team, export
 * method and bundle IDs on the daemon. Files the pipeline still holds keep
 * their stored team and bundles.
 */
function manualIosSigningPayload(
  values: PipelineFormValues,
  previous?: PipelineIosSigningResponse,
): SigningResult<UpdatePipelineIosSigningRequest> {
  const identity = values.ios_signing_identity_id ?? ''
  const ownFiles = identity === PIPELINE_OWN_FILES
  const enabled = values.ios_signing_enabled
  const changed = previous
    ? enabled !== previous.enabled ||
      previous.mode !== 'manual' ||
      (!ownFiles && identity !== (previous.identity_id ?? ''))
    : enabled
  if (!changed) return { payload: null, errors: [] }
  if (enabled && !identity) {
    return { payload: null, errors: ['Pick an iOS signing identity'] }
  }
  return {
    errors: [],
    payload: {
      enabled,
      mode: 'manual',
      identity_id: ownFiles || !identity ? undefined : identity,
      // The daemon replaces these with the identity's.
      export_method: ownFiles ? previous?.export_method : undefined,
      team_id: ownFiles ? (previous?.team_id ?? undefined) : undefined,
      bundle_ids: ownFiles || !identity ? (previous?.bundle_ids ?? []) : [],
      provisioning_profiles: [],
    },
  }
}

export async function buildIosSigningPayload(
  values: PipelineFormValues,
  files: IosSigningFiles,
  previous?: PipelineIosSigningResponse,
): Promise<SigningResult<UpdatePipelineIosSigningRequest>> {
  if (!values.platform_ios) return { payload: null, errors: [] }
  if (values.ios_signing_mode === 'manual') {
    return manualIosSigningPayload(values, previous)
  }

  const bundleIds = parseBundleIdsInput(values.ios_signing_bundle_ids)
  const teamId = trimToUndefined(values.ios_signing_team_id)
  const p12Password = trimToUndefined(values.ios_signing_p12_password)
  const apiKeyId = trimToUndefined(values.ios_signing_api_key_id)
  const apiIssuerId = trimToUndefined(values.ios_signing_api_issuer_id)
  const usesApi =
    values.ios_signing_mode === 'api' || values.ios_signing_mode === 'hybrid'
  const apiKeySource = values.ios_signing_api_key_source || undefined
  const ownApiKey = usesOwnApiKey(apiKeySource)
  // Only API and hybrid mode use the key, so manual mode keeps the stored choice.
  const sharedApiKeyId =
    usesApi && apiKeySource
      ? apiKeySource === 'own'
        ? ''
        : apiKeySource
      : undefined
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
      (sharedApiKeyId !== undefined &&
        sharedApiKeyId !== (previous.shared_api_key?.id ?? '')) ||
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
    values.ios_signing_enabled && values.ios_signing_mode === 'hybrid'
  if (needsManual) {
    if (!files.p12File && !previous?.has_p12) {
      errors.push('Hybrid iOS signing requires a .p12 certificate')
    }
    if (!p12Password && !previous?.has_p12_password) {
      errors.push('Hybrid iOS signing requires p12 password')
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
          `Hybrid iOS signing requires provisioning profile for ${bundleId}`,
        )
      }
    }
  }

  const needsApi = values.ios_signing_enabled && usesApi && ownApiKey
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
    files.apiKeyFile && ownApiKey ? files.apiKeyFile.text() : undefined,
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
      shared_api_key_id: sharedApiKeyId,
      api_credentials:
        ownApiKey && (apiKeyId || apiIssuerId || apiPrivateKey)
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
