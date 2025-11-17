# Setup

We have specific setup instructions depending on your editor of choice. If you don't see your editor on this
list and would like a setup guide, please open an issue.

If you're transferring your configuration from [`ruff-lsp`](https://github.com/astral-sh/ruff-lsp),
regardless of editor, there are several settings which have changed or are no longer available. See
the [migration guide](./migration.md) for more.

!!! note

    The setup instructions provided below are on a best-effort basis. If you encounter any issues
    while setting up the Ruff in an editor, please [open an issue](https://github.com/astral-sh/ruff/issues/new)
    for assistance and help in improving this documentation.

!!! tip

    Regardless of the editor, it is recommended to disable the older language server
    ([`ruff-lsp`](https://github.com/astral-sh/ruff-lsp)) to prevent any conflicts.

## VS Code

Install the Ruff extension from the [VS Code
Marketplace](https://marketplace.visualstudio.com/items?itemName=charliermarsh.ruff). It is
recommended to have the Ruff extension version `2024.32.0` or later to get the best experience with
the Ruff Language Server.

For more documentation on the Ruff extension, refer to the
[README](https://github.com/astral-sh/ruff-vscode/blob/main/README.md) of the extension repository.

## Neovim

The [`nvim-lspconfig`](https://github.com/neovim/nvim-lspconfig) plugin can be used to configure the
Ruff Language Server in Neovim. To set it up, install
[`nvim-lspconfig`](https://github.com/neovim/nvim-lspconfig) plugin, set it up as per the
[configuration](https://github.com/neovim/nvim-lspconfig#configuration) documentation, and add the
following to your `init.lua`:

=== "Neovim 0.10 (with [`nvim-lspconfig`](https://github.com/neovim/nvim-lspconfig))"

    ```lua
    require('lspconfig').ruff.setup({
      init_options = {
        settings = {
          -- Ruff language server settings go here
        }
      }
    })
    ```

=== "Neovim 0.11+ (with [`vim.lsp.config`](https://neovim.io/doc/user/lsp.html#vim.lsp.config()))"

    ```lua
    vim.lsp.config('ruff', {
      init_options = {
        settings = {
          -- Ruff language server settings go here
        }
      }
    })

    vim.lsp.enable('ruff')
    ```

!!! note

    If the installed version of `nvim-lspconfig` includes the changes from
    [neovim/nvim-lspconfig@`70d1c2c`](https://github.com/neovim/nvim-lspconfig/commit/70d1c2c31a88af4b36019dc1551be16bffb8f9db),
    you will need to use Ruff version `0.5.3` or later.

If you're using Ruff alongside another language server (like Pyright), you may want to defer to that
language server for certain capabilities, like [`textDocument/hover`](./features.md#hover):

```lua
vim.api.nvim_create_autocmd("LspAttach", {
  group = vim.api.nvim_create_augroup('lsp_attach_disable_ruff_hover', { clear = true }),
  callback = function(args)
    local client = vim.lsp.get_client_by_id(args.data.client_id)
    if client == nil then
      return
    end
    if client.name == 'ruff' then
      -- Disable hover in favor of Pyright
      client.server_capabilities.hoverProvider = false
    end
  end,
  desc = 'LSP: Disable hover capability from Ruff',
})
```

If you'd like to use Ruff exclusively for linting, formatting, and organizing imports, you can disable those
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

By default, the log level for Ruff is set to `info`. To change the log level, you can set the
[`logLevel`](./settings.md#loglevel) setting:

```lua
require('lspconfig').ruff.setup {
  init_options = {
    settings = {
      logLevel = 'debug',
    }
  }
}
```

By default, Ruff will write logs to stderr which will be available in Neovim's LSP client log file
(`:lua vim.print(vim.lsp.get_log_path())`). It's also possible to divert these logs to a separate
file with the [`logFile`](./settings.md#logfile) setting.

To view the trace logs between Neovim and Ruff, set the log level for Neovim's LSP client to `debug`:

```lua
vim.lsp.set_log_level('debug')
```

<details>
<summary>With the <a href="https://github.com/stevearc/conform.nvim"><code>conform.nvim</code></a> plugin for Neovim.</summary>

```lua
require("conform").setup({
    formatters_by_ft = {
        python = {
          -- To fix auto-fixable lint errors.
          "ruff_fix",
          -- To run the Ruff formatter.
          "ruff_format",
          -- To organize the imports.
          "ruff_organize_imports",
        },
    },
})
```

</details>

<details>
<summary>With the <a href="https://github.com/mfussenegger/nvim-lint"><code>nvim-lint</code></a> plugin for Neovim.</summary>

```lua
require("lint").linters_by_ft = {
  python = { "ruff" },
}
```

</details>

<details>
<summary>With the <a href="https://github.com/dense-analysis/ale">ALE</a> plugin for Neovim or Vim.</summary>

<i>Neovim (using Lua):</i>

```lua
-- Linters
vim.g.ale_linters = { python = { "ruff" } }
-- Fixers
vim.g.ale_fixers = { python = { "ruff", "ruff_format" } }
```

<i>Vim (using Vimscript):</i>

```vim
" Linters
let g:ale_linters = { "python": ["ruff"] }
" Fixers
let g:ale_fixers = { "python": ["ruff", "ruff_format"] }
```

For the fixers, <code>ruff</code> will run <code>ruff check --fix</code> (to fix all auto-fixable
problems) whereas <code>ruff_format</code> will run <code>ruff format</code>.

</details>

## Vim

The [`vim-lsp`](https://github.com/prabirshrestha/vim-lsp) plugin can be used to configure the Ruff Language Server in Vim.
To set it up, install [`vim-lsp`](https://github.com/prabirshrestha/vim-lsp) plugin and register the server using the following
in your `.vimrc`:

```vim
if executable('ruff')
    au User lsp_setup call lsp#register_server({
        \ 'name': 'ruff',
        \ 'cmd': {server_info->['ruff', 'server']},
        \ 'allowlist': ['python'],
        \ 'workspace_config': {},
        \ })
endif
```

See the `vim-lsp`
[documentation](https://github.com/prabirshrestha/vim-lsp/blob/master/doc/vim-lsp.txt) for more
details on how to configure the language server.

If you're using Ruff alongside another LSP (like Pyright), you may want to defer to that LSP for certain capabilities,
like [`textDocument/hover`](./features.md#hover) by adding the following to the function `s:on_lsp_buffer_enabled()`:

```vim
function! s:on_lsp_buffer_enabled() abort
    " add your keybindings here (see https://github.com/prabirshrestha/vim-lsp?tab=readme-ov-file#registering-servers)

    let l:capabilities = lsp#get_server_capabilities('ruff')
    if !empty(l:capabilities)
      let l:capabilities.hoverProvider = v:false
    endif
endfunction
```

Ruff is also available as part of the [coc-pyright](https://github.com/fannheyward/coc-pyright)
extension for [coc.nvim](https://github.com/neoclide/coc.nvim).

<details>
<summary>Ruff can also be integrated via <a href="https://github.com/mattn/efm-langserver">efm language server</a> in just a few lines.</summary>

Following is an example config for efm to use Ruff for linting and formatting Python files:

```yaml
tools:
  python-ruff:
    lint-command: "ruff check --stdin-filename ${INPUT} --output-format concise --quiet -"
    lint-stdin: true
    lint-formats:
      - "%f:%l:%c: %m"
    format-command: "ruff format --stdin-filename ${INPUT} --quiet -"
    format-stdin: true
```

</details>

## Helix

Open the [language configuration file](https://docs.helix-editor.com/languages.html#languagestoml-files) for
Helix and add the language server as follows:

```toml
[language-server.ruff]
command = "ruff"
args = ["server"]
```

Then, you'll register the language server as the one to use with Python. If you don't already have a
language server registered to use with Python, add this to `languages.toml`:

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

!!! note

    Support for multiple language servers for a language is only available in Helix version
    [`23.10`](https://github.com/helix-editor/helix/blob/master/CHANGELOG.md#2310-2023-10-24) and later.

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

By default, the log level for Ruff is set to `info`. To change the log level, you can set the
[`logLevel`](./settings.md#loglevel) setting:

```toml
[language-server.ruff]
command = "ruff"
args = ["server"]

[language-server.ruff.config.settings]
logLevel = "debug"
```

You can also divert Ruff's logs to a separate file with the [`logFile`](./settings.md#logfile) setting.

To view the trace logs between Helix and Ruff, pass in the `-v` (verbose) flag when starting Helix:

```sh
hx -v path/to/file.py
```

## Kate

1. Activate the [LSP Client plugin](https://docs.kde.org/stable5/en/kate/kate/plugins.html#kate-application-plugins).
1. Setup LSP Client [as desired](https://docs.kde.org/stable5/en/kate/kate/kate-application-plugin-lspclient.html).
1. Finally, add this to `Settings` -> `Configure Kate` -> `LSP Client` -> `User Server Settings`:

```json
{
  "servers": {
    "python": {
      "command": ["ruff", "server"],
      "url": "https://github.com/astral-sh/ruff",
      "highlightingModeRegex": "^Python$",
      "settings": {}
    }
  }
}
```

See [LSP Client documentation](https://docs.kde.org/stable5/en/kate/kate/kate-application-plugin-lspclient.html) for more details
on how to configure the server from there.

!!! important

    Kate's LSP Client plugin does not support multiple servers for the same language. As a
    workaround, you can use the [`python-lsp-server`](https://github.com/python-lsp/python-lsp-server)
    along with the [`python-lsp-ruff`](https://github.com/python-lsp/python-lsp-ruff) plugin to
    use Ruff alongside another language server. Note that this setup won't use the [server settings](settings.md)
    because the [`python-lsp-ruff`](https://github.com/python-lsp/python-lsp-ruff) plugin uses the
    `ruff` executable and not the language server.

## Sublime Text

To use Ruff with Sublime Text, install Sublime Text's [LSP](https://github.com/sublimelsp/LSP)
and [LSP-ruff](https://github.com/sublimelsp/LSP-ruff) package.

## PyCharm

Starting with version 2025.3, PyCharm supports Ruff out of the box:

1. Go to **Python | Tools | Ruff** in the Settings dialog.

1. Select the **Enable** checkbox.

1. In the Execution mode setting, select how PyCharm should search for the executable:

    **Interpreter** mode: PyCharm searches for an executable installed in your interpreter. To install the Ruff package for the selected interpreter, click _Install Ruff_.

    **Path** mode: PyCharm searches for an executable in `$PATH`. If the executable is not found, you can specify the path by clicking the Browse... icon.

1. Select which options should be enabled.

For more information, refer to [PyCharm documentation](https://www.jetbrains.com/help/pycharm/2025.3/lsp-tools.html#ruff).

### Via External Tool

Ruff can be installed as an [External Tool](https://www.jetbrains.com/help/pycharm/configuring-third-party-tools.html)
in PyCharm. Open the Preferences pane, then navigate to "Tools", then "External Tools". From there,
add a new tool with the following configuration:

![Install Ruff as an External Tool](https://github.com/user-attachments/assets/2b7af3e4-8196-4c64-a721-5bc3d7564a72)

Ruff should then appear as a runnable action:

![Ruff as a runnable action](https://user-images.githubusercontent.com/1309177/193156026-732b0aaf-3dd9-4549-9b4d-2de6d2168a33.png)

### Via third-party plugin

Ruff is also available as the [Ruff](https://plugins.jetbrains.com/plugin/20574-ruff) plugin on the
IntelliJ Marketplace (maintained by [@koxudaxi](https://github.com/koxudaxi)).

## Emacs

Ruff can be utilized as a language server via [`Eglot`](https://github.com/joaotavora/eglot), which is in Emacs's core.
To enable Ruff with automatic formatting on save, use the following configuration:

```elisp
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '(python-base-mode . ("ruff" "server"))))
(add-hook 'python-base-mode-hook
          (lambda ()
            (eglot-ensure)
            (add-hook 'after-save-hook 'eglot-format nil t)))
```

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
;; Replace default (black) to use ruff for sorting import and formatting.
(setf (alist-get 'python-mode apheleia-mode-alist)
      '(ruff-isort ruff))
(setf (alist-get 'python-ts-mode apheleia-mode-alist)
      '(ruff-isort ruff))
```

## TextMate

Ruff is also available via the [`textmate2-ruff-linter`](https://github.com/vigo/textmate2-ruff-linter)
bundle for TextMate.

## Zed

Ruff support is now built into Zed (no separate extension required).

By default, Zed uses Ruff for formatting and linting.

To set up editor-wide Ruff options, provide the [server settings](settings.md)
under the [`lsp.ruff.initialization_options.settings`](https://zed.dev/docs/configuring-zed#lsp) key of your `settings.json` file:

```json
{
  "lsp": {
    "ruff": {
      "initialization_options": {
        "settings": {
          // Ruff server settings go here
          "lineLength": 80,
          "lint": {
            "extendSelect": ["I"],
          }
        }
      }
    }
  }
}
```

[`format_on_save`](https://zed.dev/docs/configuring-zed#format-on-save) is enabled by default.
You can disable it for Python by changing `format_on_save` in your `settings.json` file:

```json
{
  "languages": {
    "Python": {
      "format_on_save": "off"
    }
  }
}
```

You can configure Ruff to fix lint violations and/or organize imports on-save by enabling the
`source.fixAll.ruff` and `source.organizeImports.ruff` code actions respectively:

```json
{
  "languages": {
    "Python": {
      "code_actions_on_format": {
        // Organize imports
        "source.organizeImports.ruff": true,
        // Fix all auto-fixable lint violations
        "source.fixAll.ruff": true
      }
    }
  }
}
```
