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

The grammar can be installed by `nvim-treesitter` from a local checkout. Link
the grammar into the Neovim config first:

```bash
mkdir -p ~/.config/nvim/tree-sitter
ln -sfn /path/to/purple-garden/tree-sitter ~/.config/nvim/tree-sitter/garden
```

Register the parser before calling `require('nvim-treesitter').install(...)`:

```lua
local function register_garden()
    require('nvim-treesitter.parsers').garden = {
        install_info = {
            path = vim.fn.stdpath('config') .. '/tree-sitter/garden',
            queries = 'queries/garden',
            generate = true,
        },
        -- tier 4 is treated as unsupported by nvim-treesitter's installer.
        tier = 3,
    }
end

register_garden()

vim.api.nvim_create_autocmd('User', {
    pattern = 'TSUpdate',
    callback = register_garden,
})

vim.filetype.add({
    extension = {
        garden = 'garden',
    },
})
```

Then include `garden` in the parser install list or install it manually:

```vim
:lua require('nvim-treesitter').install('garden', { force = true, generate = true }):wait(120000)
```

For a current Neovim LSP config, register the language server against the
`garden` filetype:

```lua
vim.lsp.config('purple-garden', {
    cmd = { '/path/to/purple-garden/target/debug/purple-garden', 'lsp' },
    filetypes = { 'garden' },
})
vim.lsp.enable('purple-garden')
```

Build the binary first if the path points at the local debug target:

```bash
cargo build -p purple-garden-cli
```
