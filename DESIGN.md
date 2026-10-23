# Oore frontend design system

This document is the checked-in source of truth for implementing Oore's
frontend. It governs presentation, component ownership, interaction, responsive
behavior, and feedback states.

Read this document before changing frontend UI. When this document and the
implementation differ, preserve current behavior, call out the migration debt,
and move the implementation toward this contract deliberately. Do not silently
invent a third pattern.

## Foundation

The authoritative configuration is the pair of identical
`apps/web/components.json` and `apps/docs/components.json` files:

- shadcn with Base UI primitives
- Nova style (`base-nova`)
- Neutral base color
- Amber product primary in `apps/web/src/styles.css`
- Hugeicons
- Default translucent menus with subtle menu accents

The web app uses DM Sans for interface text and JetBrains Mono for machine data.
It supports light, dark, and system appearance through `next-themes`. Component
geometry and color palettes are not runtime-selectable.

Use semantic tokens from `apps/web/src/styles.css`. Never introduce hard-coded
Tailwind colors for product UI. `success`, `warning`, `info`, and `destructive`
communicate meaning; `primary` communicates the main action or selected state.
Do not use semantic colors as decoration.

## Component selection

Use this order for every new UI need:

1. Check `apps/web/src/components/ui` for an installed shadcn component.
2. Check the shadcn registry for the configured Base UI and Nova variant.
3. Install the registry component and keep both `components.json` files aligned.
4. Compose a small product-level component only when the registry primitives do
   not express the complete recurring product pattern.

Do not hand-roll dialogs, menus, popovers, selects, comboboxes, drawers, tabs,
tables, tooltips, form controls, toasts, or other accessible primitives.
Preserve the registry component's semantics, keyboard behavior, data slots, and
`cn-*` hooks. Product components may compose primitives; they must not fork a
second primitive library.

Use the established libraries for their specific jobs:

- Base UI through shadcn for accessible primitives.
- TanStack Router for file-based routing in the operator web app.
- TanStack Query for server state.
- Zustand for genuinely shared client-only state.
- React Hook Form with Zod for forms.
- TanStack Table v9 for every product data table. Define domain columns with
  `ColumnDef`, render them through the shared shadcn data-table component, and
  register only the features that the shared table contract uses.
- TanStack Virtual only for the build-log workbench.
- cmdk for the command palette.
- Sonner for transient feedback.
- Shiki for the approved, lazy ANSI log-rendering boundary and Fumadocs code
  blocks in the static documentation site.

Do not add another component, form, state, table, virtualization, notification,
or syntax-highlighting library without a measured gap and an issue recording
the decision. Existing TanStack Virtual usage remains intentional even though
other general-purpose virtualization libraries exist; dependency churn is not
a design improvement.

Use `HugeiconsIcon` with icons from `@hugeicons/core-free-icons`. Do not add
Lucide, inline SVG icons, or one-off icon systems.

## Experience families

Oore has three related but intentionally different experience families.

### Operator workspace

The operator app owns a fixed sidebar and top header. The viewport itself does
not scroll; the content body below the header is the single scroll owner.
Routes render inside that body and never create a competing page-level scroll
container unless the screen is an explicit workbench.

### QA release portal

QA is a focused release and installation portal, not a restricted copy of the
operator app. It uses a compact sticky header, an app picker, an avatar-only
account menu, and no operator sidebar. Release, installation, checks, and
diagnostics reuse the same tokens and core components as the operator app.

### Onboarding and access

Instance discovery, login, auth callback, and setup use a narrow, centered
focused-flow shell outside authenticated product chrome. They may have their own
step navigation but must retain the same fields, feedback, type, tokens, and
motion rules.

## Page anatomy

### `PageLayout`

Use the shared `PageLayout` for operator pages:

| Width     | Use                                          |
| --------- | -------------------------------------------- |
| `narrow`  | Focused actions and short forms              |
| `default` | Ordinary details and focused forms           |
| `wide`    | Collections, primary settings, dense details |
| `full`    | Workbenches such as build logs               |

Do not choose `wide` merely because space is available. Top-level settings
screens use `wide` so their page edges align across the settings family; focused
forms and details remain readable at `default` width. A route may use a
family-owned layout instead of `PageLayout` only for QA, onboarding, or a true
full-width workbench.

Use `fill` for primary inventory screens. It gives the result frame the
remaining content-body height while preserving the page gutter; the viewport
still does not become the scroll owner.

### `PageHeader`

The page header owns entity identity, concise context or status, and primary
page actions. Titles use sentence case. A page description is allowed only when
the title is an entity name and the description is that entity's identity or
status: a project's description, a pipeline's trigger summary, a source's host.
A collection or settings screen has no page description; its title is enough.
Put status in `meta`, not in the description.

Headers have no divider by default. Use `divided` only when a boundary is
required by the surrounding layout, not as routine page decoration.

Primary creation actions belong in the page header. Section-level actions
belong beside that section's title. Use `View all`, not `View all projects` or
other wording that repeats the visible section heading.

### Sections and cards

Use whitespace, headings, and separators for ordinary page structure. A `Card`
represents a contained object, summary, or action surface. Do not wrap every
section in a card and do not stack decorative cards to manufacture hierarchy.

Settings pages place each section heading outside one restrained bordered
surface. Related `Item` rows share that surface with internal separators; form
sections share one inset field surface and divide their subsections internally.
Do not stack a `Card` for every setting, and do not leave settings controls
floating without a visible group anchor.

Section headings use one compact treatment across a screen. Counts use the
same compact badge or count treatment; do not repeat the unit in the count.

### Headings and hints

Every level of the page can carry help text, and a screen that fills all of
them reads as an article instead of a settings screen. Each level has one job:

| Level       | Primitive                                | Help allowed                                        | May contain                                                         |
| ----------- | ---------------------------------------- | --------------------------------------------------- | ------------------------------------------------------------------- |
| Page        | `PageHeader`                             | A description only when the title is an entity name | That entity's identity or status                                    |
| Section     | `SettingsSection`, `PipelineFormSection` | A `hint`: one sentence behind an info icon          | A rule governing every control: "Running builds are never removed." |
| Subsection  | `PipelineFormSubsection`                 | A `hint`, same as a section                         |                                                                     |
| Field       | `FormDescription`                        | Only if label plus placeholder cannot carry it      | Format, unit, limit, consequence                                    |
| Empty state | `Empty`                                  | One visible sentence, the why                       | Why you would add the first one, with a Learn more link             |
| Footnote    | Muted paragraph under a surface          | One visible sentence                                | Where it is stored, what overrides what                             |

Section and subsection headings never carry visible prose. Help for a heading
goes in `hint`, which renders an info icon that shows the sentence on hover
and on keyboard focus. The heading row stays one line and the sentence is
there for whoever wants it. Live state is different from help: a mode, a
count, or a signing summary goes in `meta` beside the title as a badge or
muted text, and a sentence that describes what a toggle will do with the
current form values goes in `status` under the title. Neither slot takes
static copy.

A hint earns its place only if it passes all three tests. Otherwise delete
it or move it down to the lowest level that still applies:

1. Cover it with your thumb. If the title, the controls, and their labels still
   tell you what to do, delete it.
2. If it could be generated from the title ("Configure retention", "Machines
   that run builds"), delete it.
3. If it is about one field, it is a `FormDescription`. If it is about why you
   would want the feature, it is an empty state or a docs link. If it is about
   storage or precedence, it is a footnote. Only a rule that governs the whole
   section stays at section level.

A hint that survives is one sentence of about 90 characters, says what happens
or what the rule is rather than what the control is for, and never explains
how Oore implements something. Implementation detail belongs in the docs
site, linked once from the empty state or footnote with `Learn more`.
`SettingsSection` and `PipelineFormSubsection` warn in development when a
hint exceeds 120 characters.

Use breadcrumbs only when they express meaningful entity hierarchy. Do not use
them as a generic screen label.

## Collections

A collection screen has one chassis:

1. `PageHeader` with identity and creation action.
2. Collection controls with search, filters, sort, and clear.
3. A result frame that owns loading, refresh, error, empty, and filtered-empty
   states.
4. One responsive record representation.
5. Pagination when results are server-paginated.

Routes own queries, mutations, permissions, navigation, and URL state. The
shared `DataTable` owns the toolbar, table frame, column visibility, empty row,
and pagination. Domain files define data, columns, status, links, and actions.

Keep collection controls in one toolbar row. When search plus several filters
would wrap at an intermediate width, retain search and collapse the filters into
one contained disclosure toggled by a shadcn `Button` rather than scattering
controls across multiple rows. Data tables use sortable column headers.

Use the shared shadcn and TanStack `DataTable` for each tabular dataset at every
breakpoint. Do not render a second compact record tree for the same dataset. Do
not map `TableHead`, `TableCell`, or `TableRow` in routes. Do not turn every list
into a table.

The shared component follows the shadcn data-table composition: a toolbar, one
`rounded-md` bordered table surface, and pagination below it. Routes must not
add another frame, scroll owner, sticky header, column metadata style, hidden
responsive column, loading row renderer, or row-prop extension.

Pagination uses the default previous and next controls. Table row actions use
the shadcn dropdown-menu pattern: an icon-size ghost trigger, an `Actions`
label, grouped items, and a content width that keeps every action on one line.

Render one `DataTable` tree for a tabular dataset. Preserve one semantic reading
and focus order across breakpoints and refreshes.

Server-paginated collections do not need virtualization. Users may extend the
chassis with selection and bulk actions without creating a separate collection
system.

## Details and forms

Entity details lead with `PageHeader`. Tabs represent peer views, not ordinary
sections. Use separator-first sections for normal content and reserve side
panels for stable contextual summaries or actions. Side panels become inline
content on small screens and must not introduce a competing scroll owner.

Dense workbenches may use full width. Ordinary details use `default` or `wide`
width based on the information, not the viewport.

Forms use shadcn Form with React Hook Form and Zod. Prefer one readable column;
group fields into consistent domain-owned sections. Routes coordinate loading,
mutation, permissions, and navigation. Domain sections own fields and behavior.
Shared form components own anatomy and async presentation.

Show field validation inline. Use `Alert` for persistent request failures and
Sonner for transient success. Keep entered data while refreshing or retrying.
Disable only the action being mutated, not the whole page. Put Save and Cancel
at the end; use sticky actions only for genuinely long dirty forms.

Settings forms follow one rule so every screen saves the same way:

- `FormActions` sits directly after the form's surface, never inside it or
  mid-page.
- The submit label is `Save changes` when editing existing settings. Creating
  something uses a verb for the object, such as `Create channel`.
- Save stays disabled until the form is dirty, and the form resets to the saved
  values after a successful save.
- Selects inside forms are full width (`SelectTrigger className="w-full"`).
- A screen has at most one filled primary action. A single-collection screen
  puts its creation action in `PageHeader`; a screen with several peer
  sections gives each section an outline action beside its title.

Place destructive actions in a final, separated danger section and confirm them
with `AlertDialog`.

## Feedback states

Query-backed screens must distinguish:

- initial loading
- background refresh
- request failure
- true empty
- filtered empty
- populated

Use `Skeleton` when content shape is known and `Spinner` for compact
indeterminate activity. Use `Alert` for persistent problems that remain relevant
on the page. Use Sonner for short-lived confirmation. Never present a failed
request as an empty collection.

Warnings and errors must be proportional. Reserve strong destructive treatment
for actionable failures or structured failure data. Use compact `Item` rows for
lists of alerts rather than oversized banners.

## Build log workbench

The operator and QA experiences share one log core.

- Desktop uses a two-pane workbench: an `Item`-based step list in a
  `ScrollArea`, then the log viewport.
- Mobile uses a shadcn/Base UI `Select` for step navigation.
- The viewport is the workbench's one internal scroll region.
- Search shows match count and previous/next controls.
- Wrap, follow live, jump to latest, and raw download are explicit actions.
- Scrolling away pauses follow; jumping to latest restores it.
- Browser find remains browser-owned.
- Desktop actions use text labels where space allows. Icon-only actions require
  both a Tooltip and an accessible name.

Keep TanStack Virtual. Render only visible ANSI rows with Shiki's public
`tokenizeAnsiWithTheme()` and one `createCssVariablesTheme()`. Keep raw log text
as the source of truth and retain a plain-text fallback. Do not add a Shiki
highlighter, HTML renderer, stream package, worker, or full-log token cache
without new measurements proving the need.

Structured failures may receive strong emphasis. Heuristic matches use a
restrained gutter marker rather than recoloring an entire row. Handle loading,
streaming, paused follow, disconnected, unavailable, terminal-without-logs,
empty-step, and no-match states explicitly.

## QA release and install flows

The selected app lives in the QA header; do not repeat it as a decorative hero.
The release hub prioritizes one current testable release, then newer build
activity, checks, and earlier releases. Platform actions are square icon buttons
on compact screens and labeled buttons on wider screens.

Install detail aligns app, version, platform, readiness, release notes, and the
install action. Platform selection uses an explicit Toggle Group selected state.
Guidance must match the platform and device. Desktop may use a contained install
panel; mobile uses a sticky install action when scrolling would otherwise hide
the task.

Diagnostics are secondary and collapsed by default. They reuse the shared log
core with QA-appropriate controls and omit operator-only infrastructure data.

Distinguish no apps, no builds, no installable artifact, expired artifact,
incompatible device, loading, refresh, and request failure.

## Responsive and accessible behavior

Start with one content hierarchy and adapt it; do not build unrelated mobile and
desktop screens. Preserve semantic order when layout changes. Use the existing
small-screen touch target rules and keep controls at least 44 pixels on coarse
or compact inputs.

Every interactive control needs a visible purpose, keyboard access, focus
treatment, and an accessible name. Do not nest interactive elements. Use
Tooltip to explain unfamiliar icon actions, never as the only accessible label.

Truncation must preserve access to the full value where the value matters.
Machine identifiers use tabular or monospace treatment. Commit identifiers are
links when a repository URL is available.

## Motion

Motion is short, functional, and secondary to hierarchy. Prefer CSS transitions
for simple state changes. Use a motion library only for interaction that
requires springs, gesture values, coordinated layout, or exit animation.

Respect `prefers-reduced-motion`. Do not use looping decoration, large travel,
or motion as the only indication of state. Live-status animation must be subtle
and motion-safe.

## Performance and validation

Keep optional features lazy. Collection convergence must reduce duplicate
mounted record trees. Streaming logs must not regroup or retokenize the full
history for every batch.

Before handing off a frontend migration:

1. Exercise initial loading, refresh, error, true-empty, filtered-empty, and
   populated states where applicable.
2. Check representative desktop and compact widths.
3. Verify keyboard navigation, focus order, and accessible names.
4. Confirm one scroll owner and one mounted collection representation.
5. Run the focused static and build checks relevant to the change.
6. Run `make validate` before final handoff.

Do not create broad visual or DOM-presence tests merely to prove that markup
exists. Prefer focused behavior checks and representative manual acceptance.
