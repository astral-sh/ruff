## The Ruff Language Server

Welcome!

`ruff server` is a language server that powers Ruff's editor integrations.

The job of the language server is to listen for requests from the client (in this case, the code editor of your choice)
and call into Ruff's linter and formatter crates to construct real-time diagnostics or formatted code, which is then
sent back to the client. It also tracks configuration files in your editor's workspace, and will refresh its in-memory
configuration whenever those files are modified.

### Setup

We have specific setup instructions depending on your editor. If you don't see your editor on this list and would like a
setup guide, please open an issue.

If you're transferring your configuration from [`ruff-lsp`](https://github.com/astral-sh/ruff-lsp), regardless of
editor, there are several settings which have changed or are no longer available. See the [migration guide](docs/MIGRATION.md) for
more.

#### VS Code

Install the Ruff extension from the [VS Code Marketplace](https://marketplace.visualstudio.com/items?itemName=charliermarsh.ruff).

As this server is still in Beta, you will need to enable the "Native Server" extension setting, either in the settings
UI:

![A screenshot showing an enabled "Native Server" extension setting in the VS Code settings view](assets/nativeServer.png)

Or in your `settings.json`:

```json
{
  "ruff.nativeServer": true
}
```

From there, you can configure Ruff to format Python code on-save with:

```json
{
  "[python]": {
    "editor.formatOnSave": true,
    "editor.defaultFormatter": "charliermarsh.ruff"
  }
}
```

For more, see [_Configuring VS Code_](https://github.com/astral-sh/ruff-vscode?tab=readme-ov-file#configuring-vs-code)
in the Ruff extension documentation.

By default, the extension will run against the `ruff` binary that it discovers in your environment. If you don't have
`ruff` installed, the extension will fall back to a bundled version of the binary.

#### Neovim

See the [Neovim setup guide](docs/setup/NEOVIM.md).

#### Helix

See the [Helix setup guide](docs/setup//HELIX.md).

#### Vim

See the [Vim setup guide](docs/setup/VIM.md).

#### Kate

See the [Kate setup guide](docs/setup/KATE.md).

### Contributing

If you're interested in contributing to `ruff server` - well, first of all, thank you! Second of all, you might find the
[**contribution guide**](CONTRIBUTING.md) to be a useful resource.

Finally, don't hesitate to reach out on [**Discord**](https://discord.com/invite/astral-sh) if you have questions.
