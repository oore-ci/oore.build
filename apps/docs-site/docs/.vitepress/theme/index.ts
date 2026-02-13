import DefaultTheme from "vitepress/theme";
import type { Theme } from "vitepress";
import { theme, useOpenapi } from "vitepress-openapi/client";
import "vitepress-openapi/dist/style.css";
import spec from "../../public/openapi.json" with { type: "json" };
import "./custom.css";

export default {
  extends: DefaultTheme,
  enhanceApp({ app }) {
    const openapi = useOpenapi({
      spec,
      config: {
        server: {
          allowCustomServer: true,
        },
      },
    });
    theme.enhanceApp({ app, openapi });
  },
} satisfies Theme;
