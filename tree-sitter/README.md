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

Point your Tree-sitter parser path at this directory or copy the generated
parser into your parser install path.
