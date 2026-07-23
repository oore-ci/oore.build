# Domain docs

Oore is one product domain despite being implemented as a monorepo.

Internal domain documentation and ADRs follow the private documentation
governance defined in `AGENTS.md`. This repository intentionally does not
contain `CONTEXT.md`, `CONTEXT-MAP.md`, or `docs/adr/`.

Before exploring or changing a domain:

1. Read `AGENTS.md`.
2. Read the required private source-of-truth index and the relevant domain
   notes and ADRs.
3. Flag conflicts with an existing ADR instead of silently overriding it.

Domain-modeling sessions update the private glossary or ADR collection when a
project-specific term or durable architectural decision is resolved. They must
not create repo-local replacements or expose private-note links.
