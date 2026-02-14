# Phase 8 MVP Architecture Guide

Audience: beginner Rust developer with Zig and TypeScript experience.

This guide is architecture-only for **Phase 8: Reliability and Tests**.

## Phase 8 Goal

Harden the MVP so it is stable for daily use and easier to debug.

## Phase 8 Non-Goals

Do not implement these yet:
- major new feature scope,
- large UX redesigns,
- performance micro-optimizations without evidence.

## Architecture Delta from Phase 7

1. **Failure behavior is designed, not accidental**  
Terminal restore and panic paths are explicit.

2. **Tests cover core boundaries**  
Config, session persistence, and tool safety checks get targeted tests.

3. **Operational diagnostics improve**  
Errors are concise for users and detailed in logs.

## Reliability Boundaries

- terminal restore on normal exit,
- terminal restore on panic/unhandled error,
- bounded output handling for tools,
- clear cancellation cleanup for running turns.

## Test Strategy (MVP Scope)

- Unit tests:
  - config parsing/merge/validation
  - session event serialization/replay
  - tool path guard logic
- Integration smoke path:
  - non-TUI provider/session flow at minimum

## Step-by-Step Build Plan (Checklist)

- [ ] Step 1: Define panic hook strategy with guaranteed terminal restore.
- [ ] Step 2: Define shutdown cleanup order for active tasks and resources.
- [ ] Step 3: Add unit test plan for config layering and invalid input cases.
- [ ] Step 4: Add unit test plan for session JSONL round-trip and replay.
- [ ] Step 5: Add unit test plan for workspace path guards and traversal attempts.
- [ ] Step 6: Add integration smoke test plan for provider-session path.
- [ ] Step 7: Define error message style guide (short, actionable, contextual).
- [ ] Step 8: Ensure logs retain full technical diagnostics (`tracing`).
- [ ] Step 9: Define expected behavior for recoverable vs fatal errors.
- [ ] Step 10: Run manual crash-path validation for terminal restore.

## Phase 8 Done Criteria (Checklist)

- [ ] Terminal state is restored on all tested crash/exit paths.
- [ ] Core modules have unit tests for happy path + key failures.
- [ ] At least one integration smoke path exists.
- [ ] User-facing errors are readable and actionable.
- [ ] Logs provide enough detail for debugging.

## Rust Learning Focus

- Test organization (`#[cfg(test)]`, `tests/`).
- Error typing and conversion boundaries.
- Defensive cleanup patterns with scope guards and explicit drop ordering.

## Handoff to Phase 9

When Phase 8 is complete:
- feature scope should be stable,
- focus on packaging, docs, validation, and release readiness.
