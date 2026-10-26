import { useRouter } from '@tanstack/react-router'
import { toast } from '@/lib/toast'

import type { Artifact } from '@oore/client/models'

/** Copies the in-app install page link for one of a build's apps. */
export function useCopyInstallLink() {
  const router = useRouter()

  return (buildId: string, artifact: Artifact, label: string) => {
    const { href } = router.buildLocation({
      to: '/builds/$buildId',
      params: { buildId },
      search: { install: artifact.id },
    })
    void navigator.clipboard
      .writeText(new URL(href, window.location.origin).href)
      .then(
        () => toast.success(`${label} install link copied`),
        () => toast.error('Could not copy link. Try again.'),
      )
  }
}
