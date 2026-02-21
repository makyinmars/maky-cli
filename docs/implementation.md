# Maky CLI MVP Implementation Guide

This document describes the architecture for a Rust-first agent CLI MVP using `ratatui`, with OpenAI as the first provider.

Audience: beginner to intermediate Rust developer.

Preferred package references:
- `tokio`: https://tokio.rs/
- `tracing`: https://github.com/tokio-rs/tracing
- `clap`: https://github.com/clap-rs/clap
- `anyhow`: https://github.com/dtolnay/anyhow
- `thiserror`: https://github.com/dtolnay/thiserror

## 1) Product Shape

MVP capabilities:
- Terminal chat UI (Ratatui).
- OAuth login with ChatGPT plans.
- OpenAI Responses API streaming output.
- Session save/resume (JSONL or SQLite).
- Minimal tool calls (`list_files`, `read_file`, `exec_command`) with approval.
- Config via file/env/CLI overrides.

Not in MVP:
- MCP integration.
- Multi-provider UI controls.
- Complex plugin marketplace.

## 2) Architecture Overview

Keep one crate for now, but separate by module boundaries:

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
  storage/
    mod.rs
    sessions.rs
    jsonl_sessions.rs
    sqlite_sessions.rs
    config.rs
  model/
    mod.rs
    types.rs
```

Design rule:
- `agent/`, `providers/`, `tools/`, `storage/` should not depend on `ratatui`.
- `app/` is the only UI-specific layer.

## 3) Runtime Flow (End-to-End)

1. `main.rs` parses CLI args, loads config, initializes tracing.
2. Auth bootstrap resolves credentials:
   - prefer OAuth session for ChatGPT plans,
   - fallback to `OPENAI_API_KEY` if configured.
3. App enters terminal mode (raw mode + alt screen).
4. `app/controller.rs` runs async loop:
   - consumes terminal events (keys, resize, tick),
   - processes runtime events (provider stream chunks, tool approvals/results),
   - updates `AppState`,
   - triggers redraws through `ui.rs`.
5. On submit:
   - user message appended to session,
   - `AgentSession` builds a provider turn request,
   - provider streams assistant events,
   - UI appends delta text live.
6. If model emits tool call:
   - tool registry resolves handler,
   - approval gate checks policy,
   - tool executes in bounded workspace context,
   - result fed back into the active turn.
7. Session events append continuously to the configured session store (`jsonl` or `sqlite`).
8. On exit or panic, terminal restore runs.

## 4) Core Interfaces

Provider abstraction:

```rust
#[async_trait::async_trait]
pub trait ModelProvider: Send + Sync {
    fn id(&self) -> &'static str;
    async fn stream_turn(
        &self,
        request: ProviderTurnRequest,
    ) -> anyhow::Result<ProviderEventStream>;
}
```

Auth abstraction:

```rust
#[async_trait::async_trait]
pub trait AuthProvider: Send + Sync {
    fn id(&self) -> &'static str;
    async fn login(&self) -> anyhow::Result<AuthSession>;
    async fn refresh_if_needed(&self, session: &mut AuthSession) -> anyhow::Result<()>;
}
```

Tool abstraction:

```rust
#[async_trait::async_trait]
pub trait ToolHandler: Send + Sync {
    fn name(&self) -> &'static str;
    async fn execute(
        &self,
        call: ToolCall,
        ctx: &ToolContext,
    ) -> anyhow::Result<ToolResult>;
}
```

Why this matters:
- Provider and tools become swappable modules.
- UI can remain unchanged when backend internals evolve.

## 5) Data Model (Minimum Viable)

Core types in `model/types.rs`:
- `Message`:
  - `id`, `role` (`user|assistant|tool|system`), `content`, `timestamp`.
- `SessionMeta`:
  - `session_id`, `created_at`, `updated_at`, `model`.
- `ToolCall` / `ToolResult`.
- `ProviderEvent` enum:
  - `TextDelta`, `ToolCallRequested`, `Completed`, `Error`.

Session persistence backends:
- `SessionStore` trait defines shared append/load/replay behavior.
- JSONL backend:
  - one event per line (append-only),
  - safe for crash recovery,
  - easy to inspect and replay.
- SQLite backend:
  - `sessions` table stores metadata,
  - `session_events` table stores ordered event stream,
  - transactions preserve event ordering and durability.

## 6) Config Layering

Load in this order (lowest to highest priority):
1. hardcoded defaults,
2. `config.toml`,
3. environment variables,
4. CLI flags.

Essential config keys:
- `provider = "openai"`
- `model = "..."` (do not hardcode permanently)
- `auth.mode = "oauth" | "api_key"`
- `auth.token_store = "keyring" | "file"`
- `openai.base_url` (default official API)
- `openai.api_key_env = "OPENAI_API_KEY"`
- `session_store = "jsonl" | "sqlite"`
- `session_dir`
- `session_db_path` (used when `session_store = "sqlite"`)
- `approval_policy` (`on-request` for MVP)
- `ui.alt_screen` (bool)

## 7) OAuth in MVP

MVP auth behavior:
- Primary path: OAuth login for ChatGPT plans.
- Fallback path: `OPENAI_API_KEY` for local/dev setups.
- Add commands:
  - `/login`
  - `/auth`
  - `/logout`
- Store tokens in OS keyring by default.
- If keyring is unavailable, allow explicit file-store fallback with warning.
- Refresh expired access tokens automatically before API calls.

## 8) Tool Safety Model (MVP)

Rules:
- All file operations must resolve inside workspace root.
- `exec_command` requires explicit approval per call.
- Block dangerous path traversal (`..`, symlink escapes after canonicalization).
- Capture stdout/stderr and limit output size.

Recommended initial approval modes:
- `always_ask` (default for MVP),
- `never` (for CI/dry runs).

## 9) Error Handling Strategy

- Use `anyhow` at app boundaries for ergonomic error propagation with context.
- Use `thiserror` for typed domain errors (`ConfigError`, `ToolError`, `ProviderError`).
- Convert low-level errors into short user-facing status messages.
- Keep full diagnostics in logs via `tracing`.

## 10) Concurrency Strategy

- One main controller loop owns mutable `AppState`.
- Background tasks send events through channels.
- Avoid shared mutable state across tasks where possible.

This keeps borrow/lifetime complexity lower for early Rust development.

## 11) Suggested Crates for Your Agent CLI

Core MVP crates:
- `ratatui` - terminal UI rendering.
- `crossterm` - terminal input/events and lifecycle.
- `tokio` - async runtime (`https://tokio.rs/`).
- `futures` and `tokio-stream` - stream handling.
- `reqwest` - HTTP client for Responses API streaming.
- `serde`, `serde_json`, `toml` - data and config serialization.
- `clap` - CLI args parsing (`https://github.com/clap-rs/clap`).
- `anyhow` - ergonomic app-level error handling (`https://github.com/dtolnay/anyhow`).
- `thiserror` - typed domain errors (`https://github.com/dtolnay/thiserror`).
- `directories` - config/session directories.
- `tracing` and `tracing-subscriber` - logging (`https://github.com/tokio-rs/tracing`).
- `keyring` - secure OAuth token storage in OS credential store.
- `webbrowser` - open OAuth login URLs from terminal app.
- `url` - callback URL parsing and validation.
- `rusqlite` - SQLite-backed session store.

Very useful additions:
- `async-trait` - async trait methods (provider/tool interfaces).
- `uuid` - session/turn ids.
- `chrono` - timestamps.
- `once_cell` - global lazy init for static config/logging helpers.

Testing crates:
- Add these only when needed for important unit or integration coverage.
- `pretty_assertions` - readable assertion diffs.
- `tempfile` - isolated filesystem tests.
- `wiremock` or `httpmock` - API/provider integration tests without real network.

Optional later:
- `color-eyre` - richer panic/error reports in terminal apps (Codex also includes this in TUI).
- `miette` - nicer CLI diagnostics.
- `indicatif` - progress feedback for non-TUI commands.
- `ignore` - `.gitignore` aware file walking.

## 12) Cargo.toml Starter (Example)

```toml
[dependencies]
anyhow = "1"
async-trait = "0.1"
clap = { version = "4", features = ["derive"] }
crossterm = { version = "0.28", features = ["event-stream", "bracketed-paste"] }
directories = "6"
futures = "0.3"
keyring = { version = "3", default-features = false }
reqwest = { version = "0.12", features = ["json", "stream", "rustls-tls"] }
ratatui = "0.29"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "signal", "time", "process"] }
tokio-stream = "0.1"
toml = "0.8"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }
url = "2"
uuid = { version = "1", features = ["v4", "serde"] }
webbrowser = "1"
chrono = { version = "0.4", features = ["serde"] }

[dev-dependencies]
tempfile = "3"
pretty_assertions = "1"
```

Note:
- Keep versions flexible during early development.
- Pin exact versions before your first public release.

## 13) Beginner-Friendly Implementation Order

1. Build terminal skeleton and stable exit paths.
2. Add local fake chat loop.
3. Add OAuth login (ChatGPT plans) with persisted token store.
4. Add OpenAI streaming provider using OAuth token.
5. Add session persistence with pluggable JSONL/SQLite backends.
6. Add tool registry and one read-only tool.
7. Add `exec_command` with approvals.
8. Add config layering and model/auth overrides.
9. Add tests and release polish.

This order keeps feedback loops short and reduces Rust complexity spikes early.
