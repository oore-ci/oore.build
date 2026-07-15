import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { Logout03Icon } from '@hugeicons/core-free-icons'

import { Button } from '@/components/ui/button'
import { useLogout } from '@/hooks/use-auth'
import { useAuthStore } from '@/stores/auth-store'

export default function QaAppHeader() {
  const user = useAuthStore((state) => state.user)
  const logoutMutation = useLogout()

  return (
    <header className="sticky top-0 z-30 border-b bg-background pt-[var(--safe-area-top)]">
      <div className="mx-auto flex h-16 w-full max-w-6xl items-center gap-3 px-4 sm:px-6 lg:px-10">
        <Link to="/" className="flex min-w-0 items-center gap-3">
          <img src="/logo.svg" alt="Oore CI" className="size-8" />
          <span className="font-semibold tracking-tight">Oore</span>
        </Link>

        <div className="ml-auto flex min-w-0 items-center gap-3">
          <span className="hidden max-w-64 truncate text-xs text-muted-foreground md:block">
            {user?.email}
          </span>
          <Button
            variant="ghost"
            aria-label="Sign out"
            onClick={() => logoutMutation.mutate()}
            disabled={logoutMutation.isPending}
          >
            <HugeiconsIcon icon={Logout03Icon} />
            <span className="hidden sm:inline">Sign out</span>
          </Button>
        </div>
      </div>
    </header>
  )
}
