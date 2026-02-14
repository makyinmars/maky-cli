# Phase 9 MVP Architecture Guide

Audience: beginner Rust developer with Zig and TypeScript experience.

This guide is architecture-only for **Phase 9: MVP Release Cut**.

## Phase 9 Goal

Package, document, and validate a `v0.1` release that new users can run end-to-end.

## Phase 9 Non-Goals

Do not include these in release scope:
- large new features,
- deep refactors across stable modules,
- speculative infrastructure work.

## Architecture Delta from Phase 8

1. **Productization layer is finalized**  
Docs, CLI metadata, and release artifacts become part of the architecture.

2. **Acceptance criteria become enforceable gates**  
Ship only when MVP behavior is validated on target environments.

3. **Known limitations are explicit**  
Release notes define what is intentionally not solved yet.

## Release Surface

- README: install/setup/usage/auth/session/tool basics.
- CLI metadata: `--version`, help text, command docs.
- Changelog/release notes: features, risks, known gaps.
- basic platform verification: macOS and Linux terminals.

## Step-by-Step Build Plan (Checklist)

- [ ] Step 1: Freeze Phase 1-8 contracts and avoid scope creep.
- [ ] Step 2: Define release acceptance checklist from MVP definition of done.
- [ ] Step 3: Ensure README includes setup, auth options, and core commands.
- [ ] Step 4: Ensure README documents known limitations and safety model.
- [ ] Step 5: Add/update `--version` output and semantic versioning strategy.
- [ ] Step 6: Draft changelog entry summarizing shipped capabilities.
- [ ] Step 7: Run full smoke validation on fresh environment assumptions.
- [ ] Step 8: Validate terminal behavior on macOS.
- [ ] Step 9: Validate terminal behavior on Linux.
- [ ] Step 10: Tag and publish release artifacts/process for `v0.1`.

## Phase 9 Done Criteria (Checklist)

- [ ] Fresh user can run the app and start a session.
- [ ] OAuth path and API-key fallback are documented and functional.
- [ ] Streaming chat works.
- [ ] Session persistence/resume works.
- [ ] At least minimal tools path with approval works.
- [ ] Terminal restore reliability is verified.
- [ ] Release notes/changelog clearly communicate shipped scope.

## Rust Learning Focus

- Release discipline for Rust CLI projects.
- Reproducible build habits and validation workflows.
- Documentation as architecture (not just marketing text).

## Handoff After Phase 9

After `v0.1`:
- collect user feedback and bug reports,
- prioritize Phase 10+ roadmap based on real usage signals.
