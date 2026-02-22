# Phase 6 MVP Architecture Guide

Audience: beginner Rust developer with Zig and TypeScript experience.

This guide is architecture-only for **Phase 6: Tools v0 with Safety**.

## Phase 6 Goal

Add minimal tool execution with explicit user approval and workspace safety boundaries.

## Phase 6 Non-Goals

Do not implement these yet:
- full plugin/marketplace system,
- broad filesystem/network access,
- autonomous unsafe command execution.

## Architecture Delta from Phase 5

1. **Tool boundary is explicit and typed**  
All model-requested actions pass through `ToolHandler` contracts.

2. **Approval gate is mandatory for risky actions**  
`exec_command` requires explicit approval before execution.

3. **Safety checks happen before execution**  
Path validation and workspace boundaries are enforced first.

## Module Responsibilities (Phase 6)

- `tools/registry.rs`: tool registration and lookup.
- `tools/read.rs`: workspace-limited file read.
- `tools/ls.rs`: workspace-limited list files.
- `tools/exec.rs`: command execution with approval requirement.
- `app/controller.rs`: approval UI flow and tool result routing.

## Core Data/Contract Shape

- `ToolCall { id, name, args }`
- `ToolResult { call_id, output, error?, truncated? }`
- `ApprovalRequest { tool_name, summary, risk_level }`
- `ApprovalDecision` enum (`AllowOnce`, `Deny`)

## Step-by-Step Build Plan (Checklist)

- [x] Step 1: Define `ToolHandler` trait and tool registry contract.
- [x] Step 2: Define tool call/result event types for provider-session pipeline.
- [x] Step 3: Add workspace-root canonicalization policy for path safety.
- [x] Step 4: Add `list_files` architecture boundary and arguments schema.
- [x] Step 5: Add `read_file` architecture boundary and arguments schema.
- [x] Step 6: Add `exec_command` architecture boundary and arguments schema.
- [x] Step 7: Define approval UI state and decision routing in controller.
- [x] Step 8: Enforce approval requirement before risky tool execution.
- [x] Step 9: Define stdout/stderr capture and output truncation policy.
- [x] Step 10: Persist tool calls, approvals, and results in session events.
- [x] Step 11: Define denial behavior and user-visible status messaging.

## Phase 6 Done Criteria (Checklist)

- [x] At least one tool call path works end-to-end.
- [x] Risky tool actions require explicit approval.
- [x] Path traversal/symlink escape protections are defined.
- [x] Tool outputs are displayed and persisted.
- [x] Denied tool requests do not execute and are visible in UI/logs.

## Rust Learning Focus

- Trait-based polymorphism for handlers.
- Process execution boundaries (`tokio::process::Command` style).
- Defensive coding for untrusted inputs.

## Handoff to Phase 7

When Phase 6 is complete:
- keep tool contracts stable,
- make behavior configurable through file/env/CLI layering next.
