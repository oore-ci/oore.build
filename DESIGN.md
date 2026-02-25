# DESIGN.md

Design system governance for `apps/web` (SolidJS). Read this before any frontend UI work.

## Scope

This document governs **`apps/web`** and frontend migration workstreams. `apps/docs-site` (VitePress) follows its own conventions.

## Design Philosophy

1. **Industrial command-center aesthetic.** Oore CI is a CI platform, not a consumer app. The UI should feel like mission control — authoritative, information-dense, and confident. Every element should communicate competence and production-readiness.
2. **shadcn-first.** Use shadcn registry components before building custom ones.
3. **Consistency over novelty.** Reuse established patterns across all pages.
4. **Accessible by default.** All interactive elements must be keyboard-navigable and screen-reader friendly.
5. **Minimal custom CSS.** Tailwind utility classes and CSS variable tokens handle styling. No hand-written CSS files.
6. **No inline SVGs.** All icons come from Hugeicons.

## Visual Language

### Typography Hierarchy

The typography system uses deliberate weight and size contrast to create clear hierarchy:

- **Page titles:** `text-3xl font-bold tracking-tight` — commanding presence
- **Stat card values:** `text-2xl font-bold tracking-tight` — large and prominent
- **Section labels (card titles):** `text-sm font-medium uppercase tracking-wider text-muted-foreground` — small, uppercase, always muted to differentiate from content
- **Body text:** `text-sm` (14px)
- **Helper/muted text:** `text-xs text-muted-foreground`
- **Mono data:** `font-mono text-[11px] text-muted-foreground` for IDs, SHAs, timestamps
- **Back links:** `text-xs font-medium uppercase tracking-wider text-muted-foreground hover:text-foreground`

### Card Patterns

> **NEVER add `p-*` to `CardContent`.** The `Card` component provides `py-6` (vertical) and `CardContent` provides `px-6` (horizontal). Adding `p-4`, `p-5`, `p-6`, etc. to `CardContent` doubles the vertical padding and creates bloated, uneven cards. If you need tighter cards, use `<Card size="sm">` which switches to `py-4` / `px-4` automatically.

#### Stat Cards (compact, no header)

Use for overview metrics at the top of pages. No `CardHeader` — label goes directly inside `CardContent`. The `Card` component already provides symmetric `py-6` padding, so do **not** add `pt-6` to `CardContent` (that would double the top padding):

```tsx
<Card>
  <CardContent>
    <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">Label</p>
    <p className="mt-3 text-2xl font-bold tracking-tight">Value</p>
    <p className="mt-1 text-xs text-muted-foreground">Description</p>
  </CardContent>
</Card>
```

For stat cards with a status badge inline:

```tsx
<Card>
  <CardContent>
    <div className="flex items-center justify-between">
      <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">Label</p>
      <Badge variant="success">online</Badge>
    </div>
    <p className="mt-3 text-2xl font-bold tracking-tight">Value</p>
    <p className="mt-1 text-xs text-muted-foreground">Description</p>
  </CardContent>
</Card>
```

#### Content Cards (with header)

Use for tables, forms, and detailed content. Card titles use the uppercase label style:

```tsx
<Card>
  <CardHeader>
    <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
      Section Title
    </CardTitle>
  </CardHeader>
  <CardContent>{/* ... */}</CardContent>
</Card>
```

For card headers with a count or action:

```tsx
<Card>
  <CardHeader>
    <div className="flex items-center justify-between">
      <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
        Inventory
      </CardTitle>
      <span className="text-xs text-muted-foreground">{count} total</span>
    </div>
  </CardHeader>
  <CardContent>{/* ... */}</CardContent>
</Card>
```

### Color Tokens

All colors use oklch CSS variables in `src/styles.css`. The token set includes a `--surface` token for the main content area background, which creates subtle depth separation between the sidebar/header (using `--background`) and the page content area.

| Token | Purpose |
|---|---|
| `--background` / `--foreground` | Header and sidebar backgrounds, text |
| `--surface` | Main content area background (subtle separation from header) |
| `--card` / `--card-foreground` | Card surfaces |
| `--popover` / `--popover-foreground` | Popovers and dropdowns |
| `--primary` / `--primary-foreground` | Primary actions (amber) |
| `--secondary` / `--secondary-foreground` | Secondary actions |
| `--muted` / `--muted-foreground` | Subdued text and backgrounds |
| `--accent` / `--accent-foreground` | Hover/active states |
| `--destructive` | Destructive/error actions |
| `--border` | Borders |
| `--input` | Input borders |
| `--ring` | Focus rings |
| `--success` / `--warning` / `--info` | Semantic status colors |

### Dark Mode

Dark mode is toggled by adding the `.dark` class to the root `<html>` element. The `@custom-variant dark (&:is(.dark *))` directive handles it.

**Rule:** Never use hard-coded Tailwind color classes (e.g., `text-green-600`, `text-blue-600`). Use token-based classes (`text-primary`, `text-destructive`, `text-muted-foreground`) or define new tokens if needed.

### `@theme inline`

Tailwind v4 maps CSS variables to utility classes via `@theme inline` in `styles.css`. When adding a new semantic token, add it to both `:root` / `.dark` and to `@theme inline`.

## Component Selection Rule

When you need a UI component, follow this decision tree:

```
Need a component
  -> Check shadcn-solid registry (bunx shadcn-solid add <name>)
     -> Exists? Install it and use it.
     -> Doesn't exist? Build custom with Solid primitives + tokenized styles.
        -> No viable primitive? Build with plain HTML + Tailwind, document the pattern here.
```

**Never** build a custom dialog, dropdown, drawer, select, table, form, or toast when shadcn has an equivalent.

## Typography

- **Sans font:** Google Sans Flex (self-hosted variable woff2, weight 100-900)
- **Mono font:** JetBrains Mono Variable (`@fontsource-variable/jetbrains-mono`) — commit SHAs, build numbers, IDs
- **Body text:** `text-sm` (14px)
- **Page headings:** `text-3xl font-bold tracking-tight`
- **Section labels:** `text-sm font-medium uppercase tracking-wider text-muted-foreground`
- **Labels:** `text-xs font-medium uppercase tracking-wider` for category labels; shadcn `Label` for form fields
- **Muted/helper text:** `text-sm text-muted-foreground` or `text-xs text-muted-foreground`

## Spacing and Layout

- **Page containers:** Use `PageLayout` component (`mx-auto w-full px-6 py-8 lg:px-10 lg:py-10 space-y-6`). Supports `width="narrow"` (max-w-xl), `width="default"` (max-w-4xl), and `width="wide"` (max-w-6xl).
- **Data-dense pages:** Prefer `width="wide"` for project/build/integrations/runners inventory and detail pages.
- **Page headers:** Use `PageHeader` component with `title`, optional `description`, `actions`, `back`, and `meta` props. Title uses `text-3xl font-bold tracking-tight`. Back link uses uppercase tracking.
- **Focused flows** (login, setup): `max-w-lg` or `max-w-sm` centered with a branded header (logo in bordered square container + bold title)
- **Vertical rhythm:** `space-y-6` between page sections, `space-y-4` within cards
- **Gaps:** `gap-3` for inline form rows, `gap-2` for button groups

### Branded Header Pattern

Used on login, setup, and welcome (no-instance) pages for strong brand presence:

```tsx
<div className="mx-auto flex size-14 items-center justify-center">
  <img src="/logo.svg" alt="Oore logo" className="size-full" />
</div>
<h1 className="text-3xl font-bold tracking-tight">Title</h1>
<p className="text-sm text-muted-foreground">Subtitle</p>
```

## Icons

**Hugeicons only.** Import icon data from `@hugeicons/core-free-icons` and render via the shared Solid `HugeIcon` adapter.

```tsx
import { HugeIcon } from '@/components/huge-icon'
import { Menu02Icon } from '@hugeicons/core-free-icons'

<HugeIcon icon={Menu02Icon} size={20} />
```

**Anti-pattern:** Never use inline `<svg>` elements for icons. Never import icons from other libraries.

## Component Patterns

### Quick Action Links

Used on the dashboard for navigation items with icons:

```tsx
<Link
  to={to}
  className="group flex items-center justify-between gap-4 border border-border/60 bg-card p-4 text-left transition-colors hover:border-primary/30 hover:bg-primary/5"
>
  <div className="flex items-center gap-4">
    <div className="flex size-9 shrink-0 items-center justify-center border bg-muted/40 text-muted-foreground transition-colors group-hover:border-primary/30 group-hover:text-primary">
      <HugeiconsIcon icon={icon} size={16} />
    </div>
    <div className="min-w-0">
      <p className="text-sm font-medium">{title}</p>
      <p className="text-xs text-muted-foreground">{description}</p>
    </div>
  </div>
  <HugeiconsIcon icon={ArrowRight01Icon} size={16} className="shrink-0 text-muted-foreground transition-transform group-hover:translate-x-0.5" />
</Link>
```

### Dialog

For general-purpose modals (forms, informational):

```tsx
import {
  Dialog, DialogContent, DialogHeader,
  DialogTitle, DialogDescription, DialogFooter,
} from '@/components/ui/dialog'

<Dialog open={open} onOpenChange={onOpenChange}>
  <DialogContent>
    <DialogHeader>
      <DialogTitle>Title</DialogTitle>
      <DialogDescription>Description</DialogDescription>
    </DialogHeader>
    {/* body */}
    <DialogFooter>
      <Button variant="outline" onClick={() => onOpenChange(false)}>Cancel</Button>
      <Button onClick={onConfirm}>Confirm</Button>
    </DialogFooter>
  </DialogContent>
</Dialog>
```

### AlertDialog

For destructive confirmations:

```tsx
import {
  AlertDialog, AlertDialogContent, AlertDialogHeader,
  AlertDialogTitle, AlertDialogDescription, AlertDialogFooter,
  AlertDialogCancel, AlertDialogAction,
} from '@/components/ui/alert-dialog'

<AlertDialog open={open} onOpenChange={onOpenChange}>
  <AlertDialogContent>
    <AlertDialogHeader>
      <AlertDialogTitle>Are you sure?</AlertDialogTitle>
      <AlertDialogDescription>This action cannot be undone.</AlertDialogDescription>
    </AlertDialogHeader>
    <AlertDialogFooter>
      <AlertDialogCancel>Cancel</AlertDialogCancel>
      <AlertDialogAction onClick={onConfirm}>Continue</AlertDialogAction>
    </AlertDialogFooter>
  </AlertDialogContent>
</AlertDialog>
```

### Sheet (Drawer)

For side panels and mobile navigation:

```tsx
import { Sheet, SheetContent, SheetHeader, SheetTitle } from '@/components/ui/sheet'

<Sheet open={open} onOpenChange={onOpenChange}>
  <SheetContent side="left" className="w-72">
    <SheetHeader>
      <SheetTitle>Navigation</SheetTitle>
    </SheetHeader>
    {/* content */}
  </SheetContent>
</Sheet>
```

### DropdownMenu

For action menus and selection dropdowns:

```tsx
import {
  DropdownMenu, DropdownMenuTrigger, DropdownMenuContent,
  DropdownMenuItem, DropdownMenuSeparator,
} from '@/components/ui/dropdown-menu'

<DropdownMenu>
  <DropdownMenuTrigger render={<Button variant="outline" />}>
    Open
  </DropdownMenuTrigger>
  <DropdownMenuContent>
    <DropdownMenuItem>Option A</DropdownMenuItem>
    <DropdownMenuSeparator />
    <DropdownMenuItem>Option B</DropdownMenuItem>
  </DropdownMenuContent>
</DropdownMenu>
```

### Select

For form select inputs:

```tsx
import {
  Select, SelectTrigger, SelectContent,
  SelectItem, SelectValue,
} from '@/components/ui/select'

<Select value={value} onValueChange={onChange}>
  <SelectTrigger>
    <SelectValue placeholder="Choose..." />
  </SelectTrigger>
  <SelectContent>
    <SelectItem value="a">Option A</SelectItem>
    <SelectItem value="b">Option B</SelectItem>
  </SelectContent>
</Select>
```

### Table

For simple, static tables:

```tsx
import {
  Table, TableHeader, TableRow, TableHead,
  TableBody, TableCell,
} from '@/components/ui/table'

<Table>
  <TableHeader>
    <TableRow>
      <TableHead>Column</TableHead>
    </TableRow>
  </TableHeader>
  <TableBody>
    <TableRow>
      <TableCell>Value</TableCell>
    </TableRow>
  </TableBody>
</Table>
```

### DataTable

For interactive data tables with sorting, filtering, or row selection, use `DataTable` (built on TanStack Table) with the shadcn `Table` primitives:

```tsx
import { createSolidTable, getCoreRowModel, getSortedRowModel, getFilteredRowModel } from '@tanstack/solid-table'
import { DataTable } from '@/components/ui/data-table'

const table = createSolidTable({
  data,
  columns,
  getCoreRowModel: getCoreRowModel(),
  getSortedRowModel: getSortedRowModel(),
  getFilteredRowModel: getFilteredRowModel(),
  state: { sorting, columnFilters, rowSelection },
  onSortingChange: setSorting,
  onColumnFiltersChange: setColumnFilters,
  onRowSelectionChange: setRowSelection,
})

<DataTable table={table} />
```

**Column definitions** go in a separate `-<name>-columns.tsx` file (prefixed with `-` under `routes/` to exclude from router). Pass callbacks via an options object so columns don't depend on hooks:

```tsx
export function getColumns(options: ColumnOptions): Array<ColumnDef<MyType>> { ... }
```

**Key patterns:**
- **Sortable headers:** `Button variant="ghost"` with `ArrowUpDownIcon` that calls `column.toggleSorting()`
- **Row selection:** `Checkbox` with explicit `indeterminate` boolean handling
- **Row actions:** `DropdownMenu` with ellipsis trigger (`MoreHorizontalCircle01Icon`)
- **Toolbar:** Separate component with filter `Input` bound to `column.setFilterValue()` and bulk action buttons
- **Non-selectable rows:** `enableRowSelection: (row) => boolean` on the table instance

Reference implementation: `routes/settings/users.tsx` with `-users-columns.tsx` and `-users-toolbar.tsx`.

### Form

All forms use `@tanstack/solid-form` + zod + shared shadcn-solid field wrappers:

```tsx
import { createForm } from '@tanstack/solid-form'
import { z } from 'zod'
import { Input } from '@/components/ui/input'

const schema = z.object({ name: z.string().min(1) })

function MyForm() {
  const form = createForm(() => ({
    defaultValues: { name: '' },
    validators: { onChange: schema },
    onSubmit: ({ value }) => onSubmit(value),
  }))
  return (
    <form
      onSubmit={(event) => {
        event.preventDefault()
        void form.handleSubmit()
      }}
    >
      <form.Field name="name">
        {(field) => (
          <Input
            value={field().state.value}
            onInput={(event) => field().handleChange(event.currentTarget.value)}
          />
        )}
      </form.Field>
    </form>
  )
}
```

## Solid Component Specifics

shadcn-solid components should remain token-driven and framework-native:

- Prefer generated shadcn-solid components over custom controls.
- Keep route links on TanStack `Link`/`createLink`.
- Avoid framework-specific compatibility shims that reintroduce React runtime patterns.

## Loading States

| Scenario | Component |
|---|---|
| Full-page loading | `Spinner` centered with descriptive text |
| Data table loading | `Skeleton` rows matching table layout |
| Button loading | `Spinner` inside button + text (e.g., "Inviting...") |

```tsx
import { Spinner } from '@/components/ui/spinner'
import { Skeleton } from '@/components/ui/skeleton'

// Full page
<div className="min-h-screen flex items-center justify-center">
  <div className="flex items-center gap-3">
    <Spinner className="size-5" />
    <p className="text-muted-foreground text-sm">Loading...</p>
  </div>
</div>

// Skeleton rows
<Skeleton className="h-4 w-full" />
```

## Error States

- **Page-level errors:** `Alert variant="destructive"` with title and description
- **Form field errors:** `FormMessage` (within shadcn Form) or `<p className="text-sm text-destructive">`
- **Inline errors:** `Alert variant="destructive"` within the relevant section

## Feedback Patterns

- **Transient feedback** (action succeeded/failed): Sonner `toast` via `sonner`
- **Persistent feedback** (blocking issues): `Alert` component

```tsx
import { toast } from 'sonner'

// Success
toast.success('User invited')

// Error
toast.error('Failed to update role')
```

## Accessibility

- All interactive elements must be focusable and keyboard-operable
- Modals must trap focus and restore it on close (shadcn Dialog/AlertDialog/Sheet handle this)
- Form inputs must have associated labels
- Color alone must not convey meaning (pair with text or icons)
- Minimum touch target: 44x44px for mobile interactions

## Anti-Patterns

| Don't | Do Instead |
|---|---|
| `<div className="fixed inset-0">` for modals | shadcn `Dialog` or `AlertDialog` |
| Manual backdrop div + click handler | shadcn overlay components |
| `useEffect` for escape-key closing | shadcn components handle this |
| `useRef` + click-outside detection | shadcn `DropdownMenu` or `Popover` |
| Inline `<svg>` icons | Hugeicons via `@hugeicons/react` |
| Raw `<table>` elements | shadcn `Table` |
| Raw `<select>` elements | shadcn `Select` |
| Raw `<input>` without styling | shadcn `Input` |
| `useState` + `setTimeout` for feedback | Sonner `toast` |
| Hard-coded color classes (`text-green-600`) | Token classes (`text-primary`, `text-success`) or new tokens |
| Manual status-to-color badge mapping | `getStatusVariant()` / `getIntegrationStatusVariant()` from `@/lib/status-variants` |
| Ad-hoc `<h1>` + `<p>` page headers | `PageHeader` component |
| Inline `max-w-4xl mx-auto px-6 py-8` wrappers | `PageLayout` component |
| Custom drawer with `transform transition` | shadcn `Sheet` |
| `CardTitle className="text-base"` for section titles | `CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground"` |
| CardHeader + CardTitle for stat cards | Compact pattern: `CardContent` (no `pt-6`) with inline label |
| `CardContent className="p-4"` or any `p-*` override | Plain `CardContent` — `Card` provides `py-6`, `CardContent` provides `px-6`. Use `<Card size="sm">` for compact cards. |

## Sidebar Layout

The app uses a `SidebarProvider` + `Sidebar` + `SidebarInset` layout (shadcn sidebar-07 pattern). The sidebar is visible on all authenticated app routes but hidden on login, auth callback, and setup routes.

### Structure

- **SidebarHeader**: `InstanceSwitcher` — switch/add/remove backend instances
- **SidebarContent**: `NavMain` — flat navigation (Dashboard, Projects, Builds + admin section)
- **SidebarFooter**: `NavUser` — user avatar dropdown with email, role, sign out, theme switcher

### When Sidebar Shows

| Route | Sidebar Visible? |
|---|---|
| `/` (Dashboard) | Yes (when authenticated) |
| `/settings/users` | Yes |
| `/login` | No |
| `/auth/callback` | No |
| `/setup/*` | No |

When no active instance or unauthenticated, `AppSidebar` returns null and the layout degrades gracefully.

### Content Header

Every page within the sidebar layout has a sticky top header (`h-12`) with:
1. `SidebarTrigger` — hamburger button to toggle sidebar
2. Vertical `Separator`
3. Logo and brand name
4. Vertical `Separator`
5. `PageBreadcrumb` — route-aware breadcrumb

The header uses `bg-background` (solid) while the content area uses `bg-surface` (subtly different) for depth.

### Avatar Pattern

User avatars use the `Avatar` + `AvatarImage` + `AvatarFallback` pattern:

```tsx
import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar'

<Avatar>
  {user.avatar_url ? <AvatarImage src={user.avatar_url} alt={user.email} /> : null}
  <AvatarFallback>{initials}</AvatarFallback>
</Avatar>
```

Initials are derived from the email prefix (split on `.`, `_`, `-`). The OIDC `picture` claim is extracted at login and stored as `avatar_url`.

### Nav Link Integration

Sidebar nav items use TanStack Router `Link` directly:

```tsx
<Link to="/path" class={isActive ? 'text-primary' : 'text-muted-foreground'}>
  <HugeIcon icon={SomeIcon} size={18} />
  <span>Label</span>
</Link>
```

## Review Checklist

Before submitting frontend changes, verify:

- [ ] All new components checked against shadcn registry first
- [ ] No inline SVG icons (Hugeicons only)
- [ ] No raw HTML `<select>`, `<table>`, or modal implementations
- [ ] All colors use token-based classes (no hard-coded Tailwind colors)
- [ ] Loading states use Spinner or Skeleton
- [ ] Feedback uses toast (transient) or Alert (persistent)
- [ ] Forms use @tanstack/solid-form + zod
- [ ] Dark mode works with all new UI
- [ ] All interactive elements are keyboard-accessible
- [ ] Card titles use uppercase label style (`text-sm font-medium uppercase tracking-wider text-muted-foreground`)
- [ ] Stat cards use compact pattern (no CardHeader, label inside CardContent)
- [ ] Page titles use `text-3xl font-bold tracking-tight`

## Governance

To change this design system:

1. Propose the change in a Linear ADR (see `docs/README.md` and the Docs Index)
2. Update this document
3. Update `AGENTS.md` and `CLAUDE.md` if rules changed
4. Refactor existing code to match the new pattern
