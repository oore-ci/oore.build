import { Link, useRouter } from '@tanstack/react-router'
import type { ErrorComponentProps } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  AlertCircleIcon,
  ArrowLeft02Icon,
  Home01Icon,
  RotateClockwiseIcon,
} from '@hugeicons/core-free-icons'

import { Button } from '@/components/ui/button'

export function RootNotFound() {
  return (
    <div className="flex min-h-[60vh] flex-col items-center justify-center gap-6 px-6 text-center">
      <div className="space-y-2">
        <p className="text-6xl font-bold tracking-tight text-muted-foreground/40">
          404
        </p>
        <h1 className="text-2xl font-bold tracking-tight">Page not found</h1>
        <p className="text-sm text-muted-foreground">
          The page you're looking for doesn't exist or has been moved.
        </p>
      </div>
      <Button variant="outline" render={<Link to="/" />} nativeButton={false}>
        <HugeiconsIcon icon={Home01Icon} size={16} />
        Dashboard
      </Button>
    </div>
  )
}

export function RootErrorBoundary({ error, reset }: ErrorComponentProps) {
  const router = useRouter()

  return (
    <div className="flex min-h-[60vh] flex-col items-center justify-center gap-6 px-6 text-center">
      <div className="flex size-12 items-center justify-center border border-destructive/30 bg-destructive/10">
        <HugeiconsIcon
          icon={AlertCircleIcon}
          size={24}
          className="text-destructive"
        />
      </div>
      <div className="space-y-2">
        <h1 className="text-2xl font-bold tracking-tight">
          Something went wrong
        </h1>
        <p className="text-sm text-muted-foreground">
          An unexpected error occurred. Try refreshing or go back.
        </p>
        {import.meta.env.DEV && error instanceof Error ? (
          <pre className="mt-4 max-w-lg overflow-x-auto border bg-muted/50 p-3 text-left font-mono text-xs text-muted-foreground">
            {error.message}
          </pre>
        ) : null}
      </div>
      <div className="flex items-center gap-3">
        <Button
          variant="outline"
          onClick={() => {
            reset()
            void router.invalidate()
          }}
        >
          <HugeiconsIcon icon={RotateClockwiseIcon} size={16} />
          Try again
        </Button>
        <Button variant="outline" onClick={() => window.history.back()}>
          <HugeiconsIcon icon={ArrowLeft02Icon} size={16} />
          Go back
        </Button>
      </div>
    </div>
  )
}
