# omv

A modal text editor in Rust. Vim-inspired, but with its own rules — modes,
motions and text objects you already know, without the legacy corners, and with
every key binding declared in one YAML file.

```
cargo run -p omv -- src/main.rs
```

## What works today

| | |
|---|---|
| **Modal editing** | normal / insert / visual / visual-line / command, counts (`3dd`, `13G`) |
| **Motions** | `hjkl w b e 0 ^ $ gg G % <C-d> <C-u>` |
| **Text objects** | `diw daw ciw di( ci" di{ da[ yiw` … |
| **Editing** | `dd D dw cc C cw yy p P J >> << x X u <C-r>` |
| **File explorer** | docked left, lazy expansion, reveals the current file |
| **Fuzzy finder** | files, buffers, and project-wide grep — all in-process |
| **Search** | `/pattern`, `n`, `N` |
| **LSP** | diagnostics, hover, go-to-definition, references, formatting |
| **Syntax** | tree-sitter (Rust, JSON) |
| **Ex commands** | `:w :q :q! :wq :e <path> :bn :bp :42` |

## Configuration

Config lives at `~/.config/omv/config.yaml`. It is **merged over** the built-in
defaults per key, so you only write what you want to change — rebinding `j`
doesn't cost you every other binding. Map an action to `nop` to delete a default.

```yaml
leader: "<Space>"
indent_width: 4
line_numbers: relative     # absolute | relative | none

keys:
  normal:
    "<leader>ff": find_files
    "<leader>fg": find_text
    "jj": nop              # remove a default you don't want
    "gh": move_line_start   # invent your own
```

Key notation is vim's: bare characters are literal (`dd` is two presses), angle
brackets name special keys — `<C-p>`, `<A-x>`, `<Esc>`, `<CR>`, `<Space>`,
`<Tab>`, `<F5>`, `<leader>`. A lone `<` is literal, so `<<` and `<` bind fine.

Two things make the config hard to get wrong:

- **Actions are a Rust enum, not strings.** A typo like `move_lft` fails at load
  with a line number instead of silently doing nothing at 2am.
- `omv --list-actions` prints every bindable action; `omv --list-keys` prints the
  keymap you actually ended up with after the merge.

## Architecture

Six crates, so the editing core stays testable without a terminal, a runtime, or
a language server anywhere near it.

```
                    ┌───────────────────────────────────────┐
 input thread ──┐   │ omv (bin)                             │
 lsp (tokio)  ──┼──►│  crossterm ─ ratatui ─ panels ─ theme  │
 find threads ──┘   └───────────────┬───────────────────────┘
      mpsc<AppEvent>                │ Action            ▲ Effect
                    ┌───────────────▼───────────────────┴───┐
                    │ omv-core   rope · modes · motions ·   │
                    │            text objects · undo        │
                    └───────────────────────────────────────┘
   omv-config (YAML → keymap trie)   omv-syntax (tree-sitter)
   omv-find   (nucleo · ignore · grep)   omv-lsp (JSON-RPC over tokio)
```

| Crate | Responsibility |
|---|---|
| `omv-core` | Rope buffer, cursor, modes, motions, text objects, undo, the `Action` enum |
| `omv-config` | Key notation, keymap trie, config merge |
| `omv-syntax` | tree-sitter parsing → flat highlight spans |
| `omv-find` | File walk, fuzzy matching, grep |
| `omv-lsp` | LSP client; tokio inside, plain channels outside |
| `omv` | Terminal UI, event loop, panels |

Three ideas hold it together:

1. **The core returns `Effect`s, never side effects.** `dispatch` says it *wants*
   a picker open or a hover fetched; the UI decides how. That is what lets the
   whole editing model be unit-tested with no terminal.
2. **One channel, many producers.** Input, LSP, and search all feed a single
   `mpsc<AppEvent>` that the render loop blocks on — no polling, no wasted frames.
   Queued events are drained before redrawing, so a paste costs one frame.
3. **Async is quarantined in `omv-lsp`.** Each server owns a thread with a
   current-thread tokio runtime and talks over `std::sync::mpsc`.

## Testing

```
cargo test --workspace     # 38 tests
```

`omv-lsp` ships a mock language server (`omv-mock-lsp`) so the client's framing,
request/response correlation, and event mapping are covered without any language
server installed. The UI is tested headlessly through ratatui's `TestBackend`.

## Known limits

- **Prefix bindings need a following key.** If `d` and `dd` are both bound, `d`
  only fires once a non-matching key arrives — there is no `timeoutlen` yet.
  The default keymap avoids such pairs.
- **No splits.** One editor view with docked panels; the window tree is future work.
- **Full-document sync.** Both tree-sitter and LSP re-read the whole buffer on
  every change. Fine to a few thousand lines; incremental is the next step.
- **No operator-pending grammar.** `d` + motion doesn't compose — `dw`, `diw` and
  friends are discrete bindings. Deliberate, per the "my own rules" design, but it
  means new text objects need new actions.
- **Rename has no prompt yet**, and `tab` is expanded at render time only.
- Linux/WSL only so far.

## License

MIT
