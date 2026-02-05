import { defineConfig } from "vitepress";

export default defineConfig({
  title: "oore.build Docs",
  description: "Product and engineering documentation for oore.build",
  cleanUrls: true,
  themeConfig: {
    nav: [
      { text: "Home", link: "/" },
      { text: "Platform Contract", link: "/platform-contract" }
    ],
    sidebar: [
      {
        text: "Core",
        items: [
          { text: "Overview", link: "/" },
          { text: "Platform Contract", link: "/platform-contract" }
        ]
      }
    ]
  }
});
