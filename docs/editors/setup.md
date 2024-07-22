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

```lua
require('lspconfig').ruff.setup({
  init_options = {
    settings = {
      -- Ruff language server settings go here
    }
  }
})
```

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

By default, Ruff will not show any logs. To enable logging in Neovim, you'll need to set the
`RUFF_TRACE` environment variable to either `messages` or `verbose`, and use the
[`logLevel`](./settings.md#loglevel) setting to change the log level:

```lua
require('lspconfig').ruff.setup {
  cmd_env = { RUFF_TRACE = "messages" }
  init_options = {
    settings = {
      logLevel = "debug",
    }
  }
}
```

By default, this will write logs to stderr which will be available in Neovim's LSP client log file
(`:lua vim.print(vim.lsp.get_log_path())`). It's also possible to divert these logs to a separate
file with the [`logFile`](./settings.md#logfile) setting.

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
<summary>With the <a href="https://github.com/dense-analysis/ale">ALE</a> plugin for Vim or Neovim.</summary>

```vim
" Linter
let g:ale_linters = { "python": ["ruff"] }
" Formatter
let g:ale_fixers = { "python": ["ruff-format"] }
```

</details>

<details>
<summary>Ruff can also be integrated via <a href="https://github.com/mattn/efm-langserver">efm language server</a> in just a few lines.</summary>
<br>

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

<details>
<summary>With the <a href="https://github.com/stevearc/conform.nvim"><code>conform.nvim</code></a> plugin for Neovim.</summary>
<br>

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

By default, Ruff does not log anything to Helix. To enable logging, set the `RUFF_TRACE` environment
variable to either `messages` or `verbose`, and use the [`logLevel`](./settings.md#loglevel) setting to change
the log level:

```toml
[language-server.ruff]
command = "ruff"
args = ["server"]
environment = { "RUFF_TRACE" = "messages" }

[language-server.ruff.config.settings]
logLevel = "debug"
```

You can also divert Ruff's logs to a separate file with the [`logFile`](./settings.md#logfile) setting.

!!! note
    Setting `RUFF_TRACE=verbose` does not enable Helix's verbose mode by itself. You'll need to run
    Helix with `-v` for verbose logging.

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
    Kate's LSP Client plugin does not support multiple servers for the same language.

## Sublime Text

To use Ruff with Sublime Text, install Sublime Text's [LSP](https://github.com/sublimelsp/LSP)
and [LSP-ruff](https://github.com/sublimelsp/LSP-ruff) package.

## PyCharm

### Via External Tool

Ruff can be installed as an [External Tool](https://www.jetbrains.com/help/pycharm/configuring-third-party-tools.html)
in PyCharm. Open the Preferences pane, then navigate to "Tools", then "External Tools". From there,
add a new tool with the following configuration:

![Install Ruff as an External Tool](https://user-images.githubusercontent.com/1309177/193155720-336e43f0-1a8d-46b4-bc12-e60f9ae01f7e.png)

Ruff should then appear as a runnable action:

![Ruff as a runnable action](https://user-images.githubusercontent.com/1309177/193156026-732b0aaf-3dd9-4549-9b4d-2de6d2168a33.png)

### Via third-party plugin

Ruff is also available as the [Ruff](https://plugins.jetbrains.com/plugin/20574-ruff) plugin on the
IntelliJ Marketplace (maintained by [@koxudaxi](https://github.com/koxudaxi)).

## Emacs

Ruff can be utilized as a language server via [`Eglot`](https://github.com/joaotavora/eglot), which is in Emacs's core.
To enable Ruff with automatic formatting on save, use the following configuration:

```elisp
(add-hook 'python-mode-hook 'eglot-ensure)
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '(python-mode . ("ruff" "server")))
  (add-hook 'after-save-hook 'eglot-format))
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
