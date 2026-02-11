import { defineConfig } from "vitepress";

export default defineConfig({
  title: "Oore CI Docs",
  description:
    "Self-hosted, Flutter-first mobile CI and internal app distribution platform",
  cleanUrls: true,

  // Ignore dead links in old pages that haven't been migrated yet.
  // These pages reference removed guide/, api/, cli/ directories.
  // Once all waves are complete and old pages are deleted, this can be removed.
  ignoreDeadLinks: true,

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
        content: "/logo512.png",
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
        content: "/logo512.png",
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
      { text: "Concepts", link: "/concepts/architecture" },
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
            { text: "Prerequisites", link: "/getting-started/prerequisites" },
            { text: "Install", link: "/getting-started/install" },
            {
              text: "Set Up Your Instance",
              link: "/getting-started/first-instance",
            },
            { text: "Connect GitHub", link: "/getting-started/connect-github" },
            { text: "First Build", link: "/getting-started/first-build" },
          ],
        },
      ],
      "/guides/": [
        {
          text: "OIDC Authentication",
          items: [
            { text: "Overview", link: "/guides/oidc/" },
            { text: "Google", link: "/guides/oidc/google" },
          ],
        },
        {
          text: "Integrations",
          items: [
            { text: "GitHub App", link: "/guides/integrations/github-app" },
            { text: "GitLab", link: "/guides/integrations/gitlab" },
          ],
        },
        {
          text: "Projects",
          items: [
            { text: "Create a Project", link: "/guides/projects/create-project" },
            { text: "Pipeline Config (.oore.yaml)", link: "/guides/projects/pipeline-config" },
            { text: "Pipeline via UI", link: "/guides/projects/pipeline-ui-fallback" },
            { text: "Trigger Builds", link: "/guides/projects/trigger-builds" },
          ],
        },
        {
          text: "Multi-Instance",
          items: [
            { text: "Add Instance", link: "/guides/multi-instance/add-instance" },
            { text: "Switch Instances", link: "/guides/multi-instance/switch-instances" },
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
            { text: "Build Logs", link: "/reference/api/logs" },
          ],
        },
        {
          text: "CLI Reference",
          items: [
            { text: "Overview", link: "/reference/cli/" },
            { text: "oore setup", link: "/reference/cli/oore-setup" },
          ],
        },
        {
          text: "Configuration",
          items: [
            { text: ".oore.yaml", link: "/reference/config/oore-yaml" },
          ],
        },
        {
          text: "State Machines",
          items: [
            { text: "Setup States", link: "/reference/setup-states" },
            { text: "Build States", link: "/reference/build-states" },
          ],
        },
      ],
      "/concepts/": [
        {
          text: "Concepts",
          items: [
            { text: "Architecture", link: "/concepts/architecture" },
            { text: "Build Execution", link: "/concepts/build-execution" },
            { text: "Multi-Instance", link: "/concepts/multi-instance" },
          ],
        },
      ],
      // Legacy pages (not yet migrated — will be removed after Wave 5)
      "/architecture/": [
        {
          text: "Architecture (Legacy)",
          items: [
            { text: "Overview", link: "/architecture/overview" },
            { text: "Backend", link: "/architecture/backend" },
            { text: "Frontend", link: "/architecture/frontend" },
            { text: "CLI", link: "/architecture/cli" },
          ],
        },
      ],
      "/features/": [
        {
          text: "Features (Legacy)",
          items: [
            { text: "Setup Wizard", link: "/features/setup-wizard" },
            { text: "OIDC Authentication", link: "/features/oidc-authentication" },
            { text: "Setup Wizard UI", link: "/features/setup-wizard-ui" },
            { text: "Multi-Instance", link: "/features/multi-instance" },
            { text: "Roles & Permissions", link: "/features/rbac" },
            { text: "User Management", link: "/features/user-management" },
            { text: "Runner Management", link: "/features/runner-management" },
            { text: "Android Signing", link: "/features/android-signing" },
            { text: "iOS Signing", link: "/features/ios-signing" },
            { text: "Artifact Storage", link: "/features/artifact-storage-management" },
            { text: "File-First Pipeline Config", link: "/features/file-first-pipeline-config" },
          ],
        },
      ],
      "/security/": [
        {
          text: "Security (Legacy)",
          items: [
            { text: "Overview", link: "/security/overview" },
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
      copyright: "Copyright &copy; 2026 Arya Labs",
    },

    search: {
      provider: "local",
    },

    outline: {
      level: [2, 3],
    },
  },
});
