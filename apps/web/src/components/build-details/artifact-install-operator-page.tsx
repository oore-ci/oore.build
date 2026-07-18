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
  getIosAppMetadata,
} from '@/lib/artifact-install'
import type { InstallDevice } from '@/lib/artifact-install'
import { useArtifactInstallLink } from '@/hooks/use-builds'
import { formatFileSize } from '@/lib/format-utils'
import { PageMeta } from '@/lib/seo'
import type { Artifact, Build } from '@/lib/types'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import PageLayout from '@/components/page-layout'
import { Spinner } from '@/components/ui/spinner'

const expiryFormatter = new Intl.DateTimeFormat(undefined, {
  dateStyle: 'medium',
  timeStyle: 'short',
})

function expiryLabel(expiresAt: number): string {
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

export default function OperatorArtifactInstallPage({
  artifact,
  build,
  buildId,
  device,
}: {
  artifact: Artifact
  build: Build
  buildId: string
  device: InstallDevice
}) {
  const installMutation = useArtifactInstallLink()
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
    installMutation.mutate(artifact.id, {
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
