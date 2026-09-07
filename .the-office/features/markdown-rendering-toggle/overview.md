---
spec_version: 1
id: markdown-rendering-toggle
type: feature
requirements:
  REQ-001: A user editing a buffer can toggle a formatted, read-only rendering of its Markdown content on and off, shown as a companion pane placed side-by-side with the raw text of the window it was toggled from — never replacing that window's own content — via a dedicated key binding and a matching `:` command.
  REQ-002: The render pane visually distinguishes headings (by level), bold emphasis, italic emphasis, list items (ordered and unordered), fenced code blocks, and horizontal rules from plain paragraph text and from each other, while the raw text pane continues to show the literal, unmodified Markdown source next to it.
  REQ-003: The render pane is display-only: it is never reachable via directional window focus (`<C-w>h/j/k/l`) and never becomes the focused window, so no key is ever routed to it for editing. Editing keys keep operating on the raw text pane exactly as they did before the render pane appeared.
  REQ-004: The render pane's presence never alters the raw pane's window state (buffer, cursor, scroll). Toggling it on, editing normally, and toggling it off leaves that window exactly as continuous normal editing would have — there is no separate "restore" step because nothing about the source window was ever touched.
  REQ-005: While the render pane is visible, it stays live: it reflects the source window's buffer content after every edit, and it follows the source window if that window's own buffer changes (e.g. `:bn`/`:bp`/`:e`). Its own scroll position updates to stay approximately aligned with the source window's scroll; exact line-for-line correspondence is not required, since raw and rendered line counts can differ.
  REQ-006: The toggle is available for the buffer in the focused window regardless of file extension; it is not gated on the file being named `.md`/`.markdown`.
  REQ-007: The new action is declared through the existing `actions!` macro (name, category, description) and bound identically in `crates/omv-config/assets/default.yaml` and `examples/config.yaml`, so `--list-actions`, `:keys`, and the two config files stay in sync.
  REQ-008: Real window-tree actions behave predictably around the render pane. `split_vertical`/`split_horizontal` on the source window keep the render pane attached to whichever half retains the source window's identity. `window_close` on the source window also removes its render pane. `window_only` keeps the render pane only if the surviving window is the source window, and removes it otherwise. None of these actions ever create, close, or focus a window for the render pane itself, because it is not a member of the window tree.
  REQ-009: The render pane's default placement is a vertical split with the render pane on the right of the raw text pane. This placement and orientation are fixed in v1 and are not configurable.
acceptance_criteria:
  AC-001:
    requirement: REQ-001
    description: With a buffer open and focused, pressing the bound key shows the render pane side-by-side with the still-visible raw text of the focused window; pressing it again (or running the `:` command either way) removes the render pane and the raw pane returns to its full width. The same is true driving the toggle purely through the `:` command.
  AC-002:
    requirement: REQ-002
    description: Given a buffer containing at least one heading, bold text, italic text, an unordered list, an ordered list, a fenced code block, and a horizontal rule, with the render toggled on, the rendered screen buffer shows both the raw source columns (left) and the styled render columns (right) simultaneously, with each Markdown construct styled distinguishably from plain text and from the other constructs in the render columns, while the raw columns still show the literal source text (verified by asserting on the rendered screen buffer, matching this repo's existing `TestBackend` UI test style).
  AC-003:
    requirement: REQ-003
    description: With the render pane visible, repeated directional-focus key presses (`<C-w>h`, `<C-w>j`, `<C-w>k`, `<C-w>l`) never change which real window is focused to something new that didn't exist before the render pane appeared, and the window count reported by the window tree is unchanged by toggling the render pane on or off. Pressing a key normally bound to `insert_mode` or a delete/change/paste action still edits the raw buffer exactly as it would with the render pane hidden.
  AC-004:
    requirement: REQ-004
    description: After toggling the render pane on, editing text, and toggling it off, the source window's buffer content, cursor position, and scroll offset are identical to what the same edits would have produced with the render pane never toggled on at all.
  AC-005:
    requirement: REQ-005
    description: While the render pane is visible, editing the buffer's text updates the render pane's content without needing to toggle it off and back on. Switching the source window to a different buffer (e.g. `:bn`) while the render pane is visible updates the render pane to show the newly focused buffer rather than the old one.
  AC-006:
    requirement: REQ-006
    description: The toggle key/command works on a buffer with no extension and on one with a non-`.md` extension (e.g. opened as a scratch buffer or `.txt`) exactly as it does on a `.md` buffer.
  AC-007:
    requirement: REQ-007
    description: `cargo run -p omv -- --list-actions` lists the new action with its category and description; `crates/omv-config/assets/default.yaml` and `examples/config.yaml` bind it identically; `./.github/scripts/arch-check.py` (in particular `F4 keymap-sync` and `F5 unreachable-action`) passes.
  AC-008:
    requirement: REQ-008
    description: With the render pane visible on the source window, running `split_vertical` leaves exactly one render pane, attached to whichever resulting window kept the source window's identity, and the window count increases by exactly one (the same as splitting with no render pane active). Running `window_close` on the source window removes the render pane along with it. Running `window_only` while focused on a window other than the source removes the render pane.
  AC-009:
    requirement: REQ-009
    description: The render pane always appears as a vertical (side-by-side) split with the render on the right — verified by the raw text's screen columns preceding the render's screen columns left-to-right in AC-002's screen assertion. No key binding, config key, or `:` command argument changes this placement in v1.
open_questions: []
---
# Toggleable in-terminal Markdown render

## Request
"implement a markdown rendering. The user must be able to toggle the render."
(verbatim, routed as a feature)

**Gate 1 redirect (human, recorded verbatim):** "the render must appear
side-by-side with the raw text, not replacing the focused window's content...
Render in a split next to the raw text rather than replacing the window's
content, so you can see source and render together." This spec reflects that
redirect; everything else confirmed at the first Gate 1 pass (read-only
render, no new `Mode`, key + `:` command, no extension gating, the v1
construct scope, `actions!` + both config files in sync) is unchanged.

## Objective
`omv` currently shows every buffer as raw text with `omv-syntax`
token-highlighting (Rust and JSON grammars only; there is no Markdown grammar
registered today — `crates/omv-syntax/src/lib.rs` lists `tree_sitter_rust`
and `tree_sitter_json` only). The objective is to let a user working on
Markdown prose see a formatted rendering — headings, emphasis, lists, code
blocks, rules — *alongside* the raw editable text they are actively working
in, so they can keep typing while watching the formatted result update, and
to hide that rendering on demand. This is a reading aid layered next to the
existing buffer view, not a replacement for it and not a second copy of the
buffer's data.

## Out of scope
- Rendering to an external target (browser, image, PDF, or any process
  spawned outside `omv`'s own terminal UI). Recommended interpretation:
  "render" means an in-terminal, ratatui-drawn formatted view, consistent
  with every other panel in this codebase (Explorer, Help, Diagnostics,
  Terminal) being drawn in-process rather than shelling out.
- Replacing the focused window's own content with the render (this was the
  interpretation in the pre-Gate-1 draft; the human redirected it to
  side-by-side, and this spec now specifies side-by-side throughout).
- A render pane implemented as a real member of the `Windows` split tree
  (see Assumptions for why) — no new `Action`/effect gives the render pane
  its own `WindowId`, and it is never a target of `window_left/right/up/down`,
  `window_close`, or `window_only` in isolation.
- Horizontal (top/bottom) orientation, or making the side/orientation
  configurable. REQ-009 fixes it to a vertical split, render on the right,
  for v1.
- More than one render pane visible at once, or a render pane that survives
  independent of the window it was toggled on. The toggle is a single,
  editor-wide flag, matching how `explorer.visible`/`diagnostics_visible`/the
  terminal panel are already modeled — see Assumptions.
- Independent scrolling or any interaction with the render pane itself
  (e.g. a separate cursor inside it). Since it never takes focus (REQ-003),
  the user reads it by scrolling/moving the cursor in the raw pane; the
  render pane's own scroll follows along approximately (REQ-005).
- GitHub-Flavored-Markdown extensions beyond REQ-002's list: no tables, task
  list checkboxes, footnotes, strikethrough, autolinks/clickable links,
  inline images, or embedded HTML. Blockquotes are also excluded from v1 —
  the request and this spec bound the construct list to headings,
  bold/italic, lists, code blocks, and horizontal rules; anything else is a
  follow-up.
- Syntax highlighting *inside* fenced code blocks in the render pane (i.e.
  colorizing the Rust/JSON/etc. inside a ```` ```rust ```` fence using
  `omv-syntax`). The code block only needs to be visually set off as a
  block; highlighting its contents is an enhancement, not required here.
- Precise mapping of raw-buffer cursor position to a position within the
  rendered layout. REQ-005 only requires the render pane's scroll to stay
  *approximately* aligned with the source window's scroll; exact line
  correspondence is explicitly not a tested contract, since collapsed markup
  (`**bold**` → `bold`) makes raw and rendered line counts differ.
- Auto-showing the render on file open, or remembering "this file/window was
  last viewed with its render pane open" across sessions. Every open starts
  with no render pane; the toggle is a per-session, per-toggle action only.

## Constraints
- **Core purity (CLAUDE.md rule 1).** `omv-core` returns `Effect`s only.
  Whatever component turns Markdown text into structured/styled content must
  be pure (text in, structure out) and must not add a new dependency to
  `omv-core`'s Cargo.toml — `omv-core` depends on nothing in-workspace today,
  and `F1 dep-direction` in `arch-check.py` enumerates the allowed edges
  literally, so a new one is a deliberate, visible change, not an accident.
- **No new async/tokio.** `F2 async-quarantine` restricts tokio to `omv-lsp`;
  Markdown parsing/rendering is synchronous, in-process work and must stay
  so.
- **`Action`/keymap-sync discipline (CLAUDE.md "Adding a key binding").** The
  new action goes through the `actions!` macro in
  `crates/omv-core/src/action.rs`, is handled in `Editor::dispatch`, produces
  an `Effect` handled in `App::apply_effects` (`crates/omv/src/app.rs`), and
  is bound identically in `crates/omv-config/assets/default.yaml` and
  `examples/config.yaml` (`F4 keymap-sync`) and reachable by a key or
  `:command` (`F5 unreachable-action`).
- **Windows are views, not buffers, and the editor owns exactly one live
  cursor (CLAUDE.md "Things that bite").** `crates/omv/src/window.rs`'s
  `Window` carries a buffer index, scroll, and cursor per split, and
  `App::sync_focused_window`/`App::focus_window`
  (`crates/omv/src/app.rs`) move the editor's single live cursor in and out
  of whichever `Window` has focus on every effect batch and every focus
  change. A render pane that were a real `Window` would need its own
  buffer/cursor identity and would risk receiving the live cursor the moment
  it was focused (accidentally, via `<C-w>` directional focus, or by
  `window_only`) — in direct conflict with REQ-003's read-only, unfocusable
  requirement. The render pane must therefore stay outside the `Windows`
  tree entirely; see Assumptions for the resulting shape.
- **Full-document sync precedent.** Both tree-sitter highlighting and LSP
  already re-read the whole buffer on every `Effect::BufferChanged` rather
  than incrementally; REQ-005's live refresh follows the same
  "recompute from the whole buffer" approach at this scale, at the same cost
  class CLAUDE.md already accepts for syntax/LSP ("Fine at current scale;
  incremental is future work").

## Assumptions
- **"Render" means in-terminal formatted preview, not an external
  browser/document.** Basis: every existing "show me something extra"
  feature (Explorer, Help, Diagnostics, Terminal, pickers) is drawn by `omv`
  itself in the same terminal session; nothing in the codebase shells out to
  a viewer, and `omv-core`'s core-purity rule discourages introducing that
  pattern for one feature.
- **The render pane is a renderer-level companion, not a real `Window` in
  the split tree.** Basis: per the Constraints entry above, a real `Window`
  would need a buffer/cursor identity and could be reached by focus
  navigation, both of which conflict with the render pane being strictly
  read-only and unfocusable (REQ-003). Instead, it is drawn by subdividing
  the source window's own on-screen rect at draw time (the same place
  `app.windows.layout(text_area)` and `render_text` already assign and draw
  real windows' rects in `crates/omv/src/ui.rs`), without adding a node to
  `Windows`'/`window.rs`'s tree. This is why REQ-008's window-tree actions
  (`split_vertical`, `window_close`, `window_only`, directional focus) can
  all be described purely in terms of "what happens to the source window,"
  with the render pane following along rather than being separately
  addressable.
- **The toggle is a single, editor-wide flag that remembers which window it
  is attached to** — modeled the same way `explorer.visible`/
  `diagnostics_visible`/the terminal panel's visibility already are (single
  booleans on `App`), plus the source `WindowId` so the draw step knows
  which window's rect to subdivide. Turning the toggle on attaches it to the
  currently focused window; turning it off clears it, regardless of which
  window is focused at that moment. Only one render pane can be visible at a
  time in v1 — basis: every existing panel in this codebase is a singleton,
  and multi-window multi-render-pane support was not asked for.
- **Default placement is a vertical split (`Axis::Columns`), render on the
  right, not configurable in v1.** Basis: this is the explicit recommendation
  the human confirmed at Gate 1 redirect ("obvious guess"); existing splits
  (`:vsp`) already default new content to the right/below of the window they
  split from, so this matches the reading direction of the rest of the UI.
- **Focus stays in the raw text pane when the render pane appears; nothing
  about `Focus` (`crates/omv/src/app.rs`) changes.** Basis: the human's
  explicit redirect ("focus STAYS in the raw text pane so the user keeps
  editing, with the render updating alongside"). Unlike Explorer/Help/
  Diagnostics/Substitute, this feature adds no new `Focus` variant at all —
  the render pane is pure output with no key routing, which is a stronger
  form of "panels are not modes" than those panels use (they still capture
  keys at the UI layer; this captures none, because it is never focused).
- **The render pane refreshes live, on every relevant `Effect::BufferChanged`
  for the buffer it is mirroring, rather than only when toggled.** Basis:
  the human's explicit redirect — side-by-side implies both panes are meant
  to be looked at together, so a stale render next to live-edited text would
  read as broken. Cost: this recomputes Markdown structure from the whole
  buffer on every text-changing keystroke while the pane is visible, the
  same cost class CLAUDE.md already accepts as the norm for tree-sitter and
  LSP full-document re-reads, and it only applies while a render pane is
  actually visible (idle otherwise).
- **Toggle key is `<leader>m` in Normal mode, with a `:command` alias.**
  Basis: existing panel toggles are `<leader>`-prefixed with a mnemonic
  letter (`<leader>e` explorer, `<leader>d` diagnostics, `<leader>?`
  show_keys); `m` for "markdown" is free in both
  `crates/omv-config/assets/default.yaml` and `examples/config.yaml` today.
  A paired `:command` follows the `toggle_terminal` precedent (bound to
  `<C-j>` *and* reachable via `:term`/`:terminal` in
  `Editor::execute_command_line`), so this feature adds both rather than
  only a key. Exact command spelling (e.g. `:md`) is left to the Planner;
  the requirement is that one exists and is documented.
- **The toggle is not gated by file extension.** Basis: every other panel
  toggle (Explorer, Terminal, Diagnostics) is available unconditionally on
  whatever buffer is focused; gating a manually-invoked toggle on a file
  extension would make it silently do nothing for an extensionless or
  non-`.md` buffer containing Markdown (e.g. a commit message scratch
  buffer), which is more surprising than simply always rendering whatever
  text is there.
- **Markdown parsing/structuring is a small, pure, side-effect-free
  component** (block-level: headings, list items, fenced code, rules; plus
  inline bold/italic spans) that respects the dependency graph
  `arch-check.py` already enforces. Whether it lives inside `omv-syntax`
  (which already exists to turn buffer text into structured display data,
  today via tree-sitter for Rust/JSON) or as a small hand-rolled parser is
  an implementation decision for the Planner — this spec only constrains it
  to stay pure and to not require a new `omv-core` dependency or new async
  runtime, per the Constraints above.

## Harness impact
The existing sensors already cover this feature's structural risk: `F1
dep-direction` and `F2 async-quarantine` will fail the build if the Markdown
parser is placed somewhere that breaks core purity or pulls in async; `F4
keymap-sync` and `F5 unreachable-action` will fail if the two keymap YAML
files drift or the action is unreachable. No new sensor is required.
`crates/omv/src/window.rs`'s existing test suite (split/close/only/
directional-focus behaviour) is the right place to add coverage confirming
the render pane's presence doesn't change window-tree behaviour (REQ-008),
since that suite already tests those operations directly against `Windows`
with no rendering involved. The 83-test floor in `.the-office/harness.md` /
CI rises by however many tests this feature adds (existing convention:
`cargo test --workspace` count is a ratchet, raise it in the same commit as
new tests).

## User flows
1. User opens a buffer containing Markdown text (any extension) and focuses
   its window. They press the toggle key (or run the toggle `:command`). A
   render pane appears to the right of that window's raw text, in the same
   window's screen area, showing headings, bold, italic, lists, fenced code
   blocks, and horizontal rules each styled distinctly; the raw text pane on
   the left is unchanged and still has focus.
2. User keeps typing and navigating in the raw text pane exactly as before.
   As they edit, the render pane updates to reflect the new content without
   any extra action.
3. User presses the toggle key again (or reruns the `:command`). The render
   pane disappears and the (never-modified) raw pane returns to its full
   window width, with the cursor and scroll exactly where ongoing editing
   left them.
4. User splits the source window (`:vsp`/`:hsp`) while its render pane is
   visible: the render pane stays attached to whichever half kept the
   original window's identity; the new sibling window is a plain raw-text
   window with no render pane of its own.
5. User closes the source window (`<C-w>c`) while its render pane is
   visible: the render pane closes with it, since it was never an
   independently addressable window.

## Business rules
Not applicable — this is an editor display feature with no domain/business
logic; the "rules" are the structural/architectural constraints captured
above under Constraints.

## Data and integrations
None. No new external dependency contract, no network, no filesystem beyond
the buffer's existing load/save path. Any new library dependency (e.g. for
Markdown parsing) is an implementation choice for the Planner, constrained to
respect `F1`/`F2` as stated above.

## Failure and empty states
- **Empty buffer.** Toggling the render pane on an empty buffer shows an
  empty render pane next to the empty raw pane (no headings/lists/etc. to
  draw); toggling off returns to the single full-width empty raw pane. Not
  an error.
- **Buffer with no Markdown-like structure** (plain prose, or a non-Markdown
  file toggled per REQ-006). The render pane shows the text as plain
  paragraph content with no special styling applied, since no headings/
  emphasis/lists/code fences/rules are present to detect. Not an error.
- **Terminal too narrow to usefully show two panes.** The render pane still
  appears per REQ-009 (fixed vertical split, not adaptive in v1); a pane
  rendered too narrow to be readable is a known limitation of a fixed-layout
  split, the same limitation an existing `:vsp` on a narrow terminal already
  has, and is not a new failure mode this feature must solve.
- **Malformed/unterminated constructs** (e.g. an unclosed fenced code block,
  a stray `**` with no matching close). The render should degrade
  gracefully — treat the rest of the buffer as inside/outside the construct
  in the most visually stable way rather than panicking or discarding
  content — but the exact degradation rule is an implementation detail left
  to the Planner, not a tested contract beyond "never panics, never
  loses/mutates buffer text."
