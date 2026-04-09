# Editor Refactoring Implementation Plan

**Goal:** Eliminate bare `.rs` files in `src/` (move each to its own `mod.rs` subdirectory) and break `src/editor/mod.rs` (~5517 lines) into focused sub-files using Rust's child-module impl split pattern.

**Architecture:** Every module in `src/` gets its own directory (`src/foo/mod.rs` instead of `src/foo.rs`). The `editor/mod.rs` `impl<T: TerminalBackend> Editor<T>` block is split across private child modules — each child does `use super::Editor;` and Rust allows accessing private fields from descendant modules. The struct definition, constructors, Drop, and small public accessors stay in `mod.rs`.

**Tech Stack:** Rust 2021 edition; no external tooling required — pure file moves and split.

---

## Pre-flight: Baseline

Before changing anything, record the current state so regressions can be caught.

**Known pre-existing failures (do NOT fix these — they exist before refactoring):**
- Test `keymap::tests::test_default_explorer_toggle_hidden_keybind` — FAILS
- Benchmark `render_bench` — fails to compile (`RenderState` has no field `modal`)

**Baseline test result:** 916 passed, 1 failed.

### Baseline benchmark numbers (run 2026-04-07)

| Bench | Group | Time (median) |
|-------|-------|---------------|
| buffer_bench | buffer_insertion/insert_char_end | ~71 µs |
| buffer_bench | buffer_insertion/insert_str_small | ~2.1 µs |
| buffer_bench | buffer_deletion/delete_backward | ~67 µs |
| buffer_bench | buffer_access/iter_full | ~6.5 ms |
| buffer_bench | buffer_access/get_line_bytes_random | ~19 µs |
| wrap_bench | displaymap_nav/visual_down_1000 | ~80 µs |
| wrap_bench | displaymap_nav/char_to_visual_row_1000 | ~27 µs |
| search_bench | (various) | 2 µs – 5 ms |

---

## Rust Mechanics: Splitting `impl` Blocks

In Rust, private struct fields are visible to all **descendant** modules (not just the defining module). A child module `editor::run_loop` declared via `mod run_loop;` in `editor/mod.rs` is a descendant of `editor`, so it can freely access private fields of `Editor<T>`.

Pattern for every new file:

```rust
// src/editor/some_group.rs
use super::Editor;
use crate::term::TerminalBackend;
// ... any other crate imports this file needs

impl<T: TerminalBackend> Editor<T> {
    fn method_one(&mut self) { ... }
    fn method_two(&mut self) -> bool { ... }
}
```

And in `editor/mod.rs`, add at the top (after existing `pub mod actions;`):

```rust
mod some_group;
```

---

## Part 1 — Move Bare src-Root Files to Subdirectories

Files to move (each becomes `src/<name>/mod.rs`):

| From | To | Notes |
|------|----|-------|
| `src/character.rs` | `src/character/mod.rs` | Has `#[path = "character_tests.rs"] mod tests;` — update to just `mod tests;` |
| `src/character_tests.rs` | `src/character/tests.rs` | Moved alongside |
| `src/constants.rs` | `src/constants/mod.rs` | No tests |
| `src/dot_repeat.rs` | `src/dot_repeat/mod.rs` | No tests |
| `src/key.rs` | `src/key/mod.rs` | No inline tests |
| `src/message.rs` | `src/message/mod.rs` | No tests |
| `src/mode.rs` | `src/mode/mod.rs` | No tests |
| `src/perf.rs` | `src/perf/mod.rs` | No tests |
| `src/wrap.rs` | `src/wrap/mod.rs` | Has inline `#[cfg(test)] mod tests;` at end — keep inline |

`src/lib.rs` does **not** need changes (module names stay the same).

### Task 1: Move `character`

**Files:**
- Create: `src/character/mod.rs` (content of `src/character.rs`, with one edit)
- Create: `src/character/tests.rs` (content of `src/character_tests.rs`)
- Delete: `src/character.rs`, `src/character_tests.rs`

- [ ] **Step 1: Create directory and copy file**

```bash
mkdir src/character
cp src/character.rs src/character/mod.rs
cp src/character_tests.rs src/character/tests.rs
```

- [ ] **Step 2: Fix the `#[path]` attribute in `src/character/mod.rs`**

The last line of `src/character.rs` is:
```rust
#[cfg(test)]
#[path = "character_tests.rs"]
mod tests;
```

Change it to:
```rust
#[cfg(test)]
mod tests;
```

(The file is now `character/tests.rs` so no `#[path]` is needed.)

- [ ] **Step 3: Delete the old files**

```bash
rm src/character.rs src/character_tests.rs
```

- [ ] **Step 4: Verify tests still pass**

```bash
cargo test character 2>&1 | tail -5
```

Expected: all character tests pass, same count as before.

- [ ] **Step 5: Commit**

```bash
git add src/character/ && git rm src/character.rs src/character_tests.rs
git commit -m "refactor: move character module to src/character/mod.rs"
```

---

### Task 2: Move `constants`, `dot_repeat`, `key`, `message`, `mode`

**Files:**
- Create: `src/constants/mod.rs`, `src/dot_repeat/mod.rs`, `src/key/mod.rs`, `src/message/mod.rs`, `src/mode/mod.rs`
- Delete: corresponding `src/*.rs` files

- [ ] **Step 1: Create directories and copy files**

```bash
mkdir src/constants src/dot_repeat src/key src/message src/mode
cp src/constants.rs src/constants/mod.rs
cp src/dot_repeat.rs src/dot_repeat/mod.rs
cp src/key.rs src/key/mod.rs
cp src/message.rs src/message/mod.rs
cp src/mode.rs src/mode/mod.rs
```

- [ ] **Step 2: Delete old files**

```bash
rm src/constants.rs src/dot_repeat.rs src/key.rs src/message.rs src/mode.rs
```

- [ ] **Step 3: Verify compilation and tests**

```bash
cargo test 2>&1 | tail -10
```

Expected: same pass/fail counts as baseline (916 passed, 1 failed).

- [ ] **Step 4: Commit**

```bash
git add src/constants/ src/dot_repeat/ src/key/ src/message/ src/mode/
git rm src/constants.rs src/dot_repeat.rs src/key.rs src/message.rs src/mode.rs
git commit -m "refactor: move constants/dot_repeat/key/message/mode to subdirectories"
```

---

### Task 3: Move `perf` and `wrap`

**Files:**
- Create: `src/perf/mod.rs`, `src/wrap/mod.rs`
- Delete: `src/perf.rs`, `src/wrap.rs`

- [ ] **Step 1: Create directories and copy files**

```bash
mkdir src/perf src/wrap
cp src/perf.rs src/perf/mod.rs
cp src/wrap.rs src/wrap/mod.rs
```

- [ ] **Step 2: Delete old files**

```bash
rm src/perf.rs src/wrap.rs
```

- [ ] **Step 3: Verify compilation and tests**

```bash
cargo test 2>&1 | tail -10
cargo bench --bench wrap_bench 2>&1 | grep "time:" | head -10
```

Expected: same pass/fail counts; wrap_bench times within 10% of baseline.

- [ ] **Step 4: Commit**

```bash
git add src/perf/ src/wrap/
git rm src/perf.rs src/wrap.rs
git commit -m "refactor: move perf and wrap modules to subdirectories"
```

---

## Part 2 — Split `editor/mod.rs` Into Focused Sub-Files

The `impl<T: TerminalBackend> Editor<T>` block (lines 151–5505) will be split into 14 child modules. Each file gets `mod <name>;` in `editor/mod.rs`. The struct definition, constructor, Drop, `plugin_dirs`, `resolve_display_map`, and three small public accessors stay in `mod.rs`.

**New `editor/` directory structure after this part:**

```
editor/
  mod.rs            (~340 lines)  - struct, helpers, new/with_file, Drop, tiny accessors
  document_ops.rs   (~270 lines)  - file/document lifecycle
  run_loop.rs       (~385 lines)  - main event loop
  action_handler.rs (~570 lines)  - handle_action dispatch + insert text
  panel_handlers.rs (~270 lines)  - buffer-specific action handlers, clipboard/messages
  plugin_ops.rs     (~400 lines)  - Lua state, plugin highlight adjustment, mutation apply
  file_ops.rs       (~360 lines)  - save/quit, buffer nav, notifications, split window
  history.rs        (~115 lines)  - undo/redo/undo-tree navigation
  command_exec.rs   (~115 lines)  - execute_buffer_command, incremental syntax parse
  mode_mgmt.rs      (~300 lines)  - handle_mode_management, set_mode
  render.rs         (~550 lines)  - all render/viewport methods
  completion.rs     (~80 lines)   - completion result handling
  explorer.rs       (~980 lines)  - explorer, clipboard, undotree panels
  operators.rs      (~205 lines)  - execute_operator, dot-repeat, spawn_syntax_parse_job
  jobs.rs           (~410 lines)  - handle_job_message
  command_line.rs   (~200 lines)  - execute_command_line, handle_command_line_message, search highlights
  actions.rs        (unchanged)   - re-export only
  context_impl.rs   (unchanged)   - EditorContext trait impl
  terminal_tests.rs (unchanged)   - test helper
  tests.rs          (unchanged)   - integration tests
```

---

### Task 4: Extract `document_ops.rs`

Functions to move from `editor/mod.rs`:
- `switch_focus` (~line 349)
- `save_current_view_state` (~line 368)
- `restore_view_state` (~line 376)
- `sync_state_with_active_document` (~line 386)
- `force_full_redraw` (~line 416)
- `load_plugins` (~line 427)
- `remove_document` (~line 452)
- `open_file` (~line 466)
- `open_terminal` (~line 536)
- `perform_search` (~line 565)
- `goto_line` (~line 602)
- `run_command` (~line 607)
- `jump_to_pattern` (~line 612)

**Files:**
- Create: `src/editor/document_ops.rs`
- Modify: `src/editor/mod.rs` (remove those functions, add `mod document_ops;`)

- [ ] **Step 1: Create `src/editor/document_ops.rs`**

Add these imports at the top, then paste each function body verbatim from `mod.rs`:

```rust
use super::Editor;
use crate::error::{ErrorSeverity, ErrorType, RiftError};
use crate::mode::Mode;
use crate::search::SearchDirection;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    // paste: switch_focus, save_current_view_state, restore_view_state,
    //        sync_state_with_active_document, force_full_redraw, load_plugins,
    //        remove_document, open_file, open_terminal, perform_search,
    //        goto_line, run_command, jump_to_pattern
}
```

- [ ] **Step 2: Remove those functions from `editor/mod.rs`**

Delete lines from the start of `fn switch_focus` (~line 349) to the end of `fn jump_to_pattern` (~line 616, just before `pub fn run`).

- [ ] **Step 3: Add `mod document_ops;` to `editor/mod.rs`**

Add after `pub mod actions;` near the top:
```rust
mod document_ops;
```

- [ ] **Step 4: Verify**

```bash
cargo test 2>&1 | tail -5
```

Expected: 916 passed, 1 failed (same as baseline).

- [ ] **Step 5: Commit**

```bash
git add src/editor/document_ops.rs src/editor/mod.rs
git commit -m "refactor(editor): extract document_ops into child module"
```

---

### Task 5: Extract `run_loop.rs`

Functions to move:
- `run` (~line 617)
- `handle_key_actions` (~line 948)

**Files:**
- Create: `src/editor/run_loop.rs`
- Modify: `src/editor/mod.rs`

- [ ] **Step 1: Create `src/editor/run_loop.rs`**

```rust
use super::Editor;
use crate::error::RiftError;
use crate::key_handler::KeyAction;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    // paste: run, handle_key_actions
}
```

- [ ] **Step 2: Remove `run` and `handle_key_actions` from `editor/mod.rs`**

Delete lines from `pub fn run` to the end of `fn handle_key_actions` (just before `fn handle_action`).

- [ ] **Step 3: Add `mod run_loop;` to `editor/mod.rs`**

- [ ] **Step 4: Verify**

```bash
cargo test 2>&1 | tail -5
```

- [ ] **Step 5: Commit**

```bash
git add src/editor/run_loop.rs src/editor/mod.rs
git commit -m "refactor(editor): extract run loop into child module"
```

---

### Task 6: Extract `action_handler.rs`

Functions to move:
- `handle_action` (~line 1001)
- `insert_text_at_cursor` (~line 1492)

**Files:**
- Create: `src/editor/action_handler.rs`
- Modify: `src/editor/mod.rs`

- [ ] **Step 1: Create `src/editor/action_handler.rs`**

```rust
use super::Editor;
use crate::action::{Action, EditorAction, Motion};
use crate::command::Command;
use crate::error::RiftError;
use crate::mode::Mode;
use crate::search::SearchDirection;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    // paste: handle_action, insert_text_at_cursor
}
```

- [ ] **Step 2: Remove functions from `editor/mod.rs`**

- [ ] **Step 3: Add `mod action_handler;` to `editor/mod.rs`**

- [ ] **Step 4: Verify**

```bash
cargo test 2>&1 | tail -5
```

- [ ] **Step 5: Commit**

```bash
git add src/editor/action_handler.rs src/editor/mod.rs
git commit -m "refactor(editor): extract action handler into child module"
```

---

### Task 7: Extract `panel_handlers.rs`

Functions to move:
- `handle_directory_buffer_action`
- `handle_undotree_buffer_action`
- `handle_messages_buffer_action`
- `handle_clipboard_buffer_action`
- `handle_clipboard_entry_action`
- `handle_clipboard_entry_close`
- `refresh_clipboard_buffer_if_open`
- `handle_clipboard_select`
- `handle_clipboard_new`
- `apply_clipboard_entry_save`
- `refresh_messages_buffer_if_open`

**Files:**
- Create: `src/editor/panel_handlers.rs`
- Modify: `src/editor/mod.rs`

- [ ] **Step 1: Create `src/editor/panel_handlers.rs`**

```rust
use super::Editor;
use crate::term::TerminalBackend;
// Add any additional imports that the compiler reports as missing

impl<T: TerminalBackend> Editor<T> {
    // paste all 11 functions
}
```

- [ ] **Step 2: Remove functions from `editor/mod.rs`**, add `mod panel_handlers;`

- [ ] **Step 3: Verify**

```bash
cargo test 2>&1 | tail -5
```

- [ ] **Step 4: Commit**

```bash
git add src/editor/panel_handlers.rs src/editor/mod.rs
git commit -m "refactor(editor): extract panel handlers into child module"
```

---

### Task 8: Extract `plugin_ops.rs`

Functions to move:
- `update_lua_state`
- `adjust_plugin_highlights_for_edits`
- `apply_plugin_mutations`

**Files:**
- Create: `src/editor/plugin_ops.rs`
- Modify: `src/editor/mod.rs`

- [ ] **Step 1: Create `src/editor/plugin_ops.rs`**

```rust
use super::Editor;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    // paste: update_lua_state, adjust_plugin_highlights_for_edits, apply_plugin_mutations
}
```

- [ ] **Step 2: Remove functions from `editor/mod.rs`**, add `mod plugin_ops;`

- [ ] **Step 3: Verify**

```bash
cargo test 2>&1 | tail -5
```

- [ ] **Step 4: Commit**

```bash
git add src/editor/plugin_ops.rs src/editor/mod.rs
git commit -m "refactor(editor): extract plugin ops into child module"
```

---

### Task 9: Extract `file_ops.rs`

Functions to move:
- `do_save`
- `do_save_and_quit`
- `do_quit`
- `do_buffer_next`
- `do_buffer_prev`
- `do_show_buffer_list`
- `do_notification_clear`
- `do_split_window`

**Files:**
- Create: `src/editor/file_ops.rs`
- Modify: `src/editor/mod.rs`

- [ ] **Step 1: Create `src/editor/file_ops.rs`**

```rust
use super::Editor;
use crate::error::RiftError;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    // paste all 8 functions
}
```

- [ ] **Step 2: Remove functions from `editor/mod.rs`**, add `mod file_ops;`

- [ ] **Step 3: Verify**

```bash
cargo test 2>&1 | tail -5
```

- [ ] **Step 4: Commit**

```bash
git add src/editor/file_ops.rs src/editor/mod.rs
git commit -m "refactor(editor): extract file/buffer ops into child module"
```

---

### Task 10: Extract `history.rs`

Functions to move:
- `do_undo`
- `do_redo`
- `do_undo_goto`
- `navigate_history_up`
- `navigate_history_down`

**Files:**
- Create: `src/editor/history.rs`
- Modify: `src/editor/mod.rs`

- [ ] **Step 1: Create `src/editor/history.rs`**

```rust
use super::Editor;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    // paste all 5 functions
}
```

- [ ] **Step 2: Remove functions from `editor/mod.rs`**, add `mod history;`

- [ ] **Step 3: Verify**

```bash
cargo test 2>&1 | tail -5
```

- [ ] **Step 4: Commit**

```bash
git add src/editor/history.rs src/editor/mod.rs
git commit -m "refactor(editor): extract history/undo-redo into child module"
```

---

### Task 11: Extract `command_exec.rs`

Functions to move:
- `execute_buffer_command`
- `do_incremental_syntax_parse`

**Files:**
- Create: `src/editor/command_exec.rs`
- Modify: `src/editor/mod.rs`

- [ ] **Step 1: Create `src/editor/command_exec.rs`**

```rust
use super::Editor;
use crate::command::Command;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    // paste: execute_buffer_command, do_incremental_syntax_parse
}
```

- [ ] **Step 2: Remove functions from `editor/mod.rs`**, add `mod command_exec;`

- [ ] **Step 3: Verify**

```bash
cargo test 2>&1 | tail -5
```

- [ ] **Step 4: Commit**

```bash
git add src/editor/command_exec.rs src/editor/mod.rs
git commit -m "refactor(editor): extract command execution into child module"
```

---

### Task 12: Extract `mode_mgmt.rs`

Functions to move:
- `handle_mode_management`
- `set_mode` (currently at ~line 4667 in mod.rs — move here for coherence)

Note: `set_mode` is not contiguous with `handle_mode_management` in the current file. Cut it from its current location (~line 4667) and paste it here along with `handle_mode_management`.

**Files:**
- Create: `src/editor/mode_mgmt.rs`
- Modify: `src/editor/mod.rs`

- [ ] **Step 1: Create `src/editor/mode_mgmt.rs`**

```rust
use super::Editor;
use crate::mode::Mode;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    // paste: handle_mode_management, set_mode
}
```

- [ ] **Step 2: Remove `handle_mode_management` (~line 2809) and `set_mode` (~line 4667) from `editor/mod.rs`**

Both are cut and do NOT appear in `mod.rs` anymore. Add `mod mode_mgmt;`.

- [ ] **Step 3: Verify**

```bash
cargo test 2>&1 | tail -5
```

- [ ] **Step 4: Commit**

```bash
git add src/editor/mode_mgmt.rs src/editor/mod.rs
git commit -m "refactor(editor): extract mode management into child module"
```

---

### Task 13: Extract `render.rs`

Functions to move:
- `update_state_and_render`
- `update_and_render`
- `render_clipboard_tooltip`
- `render_to_terminal`
- `render`
- `render_plugin_float`
- `update_window_viewports`
- `render_multi_window`

**Files:**
- Create: `src/editor/render.rs`
- Modify: `src/editor/mod.rs`

- [ ] **Step 1: Create `src/editor/render.rs`**

```rust
use super::Editor;
use crate::error::RiftError;
use crate::screen_buffer::FrameStats;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    // paste all 8 functions
}
```

- [ ] **Step 2: Remove functions from `editor/mod.rs`**, add `mod render;`

Note: there may be a name conflict since `editor/mod.rs` already has `use crate::render;` at the top. If so, rename the child module to `rendering` (both the file and the `mod rendering;` declaration) to avoid the clash. Adjust accordingly.

- [ ] **Step 3: Verify**

```bash
cargo test 2>&1 | tail -5
```

- [ ] **Step 4: Commit**

```bash
git add src/editor/render.rs src/editor/mod.rs  # or rendering.rs
git commit -m "refactor(editor): extract render methods into child module"
```

---

### Task 14: Extract `completion.rs`

Functions to move:
- `handle_completion_result`
- `apply_completion_text`

**Files:**
- Create: `src/editor/completion.rs`
- Modify: `src/editor/mod.rs`

- [ ] **Step 1: Create `src/editor/completion.rs`**

```rust
use super::Editor;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    // paste: handle_completion_result, apply_completion_text
}
```

- [ ] **Step 2: Remove functions from `editor/mod.rs`**, add `mod completion;`

- [ ] **Step 3: Verify**

```bash
cargo test 2>&1 | tail -5
```

- [ ] **Step 4: Commit**

```bash
git add src/editor/completion.rs src/editor/mod.rs
git commit -m "refactor(editor): extract completion handling into child module"
```

---

### Task 15: Extract `explorer.rs`

Functions to move (everything explorer/undotree/open-panel related):
- `reload_directory_buffer`
- `handle_explorer_select`
- `handle_explorer_parent`
- `open_explorer`
- `close_split_panel`
- `open_messages`
- `open_clipboard`
- `update_clipboard_preview`
- `apply_clipboard_diff`
- `open_undotree_split`
- `update_explorer_preview`
- `update_undotree_preview`
- `handle_explorer_split_select`
- `handle_explorer_split_parent`
- `handle_undotree_select`
- `handle_explorer_toggle_hidden`
- `handle_explorer_refresh`
- `handle_undotree_refresh`
- `apply_directory_diff`

**Files:**
- Create: `src/editor/explorer.rs`
- Modify: `src/editor/mod.rs`

- [ ] **Step 1: Create `src/editor/explorer.rs`**

```rust
use super::Editor;
use crate::error::RiftError;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    // paste all 19 functions
}
```

- [ ] **Step 2: Remove all those functions from `editor/mod.rs`**, add `mod explorer;`

- [ ] **Step 3: Verify**

```bash
cargo test 2>&1 | tail -5
```

- [ ] **Step 4: Commit**

```bash
git add src/editor/explorer.rs src/editor/mod.rs
git commit -m "refactor(editor): extract explorer/panel operations into child module"
```

---

### Task 16: Extract `operators.rs`

Functions to move:
- `term_mut` (currently near line 4703 — tiny accessor, can stay in mod.rs OR move here)
- `execute_operator`
- `execute_operator_linewise`
- `execute_dot_repeat`
- `spawn_syntax_parse_job`

**Files:**
- Create: `src/editor/operators.rs`
- Modify: `src/editor/mod.rs`

- [ ] **Step 1: Create `src/editor/operators.rs`**

```rust
use super::Editor;
use crate::action::{Motion, OperatorType};
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    // paste: execute_operator, execute_operator_linewise, execute_dot_repeat, spawn_syntax_parse_job
    // term_mut can stay in mod.rs
}
```

- [ ] **Step 2: Remove those functions from `editor/mod.rs`**, add `mod operators;`

Keep `term_mut` in `mod.rs` since it's a one-liner public accessor.

- [ ] **Step 3: Verify**

```bash
cargo test 2>&1 | tail -5
```

- [ ] **Step 4: Commit**

```bash
git add src/editor/operators.rs src/editor/mod.rs
git commit -m "refactor(editor): extract operator execution into child module"
```

---

### Task 17: Extract `jobs.rs`

Functions to move:
- `handle_job_message`

**Files:**
- Create: `src/editor/jobs.rs`
- Modify: `src/editor/mod.rs`

- [ ] **Step 1: Create `src/editor/jobs.rs`**

```rust
use super::Editor;
use crate::error::RiftError;
use crate::job_manager::JobMessage;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    // paste: handle_job_message
}
```

- [ ] **Step 2: Remove from `editor/mod.rs`**, add `mod jobs;`

- [ ] **Step 3: Verify**

```bash
cargo test 2>&1 | tail -5
```

- [ ] **Step 4: Commit**

```bash
git add src/editor/jobs.rs src/editor/mod.rs
git commit -m "refactor(editor): extract job message handling into child module"
```

---

### Task 18: Extract `command_line.rs`

Functions to move:
- `update_search_highlights`
- `execute_command_line`
- `handle_command_line_message`

**Files:**
- Create: `src/editor/command_line.rs`
- Modify: `src/editor/mod.rs`

- [ ] **Step 1: Create `src/editor/command_line.rs`**

```rust
use super::Editor;
use crate::error::RiftError;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    // paste: update_search_highlights, execute_command_line, handle_command_line_message
}
```

- [ ] **Step 2: Remove those functions from `editor/mod.rs`**, add `mod command_line;`

After this step `editor/mod.rs` should contain only:
- Module-level doc comment
- `pub mod actions;` and all the new `mod foo;` declarations
- `use` imports
- `plugin_dirs()` free function
- `resolve_display_map()` free function
- `struct Editor<T: TerminalBackend>` definition
- `struct PostPasteState`, `enum PanelKind`, `struct PanelLayout`
- `impl<T: TerminalBackend> Editor<T>` with only: `new`, `with_file`, `active_document_id`, `active_document`, `term_mut`
- `impl<T: TerminalBackend> Drop for Editor<T>`
- `mod context_impl;`
- `#[cfg(test)] mod tests;`

- [ ] **Step 3: Verify line count of mod.rs**

```bash
wc -l src/editor/mod.rs
```

Expected: ~330–360 lines.

- [ ] **Step 4: Final test + benchmark run**

```bash
cargo test 2>&1 | tail -10
cargo bench --bench buffer_bench --bench wrap_bench --bench search_bench 2>&1 | grep "time:"
```

Expected: 916 passed, 1 failed. Benchmark times within 10% of baseline.

- [ ] **Step 5: Commit**

```bash
git add src/editor/command_line.rs src/editor/mod.rs
git commit -m "refactor(editor): extract command-line handling into child module"
```

---

## Final Verification

- [ ] Run full test suite: `cargo test 2>&1 | tail -5`
  - Expected: 916 passed, 1 failed (`keymap::tests::test_default_explorer_toggle_hidden_keybind`)
- [ ] Verify no bare `.rs` files remain in `src/` root (except `main.rs` and `lib.rs`):
  ```bash
  ls src/*.rs
  ```
  Expected output: only `src/lib.rs` and `src/main.rs`.
- [ ] Verify `editor/mod.rs` is under 400 lines:
  ```bash
  wc -l src/editor/mod.rs
  ```
- [ ] Run benchmarks one more time and compare to baseline numbers above.

---

## Troubleshooting Notes

**Import errors after moving a function:** The child module needs explicit `use crate::...` imports for every type it references. Start with `use super::Editor; use crate::term::TerminalBackend;` and add more as the compiler reports `E0412` (type not found) errors. The compiler tells you exactly what's missing.

**Name conflict `mod render` vs `use crate::render`:** Rename the child module to `mod rendering;` / `rendering.rs`. Inside `rendering.rs`, import the render crate as `use crate::render;`.

**Private field access errors:** Should not happen — Rust allows descendant module code to access private fields of a type defined in an ancestor module. If you see this, double-check that the file is declared as `mod foo;` inside `editor/mod.rs` (making it a child), not as a separate crate module.
