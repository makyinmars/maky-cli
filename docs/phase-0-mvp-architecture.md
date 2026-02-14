# Phase 0 MVP Architecture Guide

Audience: beginner Rust developer with Zig and TypeScript experience.

This guide is architecture-only for **Phase 0: Setup and Rust Basics**.

## Phase 0 Goal

Create a clean project baseline so every next phase is fast and safe to build.

## Phase 0 Non-Goals

Do not implement these yet:
- full TUI behavior,
- chat loop,
- OpenAI/OAuth integration,
- persistence,
- tool execution.

## Architecture Decisions (Lock These Early)

1. **One binary crate for MVP**  
Keep one crate now, split by modules before split-by-crate complexity.

2. **Async runtime from day one**  
Use `tokio` early so later streaming/tool work does not require rewrites.

3. **Errors and logs are first-class**  
Use `anyhow` for boundaries and `tracing` for diagnostics.

4. **Module boundaries before implementation**  
Create folders for `app`, `agent`, `providers`, `tools`, `storage`, `model`.

## Proposed Base Layout

```text
src/
  main.rs
  app/
    mod.rs
  agent/
    mod.rs
  providers/
    mod.rs
  tools/
    mod.rs
  storage/
    mod.rs
  model/
    mod.rs
```

## Step-by-Step Build Plan (Checklist)

- [ ] Step 1: Verify Rust toolchain setup (`rustc`, `cargo`, `rustfmt`, `clippy`).
- [ ] Step 2: Confirm binary crate setup is correct (`cargo init --bin .` if needed).
- [ ] Step 3: Add core dependencies (`tokio`, `clap`, `anyhow`, `thiserror`, `serde`, `tracing`).
- [ ] Step 4: Add starter modules/folders for future architecture boundaries.
- [ ] Step 5: Create a minimal CLI entrypoint that prints help/version cleanly.
- [ ] Step 6: Initialize logging bootstrap (`tracing` + subscriber).
- [ ] Step 7: Define a minimal error boundary in `main.rs` (`Result<()>` style).
- [ ] Step 8: Decide project conventions (file naming, module naming, command naming).
- [ ] Step 9: Run `cargo fmt` and ensure formatting baseline is stable.
- [ ] Step 10: Run `cargo check` and fix compile issues.
- [ ] Step 11: Run `cargo clippy` and resolve critical warnings.
- [ ] Step 12: Record setup commands in docs so onboarding is repeatable.

## Phase 0 Done Criteria (Checklist)

- [ ] `cargo check` passes.
- [ ] `cargo fmt --check` passes.
- [ ] `cargo clippy` passes with no critical warnings.
- [ ] `maky --help` runs.
- [ ] Starter module layout exists for future phases.

## Rust Learning Focus

- `Result<T, E>` and `?` operator.
- Module system (`mod`, file layout).
- Basic ownership/borrowing in function signatures.
- Dependency and feature management in `Cargo.toml`.

## Handoff to Phase 1

When Phase 0 is complete:
- keep module boundaries stable,
- start terminal lifecycle + event loop architecture in Phase 1,
- avoid adding provider/auth logic yet.
