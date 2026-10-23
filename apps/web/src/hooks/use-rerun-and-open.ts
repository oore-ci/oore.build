import { useNavigate } from '@tanstack/react-router'

import { useRerunBuild } from '@/hooks/use-builds'
import { toast } from '@/lib/toast'

/** Re-runs a build, then opens the new one. */
export function useRerunAndOpen() {
  const navigate = useNavigate()
  const mutation = useRerunBuild()

  function rerun(buildId: string) {
    if (mutation.isPending) return
    // A promise, not mutate callbacks: Quick open unmounts as soon as it
    // starts the re-run, and mutate callbacks skip unmounted callers.
    mutation.mutateAsync(buildId).then(
      (result) => {
        toast.success(`Re-run queued as build #${result.build.build_number}`)
        void navigate({
          to: '/builds/$buildId',
          params: { buildId: result.build.id },
        })
      },
      (err: Error) => {
        toast.error(`Failed to re-run: ${err.message}`)
      },
    )
  }

  return { rerun, isPending: mutation.isPending }
}
