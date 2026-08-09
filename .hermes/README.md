# Hermes Agent Configuration for ruff

This directory contains Hermes Agent configuration for contributing to and working with the **ruff** repository.

## What is Hermes Agent?

[Hermes Agent](https://hermes-agent.nousresearch.com/) is an open-source AI agent framework by Nous Research that runs in your terminal, desktop app, and IDE. It's built for autonomous code generation, multi-agent workflows, and seamless developer collaboration.

Unlike traditional AI coding assistants, Hermes:
- **Learns from experience** through reusable skills
- **Persists memory** across sessions and projects
- **Multi-platform** — runs on CLI, desktop, Slack, Discord, Teams, and more
- **Provider-agnostic** — swap models mid-workflow without reconfiguring
- **Self-improving** — saves procedures as skills for reuse in future projects

## Why This Config Exists

This `.hermes/` directory standardizes the development environment for ruff within Hermes Agent sessions. It provides:

- **Consistent setup instructions** — automatically installs Rust, uv, and required dev tools
- **Verification commands** — quickly confirm that your build environment is working (tests, linting, clippy)
- **Metadata** — language, repository links, stars, tags for discovery
- **Developer context** — Hermes can load this config to understand the repo's structure without asking

## How to Use

### Quick Start with Hermes

```bash
hermes chat -q "Help me set up ruff for development and run the test suite."
```

Hermes will:
1. Load this `.hermes/config.yaml`
2. Follow the setup steps (install Rust, uv, build tools)
3. Build the project
4. Run verification commands (tests, linting, clippy)
5. Report success or flag missing dependencies

### Manual Setup

If you prefer to set up without Hermes:

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

# Install uv
curl -LsSf https://astral.sh/uv/install.sh | sh

# Install dev tools
cargo install cargo-insta
cargo install cargo-nextest --locked

# Install development dependencies
uv run --only-group dev --locked prek install

# Build the project
cargo build --release
```

### Verification

Confirm everything is working:

```bash
cargo nextest run --release      # Run tests faster
cargo fmt -- --check             # Check formatting
cargo clippy --all-targets --all-features  # Lint
cargo run --release --bin ruff -- --version  # Binary verification
```

## Config Structure

- **name** — Repository name (`ruff`)
- **description** — What the project does
- **language** — Primary languages (Rust, Python)
- **repository** — Upstream and fork URLs
- **setup** — Step-by-step build instructions
- **verification** — Commands to confirm a working build
- **tags** — Searchable keywords (e.g., `linter`, `formatter`, `rust`, `cli-tool`)

## Contributing

See the upstream repository's [CONTRIBUTING.md](../CONTRIBUTING.md) for contribution guidelines.

Key points:
- Ruff is part of the Astral ecosystem
- Follow the Rust and Python contribution guidelines
- Use `cargo fmt` and `cargo clippy` before submitting PRs

## Issues or Improvements?

If this config is outdated or incomplete:
1. Update `.hermes/config.yaml` with the corrected setup/verification steps
2. Test locally: `cargo build && cargo nextest run --release`
3. Commit and push to your fork
4. Open a PR with description of changes

---

**Last updated:** 2026-08-09  
**Repo:** [astral-sh/ruff](https://github.com/astral-sh/ruff)  
**Stars:** 49,112 ⭐
