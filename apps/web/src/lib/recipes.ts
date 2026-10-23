import { cva, type VariantProps } from 'class-variance-authority'

/**
 * Layout and text recipes. Each one returns a plain class string, so the
 * same recipe works from a React component, a `className`, or any other
 * renderer. Keep this file free of framework imports.
 *
 * DESIGN.md, "Layout primitives": three or more utilities repeated in two
 * files become a recipe here.
 */

const gap = {
  0: 'gap-0',
  0.5: 'gap-0.5',
  1: 'gap-1',
  1.5: 'gap-1.5',
  2: 'gap-2',
  3: 'gap-3',
  3.5: 'gap-3.5',
  4: 'gap-4',
  5: 'gap-5',
  6: 'gap-6',
  8: 'gap-8',
  10: 'gap-10',
} as const

/** A vertical flex column. `fill` takes the remaining height of a flex parent. */
export const stackVariants = cva('flex flex-col', {
  variants: {
    gap,
    fill: {
      true: 'min-h-0 min-w-0 flex-1',
      false: '',
    },
  },
  defaultVariants: { fill: false },
})

/** A horizontal flex row with centered items. */
export const rowVariants = cva('flex items-center', {
  variants: { gap },
})

/** Stacks on phones; one row with the ends pushed apart from `sm` up. */
export const splitRowVariants = cva(
  'flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between',
)

/** Secondary text. `muted` for body copy, `caption` for small print. */
export const textVariants = cva('', {
  variants: {
    variant: {
      muted: 'text-sm text-muted-foreground',
      caption: 'text-xs text-muted-foreground',
      mono: 'font-mono text-xs text-muted-foreground',
    },
  },
  defaultVariants: { variant: 'muted' },
})

/** Machine data: identifiers, hashes, commands, paths. */
export const monoVariants = cva('font-mono', {
  variants: {
    size: {
      inherit: '',
      xs: 'text-xs',
      '2xs': 'text-[11px]',
    },
  },
  defaultVariants: { size: 'inherit' },
})

/** Keyboard focus treatment for custom focusable surfaces such as linked rows. */
export const focusRingVariants = cva(
  'outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50',
)

/** A numbered or checked step marker in setup checklists. */
export const stepMarkerVariants = cva(
  'flex size-5 shrink-0 items-center justify-center rounded-full border text-[11px] font-medium tabular-nums',
)

/** The action bar pinned to the bottom of the viewport on phones. */
export const stickyActionBarVariants = cva(
  'fixed inset-x-0 bottom-0 z-40 border-t bg-background/95 px-4 pt-3 pb-[calc(0.75rem+env(safe-area-inset-bottom))] backdrop-blur',
)

export type StackVariantProps = VariantProps<typeof stackVariants>
export type RowVariantProps = VariantProps<typeof rowVariants>
export type TextVariantProps = VariantProps<typeof textVariants>
export type MonoVariantProps = VariantProps<typeof monoVariants>
