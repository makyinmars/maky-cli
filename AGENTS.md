# Repository Guidelines

## Project Context
This project is intentionally beginner-friendly: the maintainer is new to Rust and is building `maky-cli` to learn the language while becoming more familiar with agent CLI workflows. Favor clear, incremental changes over clever abstractions, and document non-obvious decisions in `docs/`.

## Project Structure & Module Organization
This repository is a Rust CLI crate. Keep runtime code in `src/` (currently `src/main.rs`), architecture and phase plans in `docs/`, and exploratory notes in `research/`. Build outputs belong in `target/` and should never be committed.  
When splitting the codebase beyond `main.rs`, follow the module boundaries described in `docs/implementation.md` (for example `app/`, `auth/`, `providers/`, `tools/`, `storage/`, `model/`).

## Build, Test, and Development Commands
Use Cargo for all local workflows:

```bash
cargo check                      # fast compile/type check
cargo run                        # run the CLI locally
cargo test                       # run unit/integration tests
cargo fmt                        # format source
cargo clippy --all-targets --all-features -- -D warnings
```

Run `cargo fmt` and `cargo clippy` before opening a PR.

## Coding Style & Naming Conventions
Use default Rust style (`rustfmt`, 4-space indentation). Prefer:
- `snake_case` for modules, files, functions, and variables.
- `UpperCamelCase` for structs/enums/traits.
- `SCREAMING_SNAKE_CASE` for constants.

Keep functions focused and avoid mixing terminal UI concerns with provider/tool/storage logic. Prefer `Result`-based error handling over panics in normal control flow.

## Testing Guidelines
Add tests when they protect important behavior or risky boundaries, not for every small implementation detail. Place unit tests next to the code they cover with `#[cfg(test)]`. Add integration tests under `tests/` when important behavior spans modules. Name tests by behavior, e.g. `loads_config_from_env` or `rejects_path_traversal`.  
At minimum, run `cargo test` before pushing. For filesystem-heavy tests, use `tempfile` to isolate state.

## Commit & Pull Request Guidelines
Current history uses short, direct commit subjects (examples: `adding docs`, `main update`). Keep that style: concise, present tense, one logical change per commit, ideally under 72 characters.

PRs should include:
- What changed and why.
- How to verify (`cargo test`, `cargo clippy`, manual CLI check).
- Linked issue(s) when applicable.
- Terminal screenshots/gifs for visible TUI behavior changes.

## Security & Configuration Tips
Never commit secrets or tokens. If API keys are added later, load them from environment variables (for example `OPENAI_API_KEY`) instead of hardcoding. Validate file and command operations against workspace boundaries.
