import { useMountEffect } from '@/hooks/use-mount-effect'

export default function ThemeColorSync() {
  useMountEffect(() => {
    const root = document.documentElement
    const light = document.querySelector<HTMLMetaElement>(
      'meta[name="theme-color"][data-theme="light"]',
    )
    const dark = document.querySelector<HTMLMetaElement>(
      'meta[name="theme-color"][data-theme="dark"]',
    )
    if (!light || !dark) return

    const sync = () => {
      const isDark =
        root.classList.contains('dark') ||
        (!root.classList.contains('light') &&
          window.matchMedia('(prefers-color-scheme: dark)').matches)
      light.media = isDark ? 'not all' : 'all'
      dark.media = isDark ? 'all' : 'not all'
    }
    const observer = new MutationObserver(sync)
    observer.observe(root, { attributes: true, attributeFilter: ['class'] })
    sync()
    return () => observer.disconnect()
  })

  return null
}
