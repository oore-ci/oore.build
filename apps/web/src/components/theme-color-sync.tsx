import { useMountEffect } from '@/hooks/use-mount-effect'

function buildThemedFavicon(color: string) {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><defs><circle id="c" cx="16" cy="16" r="7"/><mask id="m"><rect width="32" height="32" fill="white"/><use href="#c" fill="black"/></mask><clipPath id="l"><rect width="15" height="32"/></clipPath><clipPath id="r"><rect x="17" width="15" height="32"/></clipPath></defs><rect x="2" y="2" width="28" height="28" rx="6" fill="${color}" clip-path="url(#l)" mask="url(#m)"/><rect x="2" y="2" width="28" height="28" rx="6" fill="${color}" clip-path="url(#r)" mask="url(#m)"/></svg>`

  return `data:image/svg+xml,${encodeURIComponent(svg)}`
}

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
      const primary = getComputedStyle(root)
        .getPropertyValue('--primary')
        .trim()
      light.media = isDark ? 'not all' : 'all'
      dark.media = isDark ? 'all' : 'not all'
      if (primary) {
        const activeThemeColor = isDark ? dark : light
        activeThemeColor.content = primary
        const favicon = buildThemedFavicon(primary)
        document
          .querySelectorAll<HTMLLinkElement>('[data-theme-icon]')
          .forEach((link) => (link.href = favicon))
      }
    }
    const observer = new MutationObserver(sync)
    observer.observe(root, { attributes: true, attributeFilter: ['class'] })
    window.addEventListener('oore-color-theme-change', sync)
    sync()
    return () => {
      observer.disconnect()
      window.removeEventListener('oore-color-theme-change', sync)
    }
  })

  return null
}
