import { demoApi } from './api'
import { HttpResponse, delay } from 'msw'
import * as z from 'zod'
import { ago } from '../seed'
import { getDemoPersonaFromRequest, getDemoProjectRole } from '../personas'
import { requireDemoProjectPermission } from '../authorization'
import { demoState } from '../state'
import { demoDraftNotes } from '../data/release-notes'
import type { Build, VersionReleaseNotes } from '@oore/client/models'

const updateSchema = z.object({
  text: z.string(),
  locales: z
    .array(z.object({ locale: z.string(), text: z.string() }))
    .default([]),
})

const notFound = (version: string) =>
  HttpResponse.json(
    { error: `No release notes for ${version} yet`, code: 'not_found' },
    { status: 404 },
  )

/** The notes every build of `build`'s app version shares. */
export function demoNotesForBuild(build: Build): VersionReleaseNotes | null {
  if (!build.version_name) return null
  return (
    demoState.releaseNotes.find(
      (notes) =>
        notes.project_id === build.project_id &&
        notes.version_name === build.version_name,
    ) ?? null
  )
}

function findNotes(projectId: string, versionName: string) {
  return demoState.releaseNotes.find(
    (notes) =>
      notes.project_id === projectId && notes.version_name === versionName,
  )
}

/** Commit titles of the version's builds, newest first. */
function versionCommitTitles(projectId: string, versionName: string) {
  return demoState.builds
    .filter(
      (build) =>
        build.project_id === projectId &&
        build.version_name === versionName &&
        build.commit_title,
    )
    .sort((a, b) => b.created_at - a.created_at)
    .map((build) => build.commit_title ?? '')
}

/** The newest other version of the project that has notes. */
function previousVersion(projectId: string, versionName: string) {
  return (
    demoState.releaseNotes
      .filter(
        (notes) =>
          notes.project_id === projectId && notes.version_name !== versionName,
      )
      .sort((a, b) => b.updated_at - a.updated_at)
      .at(0)?.version_name ?? null
  )
}

export const releaseNotesHandlers = [
  demoApi.getReleaseNotes(async ({ params, request }) => {
    await delay(120)
    const projectId = String(params.project_id)
    const versionName = String(params.version_name)
    const persona = getDemoPersonaFromRequest(request)
    if (!getDemoProjectRole(persona, projectId)) return notFound(versionName)
    const notes = findNotes(projectId, versionName)
    return notes ? HttpResponse.json(notes) : notFound(versionName)
  }),

  demoApi.updateReleaseNotes(async ({ params, request }) => {
    await delay(300)
    const projectId = String(params.project_id)
    const versionName = String(params.version_name)
    const forbidden = requireDemoProjectPermission(
      request,
      projectId,
      'artifacts:write',
    )
    if (forbidden) return forbidden
    const existing = findNotes(projectId, versionName)
    if (existing?.source === 'repository') {
      return HttpResponse.json(
        {
          error: `The notes for ${versionName} come from ${existing.source_path ?? 'the repository'}. Change the file and build again.`,
          code: 'notes_in_repository',
        },
        { status: 409 },
      )
    }
    const body = updateSchema.parse(await request.json())
    const persona = getDemoPersonaFromRequest(request)
    const notes: VersionReleaseNotes = {
      project_id: projectId,
      version_name: versionName,
      text: body.text.trim(),
      locales: body.locales,
      source: 'manual',
      updated_at: ago(0),
      updated_by: persona.email,
    }
    demoState.releaseNotes = [
      notes,
      ...demoState.releaseNotes.filter((entry) => entry !== existing),
    ]
    return HttpResponse.json(notes)
  }),

  demoApi.draftReleaseNotes(async ({ params, request }) => {
    await delay(400)
    const projectId = String(params.project_id)
    const versionName = String(params.version_name)
    const persona = getDemoPersonaFromRequest(request)
    if (!getDemoProjectRole(persona, projectId)) return notFound(versionName)
    return HttpResponse.json(
      demoDraftNotes(
        versionCommitTitles(projectId, versionName),
        previousVersion(projectId, versionName),
      ),
    )
  }),
]
