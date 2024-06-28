# Integrations

## Editor Integrations

### Ruff Language Server (`ruff server`)

The Ruff language server, also known as `ruff server`, is what powers the diagnostic and formatting capabilities of Ruff's VS Code extension and other editors. It is a single, common backend for editor integrations built directly into Ruff, and a direct replacement for `ruff-lsp`, our previous language server. You can read more about `ruff server` in the [`v0.4.5` blog post](https://astral.sh/blog/ruff-v0.4.5).

`ruff server` uses the LSP (Language Server Protocol), and as such works with any editor that supports this protocol. VS Code, Neovim, Helix, and Kate all have official, documented support for `ruff server`. Other editors may support the older `ruff-lsp` server - see the [Setup](https://github.com/astral-sh/ruff-lsp#setup) guide in `ruff-lsp` for more details.

### VS Code (Official)

Download the [Ruff VS Code extension](https://marketplace.visualstudio.com/items?itemName=charliermarsh.ruff), which supports fix actions, import sorting, and more.

By default, the extension will attempt to use the `ruff` executable installed on your local system, and will use a bundled executable if that's not available. If `ruff` is `v0.4.5` or later, the extension will automatically use `ruff server` - otherwise, it will use `ruff-lsp`.

To learn more about configuring the extension, and the settings available for the extension, refer to [Configuring Ruff](https://github.com/astral-sh/ruff-vscode/?tab=readme-ov-file#configuring-ruff) in the `ruff-vscode` documentation.

### Vim & Neovim (Official)

#### Using `nvim-lspconfig`

1. Install [`nvim-lspconfig`](https://github.com/neovim/nvim-lspconfig).
1. Setup `nvim-lspconfig` with the [suggested configuration](https://github.com/neovim/nvim-lspconfig/tree/master#suggested-configuration).
1. Finally, add this to your `init.lua`:

```lua
require('lspconfig').ruff.setup {}
```

See [`nvim-lspconfig`'s server configuration guide](https://github.com/neovim/nvim-lspconfig/blob/master/doc/server_configurations.md#ruff) for more details
on how to configure the server from there.

> \[!IMPORTANT\]
>
> If you have the older language server (`ruff-lsp`) configured in Neovim, make sure to disable it to prevent any conflicts.

##### Tips

If you're using Ruff alongside another LSP (like Pyright), you may want to defer to that LSP for certain capabilities,
like `textDocument/hover`:

```lua
local on_attach = function(client, bufnr)
  if client.name == 'ruff' then
    -- Disable hover in favor of Pyright
    client.server_capabilities.hoverProvider = false
  end
end

require('lspconfig').ruff.setup {
  on_attach = on_attach,
}
```

If you'd like to use Ruff exclusively for linting, formatting, and import organization, you can disable those
capabilities for Pyright:

```lua
require('lspconfig').pyright.setup {
  settings = {
    pyright = {
      -- Using Ruff's import organizer
      disableOrganizeImports = true,
    },
    python = {
      analysis = {
        -- Ignore all files for analysis to exclusively use Ruff for linting
        ignore = { '*' },
      },
    },
  },
}
```

By default, Ruff will not show any logs. To enable logging in Neovim, you'll need to set the `RUFF_TRACE` environment variable
to either `messages` or `verbose`:

```lua
require('lspconfig').ruff.setup {
  cmd_env = { RUFF_TRACE = "messages" }
}
```

You can set the log level in `settings`:

```lua
require('lspconfig').ruff.setup {
  cmd_env = { RUFF_TRACE = "messages" },
  init_options = {
    settings = {
      logLevel = "debug",
    }
  }
}
```

It's also possible to divert Ruff's logs to a separate file with the `logFile` setting:

```lua
require('lspconfig').ruff.setup {
  cmd_env = { RUFF_TRACE = "messages" },
  init_options = {
    settings = {
      logLevel = "debug",
      logFile = "~/.local/state/nvim/ruff.log"
    }
  }
}
```

The `logFile` path supports tildes and environment variables.

### Helix (Official)

First, open the language configuration file for Helix. On Linux and macOS, this will be at `~/.config/helix/languages.toml`,
and on Windows this will be at `%AppData%\helix\languages.toml`.

Add the language server by adding:

```toml
[language-server.ruff]
command = "ruff"
args = ["server", "--preview"]
```

Then, you'll register the language server as the one to use with Python.
If you don't already have a language server registered to use with Python, add this to `languages.toml`:

```toml
[[language]]
name = "python"
language-servers = ["ruff"]
```

Otherwise, if you already have `language-servers` defined, you can simply add `"ruff"` to the list. For example,
if you already have `pylsp` as a language server, you can modify the language entry as follows:

```toml
[[language]]
name = "python"
language-servers = ["ruff", "pylsp"]
```

> \[!NOTE\]
> Multiple language servers for a single language are only supported in Helix version [`23.10`](https://github.com/helix-editor/helix/blob/master/CHANGELOG.md#2310-2023-10-24) and later.

Once you've set up the server, you should see diagnostics in your Python files. Code actions and other LSP features should also be available.

![A screenshot showing an open Python file in Helix with highlighted diagnostics and a code action dropdown menu open](assets/SuccessfulHelixSetup.png)
*This screenshot is using `select=["ALL]"` for demonstration purposes.*

If you want to, as an example, turn on auto-formatting, add `auto-format = true`:

```toml
[[language]]
name = "python"
language-servers = ["ruff", "pylsp"]
auto-format = true
```

See the [Helix documentation](https://docs.helix-editor.com/languages.html) for more settings you can use here.

You can pass settings into `ruff server` using `[language-server.ruff.config.settings]`. For example:

```toml
[language-server.ruff.config.settings]
lineLength = 80
[language-server.ruff.config.settings.lint]
select = ["E4", "E7"]
preview = false
[language-server.ruff.config.settings.format]
preview = true
```

By default, Ruff does not log anything to Helix. To enable logging, set the `RUFF_TRACE` environment variable
to either `messages` or `verbose`.

```toml
[language-server.ruff]
command = "ruff"
args = ["server", "--preview"]
environment = { "RUFF_TRACE" = "messages" }
```

> \[!NOTE\]
> `RUFF_TRACE=verbose` does not enable Helix's verbose mode by itself. You'll need to run Helix with `-v` for verbose logging.

To change the log level for Ruff (which is `info` by default), use the `logLevel` setting:

```toml
[language-server.ruff]
command = "ruff"
args = ["server", "--preview"]
environment = { "RUFF_TRACE" = "messages" }

[language-server.ruff.config.settings]
logLevel = "debug"
```

You can also divert Ruff's logs to a separate file with the `logFile` setting:

```toml
[language-server.ruff]
command = "ruff"
args = ["server", "--preview"]
environment = { "RUFF_TRACE" = "messages" }

[language-server.ruff.config.settings]
logLevel = "debug"
logFile = "~/.cache/helix/ruff.log"
```

The `logFile` path supports tildes and environment variables.

### Kate (Official)

1. Activate the [LSP Client plugin](https://docs.kde.org/stable5/en/kate/kate/plugins.html#kate-application-plugins).
1. Setup LSP Client [as desired](https://docs.kde.org/stable5/en/kate/kate/kate-application-plugin-lspclient.html).
1. Finally, add this to `Settings` -> `Configure Kate` -> `LSP Client` -> `User Server Settings`:

```json
{
  "servers": {
    "python": {
      "command": ["ruff", "server", "--preview"],
      "url": "https://github.com/astral-sh/ruff",
      "highlightingModeRegex": "^Python$",
      "settings": {}
    }
  }
}
```

See [LSP Client documentation](https://docs.kde.org/stable5/en/kate/kate/kate-application-plugin-lspclient.html) for more details
on how to configure the server from there.

> \[!IMPORTANT\]
>
> Kate's LSP Client plugin does not support multiple servers for the same language.

### PyCharm (External Tool)

Ruff can be installed as an [External Tool](https://www.jetbrains.com/help/pycharm/configuring-third-party-tools.html)
in PyCharm. Open the Preferences pane, then navigate to "Tools", then "External Tools". From there,
add a new tool with the following configuration:

![Install Ruff as an External Tool](https://user-images.githubusercontent.com/1309177/193155720-336e43f0-1a8d-46b4-bc12-e60f9ae01f7e.png)

Ruff should then appear as a runnable action:

![Ruff as a runnable action](https://user-images.githubusercontent.com/1309177/193156026-732b0aaf-3dd9-4549-9b4d-2de6d2168a33.png)

### PyCharm (Unofficial)

Ruff is also available as the [Ruff](https://plugins.jetbrains.com/plugin/20574-ruff) plugin on the
IntelliJ Marketplace (maintained by @koxudaxi).

### Emacs (Unofficial)

Ruff is available as [`flymake-ruff`](https://melpa.org/#/flymake-ruff) on MELPA:

```elisp
(require 'flymake-ruff)
(add-hook 'python-mode-hook #'flymake-ruff-load)
```

Ruff is also available as [`emacs-ruff-format`](https://github.com/scop/emacs-ruff-format):

```elisp
(require 'ruff-format)
(add-hook 'python-mode-hook 'ruff-format-on-save-mode)
```

Alternatively, it can be used via the [Apheleia](https://github.com/radian-software/apheleia) formatter library, by setting this configuration:

```emacs-lisp
(add-to-list 'apheleia-mode-alist '(python-mode . ruff))
(add-to-list 'apheleia-mode-alist '(python-ts-mode . ruff))
```

### TextMate (Unofficial)

Ruff is also available via the [`textmate2-ruff-linter`](https://github.com/vigo/textmate2-ruff-linter)
bundle for TextMate.

## Other Integrations

### pre-commit

Ruff can be used as a [pre-commit](https://pre-commit.com) hook via [`ruff-pre-commit`](https://github.com/astral-sh/ruff-pre-commit):

```yaml
- repo: https://github.com/astral-sh/ruff-pre-commit
  # Ruff version.
  rev: v0.4.10
  hooks:
    # Run the linter.
    - id: ruff
    # Run the formatter.
    - id: ruff-format
```

To enable lint fixes, add the `--fix` argument to the lint hook:

```yaml
- repo: https://github.com/astral-sh/ruff-pre-commit
  # Ruff version.
  rev: v0.4.10
  hooks:
    # Run the linter.
    - id: ruff
      args: [ --fix ]
    # Run the formatter.
    - id: ruff-format
```

To run the hooks over Jupyter Notebooks too, add `jupyter` to the list of allowed filetypes:

```yaml
- repo: https://github.com/astral-sh/ruff-pre-commit
  # Ruff version.
  rev: v0.4.10
  hooks:
    # Run the linter.
    - id: ruff
      types_or: [ python, pyi, jupyter ]
      args: [ --fix ]
    # Run the formatter.
    - id: ruff-format
      types_or: [ python, pyi, jupyter ]
```

When running with `--fix`, Ruff's lint hook should be placed _before_ Ruff's formatter hook, and
_before_ Black, isort, and other formatting tools, as Ruff's fix behavior can output code changes
that require reformatting.

When running without `--fix`, Ruff's formatter hook can be placed before or after Ruff's lint hook.

(As long as your Ruff configuration avoids any [linter-formatter incompatibilities](formatter.md#conflicting-lint-rules),
`ruff format` should never introduce new lint errors, so it's safe to run Ruff's format hook _after_
`ruff check --fix`.)

### mdformat (Unofficial)

[mdformat](https://mdformat.readthedocs.io/en/stable/users/plugins.html#code-formatter-plugins) is
capable of formatting code blocks within Markdown. The [`mdformat-ruff`](https://github.com/Freed-Wu/mdformat-ruff)
plugin enables mdformat to format Python code blocks with Ruff.

### GitHub Actions

GitHub Actions has everything you need to run Ruff out-of-the-box:

```yaml
name: CI
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install Python
        uses: actions/setup-python@v5
        with:
          python-version: "3.11"
      - name: Install dependencies
        run: |
          python -m pip install --upgrade pip
          pip install ruff
      # Update output format to enable automatic inline annotations.
      - name: Run Ruff
        run: ruff check --output-format=github .
```

Ruff can also be used as a GitHub Action via [`ruff-action`](https://github.com/chartboost/ruff-action).

By default, `ruff-action` runs as a pass-fail test to ensure that a given repository doesn't contain
any lint rule violations as per its [configuration](https://github.com/astral-sh/ruff/blob/main/docs/configuration.md).
However, under-the-hood, `ruff-action` installs and runs `ruff` directly, so it can be used to
execute any supported `ruff` command (e.g., `ruff check --fix`).

`ruff-action` supports all GitHub-hosted runners, and can be used with any published Ruff version
(i.e., any version available on [PyPI](https://pypi.org/project/ruff/)).

To use `ruff-action`, create a file (e.g., `.github/workflows/ruff.yml`) inside your repository
with:

```yaml
name: Ruff
on: [ push, pull_request ]
jobs:
  ruff:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: chartboost/ruff-action@v1
```

Alternatively, you can include `ruff-action` as a step in any other workflow file:

```yaml
      - uses: chartboost/ruff-action@v1
```

`ruff-action` accepts optional configuration parameters via `with:`, including:

- `version`: The Ruff version to install (default: latest).
- `args`: The command-line arguments to pass to Ruff (default: `"check"`).
- `src`: The source paths to pass to Ruff (default: `"."`).

For example, to run `ruff check --select B ./src` using Ruff version `0.0.259`:

```yaml
- uses: chartboost/ruff-action@v1
  with:
    version: 0.0.259
    args: check --select B
    src: "./src"
```
