# tree-sitter

Tree-sitter grammar for Purple Garden.

This repository keeps the grammar alongside the compiler so the language
syntax and the editor grammar stay in sync.

## Build

From this directory:

```bash
tree-sitter generate
```

That produces the parser sources under `src/`.

## Neovim

Link the local grammar into your Neovim config:

```bash
mkdir -p ~/.config/nvim/tree-sitter
ln -sfn /path/to/purple-garden/tree-sitter ~/.config/nvim/tree-sitter/garden
```

Register it before your `nvim-treesitter` setup/install call:

```lua
require('nvim-treesitter.parsers').garden = {
    install_info = {
        path = vim.fn.stdpath('config') .. '/tree-sitter/garden',
        queries = 'queries/garden',
        generate = true,
    },
    tier = 3,
}

vim.filetype.add({
    extension = { garden = 'garden' },
})
```

Install it:

```vim
:lua require('nvim-treesitter').install('garden', { force = true, generate = true }):wait(120000)
```

LSP setup:

```lua
vim.lsp.config('purple-garden', {
    cmd = { '/path/to/purple-garden/target/debug/purple-garden', 'lsp' },
    filetypes = { 'garden' },
})
vim.lsp.enable('purple-garden')
```

The language server currently supports:

- incremental document sync
- pull diagnostics
- hover docs for keywords, types, bindings, packages and functions
- completions for keywords, types, packages and package functions
- go to definition for local bindings, functions and imports
- code actions for diagnostics with suggested replacements

Build the binary first if you use the local debug target:

```bash
cargo build -p purple-garden-cli
```
