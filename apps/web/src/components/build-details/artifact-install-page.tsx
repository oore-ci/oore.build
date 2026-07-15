import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ArrowLeft01Icon,
  Copy01Icon,
  Download04Icon,
  Globe02Icon,
  InformationCircleIcon,
  SmartPhone01Icon,
} from '@hugeicons/core-free-icons'
import { toast } from 'sonner'

import {
  artifactInstallReadiness,
  detectInstallDevice,
  getIosAppMetadata,
} from '@/lib/artifact-install'
import {
  useArtifactDownloadLink,
  useArtifactInstallLink,
  useArtifacts,
  useBuild,
} from '@/hooks/use-builds'
import { formatFileSize } from '@/lib/format-utils'
import { PageMeta } from '@/lib/seo'
import { useBreadcrumbLabel } from '@/hooks/use-breadcrumb-label'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import PageLayout from '@/components/page-layout'
import { Skeleton } from '@/components/ui/skeleton'
import { Spinner } from '@/components/ui/spinner'

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
  artifactId: string
}) {
  useBreadcrumbLabel('/builds/$buildId', 'Install')
  const buildQuery = useBuild(buildId)
  const artifactsQuery = useArtifacts(buildId)
  const installMutation = useArtifactInstallLink()
  const downloadMutation = useArtifactDownloadLink()
  const artifact = artifactsQuery.data?.artifacts.find(
    (candidate) => candidate.id === artifactId,
  )
  const device = detectInstallDevice(
    typeof navigator === 'undefined' ? '' : navigator.userAgent,
  )

  if (buildQuery.isLoading || artifactsQuery.isLoading) {
    return (
      <PageLayout width="narrow">
        <PageMeta title="Install artifact" noindex />
        <Skeleton className="h-8 w-56" />
        <Skeleton className="h-72 w-full" />
      </PageLayout>
    )
  }

  const queryError = buildQuery.error ?? artifactsQuery.error
  if (queryError) {
    return (
      <PageLayout width="narrow">
        <PageMeta title="Install artifact" noindex />
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} />
          <AlertDescription>
            Failed to load this artifact: {queryError.message}
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

  function handleInstall() {
    installMutation.mutate(artifactId, {
      onSuccess: (response) => {
        window.location.assign(response.install_url)
      },
      onError: (error) => {
        toast.error(`Could not start installation: ${error.message}`)
      },
    })
  }

  function handleDownload() {
    downloadMutation.mutate(artifactId, {
      onSuccess: (response) => {
        window.open(response.download_url, '_blank', 'noopener,noreferrer')
      },
      onError: (error) => {
        toast.error(`Could not download artifact: ${error.message}`)
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
    ? 'Install on iPhone'
    : device === 'other'
      ? 'Download APK'
      : 'Install APK'

  return (
    <PageLayout width="narrow" className="py-6 sm:py-10">
      <PageMeta title={`Install ${appName}`} noindex />

      <Button
        variant="ghost"
        size="sm"
        render={<Link to="/builds/$buildId" params={{ buildId }} search={{}} />}
        nativeButton={false}
        className="w-fit"
      >
        <HugeiconsIcon icon={ArrowLeft01Icon} />
        Build #{build.build_number}
      </Button>

      <Card>
        <CardHeader className="border-b">
          <div className="flex items-start gap-4">
            <div className="flex size-12 shrink-0 items-center justify-center border bg-muted">
              <HugeiconsIcon icon={SmartPhone01Icon} size={24} />
            </div>
            <div className="min-w-0 flex-1">
              <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                Device installation
              </p>
              <h1 className="mt-1 truncate text-2xl font-bold tracking-tight">
                {appName}
              </h1>
              <div className="mt-2 flex flex-wrap items-center gap-2">
                <Badge variant={isIos ? 'success' : 'info'}>
                  {isIos ? 'iOS' : 'Android'}
                </Badge>
                {iosApp ? (
                  <span className="text-xs text-muted-foreground">
                    Version {iosApp.version} ({iosApp.buildNumber})
                  </span>
                ) : null}
              </div>
            </div>
          </div>
        </CardHeader>
        <CardContent className="space-y-5">
          <dl className="grid grid-cols-2 gap-x-4 gap-y-3 text-xs">
            <div>
              <dt className="text-muted-foreground">Build</dt>
              <dd className="mt-0.5 font-medium">#{build.build_number}</dd>
            </div>
            <div>
              <dt className="text-muted-foreground">Size</dt>
              <dd className="mt-0.5 font-medium">
                {artifact.file_size != null
                  ? formatFileSize(artifact.file_size)
                  : '—'}
              </dd>
            </div>
            <div className="col-span-2">
              <dt className="text-muted-foreground">Availability</dt>
              <dd className="mt-0.5 font-medium">
                {expiryLabel(artifact.expires_at)}
              </dd>
            </div>
            {build.commit_sha ? (
              <div className="col-span-2">
                <dt className="text-muted-foreground">Commit</dt>
                <dd className="mt-0.5 font-mono font-medium">
                  {build.commit_sha.slice(0, 12)}
                </dd>
              </div>
            ) : null}
          </dl>

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
                iOS app installation can only start from Safari. Copy this page
                link, then paste it into Safari on this iPhone.
              </AlertDescription>
            </Alert>
          ) : null}

          {isDesktopIos ? (
            <Alert>
              <HugeiconsIcon icon={SmartPhone01Icon} />
              <AlertTitle>Continue on your iPhone</AlertTitle>
              <AlertDescription>
                Open this page in Safari on an iPhone included in the app’s
                provisioning profile.
              </AlertDescription>
            </Alert>
          ) : null}

          {wrongPhone ? (
            <Alert>
              <HugeiconsIcon icon={InformationCircleIcon} />
              <AlertTitle>Different platform required</AlertTitle>
              <AlertDescription>
                Open this page on {isIos ? 'an iPhone' : 'an Android phone'} to
                install this build.
              </AlertDescription>
            </Alert>
          ) : null}

          <div className="grid gap-2 sm:grid-cols-2">
            <Button
              size="lg"
              onClick={handleInstall}
              disabled={!canInstall || installMutation.isPending}
              className="sm:col-span-2"
            >
              {installMutation.isPending ? (
                <Spinner />
              ) : (
                <HugeiconsIcon icon={SmartPhone01Icon} />
              )}
              {primaryLabel}
            </Button>
            <Button variant="outline" onClick={handleCopyPageLink}>
              <HugeiconsIcon icon={Copy01Icon} />
              Copy page link
            </Button>
            <Button
              variant="outline"
              onClick={handleDownload}
              disabled={expired || downloadMutation.isPending}
            >
              {downloadMutation.isPending ? (
                <Spinner />
              ) : (
                <HugeiconsIcon icon={Download04Icon} />
              )}
              Download file
            </Button>
          </div>
        </CardContent>
      </Card>

      <Card size="sm">
        <CardHeader>
          <CardTitle className="text-sm uppercase tracking-wider text-muted-foreground">
            Before you install
          </CardTitle>
        </CardHeader>
        <CardContent>
          {isIos ? (
            <ol className="space-y-3 text-sm text-muted-foreground">
              <li className="flex gap-3">
                <span className="font-mono text-foreground">01</span>
                <span>Use Safari on the registered iPhone.</span>
              </li>
              <li className="flex gap-3">
                <span className="font-mono text-foreground">02</span>
                <span>
                  Tap Install and confirm the iOS installation prompt.
                </span>
              </li>
              <li className="flex gap-3">
                <span className="font-mono text-foreground">03</span>
                <span>
                  Before opening the app, enable Developer Mode if iOS asks. The
                  phone’s UDID must be in this build’s provisioning profile.
                </span>
              </li>
            </ol>
          ) : (
            <ol className="space-y-3 text-sm text-muted-foreground">
              <li className="flex gap-3">
                <span className="font-mono text-foreground">01</span>
                <span>Tap Install APK to download the build.</span>
              </li>
              <li className="flex gap-3">
                <span className="font-mono text-foreground">02</span>
                <span>
                  Allow this browser to install unknown apps if Android asks.
                </span>
              </li>
              <li className="flex gap-3">
                <span className="font-mono text-foreground">03</span>
                <span>Open the downloaded APK and confirm installation.</span>
              </li>
            </ol>
          )}
        </CardContent>
      </Card>
    </PageLayout>
  )
}
