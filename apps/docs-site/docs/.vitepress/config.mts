import { defineConfig } from "vitepress";

export default defineConfig({
  title: "Oore CI Docs",
  description:
    "Self-hosted, Flutter-first mobile CI and internal app distribution platform",
  cleanUrls: true,

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
        content: "#f49f1e",
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
        content: "Documentation and architecture reference for Oore CI.",
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
        content: "Documentation and architecture reference for Oore CI.",
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
      { text: "Guide", link: "/guide/overview" },
      { text: "API", link: "/api/overview" },
      { text: "CLI", link: "/cli/overview" },
      {
        text: "GitHub",
        link: "https://github.com/devaryakjha/oore.build",
      },
    ],

    sidebar: [
      {
        text: "Getting Started",
        items: [
          { text: "Overview", link: "/guide/overview" },
          { text: "Installation", link: "/guide/installation" },
          { text: "Quick Start", link: "/guide/quick-start" },
        ],
      },
      {
        text: "Architecture",
        items: [
          { text: "Overview", link: "/architecture/overview" },
          { text: "Backend", link: "/architecture/backend" },
          { text: "Frontend", link: "/architecture/frontend" },
          { text: "CLI", link: "/architecture/cli" },
        ],
      },
      {
        text: "Features",
        items: [
          { text: "Setup Wizard", link: "/features/setup-wizard" },
          {
            text: "OIDC Authentication",
            link: "/features/oidc-authentication",
          },
          { text: "Setup Wizard UI", link: "/features/setup-wizard-ui" },
          { text: "Multi-Instance", link: "/features/multi-instance" },
          {
            text: "Roles & Permissions",
            link: "/features/rbac",
          },
          {
            text: "User Management",
            link: "/features/user-management",
          },
        ],
      },
      {
        text: "API Reference",
        items: [
          { text: "Overview", link: "/api/overview" },
          { text: "Setup API", link: "/api/setup" },
          { text: "Auth API", link: "/api/auth" },
          { text: "Users API", link: "/api/users" },
        ],
      },
      {
        text: "CLI Reference",
        items: [
          { text: "Overview", link: "/cli/overview" },
          { text: "setup Command", link: "/cli/setup" },
        ],
      },
      {
        text: "Security",
        items: [{ text: "Overview", link: "/security/overview" }],
      },
    ],

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
