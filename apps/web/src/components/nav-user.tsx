import { Suspense, lazy, useState } from 'react'
import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import { ArrowUp as ArrowUp01Icon } from 'lucide-react'

import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar'
import {
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from '@/components/ui/sidebar'
import { useAuthStore } from '@/stores/auth-store'

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

function UserButton({
  onClick,
  onFocus,
  onMouseEnter,
}: {
  onClick: () => void
  onFocus: () => void
  onMouseEnter: () => void
}) {
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
          onFocus={onFocus}
          onMouseEnter={onMouseEnter}
        >
          <Avatar className="size-8">
            {authUser.avatar_url ? (
              <AvatarImage src={authUser.avatar_url} alt={authUser.email} />
            ) : null}
            <AvatarFallback className="text-xs">
              {getInitials(authUser.email)}
            </AvatarFallback>
          </Avatar>
          <div className="grid flex-1 text-left text-sm leading-tight">
            <span className="truncate font-medium">{authUser.email}</span>
            <span className="truncate text-xs text-muted-foreground capitalize">
              {authUser.role.replace('_', ' ')}
            </span>
          </div>
          <DynamicLucideIcon icon={ArrowUp01Icon} className="ml-auto size-4" />
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

  const button = (
    <UserButton
      onClick={openMenu}
      onFocus={() => void loadNavUserMenu()}
      onMouseEnter={() => void loadNavUserMenu()}
    />
  )

  if (!menuRequested) return button

  return (
    <Suspense fallback={button}>
      <NavUserMenu open={menuOpen} onOpenChange={setMenuOpen} />
    </Suspense>
  )
}
