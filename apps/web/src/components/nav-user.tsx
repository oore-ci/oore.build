import { Suspense, lazy, useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { ChevronsUpDown } from '@hugeicons/core-free-icons'

import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar'
import {
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from '@/components/ui/sidebar'
import { useAuthStore } from '@/stores/auth-store'
import { ROLE_LABELS } from '@/routes/settings/-user-role-labels'

const loadNavUserMenu = () => import('@/components/nav-user-menu')
const NavUserMenu = lazy(loadNavUserMenu)

function getInitials(email: string): string {
  const parts = email.split('@')[0].split(/[._-]/)
  return (
    parts
      .slice(0, 2)
      .map((part) => (part.length > 0 ? part[0].toUpperCase() : ''))
      .join('') || email[0].toUpperCase()
  )
}

function UserButton({ onClick }: { onClick: () => void }) {
  const authUser = useAuthStore((state) => state.user)
  if (!authUser) return null

  return (
    <SidebarMenu>
      <SidebarMenuItem>
        <SidebarMenuButton
          size="lg"
          className="data-open:bg-sidebar-accent data-open:text-sidebar-accent-foreground"
          aria-haspopup="menu"
          onClick={onClick}
        >
          <Avatar className="size-8 rounded-lg after:rounded-lg">
            {authUser.avatar_url ? (
              <AvatarImage src={authUser.avatar_url} alt={authUser.email} />
            ) : null}
            <AvatarFallback className="rounded-lg">
              {getInitials(authUser.email)}
            </AvatarFallback>
          </Avatar>
          <div className="grid flex-1 text-left text-sm leading-tight">
            <span className="truncate font-medium">{authUser.email}</span>
            <span className="truncate text-xs">
              {ROLE_LABELS[authUser.role] ?? authUser.role}
            </span>
          </div>
          <HugeiconsIcon icon={ChevronsUpDown} className="ml-auto size-4" />
        </SidebarMenuButton>
      </SidebarMenuItem>
    </SidebarMenu>
  )
}

export default function NavUser() {
  const authUser = useAuthStore((state) => state.user)
  const [menuRequested, setMenuRequested] = useState(false)
  const [menuOpen, setMenuOpen] = useState(false)

  if (!authUser) return null

  function openMenu() {
    setMenuOpen(true)
    setMenuRequested(true)
  }

  const button = <UserButton onClick={openMenu} />

  if (!menuRequested) return button

  return (
    <Suspense fallback={button}>
      <NavUserMenu open={menuOpen} onOpenChange={setMenuOpen} />
    </Suspense>
  )
}
