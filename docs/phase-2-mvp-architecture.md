# Phase 2 MVP Architecture Guide

Audience: beginner Rust developer with Zig and TypeScript experience.

This guide is architecture-only for **Phase 2: Local Chat Loop (No Network Yet)**.

## Phase 2 Goal

Validate the chat interaction flow locally before adding provider/network complexity.

## Phase 2 Non-Goals

Do not implement these yet:
- OpenAI API calls,
- OAuth login,
- persistent sessions,
- tool execution.

## Architecture Delta from Phase 1

1. **Input becomes interaction state**  
Input field now drives message submission and local responses.

2. **Command path and chat path are separated**  
Slash commands route through command handling, not chat generation.

3. **Assistant output is local and deterministic**  
Use a fake/local responder to test flow and UI updates.

## Module Responsibilities (Phase 2)

- `app/state.rs`: add message list and input editing state.
- `app/event.rs`: add submit events and command events.
- `app/controller.rs`: handle send flow and command dispatch.
- `app/ui.rs`: reflect new message history behavior in render logic.

## Core Data/Contract Shape

- `ChatMessage { role, text, timestamp }`
- `LocalCommand` enum: `/help`, `/quit`
- `InputMode` or equivalent edit state
- optional `TurnState` enum for `Idle | HandlingLocalResponse`

## Step-by-Step Build Plan (Checklist)

- [ ] Step 1: Add state fields for chat messages and editable input text.
- [ ] Step 2: Define message roles (`user`, `assistant`, `system`) for UI labeling.
- [ ] Step 3: Add submit event handling (`Enter` behavior) in controller.
- [ ] Step 4: Parse slash commands before normal message processing.
- [ ] Step 5: Define `/help` behavior in architecture (status/message output path).
- [ ] Step 6: Define `/quit` behavior to route to `Quit` event.
- [ ] Step 7: Add local fake responder contract for assistant replies.
- [ ] Step 8: Ensure user message append and assistant message append are separate steps.
- [ ] Step 9: Define failure/status path for command parsing errors.
- [ ] Step 10: Validate keyboard editing behavior (insert, backspace, cursor policy).
- [ ] Step 11: Validate message-pane scrolling policy for growing history.
- [ ] Step 12: Confirm clean exit semantics still hold after local chat flow is added.

## Phase 2 Done Criteria (Checklist)

- [ ] You can type and submit messages locally.
- [ ] User messages appear in history.
- [ ] Local assistant responses appear in history.
- [ ] `/help` works.
- [ ] `/quit` exits cleanly.
- [ ] No network/provider dependencies are required for this phase.

## Rust Learning Focus

- `enum` + `match` for state transitions.
- String handling and input buffering.
- Clean controller logic with explicit event branches.

## Handoff to Phase 3

When Phase 2 is complete:
- keep local turn flow as fallback/debug path,
- add auth architecture in isolation before network streaming.
