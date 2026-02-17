//  @ts-check

import { tanstackConfig } from '@tanstack/eslint-config'

export default [
  ...tanstackConfig,
  {
    ignores: [
      // JS config files are not part of the TS project (no allowJs); linting them
      // with type-aware rules triggers parserOptions.project errors.
      'eslint.config.js',
      'prettier.config.js',

      // Generated VitePress artifacts (not source).
      'docs/.vitepress/cache/**',
      'docs/.vitepress/dist/**',
    ],
  },
]
