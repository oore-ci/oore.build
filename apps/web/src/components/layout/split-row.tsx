import { mergeProps } from '@base-ui/react/merge-props'
import { useRender } from '@base-ui/react/use-render'

import { splitRowVariants } from '@/lib/recipes'
import { cn } from '@/lib/utils'

/** Stacks on phones; one row with the ends pushed apart from `sm` up. */
export function SplitRow({
  className,
  render,
  ...props
}: useRender.ComponentProps<'div'>) {
  return useRender({
    defaultTagName: 'div',
    props: mergeProps<'div'>(
      { className: cn(splitRowVariants(), className) },
      props,
    ),
    render,
  })
}
