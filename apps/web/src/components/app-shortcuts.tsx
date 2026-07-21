import useMatchMedia from '@/hooks/use-match-media'
import { useHotkeys } from '@tanstack/react-hotkeys'
import { useRouter } from '@tanstack/react-router'
import { useTheme } from 'next-themes'

export default function AppShortcuts() {
  const router = useRouter()
  const { theme = 'light', setTheme } = useTheme()

  const [prefersDark] = useMatchMedia(['(prefers-color-scheme: dark)'])

  useHotkeys(
    [
      {
        hotkey: 'B',
        callback: () => {
          void router.navigate({ to: '/builds' })
        },
      },
      {
        hotkey: 'D',
        callback: () => {
          setTheme(
            theme == 'system'
              ? prefersDark
                ? 'light'
                : 'dark'
              : theme == 'dark'
                ? 'light'
                : 'dark',
          )
        },
      },
    ],
    { preventDefault: true },
  )

  return <></>
}
