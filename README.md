# Rift

This is a vim-like text editor. 

I use this daily and primarily. 

## Implemented:
- Quite a lot more than the below items, I just haven't updated this in a while due to actually working on it.
- Vim-style modal editing: normal/insert/visual/command modes, operators (d/c/y), dot repeat, granular motions
- Multi-select visual selection with a selection buffer and forward-search selection
- Configurable GhostCut-style delete
- Rope + piece-table text buffer with hybrid delta/checkpoint undo and a fancy undotree
- Custom regex engine (monster-regex) powering search and replace
- Async tree-sitter syntax highlighting across 19+ languages, with time-budgeted incremental parsing
- LSP client: diagnostics, go-to-definition, find references, hover, rename, formatting, code actions
- Annotation framework for inline document metadata (used by LSP diagnostics, extensible via plugins)
- Lua plugin system with a rich API (annotations, buffer metadata, navigation, event hooks)
- Remote daemon mode: run headless, connect over SSH with token auth
- Vertical/horizontal splits with resize, equalize, freeze, and directional pane moving
- Integrated terminal emulator (`:terminal`)
- Ranger-style file explorer (open, rename, delete, copy, bulk ops, hidden-file toggle)
- Clipboard ring buffer with system clipboard integration
- Fast search (sub-15ms on a 1MB buffer) with smartcase and background index warming
- Full Unicode support: multi-width rendering, correct decoding, BOM stripping
- Command-line completion for commands, settings, and file paths
- Command and search history with navigation
- Everything-is-a-buffer architecture (explorer, undotree, terminal all buffers)
- Async, non-blocking I/O via a threaded job manager

Todo:
- registers + unified yank/paste/delete
- macros (q{reg} / @{reg})
- jump list (Ctrl+O / Ctrl+I)
- marks and jumps (m{a-z}, '{mark})
- change list (g; / g,)
- help manual
- .riftrc config file
- code folding (zf/zo/zc)
- operator pending improvements (indent, format, case, etc.)
- animations

Known issues:
- 4 byte unicode characters are not able to be inserted on windows (this is a crossterm issue on windows, idk man)

## Fixed stuff (lightly tracked):
- TOCTOU Race Condition between file check and file open
- Gap Buffer uses a lot of `unsafe`. Write more debug asserts
- inefficient string construction wrap_text in render loop
- Ascii only operation
    - Renderer does not handle multibyte characters
    - cursor calculation does not account for multibyte characters
    - fix by using String instead of u8
    - Use unicode-width instead of assuming everything is a single char
- Allow multiline notifications
- Ensure all components use the theme system
- search doesn't close on successful search
- :e to reload a file clears highlights
- floating window components not matching theme bg/fg
- searching for non existent text is really slow
- undo preview does not autoload
- syntax highlighting not updated as typing occurs
- scroll fails on select_view beyond window
- commandline commands do not properly close commandline
- insert operations not grouped as undo transactions
- delete operations not grouped as undo transactions
- undotree initial position wrong
- terminal crashes when running interactive programs
- terminal open in split causes flicker
- notification padding wrong
- cursor does not move after typing a space in the terminal
- input boxes for file explorer not accepting input
- split rendering and navigation fixes
- clipboard tooltip not showing system clipboard on Wayland (missing `wayland-data-control` feature)
- search highlight offset wrong on non-ASCII characters
- operator pending mode blocking on certain key sequences
- file explorer entry duplication on recursive directory traversal
- terminal keybinding actions not bindable to other keys
- annotation line adornments (e.g. markdown horizontal rules) misrendering on the wrong line when multibyte characters appeared earlier in the buffer (byte offset used where a char offset was required)
- undo tree jumps to a distant history node by replaying the diff path from the common ancestor, which got slower the further away the target was; added snapshot-based teleport so distant jumps restore state directly instead of replaying every intermediate edit
- quote text objects (`i"`/`a"` etc.) used a "nearest enclosing, doubled-quote" pairing rule that diverges from real vim; switching to vim's sequential pairing broke that nesting behavior, so quote/character disambiguation now only kicks in when the cursor sits exactly on a quote char
- a leading count before an operator (e.g. `2dw`) was captured eagerly and cleared the count text objects rely on later, breaking text-object grammar; the count is now captured lazily on the first digit typed after entering operator-pending mode
- deleting a surround pair shared between multiple selected regions could shift a sibling region onto the wrong (inner) pair after the first deletion; fixed by tracking already-consumed coordinate ranges and skipping any region whose anchor falls inside one, instead of shifting positions


## Install

```sh
cargo install monster-rift
```

With syntax highlighting: 

```sh
cargo install -F treesitter monster-rift
```

or install from source

```sh
git clone https://github.com/monster0506/rift
cd rift
cargo install -F treesitter --path .
```
