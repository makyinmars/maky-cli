# Phase 3 MVP Architecture Guide

Audience: beginner Rust developer with Zig and TypeScript experience.

This guide is architecture-only for **Phase 3: OAuth Login (ChatGPT Plans)**.

## Phase 3 Goal

Add authentication architecture so users can sign in via OAuth and stay signed in.

## Phase 3 Non-Goals

Do not implement these yet:

- full streaming provider integration,
- tool execution,
- advanced multi-provider auth UX.

## Architecture Delta from Phase 2

1. **Auth is a dedicated boundary**  
`auth` module handles login/refresh/session storage.

2. **Credentials are resolved at app boundary**  
Prefer OAuth session, fallback to `OPENAI_API_KEY`.

3. **Token storage is abstracted**  
Use keyring-first strategy with explicit fallback policy.

## Proposed Module Additions

```text
src/auth/
  mod.rs
  provider.rs
  oauth_chatgpt.rs
  token_store.rs
```

Responsibility split:

- `provider.rs`: auth trait and contract types.
- `oauth_chatgpt.rs`: OAuth flow-specific logic.
- `token_store.rs`: secure persistence boundary.

## Core Data/Contract Shape

- `AuthSession { access_token, refresh_token, expires_at, provider_id, id_token?, account_id? }`
- `AuthStatus` enum (`SignedOut | SignedIn | Expired | Refreshing`)
- `AuthProvider` trait (`login`, `refresh_if_needed`)
- `TokenStore` trait (`load`, `save`, `clear`)

## Step-by-Step Build Plan (Checklist)

- [x] Step 1: Define auth domain models and trait boundaries.
- [x] Step 2: Add startup credential resolution policy (OAuth first, API key fallback).
- [x] Step 3: Add token-store abstraction and keyring-first policy.
- [x] Step 4: Define file fallback policy and warning behavior when keyring is unavailable.
- [x] Step 5: Add `/login` command architecture path.
- [x] Step 6: Add `/auth` command architecture path for status inspection.
- [x] Step 7: Add `/logout` command architecture path for session removal.
- [x] Step 8: Define refresh-before-request behavior contract.
- [x] Step 9: Define auth failure surfaces (status line + logs).
- [x] Step 10: Validate restart behavior expectation (session persists across runs).

## Phase 3 Done Criteria (Checklist)

- [x] OAuth login flow can be initiated from CLI/TUI commands.
- [x] Auth status can be queried.
- [x] Logout path is defined and clears active session data.
- [x] Session persistence strategy is defined.
- [x] Expired token refresh behavior is defined before model requests.

## Rust Learning Focus

- Trait-based architecture boundaries.
- Domain modeling with structs/enums.
- Error typing for auth domain (`thiserror`) and boundary propagation (`anyhow`).

## Handoff to Phase 4

When Phase 3 is complete:

- pass resolved credentials into provider layer,
- keep auth independent from TUI rendering details.
