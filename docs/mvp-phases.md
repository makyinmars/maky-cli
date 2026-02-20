# Maky CLI MVP Phases (Rust + Ratatui)

Based on `research/agent-cli-mvp-rust-ratatui.md`

This breaks the MVP into small phases so you can build and validate one layer at a time.

This version treats OAuth login with ChatGPT plans as part of MVP scope.

## Phase 0: Setup and Rust Basics

Goal: Make the project runnable and remove setup friction.

Tasks:
- Create a binary crate (`cargo init --bin .` if needed).
- Add formatting and linting: `rustfmt`, `clippy`.
- Add base dependencies (`tokio`, `clap`, `anyhow`, `thiserror`, `serde`, `tracing`).
- Create starter folders in `src/` for `app`, `agent`, `providers`, `tools`, `storage`, `model`.

Done when:
- `cargo check` passes.
- `cargo clippy` passes with no critical warnings.
- You can run a placeholder command like `maky --help`.

Rust learning focus:
- `Result<T, E>`, `?` operator, modules (`mod.rs`), traits.

Preferred package references:
- `tokio`: https://tokio.rs/
- `tracing`: https://github.com/tokio-rs/tracing
- `clap`: https://github.com/clap-rs/clap
- `anyhow`: https://github.com/dtolnay/anyhow
- `thiserror`: https://github.com/dtolnay/thiserror

## Phase 1: Terminal App Skeleton

Goal: Open/close TUI safely and handle keyboard events.

Tasks:
- Implement terminal lifecycle (raw mode, alternate screen, restore on exit).
- Add event loop with tick + input events.
- Implement minimal `AppState` and `ui.rs` with 3 panes:
  - header/status
  - message history
  - input box
- Add clean shutdown on `Ctrl+C` and `Esc`.

Done when:
- `maky` opens a stable TUI.
- Exiting never leaves terminal broken.

Rust learning focus:
- async tasks (`tokio::spawn`), channels (`tokio::sync::mpsc`), ownership between app state and renderer.

## Phase 2: Local Chat Loop (No Network Yet)

Goal: Validate app flow before integrating OpenAI.

Tasks:
- Capture user input and append to message history.
- Add a fake assistant response generator for testing UI updates.
- Add basic command parsing for slash commands (`/help`, `/quit`).

Done when:
- You can send messages and see deterministic local responses.
- Input/edit behavior is smooth.

Rust learning focus:
- enums for event handling, pattern matching, state transitions.

## Phase 3: OAuth Login (ChatGPT Plans)

Goal: Support ChatGPT-plan OAuth login so API keys are optional.

Tasks:
- Define an auth abstraction and OAuth session model.
- Implement login flow for ChatGPT plan users (`/login` command and startup prompt).
- Persist auth session securely (keyring first, file fallback only when needed).
- Add `/auth` status and `/logout`.
- Add token refresh handling before model requests.

Done when:
- A new user can sign in via OAuth and stay signed in across restarts.
- Chat requests work after OAuth login without setting `OPENAI_API_KEY`.

Rust learning focus:
- auth state machines, secure storage boundaries, expiry/refresh handling.

## Phase 4: OpenAI Streaming Provider

Goal: Replace fake responses with real streaming output from Responses API.

Tasks:
- Define `ModelProvider` trait and `OpenAIResponsesProvider`.
- Read access token from OAuth session, with `OPENAI_API_KEY` fallback.
- Implement streaming assistant output into active message.
- Surface API/network errors in a user-visible status line.

Done when:
- A prompt produces streamed text from OpenAI in the TUI.
- Cancel action stops a running request cleanly.

Rust learning focus:
- trait objects, async traits (`async-trait`), streaming with `futures`/`tokio-stream`.

## Phase 5: Session Engine and Persistence

Goal: Persist conversations and resume them.

Tasks:
- Add `AgentSession` type (session id, messages, timestamps, model id).
- Save conversation events as JSONL.
- Load latest session on startup (or via `--resume <id>`).
- Add `/new` command to start a fresh session.

Done when:
- Closing/reopening app restores history.
- Multiple sessions can be resumed by id.

Rust learning focus:
- serde serialization, file I/O, append-only logs.

## Phase 6: Tools v0 with Safety

Goal: Add minimal tool execution with explicit user approvals.

Tasks:
- Define `ToolHandler` trait and registry.
- Implement:
  - `list_files` (workspace-only)
  - `read_file` (workspace-only)
  - `exec_command` (workspace-only + approval required)
- Add approval UI prompt (`Allow once`, `Deny`, later `Allow always`).
- Track tool calls/results in session log.

Done when:
- At least one tool call runs end-to-end from model request to displayed result.
- Mutating/dangerous tool actions cannot run without approval.

Rust learning focus:
- trait dispatch, bounded contexts, process execution (`tokio::process::Command`).

## Phase 7: Config and Model Controls

Goal: Make behavior configurable without code edits.

Tasks:
- Add config file loading (TOML) from standard config path.
- Merge precedence:
  1. defaults
  2. config file
  3. env vars
  4. CLI flags
- Add auth settings (`auth.mode = oauth|api_key`, token store policy).
- Add `--model` CLI override and persist default model.

Done when:
- You can switch models without code changes.
- Effective config is predictable and testable.

Rust learning focus:
- config layering, typed config structs, validation.

## Phase 8: Reliability and Tests

Goal: Make MVP stable enough for daily use.

Tasks:
- Add panic hook + guaranteed terminal restore.
- Add targeted unit tests for important logic (config parsing, session serialization, tool path guards).
- Add an integration smoke test for an important non-TUI provider/session path.
- Improve error messages (actionable, short, contextual).

Done when:
- Crash paths still restore terminal.
- Important modules have test coverage on happy path + key failures.

Rust learning focus:
- test layout (`#[cfg(test)]`, `tests/`), error typing, defensive coding.

## Phase 9: MVP Release Cut

Goal: Package and document v0.1.

Tasks:
- Add README with setup, env vars, commands, and known limitations.
- Add `--version`, changelog entry, and release tag.
- Validate on macOS/Linux terminal behavior.

Done when:
- Fresh user can install, log in with OAuth (or use API key), chat, and resume session.
- MVP acceptance criteria from research doc are met.

Rust learning focus:
- release workflow, reproducible builds, documentation quality.

## Suggested Build Order

1. Phase 0
2. Phase 1
3. Phase 2
4. Phase 3
5. Phase 4
6. Phase 5
7. Phase 6
8. Phase 7
9. Phase 8
10. Phase 9

## MVP Definition of Done

- TUI launches reliably with `maky`.
- OAuth login with ChatGPT plan works and persists session.
- OpenAI response streaming works.
- One or more tools execute with approval.
- Sessions persist and can be resumed.
- Running turn can be canceled.
- Terminal is always restored on exit/crash.
