import { mergeProps } from '@base-ui/react/merge-props'
import { useRender } from '@base-ui/react/use-render'
import { cva } from 'class-variance-authority'
import type { VariantProps } from 'class-variance-authority'

import { cn } from '@/lib/utils'

const badgeVariants = cva(
  'cn-badge group/badge inline-flex w-fit shrink-0 items-center justify-center overflow-hidden whitespace-nowrap focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 aria-invalid:border-destructive aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 [&>svg]:pointer-events-none',
  {
    variants: {
      variant: {
        default: 'cn-badge-variant-default',
        secondary: 'cn-badge-variant-secondary',
        destructive: 'cn-badge-variant-destructive',
        outline: 'cn-badge-variant-outline',
        ghost: 'cn-badge-variant-ghost',
        link: 'cn-badge-variant-link',
        success: 'bg-success/12 text-success [a]:hover:bg-success/18',
        warning: 'bg-warning/12 text-warning [a]:hover:bg-warning/18',
        info: 'bg-info/12 text-info [a]:hover:bg-info/18',
      },
    },
    defaultVariants: {
      variant: 'default',
    },
  },
)

export type BadgeVariant = NonNullable<
  VariantProps<typeof badgeVariants>['variant']
>

function Badge({
  className,
  variant = 'default',
  render,
  ...props
}: useRender.ComponentProps<'span'> & VariantProps<typeof badgeVariants>) {
  return useRender({
    defaultTagName: 'span',
    props: mergeProps<'span'>(
      {
        className: cn(badgeVariants({ variant }), className),
      },
      props,
    ),
    render,
    state: {
      slot: 'badge',
      variant,
    },
  })
}

export { Badge }
