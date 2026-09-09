import { Suspense, lazy } from 'react'
import { useNavigate } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  AndroidIcon,
  AppleIcon,
  ArrowDown01Icon,
  CheckmarkCircle02Icon,
  Clock01Icon,
  SmartPhone01Icon,
} from '@hugeicons/core-free-icons'
import { toast } from '@/lib/toast'

import {
  artifactInstallReadiness,
  detectInstallDevice,
  selectInstallArtifact,
} from '@/lib/artifact-install'
import {
  isTerminalStatus,
  useArtifactInstallLink,
  useArtifacts,
  useBuild,
  useProjectArtifacts,
} from '@/hooks/use-builds'
import { useProject } from '@/hooks/use-projects'
import {
  formatDuration,
  formatFileSize,
  relativeTime,
} from '@/lib/format-utils'
import { qaBuildVersion, qaProjectVersionBase } from '@/lib/qa-releases'
import { PageMeta } from '@/lib/seo'
import { getStatusVariant } from '@/lib/status-variants'
import type { Project } from '@oore/client/models'
import type { Artifact, Build } from '@oore/client/models'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import {
  Item,
  ItemActions,
  ItemContent,
  ItemGroup,
  ItemMedia,
  ItemTitle,
} from '@/components/ui/item'
import PageLayout from '@/components/page-layout'
import RepositoryAvatar from '@/components/repository-avatar'
import { Skeleton } from '@/components/ui/skeleton'
import { Spinner } from '@/components/ui/spinner'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { useAuthStore } from '@/stores/auth-store'
import { useTime } from '@/hooks/use-time'

const ChangelogMarkdown = lazy(() => import('./changelog-markdown'))
const QaBuildLogs = lazy(() => import('./qa-build-logs'))
const QaArtifactUnavailableAlert = lazy(
  () => import('./qa-artifact-unavailable-alert'),
)
const QaInstallReadinessAlerts = lazy(
  () => import('./qa-install-readiness-alerts'),
)
const OperatorArtifactInstallPage = lazy(
  () => import('./artifact-install-operator-page'),
)
const expiryFormatter = new Intl.DateTimeFormat(undefined, {
  dateStyle: 'medium',
  timeStyle: 'short',
})

function expiryLabel(expiresAt: number | undefined): string {
  if (expiresAt == null) return 'No scheduled expiry'
  return `Available until ${expiryFormatter.format(new Date(expiresAt * 1000))}`
}

function ArtifactInstallLoading() {
  return (
    <PageLayout width="narrow">
      <PageMeta title="Install artifact" noindex />
      <Skeleton className="h-8 w-56" />
      <Skeleton className="h-72 w-full" />
    </PageLayout>
  )
}

function ArtifactInstallError({
  message,
  onRetry,
}: {
  message: string
  onRetry: () => void
}) {
  return (
    <PageLayout width="narrow">
      <PageMeta title="Install artifact" noindex />
      <Alert variant="destructive">
        <AlertDescription>
          Failed to load this build: {message}
        </AlertDescription>
        <Button variant="outline" size="sm" onClick={onRetry}>
          Retry
        </Button>
      </Alert>
    </PageLayout>
  )
}

function QaReleaseDetail({
  artifact,
  artifacts,
  build,
  historyArtifacts,
  historyError,
  historyLoading,
  project,
}: {
  artifact?: Artifact
  artifacts: Array<Artifact>
  build: Build
  historyArtifacts: Array<Artifact>
  historyError: boolean
  historyLoading: boolean
  project?: Project
}) {
  const time = useTime()
  const navigate = useNavigate()
  const installMutation = useArtifactInstallLink()
  const device = detectInstallDevice(globalThis.navigator?.userAgent ?? '')
  const readiness = artifact ? artifactInstallReadiness(artifact) : null
  const isIos = artifact?.artifact_type === 'ipa'
  const isAndroid = artifact?.artifact_type === 'apk'
  const expired =
    artifact?.expires_at != null &&
    artifact.expires_at <= Math.floor(time / 1000)
  const wrongPhone =
    (isIos && device === 'android') ||
    (isAndroid && (device === 'iphone-safari' || device === 'iphone-other'))
  const needsSafari = isIos && device === 'iphone-other'
  const isDesktopIos = isIos && device === 'other'
  const canInstall =
    !!artifact &&
    readiness?.ready === true &&
    !expired &&
    !wrongPhone &&
    !needsSafari &&
    !isDesktopIos
  const hasInstallGuidance =
    (readiness != null && !readiness.ready) ||
    expired ||
    wrongPhone ||
    needsSafari ||
    isDesktopIos
  const appName = qaBuildVersion(
    build,
    artifacts,
    qaProjectVersionBase(historyArtifacts),
  )

  function handleInstall() {
    if (!artifact) return
    installMutation.mutate(artifact.id, {
      onSuccess: (response) => window.location.assign(response.install_url),
      onError: (error) =>
        toast.error(`Could not start installation: ${error.message}`),
    })
  }

  const primaryLabel = isIos
    ? 'Install'
    : device === 'other'
      ? 'Download APK'
      : 'Install'
  const projectName = project?.name ?? build.context?.project_name
  const releasedAt = build.finished_at ?? build.created_at
  const androidArtifact = artifacts.find(
    (candidate) => candidate.artifact_type === 'apk',
  )
  const iosArtifact = artifacts.find(
    (candidate) => candidate.artifact_type === 'ipa',
  )
  const steps = build.step_results ?? []
  const passedSteps = steps.filter((step) => step.status === 'succeeded').length

  return (
    <PageLayout
      width="default"
      className={artifact ? 'px-4 pt-4 pb-28 sm:px-6 sm:py-8' : undefined}
    >
      <PageMeta
        title={`${projectName ? `${projectName} · ` : ''}${appName}`}
        noindex
      />

      <header>
        <div className="flex items-center gap-2">
          {project ? (
            <RepositoryAvatar
              fullName={project.repository_full_name ?? project.name}
              avatarUrl={project.repository_avatar_url}
              repositoryId={project.repository_id}
              provider={project.repository_provider}
              size="sm"
            />
          ) : null}
          <p className="truncate text-sm text-muted-foreground">
            {projectName ?? 'Test release'}
          </p>
        </div>
        <h1 className="mt-2 text-2xl font-semibold tracking-tight wrap-break-word sm:text-3xl">
          {appName}
        </h1>
        <div className="mt-2 flex flex-wrap items-center gap-2">
          <Badge variant={getStatusVariant(build.status)}>
            {build.status === 'succeeded' ? 'Ready to test' : build.status}
          </Badge>
          {artifact ? (
            <Badge variant="outline">{isIos ? 'iOS' : 'Android'}</Badge>
          ) : null}
          <span className="text-xs text-muted-foreground">
            Released {relativeTime(releasedAt)}
          </span>
        </div>
      </header>

      {(androidArtifact && iosArtifact) ||
      (artifact && artifact.id !== androidArtifact?.id) ? (
        <ToggleGroup
          aria-label="Choose platform"
          value={artifact ? [artifact.id] : []}
          variant="outline"
          size="sm"
          onValueChange={(value) => {
            const artifactId = value.at(0)
            if (!artifactId) return
            void navigate({
              to: '/builds/$buildId',
              params: { buildId: build.id },
              search: { install: artifactId },
              resetScroll: true,
            })
          }}
        >
          {androidArtifact ? (
            <ToggleGroupItem value={androidArtifact.id}>
              <HugeiconsIcon icon={AndroidIcon} data-icon="inline-start" />
              Android
              {isAndroid ? (
                <HugeiconsIcon
                  icon={CheckmarkCircle02Icon}
                  data-icon="inline-end"
                />
              ) : null}
            </ToggleGroupItem>
          ) : null}
          {iosArtifact ? (
            <ToggleGroupItem value={iosArtifact.id}>
              <HugeiconsIcon icon={AppleIcon} data-icon="inline-start" />
              iOS
              {isIos ? (
                <HugeiconsIcon
                  icon={CheckmarkCircle02Icon}
                  data-icon="inline-end"
                />
              ) : null}
            </ToggleGroupItem>
          ) : null}
        </ToggleGroup>
      ) : null}

      <div className="grid items-start gap-8 lg:grid-cols-[minmax(0,1fr)_18rem]">
        <div className="min-w-0 space-y-6">
          {build.changelog ? (
            <section aria-labelledby="release-notes-title">
              <h2 id="release-notes-title" className="text-sm font-medium">
                What changed
              </h2>
              <Suspense fallback={<Skeleton className="mt-2 h-16 w-full" />}>
                <ChangelogMarkdown>{build.changelog}</ChangelogMarkdown>
              </Suspense>
            </section>
          ) : (
            <section aria-labelledby="release-notes-title">
              <h2 id="release-notes-title" className="text-sm font-medium">
                What changed
              </h2>
              <p className="mt-2 text-sm text-muted-foreground">
                No release notes were provided for this version.
              </p>
            </section>
          )}

          {!artifact ? (
            <Suspense
              fallback={
                <Alert>
                  <AlertDescription>
                    Checking artifact availability
                  </AlertDescription>
                </Alert>
              }
            >
              <QaArtifactUnavailableAlert buildStatus={build.status} />
            </Suspense>
          ) : null}

          {hasInstallGuidance ? (
            <Suspense
              fallback={
                <Alert>
                  <AlertDescription>Loading install guidance</AlertDescription>
                </Alert>
              }
            >
              <QaInstallReadinessAlerts
                conditions={{
                  desktopIos: isDesktopIos,
                  expired,
                  needsSafari,
                  wrongPhone,
                }}
                platform={isIos ? 'iOS' : 'Android'}
                readiness={readiness}
              />
            </Suspense>
          ) : null}

          {steps.length > 0 ? (
            <Collapsible className="border-y">
              <CollapsibleTrigger className="flex min-h-11 w-full items-center gap-3 py-2 text-left text-sm outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50">
                <span className="font-medium">Build checks</span>
                <span className="text-muted-foreground">
                  {passedSteps} of {steps.length} completed
                </span>
                <HugeiconsIcon
                  icon={ArrowDown01Icon}
                  className="ml-auto size-4 text-muted-foreground transition-transform in-data-open:rotate-180"
                  aria-hidden
                />
              </CollapsibleTrigger>
              <CollapsibleContent className="pb-3">
                <ItemGroup className="gap-1">
                  {steps.map((step) => (
                    <Item key={step.name} size="xs">
                      <ItemMedia variant="icon">
                        <HugeiconsIcon
                          icon={
                            step.status === 'succeeded'
                              ? CheckmarkCircle02Icon
                              : Clock01Icon
                          }
                        />
                      </ItemMedia>
                      <ItemContent>
                        <ItemTitle>{step.name}</ItemTitle>
                      </ItemContent>
                      <ItemActions className="text-xs text-muted-foreground">
                        {step.duration_ms != null
                          ? formatDuration(step.duration_ms / 1000)
                          : step.status}
                      </ItemActions>
                    </Item>
                  ))}
                </ItemGroup>
              </CollapsibleContent>
            </Collapsible>
          ) : null}

          <Collapsible>
            <CollapsibleTrigger className="flex min-h-11 w-full items-center gap-3 text-left text-sm outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50">
              <span className="font-medium">Build diagnostics</span>
              <span className="text-muted-foreground">
                Logs for troubleshooting
              </span>
              <HugeiconsIcon
                icon={ArrowDown01Icon}
                className="ml-auto size-4 text-muted-foreground transition-transform in-data-open:rotate-180"
                aria-hidden
              />
            </CollapsibleTrigger>
            <CollapsibleContent className="pt-3">
              <Suspense fallback={<Skeleton className="h-96 w-full" />}>
                <QaBuildLogs build={build} />
              </Suspense>
            </CollapsibleContent>
          </Collapsible>

          {historyError ? (
            <p className="text-xs text-muted-foreground">
              Historical version context is temporarily unavailable. This does
              not affect installation.
            </p>
          ) : null}
        </div>

        {artifact ? (
          <Card className="lg:sticky lg:top-20">
            <CardHeader>
              <CardTitle>Install this release</CardTitle>
              <CardDescription>
                {isIos ? 'iOS' : 'Android'} ·{' '}
                {artifact.file_size != null
                  ? formatFileSize(artifact.file_size ?? undefined)
                  : 'Size unavailable'}
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4 pb-(--card-spacing) sm:pb-0">
              <p className="text-xs text-muted-foreground">
                {expiryLabel(artifact.expires_at ?? undefined)}
              </p>
              <div className="space-y-1.5 text-sm">
                <p className="font-medium">Before you install</p>
                <p className="text-muted-foreground">
                  {isIos
                    ? 'Open this page in Safari on a registered iPhone, then confirm the iOS install prompt.'
                    : 'Download the APK, allow installs from this browser if asked, then confirm installation.'}
                </p>
              </div>
            </CardContent>
            <CardFooter className="hidden sm:flex">
              <Button
                size="lg"
                onClick={handleInstall}
                disabled={!canInstall || installMutation.isPending}
                className="w-full"
              >
                {installMutation.isPending ? (
                  <Spinner />
                ) : (
                  <HugeiconsIcon icon={SmartPhone01Icon} />
                )}
                {primaryLabel}
              </Button>
            </CardFooter>
          </Card>
        ) : historyLoading ? (
          <Skeleton className="h-52 w-full" />
        ) : null}
      </div>

      {artifact ? (
        <div className="fixed inset-x-0 bottom-0 z-40 border-t bg-background/95 px-4 pt-3 pb-[calc(0.75rem+env(safe-area-inset-bottom))] backdrop-blur sm:hidden">
          <Button
            size="lg"
            onClick={handleInstall}
            disabled={!canInstall || installMutation.isPending}
            className="min-h-11 w-full"
          >
            {installMutation.isPending ? (
              <Spinner />
            ) : (
              <HugeiconsIcon icon={SmartPhone01Icon} />
            )}
            {primaryLabel}
          </Button>
        </div>
      ) : null}
    </PageLayout>
  )
}

export function ArtifactInstallPage({
  buildId,
  artifactId,
}: {
  buildId: string
  artifactId?: string
}) {
  const buildQuery = useBuild(buildId, {
    refetchInterval: (query) =>
      query.state.data && isTerminalStatus(query.state.data.build.status)
        ? false
        : 3000,
  })
  const isTerminal = buildQuery.data
    ? isTerminalStatus(buildQuery.data.build.status)
    : false
  const artifactsQuery = useArtifacts(buildId, {
    refetchInterval: isTerminal ? false : 3000,
  })
  const isQaViewer = useAuthStore((state) => state.user?.role === 'qa_viewer')
  const qaProjectId = isQaViewer
    ? (buildQuery.data?.build.project_id ?? '')
    : ''
  const projectQuery = useProject(qaProjectId)
  const projectArtifactsQuery = useProjectArtifacts(qaProjectId)
  const artifacts = artifactsQuery.data?.artifacts ?? []
  const device = detectInstallDevice(globalThis.navigator?.userAgent ?? '')
  const artifact = isQaViewer
    ? selectInstallArtifact(artifacts, device, artifactId)
    : artifacts.find((candidate) => candidate.id === artifactId)

  if (buildQuery.isLoading || artifactsQuery.isLoading) {
    return <ArtifactInstallLoading />
  }

  const queryError = buildQuery.error ?? artifactsQuery.error
  if (queryError) {
    return (
      <ArtifactInstallError
        message={queryError.message}
        onRetry={() => {
          void buildQuery.refetch()
          void artifactsQuery.refetch()
        }}
      />
    )
  }

  if (!buildQuery.data) return null

  const { build } = buildQuery.data

  if (isQaViewer) {
    return (
      <QaReleaseDetail
        artifact={artifact}
        artifacts={artifacts}
        build={build}
        historyArtifacts={projectArtifactsQuery.data?.artifacts ?? []}
        historyError={projectArtifactsQuery.isError}
        historyLoading={projectArtifactsQuery.isLoading}
        project={projectQuery.data?.project}
      />
    )
  }

  if (!artifact) {
    return (
      <PageLayout width="narrow">
        <PageMeta title="Artifact unavailable" noindex />
        <Alert variant="destructive">
          <AlertDescription>
            This artifact is unavailable or has been removed.
          </AlertDescription>
        </Alert>
      </PageLayout>
    )
  }

  return (
    <Suspense fallback={<ArtifactInstallLoading />}>
      <OperatorArtifactInstallPage
        artifact={artifact}
        build={build}
        buildId={buildId}
        device={device}
      />
    </Suspense>
  )
}
