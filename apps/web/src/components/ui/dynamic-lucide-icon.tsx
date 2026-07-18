import type { LucideIcon, LucideProps } from 'lucide-react'

function DynamicLucideIcon({
  icon: Icon,
  ...props
}: LucideProps & { icon: LucideIcon }) {
  return <Icon {...props} />
}

export { DynamicLucideIcon }
