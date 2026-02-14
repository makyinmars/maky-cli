# MVP Phase Guides (Architecture + Checklists)

Use these guides in order. Each one is architecture-only and split into baby steps.

## Recommended Order

1. [Phase 0](./phase-0-mvp-architecture.md): setup, module boundaries, Rust baseline.
2. [Phase 1](./phase-1-mvp-architecture.md): terminal skeleton, event loop, safe exit.
3. [Phase 2](./phase-2-mvp-architecture.md): local chat loop, slash commands, no network.
4. [Phase 3](./phase-3-mvp-architecture.md): OAuth architecture and auth session handling.
5. [Phase 4](./phase-4-mvp-architecture.md): streaming provider architecture.
6. [Phase 5](./phase-5-mvp-architecture.md): session persistence and resume.
7. [Phase 6](./phase-6-mvp-architecture.md): tools v0 with approval and safety guards.
8. [Phase 7](./phase-7-mvp-architecture.md): config layering and model controls.
9. [Phase 8](./phase-8-mvp-architecture.md): reliability hardening and tests.
10. [Phase 9](./phase-9-mvp-architecture.md): release cut for MVP `v0.1`.

## How to Use These Guides

- Start each phase only after the previous phase done checklist is complete.
- Treat each guide as an architecture contract before coding details.
- Keep notes in each checklist as you complete steps.
- Avoid jumping ahead unless a blocker forces it.
