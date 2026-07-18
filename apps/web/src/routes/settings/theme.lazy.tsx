import { useState } from 'react'
import type { CSSProperties } from 'react'
import { createLazyFileRoute } from '@tanstack/react-router'
import { useTheme } from 'next-themes'
import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import {
  Moon as Moon02Icon,
  Smartphone as SmartPhone01Icon,
  Sun as Sun03Icon,
  Check as Tick02Icon,
} from 'lucide-react'

import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import { Button } from '@/components/ui/button'
import { Separator } from '@/components/ui/separator'
import {
  COLOR_THEMES,
  getStoredColorTheme,
  saveColorTheme,
} from '@/lib/color-theme'
import type { ColorTheme } from '@/lib/color-theme'
import {
  COMPONENT_STYLES,
  getStoredComponentStyle,
  saveComponentStyle,
} from '@/lib/component-style'
import type { ComponentStyle } from '@/lib/component-style'
import { PageMeta } from '@/lib/seo'
import { cn } from '@/lib/utils'

export const Route = createLazyFileRoute('/settings/theme')({
  component: ThemeSettingsPage,
})

const THEME_MODES = [
  { value: 'light', label: 'Light', icon: Sun03Icon },
  { value: 'dark', label: 'Dark', icon: Moon02Icon },
  { value: 'system', label: 'System', icon: SmartPhone01Icon },
] as const

function ThemeSettingsPage() {
  const { theme, setTheme } = useTheme()
  const [colorTheme, setColorTheme] = useState(getStoredColorTheme)
  const [componentStyle, setComponentStyle] = useState(getStoredComponentStyle)
  const activeMode = theme ?? 'system'

  function selectColorTheme(nextColorTheme: ColorTheme) {
    saveColorTheme(nextColorTheme)
    setColorTheme(nextColorTheme)
  }

  async function selectComponentStyle(nextComponentStyle: ComponentStyle) {
    await saveComponentStyle(nextComponentStyle)
    setComponentStyle(nextComponentStyle)
  }

  return (
    <PageLayout width="narrow">
      <PageMeta title="Theme" noindex />
      <PageHeader
        title="Theme"
        description="Choose how Oore looks on this browser."
      />

      <section className="flex flex-col gap-3" aria-labelledby="style-heading">
        <div className="flex flex-col gap-1">
          <h2 id="style-heading" className="text-sm font-semibold">
            Style
          </h2>
          <p className="text-sm text-muted-foreground">
            Changes component shape, spacing, and density.
          </p>
        </div>

        <div
          role="radiogroup"
          aria-label="Style"
          className="grid gap-2 sm:grid-cols-2"
        >
          {COMPONENT_STYLES.map((option) => {
            const selected = componentStyle === option.name

            return (
              <Button
                key={option.name}
                type="button"
                variant="outline"
                role="radio"
                aria-checked={selected}
                onClick={() => void selectComponentStyle(option.name)}
                className={cn(
                  'h-auto min-h-14 justify-start gap-3 px-3 py-3 text-left shadow-none',
                  selected &&
                    'border-primary bg-primary/8 text-foreground ring-2 ring-primary/15 hover:bg-primary/12',
                )}
              >
                <span className="flex min-w-0 flex-col items-start">
                  <span>{option.title}</span>
                  <span className="text-xs font-normal text-muted-foreground">
                    {option.description}
                  </span>
                </span>
                {selected ? (
                  <DynamicLucideIcon
                    icon={Tick02Icon}
                    className="ml-auto text-primary"
                    aria-hidden
                  />
                ) : null}
              </Button>
            )
          })}
        </div>
      </section>

      <Separator />

      <section
        className="flex flex-col gap-3"
        aria-labelledby="color-set-heading"
      >
        <div className="flex flex-col gap-1">
          <h2 id="color-set-heading" className="text-sm font-semibold">
            Color set
          </h2>
          <p className="text-sm text-muted-foreground">
            Changes actions, links, focus, and selected navigation.
          </p>
        </div>

        <div
          role="radiogroup"
          aria-label="Color set"
          className="grid gap-2 sm:grid-cols-2"
        >
          {COLOR_THEMES.map((option) => {
            const selected = colorTheme === option.name

            return (
              <Button
                key={option.name}
                type="button"
                variant="outline"
                role="radio"
                aria-checked={selected}
                onClick={() => selectColorTheme(option.name)}
                className={cn(
                  'h-auto min-h-14 justify-start gap-3 px-3 py-3 text-left shadow-none',
                  selected &&
                    'border-primary bg-primary/8 text-foreground ring-2 ring-primary/15 hover:bg-primary/12',
                )}
              >
                <span
                  style={
                    {
                      '--color': option.cssVars.dark.primary,
                    } as CSSProperties
                  }
                  className="size-6 shrink-0 border border-foreground/15 bg-(--color)"
                  aria-hidden
                />
                <span>{option.title}</span>
                {selected ? (
                  <DynamicLucideIcon
                    icon={Tick02Icon}
                    className="ml-auto text-primary"
                    aria-hidden
                  />
                ) : null}
              </Button>
            )
          })}
        </div>
      </section>

      <Separator />

      <section
        className="flex flex-col gap-3"
        aria-labelledby="theme-mode-heading"
      >
        <div className="flex flex-col gap-1">
          <h2 id="theme-mode-heading" className="text-sm font-semibold">
            Mode
          </h2>
          <p className="text-sm text-muted-foreground">
            Use light, dark, or your device setting.
          </p>
        </div>

        <div
          role="radiogroup"
          aria-label="Theme mode"
          className="flex flex-wrap gap-2"
        >
          {THEME_MODES.map((option) => {
            const selected = activeMode === option.value

            return (
              <Button
                key={option.value}
                type="button"
                variant={selected ? 'default' : 'outline'}
                role="radio"
                aria-checked={selected}
                onClick={() => setTheme(option.value)}
                className="min-w-28 justify-start"
              >
                <DynamicLucideIcon icon={option.icon} aria-hidden />
                {option.label}
              </Button>
            )
          })}
        </div>
      </section>
    </PageLayout>
  )
}
