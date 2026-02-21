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

2. **Pluggable persistence backend**  
Session events use one domain schema with two storage options:
- JSONL append log (simple debug/replay),
- SQLite tables (durable indexed storage).

3. **Resume path is explicit**  
Startup can load latest session or selected session id.

## Module Responsibilities (Phase 5)

- `agent/session.rs`: in-memory session model and operations.
- `storage/sessions.rs`: `SessionStore` trait + backend selector.
- `storage/jsonl_sessions.rs`: JSONL append/load/replay implementation.
- `storage/sqlite_sessions.rs`: SQLite schema/init/append/load/replay implementation.
- `app/controller.rs`: writes events and handles resume/new session commands.
- `cli.rs` or arg parser: `--resume <id>` support.

## Core Data/Contract Shape

- `SessionMeta { session_id, model, created_at, updated_at }`
- `SessionEvent` enum (`UserMessage`, `AssistantDelta`, `AssistantFinal`, `Error`, ...)
- `SessionStore` trait:
  - `append(event)`
  - `load(session_id)`
  - `load_latest()`
- config surface:
  - `session_store = "jsonl" | "sqlite"` (default `jsonl`)
  - `session_dir` for JSONL files
  - `session_db_path` for SQLite database file
- command surface:
  - `/new`
  - optional `/resume <id>`

## Step-by-Step Build Plan (Checklist)

- [ ] Step 1: Define session metadata and event schema.
- [ ] Step 2: Define persistence schema for both backends:
  - JSONL line format per event type,
  - SQLite tables/indexes (`sessions`, `session_events`) with event ordering.
- [ ] Step 3: Add append contract + durability strategy:
  - JSONL flush/append behavior,
  - SQLite transaction boundary per append batch.
- [ ] Step 4: Add replay contract to rebuild in-memory session state from either backend.
- [ ] Step 5: Add startup policy (`latest` by default, optional explicit resume).
- [ ] Step 6: Add `/new` architecture path to create fresh session id.
- [ ] Step 7: Define storage layout conventions:
  - JSONL directory + file naming convention,
  - SQLite DB path and initialization behavior.
- [ ] Step 8: Define corruption tolerance policy (skip bad line vs fail fast).
- [ ] Step 9: Define UI indicators for active session id and restore status.
- [ ] Step 10: Verify provider turn pipeline writes events continuously.

## Phase 5 Done Criteria (Checklist)

- [ ] Conversations persist to disk in the configured store (JSONL or SQLite).
- [ ] Restarting app restores history from latest session.
- [ ] `--resume <id>` behavior is defined and works in architecture.
- [ ] `/new` starts a fresh session cleanly.
- [ ] Session ids are stable and unique.

## Rust Learning Focus

- `serde` serialization/deserialization patterns.
- File IO and append-safe writes.
- Basic SQLite schema design and transaction usage.
- Rebuild state by replaying immutable events.

## Handoff to Phase 6

When Phase 5 is complete:
- keep persistence as independent boundary,
- layer tools on top of existing session event model.
