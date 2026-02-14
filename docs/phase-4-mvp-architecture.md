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

- [ ] Step 1: Define `ModelProvider` trait with streaming turn interface.
- [ ] Step 2: Define provider request/response event types.
- [ ] Step 3: Add OpenAI provider module boundary and config dependency points.
- [ ] Step 4: Wire controller submit path to provider request creation.
- [ ] Step 5: Route streaming deltas into active assistant message in state.
- [ ] Step 6: Define completion event handling and turn finalization.
- [ ] Step 7: Define cancel action routing and cancellation semantics.
- [ ] Step 8: Define provider/network error surfaces for status line + logs.
- [ ] Step 9: Preserve UI responsiveness while stream is active.
- [ ] Step 10: Confirm auth/session dependency injection into provider requests.

## Phase 4 Done Criteria (Checklist)

- [ ] A submitted prompt can produce streamed assistant text.
- [ ] Streaming updates appear incrementally in the TUI.
- [ ] Turn completion transitions state back to idle.
- [ ] Cancel action stops an active turn cleanly.
- [ ] Provider errors are visible to user and logged for debugging.

## Rust Learning Focus

- Async streams and channels.
- Trait objects and boundary-driven design.
- Task lifecycle and cancellation patterns with `tokio`.

## Handoff to Phase 5

When Phase 4 is complete:
- preserve provider event contracts,
- add persistence around turns/events without changing UI contracts.
