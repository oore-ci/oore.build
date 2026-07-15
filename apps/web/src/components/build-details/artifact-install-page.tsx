import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ArrowLeft01Icon,
  Copy01Icon,
  Globe02Icon,
  InformationCircleIcon,
  SmartPhone01Icon,
} from '@hugeicons/core-free-icons'
import { toast } from 'sonner'
import ReactMarkdown from 'react-markdown'

import {
  artifactInstallReadiness,
  detectInstallDevice,
  getIosAppMetadata,
  selectInstallArtifact,
} from '@/lib/artifact-install'
import {
  useArtifactInstallLink,
  useArtifacts,
  useBuild,
  useProjectArtifacts,
} from '@/hooks/use-builds'
import { useProject } from '@/hooks/use-projects'
import { formatFileSize } from '@/lib/format-utils'
import { qaBuildVersion, qaProjectVersionBase } from '@/lib/qa-releases'
import { PageMeta } from '@/lib/seo'
import { useBreadcrumbLabel } from '@/hooks/use-breadcrumb-label'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import PageLayout from '@/components/page-layout'
import RepositoryAvatar from '@/components/repository-avatar'
import { Skeleton } from '@/components/ui/skeleton'
import { Spinner } from '@/components/ui/spinner'
import { useAuthStore } from '@/stores/auth-store'

function expiryLabel(expiresAt: number | undefined): string {
  if (expiresAt == null) return 'No scheduled expiry'
  return `Available until ${new Intl.DateTimeFormat(undefined, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(new Date(expiresAt * 1000))}`
}

function displayName(name: string): string {
  return name.replace(/\.(apk|ipa)$/i, '')
}

export function ArtifactInstallPage({
  buildId,
  artifactId,
}: {
  buildId: string
  artifactId?: string
}) {
  useBreadcrumbLabel('/builds/$buildId', 'Install')
  const buildQuery = useBuild(buildId)
  const artifactsQuery = useArtifacts(buildId)
  const installMutation = useArtifactInstallLink()
  const isQaViewer = useAuthStore((state) => state.user?.role === 'qa_viewer')
  const projectQuery = useProject(
    isQaViewer ? (buildQuery.data?.build.project_id ?? '') : '',
  )
  const projectArtifactsQuery = useProjectArtifacts(
    buildQuery.data?.build.project_id ?? '',
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

  if (
    buildQuery.isLoading ||
    artifactsQuery.isLoading ||
    (isQaViewer &&
      (projectQuery.isLoading || projectArtifactsQuery.isLoading))
  ) {
    return (
      <PageLayout width="narrow">
        <PageMeta title="Install artifact" noindex />
        <Skeleton className="h-8 w-56" />
        <Skeleton className="h-72 w-full" />
      </PageLayout>
    )
  }

  const queryError = buildQuery.error ?? artifactsQuery.error
  const qaQueryError = isQaViewer
    ? (projectQuery.error ?? projectArtifactsQuery.error)
    : null
  if (queryError || qaQueryError) {
    return (
      <PageLayout width="narrow">
        <PageMeta title="Install artifact" noindex />
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} />
          <AlertDescription>
            Failed to load this artifact:{' '}
            {(queryError ?? qaQueryError)?.message}
          </AlertDescription>
        </Alert>
      </PageLayout>
    )
  }

  if (!artifact || !buildQuery.data) {
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

  const { build } = buildQuery.data
  const project = projectQuery.data?.project
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
  const appName = isQaViewer
    ? qaBuildVersion(
        build,
        artifactsQuery.data?.artifacts ?? [],
        qaProjectVersionBase(projectArtifactsQuery.data?.artifacts ?? []),
      )
    : (iosApp?.displayName ?? displayName(artifact.name))
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

  function handleCopyPageLink() {
    void navigator.clipboard.writeText(window.location.href).then(
      () => toast.success('Install page link copied'),
      () => toast.error('Failed to copy install page link'),
    )
  }

  const primaryLabel = isIos
    ? 'Install'
    : device === 'other'
      ? 'Download APK'
      : 'Install'

  return (
    <PageLayout width="narrow" className="px-4 pt-4 pb-28 sm:px-6 sm:py-10">
      <PageMeta
        title={`Install ${project ? `${project.name} ${appName}` : appName}`}
        noindex
      />

      <Button
        variant="ghost"
        size="sm"
        render={
          isQaViewer ? (
            <Link to="/" resetScroll />
          ) : (
            <Link
              to="/builds/$buildId"
              params={{ buildId }}
              search={{}}
              resetScroll
            />
          )
        }
        nativeButton={false}
        className="hidden w-fit sm:inline-flex"
      >
        <HugeiconsIcon icon={ArrowLeft01Icon} />
        {isQaViewer ? 'Back to apps' : `Build #${build.build_number}`}
      </Button>

      <section className="flex flex-col gap-5 pt-2 sm:pt-6">
        <header>
          {isQaViewer && project ? (
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
          ) : null}
          <Badge variant={isIos ? 'success' : 'info'}>
            {isIos ? 'iOS' : 'Android'}
          </Badge>
          {isQaViewer ? (
            <p className="mt-3 break-words text-2xl font-bold tracking-tight sm:text-3xl">
              {appName}
            </p>
          ) : (
            <h1 className="mt-3 break-words text-2xl font-bold tracking-tight sm:text-3xl">
              {appName}
            </h1>
          )}
          <p className="mt-2 text-sm text-muted-foreground">
            {artifact.file_size != null
              ? formatFileSize(artifact.file_size)
              : 'Size unavailable'}
            {artifact.expires_at != null
              ? ` · ${expiryLabel(artifact.expires_at)}`
              : ''}
          </p>
          {!isQaViewer && iosApp ? (
            <p className="mt-1 text-xs text-muted-foreground">
              {iosApp.version}+{iosApp.buildNumber}
            </p>
          ) : null}
        </header>

        {isQaViewer && build.changelog ? (
          <section>
            <h2 className="text-sm font-medium">What’s new</h2>
            <ReactMarkdown
              skipHtml
              components={{
                a: ({ children, ...props }) => (
                  <a
                    {...props}
                    target="_blank"
                    rel="noreferrer"
                    className="underline underline-offset-4"
                  >
                    {children}
                  </a>
                ),
                ol: ({ children }) => (
                  <ol className="mt-2 flex list-decimal flex-col gap-1 pl-5 text-sm text-muted-foreground">
                    {children}
                  </ol>
                ),
                p: ({ children }) => (
                  <p className="mt-2 whitespace-pre-wrap text-sm text-muted-foreground">
                    {children}
                  </p>
                ),
                ul: ({ children }) => (
                  <ul className="mt-2 flex list-disc flex-col gap-1 pl-5 text-sm text-muted-foreground">
                    {children}
                  </ul>
                ),
              }}
            >
              {build.changelog}
            </ReactMarkdown>
          </section>
        ) : null}

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

        <div className="fixed inset-x-0 bottom-0 z-[60] border-t bg-background/95 px-4 pt-3 pb-[calc(0.75rem+env(safe-area-inset-bottom))] backdrop-blur sm:static sm:border-0 sm:bg-transparent sm:p-0 sm:backdrop-blur-none">
          <div className="flex gap-2">
            <Button
              variant="outline"
              size="icon-lg"
              render={
                isQaViewer ? (
                  <Link to="/" resetScroll />
                ) : (
                  <Link
                    to="/builds/$buildId"
                    params={{ buildId }}
                    search={{}}
                    resetScroll
                  />
                )
              }
              nativeButton={false}
              aria-label={isQaViewer ? 'Back to apps' : 'Back to build'}
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
          {!isQaViewer ? (
            <Button
              variant="ghost"
              onClick={handleCopyPageLink}
              className="mt-2 w-full"
            >
              <HugeiconsIcon icon={Copy01Icon} />
              Copy install page link
            </Button>
          ) : null}
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
