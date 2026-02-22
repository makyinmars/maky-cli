# Phase 4 MVP Architecture Guide

Audience: beginner Rust developer with Zig and TypeScript experience.

This guide is architecture-only for **Phase 4: OpenAI Streaming Provider**.

## Phase 4 Goal

Replace local fake assistant responses with real streaming model output.

## Phase 4 Non-Goals

Do not implement these yet:

- tool execution loop,
- advanced multi-provider routing,
- long-term session storage optimizations.

## Architecture Delta from Phase 3

1. **Provider boundary becomes active**  
Model requests and streamed events flow through `ModelProvider`.

2. **Streaming is event-driven**  
UI receives deltas from provider events and appends live text.

3. **Cancellation is first-class**  
Running turns can be canceled cleanly from input.

## Module Responsibilities (Phase 4)

- `providers/provider.rs`: trait and event stream contracts.
- `providers/openai_responses.rs`: OpenAI Responses implementation.
- `app/controller.rs`: turn orchestration, stream event handling, cancel routing.
- `app/state.rs`: active turn metadata and partial assistant content.

## Core Data/Contract Shape

- `ProviderTurnRequest { messages, model, tools?, auth }`
- `ProviderEvent` enum:
  - `TextDelta(String)`
  - `Completed`
  - `Error(String)`
- `TurnHandle` for cancellation and task lifecycle management.

## Step-by-Step Build Plan (Checklist)

- [x] Step 1: Define `ModelProvider` trait with streaming turn interface.
- [x] Step 2: Define provider request/response event types.
- [x] Step 3: Add OpenAI provider module boundary and config dependency points.
- [x] Step 4: Wire controller submit path to provider request creation.
- [x] Step 5: Route streaming deltas into active assistant message in state.
- [x] Step 6: Define completion event handling and turn finalization.
- [x] Step 7: Define cancel action routing and cancellation semantics.
- [x] Step 8: Define provider/network error surfaces for status line + logs.
- [x] Step 9: Preserve UI responsiveness while stream is active.
- [x] Step 10: Confirm auth/session dependency injection into provider requests.
- [x] Step 11: Add Markdown rendering for assistant output (`pulldown-cmark` parser + custom `ratatui` renderer adapter).

## Phase 4 Done Criteria (Checklist)

- [x] A submitted prompt can produce streamed assistant text.
- [x] Streaming updates appear incrementally in the TUI.
- [x] Turn completion transitions state back to idle.
- [x] Cancel action stops an active turn cleanly.
- [x] Provider errors are visible to user and logged for debugging.
- [x] Streamed Markdown content (lists, code fences, emphasis) is rendered readably in the TUI.

## Rust Learning Focus

- Async streams and channels.
- Trait objects and boundary-driven design.
- Task lifecycle and cancellation patterns with `tokio`.

## Handoff to Phase 5

When Phase 4 is complete:

- preserve provider event contracts,
- add persistence around turns/events without changing UI contracts.

## Implementation Notes

- `providers/openai_responses.rs` now performs a real SSE request to the fixed Codex endpoint: `https://chatgpt.com/backend-api/codex/responses`.
- OAuth sessions are required for the live Codex path; non-OAuth credentials surface a provider error that directs users to run `/login`.
- `ChatGPT-Account-Id` is attached when available from OAuth session claims.
- Tests and offline controller flows use `mock://openai` to keep deterministic streaming behavior without network access.
- Recommended Markdown approach (matching Codex-style architecture): use `pulldown-cmark` for parsing and keep formatting decisions in a small `app/` renderer layer that outputs `ratatui::text::Line` values.
