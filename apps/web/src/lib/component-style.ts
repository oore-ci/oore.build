const COMPONENT_STYLE_STORAGE_KEY = 'oore_component_style'
const COMPONENT_STYLE_CLASS_PREFIX = 'style-'

// Keep these styles and their cn-* component hooks source-identical to
// shadcn/ui commit d28738b183c5eaa69d8d540826e450f30d39ab6c.
// Runtime switching follows Create: one managed style-* class on document.body.

export const COMPONENT_STYLES = [
  {
    name: 'vega',
    title: 'Vega',
    description: 'Clean and familiar',
    radius: '0.625rem',
  },
  {
    name: 'nova',
    title: 'Nova',
    description: 'Reduced spacing',
    radius: '0.625rem',
  },
  {
    name: 'maia',
    title: 'Maia',
    description: 'Soft and spacious',
    radius: '0.625rem',
  },
  {
    name: 'lyra',
    title: 'Lyra',
    description: 'Boxy and sharp',
    radius: '0rem',
  },
  {
    name: 'mira',
    title: 'Mira',
    description: 'Compact and dense',
    radius: '0.625rem',
  },
  {
    name: 'luma',
    title: 'Luma',
    description: 'Fluid and soft',
    radius: '0.625rem',
  },
  {
    name: 'sera',
    title: 'Sera',
    description: 'Editorial',
    radius: '0rem',
  },
  {
    name: 'rhea',
    title: 'Rhea',
    description: 'Soft and compact',
    radius: '0.625rem',
  },
] as const

export type ComponentStyle = (typeof COMPONENT_STYLES)[number]['name']

export const DEFAULT_COMPONENT_STYLE: ComponentStyle = 'vega'

const COMPONENT_STYLE_LOADERS: Record<ComponentStyle, () => Promise<unknown>> =
  {
    vega: () => import('../styles/shadcn/loaders/style-vega.css'),
    nova: () => import('../styles/shadcn/loaders/style-nova.css'),
    maia: () => import('../styles/shadcn/loaders/style-maia.css'),
    lyra: () => import('../styles/shadcn/loaders/style-lyra.css'),
    mira: () => import('../styles/shadcn/loaders/style-mira.css'),
    luma: () => import('../styles/shadcn/loaders/style-luma.css'),
    sera: () => import('../styles/shadcn/loaders/style-sera.css'),
    rhea: () => import('../styles/shadcn/loaders/style-rhea.css'),
  }

export function parseComponentStyle(value: unknown): ComponentStyle {
  return (
    COMPONENT_STYLES.find((style) => style.name === value)?.name ??
    DEFAULT_COMPONENT_STYLE
  )
}

export function getStoredComponentStyle(
  storage: Pick<Storage, 'getItem'> = window.localStorage,
): ComponentStyle {
  try {
    return parseComponentStyle(storage.getItem(COMPONENT_STYLE_STORAGE_KEY))
  } catch {
    return DEFAULT_COMPONENT_STYLE
  }
}

export function applyComponentStyle(
  componentStyle: unknown,
  targetDocument: Document = document,
) {
  const style = parseComponentStyle(componentStyle)
  for (const className of Array.from(targetDocument.body.classList)) {
    if (className.startsWith(COMPONENT_STYLE_CLASS_PREFIX)) {
      targetDocument.body.classList.remove(className)
    }
  }
  targetDocument.body.classList.add(`${COMPONENT_STYLE_CLASS_PREFIX}${style}`)
  targetDocument.documentElement.style.setProperty(
    '--radius',
    COMPONENT_STYLES.find((option) => option.name === style)?.radius ??
      COMPONENT_STYLES[0].radius,
  )
}

export async function loadComponentStyle(componentStyle: unknown) {
  await COMPONENT_STYLE_LOADERS[parseComponentStyle(componentStyle)]()
}

export async function saveComponentStyle(
  componentStyle: ComponentStyle,
  storage: Pick<Storage, 'setItem'> = window.localStorage,
  targetDocument: Document = document,
) {
  await loadComponentStyle(componentStyle)
  applyComponentStyle(componentStyle, targetDocument)

  try {
    storage.setItem(COMPONENT_STYLE_STORAGE_KEY, componentStyle)
  } catch {
    // The active page can still use the selection when storage is unavailable.
  }
}
