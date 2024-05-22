## The Ruff Language Server

Welcome! `ruff server` is a language server that powers editor integrations with Ruff. The job of the language server is to
listen for requests from the client, (in this case, the code editor of your choice) and call into Ruff's linter and formatter
crates to create real-time diagnostics or formatted code, which is then sent back to the client. It also tracks configuration
files in your editor's workspace, and will refresh its in-memory configuration whenever those files are modified.

### Setup

We have specific setup instructions depending on your editor. If you don't see your editor on this list and would like a setup guide, please open an issue.

#### Visual Studio Code

Install the [Ruff extension from the VS Code Marketplace](https://marketplace.visualstudio.com/items?itemName=charliermarsh.ruff).

As this server is still in beta, you will need to enable the `Native Server` extension setting:

![A screenshot showing an enabled "Native Server" extension setting in the VS Code settings view](assets/nativeServer.png)

You can also set it in your user / workspace JSON settings as follows:

```json
"ruff.nativeServer": true
```

The language server used by the extension will be, by default, the one in your actively-installed `ruff` binary. If you don't have `ruff` installed and haven't provided a path to the extension, it comes with a bundled `ruff` version that it will use instead.

#### Neovim

See the [Neovim setup guide](docs/setup/NEOVIM.md).

#### Helix

See the [Helix setup guide](docs/setup//HELIX.md).

If you are transferring your configuration from `ruff-lsp`, regardless of editor, there are several settings which have changed or are no longer available which you should be aware of. See the [migration guide](docs/MIGRATION.md) for more details.

### Contributing

If you're interested in contributing to `ruff server` - well, first of all, thank you! Second of all, you might find the [**contribution guide**](CONTRIBUTING.md) to be a useful resource. Finally, don't hesitate to reach out on our [**Discord**](https://discord.com/invite/astral-sh) if you have questions.
