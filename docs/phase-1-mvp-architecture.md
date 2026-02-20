# Phase 1 MVP Architecture Guide

Audience: beginner Rust developer with Zig and TypeScript experience.

This guide is architecture-only for **Phase 1: Terminal App Skeleton**.  
No feature implementation details, just what to build and in what order.

Reference docs:

- `docs/mvp-phases.md` (overall phase plan)
- `docs/implementation.md` (full MVP architecture)

## Phase 1 Goal

Build a stable TUI shell that:

- opens cleanly,
- handles keyboard events in a loop,
- renders 3 panes (header, message history, input),
- exits cleanly without breaking the terminal.

## Phase 1 Non-Goals

Do not implement these in this phase:

- network calls,
- model/provider integration,
- OAuth/auth flows,
- session persistence,
- tool execution.

## Architecture Decisions (Lock These Early)

1. **Single owner of mutable app state**  
`controller` owns and mutates `AppState`. UI only reads state.

2. **Event-driven runtime**  
Everything is an event (key input, tick, resize, shutdown).

3. **UI is pure rendering**  
`ui` maps `AppState -> frame`; no side effects in rendering.

4. **Terminal lifecycle is isolated**  
Raw mode + alternate screen setup/teardown is handled in one boundary.

5. **Shutdown must be safe-by-default**  
Esc/Ctrl+C paths and unexpected errors must still restore terminal state.

## Proposed Module Layout (Phase 1)

```text
src/
  main.rs
  app/
    mod.rs
    controller.rs
    event.rs
    state.rs
    ui.rs
```

Responsibility split:

- `main.rs`: bootstrap, startup/shutdown wiring.
- `app/controller.rs`: event loop and state transitions.
- `app/event.rs`: event types and event-source contract.
- `app/state.rs`: `AppState` and UI-facing data model.
- `app/ui.rs`: all rendering for header/history/input panes.

## Runtime Flow (Phase 1)

1. Startup initializes terminal mode and app state.
2. Event loop waits for input/tick/resize events.
3. Controller applies event to `AppState`.
4. UI renders current state.
5. Loop repeats until exit signal.
6. Shutdown restores terminal state and exits.

## Data Model Shape (Phase 1)

Keep this minimal:

- `AppState`
- `running: bool`
- `status_line: String`
- `input_buffer: String`
- `messages: Vec<MessageLine>`
- `last_tick: Instant` (or equivalent timing marker)

Message model:

- `MessageLine`
- `role` (`system|user|assistant` for UI labeling)
- `text`

Event model:

- `AppEvent::Key(...)`
- `AppEvent::Tick`
- `AppEvent::Resize(...)`
- `AppEvent::Quit`

## Rust Mapping (From Zig + TypeScript)

- TypeScript discriminated unions -> Rust `enum` (`AppEvent`).
- Zig explicit cleanup mindset -> Rust RAII and structured teardown.
- Node event loop intuition -> `tokio` loop + channels/events.
- TypeScript mutable store patterns -> single mutable owner (`controller`) to avoid borrow complexity.

## Step-by-Step Build Plan (Checklist)

Use this as your execution checklist.

- [x] Step 1: Confirm Phase 0 baseline is green (`cargo check`, basic app entrypoint works).
- [x] Step 2: Create module skeleton (`app/{mod,controller,event,state,ui}.rs`) with compile-only stubs.
- [x] Step 3: Define `AppState` with only fields needed for the 3-pane UI.
- [x] Step 4: Define `AppEvent` enum for key/tick/resize/quit events.
- [x] Step 5: Define controller contract: receives events, mutates state, exposes redraw timing.
- [x] Step 6: Define UI contract: render header, message area, input box from immutable `AppState`.
- [x] Step 7: Add terminal lifecycle boundary in startup/shutdown path (enter/restore terminal mode).
- [x] Step 8: Wire event source into controller loop (keyboard + periodic tick + resize).
- [x] Step 9: Implement exit semantics at architecture level (`Esc` and `Ctrl+C` route to `Quit`).
- [x] Step 10: Define status/error surface in header so runtime issues are visible in-TUI.
- [x] Step 11: Add panic-safe restore strategy so terminal state is not left broken.
- [x] Step 12: Validate redraw policy (only redraw on event or tick, avoid busy-looping).
- [x] Step 13: Run smoke checks for open, key input, resize behavior, and clean exit (see `docs/phase-1-verification.md`).
- [x] Step 14: Freeze Phase 1 interfaces before Phase 2 to reduce refactors later.

## Phase 1 Done Criteria (Checklist)

- [x] Running `maky` opens a stable TUI.
- [x] Header, message pane, and input pane render consistently.
- [x] Keyboard input is captured and reflected in input state.
- [x] Resize events do not crash or corrupt layout.
- [x] `Esc` exits cleanly.
- [x] `Ctrl+C` exits cleanly.
- [x] Terminal is restored on all normal exit paths.
- [ ] Terminal is restored on panic/error paths you tested.

## Phase 1 Interface Freeze

Freeze these interfaces for Phase 2 unless a breaking need appears:

- `AppState` in `src/app/state.rs`.
- `AppEvent` in `src/app/event.rs`.
- `AppController::run` in `src/app/controller.rs`.
- `ui::draw` in `src/app/ui.rs`.

Allowed Phase 2 changes should be additive where possible (new fields/events) to reduce refactors.

## Handoff to Phase 2

When all done criteria are checked:

- Keep the same `AppState` + `AppEvent` core.
- Add local chat behavior without adding networking yet.
- Continue treating UI as read-only over state.
