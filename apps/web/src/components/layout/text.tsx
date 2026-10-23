import { mergeProps } from '@base-ui/react/merge-props'
import { useRender } from '@base-ui/react/use-render'

import { textVariants, type TextVariantProps } from '@/lib/recipes'
import { cn } from '@/lib/utils'

/** Secondary copy. Renders a `p`; pass `render={<span />}` for inline text. */
export function Text({
  className,
  render,
  variant,
  ...props
}: useRender.ComponentProps<'p'> & TextVariantProps) {
  return useRender({
    defaultTagName: 'p',
    props: mergeProps<'p'>(
      { className: cn(textVariants({ variant }), className) },
      props,
    ),
    render,
  })
}
