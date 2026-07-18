# Oore frontend design contract

Read this before changing any Oore user interface.

## Scope

The visual direction applies across the signed-in product (`apps/web`), public documentation (`apps/docs-site`), and the public site (`apps/site`). The component and state-management rules in this document apply specifically to `apps/web`.

- `apps/web` is React with TanStack Router, TanStack Query, Zustand, shadcn, and Base UI.
- `apps/docs-site` is VitePress with a Vue theme. Do not add a parallel React or shadcn application scaffold.
- `apps/site` is a small static Vite application. Keep it dependency-light and truthful to the product.

## Product character

Oore is a quiet technical tool. It should feel precise, calm, and dependable under real operational pressure.

1. **Truth before decoration.** State, scope, permissions, and recovery actions must be unambiguous.
2. **Compact without becoming cryptic.** Prefer useful density, short copy, and progressive disclosure over giant cards or explanatory walls.
3. **One interaction grammar.** Equivalent actions and states must look and behave the same across routes.
4. **Hierarchy from type, space, and dividers.** A new card is not the default way to create a section.
5. **Accessible by default.** Keyboard use, visible focus, readable contrast, reduced motion, zoom, and touch targets are part of the component contract.

Avoid “mission control” theatre: excessive uppercase, ornamental telemetry, fake terminal treatment, gradients, glow, glass effects, decorative pills, and dashboards made from interchangeable stat cards.

## Visual language

### Typography

- **UI text:** Inter Variable.
- **Machine data:** JetBrains Mono Variable for commit SHAs, build numbers, IDs, paths, code, and command output.
- **Page title:** `text-2xl font-semibold tracking-tight`.
- **Section title:** `text-sm font-semibold` unless the content hierarchy requires a larger heading.
- **Body:** `text-sm` with readable line height.
- **Supporting text:** `text-xs text-muted-foreground` or `text-sm text-muted-foreground`.

Use sentence case for headings, buttons, labels, tabs, and navigation. Uppercase is reserved for established identifiers and protocol vocabulary such as `GET`, `OIDC`, or a literal environment variable.

Do not use wide letter spacing as a substitute for hierarchy. Do not make every heading bold or oversized.

### Color and surfaces

The product defaults to shadcn Create's Neutral base, Amber color theme, and Vega component style. Signed-in operators may choose any color theme or component style exposed by the Create route; Oore mirrors the upstream values instead of maintaining a parallel design registry.

- `--primary` is for the primary action, selected navigation, links, and focus emphasis—not generic status decoration.
- Runtime themes merge the upstream Neutral base with the selected Theme overlay, then replace one injected `:root`/`.dark` variable sheet, matching shadcn Create's application method. The copied registry data stays source-identical; Oore deliberately aliases sidebar emphasis to the app primary so a theme has one accent color everywhere. Typography remains outside this color-only preference.
- Runtime component styles are Vega, Nova, Maia, Lyra, Mira, Luma, Sera, and Rhea. Shared Base UI primitives expose shadcn's neutral `cn-*` hooks; the selected source-identical style sheet is loaded on demand and one validated `style-*` class is applied to `document.body`, matching shadcn Create's switching method. Style selection also applies Create's default proportional radius scale; Lyra and Sera force a zero radius as they do upstream.
- Oore-only success, warning, and info tokens retain their semantic meaning across themes.
- `--background`, `--surface`, `--card`, and their foreground pairs create subtle depth without gradients.
- `--success`, `--warning`, `--destructive`, and `--info` communicate semantic state with text or an icon; color alone is never sufficient.
- Borders and dividers should remain visible in both themes without becoming the loudest element.
- Do not flatten the radius scale or override style-owned geometry in application components.

Static application colors come from `apps/web/src/styles.css`. Runtime theme registry values live in `apps/web/src/lib/color-theme.ts` and must stay source-identical to shadcn Create; documented Oore token aliases are applied after that data rather than changing the copy. Add a light and dark value plus the `@theme inline` mapping when a new Oore-specific semantic token is genuinely required. Never use hard-coded Tailwind palette classes in application UI.

### Borders, cards, and hierarchy

Start with a semantic `section`, a heading, spacing, and a divider. Use `Card` only when the content is an independent boundary, such as a focused form, a self-contained preview, or a group that must remain visually distinct from adjacent content.

- Do not wrap a table in a card merely because it is a table.
- Do not nest cards or repeat a border around content already bounded by a parent.
- Prefer `divide-y border bg-card` for a related row group.
- Avoid rows of decorative stat cards when a compact summary or definition list communicates the same facts.
- Keep status badges small and semantic. Do not turn ordinary metadata or actions into pills.

The checked-in `Card` primitive owns its spacing. Do not add blanket `p-*` overrides to `CardContent`; use the supported compact size or a different structural pattern.

## Page structure

### `PageLayout`

Use `@/components/page-layout` for signed-in route content.

| Width     | Class        | Use                                               |
| --------- | ------------ | ------------------------------------------------- |
| `narrow`  | `max-w-2xl`  | focused forms and setup-like tasks                |
| `default` | `max-w-5xl`  | settings and ordinary detail pages                |
| `wide`    | `max-w-7xl`  | collections and information-dense detail pages    |
| `full`    | no max width | immersive logs or genuinely full-width workspaces |

The component owns responsive page padding and vertical rhythm. Do not reproduce one-off `max-w-* mx-auto px-*` route wrappers.

### `PageHeader`

Use `@/components/page-header` for the page title, optional description, primary actions, and compact metadata.

- Keep the title and action in the same predictable header row on desktop.
- Actions wrap below the title on narrow screens and remain touch-safe.
- Put the route's primary create/add action here, not inside an arbitrary table toolbar.
- Use the application breadcrumb for parent navigation; do not add a second decorative back-link system.
- Keep descriptions under roughly 65 characters per line and omit them when the title is already sufficient.

### Settings

`/settings` is the canonical, role-aware Settings hub. Its categories and route metadata live in `@/components/settings/settings-navigation`.

- Add a settings destination to that shared model instead of creating another sidebar section or an unlinked route.
- A settings detail page should solve one administrative task. Use progressive disclosure for rare credentials or provider-specific controls.
- Large repository or account sets use search and pagination; never render every repository as a permanently expanded configuration block.
- Security policy copy must state scope and consequence without pressuring an operator to weaken policy. A blocked repository is a valid steady state.

## Component selection

For `apps/web`, check the shadcn registry first. Vega remains the checked-in generator seed, but shared Base UI primitives must preserve the neutral `cn-*` hook contract so the supported runtime styles can own component geometry.

1. Use the installed shadcn component when it covers the behavior.
2. If it does not, compose a small component from Base UI primitives.
3. Use semantic HTML and Tailwind only when neither provides the necessary primitive.
4. Extract a shared product primitive only after at least two screens prove the same behavior and shape.

When installing or refreshing a registry component, preserve its official Base UI behavior and Hugeicons integration, then verify that every upstream neutral `cn-*` hook remains present. Do not leave Vega-only size, spacing, radius, or typography utilities in the shared primitive when those values belong to the selected style sheet.

Never create custom dialogs, alert dialogs, sheets, dropdown menus, selects, tables, toasts, or form controls when the shadcn equivalent exists.

Base UI specifics:

- Use `render`, not Radix's `asChild`, for custom element rendering.
- Set `nativeButton={false}` when a button-semantic Base UI primitive renders a non-button element.
- Preserve the official focus, keyboard, dismissal, and portal behavior.

Use Hugeicons through `@hugeicons/react` and `@hugeicons/core-free-icons`. No inline SVG icons or mixed icon libraries. Icon-only controls require an accessible name and a tooltip when their meaning is not universal.

## Forms and actions

- Use react-hook-form, zod, and the shadcn Form components for forms.
- Put a visible label on every field. Placeholder text is not a label.
- Use one primary submit action, a quiet cancel/back action, and explicit destructive confirmation.
- Keep identical verbs for identical actions: create/add, connect, save, retry, disconnect, delete.
- Disable or explain unavailable actions at the point of use; do not make users infer a role or policy restriction from a missing control.
- Use Toast for transient action feedback and Alert for persistent or blocking information.

## Collections

Use the existing product primitives rather than inventing a generic parallel table system:

- `CollectionSearchInput` for debounced collection search.
- `SortableTableHead` for accessible sort direction and toggling.
- `CollectionPagination` for result context, 20/50/100 page size, and previous/next navigation.
- shadcn `Table` and TanStack Table for tabular behavior where comparison across columns matters.

The standard collection order is:

1. Page header with the route-level primary action.
2. Search, related filters, conditional bulk actions, then collection-specific secondary actions.
3. The collection region with its own loading, error, empty, and filtered-empty states.
4. Result count and pagination separated by a top divider.

Search, filter, sort, page, and page size should be URL-backed when restoration, sharing, or browser Back behavior is useful. Server-paginated data must be sorted on the server with a stable tie-breaker; do not sort only the visible page and imply a global order.

Row actions belong at the trailing edge and use one overflow-menu treatment unless a single frequent action is clearly primary. Destructive row actions require confirmation.

### Responsive collections

- **Desktop (1024 px and above):** use the full table when comparison is useful.
- **Tablet (640–1023 px):** preserve identity and primary status/value, hide or disclose secondary metadata, and wrap controls in a stable order. Horizontal scrolling is not the default solution.
- **Phone (below 640 px):** use compact list rows when cross-column comparison is not essential. Put full-width search first, then filter/sort controls; keep identity, status, primary value, and actions predictable.
- Use horizontal scrolling only for genuinely comparative datasets, with an obvious affordance and the identity column retained where practical.

Skeletons must resemble the final rows. Long text truncates deliberately and exposes the full value when needed. Touch targets remain at least 44 px on coarse pointers.

## Truthful query states

TanStack Query owns server state. Every query-backed region must distinguish:

| State                           | Required treatment                                                  |
| ------------------------------- | ------------------------------------------------------------------- |
| Initial load                    | destination-shaped skeletons or a small labelled spinner            |
| Background refresh              | preserve usable data and show only subtle pending feedback          |
| Error                           | explicit error copy and a retry action in the affected region       |
| Empty                           | explain that no records exist and offer the relevant primary action |
| Filtered empty                  | say no results match and offer Clear filters/search                 |
| Partial or unavailable relation | name the unavailable source instead of silently dropping it         |
| Policy-blocked work             | show the exact block reason and link to the canonical control       |

Do not render `[]` after a failed request, replace useful data with a page spinner during ordinary pagination, or show stale status as current. Propagate abort signals for query-backed reads and preserve prior page data only when its stale nature cannot mislead the user.

## Motion

Motion is feedback, not decoration.

- Prefer color and opacity transitions; use short transforms only for direct spatial feedback.
- Keep routine transitions around 100–180 ms.
- Avoid entrance choreography, layout animation on dense collections, looping decoration, and motion that delays access to content.
- `prefers-reduced-motion: reduce` must collapse nonessential animation and smooth scrolling.

## Focused and unauthenticated flows

Login, setup, recovery, and no-instance screens may use a narrow centered layout, but they still use the same tokens, typography, field patterns, and action hierarchy. On short screens, content starts at the top and remains scrollable instead of being vertically trapped.

The authenticated shell uses the shared sidebar and breadcrumb header. Navigation is off-canvas through tablet widths; no route should create a second competing navigation rail.

## Public docs and site

- Public copy must describe capabilities that exist now. Label alpha constraints and Direct macOS runner trust boundaries plainly.
- Documentation navigation is task-first: Get started, Guides, Reference/OpenAPI, and Operations.
- Load the interactive OpenAPI client only on OpenAPI routes.
- Product screenshots must come from the current local demo with representative data, not hand-built facsimiles.
- Keep the shared visual direction—Inter, the selected theme accent, cool neutrals, dividers, and minimal motion—without forcing React/shadcn dependencies into VitePress or the static site.

## Review checklist

Before handing off frontend work, verify:

- [ ] The page uses `PageLayout`, `PageHeader`, and shared Settings/collection primitives where applicable.
- [ ] Headings and actions use sentence case; no decorative uppercase or verbose helper copy was added.
- [ ] Cards represent real boundaries; there is no redundant card or divider nesting.
- [ ] Initial load, refresh, error, empty, and filtered-empty states are truthful and local to the affected region.
- [ ] Desktop, tablet, phone, light, and dark behavior remain coherent.
- [ ] Keyboard order, focus visibility, screen-reader names, 200% zoom, and touch targets are usable.
- [ ] Colors use semantic tokens and status is not communicated by color alone.
- [ ] Components are shadcn/Base UI first, icons are Hugeicons, and forms use react-hook-form + zod.
- [ ] Motion is minimal and reduced-motion behavior is preserved.
- [ ] User-facing behavior is recorded in `docs/changes.md` and the canonical Linear docs are updated or explicitly listed for manual sync.

## Governance

A change to this contract requires the same implementation slice to update `DESIGN.md`, the relevant rules in `AGENTS.md`, `docs/changes.md`, the frontend execution plan when milestone truth changes, and the canonical Linear feature/contract/ADR documents required by `docs/README.md`.
