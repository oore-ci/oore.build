# Oore CI documentation

The public documentation is a single VitePress site under `docs/`.

```bash
bun run dev
bun run test
bun run lint
bun run build
```

Production output is written to `docs/.vitepress/dist`.

- Navigation and metadata: `docs/.vitepress/config.mts`
- Theme tokens and layout adjustments: `docs/.vitepress/theme/`
- Public guides and reference: `docs/`
- Generated API contract: `docs/public/openapi.json`
