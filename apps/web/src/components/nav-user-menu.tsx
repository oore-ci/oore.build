import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import {
  ArrowUp as ArrowUp01Icon,
  BookOpen as BookOpen01Icon,
  LogOut as Logout03Icon,
  Moon as Moon02Icon,
  Smartphone as SmartPhone01Icon,
  Sun as Sun03Icon,
} from 'lucide-react'
import { useTheme } from 'next-themes'

import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
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
import { isDemoMode } from '@/lib/demo-mode'
import type { UserRole } from '@/lib/types'
import type { DemoScenario } from '@/demo/state'

const PERSONA_OPTIONS: ReadonlyArray<{ value: UserRole; label: string }> = [
  { value: 'owner', label: 'Owner' },
  { value: 'admin', label: 'Admin' },
  { value: 'developer', label: 'Developer' },
  { value: 'qa_viewer', label: 'QA viewer' },
]

const SCENARIO_OPTIONS: ReadonlyArray<{
  value: DemoScenario
  label: string
}> = [
  { value: 'operating', label: 'Operating' },
  { value: 'blocked', label: 'Blocked' },
  { value: 'degraded', label: 'Degraded' },
  { value: 'empty', label: 'Empty' },
  { value: 'setup', label: 'Setup' },
]

function getInitials(email: string): string {
  const parts = email.split('@')[0].split(/[._-]/)
  return (
    parts
      .slice(0, 2)
      .map((part) => (part.length > 0 ? part[0].toUpperCase() : ''))
      .join('') || email[0].toUpperCase()
  )
}

export default function NavUserMenu({
  open,
  onOpenChange,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const { isMobile } = useSidebar()
  const authUser = useAuthStore((state) => state.user)
  const logoutMutation = useLogout()
  const { theme, setTheme } = useTheme()
  const currentScenario =
    SCENARIO_OPTIONS.find(
      ({ value }) =>
        value ===
        new URLSearchParams(window.location.search).get('demoScenario'),
    ) ?? SCENARIO_OPTIONS[0]

  const changePersona = (role: UserRole) => {
    void import('@/demo/controls').then(({ activateDemoPersona }) => {
      if (activateDemoPersona(role)) window.location.reload()
    })
  }

  const changeScenario = (scenario: DemoScenario) => {
    void import('@/demo/controls').then(({ demoScenarioUrl }) => {
      window.location.assign(demoScenarioUrl(window.location.href, scenario))
    })
  }

  if (!authUser) return null

  return (
    <SidebarMenu>
      <SidebarMenuItem>
        <DropdownMenu open={open} onOpenChange={onOpenChange}>
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
            <DynamicLucideIcon icon={ArrowUp01Icon} className="ml-auto size-4" />
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
            {isDemoMode ? (
              <>
                <DropdownMenuGroup>
                  <DropdownMenuLabel>Demo tools</DropdownMenuLabel>
                  <DropdownMenuSub>
                    <DropdownMenuSubTrigger
                      aria-label={`Persona: ${PERSONA_OPTIONS.find(({ value }) => value === authUser.role)?.label ?? authUser.role}`}
                    >
                      Persona
                      <span className="ml-auto text-xs text-muted-foreground">
                        {PERSONA_OPTIONS.find(
                          ({ value }) => value === authUser.role,
                        )?.label ?? authUser.role}
                      </span>
                    </DropdownMenuSubTrigger>
                    <DropdownMenuSubContent>
                      <DropdownMenuRadioGroup value={authUser.role}>
                        {PERSONA_OPTIONS.map((option) => (
                          <DropdownMenuRadioItem
                            key={option.value}
                            value={option.value}
                            onClick={() => changePersona(option.value)}
                          >
                            {option.label}
                          </DropdownMenuRadioItem>
                        ))}
                      </DropdownMenuRadioGroup>
                    </DropdownMenuSubContent>
                  </DropdownMenuSub>
                  <DropdownMenuSub>
                    <DropdownMenuSubTrigger
                      aria-label={`Scenario: ${currentScenario.label}`}
                    >
                      Scenario
                      <span className="ml-auto text-xs text-muted-foreground">
                        {currentScenario.label}
                      </span>
                    </DropdownMenuSubTrigger>
                    <DropdownMenuSubContent>
                      <DropdownMenuRadioGroup value={currentScenario.value}>
                        {SCENARIO_OPTIONS.map((option) => (
                          <DropdownMenuRadioItem
                            key={option.value}
                            value={option.value}
                            onClick={() => changeScenario(option.value)}
                          >
                            {option.label}
                          </DropdownMenuRadioItem>
                        ))}
                      </DropdownMenuRadioGroup>
                    </DropdownMenuSubContent>
                  </DropdownMenuSub>
                </DropdownMenuGroup>
                <DropdownMenuSeparator />
              </>
            ) : null}
            <DropdownMenuGroup>
              <DropdownMenuLabel className="text-xs text-muted-foreground">
                Mode
              </DropdownMenuLabel>
              <DropdownMenuItem onClick={() => setTheme('light')}>
                <DynamicLucideIcon icon={Sun03Icon} size={16} />
                Light
                {theme === 'light' ? (
                  <span className="ml-auto text-xs text-primary">Active</span>
                ) : null}
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => setTheme('dark')}>
                <DynamicLucideIcon icon={Moon02Icon} size={16} />
                Dark
                {theme === 'dark' ? (
                  <span className="ml-auto text-xs text-primary">Active</span>
                ) : null}
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => setTheme('system')}>
                <DynamicLucideIcon icon={SmartPhone01Icon} size={16} />
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
              <DynamicLucideIcon icon={BookOpen01Icon} size={16} />
              Documentation
            </DropdownMenuItem>
            <DropdownMenuItem
              onClick={() => logoutMutation.mutate()}
              disabled={logoutMutation.isPending}
            >
              <DynamicLucideIcon icon={Logout03Icon} size={16} />
              Sign out
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </SidebarMenuItem>
    </SidebarMenu>
  )
}
