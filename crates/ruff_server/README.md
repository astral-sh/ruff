## `ruff server`

Welcome! `ruff server` is a language server that powers editor integrations with Ruff. The job of the language server is to
listen for requests from the client, (in this case, the code editor of your choice) and call into Ruff's linter and formatter
crates to create real-time diagnostics or formatted code, which is then sent back to the client. It also tracks configuration
files in your editor's workspace, and will refresh its in-memory configuration whenever those files are modified.

### Roadmap

`ruff server` is still in an early stage of development. This roadmap is an overview of the planned features for `ruff server` so far.
It is subject to change as the project evolves.

- [x] Lint diagnostics show in Python files
- [x] Quick Fixes for diagnostics
- [x] Full-document formatting
- [x] Range formatting
- [x] Multi-threaded, lock-free architecture
- [x] In-memory document cache that tracks changes to unsaved files
- [x] NeoVim support
- [x] VSCode support (pre-release extension only, with limitations)
- [x] Uses `pyproject.toml`/`ruff.toml`/`.ruff.toml` for linter/formatter configuration, per workspace folder
- [x] Automatic configuration reloading when a config file is changed on disk
- [ ] Jupyter Notebook document support
- [ ] Source-level Code Actions / Commands for VS Code (Fix All, Organize Imports)
- [ ] Substantial test suite for features and common scenarios
- [ ] Sublime Text support
- [ ] Support for extension-specific configuration and settings
- [ ] Improved scheduler with support for event handling and task cancellation
- [ ] Support for Helix, Lapce, Kate, and Zed
- [ ] Proper configuration overrides for sub-folder configuration
- [ ] Optimized calls to Ruff (for example: cached diagnostics)
- [ ] Workspace-wide diagnostics window
- [ ] Task progress bar
- [ ] Test suite that directly emulates real editor sessions

### Contributing

If you're interested in contributing to `ruff server` - well, first of all, thank you! Second of all, you might find the [**contribution guide**](CONTRIBUTING.md) to be a useful resource. Finally, don't hesitate to reach out on our [**Discord**](https://discord.com/invite/astral-sh) if you have questions.
