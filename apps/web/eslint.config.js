//  @ts-check

import { tanstackConfig } from '@tanstack/eslint-config'

export default [
  ...tanstackConfig,
  {
    ignores: [
      'eslint.config.js',
      'prettier.config.js',
      'public/mockServiceWorker.js',
      'tools/oore-web.js',
      'tools/oore-web.test.js',
    ],
  },
  {
    rules: {
      'no-restricted-syntax': [
        'error',
        {
          selector: "CallExpression[callee.name='useEffect']",
          message:
            'Direct useEffect is banned. Use useMountEffect for mount-only effects, derive state inline, use event handlers, or use TanStack Query. See: OOR-145',
        },
        {
          selector:
            "CallExpression[callee.object.name='React'][callee.property.name='useEffect']",
          message:
            'Direct React.useEffect is banned. Use useMountEffect for mount-only effects, derive state inline, use event handlers, or use TanStack Query. See: OOR-145',
        },
      ],
    },
  },
  {
    files: [
      '**/use-mount-effect.ts',
      '**/use-breadcrumb-label.ts',
      '**/use-auto-scroll.ts',
      '**/use-build-notification.ts',
      '**/use-session-countdown.ts',
      '**/use-log-stream.ts',
      '**/use-index-auth-guard.ts',
      '**/use-trusted-proxy-auto-login.ts',
      '**/use-setup-route-transitions.ts',
    ],
    rules: {
      'no-restricted-syntax': 'off',
    },
  },
]
