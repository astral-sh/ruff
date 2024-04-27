## Neovim Setup Guide for `ruff server`

### Using `nvim-lspconfig`

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

#### Tips

If you're using Ruff alongside another LSP (like Pyright), you may want to defer to that LSP for certain capabilities, like `textDocument/hover`:

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

If you'd like to use Ruff exclusively for linting, formatting, and import organization, you can disable those capabilities for Pyright:

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
