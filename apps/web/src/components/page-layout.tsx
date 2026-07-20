import { cn } from '@/lib/utils'

const WIDTH_CLASSES = {
  default: 'max-w-5xl',
  narrow: 'max-w-2xl',
  wide: 'max-w-7xl',
  full: '',
} as const

interface PageLayoutProps {
  children: React.ReactNode
  width?: keyof typeof WIDTH_CLASSES
  className?: string
}

export default function PageLayout({
  children,
  width = 'default',
  className,
}: PageLayoutProps) {
  return (
    <div
      className={cn(
        WIDTH_CLASSES[width],
        'mx-auto w-full min-w-0 space-y-5 px-4 py-5 sm:p-6 lg:p-8',
        className,
      )}
    >
      {children}
    </div>
  )
}
