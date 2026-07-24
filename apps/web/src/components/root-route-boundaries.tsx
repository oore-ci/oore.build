import { Link, useRouter } from '@tanstack/react-router'
import type { ErrorComponentProps } from '@tanstack/react-router'
import {
  CircleAlert as AlertCircleIcon,
  ArrowLeft as ArrowLeft02Icon,
  House as Home01Icon,
  RotateCw as RotateClockwiseIcon,
} from 'lucide-react'

import { Button } from '@/components/ui/button'
import { Alert, AlertDescription } from '@/components/ui/alert'
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'

export function RootNotFound() {
  return (
    <Empty className="min-h-[60vh]">
      <EmptyHeader>
        <EmptyMedia>
          <span className="text-5xl font-bold tracking-tight text-muted-foreground/40">
            404
          </span>
        </EmptyMedia>
        <EmptyTitle>Page not found</EmptyTitle>
        <EmptyDescription>
          The page you're looking for doesn't exist or has been moved.
        </EmptyDescription>
      </EmptyHeader>
      <EmptyContent>
        <Button variant="outline" render={<Link to="/" />} nativeButton={false}>
          <Home01Icon />
          Dashboard
        </Button>
      </EmptyContent>
    </Empty>
  )
}

export function RootErrorBoundary({ error, reset }: ErrorComponentProps) {
  const router = useRouter()

  return (
    <Empty className="min-h-[60vh]">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <AlertCircleIcon className="text-destructive" />
        </EmptyMedia>
        <EmptyTitle>Something went wrong</EmptyTitle>
        <EmptyDescription>
          An unexpected error occurred. Try refreshing or go back.
        </EmptyDescription>
      </EmptyHeader>
      <EmptyContent>
        {import.meta.env.DEV && error instanceof Error ? (
          <Alert>
            <AlertDescription className="max-w-lg overflow-x-auto text-left font-mono text-xs">
              {error.message}
            </AlertDescription>
          </Alert>
        ) : null}
        <div className="flex items-center gap-3">
          <Button
            variant="outline"
            onClick={() => {
              reset()
              void router.invalidate()
            }}
          >
            <RotateClockwiseIcon />
            Try again
          </Button>
          <Button variant="outline" onClick={() => window.history.back()}>
            <ArrowLeft02Icon />
            Go back
          </Button>
        </div>
      </EmptyContent>
    </Empty>
  )
}
