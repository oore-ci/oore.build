import { Suspense, lazy, useState } from 'react'
import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ArrowLeft01Icon,
  Copy01Icon,
  Globe02Icon,
  InformationCircleIcon,
  SmartPhone01Icon,
} from '@hugeicons/core-free-icons'
import { toast } from '@/lib/toast'

import {
  artifactInstallReadiness,
  detectInstallDevice,
  getIosAppMetadata,
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
import { formatFileSize } from '@/lib/format-utils'
import { qaBuildVersion, qaProjectVersionBase } from '@/lib/qa-releases'
import { PageMeta } from '@/lib/seo'
import { getStatusVariant } from '@/lib/status-variants'
import type { Artifact, Build, Project } from '@/lib/types'
import { useBreadcrumbLabel } from '@/hooks/use-breadcrumb-label'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import PageLayout from '@/components/page-layout'
import RepositoryAvatar from '@/components/repository-avatar'
import { Skeleton } from '@/components/ui/skeleton'
import { Spinner } from '@/components/ui/spinner'
import { useAuthStore } from '@/stores/auth-store'

const ChangelogMarkdown = lazy(() => import('./changelog-markdown'))
const QaBuildLogs = lazy(() => import('./qa-build-logs'))
const expiryFormatter = new Intl.DateTimeFormat(undefined, {
  dateStyle: 'medium',
  timeStyle: 'short',
})

function expiryLabel(expiresAt: number | undefined): string {
  if (expiresAt == null) return 'No scheduled expiry'
  return `Available until ${expiryFormatter.format(new Date(expiresAt * 1000))}`
}

function displayName(name: string): string {
  return name.replace(/\.(apk|ipa)$/i, '')
}

function copyPageLink() {
  void navigator.clipboard.writeText(window.location.href).then(
    () => toast.success('Install page link copied'),
    () => toast.error('Failed to copy install page link'),
  )
}

function QaInstallReadinessAlerts({
  conditions,
  platform,
  readiness,
}: {
  conditions: {
    desktopIos: boolean
    expired: boolean
    needsSafari: boolean
    wrongPhone: boolean
  }
  platform: 'iOS' | 'Android'
  readiness: ReturnType<typeof artifactInstallReadiness> | null
}) {
  return (
    <>
      {readiness && !readiness.ready ? (
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} />
          <AlertTitle>Not install-ready</AlertTitle>
          <AlertDescription>{readiness.reason}</AlertDescription>
        </Alert>
      ) : null}
      {conditions.expired ? (
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} />
          <AlertTitle>Artifact expired</AlertTitle>
          <AlertDescription>
            Ask a developer to run a fresh build before installing.
          </AlertDescription>
        </Alert>
      ) : null}
      {conditions.needsSafari ? (
        <Alert>
          <HugeiconsIcon icon={Globe02Icon} />
          <AlertTitle>Open this page in Safari</AlertTitle>
          <AlertDescription>
            iOS installation can only start from Safari on this iPhone.
          </AlertDescription>
        </Alert>
      ) : null}
      {conditions.desktopIos ? (
        <Alert>
          <HugeiconsIcon icon={SmartPhone01Icon} />
          <AlertTitle>Open this page on the registered iPhone</AlertTitle>
          <AlertDescription>
            Use Safari on a device included in this version’s provisioning
            profile.
          </AlertDescription>
        </Alert>
      ) : null}
      {conditions.wrongPhone ? (
        <Alert>
          <HugeiconsIcon icon={InformationCircleIcon} />
          <AlertTitle>Open this page on the right device</AlertTitle>
          <AlertDescription>This version is for {platform}.</AlertDescription>
        </Alert>
      ) : null}
    </>
  )
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
        <HugeiconsIcon icon={InformationCircleIcon} />
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
  const [section, setSection] = useState<'release' | 'logs'>('release')
  const installMutation = useArtifactInstallLink()
  const device = detectInstallDevice(
    typeof navigator === 'undefined' ? '' : navigator.userAgent,
  )
  const readiness = artifact ? artifactInstallReadiness(artifact) : null
  const isIos = artifact?.artifact_type === 'ipa'
  const isAndroid = artifact?.artifact_type === 'apk'
  const expired =
    artifact?.expires_at != null &&
    artifact.expires_at <= Math.floor(Date.now() / 1000)
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

  return (
    <PageLayout
      width="narrow"
      className={artifact ? 'px-4 pt-4 pb-28 sm:px-6 sm:py-10' : undefined}
    >
      <PageMeta
        title={`${projectName ? `${projectName} · ` : ''}${appName}`}
        noindex
      />

      <Button
        variant="ghost"
        size="sm"
        render={<Link to="/" resetScroll />}
        nativeButton={false}
        className="hidden w-fit sm:inline-flex"
      >
        <HugeiconsIcon icon={ArrowLeft01Icon} />
        Back to apps
      </Button>

      <div
        role="tablist"
        aria-label="Release details"
        className="flex gap-1 border-b pt-2 sm:pt-6"
      >
        <Button
          role="tab"
          aria-selected={section === 'release'}
          aria-controls="qa-release-panel"
          variant="ghost"
          className={section === 'release' ? 'border-b-2 border-b-primary' : ''}
          onClick={() => setSection('release')}
        >
          Release
        </Button>
        <Button
          role="tab"
          aria-selected={section === 'logs'}
          aria-controls="qa-logs-panel"
          variant="ghost"
          className={section === 'logs' ? 'border-b-2 border-b-primary' : ''}
          onMouseEnter={() => void import('./qa-build-logs')}
          onFocus={() => void import('./qa-build-logs')}
          onClick={() => setSection('logs')}
        >
          Logs
        </Button>
      </div>

      {section === 'release' ? (
        <div
          id="qa-release-panel"
          role="tabpanel"
          className="flex flex-col gap-5"
        >
          <header>
            {project ? (
              <div className="mb-6 flex items-center gap-3">
                <RepositoryAvatar
                  fullName={project.repository_full_name ?? project.name}
                  avatarUrl={project.repository_avatar_url}
                  repositoryId={project.repository_id}
                  provider={project.repository_provider}
                  size="lg"
                />
                <h1 className="truncate text-xl font-semibold tracking-tight">
                  {project.name}
                </h1>
              </div>
            ) : projectName ? (
              <h1 className="mb-6 truncate text-xl font-semibold tracking-tight">
                {projectName}
              </h1>
            ) : null}
            <div className="flex flex-wrap items-center gap-2">
              <Badge variant={getStatusVariant(build.status)}>
                {build.status.replaceAll('_', ' ')}
              </Badge>
              {artifact ? (
                <Badge variant="outline">{isIos ? 'iOS' : 'Android'}</Badge>
              ) : null}
            </div>
            <p className="mt-3 wrap-break-word text-2xl font-bold tracking-tight sm:text-3xl">
              {appName}
            </p>
            {artifact ? (
              <p className="mt-2 text-sm text-muted-foreground">
                {artifact.file_size != null
                  ? formatFileSize(artifact.file_size)
                  : 'Size unavailable'}
                {artifact.expires_at != null
                  ? ` · ${expiryLabel(artifact.expires_at)}`
                  : ''}
              </p>
            ) : historyLoading ? (
              <Skeleton className="mt-2 h-4 w-36" />
            ) : null}
          </header>

          {build.changelog ? (
            <section>
              <h2 className="text-sm font-medium">What’s new</h2>
              <Suspense fallback={<Skeleton className="mt-2 h-16 w-full" />}>
                <ChangelogMarkdown>{build.changelog}</ChangelogMarkdown>
              </Suspense>
            </section>
          ) : (
            <p className="text-sm text-muted-foreground">
              No changelog was provided for this build.
            </p>
          )}

          {!artifact ? (
            <Alert
              variant={build.status === 'failed' ? 'destructive' : 'default'}
            >
              <HugeiconsIcon icon={InformationCircleIcon} />
              <AlertTitle>
                {build.status === 'failed'
                  ? 'Build failed'
                  : isTerminalStatus(build.status)
                    ? 'No installable artifact'
                    : 'Build is still in progress'}
              </AlertTitle>
              <AlertDescription>
                {build.status === 'failed'
                  ? 'Open Logs for diagnostic output from this build.'
                  : isTerminalStatus(build.status)
                    ? 'This build finished without an installable APK or IPA.'
                    : 'Installation becomes available here when a signed APK or IPA is ready.'}
              </AlertDescription>
            </Alert>
          ) : null}

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

          {artifact ? (
            <>
              <section className="flex flex-col gap-4 pt-1">
                <h2 className="text-sm font-medium">Before you install</h2>
                {isIos ? (
                  <ol className="flex flex-col gap-3 text-sm text-muted-foreground">
                    <li>01 · Use Safari on the registered iPhone.</li>
                    <li>02 · Tap Install and confirm the iOS prompt.</li>
                    <li>
                      03 · Enable Developer Mode if iOS asks. The device must be
                      in this version’s provisioning profile.
                    </li>
                  </ol>
                ) : (
                  <ol className="flex flex-col gap-3 text-sm text-muted-foreground">
                    <li>01 · Tap Install to download the APK.</li>
                    <li>
                      02 · Allow this browser to install unknown apps if asked.
                    </li>
                    <li>03 · Open the APK and confirm installation.</li>
                  </ol>
                )}
              </section>
              <div className="fixed inset-x-0 bottom-0 z-60 border-t bg-background/95 px-4 pt-3 pb-[calc(0.75rem+env(safe-area-inset-bottom))] backdrop-blur sm:static sm:border-0 sm:bg-transparent sm:p-0 sm:backdrop-blur-none">
                <div className="flex gap-2">
                  <Button
                    variant="outline"
                    size="icon-lg"
                    render={<Link to="/" resetScroll />}
                    nativeButton={false}
                    aria-label="Back to apps"
                    className="min-h-11 sm:hidden"
                  >
                    <HugeiconsIcon icon={ArrowLeft01Icon} />
                  </Button>
                  <Button
                    size="lg"
                    onClick={handleInstall}
                    disabled={!canInstall || installMutation.isPending}
                    className="min-h-11 min-w-0 flex-1 sm:w-full"
                  >
                    {installMutation.isPending ? (
                      <Spinner />
                    ) : (
                      <HugeiconsIcon icon={SmartPhone01Icon} />
                    )}
                    {primaryLabel}
                  </Button>
                </div>
              </div>
            </>
          ) : null}

          {historyError ? (
            <p className="text-xs text-muted-foreground">
              Historical version context is temporarily unavailable. This does
              not affect installation.
            </p>
          ) : null}
        </div>
      ) : (
        <div id="qa-logs-panel" role="tabpanel" className="min-h-96">
          <Suspense fallback={<Skeleton className="h-96 w-full" />}>
            <QaBuildLogs build={build} />
          </Suspense>
        </div>
      )}
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
  useBreadcrumbLabel('/builds/$buildId', 'Install')
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
  const installMutation = useArtifactInstallLink()
  const isQaViewer = useAuthStore((state) => state.user?.role === 'qa_viewer')
  const projectQuery = useProject(
    isQaViewer ? (buildQuery.data?.build.project_id ?? '') : '',
  )
  const projectArtifactsQuery = useProjectArtifacts(
    isQaViewer ? (buildQuery.data?.build.project_id ?? '') : '',
  )
  const device = detectInstallDevice(
    typeof navigator === 'undefined' ? '' : navigator.userAgent,
  )
  const artifact = isQaViewer
    ? selectInstallArtifact(
        artifactsQuery.data?.artifacts ?? [],
        device,
        artifactId,
      )
    : artifactsQuery.data?.artifacts.find(
        (candidate) => candidate.id === artifactId,
      )

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
        artifacts={artifactsQuery.data?.artifacts ?? []}
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
          <HugeiconsIcon icon={InformationCircleIcon} />
          <AlertDescription>
            This artifact is unavailable or has been removed.
          </AlertDescription>
        </Alert>
      </PageLayout>
    )
  }

  const readiness = artifactInstallReadiness(artifact)
  const iosApp = getIosAppMetadata(artifact)
  const isIos = artifact.artifact_type === 'ipa'
  const isAndroid = artifact.artifact_type === 'apk'
  const expired =
    artifact.expires_at != null &&
    artifact.expires_at <= Math.floor(Date.now() / 1000)
  const wrongPhone =
    (isIos && device === 'android') ||
    (isAndroid && (device === 'iphone-safari' || device === 'iphone-other'))
  const needsSafari = isIos && device === 'iphone-other'
  const isDesktopIos = isIos && device === 'other'
  const canInstall =
    readiness.ready && !expired && !wrongPhone && !needsSafari && !isDesktopIos
  const appName = iosApp?.displayName ?? displayName(artifact.name)
  const selectedArtifactId = artifact.id

  function handleInstall() {
    installMutation.mutate(selectedArtifactId, {
      onSuccess: (response) => {
        window.location.assign(response.install_url)
      },
      onError: (error) => {
        toast.error(`Could not start installation: ${error.message}`)
      },
    })
  }

  const primaryLabel = isIos
    ? 'Install'
    : device === 'other'
      ? 'Download APK'
      : 'Install'

  return (
    <PageLayout width="narrow" className="px-4 pt-4 pb-28 sm:px-6 sm:py-10">
      <PageMeta title={`Install ${appName}`} noindex />

      <Button
        variant="ghost"
        size="sm"
        render={
          <Link
            to="/builds/$buildId"
            params={{ buildId }}
            search={{}}
            resetScroll
          />
        }
        nativeButton={false}
        className="hidden w-fit sm:inline-flex"
      >
        <HugeiconsIcon icon={ArrowLeft01Icon} />
        Build #{build.build_number}
      </Button>

      <section className="flex flex-col gap-5 pt-2 sm:pt-6">
        <header>
          <Badge variant="outline">{isIos ? 'iOS' : 'Android'}</Badge>
          <h1 className="mt-3 wrap-break-word text-2xl font-bold tracking-tight sm:text-3xl">
            {appName}
          </h1>
          <p className="mt-2 text-sm text-muted-foreground">
            {artifact.file_size != null
              ? formatFileSize(artifact.file_size)
              : 'Size unavailable'}
            {artifact.expires_at != null
              ? ` · ${expiryLabel(artifact.expires_at)}`
              : ''}
          </p>
          {iosApp ? (
            <p className="mt-1 text-xs text-muted-foreground">
              {iosApp.version}+{iosApp.buildNumber}
            </p>
          ) : null}
        </header>

        {!readiness.ready ? (
          <Alert variant="destructive">
            <HugeiconsIcon icon={InformationCircleIcon} />
            <AlertTitle>Not install-ready</AlertTitle>
            <AlertDescription>{readiness.reason}</AlertDescription>
          </Alert>
        ) : null}

        {expired ? (
          <Alert variant="destructive">
            <HugeiconsIcon icon={InformationCircleIcon} />
            <AlertTitle>Artifact expired</AlertTitle>
            <AlertDescription>
              Ask a developer to run a fresh build before installing.
            </AlertDescription>
          </Alert>
        ) : null}

        {needsSafari ? (
          <Alert>
            <HugeiconsIcon icon={Globe02Icon} />
            <AlertTitle>Open this page in Safari</AlertTitle>
            <AlertDescription>
              iOS installation can only start from Safari on this iPhone.
            </AlertDescription>
          </Alert>
        ) : null}

        {isDesktopIos ? (
          <Alert>
            <HugeiconsIcon icon={SmartPhone01Icon} />
            <AlertTitle>Open this page on the registered iPhone</AlertTitle>
            <AlertDescription>
              Use Safari on a device included in this version’s provisioning
              profile.
            </AlertDescription>
          </Alert>
        ) : null}

        {wrongPhone ? (
          <Alert>
            <HugeiconsIcon icon={InformationCircleIcon} />
            <AlertTitle>Open this page on the right device</AlertTitle>
            <AlertDescription>
              This version is for {isIos ? 'iOS' : 'Android'}.
            </AlertDescription>
          </Alert>
        ) : null}

        <div className="fixed inset-x-0 bottom-0 z-60 border-t bg-background/95 px-4 pt-3 pb-[calc(0.75rem+env(safe-area-inset-bottom))] backdrop-blur sm:static sm:border-0 sm:bg-transparent sm:p-0 sm:backdrop-blur-none">
          <div className="flex gap-2">
            <Button
              variant="outline"
              size="icon-lg"
              render={
                <Link
                  to="/builds/$buildId"
                  params={{ buildId }}
                  search={{}}
                  resetScroll
                />
              }
              nativeButton={false}
              aria-label="Back to build"
              className="min-h-11 sm:hidden"
            >
              <HugeiconsIcon icon={ArrowLeft01Icon} />
            </Button>
            <Button
              size="lg"
              onClick={handleInstall}
              disabled={!canInstall || installMutation.isPending}
              className="min-h-11 min-w-0 flex-1 sm:w-full"
            >
              {installMutation.isPending ? (
                <Spinner />
              ) : (
                <HugeiconsIcon icon={SmartPhone01Icon} />
              )}
              {primaryLabel}
            </Button>
          </div>
          <Button
            variant="ghost"
            onClick={copyPageLink}
            className="mt-2 w-full"
          >
            <HugeiconsIcon icon={Copy01Icon} />
            Copy install page link
          </Button>
        </div>
      </section>

      <section className="flex flex-col gap-4 pt-1">
        <h2 className="text-sm font-medium">Before you install</h2>
        {isIos ? (
          <ol className="flex flex-col gap-3 text-sm text-muted-foreground">
            <li className="flex gap-3">
              <span className="font-mono text-foreground">01</span>
              <span>Use Safari on the registered iPhone.</span>
            </li>
            <li className="flex gap-3">
              <span className="font-mono text-foreground">02</span>
              <span>Tap Install and confirm the iOS prompt.</span>
            </li>
            <li className="flex gap-3">
              <span className="font-mono text-foreground">03</span>
              <span>
                Enable Developer Mode if iOS asks. The device must be included
                in this version’s provisioning profile.
              </span>
            </li>
          </ol>
        ) : (
          <ol className="flex flex-col gap-3 text-sm text-muted-foreground">
            <li className="flex gap-3">
              <span className="font-mono text-foreground">01</span>
              <span>Tap Install to download the APK.</span>
            </li>
            <li className="flex gap-3">
              <span className="font-mono text-foreground">02</span>
              <span>Allow this browser to install unknown apps if asked.</span>
            </li>
            <li className="flex gap-3">
              <span className="font-mono text-foreground">03</span>
              <span>Open the APK and confirm installation.</span>
            </li>
          </ol>
        )}
      </section>
    </PageLayout>
  )
}
