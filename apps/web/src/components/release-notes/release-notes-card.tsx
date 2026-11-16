import { useState } from 'react'
import type {
  ReleaseNotesLocale,
  VersionReleaseNotes,
} from '@oore/client/models'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'
import {
  useDraftReleaseNotes,
  useUpdateReleaseNotes,
} from '@/hooks/use-release-notes'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import { relativeTime } from '@/lib/format-utils'
import { toast } from '@/lib/toast'
import { cn } from '@/lib/utils'

/** Longest release notes Google Play takes per language. */
export const PLAY_NOTES_LIMIT = 500
/** Longest "What to Test" and "What's New" App Store Connect takes. */
export const APPLE_NOTES_LIMIT = 4000

/** Where the notes came from, in one line. */
export function releaseNotesSourceLabel(notes: VersionReleaseNotes) {
  switch (notes.source) {
    case 'repository':
      return `From ${notes.source_path ?? 'the repository'}`
    case 'manual':
      return notes.updated_by
        ? `Written by ${notes.updated_by}`
        : 'Written in Oore'
    default:
      return notes.drafted_since
        ? `Drafted from commits since ${notes.drafted_since}`
        : 'Drafted from commits'
  }
}

/** How the text fits the stores' limits. */
function LimitHint({ text }: { text: string }) {
  const length = text.trim().length
  const overPlay = length > PLAY_NOTES_LIMIT
  return (
    <Text variant="muted" className={cn('text-xs', overPlay && 'text-warning')}>
      {length.toLocaleString()} characters.{' '}
      {overPlay
        ? `Google Play shows the first ${PLAY_NOTES_LIMIT}.`
        : `Google Play takes ${PLAY_NOTES_LIMIT}, Apple ${APPLE_NOTES_LIMIT.toLocaleString()}.`}
    </Text>
  )
}

export function ReleaseNotesDialog({
  projectId,
  versionName,
  notes,
  onOpenChange,
}: {
  projectId: string
  versionName: string
  notes: VersionReleaseNotes | null | undefined
  onOpenChange: (open: boolean) => void
}) {
  const [text, setText] = useState(notes?.text ?? '')
  const [locales, setLocales] = useState<Array<ReleaseNotesLocale>>(
    notes?.locales ?? [],
  )
  const update = useUpdateReleaseNotes(projectId, versionName)
  const draft = useDraftReleaseNotes(projectId, versionName)

  function setLocale(index: number, change: Partial<ReleaseNotesLocale>) {
    setLocales((current) =>
      current.map((locale, i) =>
        i === index ? { ...locale, ...change } : locale,
      ),
    )
  }

  function save() {
    update.mutate(
      {
        text: text.trim(),
        locales: locales
          .map((locale) => ({
            locale: locale.locale.trim(),
            text: locale.text.trim(),
          }))
          .filter((locale) => locale.locale && locale.text),
      },
      {
        onSuccess: () => {
          toast.success(`Release notes for ${versionName} saved`)
          onOpenChange(false)
        },
        onError: (error) => toast.error(getApiErrorMessage(error, {})),
      },
    )
  }

  return (
    <Dialog open onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>Release notes for {versionName}</DialogTitle>
          <DialogDescription>
            Testers see these on the install page and in TestFlight. Google Play
            and the App Store get them when a build of {versionName} is uploaded
            or promoted.
          </DialogDescription>
        </DialogHeader>
        <Stack gap={4}>
          <Stack gap={2}>
            <Row className="justify-between gap-2">
              <Label htmlFor="release-notes-text">en-US</Label>
              <Button
                type="button"
                variant="ghost"
                size="xs"
                disabled={draft.isPending}
                onClick={() =>
                  draft.mutate(undefined, {
                    onSuccess: (response) => {
                      if (response.text) setText(response.text)
                      else toast.message('No commits to draft from yet')
                    },
                    onError: (error) =>
                      toast.error(getApiErrorMessage(error, {})),
                  })
                }
              >
                {draft.isPending ? 'Drafting...' : 'Draft from commits'}
              </Button>
            </Row>
            <Textarea
              id="release-notes-text"
              value={text}
              rows={6}
              maxLength={APPLE_NOTES_LIMIT}
              placeholder="Faster sync and a fix for signing in on slow networks."
              onChange={(event) => setText(event.target.value)}
            />
            <LimitHint text={text} />
          </Stack>
          {locales.map((locale, index) => (
            <Stack key={index} gap={2}>
              <Row className="gap-2">
                <Input
                  aria-label="Language code"
                  value={locale.locale}
                  placeholder="de-DE"
                  className="w-28 font-mono text-xs"
                  onChange={(event) =>
                    setLocale(index, { locale: event.target.value })
                  }
                />
                <Button
                  type="button"
                  variant="ghost"
                  size="xs"
                  onClick={() =>
                    setLocales((current) =>
                      current.filter((_, i) => i !== index),
                    )
                  }
                >
                  Remove
                </Button>
              </Row>
              <Textarea
                aria-label={`Release notes in ${locale.locale || 'this language'}`}
                value={locale.text}
                rows={3}
                maxLength={APPLE_NOTES_LIMIT}
                onChange={(event) =>
                  setLocale(index, { text: event.target.value })
                }
              />
            </Stack>
          ))}
          <div>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() =>
                setLocales((current) => [...current, { locale: '', text: '' }])
              }
            >
              Add a language
            </Button>
          </div>
        </Stack>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button disabled={update.isPending} onClick={save}>
            {update.isPending ? 'Saving...' : 'Save notes'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

/**
 * The release notes of a build's app version: one set per version, shared
 * by every rebuild, sent to testers and the stores.
 */
export function ReleaseNotesCard({
  projectId,
  versionName,
  notes,
  canEdit,
}: {
  projectId: string
  versionName: string | null | undefined
  notes: VersionReleaseNotes | null | undefined
  canEdit: boolean
}) {
  const [editing, setEditing] = useState(false)
  const fromRepository = notes?.source === 'repository'
  const languages = notes?.locales.length ?? 0

  return (
    <Card size="sm">
      <CardHeader>
        <Row className="justify-between gap-2">
          <CardTitle>
            Release notes{versionName ? ` for ${versionName}` : ''}
          </CardTitle>
          {versionName && canEdit && !fromRepository ? (
            <Button
              size="xs"
              variant="outline"
              onClick={() => setEditing(true)}
            >
              {notes?.text ? 'Edit' : 'Write notes'}
            </Button>
          ) : null}
        </Row>
      </CardHeader>
      <CardContent>
        {!versionName ? (
          <Text variant="muted">
            The notes appear once the build reads its app version from
            pubspec.yaml.
          </Text>
        ) : notes?.text ? (
          <Stack gap={2}>
            <p className="text-sm whitespace-pre-line">{notes.text}</p>
            <Text variant="muted" className="text-xs">
              {releaseNotesSourceLabel(notes)}
              {notes.source !== 'repository' && notes.updated_at
                ? `, ${relativeTime(notes.updated_at)}`
                : ''}
              {languages
                ? `. Also in ${languages} other ${languages === 1 ? 'language' : 'languages'}.`
                : '.'}
              {fromRepository
                ? ' Change the file and build again to update them.'
                : ' Every build of this version shares them.'}
            </Text>
          </Stack>
        ) : (
          <Text variant="muted">
            No notes for {versionName} yet. Testers and the stores get whatever
            you write here.
          </Text>
        )}
      </CardContent>
      {editing && versionName ? (
        <ReleaseNotesDialog
          projectId={projectId}
          versionName={versionName}
          notes={notes}
          onOpenChange={setEditing}
        />
      ) : null}
    </Card>
  )
}
