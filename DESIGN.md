# DESIGN.md

Design system governance for `apps/web`. Read this before any frontend UI work.

## Scope

This document governs **`apps/web`** only. `apps/docs-site` (VitePress) follows its own conventions.

## Design Philosophy

1. **shadcn-first.** Use shadcn registry components before building custom ones.
2. **Consistency over novelty.** Reuse existing patterns; avoid inventing new ones.
3. **Accessible by default.** All interactive elements must be keyboard-navigable and screen-reader friendly.
4. **Minimal custom CSS.** Tailwind utility classes and CSS variable tokens handle styling. No hand-written CSS files.
5. **No inline SVGs.** All icons come from Hugeicons.

## Component Selection Rule

When you need a UI component, follow this decision tree:

```
Need a component
  -> Check shadcn registry (npx shadcn@latest add <name>)
     -> Exists? Install it and use it.
     -> Doesn't exist? Build custom using Base UI primitives.
        -> No Base UI primitive? Build with plain HTML + Tailwind, document the pattern here.
```

**Never** build a custom dialog, dropdown, drawer, select, table, form, or toast when shadcn has an equivalent.

## Theming

### CSS Variable Tokens

All colors use oklch CSS variables defined in `src/styles.css`. The token set includes:

| Token | Purpose |
|---|---|
| `--background` / `--foreground` | Page background and text |
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

### Dark Mode

Dark mode is toggled by adding the `.dark` class to the root `<html>` element. The `@custom-variant dark (&:is(.dark *))` directive handles it.

**Rule:** Never use hard-coded Tailwind color classes (e.g., `text-green-600`, `text-blue-600`). Use token-based classes (`text-primary`, `text-destructive`, `text-muted-foreground`) or define new tokens if needed.

### `@theme inline`

Tailwind v4 maps CSS variables to utility classes via `@theme inline` in `styles.css`. When adding a new semantic token, add it to both `:root` / `.dark` and to `@theme inline`.

## Typography

- **Font:** Inter Variable (`@fontsource-variable/inter`)
- **Body text:** `text-sm` (14px)
- **Page headings:** `text-2xl font-semibold tracking-tight`
- **Section headings:** `text-lg font-medium` or `text-sm font-medium` for card titles
- **Labels:** `text-xs font-medium uppercase tracking-wider` for category labels; shadcn `Label` for form fields
- **Muted/helper text:** `text-sm text-muted-foreground`

## Spacing and Layout

- **Page containers:** `max-w-4xl mx-auto px-6 py-8` for standard pages
- **Focused flows** (login, setup): `max-w-lg` or `max-w-sm` centered
- **Vertical rhythm:** `space-y-8` between page sections, `space-y-4` within cards
- **Gaps:** `gap-3` for inline form rows, `gap-2` for button groups

## Icons

**Hugeicons only.** Import from `@hugeicons/react` with icon data from `@hugeicons/core-free-icons`.

```tsx
import { HugeiconsIcon } from '@hugeicons/react'
import { Menu02Icon } from '@hugeicons/core-free-icons'

<HugeiconsIcon icon={Menu02Icon} size={20} />
```

**Anti-pattern:** Never use inline `<svg>` elements for icons. Never import icons from other libraries.

## Component Patterns

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
import { getCoreRowModel, getSortedRowModel, getFilteredRowModel, useReactTable } from '@tanstack/react-table'
import { DataTable } from '@/components/ui/data-table'

const table = useReactTable({
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
- **Row selection:** `Checkbox` with `indeterminate` prop (Base UI uses a separate boolean prop, not `"indeterminate"` string)
- **Row actions:** `DropdownMenu` with ellipsis trigger (`MoreHorizontalCircle01Icon`)
- **Toolbar:** Separate component with filter `Input` bound to `column.setFilterValue()` and bulk action buttons
- **Non-selectable rows:** `enableRowSelection: (row) => boolean` on the table instance

Reference implementation: `routes/settings/users.tsx` with `-users-columns.tsx` and `-users-toolbar.tsx`.

### Form

All forms use react-hook-form + zod + shadcn Form component:

```tsx
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { z } from 'zod'
import { Form, FormField, FormItem, FormLabel, FormControl, FormMessage } from '@/components/ui/form'

const schema = z.object({ name: z.string().min(1) })

function MyForm() {
  const form = useForm({ resolver: zodResolver(schema) })
  return (
    <Form {...form}>
      <form onSubmit={form.handleSubmit(onSubmit)}>
        <FormField control={form.control} name="name" render={({ field }) => (
          <FormItem>
            <FormLabel>Name</FormLabel>
            <FormControl><Input {...field} /></FormControl>
            <FormMessage />
          </FormItem>
        )} />
      </form>
    </Form>
  )
}
```

## Base UI Specifics

shadcn with `style: base-vega` uses Base UI primitives, not Radix. Key differences:

- Use `render` prop, not `asChild`, for custom element rendering
- Use `multiple` prop, not `type="multiple"`, on accordion/toggle groups
- Use `nativeButton={false}` on trigger components when wrapping a `<Button>`

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
| Hard-coded color classes (`text-green-600`) | Token classes (`text-primary`) or new tokens |
| Custom drawer with `transform transition` | shadcn `Sheet` |

## Sidebar Layout

The app uses a `SidebarProvider` + `Sidebar` + `SidebarInset` layout (shadcn sidebar-07 pattern). The sidebar is visible on all authenticated app routes but hidden on login, auth callback, and setup routes.

### Structure

- **SidebarHeader**: `InstanceSwitcher` — switch/add/remove backend instances
- **SidebarContent**: `NavMain` — navigation items (Dashboard, Users for admin/owner)
- **SidebarFooter**: `NavUser` — user avatar dropdown with email, role, sign out

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

Every page within the sidebar layout has a top header with:
1. `SidebarTrigger` — hamburger button to toggle sidebar
2. Vertical `Separator`
3. `PageBreadcrumb` — route-aware breadcrumb

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

Sidebar nav items use `SidebarMenuButton` with TanStack Router `Link` via the `render` prop:

```tsx
<SidebarMenuButton isActive={isActive} render={<Link to="/path" />}>
  <HugeiconsIcon icon={SomeIcon} size={18} />
  <span>Label</span>
</SidebarMenuButton>
```

## Review Checklist

Before submitting frontend changes, verify:

- [ ] All new components checked against shadcn registry first
- [ ] No inline SVG icons (Hugeicons only)
- [ ] No raw HTML `<select>`, `<table>`, or modal implementations
- [ ] All colors use token-based classes (no hard-coded Tailwind colors)
- [ ] Loading states use Spinner or Skeleton
- [ ] Feedback uses toast (transient) or Alert (persistent)
- [ ] Forms use react-hook-form + zod
- [ ] Dark mode works with all new UI
- [ ] All interactive elements are keyboard-accessible

## Governance

To change this design system:

1. Propose the change in an ADR under `docs/adrs/`
2. Update this document
3. Update `AGENTS.md` and `CLAUDE.md` if rules changed
4. Refactor existing code to match the new pattern
