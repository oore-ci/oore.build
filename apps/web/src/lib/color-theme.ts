export const COLOR_THEME_STORAGE_KEY = 'oore_color_theme'

const THEME_STYLE_ELEMENT_ID = 'oore-color-theme-vars'

/**
 * Mirrors the Theme picker used by https://ui.shadcn.com/create.
 * The first entry is its Neutral base; the remaining entries are theme overlays.
 */
export const COLOR_THEMES = [
  {
    name: 'neutral',
    title: 'Neutral',
    cssVars: {
      light: {
        background: 'oklch(1 0 0)',
        foreground: 'oklch(0.145 0 0)',
        card: 'oklch(1 0 0)',
        'card-foreground': 'oklch(0.145 0 0)',
        popover: 'oklch(1 0 0)',
        'popover-foreground': 'oklch(0.145 0 0)',
        primary: 'oklch(0.205 0 0)',
        'primary-foreground': 'oklch(0.985 0 0)',
        secondary: 'oklch(0.97 0 0)',
        'secondary-foreground': 'oklch(0.205 0 0)',
        muted: 'oklch(0.97 0 0)',
        'muted-foreground': 'oklch(0.556 0 0)',
        accent: 'oklch(0.97 0 0)',
        'accent-foreground': 'oklch(0.205 0 0)',
        destructive: 'oklch(0.577 0.245 27.325)',
        border: 'oklch(0.922 0 0)',
        input: 'oklch(0.922 0 0)',
        ring: 'oklch(0.708 0 0)',
        'chart-1': 'oklch(0.87 0 0)',
        'chart-2': 'oklch(0.556 0 0)',
        'chart-3': 'oklch(0.439 0 0)',
        'chart-4': 'oklch(0.371 0 0)',
        'chart-5': 'oklch(0.269 0 0)',
        sidebar: 'oklch(0.985 0 0)',
        'sidebar-foreground': 'oklch(0.145 0 0)',
        'sidebar-primary': 'oklch(0.205 0 0)',
        'sidebar-primary-foreground': 'oklch(0.985 0 0)',
        'sidebar-accent': 'oklch(0.97 0 0)',
        'sidebar-accent-foreground': 'oklch(0.205 0 0)',
        'sidebar-border': 'oklch(0.922 0 0)',
        'sidebar-ring': 'oklch(0.708 0 0)',
      },
      dark: {
        background: 'oklch(0.145 0 0)',
        foreground: 'oklch(0.985 0 0)',
        card: 'oklch(0.205 0 0)',
        'card-foreground': 'oklch(0.985 0 0)',
        popover: 'oklch(0.205 0 0)',
        'popover-foreground': 'oklch(0.985 0 0)',
        primary: 'oklch(0.922 0 0)',
        'primary-foreground': 'oklch(0.205 0 0)',
        secondary: 'oklch(0.269 0 0)',
        'secondary-foreground': 'oklch(0.985 0 0)',
        muted: 'oklch(0.269 0 0)',
        'muted-foreground': 'oklch(0.708 0 0)',
        accent: 'oklch(0.269 0 0)',
        'accent-foreground': 'oklch(0.985 0 0)',
        destructive: 'oklch(0.704 0.191 22.216)',
        border: 'oklch(1 0 0 / 10%)',
        input: 'oklch(1 0 0 / 15%)',
        ring: 'oklch(0.556 0 0)',
        'chart-1': 'oklch(0.87 0 0)',
        'chart-2': 'oklch(0.556 0 0)',
        'chart-3': 'oklch(0.439 0 0)',
        'chart-4': 'oklch(0.371 0 0)',
        'chart-5': 'oklch(0.269 0 0)',
        sidebar: 'oklch(0.205 0 0)',
        'sidebar-foreground': 'oklch(0.985 0 0)',
        'sidebar-primary': 'oklch(0.488 0.243 264.376)',
        'sidebar-primary-foreground': 'oklch(0.985 0 0)',
        'sidebar-accent': 'oklch(0.269 0 0)',
        'sidebar-accent-foreground': 'oklch(0.985 0 0)',
        'sidebar-border': 'oklch(1 0 0 / 10%)',
        'sidebar-ring': 'oklch(0.556 0 0)',
      },
    },
  },
  {
    name: 'amber',
    title: 'Amber',
    cssVars: {
      light: {
        primary: 'oklch(0.555 0.163 48.998)',
        'primary-foreground': 'oklch(0.987 0.022 95.277)',
        secondary: 'oklch(0.967 0.001 286.375)',
        'secondary-foreground': 'oklch(0.21 0.006 285.885)',
        'chart-1': 'oklch(0.879 0.169 91.605)',
        'chart-2': 'oklch(0.769 0.188 70.08)',
        'chart-3': 'oklch(0.666 0.179 58.318)',
        'chart-4': 'oklch(0.555 0.163 48.998)',
        'chart-5': 'oklch(0.473 0.137 46.201)',
        'sidebar-primary': 'oklch(0.666 0.179 58.318)',
        'sidebar-primary-foreground': 'oklch(0.987 0.022 95.277)',
      },
      dark: {
        primary: 'oklch(0.473 0.137 46.201)',
        'primary-foreground': 'oklch(0.987 0.022 95.277)',
        secondary: 'oklch(0.274 0.006 286.033)',
        'secondary-foreground': 'oklch(0.985 0 0)',
        'chart-1': 'oklch(0.879 0.169 91.605)',
        'chart-2': 'oklch(0.769 0.188 70.08)',
        'chart-3': 'oklch(0.666 0.179 58.318)',
        'chart-4': 'oklch(0.555 0.163 48.998)',
        'chart-5': 'oklch(0.473 0.137 46.201)',
        'sidebar-primary': 'oklch(0.769 0.188 70.08)',
        'sidebar-primary-foreground': 'oklch(0.279 0.077 45.635)',
      },
    },
  },
  {
    name: 'blue',
    title: 'Blue',
    cssVars: {
      light: {
        primary: 'oklch(0.488 0.243 264.376)',
        'primary-foreground': 'oklch(0.97 0.014 254.604)',
        secondary: 'oklch(0.967 0.001 286.375)',
        'secondary-foreground': 'oklch(0.21 0.006 285.885)',
        'chart-1': 'oklch(0.809 0.105 251.813)',
        'chart-2': 'oklch(0.623 0.214 259.815)',
        'chart-3': 'oklch(0.546 0.245 262.881)',
        'chart-4': 'oklch(0.488 0.243 264.376)',
        'chart-5': 'oklch(0.424 0.199 265.638)',
        'sidebar-primary': 'oklch(0.546 0.245 262.881)',
        'sidebar-primary-foreground': 'oklch(0.97 0.014 254.604)',
      },
      dark: {
        primary: 'oklch(0.424 0.199 265.638)',
        'primary-foreground': 'oklch(0.97 0.014 254.604)',
        secondary: 'oklch(0.274 0.006 286.033)',
        'secondary-foreground': 'oklch(0.985 0 0)',
        'chart-1': 'oklch(0.809 0.105 251.813)',
        'chart-2': 'oklch(0.623 0.214 259.815)',
        'chart-3': 'oklch(0.546 0.245 262.881)',
        'chart-4': 'oklch(0.488 0.243 264.376)',
        'chart-5': 'oklch(0.424 0.199 265.638)',
        'sidebar-primary': 'oklch(0.623 0.214 259.815)',
        'sidebar-primary-foreground': 'oklch(0.97 0.014 254.604)',
      },
    },
  },
  {
    name: 'cyan',
    title: 'Cyan',
    cssVars: {
      light: {
        primary: 'oklch(0.52 0.105 223.128)',
        'primary-foreground': 'oklch(0.984 0.019 200.873)',
        secondary: 'oklch(0.967 0.001 286.375)',
        'secondary-foreground': 'oklch(0.21 0.006 285.885)',
        'chart-1': 'oklch(0.865 0.127 207.078)',
        'chart-2': 'oklch(0.715 0.143 215.221)',
        'chart-3': 'oklch(0.609 0.126 221.723)',
        'chart-4': 'oklch(0.52 0.105 223.128)',
        'chart-5': 'oklch(0.45 0.085 224.283)',
        'sidebar-primary': 'oklch(0.609 0.126 221.723)',
        'sidebar-primary-foreground': 'oklch(0.984 0.019 200.873)',
      },
      dark: {
        primary: 'oklch(0.45 0.085 224.283)',
        'primary-foreground': 'oklch(0.984 0.019 200.873)',
        secondary: 'oklch(0.274 0.006 286.033)',
        'secondary-foreground': 'oklch(0.985 0 0)',
        'chart-1': 'oklch(0.865 0.127 207.078)',
        'chart-2': 'oklch(0.715 0.143 215.221)',
        'chart-3': 'oklch(0.609 0.126 221.723)',
        'chart-4': 'oklch(0.52 0.105 223.128)',
        'chart-5': 'oklch(0.45 0.085 224.283)',
        'sidebar-primary': 'oklch(0.715 0.143 215.221)',
        'sidebar-primary-foreground': 'oklch(0.302 0.056 229.695)',
      },
    },
  },
  {
    name: 'emerald',
    title: 'Emerald',
    cssVars: {
      light: {
        primary: 'oklch(0.508 0.118 165.612)',
        'primary-foreground': 'oklch(0.979 0.021 166.113)',
        secondary: 'oklch(0.967 0.001 286.375)',
        'secondary-foreground': 'oklch(0.21 0.006 285.885)',
        'chart-1': 'oklch(0.845 0.143 164.978)',
        'chart-2': 'oklch(0.696 0.17 162.48)',
        'chart-3': 'oklch(0.596 0.145 163.225)',
        'chart-4': 'oklch(0.508 0.118 165.612)',
        'chart-5': 'oklch(0.432 0.095 166.913)',
        'sidebar-primary': 'oklch(0.596 0.145 163.225)',
        'sidebar-primary-foreground': 'oklch(0.979 0.021 166.113)',
      },
      dark: {
        primary: 'oklch(0.432 0.095 166.913)',
        'primary-foreground': 'oklch(0.979 0.021 166.113)',
        secondary: 'oklch(0.274 0.006 286.033)',
        'secondary-foreground': 'oklch(0.985 0 0)',
        'chart-1': 'oklch(0.845 0.143 164.978)',
        'chart-2': 'oklch(0.696 0.17 162.48)',
        'chart-3': 'oklch(0.596 0.145 163.225)',
        'chart-4': 'oklch(0.508 0.118 165.612)',
        'chart-5': 'oklch(0.432 0.095 166.913)',
        'sidebar-primary': 'oklch(0.696 0.17 162.48)',
        'sidebar-primary-foreground': 'oklch(0.262 0.051 172.552)',
      },
    },
  },
  {
    name: 'fuchsia',
    title: 'Fuchsia',
    cssVars: {
      light: {
        primary: 'oklch(0.518 0.253 323.949)',
        'primary-foreground': 'oklch(0.977 0.017 320.058)',
        secondary: 'oklch(0.967 0.001 286.375)',
        'secondary-foreground': 'oklch(0.21 0.006 285.885)',
        'chart-1': 'oklch(0.833 0.145 321.434)',
        'chart-2': 'oklch(0.667 0.295 322.15)',
        'chart-3': 'oklch(0.591 0.293 322.896)',
        'chart-4': 'oklch(0.518 0.253 323.949)',
        'chart-5': 'oklch(0.452 0.211 324.591)',
        'sidebar-primary': 'oklch(0.591 0.293 322.896)',
        'sidebar-primary-foreground': 'oklch(0.977 0.017 320.058)',
      },
      dark: {
        primary: 'oklch(0.452 0.211 324.591)',
        'primary-foreground': 'oklch(0.977 0.017 320.058)',
        secondary: 'oklch(0.274 0.006 286.033)',
        'secondary-foreground': 'oklch(0.985 0 0)',
        'chart-1': 'oklch(0.833 0.145 321.434)',
        'chart-2': 'oklch(0.667 0.295 322.15)',
        'chart-3': 'oklch(0.591 0.293 322.896)',
        'chart-4': 'oklch(0.518 0.253 323.949)',
        'chart-5': 'oklch(0.452 0.211 324.591)',
        'sidebar-primary': 'oklch(0.667 0.295 322.15)',
        'sidebar-primary-foreground': 'oklch(0.977 0.017 320.058)',
      },
    },
  },
  {
    name: 'green',
    title: 'Green',
    cssVars: {
      light: {
        primary: 'oklch(0.527 0.154 150.069)',
        'primary-foreground': 'oklch(0.982 0.018 155.826)',
        secondary: 'oklch(0.967 0.001 286.375)',
        'secondary-foreground': 'oklch(0.21 0.006 285.885)',
        'chart-1': 'oklch(0.871 0.15 154.449)',
        'chart-2': 'oklch(0.723 0.219 149.579)',
        'chart-3': 'oklch(0.627 0.194 149.214)',
        'chart-4': 'oklch(0.527 0.154 150.069)',
        'chart-5': 'oklch(0.448 0.119 151.328)',
        'sidebar-primary': 'oklch(0.627 0.194 149.214)',
        'sidebar-primary-foreground': 'oklch(0.982 0.018 155.826)',
      },
      dark: {
        primary: 'oklch(0.448 0.119 151.328)',
        'primary-foreground': 'oklch(0.982 0.018 155.826)',
        secondary: 'oklch(0.274 0.006 286.033)',
        'secondary-foreground': 'oklch(0.985 0 0)',
        'chart-1': 'oklch(0.871 0.15 154.449)',
        'chart-2': 'oklch(0.723 0.219 149.579)',
        'chart-3': 'oklch(0.627 0.194 149.214)',
        'chart-4': 'oklch(0.527 0.154 150.069)',
        'chart-5': 'oklch(0.448 0.119 151.328)',
        'sidebar-primary': 'oklch(0.723 0.219 149.579)',
        'sidebar-primary-foreground': 'oklch(0.982 0.018 155.826)',
      },
    },
  },
  {
    name: 'indigo',
    title: 'Indigo',
    cssVars: {
      light: {
        primary: 'oklch(0.457 0.24 277.023)',
        'primary-foreground': 'oklch(0.962 0.018 272.314)',
        secondary: 'oklch(0.967 0.001 286.375)',
        'secondary-foreground': 'oklch(0.21 0.006 285.885)',
        'chart-1': 'oklch(0.785 0.115 274.713)',
        'chart-2': 'oklch(0.585 0.233 277.117)',
        'chart-3': 'oklch(0.511 0.262 276.966)',
        'chart-4': 'oklch(0.457 0.24 277.023)',
        'chart-5': 'oklch(0.398 0.195 277.366)',
        'sidebar-primary': 'oklch(0.511 0.262 276.966)',
        'sidebar-primary-foreground': 'oklch(0.962 0.018 272.314)',
      },
      dark: {
        primary: 'oklch(0.398 0.195 277.366)',
        'primary-foreground': 'oklch(0.962 0.018 272.314)',
        secondary: 'oklch(0.274 0.006 286.033)',
        'secondary-foreground': 'oklch(0.985 0 0)',
        'chart-1': 'oklch(0.785 0.115 274.713)',
        'chart-2': 'oklch(0.585 0.233 277.117)',
        'chart-3': 'oklch(0.511 0.262 276.966)',
        'chart-4': 'oklch(0.457 0.24 277.023)',
        'chart-5': 'oklch(0.398 0.195 277.366)',
        'sidebar-primary': 'oklch(0.585 0.233 277.117)',
        'sidebar-primary-foreground': 'oklch(0.962 0.018 272.314)',
      },
    },
  },
  {
    name: 'lime',
    title: 'Lime',
    cssVars: {
      light: {
        primary: 'oklch(0.841 0.238 128.85)',
        'primary-foreground': 'oklch(0.405 0.101 131.063)',
        secondary: 'oklch(0.967 0.001 286.375)',
        'secondary-foreground': 'oklch(0.21 0.006 285.885)',
        'chart-1': 'oklch(0.897 0.196 126.665)',
        'chart-2': 'oklch(0.768 0.233 130.85)',
        'chart-3': 'oklch(0.648 0.2 131.684)',
        'chart-4': 'oklch(0.532 0.157 131.589)',
        'chart-5': 'oklch(0.453 0.124 130.933)',
        'sidebar-primary': 'oklch(0.648 0.2 131.684)',
        'sidebar-primary-foreground': 'oklch(0.986 0.031 120.757)',
      },
      dark: {
        primary: 'oklch(0.768 0.233 130.85)',
        'primary-foreground': 'oklch(0.405 0.101 131.063)',
        secondary: 'oklch(0.274 0.006 286.033)',
        'secondary-foreground': 'oklch(0.985 0 0)',
        'chart-1': 'oklch(0.897 0.196 126.665)',
        'chart-2': 'oklch(0.768 0.233 130.85)',
        'chart-3': 'oklch(0.648 0.2 131.684)',
        'chart-4': 'oklch(0.532 0.157 131.589)',
        'chart-5': 'oklch(0.453 0.124 130.933)',
        'sidebar-primary': 'oklch(0.768 0.233 130.85)',
        'sidebar-primary-foreground': 'oklch(0.274 0.072 132.109)',
      },
    },
  },
  {
    name: 'orange',
    title: 'Orange',
    cssVars: {
      light: {
        primary: 'oklch(0.553 0.195 38.402)',
        'primary-foreground': 'oklch(0.98 0.016 73.684)',
        secondary: 'oklch(0.967 0.001 286.375)',
        'secondary-foreground': 'oklch(0.21 0.006 285.885)',
        'chart-1': 'oklch(0.837 0.128 66.29)',
        'chart-2': 'oklch(0.705 0.213 47.604)',
        'chart-3': 'oklch(0.646 0.222 41.116)',
        'chart-4': 'oklch(0.553 0.195 38.402)',
        'chart-5': 'oklch(0.47 0.157 37.304)',
        'sidebar-primary': 'oklch(0.646 0.222 41.116)',
        'sidebar-primary-foreground': 'oklch(0.98 0.016 73.684)',
      },
      dark: {
        primary: 'oklch(0.47 0.157 37.304)',
        'primary-foreground': 'oklch(0.98 0.016 73.684)',
        secondary: 'oklch(0.274 0.006 286.033)',
        'secondary-foreground': 'oklch(0.985 0 0)',
        'chart-1': 'oklch(0.837 0.128 66.29)',
        'chart-2': 'oklch(0.705 0.213 47.604)',
        'chart-3': 'oklch(0.646 0.222 41.116)',
        'chart-4': 'oklch(0.553 0.195 38.402)',
        'chart-5': 'oklch(0.47 0.157 37.304)',
        'sidebar-primary': 'oklch(0.705 0.213 47.604)',
        'sidebar-primary-foreground': 'oklch(0.98 0.016 73.684)',
      },
    },
  },
  {
    name: 'pink',
    title: 'Pink',
    cssVars: {
      light: {
        primary: 'oklch(0.525 0.223 3.958)',
        'primary-foreground': 'oklch(0.971 0.014 343.198)',
        secondary: 'oklch(0.967 0.001 286.375)',
        'secondary-foreground': 'oklch(0.21 0.006 285.885)',
        'chart-1': 'oklch(0.823 0.12 346.018)',
        'chart-2': 'oklch(0.656 0.241 354.308)',
        'chart-3': 'oklch(0.592 0.249 0.584)',
        'chart-4': 'oklch(0.525 0.223 3.958)',
        'chart-5': 'oklch(0.459 0.187 3.815)',
        'sidebar-primary': 'oklch(0.592 0.249 0.584)',
        'sidebar-primary-foreground': 'oklch(0.971 0.014 343.198)',
      },
      dark: {
        primary: 'oklch(0.459 0.187 3.815)',
        'primary-foreground': 'oklch(0.971 0.014 343.198)',
        secondary: 'oklch(0.274 0.006 286.033)',
        'secondary-foreground': 'oklch(0.985 0 0)',
        'chart-1': 'oklch(0.823 0.12 346.018)',
        'chart-2': 'oklch(0.656 0.241 354.308)',
        'chart-3': 'oklch(0.592 0.249 0.584)',
        'chart-4': 'oklch(0.525 0.223 3.958)',
        'chart-5': 'oklch(0.459 0.187 3.815)',
        'sidebar-primary': 'oklch(0.656 0.241 354.308)',
        'sidebar-primary-foreground': 'oklch(0.971 0.014 343.198)',
      },
    },
  },
  {
    name: 'purple',
    title: 'Purple',
    cssVars: {
      light: {
        primary: 'oklch(0.496 0.265 301.924)',
        'primary-foreground': 'oklch(0.977 0.014 308.299)',
        secondary: 'oklch(0.967 0.001 286.375)',
        'secondary-foreground': 'oklch(0.21 0.006 285.885)',
        'chart-1': 'oklch(0.827 0.119 306.383)',
        'chart-2': 'oklch(0.627 0.265 303.9)',
        'chart-3': 'oklch(0.558 0.288 302.321)',
        'chart-4': 'oklch(0.496 0.265 301.924)',
        'chart-5': 'oklch(0.438 0.218 303.724)',
        'sidebar-primary': 'oklch(0.558 0.288 302.321)',
        'sidebar-primary-foreground': 'oklch(0.977 0.014 308.299)',
      },
      dark: {
        primary: 'oklch(0.438 0.218 303.724)',
        'primary-foreground': 'oklch(0.977 0.014 308.299)',
        secondary: 'oklch(0.274 0.006 286.033)',
        'secondary-foreground': 'oklch(0.985 0 0)',
        'chart-1': 'oklch(0.827 0.119 306.383)',
        'chart-2': 'oklch(0.627 0.265 303.9)',
        'chart-3': 'oklch(0.558 0.288 302.321)',
        'chart-4': 'oklch(0.496 0.265 301.924)',
        'chart-5': 'oklch(0.438 0.218 303.724)',
        'sidebar-primary': 'oklch(0.627 0.265 303.9)',
        'sidebar-primary-foreground': 'oklch(0.977 0.014 308.299)',
      },
    },
  },
  {
    name: 'red',
    title: 'Red',
    cssVars: {
      light: {
        primary: 'oklch(0.505 0.213 27.518)',
        'primary-foreground': 'oklch(0.971 0.013 17.38)',
        secondary: 'oklch(0.967 0.001 286.375)',
        'secondary-foreground': 'oklch(0.21 0.006 285.885)',
        'chart-1': 'oklch(0.808 0.114 19.571)',
        'chart-2': 'oklch(0.637 0.237 25.331)',
        'chart-3': 'oklch(0.577 0.245 27.325)',
        'chart-4': 'oklch(0.505 0.213 27.518)',
        'chart-5': 'oklch(0.444 0.177 26.899)',
        'sidebar-primary': 'oklch(0.577 0.245 27.325)',
        'sidebar-primary-foreground': 'oklch(0.971 0.013 17.38)',
      },
      dark: {
        primary: 'oklch(0.444 0.177 26.899)',
        'primary-foreground': 'oklch(0.971 0.013 17.38)',
        secondary: 'oklch(0.274 0.006 286.033)',
        'secondary-foreground': 'oklch(0.985 0 0)',
        'chart-1': 'oklch(0.808 0.114 19.571)',
        'chart-2': 'oklch(0.637 0.237 25.331)',
        'chart-3': 'oklch(0.577 0.245 27.325)',
        'chart-4': 'oklch(0.505 0.213 27.518)',
        'chart-5': 'oklch(0.444 0.177 26.899)',
        'sidebar-primary': 'oklch(0.637 0.237 25.331)',
        'sidebar-primary-foreground': 'oklch(0.971 0.013 17.38)',
      },
    },
  },
  {
    name: 'rose',
    title: 'Rose',
    cssVars: {
      light: {
        primary: 'oklch(0.514 0.222 16.935)',
        'primary-foreground': 'oklch(0.969 0.015 12.422)',
        secondary: 'oklch(0.967 0.001 286.375)',
        'secondary-foreground': 'oklch(0.21 0.006 285.885)',
        'chart-1': 'oklch(0.81 0.117 11.638)',
        'chart-2': 'oklch(0.645 0.246 16.439)',
        'chart-3': 'oklch(0.586 0.253 17.585)',
        'chart-4': 'oklch(0.514 0.222 16.935)',
        'chart-5': 'oklch(0.455 0.188 13.697)',
        'sidebar-primary': 'oklch(0.586 0.253 17.585)',
        'sidebar-primary-foreground': 'oklch(0.969 0.015 12.422)',
      },
      dark: {
        primary: 'oklch(0.455 0.188 13.697)',
        'primary-foreground': 'oklch(0.969 0.015 12.422)',
        secondary: 'oklch(0.274 0.006 286.033)',
        'secondary-foreground': 'oklch(0.985 0 0)',
        'chart-1': 'oklch(0.81 0.117 11.638)',
        'chart-2': 'oklch(0.645 0.246 16.439)',
        'chart-3': 'oklch(0.586 0.253 17.585)',
        'chart-4': 'oklch(0.514 0.222 16.935)',
        'chart-5': 'oklch(0.455 0.188 13.697)',
        sidebar: 'oklch(0.21 0.006 285.885)',
        'sidebar-primary': 'oklch(0.645 0.246 16.439)',
        'sidebar-primary-foreground': 'oklch(0.969 0.015 12.422)',
      },
    },
  },
  {
    name: 'sky',
    title: 'Sky',
    cssVars: {
      light: {
        primary: 'oklch(0.5 0.134 242.749)',
        'primary-foreground': 'oklch(0.977 0.013 236.62)',
        secondary: 'oklch(0.967 0.001 286.375)',
        'secondary-foreground': 'oklch(0.21 0.006 285.885)',
        'chart-1': 'oklch(0.828 0.111 230.318)',
        'chart-2': 'oklch(0.685 0.169 237.323)',
        'chart-3': 'oklch(0.588 0.158 241.966)',
        'chart-4': 'oklch(0.5 0.134 242.749)',
        'chart-5': 'oklch(0.443 0.11 240.79)',
        'sidebar-primary': 'oklch(0.588 0.158 241.966)',
        'sidebar-primary-foreground': 'oklch(0.977 0.013 236.62)',
      },
      dark: {
        primary: 'oklch(0.443 0.11 240.79)',
        'primary-foreground': 'oklch(0.977 0.013 236.62)',
        secondary: 'oklch(0.274 0.006 286.033)',
        'secondary-foreground': 'oklch(0.985 0 0)',
        'chart-1': 'oklch(0.828 0.111 230.318)',
        'chart-2': 'oklch(0.685 0.169 237.323)',
        'chart-3': 'oklch(0.588 0.158 241.966)',
        'chart-4': 'oklch(0.5 0.134 242.749)',
        'chart-5': 'oklch(0.443 0.11 240.79)',
        'sidebar-primary': 'oklch(0.685 0.169 237.323)',
        'sidebar-primary-foreground': 'oklch(0.293 0.066 243.157)',
      },
    },
  },
  {
    name: 'teal',
    title: 'Teal',
    cssVars: {
      light: {
        primary: 'oklch(0.511 0.096 186.391)',
        'primary-foreground': 'oklch(0.984 0.014 180.72)',
        secondary: 'oklch(0.967 0.001 286.375)',
        'secondary-foreground': 'oklch(0.21 0.006 285.885)',
        'chart-1': 'oklch(0.855 0.138 181.071)',
        'chart-2': 'oklch(0.704 0.14 182.503)',
        'chart-3': 'oklch(0.6 0.118 184.704)',
        'chart-4': 'oklch(0.511 0.096 186.391)',
        'chart-5': 'oklch(0.437 0.078 188.216)',
        'sidebar-primary': 'oklch(0.6 0.118 184.704)',
        'sidebar-primary-foreground': 'oklch(0.984 0.014 180.72)',
      },
      dark: {
        primary: 'oklch(0.437 0.078 188.216)',
        'primary-foreground': 'oklch(0.984 0.014 180.72)',
        secondary: 'oklch(0.274 0.006 286.033)',
        'secondary-foreground': 'oklch(0.985 0 0)',
        'chart-1': 'oklch(0.855 0.138 181.071)',
        'chart-2': 'oklch(0.704 0.14 182.503)',
        'chart-3': 'oklch(0.6 0.118 184.704)',
        'chart-4': 'oklch(0.511 0.096 186.391)',
        'chart-5': 'oklch(0.437 0.078 188.216)',
        'sidebar-primary': 'oklch(0.704 0.14 182.503)',
        'sidebar-primary-foreground': 'oklch(0.277 0.046 192.524)',
      },
    },
  },
  {
    name: 'violet',
    title: 'Violet',
    cssVars: {
      light: {
        primary: 'oklch(0.491 0.27 292.581)',
        'primary-foreground': 'oklch(0.969 0.016 293.756)',
        secondary: 'oklch(0.967 0.001 286.375)',
        'secondary-foreground': 'oklch(0.21 0.006 285.885)',
        'chart-1': 'oklch(0.811 0.111 293.571)',
        'chart-2': 'oklch(0.606 0.25 292.717)',
        'chart-3': 'oklch(0.541 0.281 293.009)',
        'chart-4': 'oklch(0.491 0.27 292.581)',
        'chart-5': 'oklch(0.432 0.232 292.759)',
        'sidebar-primary': 'oklch(0.541 0.281 293.009)',
        'sidebar-primary-foreground': 'oklch(0.969 0.016 293.756)',
      },
      dark: {
        primary: 'oklch(0.432 0.232 292.759)',
        'primary-foreground': 'oklch(0.969 0.016 293.756)',
        secondary: 'oklch(0.274 0.006 286.033)',
        'secondary-foreground': 'oklch(0.985 0 0)',
        'chart-1': 'oklch(0.811 0.111 293.571)',
        'chart-2': 'oklch(0.606 0.25 292.717)',
        'chart-3': 'oklch(0.541 0.281 293.009)',
        'chart-4': 'oklch(0.491 0.27 292.581)',
        'chart-5': 'oklch(0.432 0.232 292.759)',
        'sidebar-primary': 'oklch(0.606 0.25 292.717)',
        'sidebar-primary-foreground': 'oklch(0.969 0.016 293.756)',
      },
    },
  },
  {
    name: 'yellow',
    title: 'Yellow',
    cssVars: {
      light: {
        primary: 'oklch(0.852 0.199 91.936)',
        'primary-foreground': 'oklch(0.421 0.095 57.708)',
        secondary: 'oklch(0.967 0.001 286.375)',
        'secondary-foreground': 'oklch(0.21 0.006 285.885)',
        'chart-1': 'oklch(0.905 0.182 98.111)',
        'chart-2': 'oklch(0.795 0.184 86.047)',
        'chart-3': 'oklch(0.681 0.162 75.834)',
        'chart-4': 'oklch(0.554 0.135 66.442)',
        'chart-5': 'oklch(0.476 0.114 61.907)',
        'sidebar-primary': 'oklch(0.681 0.162 75.834)',
        'sidebar-primary-foreground': 'oklch(0.987 0.026 102.212)',
      },
      dark: {
        primary: 'oklch(0.795 0.184 86.047)',
        'primary-foreground': 'oklch(0.421 0.095 57.708)',
        secondary: 'oklch(0.274 0.006 286.033)',
        'secondary-foreground': 'oklch(0.985 0 0)',
        'chart-1': 'oklch(0.905 0.182 98.111)',
        'chart-2': 'oklch(0.795 0.184 86.047)',
        'chart-3': 'oklch(0.681 0.162 75.834)',
        'chart-4': 'oklch(0.554 0.135 66.442)',
        'chart-5': 'oklch(0.476 0.114 61.907)',
        'sidebar-primary': 'oklch(0.795 0.184 86.047)',
        'sidebar-primary-foreground': 'oklch(0.987 0.026 102.212)',
      },
    },
  },
] as const

export type ColorTheme = (typeof COLOR_THEMES)[number]['name']

export const DEFAULT_COLOR_THEME: ColorTheme = 'amber'

export function parseColorTheme(value: unknown): ColorTheme {
  return (
    COLOR_THEMES.find((theme) => theme.name === value)?.name ??
    DEFAULT_COLOR_THEME
  )
}

export function getStoredColorTheme(
  storage: Pick<Storage, 'getItem'> = window.localStorage,
): ColorTheme {
  try {
    return parseColorTheme(storage.getItem(COLOR_THEME_STORAGE_KEY))
  } catch {
    return DEFAULT_COLOR_THEME
  }
}

function buildCssRule(selector: string, cssVars: Record<string, string>) {
  const declarations = Object.entries(cssVars)
    .map(([key, value]) => `  --${key}: ${value};`)
    .join('\n')

  return `${selector} {\n${declarations}\n}\n`
}

function buildThemeCssText(colorTheme: ColorTheme) {
  const baseTheme = COLOR_THEMES[0]
  const theme =
    COLOR_THEMES.find((candidate) => candidate.name === colorTheme) ?? baseTheme

  return [
    buildCssRule(':root', {
      ...baseTheme.cssVars.light,
      ...theme.cssVars.light,
      'sidebar-primary': 'var(--primary)',
      'sidebar-primary-foreground': 'var(--primary-foreground)',
      'primary-hover': 'color-mix(in oklch, var(--primary) 88%, black)',
      surface: 'var(--card)',
    }),
    buildCssRule('.dark', {
      ...baseTheme.cssVars.dark,
      ...theme.cssVars.dark,
      'sidebar-primary': 'var(--primary)',
      'sidebar-primary-foreground': 'var(--primary-foreground)',
      'primary-hover': 'color-mix(in oklch, var(--primary) 88%, white)',
      surface: 'var(--card)',
    }),
  ].join('\n')
}

export function applyColorTheme(
  colorTheme: unknown,
  targetDocument: Document = document,
) {
  let styleElement = targetDocument.getElementById(
    THEME_STYLE_ELEMENT_ID,
  ) as HTMLStyleElement | null

  if (!styleElement) {
    styleElement = targetDocument.createElement('style')
    styleElement.id = THEME_STYLE_ELEMENT_ID
    targetDocument.head.appendChild(styleElement)
  }

  styleElement.textContent = buildThemeCssText(parseColorTheme(colorTheme))

  const view = targetDocument.defaultView
  if (view) {
    view.dispatchEvent(new view.Event('oore-color-theme-change'))
  }
}

export function saveColorTheme(
  colorTheme: ColorTheme,
  storage: Pick<Storage, 'setItem'> = window.localStorage,
  targetDocument: Document = document,
) {
  applyColorTheme(colorTheme, targetDocument)

  try {
    storage.setItem(COLOR_THEME_STORAGE_KEY, colorTheme)
  } catch {
    // The active page can still use the selection when storage is unavailable.
  }
}
