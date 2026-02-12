# Rust Agent CLI MVP Research (OpenAI Codex First)

Date: 2026-02-07

## Goal

Build a Rust-only agent CLI that:

- Feels like Codex/pi in terminal UX.
- Uses `ratatui` for the interactive interface.
- Supports only OpenAI first (Codex-capable models via Responses API).
- Is structured so other providers can be added later with minimal rewrites.

## Key Findings From Existing Projects

### 1) `openai/codex` architecture pattern

What is worth copying:

- Clear separation between CLI shell, core runtime, and TUI.
- Tool system split into:
  - tool specs/schemas,
  - registry/dispatch,
  - orchestration (approval + sandbox policy + retries).
- Config layering (user config + CLI overrides + managed layers).
- Strong terminal lifecycle handling (raw mode, alt screen, bracketed paste, event stream ownership).
- Session history + resume/fork flows.

Why this matters for your MVP:

- You can keep the first version small but still choose boundaries that scale.
- The highest-leverage idea is: **core agent logic should not depend on ratatui**.

### 2) `pi-mono` architecture pattern

What is worth copying:

- `AgentSession` as the shared core abstraction across modes.
- Modes (`interactive`, `print`, `rpc`) built on the same session engine.
- Provider/model registry as a separate concern from UI.
- Tool catalog in one module with per-tool implementations.
- Session persistence as append-only JSONL.

Why this matters for your MVP:

- If you want non-interactive mode later, you should not couple runtime logic to UI now.

### 3) Ratatui guidance and templates

Most useful template style for this project:

- `event-driven-async` template pattern:
  - dedicated event task,
  - `Tick` + terminal input events,
  - async app loop, render from state.

This is the best starter shape for a streaming agent CLI.

## Recommended MVP Scope (v0.1)

### In scope

- Interactive TUI chat loop.
- OpenAI provider only with OAuth login for ChatGPT plans (API key fallback).
- Streaming assistant output.
- Basic tool calling with a minimal safe toolset:
  - `read_file`
  - `list_files`
  - `exec_command` (workspace-scoped with approval policy)
  - optional `write_file` (if you want edits in v0.1)
- Session save/resume in local JSONL files.
- Config file + env overrides.

### Out of scope (for MVP)

- MCP server support.
- Multi-provider UI.
- Complex extension system (skills/plugins marketplace).
- Full sandbox implementation across all OSes.

## MVP Architecture (Extendable but Beginner-Friendly)

Start with one crate and explicit module boundaries (simpler than multi-crate workspace for a first Rust project).

### Suggested layout

```text
src/
  main.rs
  cli.rs
  app/
    mod.rs
    state.rs
    event.rs
    ui.rs
    controller.rs
  auth/
    mod.rs
    provider.rs
    oauth_chatgpt.rs
    token_store.rs
  agent/
    mod.rs
    session.rs
    turn.rs
  providers/
    mod.rs
    provider.rs
    openai_responses.rs
  tools/
    mod.rs
    registry.rs
    exec.rs
    read.rs
    ls.rs
    write.rs
  storage/
    mod.rs
    sessions.rs
    config.rs
  model/
    mod.rs
    types.rs
```

### Core contracts to define early

```rust
// providers/provider.rs
#[async_trait::async_trait]
pub trait ModelProvider: Send + Sync {
    fn id(&self) -> &'static str;
    async fn stream_turn(
        &self,
        request: ProviderTurnRequest,
    ) -> anyhow::Result<ProviderEventStream>;
}
```

```rust
// tools/registry.rs
#[async_trait::async_trait]
pub trait ToolHandler: Send + Sync {
    fn name(&self) -> &'static str;
    async fn execute(&self, call: ToolCall, ctx: &ToolContext) -> anyhow::Result<ToolResult>;
}
```

This gives you extensibility without building a full plugin system now.

```rust
// auth/provider.rs
#[async_trait::async_trait]
pub trait AuthProvider: Send + Sync {
    fn id(&self) -> &'static str;
    async fn login(&self) -> anyhow::Result<AuthSession>;
    async fn refresh_if_needed(&self, session: &mut AuthSession) -> anyhow::Result<()>;
}
```

## OpenAI-Only MVP Strategy

### Auth

- Primary auth: OAuth login for ChatGPT plans.
- Add `/login`, `/auth`, and `/logout` commands.
- Store tokens in OS keyring by default (explicit file fallback only when needed).
- Auto-refresh access tokens before requests.
- Keep `OPENAI_API_KEY` as fallback for local/dev workflows.
- Use `https://api.openai.com/v1/responses`.
- Keep provider config in file/env so future providers are additive.

### Models

- Do not hardcode one model forever.
- Provide:
  - default model in config,
  - `--model` CLI override,
  - `/model` switch command in TUI later.

Reason: model IDs are changing quickly across docs and repos, so your CLI should be configurable by design.

## Ratatui Implementation Plan

Use an async event loop:

1. Spawn event task:
   - read `crossterm` events,
   - emit `Tick` at fixed FPS (20-30 is enough).
2. App loop:
   - handle input events,
   - handle runtime/provider events,
   - redraw from current `AppState`.
3. Streaming handling:
   - append deltas into the active assistant message.
   - keep render cheap and deterministic.

Important terminal behaviors to include from day 1:

- raw mode enter/restore on panic-safe path,
- bracketed paste,
- alt-screen toggle (configurable).

## Suggested Dependencies for MVP

- `ratatui = "0.29"`
- `crossterm = { version = "0.28", features = ["event-stream", "bracketed-paste"] }`
- `tokio = { version = "1", features = ["rt-multi-thread", "macros", "signal", "time"] }`
- `futures`, `tokio-stream`
- `reqwest` (HTTP + SSE stream handling)
- `serde`, `serde_json`, `toml`
- `clap`
- `thiserror` + `anyhow`
- `directories` (config/session paths)
- `tracing`, `tracing-subscriber`
- `keyring` (secure OAuth token storage)
- `webbrowser` (open OAuth URL from TUI)
- `url` (callback URL parsing/validation)

## MVP Milestones

1. **Project skeleton**
   - CLI args, config load, ratatui boot/restore.
2. **OAuth auth flow**
   - ChatGPT plan login, persisted session, refresh.
3. **OpenAI provider**
   - single prompt -> streaming text in TUI (OAuth token + API key fallback).
4. **Session engine**
   - message model + JSONL persistence + resume latest.
5. **Tools v0**
   - `read_file`, `list_files`, `exec_command` with approval prompt.
6. **Polish**
   - error surfaces, cancellation (`Esc`/`Ctrl+C`), basic tests.

## MVP Acceptance Criteria

- Starts with `maky` and opens TUI reliably.
- Can log in with OAuth (ChatGPT plan) and remain signed in across restarts.
- Can send prompt and stream response from OpenAI.
- Supports at least one tool call end-to-end.
- Saves session and can resume.
- Can abort a running turn cleanly.
- No terminal corruption on crash/exit.

## Biggest Risks (and how to avoid them)

1. **UI and runtime get tangled**
   - Keep `agent/session` independent from ratatui modules.
2. **Model/API churn**
   - Configurable model/provider fields, no hardcoded assumptions.
3. **Tool safety**
   - Explicit approval mode before shell/file mutation tools.
4. **Auth/token storage complexity**
   - Keep auth behind dedicated `auth/` module and keyring-backed storage.
5. **Terminal edge cases**
   - Implement robust init/restore path and panic hook early.

## Practical Next Step

If you want, next step after this doc is I can scaffold the MVP skeleton in this repo with:

- module structure,
- OAuth auth module + token storage interfaces,
- working ratatui loop,
- OpenAI streaming provider stub,
- basic session persistence.

## Sources

- OpenAI Codex repo: https://github.com/openai/codex
- Codex Rust workspace crates: https://github.com/openai/codex/blob/main/codex-rs/Cargo.toml
- Codex CLI entrypoint: https://github.com/openai/codex/blob/main/codex-rs/cli/src/main.rs
- Provider model config + auth/base URL behavior: https://github.com/openai/codex/blob/main/codex-rs/core/src/model_provider_info.rs
- Tool registry and dispatch: https://github.com/openai/codex/blob/main/codex-rs/core/src/tools/registry.rs
- Tool orchestration (approval/sandbox/retry): https://github.com/openai/codex/blob/main/codex-rs/core/src/tools/orchestrator.rs
- TUI event stream handling: https://github.com/openai/codex/blob/main/codex-rs/tui/src/tui/event_stream.rs
- Pi monorepo overview: https://github.com/badlogic/pi-mono
- Pi coding agent docs: https://github.com/badlogic/pi-mono/tree/main/packages/coding-agent
- Pi shared session abstraction (`AgentSession`): https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/src/core/agent-session.ts
- Pi model registry: https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/src/core/model-registry.ts
- Pi tools index: https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/src/core/tools/index.ts
- Ratatui site: https://ratatui.rs/
- Ratatui templates: https://github.com/ratatui/templates
- Ratatui async event-driven template: https://github.com/ratatui/templates/tree/main/event-driven-async
- OpenAI Codex docs hub: https://developers.openai.com/codex
- OpenAI Codex auth docs: https://developers.openai.com/codex/auth
- OpenAI Codex models docs: https://developers.openai.com/codex/models
- OpenAI Responses API docs: https://platform.openai.com/docs/api-reference/responses
