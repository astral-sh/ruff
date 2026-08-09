# Hermes Agent Guidelines for Ruff

This folder helps Hermes agents (AI coding assistants) understand and safely contribute to this repository.

## Repository Overview

- **Purpose:** Fast Python linter and code formatter (Rust implementation)
- **Language:** Rust
- **Build tool:** cargo
- **Test command:** `cargo test` (or `cargo nextest run`)

## What Hermes Should Do

✓ Fix bugs from reported issues labeled `help wanted` or `good first issue`  
✓ Add or improve tests for lint rules and edge cases  
✓ Improve documentation and error messages  
✓ Refactor code for clarity (small scope)  
✓ Add test coverage for specific lint rules  

## What Hermes Should NOT Do

✗ Add new lint rules or modify existing rules without discussion  
✗ Major architectural changes without team consensus  
✗ Add external dependencies  
✗ Modify CI/CD workflows (`.github/workflows/**`)  
✗ Touch lock files (`Cargo.lock`)  
✗ Work on issues labeled `needs-decision` or `needs-design`  

## Setup Instructions

```bash
# Install Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install dependencies
cargo install cargo-insta
cargo install cargo-nextest --locked
uv run --only-group dev --locked prek install

# Clone and build
git clone https://github.com/astral-sh/ruff.git
cd ruff
cargo build --release
```

## Verification Commands

Before submitting a PR, Hermes must verify:

```bash
# Formatting
cargo fmt --check

# Linting
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Tests and snapshots
RUFF_UPDATE_SCHEMA=1 cargo test
cargo insta review
```

## Key Files to Understand

- `CONTRIBUTING.md` — contribution guidelines and AI policy
- `README.md` — project overview
- `Cargo.toml` — workspace manifest
- `crates/ruff_linter/` — main linter logic and rules
- `crates/ruff/` — CLI binary
- `tests/` — integration tests

## Issue Labels to Target

Good for Hermes contributions:
- `good first issue` — lower barrier to entry
- `help wanted` — explicitly good for community
- `bug` — concrete, scoped fixes
- `documentation` — writing improvements

Avoid:
- `needs-decision` — needs team input first
- `needs-design` — requires consensus
- `blocked-by-upstream` — external blocker

## Quick Tips

1. Read recent merged PRs to understand patterns
2. Always run `cargo test` locally before committing
3. Follow Rust idioms and existing code style
4. Keep PRs focused — one rule/feature per PR
5. Reference the issue number in your commit message
6. Check AI Policy: https://github.com/astral-sh/.github/blob/main/AI_POLICY.md

---

For more about Hermes Agent, see: https://hermes-agent.nousresearch.com
