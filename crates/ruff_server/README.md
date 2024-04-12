## The Ruff Language Server

Welcome! `ruff server` is a language server that powers editor integrations with Ruff. The job of the language server is to
listen for requests from the client, (in this case, the code editor of your choice) and call into Ruff's linter and formatter
crates to create real-time diagnostics or formatted code, which is then sent back to the client. It also tracks configuration
files in your editor's workspace, and will refresh its in-memory configuration whenever those files are modified.

### Contributing

If you're interested in contributing to `ruff server` - well, first of all, thank you! Second of all, you might find the [**contribution guide**](CONTRIBUTING.md) to be a useful resource. Finally, don't hesitate to reach out on our [**Discord**](https://discord.com/invite/astral-sh) if you have questions.
