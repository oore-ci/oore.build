import { defineConfig } from 'vitepress'
import { useSidebar } from 'vitepress-openapi'
import spec from '../public/openapi.json' with { type: 'json' }

const siteUrl = 'https://docs.oore.build'
const defaultDescription =
  'Documentation for installing, operating, and integrating Oore CI.'

const openApiSidebar = useSidebar({
  spec,
  linkPrefix: '/openapi/operations/',
})
  .generateSidebarGroups()
  .map((group) => ({ ...group, collapsed: true }))

function canonicalUrl(
  relativePath: string,
  params: Record<string, unknown> = {},
) {
  let path =
    relativePath === 'index.md'
      ? ''
      : relativePath.replace(/\.md$/, '').replace(/\/index$/, '/')

  for (const [key, value] of Object.entries(params)) {
    path = path.replace(`[${key}]`, encodeURIComponent(String(value)))
  }

  return new URL(`/${path}`, siteUrl).toString()
}

export default defineConfig({
  title: 'Oore CI docs',
  description: defaultDescription,
  cleanUrls: true,
  lastUpdated: true,

  vite: {
    build: {
      cssCodeSplit: true,
    },
  },

  sitemap: {
    hostname: siteUrl,
  },

  head: [
    ['link', { rel: 'icon', type: 'image/svg+xml', href: '/logo.svg' }],
    ['link', { rel: 'alternate icon', href: '/favicon.ico' }],
    ['link', { rel: 'apple-touch-icon', href: '/logo192.png' }],
    ['meta', { name: 'theme-color', content: '#2457c5' }],
    ['meta', { property: 'og:type', content: 'website' }],
    ['meta', { property: 'og:image', content: `${siteUrl}/og-image.png` }],
    ['meta', { property: 'og:image:type', content: 'image/png' }],
    ['meta', { property: 'og:image:width', content: '1200' }],
    ['meta', { property: 'og:image:height', content: '630' }],
    ['meta', { name: 'twitter:card', content: 'summary_large_image' }],
    ['meta', { name: 'twitter:image', content: `${siteUrl}/og-image.png` }],
  ],

  transformPageData(pageData) {
    if (pageData.params?.pageTitle) {
      pageData.title = String(pageData.params.pageTitle)
      pageData.frontmatter.title = pageData.title
    }
  },

  transformHead({ pageData }) {
    const url = canonicalUrl(pageData.relativePath, pageData.params)
    const description = pageData.description || defaultDescription
    const title = pageData.title
      ? `${pageData.title} | Oore CI docs`
      : 'Oore CI docs'

    return [
      ['link', { rel: 'canonical', href: url }],
      ['meta', { property: 'og:title', content: title }],
      ['meta', { property: 'og:description', content: description }],
      ['meta', { property: 'og:url', content: url }],
      ['meta', { name: 'twitter:title', content: title }],
      ['meta', { name: 'twitter:description', content: description }],
    ]
  },

  themeConfig: {
    siteTitle: 'Oore CI',
    logo: '/logo.svg',

    nav: [
      { text: 'Get Started', link: '/getting-started/' },
      { text: 'Guides', link: '/guides/' },
      { text: 'Reference', link: '/reference/' },
      { text: 'Operations', link: '/operations/' },
    ],

    sidebar: {
      '/getting-started/': [
        {
          text: 'Get started',
          items: [
            { text: 'Overview', link: '/getting-started/' },
            { text: 'Prerequisites', link: '/getting-started/prerequisites' },
            { text: 'Install', link: '/getting-started/install' },
            {
              text: 'Set up your instance',
              link: '/getting-started/first-instance',
            },
            {
              text: 'Hosted UI onboarding',
              link: '/getting-started/hosted-ui-onboarding',
            },
            { text: 'Connect GitHub', link: '/getting-started/connect-github' },
            {
              text: 'Run your first build',
              link: '/getting-started/first-build',
            },
            {
              text: 'Run your first signed build',
              link: '/getting-started/first-signed-build',
            },
            {
              text: 'Invite your team',
              link: '/getting-started/invite-your-team',
            },
          ],
        },
      ],

      '/guides/': [
        {
          text: 'Guides',
          items: [{ text: 'Overview', link: '/guides/' }],
        },
        {
          text: 'Sources and projects',
          collapsed: false,
          items: [
            { text: 'GitHub App', link: '/guides/integrations/github-app' },
            { text: 'GitLab', link: '/guides/integrations/gitlab' },
            { text: 'Webhooks', link: '/guides/integrations/webhooks' },
            {
              text: 'Create a project',
              link: '/guides/projects/create-project',
            },
            {
              text: 'Pipeline config',
              link: '/guides/projects/pipeline-config',
            },
            {
              text: 'Pipeline via UI',
              link: '/guides/projects/pipeline-ui-fallback',
            },
            { text: 'Trigger builds', link: '/guides/projects/trigger-builds' },
            { text: 'Cancel builds', link: '/guides/projects/cancel-builds' },
          ],
        },
        {
          text: 'Signing and artifacts',
          collapsed: true,
          items: [
            {
              text: 'Android keystore',
              link: '/guides/signing/android-keystore',
            },
            { text: 'Android Gradle', link: '/guides/signing/android-gradle' },
            {
              text: 'iOS certificates',
              link: '/guides/signing/ios-certificates',
            },
            {
              text: 'iOS manual signing',
              link: '/guides/signing/ios-manual-signing',
            },
            {
              text: 'iOS API signing',
              link: '/guides/signing/ios-api-signing',
            },
            {
              text: 'iOS device registration',
              link: '/guides/signing/ios-device-registration',
            },
            {
              text: 'Configure storage',
              link: '/guides/artifacts/configure-storage',
            },
            {
              text: 'Download artifacts',
              link: '/guides/artifacts/download-artifacts',
            },
            {
              text: 'Install mobile builds',
              link: '/guides/artifacts/install-mobile-builds',
            },
          ],
        },
        {
          text: 'Access and operators',
          collapsed: true,
          items: [
            { text: 'OIDC overview', link: '/guides/oidc/' },
            { text: 'Google', link: '/guides/oidc/google' },
            { text: 'Okta', link: '/guides/oidc/okta' },
            { text: 'Microsoft Entra ID', link: '/guides/oidc/azure-ad' },
            { text: 'Auth0', link: '/guides/oidc/auth0' },
            { text: 'Keycloak', link: '/guides/oidc/keycloak' },
            { text: 'Invite users', link: '/guides/users/invite-users' },
            { text: 'Manage roles', link: '/guides/users/manage-roles' },
            { text: 'Disable users', link: '/guides/users/disable-users' },
          ],
        },
        {
          text: 'Runners and instances',
          collapsed: true,
          items: [
            {
              text: 'Direct macOS runner',
              link: '/guides/runners/external-runner',
            },
            {
              text: 'Why embedded execution is unavailable',
              link: '/guides/runners/embedded-runner',
            },
            {
              text: 'Add an instance',
              link: '/guides/multi-instance/add-instance',
            },
            {
              text: 'Switch instances',
              link: '/guides/multi-instance/switch-instances',
            },
          ],
        },
      ],

      '/concepts/': [
        {
          text: 'Concepts',
          items: [
            { text: 'Architecture', link: '/concepts/architecture' },
            { text: 'Build execution', link: '/concepts/build-execution' },
            { text: 'Runner protocol', link: '/concepts/runner-protocol' },
            { text: 'File-first config', link: '/concepts/file-first-config' },
            { text: 'Code signing', link: '/concepts/signing-overview' },
            { text: 'Artifact access', link: '/concepts/artifact-access' },
            { text: 'Multi-instance', link: '/concepts/multi-instance' },
            { text: 'Security model', link: '/concepts/security-model' },
          ],
        },
      ],

      '/reference/': [
        {
          text: 'Reference',
          items: [
            { text: 'Overview', link: '/reference/' },
            { text: 'OpenAPI', link: '/openapi/' },
          ],
        },
        {
          text: 'CLI',
          collapsed: false,
          items: [
            { text: 'Overview', link: '/reference/cli/' },
            { text: 'oore setup', link: '/reference/cli/oore-setup' },
            { text: 'oore doctor', link: '/reference/cli/oore-doctor' },
            { text: 'oore login', link: '/reference/cli/oore-login' },
            { text: 'oore recovery', link: '/reference/cli/oore-recovery' },
            { text: 'oore status', link: '/reference/cli/oore-status' },
            { text: 'oore config', link: '/reference/cli/oore-config' },
          ],
        },
        {
          text: 'Configuration',
          collapsed: true,
          items: [
            { text: '.oore.yaml', link: '/reference/config/oore-yaml' },
            { text: 'Installer', link: '/reference/config/installer' },
            {
              text: 'Environment variables',
              link: '/reference/config/environment-variables',
            },
            {
              text: 'Daemon config',
              link: '/reference/config/daemon-config',
            },
          ],
        },
        {
          text: 'Platform',
          collapsed: true,
          items: [
            { text: 'Setup states', link: '/reference/setup-states' },
            { text: 'Build states', link: '/reference/build-states' },
            { text: 'Roles and permissions', link: '/reference/rbac' },
            { text: 'Error codes', link: '/reference/error-codes' },
          ],
        },
      ],

      '/openapi/': [
        {
          text: 'OpenAPI',
          items: [{ text: 'Overview', link: '/openapi/' }, ...openApiSidebar],
        },
      ],

      '/operations/': [
        {
          text: 'Operations',
          items: [{ text: 'Overview', link: '/operations/' }],
        },
        {
          text: 'Deploy',
          collapsed: false,
          items: [
            { text: 'Deployment', link: '/operations/deployment' },
            {
              text: 'Split backend and frontend',
              link: '/operations/split-roles',
            },
            {
              text: 'Mac Studio, NetBird, and Warpgate',
              link: '/operations/mac-studio-netbird-warpgate',
            },
          ],
        },
        {
          text: 'Maintain',
          collapsed: false,
          items: [
            { text: 'Upgrade', link: '/operations/upgrade' },
            { text: 'Backup and restore', link: '/operations/backup-restore' },
            { text: 'Monitoring', link: '/operations/monitoring' },
            {
              text: 'Release automation',
              link: '/operations/release-automation-mac-mini',
            },
          ],
        },
        {
          text: 'Support',
          collapsed: true,
          items: [
            { text: 'Troubleshooting', link: '/operations/troubleshooting' },
            { text: 'Clean reinstall', link: '/operations/clean-reinstall' },
            {
              text: 'Known limitations',
              link: '/operations/known-limitations',
            },
            { text: 'Report an issue', link: '/operations/report-an-issue' },
            { text: 'Alpha feedback', link: '/operations/alpha-feedback' },
            { text: 'Release channels', link: '/operations/release-channels' },
          ],
        },
      ],
    },

    socialLinks: [
      { icon: 'github', link: 'https://github.com/oore-ci/oore.build' },
    ],

    editLink: {
      pattern:
        'https://github.com/oore-ci/oore.build/edit/master/apps/docs-site/docs/:path',
      text: 'Edit this page on GitHub',
    },

    footer: {
      message: 'Self-hosted mobile CI for Flutter teams.',
      copyright: 'Copyright © 2026 oore.build',
    },

    search: { provider: 'local' },
    outline: { level: [2, 3] },
  },
})
