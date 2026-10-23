import { mergeProps } from '@base-ui/react/merge-props'
import { useRender } from '@base-ui/react/use-render'

import { monoVariants, type MonoVariantProps } from '@/lib/recipes'
import { cn } from '@/lib/utils'

/** Machine data such as identifiers, hashes and paths. Renders a `span`. */
export function Mono({
  className,
  render,
  size,
  ...props
}: useRender.ComponentProps<'span'> & MonoVariantProps) {
  return useRender({
    defaultTagName: 'span',
    props: mergeProps<'span'>(
      { className: cn(monoVariants({ size }), className) },
      props,
    ),
    render,
  })
}
