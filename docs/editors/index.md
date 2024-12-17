# Editor Integrations

Ruff can be integrated with various editors and IDEs to provide a seamless development experience.
This section provides instructions on [how to set up Ruff with your editor](./setup.md) and [configure it to your
liking](./settings.md).

## Language Server Protocol

The editor integration is mainly powered by the Ruff Language Server which implements the
[Language Server Protocol](https://microsoft.github.io/language-server-protocol/). The server is
written in Rust and is available as part of the `ruff` CLI via `ruff server`. It is a single, common
backend built directly into Ruff, and a direct replacement for [`ruff-lsp`](https://github.com/astral-sh/ruff-lsp),
our previous language server. You can read more about `ruff server` in the
[`v0.4.5` blog post](https://astral.sh/blog/ruff-v0.4.5).

The server supports surfacing Ruff diagnostics, providing Code Actions to fix them, and
formatting the code using Ruff's built-in formatter. Currently, the server is intended to be used
alongside another Python Language Server in order to support features like navigation and
autocompletion.

The Ruff Language Server was available first in Ruff [v0.4.5](https://astral.sh/blog/ruff-v0.4.5)
in beta and stabilized in Ruff [v0.5.3](https://github.com/astral-sh/ruff/releases/tag/0.5.3).

!!! note

    This is the documentation for Ruff's built-in language server written in Rust (`ruff server`).
    If you are looking for the documentation for the `ruff-lsp` language server, please refer to the
    [README](https://github.com/astral-sh/ruff-lsp) of the `ruff-lsp` repository.
