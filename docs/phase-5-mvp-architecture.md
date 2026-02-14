# Phase 5 MVP Architecture Guide

Audience: beginner Rust developer with Zig and TypeScript experience.

This guide is architecture-only for **Phase 5: Session Engine and Persistence**.

## Phase 5 Goal

Persist conversation history and allow session resume across app restarts.

## Phase 5 Non-Goals

Do not implement these yet:
- full tool framework,
- advanced indexing/search over sessions,
- cloud sync.

## Architecture Delta from Phase 4

1. **Session becomes a first-class domain object**  
Turns and metadata are persisted independently of UI lifecycle.

2. **Append-only event log format**  
Use JSONL for replay, crash recovery, and simple debugging.

3. **Resume path is explicit**  
Startup can load latest session or selected session id.

## Module Responsibilities (Phase 5)

- `agent/session.rs`: in-memory session model and operations.
- `storage/sessions.rs`: JSONL append/load/replay logic.
- `app/controller.rs`: writes events and handles resume/new session commands.
- `cli.rs` or arg parser: `--resume <id>` support.

## Core Data/Contract Shape

- `SessionMeta { session_id, model, created_at, updated_at }`
- `SessionEvent` enum (`UserMessage`, `AssistantDelta`, `AssistantFinal`, `Error`, ...)
- `SessionStore` trait:
  - `append(event)`
  - `load(session_id)`
  - `load_latest()`
- command surface:
  - `/new`
  - optional `/resume <id>`

## Step-by-Step Build Plan (Checklist)

- [ ] Step 1: Define session metadata and event schema.
- [ ] Step 2: Define JSONL line format for each persisted event type.
- [ ] Step 3: Add append-only write contract with flush strategy.
- [ ] Step 4: Add replay contract to rebuild in-memory session state.
- [ ] Step 5: Add startup policy (`latest` by default, optional explicit resume).
- [ ] Step 6: Add `/new` architecture path to create fresh session id.
- [ ] Step 7: Define session directory layout and file naming convention.
- [ ] Step 8: Define corruption tolerance policy (skip bad line vs fail fast).
- [ ] Step 9: Define UI indicators for active session id and restore status.
- [ ] Step 10: Verify provider turn pipeline writes events continuously.

## Phase 5 Done Criteria (Checklist)

- [ ] Conversations persist to disk in JSONL format.
- [ ] Restarting app restores history from latest session.
- [ ] `--resume <id>` behavior is defined and works in architecture.
- [ ] `/new` starts a fresh session cleanly.
- [ ] Session ids are stable and unique.

## Rust Learning Focus

- `serde` serialization/deserialization patterns.
- File IO and append-safe writes.
- Rebuild state by replaying immutable events.

## Handoff to Phase 6

When Phase 5 is complete:
- keep persistence as independent boundary,
- layer tools on top of existing session event model.
