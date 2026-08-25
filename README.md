# omv

A modal text editor in Rust. Vim-inspired, but with its own rules — modes,
motions and text objects you already know, without the legacy corners, and with
every key binding declared in one YAML file.

```
cargo run -p omv -- src/main.rs
```

## Install

Grab a binary from the [latest release](https://github.com/mbenencase/oh-my-vim/releases/latest)
— `gnu` for any mainstream distro, `musl` if its glibc is old or absent:

```
tar xzf omv-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz
sudo install omv-vX.Y.Z-x86_64-unknown-linux-gnu/omv /usr/local/bin/omv
```

Or build it yourself with `make install` (release build + copy to `/usr/local/bin`).

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
| **Find & replace** | `<C-f>` prompt, `<C-s>` for the replacement, one match or the whole file |
| **LSP** | diagnostics, hover, go-to-definition, references, formatting |
| **Syntax** | tree-sitter (Rust, JSON) |
| **Ex commands** | `:w :q :q! :wq :e <path> :bn :bp :42 :keys` |
| **Key reference** | `:keys` (or `<leader>?`) lists every binding and what it does |

## Find and replace

`<C-f>` opens a prompt in the top-right corner. Type what you are looking for:
every hit is highlighted as you type, the one you are on is highlighted brighter,
and the title counts them (`3/12`). `<C-n>` and `<C-p>` walk between them.

Press `<C-s>` and a second field appears for the replacement. From there:

| | |
|---|---|
| `<CR>` | replace this match and move to the next |
| `<C-a>` | replace **every** match in the document, as one undo step |
| `<Tab>` | switch between the two fields |
| `<C-n>` / `<C-p>` | next / previous match |
| `<Esc>` | close |

Until you press `<C-s>` there is nothing to replace *with*, so `<CR>` just walks
the matches — a stray Enter can never delete the word you were only looking for.
Closing with `<Esc>` before you have replaced anything puts the cursor back where
it started; once you have made an edit it leaves you on it. The pattern carries
over to `n`/`N`, and an in-flight `/pattern` search seeds the prompt when it opens.

Matching is literal, not regex — a replacement that reinterprets `.` or `(` is
too easy a way to lose a file. Project-wide regex search lives in `<leader>fg`,
which only ever reads.

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

### Seeing your bindings

`:keys` (aliases `:map`, `:!keys`; bound to `<leader>?`) opens a reference of
every binding, grouped by mode and category, with a description of each action:

```
NORMAL MODE
  Motion
  h          move_left                  Cursor one character left
  <Left>     move_left                  Cursor one character left
  w          move_word_forward          Start of next word
  Editing
  dd         delete_line                Delete the whole line into the register
```

It is built from the *resolved* keymap, so it shows your overrides and omits
anything you removed with `nop` — it can't drift from the real bindings.
`j`/`k` scroll, `<C-d>`/`<C-u>` page, `g`/`G` jump to the ends, `Esc` closes.

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
                    │    text objects · substitute · undo   │
                    └───────────────────────────────────────┘
   omv-config (YAML → keymap trie)   omv-syntax (tree-sitter)
   omv-find   (nucleo · ignore · grep)   omv-lsp (JSON-RPC over tokio)
```

| Crate | Responsibility |
|---|---|
| `omv-core` | Rope buffer, cursor, modes, motions, text objects, find & replace, undo, the `Action` enum |
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
cargo test --workspace     # 61 tests
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
