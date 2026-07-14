import { HugeiconsIcon } from '@hugeicons/react'
import {
  ArrowUp01Icon,
  BookOpen01Icon,
  Logout03Icon,
  Moon02Icon,
  SmartPhone01Icon,
  Sun03Icon,
} from '@hugeicons/core-free-icons'
import { useTheme } from 'next-themes'
import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import {
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  useSidebar,
} from '@/components/ui/sidebar'
import { useAuthStore } from '@/stores/auth-store'
import { useLogout } from '@/hooks/use-auth'

function getInitials(email: string): string {
  const parts = email.split('@')[0].split(/[._-]/)
  return (
    parts
      .slice(0, 2)
      .map((s) => (s.length > 0 ? s[0].toUpperCase() : ''))
      .join('') || email[0].toUpperCase()
  )
}

export default function NavUser() {
  const { isMobile } = useSidebar()
  const authUser = useAuthStore((s) => s.user)
  const logoutMutation = useLogout()
  const { theme, setTheme } = useTheme()

  if (!authUser) return null

  return (
    <SidebarMenu>
      <SidebarMenuItem>
        <DropdownMenu>
          <DropdownMenuTrigger
            render={
              <SidebarMenuButton
                size="lg"
                className="data-open:bg-sidebar-accent data-open:text-sidebar-accent-foreground"
              />
            }
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
            <HugeiconsIcon icon={ArrowUp01Icon} className="ml-auto size-4" />
          </DropdownMenuTrigger>
          <DropdownMenuContent
            className="min-w-56"
            side={isMobile ? 'bottom' : 'right'}
            align="end"
            sideOffset={4}
          >
            <DropdownMenuGroup>
              <DropdownMenuLabel className="font-normal p-0">
                <div className="flex items-center gap-2 px-1 py-1.5 text-left text-sm">
                  <Avatar className="size-8">
                    {authUser.avatar_url ? (
                      <AvatarImage
                        src={authUser.avatar_url}
                        alt={authUser.email}
                      />
                    ) : null}
                    <AvatarFallback className="text-xs">
                      {getInitials(authUser.email)}
                    </AvatarFallback>
                  </Avatar>
                  <div className="grid flex-1 text-left text-sm leading-tight">
                    <span className="truncate font-medium">
                      {authUser.email}
                    </span>
                    <span className="truncate text-xs text-muted-foreground capitalize">
                      {authUser.role.replace('_', ' ')}
                    </span>
                  </div>
                </div>
              </DropdownMenuLabel>
            </DropdownMenuGroup>
            <DropdownMenuSeparator />
            <DropdownMenuGroup>
              <DropdownMenuLabel className="text-xs text-muted-foreground">
                Theme
              </DropdownMenuLabel>
              <DropdownMenuItem onClick={() => setTheme('light')}>
                <HugeiconsIcon icon={Sun03Icon} size={16} />
                Light
                {theme === 'light' ? (
                  <span className="ml-auto text-xs text-primary">Active</span>
                ) : null}
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => setTheme('dark')}>
                <HugeiconsIcon icon={Moon02Icon} size={16} />
                Dark
                {theme === 'dark' ? (
                  <span className="ml-auto text-xs text-primary">Active</span>
                ) : null}
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => setTheme('system')}>
                <HugeiconsIcon icon={SmartPhone01Icon} size={16} />
                System
                {theme === 'system' ? (
                  <span className="ml-auto text-xs text-primary">Active</span>
                ) : null}
              </DropdownMenuItem>
            </DropdownMenuGroup>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              render={
                <a
                  href="https://docs.oore.build"
                  aria-label="Open Oore documentation"
                  target="_blank"
                  rel="noopener noreferrer"
                />
              }
            >
              <HugeiconsIcon icon={BookOpen01Icon} size={16} />
              Documentation
            </DropdownMenuItem>
            <DropdownMenuItem
              onClick={() => logoutMutation.mutate()}
              disabled={logoutMutation.isPending}
            >
              <HugeiconsIcon icon={Logout03Icon} size={16} />
              Sign out
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </SidebarMenuItem>
    </SidebarMenu>
  )
}
