# Phase 7 MVP Architecture Guide

Audience: beginner Rust developer with Zig and TypeScript experience.

This guide is architecture-only for **Phase 7: Config and Model Controls**.

## Phase 7 Goal

Make app behavior configurable without code edits.

## Phase 7 Non-Goals

Do not implement these yet:
- remote config service,
- complex profile management UI,
- advanced feature-flag platform.

## Architecture Delta from Phase 6

1. **Configuration is layered and deterministic**  
Defaults < config file < env vars < CLI flags.

2. **Typed config model**  
Parse into typed Rust structs with validation.

3. **Runtime behavior references effective config only**  
Use one resolved config object for provider/auth/tools/session settings.

## Module Responsibilities (Phase 7)

- `storage/config.rs`: load/merge/validate config layers.
- `cli.rs`: parse CLI overrides and pass to config resolver.
- `main.rs`: build effective config before runtime startup.
- app/provider/auth/tool modules: consume read-only effective config.

## Core Data/Contract Shape

- `AppConfig` root struct with:
  - `provider`
  - `model`
  - `auth`
  - `openai`
  - `session_dir`
  - `approval_policy`
  - `ui`
- `EffectiveConfig` output from layering pipeline.
- optional `ConfigError` typed domain errors.

## Step-by-Step Build Plan (Checklist)

- [ ] Step 1: Define typed config structs and default values.
- [ ] Step 2: Define config file location policy and filename.
- [ ] Step 3: Define environment variable mapping policy.
- [ ] Step 4: Define CLI override flags (`--model`, `--resume`, auth mode overrides if needed).
- [ ] Step 5: Implement merge order contract (defaults -> file -> env -> CLI).
- [ ] Step 6: Add config validation rules (missing keys, invalid model/auth combinations).
- [ ] Step 7: Define startup error messages for invalid config.
- [ ] Step 8: Expose effective config summary for debugging/logging.
- [ ] Step 9: Ensure provider/auth/tool/session modules consume only effective config.
- [ ] Step 10: Define default model persistence/update behavior.

## Phase 7 Done Criteria (Checklist)

- [ ] Config can be loaded from file.
- [ ] Env vars override file values.
- [ ] CLI flags override env/file values.
- [ ] Model can be changed without code edits.
- [ ] Effective config behavior is predictable and documented.

## Rust Learning Focus

- Struct modeling with `serde`.
- Validation and error layering patterns.
- Clean separation between parsing, validation, and runtime usage.

## Handoff to Phase 8

When Phase 7 is complete:
- stabilize config contracts,
- focus next on reliability hardening and test coverage.
