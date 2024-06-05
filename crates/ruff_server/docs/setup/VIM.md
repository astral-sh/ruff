## Vim Setup Guide for `ruff server`

### Using `vim-lsp`

1. Install [`vim-lsp`](https://github.com/prabirshrestha/vim-lsp).
1. Setup `vim-lsp` [as desired](https://github.com/prabirshrestha/vim-lsp?tab=readme-ov-file#registering-servers).
1. Finally, add this to your `.vimrc`:

```vim
if executable('ruff')
    au User lsp_setup call lsp#register_server({
        \ 'name': 'ruff',
        \ 'cmd': {server_info->['ruff', 'server', '--preview']},
        \ 'allowlist': ['python'],
        \ 'workspace_config': {},
        \ })
endif
```

See the `vim-lsp` [documentation](https://github.com/prabirshrestha/vim-lsp/blob/master/doc/vim-lsp.txt) for more
details on how to configure the language server.

> \[!IMPORTANT\]
>
> If Ruff's legacy language server (`ruff-lsp`) is configured in Vim, be sure to disable it to prevent any conflicts.

#### Tips

If you're using Ruff alongside another LSP (like Pyright), you may want to defer to that LSP for certain capabilities,
like `textDocument/hover` by adding the following to the function `s:on_lsp_buffer_enabled()`:

```vim
function! s:on_lsp_buffer_enabled() abort
    " add your keybindings here (see https://github.com/prabirshrestha/vim-lsp?tab=readme-ov-file#registering-servers)

    let l:capabilities = lsp#get_server_capabilities('ruff')
    if !empty(l:capabilities)
      let l:capabilities.hoverProvider = v:false
    endif
endfunction
```
