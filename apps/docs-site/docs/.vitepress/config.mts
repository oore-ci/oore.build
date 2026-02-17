import { defineConfig } from "vitepress";
import { useSidebar } from "vitepress-openapi";
import spec from "../public/openapi.json" with { type: "json" };

const openApiSidebar = useSidebar({
  spec,
  linkPrefix: "/openapi/operations/",
})
  .generateSidebarGroups()
  .map((group) => ({ ...group, collapsed: true }));

export default defineConfig({
  title: "Oore CI Docs",
  description:
    "Self-hosted, Flutter-first mobile CI and internal app distribution platform",
  cleanUrls: true,
  sitemap: {
    hostname: 'https://docs.oore.build',
  },

  head: [
    [
      "link",
      {
        rel: "icon",
        type: "image/svg+xml",
        href: "/logo.svg",
      },
    ],
    [
      "link",
      {
        rel: "alternate icon",
        href: "/favicon.ico",
      },
    ],
    [
      "link",
      {
        rel: "apple-touch-icon",
        href: "/logo192.png",
      },
    ],
    [
      "meta",
      {
        name: "theme-color",
        content: "#dc7702",
      },
    ],
    [
      "meta",
      {
        property: "og:title",
        content: "Oore CI Docs",
      },
    ],
    [
      "meta",
      {
        property: "og:description",
        content:
          "Documentation for oore.build — self-hosted, Flutter-first mobile CI.",
      },
    ],
    [
      "meta",
      {
        property: "og:image",
        content: "https://docs.oore.build/og-image.png",
      },
    ],
    [
      "meta",
      {
        property: "og:image:type",
        content: "image/png",
      },
    ],
    [
      "meta",
      {
        property: "og:image:width",
        content: "1200",
      },
    ],
    [
      "meta",
      {
        property: "og:image:height",
        content: "630",
      },
    ],
    [
      "meta",
      {
        property: "og:logo",
        content: "https://docs.oore.build/logo.svg",
      },
    ],
    [
      "meta",
      {
        name: "twitter:card",
        content: "summary_large_image",
      },
    ],
    [
      "meta",
      {
        name: "twitter:title",
        content: "Oore CI Docs",
      },
    ],
    [
      "meta",
      {
        name: "twitter:description",
        content:
          "Documentation for oore.build — self-hosted, Flutter-first mobile CI.",
      },
    ],
    [
      "meta",
      {
        name: "twitter:image",
        content: "https://docs.oore.build/og-image.png",
      },
    ],
    [
      "link",
      {
        rel: "canonical",
        href: "https://docs.oore.build",
      },
    ],
    [
      "meta",
      {
        property: "og:url",
        content: "https://docs.oore.build",
      },
    ],
    [
      "link",
      {
        rel: "preconnect",
        href: "https://fonts.googleapis.com",
      },
    ],
  ],

  themeConfig: {
    siteTitle: "Oore CI",
    logo: "/logo.svg",

    nav: [
      { text: "Getting Started", link: "/getting-started/" },
      { text: "Guides", link: "/guides/oidc/" },
      { text: "Reference", link: "/reference/api/" },
      { text: "OpenAPI", link: "/openapi/" },
      { text: "Concepts", link: "/concepts/architecture" },
      { text: "Operations", link: "/operations/deployment" },
      {
        text: "GitHub",
        link: "https://github.com/devaryakjha/oore.build",
      },
    ],

    sidebar: {
      "/getting-started/": [
        {
          text: "Getting Started",
          items: [
            { text: "What is oore.build?", link: "/getting-started/" },
            { text: "Public Alpha (v0.1.x)", link: "/getting-started/public-alpha" },
            { text: "Prerequisites", link: "/getting-started/prerequisites" },
            { text: "Install", link: "/getting-started/install" },
            {
              text: "Hosted UI Onboarding",
              link: "/getting-started/hosted-ui-onboarding",
            },
            {
              text: "Set Up Your Instance",
              link: "/getting-started/first-instance",
            },
            { text: "Connect GitHub", link: "/getting-started/connect-github" },
            { text: "First Build", link: "/getting-started/first-build" },
            {
              text: "First Signed Build",
              link: "/getting-started/first-signed-build",
            },
            {
              text: "Invite Your Team",
              link: "/getting-started/invite-your-team",
            },
          ],
        },
      ],
      "/guides/": [
        {
          text: "OIDC Authentication",
          items: [
            { text: "Overview", link: "/guides/oidc/" },
            { text: "Google", link: "/guides/oidc/google" },
            { text: "Okta", link: "/guides/oidc/okta" },
            { text: "Azure AD", link: "/guides/oidc/azure-ad" },
            { text: "Auth0", link: "/guides/oidc/auth0" },
            { text: "Keycloak", link: "/guides/oidc/keycloak" },
          ],
        },
        {
          text: "Integrations",
          items: [
            { text: "GitHub App", link: "/guides/integrations/github-app" },
            { text: "GitLab", link: "/guides/integrations/gitlab" },
            { text: "Webhooks", link: "/guides/integrations/webhooks" },
          ],
        },
        {
          text: "Projects",
          items: [
            {
              text: "Create a Project",
              link: "/guides/projects/create-project",
            },
            {
              text: "Pipeline Config (.oore.yaml)",
              link: "/guides/projects/pipeline-config",
            },
            {
              text: "Pipeline via UI",
              link: "/guides/projects/pipeline-ui-fallback",
            },
            { text: "Trigger Builds", link: "/guides/projects/trigger-builds" },
            { text: "Cancel Builds", link: "/guides/projects/cancel-builds" },
          ],
        },
        {
          text: "Signing",
          items: [
            {
              text: "Android Keystore",
              link: "/guides/signing/android-keystore",
            },
            { text: "Android Gradle", link: "/guides/signing/android-gradle" },
            {
              text: "iOS Certificates",
              link: "/guides/signing/ios-certificates",
            },
            {
              text: "iOS Manual Signing",
              link: "/guides/signing/ios-manual-signing",
            },
            { text: "iOS API Signing", link: "/guides/signing/ios-api-signing" },
            {
              text: "iOS Device Registration",
              link: "/guides/signing/ios-device-registration",
            },
          ],
        },
        {
          text: "Runners",
          items: [
            {
              text: "Embedded Runner",
              link: "/guides/runners/embedded-runner",
            },
            {
              text: "External Runner",
              link: "/guides/runners/external-runner",
            },
          ],
        },
        {
          text: "Artifacts",
          items: [
            {
              text: "Configure Storage",
              link: "/guides/artifacts/configure-storage",
            },
            {
              text: "Download Artifacts",
              link: "/guides/artifacts/download-artifacts",
            },
          ],
        },
        {
          text: "Users",
          items: [
            { text: "Invite Users", link: "/guides/users/invite-users" },
            { text: "Manage Roles", link: "/guides/users/manage-roles" },
            { text: "Disable Users", link: "/guides/users/disable-users" },
          ],
        },
        {
          text: "Multi-Instance",
          items: [
            {
              text: "Add Instance",
              link: "/guides/multi-instance/add-instance",
            },
            {
              text: "Switch Instances",
              link: "/guides/multi-instance/switch-instances",
            },
          ],
        },
      ],
      "/reference/": [
        {
          text: "API Reference",
          items: [
            { text: "Overview", link: "/reference/api/" },
            { text: "Setup", link: "/reference/api/setup" },
            { text: "Projects", link: "/reference/api/projects" },
            { text: "Pipelines", link: "/reference/api/pipelines" },
            { text: "Builds", link: "/reference/api/builds" },
            { text: "Integrations", link: "/reference/api/integrations" },
            { text: "Auth", link: "/reference/api/auth" },
            { text: "Users", link: "/reference/api/users" },
            { text: "Build Logs", link: "/reference/api/logs" },
            { text: "Runners", link: "/reference/api/runners" },
            { text: "Artifacts", link: "/reference/api/artifacts" },
            { text: "Settings", link: "/reference/api/settings" },
          ],
        },
        {
          text: "CLI Reference",
          items: [
            { text: "Overview", link: "/reference/cli/" },
            { text: "oore setup", link: "/reference/cli/oore-setup" },
            { text: "oore doctor", link: "/reference/cli/oore-doctor" },
            { text: "oore login", link: "/reference/cli/oore-login" },
            { text: "oore status", link: "/reference/cli/oore-status" },
            { text: "oore config", link: "/reference/cli/oore-config" },
          ],
        },
        {
          text: "Configuration",
          items: [
            { text: ".oore.yaml", link: "/reference/config/oore-yaml" },
            {
              text: "Environment Variables",
              link: "/reference/config/environment-variables",
            },
            {
              text: "Daemon Config",
              link: "/reference/config/daemon-config",
            },
          ],
        },
        {
          text: "State Machines & RBAC",
          items: [
            { text: "Setup States", link: "/reference/setup-states" },
            { text: "Build States", link: "/reference/build-states" },
            { text: "Roles & Permissions", link: "/reference/rbac" },
            { text: "Error Codes", link: "/reference/error-codes" },
          ],
        },
      ],
      "/openapi/": [
        {
          text: "OpenAPI Spec",
          items: [
            { text: "Overview", link: "/openapi/" },
            ...openApiSidebar,
          ],
        },
      ],
      "/concepts/": [
        {
          text: "Concepts",
          items: [
            { text: "Architecture", link: "/concepts/architecture" },
            { text: "Build Execution", link: "/concepts/build-execution" },
            { text: "Runner Protocol", link: "/concepts/runner-protocol" },
            {
              text: "File-First Config",
              link: "/concepts/file-first-config",
            },
            { text: "Code Signing", link: "/concepts/signing-overview" },
            { text: "Artifact Access", link: "/concepts/artifact-access" },
            { text: "Multi-Instance", link: "/concepts/multi-instance" },
            { text: "Security Model", link: "/concepts/security-model" },
          ],
        },
      ],
      "/operations/": [
        {
          text: "Operations",
          items: [
            { text: "Deployment", link: "/operations/deployment" },
            {
              text: "Release Automation (macOS + R2)",
              link: "/operations/release-automation-mac-mini",
            },
            { text: "Backup & Restore", link: "/operations/backup-restore" },
            { text: "Upgrade", link: "/operations/upgrade" },
            { text: "Monitoring", link: "/operations/monitoring" },
            { text: "Troubleshooting", link: "/operations/troubleshooting" },
          ],
        },
      ],
    },

    socialLinks: [
      { icon: "github", link: "https://github.com/devaryakjha/oore.build" },
    ],

    editLink: {
      pattern:
        "https://github.com/devaryakjha/oore.build/edit/master/apps/docs-site/docs/:path",
      text: "Edit this page on GitHub",
    },

    footer: {
      message: "Self-hosted mobile CI, built for Flutter.",
      copyright: "Copyright &copy; 2026 devaryakjha",
    },

    search: {
      provider: "local",
    },

    outline: {
      level: [2, 3],
    },
  },
});
