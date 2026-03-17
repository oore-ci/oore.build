import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useNavigate } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Home01Icon,
  FolderLibraryIcon,
  CommandLineIcon,
  Settings01Icon,
  UserMultiple02Icon,
  ComputerIcon,
  LinkSquare01Icon,
  Search01Icon,
} from '@hugeicons/core-free-icons'

import {
  Dialog,
  DialogContent,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { useProjects } from '@/hooks/use-projects'
import { useAuthStore } from '@/stores/auth-store'
import { useHasPermission } from '@/hooks/use-permissions'

interface CommandItem {
  id: string
  label: string
  section: string
  icon: typeof Home01Icon
  action: () => void
  keywords?: string
}

export default function CommandPalette() {
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState('')
  const [selectedIndex, setSelectedIndex] = useState(0)
  const inputRef = useRef<HTMLInputElement>(null)
  const listRef = useRef<HTMLDivElement>(null)
  const navigate = useNavigate()
  const authUser = useAuthStore((s) => s.user)

  const isAdmin = authUser?.role === 'owner' || authUser?.role === 'admin'
  const canWriteProjects = useHasPermission('projects', 'write')

  // Only fetch when authenticated
  const { data: projectsData } = useProjects(
    { limit: 50 },
  )
  const projects = projectsData?.projects ?? []

  // Keyboard shortcut to open
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault()
        setOpen((prev) => !prev)
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [])

  // Focus input when opened
  useEffect(() => {
    if (open) {
      setQuery('')
      setSelectedIndex(0)
      // Small delay for dialog animation
      setTimeout(() => inputRef.current?.focus(), 50)
    }
  }, [open])

  const go = useCallback(
    (to: string) => {
      setOpen(false)
      void navigate({ to })
    },
    [navigate],
  )

  const items = useMemo<CommandItem[]>(() => {
    const result: CommandItem[] = [
      {
        id: 'nav-dashboard',
        label: 'Dashboard',
        section: 'Navigation',
        icon: Home01Icon,
        action: () => go('/'),
        keywords: 'home overview',
      },
      {
        id: 'nav-projects',
        label: 'Projects',
        section: 'Navigation',
        icon: FolderLibraryIcon,
        action: () => go('/projects'),
        keywords: 'repositories repos',
      },
      {
        id: 'nav-builds',
        label: 'Builds',
        section: 'Navigation',
        icon: CommandLineIcon,
        action: () => go('/builds'),
        keywords: 'queue history runs',
      },
    ]

    if (isAdmin) {
      result.push(
        {
          id: 'nav-users',
          label: 'Users',
          section: 'Admin',
          icon: UserMultiple02Icon,
          action: () => go('/settings/users'),
          keywords: 'team members invite',
        },
        {
          id: 'nav-runners',
          label: 'Runners',
          section: 'Admin',
          icon: ComputerIcon,
          action: () => go('/settings/runners'),
          keywords: 'machines agents workers',
        },
        {
          id: 'nav-sources',
          label: 'Sources',
          section: 'Admin',
          icon: LinkSquare01Icon,
          action: () => go('/settings/integrations'),
          keywords: 'github gitlab integrations',
        },
        {
          id: 'nav-preferences',
          label: 'Preferences',
          section: 'Admin',
          icon: Settings01Icon,
          action: () => go('/settings/preferences'),
          keywords: 'settings config',
        },
      )
    }

    if (canWriteProjects) {
      result.push({
        id: 'action-new-project',
        label: 'Create new project',
        section: 'Actions',
        icon: FolderLibraryIcon,
        action: () => go('/projects?openCreate=1'),
        keywords: 'add new project create',
      })
    }

    // Add projects as searchable items
    for (const project of projects) {
      result.push({
        id: `project-${project.id}`,
        label: project.name,
        section: 'Projects',
        icon: FolderLibraryIcon,
        action: () => go(`/projects/${project.id}`),
        keywords: project.description ?? '',
      })
    }

    return result
  }, [go, isAdmin, canWriteProjects, projects])

  const filtered = useMemo(() => {
    if (!query.trim()) return items
    const q = query.toLowerCase()
    return items.filter(
      (item) =>
        item.label.toLowerCase().includes(q) ||
        item.section.toLowerCase().includes(q) ||
        (item.keywords?.toLowerCase().includes(q) ?? false),
    )
  }, [items, query])

  // Reset selection when filtered list changes
  useEffect(() => {
    setSelectedIndex(0)
  }, [filtered.length])

  // Keyboard navigation
  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      setSelectedIndex((i) => Math.min(i + 1, filtered.length - 1))
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      setSelectedIndex((i) => Math.max(i - 1, 0))
    } else if (e.key === 'Enter' && filtered[selectedIndex]) {
      e.preventDefault()
      filtered[selectedIndex].action()
    }
  }

  // Scroll selected item into view
  useEffect(() => {
    const list = listRef.current
    if (!list) return
    const selected = list.children[selectedIndex] as HTMLElement | undefined
    selected?.scrollIntoView({ block: 'nearest' })
  }, [selectedIndex])

  // Group items by section
  const sections = useMemo(() => {
    const map = new Map<string, CommandItem[]>()
    for (const item of filtered) {
      const existing = map.get(item.section)
      if (existing) {
        existing.push(item)
      } else {
        map.set(item.section, [item])
      }
    }
    return map
  }, [filtered])

  // Flat index tracker for keyboard nav
  let flatIndex = -1

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="max-w-lg gap-0 overflow-hidden p-0">
        <DialogTitle className="sr-only">Command palette</DialogTitle>
        <div className="flex items-center gap-2 border-b px-3">
          <HugeiconsIcon
            icon={Search01Icon}
            size={16}
            className="shrink-0 text-muted-foreground"
          />
          <Input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Search projects, pages, actions..."
            className="h-11 border-0 bg-transparent px-0 shadow-none focus-visible:ring-0"
          />
          <kbd className="hidden shrink-0 rounded border bg-muted px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground sm:inline-block">
            ESC
          </kbd>
        </div>
        <div
          ref={listRef}
          className="max-h-72 overflow-y-auto p-1"
        >
          {filtered.length === 0 ? (
            <p className="py-6 text-center text-sm text-muted-foreground">
              No results found.
            </p>
          ) : (
            Array.from(sections.entries()).map(([section, sectionItems]) => (
              <div key={section}>
                <p className="px-2 py-1.5 text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
                  {section}
                </p>
                {sectionItems.map((item) => {
                  flatIndex++
                  const idx = flatIndex
                  return (
                    <button
                      key={item.id}
                      type="button"
                      className={`flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-sm ${
                        idx === selectedIndex
                          ? 'bg-accent text-accent-foreground'
                          : 'text-foreground hover:bg-accent/50'
                      }`}
                      onClick={() => item.action()}
                      onMouseEnter={() => setSelectedIndex(idx)}
                    >
                      <HugeiconsIcon
                        icon={item.icon}
                        size={16}
                        className="shrink-0 text-muted-foreground"
                      />
                      {item.label}
                    </button>
                  )
                })}
              </div>
            ))
          )}
        </div>
      </DialogContent>
    </Dialog>
  )
}
