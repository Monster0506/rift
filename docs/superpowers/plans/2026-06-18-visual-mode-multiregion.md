# Visual Mode & Multi-Region Selection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the full `visual-mode-design.md` spec — Visual/VisualLine/VisualBlock modes, a non-contiguous multi-region `SelectionSet`, expand-region, the `gv` regions window, every set-aware command (`d`/`c`/`y`/`i`/`a`/`I`/`A`/`o`/`O`/`r`/`:s`/`sg`/`sd`/`sc`/`p`/`P`), and dot-repeat for multi-region operations.

**Architecture:** A new `src/selection` module owns `Region`/`SelectionSet` (plain char-offset anchor/cursor pairs — see deviation note below) and two generic drivers, `apply_to_each_region`/`enter_multi_insert`, that every set-aware command routes through. `Document` owns one `SelectionSet`. Visual mode is three new flat `Mode` variants with the in-progress anchor tracked as a separate `Editor` field, exactly like `pending_operator` today. Rendering reuses the existing search-annotation mechanism (`ui.search` → new `ui.selection.*` annotation kinds).

**Tech Stack:** Existing Rift internals only — `tree_sitter` is unrelated to this feature. No new crates.

## Deviation from the design doc (verified against this codebase, not guessed)

The design doc's §3 says regions store "plain byte offsets". **This codebase's buffer is char-indexed throughout** — `TextBuffer::cursor()`, `wrap::MotionRange::{anchor,new_cursor}`, `LineIndex::get_start`, and `clipboard::capture_text` all operate in **char** units (confirmed by reading `src/buffer/mod.rs`, `src/wrap/mod.rs`, `src/clipboard/mod.rs`). Byte offsets only exist at the tree-sitter/annotation boundary (`buffer.char_to_byte`). So:

- `Region { anchor: usize, cursor: usize }` stores **char offsets**, matching `MotionRange` and `buffer.cursor()` exactly — no conversion needed to feed a region into `compute_motion_range`-style helpers.
- Convert to byte offsets only when calling into `annotations` (rendering, Task 7) via `buffer.char_to_byte`, mirroring `Document::sync_search_annotations` exactly.

## Global Constraints

- Follow `visual-mode-design.md` exactly for *behavior*; the only intentional deviation is the char-vs-byte offset point above.
- **No `git` commands of any kind in this plan** — not `git add`, not `git commit`, nothing. Leave the working tree exactly as the edits leave it; the user reviews and stages/commits manually. Every task ends at "run the tests," full stop.
- TDD per `superpowers:test-driven-development`: write the failing test, watch it fail for the right reason, write minimal code, watch it pass.
- House rule (`CLAUDE.md`): no comment over 2 lines; keep comments ASCII outside test fixtures.
- After every task: `cargo test` (default features). For any test touching real tree-sitter grammars, also run `cargo test --features treesitter`.
- `cargo build --tests` must stay warning-free (existing pre-existing warning in `src/syntax/tests.rs` is not yours to fix).
- Reuse existing patterns exactly where named (e.g. `Document::sync_search_annotations` for Task 7) — do not invent a parallel mechanism.

## Task Index (28 tasks, 12 phases)

1. Foundation: `RangeKind::Blockwise`
2. Foundation: `Mode::Visual`/`VisualLine`/`VisualBlock`
3. `src/selection/mod.rs`: `Region`/`SelectionSet` core + bank/merge + cycling + `bank_occurrence`
4. Wire `SelectionSet` onto `Document`; clear on undo/redo/goto_seq
5. Visual entry/resume (`v`/`V`/`Ctrl-V`) + `Esc` commit/clear
6. `KeyContext::Visual` + motion fall-through + `o`/`O` swap
7. Rendering: active region + banked regions via annotations
8. Integration test: the issue's worked example end-to-end
9. `n`/`N` region cycling — context-sensitive (falls back to repeat-find/search when the set is empty)
10. `m`/`M` pattern-growth banking (disabled for Blockwise)
11. `apply_to_each_region` driver
12. `enter_multi_insert` driver + dot-repeat plumbing
13. `d`/`y` via `apply_to_each_region`
14. `c` via `enter_multi_insert` (canonical `foo`/`foofoo` test)
15. Insert family `i`/`a`/`I`/`A`/`o`/`O` via `enter_multi_insert`
16. `r` via `apply_to_each_region`
17. `sd`/`sc`/`sg` via `apply_to_each_region`
18. `:s` substitute scoped to region text
19. `p`/`P`/`PutSystemClipboard` via `apply_to_each_region`, non-destructive bare-repeat semantics
20. Single-undo-step regression test
21. `DotRegister::RegionBuildSession` + recording
22. `execute_dot_repeat` replay split by destructiveness
23. Expand region `<Space>`
24. Shrink region `<Shift-Space>` + fallback key
25. `BufferKind::Regions` list buffer
26. `gv` toggle + window navigation + `j`/`k`/`Enter`/`x`/`q`/`d`/`c`/`y` inside the window
27. VisualBlock: type only — no rectangle rendering/editing, `O` stays the plain swap; verify merge restricted to block-vs-block
28. Final §9 regression sweep (remaining tests not yet covered by earlier tasks)

---

## Phase 1 — Core data model

### Task 1: `RangeKind::Blockwise`

**Files:**
- Modify: `src/wrap/mod.rs:291` (the `RangeKind` enum)
- Test: `src/wrap/mod.rs` (inline `#[cfg(test)] mod tests` — check if one exists first; if not, this codebase puts wrap tests in `src/wrap/mod.rs` itself per existing style, add one)

**Interfaces:**
- Produces: `RangeKind::Blockwise` variant, usable everywhere `RangeKind::{Charwise,Linewise}` is matched today.

- [ ] **Step 1: Find every existing match on `RangeKind`**

Run: `grep -rn "RangeKind::" src/ --include="*.rs"`

This will show every call site (clipboard capture, render, etc.) that must compile after adding a variant — Rust will give you a compile error at each non-exhaustive match, which is your checklist.

- [ ] **Step 2: Add the variant**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeKind {
    Charwise,
    Linewise,
    /// Rectangular column-bounded selection (Ctrl-V Visual Block).
    Blockwise,
}
```

- [ ] **Step 3: Fix every non-exhaustive match from Step 1's grep**

For each site that isn't part of *this* plan's later tasks (i.e. anything in `src/clipboard/mod.rs`'s `capture_text`, any renderer), add a `RangeKind::Blockwise => { /* same as Charwise for now */ }` arm or fold it into an existing `Charwise | Linewise` arm if the logic genuinely doesn't care yet. Do **not** implement real blockwise behavior here — that's Task 27. The only goal of this task is "compiles, and the type exists."

- [ ] **Step 4: Run the build**

Run: `cargo build --tests 2>&1 | grep -E "error|warning"`
Expected: no output (clean build).

- [ ] **Step 5: Run the full suite**

Run: `cargo test 2>&1 | tail -5`
Expected: same pass count as before this task (no behavior changed yet).

- [ ] **Step 6: Stage**


---

### Task 2: `Mode::Visual` / `VisualLine` / `VisualBlock`

**Files:**
- Modify: `src/mode/mod.rs`
- Modify: `src/editor/run_loop.rs:233-252` (context resolution `match self.current_mode`)

**Interfaces:**
- Produces: `Mode::Visual`, `Mode::VisualLine`, `Mode::VisualBlock`, all reporting `as_str() == "visual"` (matches the design doc's note that Lua plugins see no distinction, mirroring how `OperatorPending` collapses to `"normal"` today).
- Produces: a `Mode::is_visual(self) -> bool` helper (`true` for all three variants) — later tasks (Task 5, 6, 13) use this instead of re-deriving the `matches!` three times.

- [ ] **Step 1: Write a failing test for `as_str` and `is_visual`**

Add to `src/mode/mod.rs` (create a `#[cfg(test)] mod tests` block at the bottom if none exists):

```rust
#[cfg(test)]
mod tests {
    use super::Mode;

    #[test]
    fn visual_variants_report_as_visual_string() {
        assert_eq!(Mode::Visual.as_str(), "visual");
        assert_eq!(Mode::VisualLine.as_str(), "visual");
        assert_eq!(Mode::VisualBlock.as_str(), "visual");
    }

    #[test]
    fn is_visual_true_only_for_visual_variants() {
        assert!(Mode::Visual.is_visual());
        assert!(Mode::VisualLine.is_visual());
        assert!(Mode::VisualBlock.is_visual());
        assert!(!Mode::Normal.is_visual());
        assert!(!Mode::OperatorPending.is_visual());
        assert!(!Mode::Insert.is_visual());
    }
}
```

- [ ] **Step 2: Run it, confirm it fails to compile**

Run: `cargo test --lib mode::tests -- --nocapture 2>&1 | tail -20`
Expected: compile error, `Mode::Visual` does not exist.

- [ ] **Step 3: Add the variants and the two methods**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Command,
    Search,
    OperatorPending,
    Rename,
    Replace,
    /// Charwise visual selection (`v`).
    Visual,
    /// Linewise visual selection (`V`).
    VisualLine,
    /// Rectangular visual selection (`Ctrl-V`).
    VisualBlock,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Normal | Mode::OperatorPending => "normal",
            Mode::Insert => "insert",
            Mode::Command => "command",
            Mode::Search => "search",
            Mode::Rename => "rename",
            Mode::Replace => "replace",
            Mode::Visual | Mode::VisualLine | Mode::VisualBlock => "visual",
        }
    }

    /// True for any of the three Visual-family modes.
    pub fn is_visual(self) -> bool {
        matches!(self, Mode::Visual | Mode::VisualLine | Mode::VisualBlock)
    }
}
```

- [ ] **Step 4: Run the test again**

Run: `cargo test --lib mode::tests -- --nocapture 2>&1 | tail -20`
Expected: 2 passed.

- [ ] **Step 5: Fix the now-non-exhaustive match in `run_loop.rs`**

Read `src/editor/run_loop.rs:233-252` first. It currently reads:

```rust
match self.current_mode {
    Mode::Normal | Mode::OperatorPending => {
        if is_directory {
            KeyContext::FileExplorer
        } else if is_undotree {
            KeyContext::UndoTree
        } else if is_clipboard {
            KeyContext::Clipboard
        } else if is_clipboard_entry {
            KeyContext::ClipboardEntry
        } else if is_terminal {
            KeyContext::TerminalNormal
        } else if is_location_list {
            KeyContext::LocationList
        } else if self.current_mode == Mode::OperatorPending {
            KeyContext::OperatorPending
        } else {
            KeyContext::Normal
        }
    }
    Mode::Insert | Mode::Replace => KeyContext::Insert,
    Mode::Command => KeyContext::Command,
    Mode::Search => KeyContext::Search,
    Mode::Rename => KeyContext::Command,
}
```

Change the first arm's pattern from `Mode::Normal | Mode::OperatorPending` to also include the three visual modes, and inside the body add a visual branch **before** the directory/undotree/etc. checks (visual selection only happens on regular file/buffer content, but checking it first is simplest and correct — none of those buffer kinds support entering Visual mode in the first place per existing keymap gating, so ordering is safe either way):

```rust
match self.current_mode {
    Mode::Normal | Mode::OperatorPending | Mode::Visual | Mode::VisualLine | Mode::VisualBlock => {
        if self.current_mode.is_visual() {
            KeyContext::Visual
        } else if is_directory {
            KeyContext::FileExplorer
        } else if is_undotree {
            KeyContext::UndoTree
        } else if is_clipboard {
            KeyContext::Clipboard
        } else if is_clipboard_entry {
            KeyContext::ClipboardEntry
        } else if is_terminal {
            KeyContext::TerminalNormal
        } else if is_location_list {
            KeyContext::LocationList
        } else if self.current_mode == Mode::OperatorPending {
            KeyContext::OperatorPending
        } else {
            KeyContext::Normal
        }
    }
    Mode::Insert | Mode::Replace => KeyContext::Insert,
    Mode::Command => KeyContext::Command,
    Mode::Search => KeyContext::Search,
    Mode::Rename => KeyContext::Command,
}
```

`KeyContext::Visual` doesn't exist yet — that's Task 6. For *this* task, temporarily comment that one line out (`// KeyContext::Visual` → leave `KeyContext::Normal` as a placeholder) so the build stays green; Task 6 will replace it. Note this explicitly so the next task's implementer isn't confused — add a one-line `// TODO(task 6): route to KeyContext::Visual` comment (this is the one sanctioned TODO in this plan, since it names the exact task that resolves it).

- [ ] **Step 6: Build and test**

Run: `cargo build --tests 2>&1 | grep -E "error|warning"`
Expected: no output.
Run: `cargo test 2>&1 | tail -5`
Expected: same pass count as before.

- [ ] **Step 7: Stage**


---

### Task 3: `src/selection/mod.rs` — `Region`/`SelectionSet` core

**Files:**
- Create: `src/selection/mod.rs`
- Modify: `src/lib.rs` (add `pub mod selection;` near the other top-level module declarations — find the exact insertion point with `grep -n "^pub mod " src/lib.rs` and insert alphabetically)
- Test: inline `#[cfg(test)] mod tests` at the bottom of `src/selection/mod.rs`

**Interfaces:**
- Produces:
  - `pub struct Region { pub anchor: usize, pub cursor: usize, pub kind: RangeKind }` (char offsets, see Deviation note)
  - `impl Region { pub fn span(&self) -> (usize, usize) }` → pure anchor/cursor char-offset math, `end` exclusive. Used **only** for set bookkeeping (merge/sort/cycle) — never for reading or mutating buffer text.
  - `impl Region { pub fn buffer_span(&self, buf: &TextBuffer) -> (usize, usize) }` → the range this region *actually covers in the buffer*: identical to `span()` for Charwise/Blockwise, but expanded to whole lines for Linewise (mirrors `clipboard::capture_text`'s Linewise branch). **Every task that deletes, yanks, inserts at, or highlights a region's text must call `buffer_span`, not `span`** — using `span()` for a Linewise region's delete/highlight range is a correctness bug (it would only touch the literal anchor..cursor chars, not the whole line).
  - `pub struct SelectionSet { pub regions: Vec<Region>, pub active: Option<Region> }`
  - `impl SelectionSet`:
    - `pub fn is_empty(&self) -> bool` (regions empty AND active is None)
    - `pub fn clear(&mut self)`
    - `pub fn bank(&mut self, region: Region)` — merge-on-true-overlap-only (§3), same-kind only, repeats until no further merges
    - `pub fn commit_active(&mut self)` — if `active.take()` is `Some`, `bank()` it
    - `pub fn sorted(&self) -> Vec<Region>` — ascending by `span().0`
    - `pub fn take_for_batch(&mut self) -> Vec<Region>` — `commit_active()`, then drain `regions` sorted **descending** by `span().0` (highest-offset-first, per §8's "process regions in a stable order... so earlier regions' offsets stay valid as later regions are removed/replaced"), leaving the set empty
    - `pub fn region_containing(&self, offset: usize) -> Option<usize>` — index into `regions` of the (committed) region containing `offset`, or `None`
    - `pub fn next_region(&self, after: usize) -> Option<&Region>` / `pub fn prev_region(&self, before: usize) -> Option<&Region>` — cycling helpers for Task 9, wrapping around `sorted()`
    - `pub fn bank_occurrence(&mut self, buf: &TextBuffer, forward: bool) -> Option<(Region, String)>` — Task 10's `m`/`M`, returns the newly-banked region and the needle text for the status message; `None` if the set is empty or no other occurrence exists. **Returns `None` without banking if the most-recently-banked region is `Blockwise`** (§7 VisualBlock resolution).

- [ ] **Step 1: Write the failing tests (the file doesn't exist yet, so this is also where the module gets created)**

Create `src/selection/mod.rs`:

```rust
//! Multi-region, non-contiguous selection set (visual-mode-design.md).
//!
//! Regions are plain char-offset anchor/cursor pairs, not edit-tracked
//! markers -- any edit outside the set-aware drivers clears the set
//! (Document::undo/redo/goto_seq do this explicitly).

use crate::buffer::TextBuffer;
use crate::wrap::RangeKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub anchor: usize,
    pub cursor: usize,
    pub kind: RangeKind,
}

impl Region {
    pub fn new(anchor: usize, cursor: usize, kind: RangeKind) -> Self {
        Self { anchor, cursor, kind }
    }

    /// Ordered (start, end) char-offset span; `end` is exclusive. Pure
    /// anchor/cursor math, used for set bookkeeping (merge/sort/cycle) where
    /// exact line-boundary precision doesn't matter -- see `buffer_span` for
    /// the precise range a Linewise region actually covers in the buffer.
    pub fn span(&self) -> (usize, usize) {
        (self.anchor.min(self.cursor), self.anchor.max(self.cursor) + 1)
    }

    /// The actual range this region covers in `buf`: identical to `span()`
    /// for Charwise/Blockwise, but expanded to whole lines for Linewise
    /// (mirroring `clipboard::capture_text`'s Linewise branch exactly).
    /// Use this -- never `span()` -- for anything that reads or mutates
    /// buffer text (delete, yank, insert-anchor, render highlight range).
    pub fn buffer_span(&self, buf: &TextBuffer) -> (usize, usize) {
        match self.kind {
            RangeKind::Linewise => {
                let first = self.anchor.min(self.cursor);
                let last = self.anchor.max(self.cursor);
                let first_line = buf.line_index.get_line_at(first);
                let last_line = buf.line_index.get_line_at(last);
                let s = buf.line_index.get_start(first_line).unwrap_or(0);
                let e = if last_line + 1 < buf.get_total_lines() {
                    buf.line_index.get_start(last_line + 1).unwrap_or(buf.len())
                } else {
                    buf.len()
                };
                (s, e)
            }
            RangeKind::Charwise | RangeKind::Blockwise => self.span(),
        }
    }

    fn overlaps(&self, other: &Region) -> bool {
        if self.kind != other.kind {
            return false;
        }
        let (a_start, a_end) = self.span();
        let (b_start, b_end) = other.span();
        a_start < b_end && b_start < a_end
    }
}

#[derive(Debug, Clone, Default)]
pub struct SelectionSet {
    pub regions: Vec<Region>,
    pub active: Option<Region>,
}

impl SelectionSet {
    pub fn is_empty(&self) -> bool {
        self.regions.is_empty() && self.active.is_none()
    }

    pub fn clear(&mut self) {
        self.regions.clear();
        self.active = None;
    }

    /// Merge `region` into the set, repeating while it overlaps another
    /// same-kind region (touching does not count -- see design doc S3).
    pub fn bank(&mut self, region: Region) {
        let mut cur = region;
        loop {
            let Some(idx) = self.regions.iter().position(|r| r.overlaps(&cur)) else {
                break;
            };
            let other = self.regions.remove(idx);
            let (a_start, a_end) = cur.span();
            let (b_start, b_end) = other.span();
            let start = a_start.min(b_start);
            let end = a_end.max(b_end);
            cur = Region::new(start, end.saturating_sub(1), cur.kind);
        }
        self.regions.push(cur);
    }

    pub fn commit_active(&mut self) {
        if let Some(region) = self.active.take() {
            self.bank(region);
        }
    }

    pub fn sorted(&self) -> Vec<Region> {
        let mut v = self.regions.clone();
        v.sort_by_key(|r| r.span().0);
        v
    }

    pub fn take_for_batch(&mut self) -> Vec<Region> {
        self.commit_active();
        let mut v = std::mem::take(&mut self.regions);
        v.sort_by_key(|r| std::cmp::Reverse(r.span().0));
        v
    }

    pub fn region_containing(&self, offset: usize) -> Option<usize> {
        self.regions.iter().position(|r| {
            let (s, e) = r.span();
            s <= offset && offset < e
        })
    }

    pub fn next_region(&self, after: usize) -> Option<Region> {
        let sorted = self.sorted();
        if sorted.is_empty() {
            return None;
        }
        sorted
            .iter()
            .find(|r| r.span().0 > after)
            .or(sorted.first())
            .copied()
    }

    pub fn prev_region(&self, before: usize) -> Option<Region> {
        let sorted = self.sorted();
        if sorted.is_empty() {
            return None;
        }
        sorted
            .iter()
            .rev()
            .find(|r| r.span().0 < before)
            .or(sorted.last())
            .copied()
    }

    /// `m`/`M`: bank the next/previous literal occurrence of the most
    /// recently banked region's text. Disabled for Blockwise (S7).
    ///
    /// Reuses `crate::search::find_next` (the same engine `/`/`n` already use)
    /// instead of hand-rolling substring search — it's already tested,
    /// already handles char-vs-byte conversion (`SearchMatch::range` is in
    /// code-points, matching `Region`'s coordinate space exactly), and
    /// already wraps around the buffer, so none of that needs reinventing.
    pub fn bank_occurrence(&mut self, buf: &TextBuffer, forward: bool) -> Option<(Region, String)> {
        use crate::buffer::api::BufferView;
        use crate::search::{find_next, SearchDirection};

        let last = *self.regions.last()?;
        if last.kind == RangeKind::Blockwise {
            return None;
        }
        let (last_start, last_end) = last.span();
        let needle: String = buf.chars(last_start..last_end).map(|c| c.to_string()).collect();
        if needle.is_empty() {
            return None;
        }
        let (direction, start_pos) = if forward {
            (SearchDirection::Forward, last_end)
        } else {
            (SearchDirection::Backward, last_start)
        };
        let (m, _stats) = find_next(buf, start_pos, &needle, direction).ok()?;
        let m = m?;
        let new_start = m.range.start;
        let new_end = m.range.end.saturating_sub(1);
        let already_contained = self.regions.iter().any(|r| {
            let (s, e) = r.span();
            s <= new_start && new_end < e
        });
        if already_contained {
            return None;
        }
        let region = Region::new(new_start, new_end, last.kind);
        self.bank(region);
        Some((region, needle))
    }
}

#[cfg(test)]
mod tests;
```

Create `src/selection/tests.rs`:

```rust
use super::*;
use crate::buffer::TextBuffer;
use crate::buffer::api::BufferView;
use crate::wrap::RangeKind;

fn region(a: usize, c: usize) -> Region {
    Region::new(a, c, RangeKind::Charwise)
}

#[test]
fn touching_regions_do_not_merge() {
    // "foo\n\nfoofoo": bank "foo" (0..2), then the two "foo"s inside "foofoo"
    // at indices 6..8 and 9..11 -- 8's neighbor is 9, sharing zero chars.
    let mut set = SelectionSet::default();
    set.bank(region(0, 2));
    set.bank(region(6, 8));
    set.bank(region(9, 11));
    assert_eq!(set.regions.len(), 3, "touching regions must stay independent");
}

#[test]
fn true_overlap_merges_into_union() {
    let mut set = SelectionSet::default();
    set.bank(region(0, 5));
    set.bank(region(3, 8));
    assert_eq!(set.regions.len(), 1);
    assert_eq!(set.regions[0].span(), (0, 9));
}

#[test]
fn overlap_merge_chains_through_a_third_region() {
    let mut set = SelectionSet::default();
    set.bank(region(0, 2));
    set.bank(region(10, 12));
    // Overlaps both the first (0..2) and the second (10..12) at once.
    set.bank(region(1, 11));
    assert_eq!(set.regions.len(), 1);
    assert_eq!(set.regions[0].span(), (0, 13));
}

#[test]
fn merge_never_crosses_kinds() {
    let mut set = SelectionSet::default();
    set.bank(region(0, 5));
    set.bank(Region::new(2, 7, RangeKind::Linewise));
    assert_eq!(set.regions.len(), 2, "Charwise and Linewise must never merge");
}

#[test]
fn commit_active_banks_into_the_set() {
    let mut set = SelectionSet::default();
    set.active = Some(region(0, 3));
    set.commit_active();
    assert!(set.active.is_none());
    assert_eq!(set.regions.len(), 1);
}

#[test]
fn take_for_batch_orders_highest_offset_first_and_clears() {
    let mut set = SelectionSet::default();
    set.bank(region(0, 2));
    set.bank(region(10, 12));
    set.bank(region(20, 22));
    let batch = set.take_for_batch();
    assert_eq!(
        batch.iter().map(|r| r.span().0).collect::<Vec<_>>(),
        vec![20, 10, 0]
    );
    assert!(set.is_empty());
}

#[test]
fn next_region_cycles_and_wraps() {
    let mut set = SelectionSet::default();
    set.bank(region(0, 2));
    set.bank(region(10, 12));
    assert_eq!(set.next_region(5).unwrap().span().0, 10);
    assert_eq!(set.next_region(15).unwrap().span().0, 0, "wraps to first");
}

#[test]
fn prev_region_cycles_and_wraps() {
    let mut set = SelectionSet::default();
    set.bank(region(0, 2));
    set.bank(region(10, 12));
    assert_eq!(set.prev_region(15).unwrap().span().0, 10);
    assert_eq!(set.prev_region(1).unwrap().span().0, 10, "wraps to last");
}

#[test]
fn bank_occurrence_finds_next_match_and_wraps() {
    let mut buf = TextBuffer::new(20).unwrap();
    buf.insert_str("foo bar foo baz foo").unwrap();
    let mut set = SelectionSet::default();
    set.bank(region(0, 2)); // first "foo"
    let (found, needle) = set.bank_occurrence(&buf, true).unwrap();
    assert_eq!(needle, "foo");
    assert_eq!(found.span(), (8, 11));
    assert_eq!(set.regions.len(), 2);
}

#[test]
fn bank_occurrence_returns_none_when_all_occurrences_banked() {
    let mut buf = TextBuffer::new(20).unwrap();
    buf.insert_str("foo foo").unwrap();
    let mut set = SelectionSet::default();
    set.bank(region(0, 2));
    set.bank(region(4, 6));
    assert!(set.bank_occurrence(&buf, true).is_none());
}

#[test]
fn bank_occurrence_disabled_for_blockwise() {
    let mut buf = TextBuffer::new(20).unwrap();
    buf.insert_str("foo foo").unwrap();
    let mut set = SelectionSet::default();
    set.bank(Region::new(0, 2, RangeKind::Blockwise));
    assert!(set.bank_occurrence(&buf, true).is_none());
}
```

- [ ] **Step 2: Register the module**

Run: `grep -n "^pub mod s" src/lib.rs` to find the alphabetical insertion point, then add `pub mod selection;` there.

- [ ] **Step 3: Run, confirm it fails to compile (module just created, nothing wired) then passes**

Run: `cargo test --lib selection:: -- --nocapture 2>&1 | tail -60`
Expected: 11 tests pass once the module compiles. `bank_occurrence` delegates to `crate::search::find_next` for the actual searching/wrapping, so an off-by-one here is most likely in the `m.range.end.saturating_sub(1)` → `new_end` conversion (`SearchMatch::range` is an exclusive `Range<usize>`, `Region`'s `new_end` is inclusive) — check that first if `bank_occurrence_finds_next_match_and_wraps` fails.

- [ ] **Step 4: Build whole crate clean**

Run: `cargo build --tests 2>&1 | grep -E "error|warning"`
Expected: no output.

- [ ] **Step 5: Stage**


---

### Task 4: Wire `SelectionSet` onto `Document`; clear on undo/redo/goto_seq

**Files:**
- Modify: `src/document/mod.rs` (add field to `struct Document`, initialize in every constructor)
- Modify: `src/document/history.rs` (`undo`, `redo`, `goto_seq`)
- Test: `src/document/tests.rs`

**Interfaces:**
- Produces: `Document.selection_set: crate::selection::SelectionSet` (public field, plain struct, `Default`).
- Consumes: `SelectionSet::clear()` from Task 3.

- [ ] **Step 1: Write the failing test**

Add to `src/document/tests.rs`:

```rust
#[test]
fn undo_clears_selection_set() {
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut doc = Document::new(1).unwrap();
    doc.buffer.insert_str("hello world").unwrap();
    doc.selection_set.bank(Region::new(0, 4, RangeKind::Charwise));
    assert!(!doc.selection_set.is_empty());

    doc.insert_char('!').unwrap();
    doc.undo();

    assert!(doc.selection_set.is_empty(), "undo must clear a banked selection set");
}

#[test]
fn redo_clears_selection_set() {
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut doc = Document::new(1).unwrap();
    doc.buffer.insert_str("hello world").unwrap();
    assert!(doc.undo());
    doc.selection_set.bank(Region::new(0, 4, RangeKind::Charwise));
    assert!(!doc.selection_set.is_empty());

    assert!(doc.redo());

    assert!(doc.selection_set.is_empty(), "redo must clear a banked selection set");
}
```

- [ ] **Step 2: Run, confirm compile failure (`selection_set` field doesn't exist)**

Run: `cargo test --lib document::tests::undo_clears_selection_set -- --nocapture 2>&1 | tail -20`
Expected: `no field 'selection_set' on type 'Document'`.

- [ ] **Step 3: Add the field**

In `src/document/mod.rs`, add to the `Document` struct (right after `pub annotations: AnnotationStore,` is a reasonable spot — both are per-buffer structured-metadata sidecars):

```rust
    /// Non-contiguous multi-region selection set (visual-mode-design.md).
    pub selection_set: crate::selection::SelectionSet,
```

Then find every `Document` struct-literal constructor with `grep -n "annotations: AnnotationStore::new()\|annotations: Default::default()" src/document/*.rs` and add `selection_set: crate::selection::SelectionSet::default(),` alongside each one (there will be a handful — `Document::new`, `new_directory`, `new_undotree`, etc. in `src/document/factories.rs` or `mod.rs`, wherever the constructors live).

- [ ] **Step 4: Add the clearing calls**

In `src/document/history.rs`, at the end of `undo()` (right before the final `true`):

```rust
        self.selection_set.clear();
        true
    }
```

Same at the end of `redo()`:

```rust
        self.selection_set.clear();
        true
    }
```

And in `goto_seq`, right before `Ok(())`:

```rust
        self.selection_set.clear();
        Ok(())
    }
```

- [ ] **Step 5: Run the tests**

Run: `cargo test --lib document::tests::undo_clears_selection_set document::tests::redo_clears_selection_set -- --nocapture 2>&1 | tail -20`
Expected: 2 passed.

- [ ] **Step 6: Full suite**

Run: `cargo test 2>&1 | tail -5`
Expected: all green, count increased by 2 (plus Task 2/3's new tests).

- [ ] **Step 7: Stage**


---

## Phase 2 — Visual mode entry/exit & rendering

### Task 5: Visual entry/resume (`v`/`V`/`Ctrl-V`)

**Files:**
- Modify: `src/mode/mod.rs` (add `Mode::visual_range_kind`)
- Modify: `src/editor/mod.rs:140` area (add `visual_anchor` field)
- Modify: `src/editor/init.rs:98` area (initialize it)
- Modify: `src/action/mod.rs` (three new `EditorAction` variants)
- Modify: `src/editor/handle_action.rs` (handlers)
- Modify: `src/keymap/defaults.rs` (register `v`/`V`/`Ctrl-V` under `KeyContext::Normal`)
- Test: `src/editor/tests.rs`

**Interfaces:**
- Consumes: `Mode::is_visual` (Task 2), `crate::selection::{Region, SelectionSet}` (Task 3), `Document.selection_set` (Task 4).
- Produces: `Mode::visual_range_kind(self) -> Option<RangeKind>` (`None` for non-visual modes). `Editor.visual_anchor: Option<usize>` (char offset, `pub(super)` like its siblings). `EditorAction::EnterVisualChar`, `EnterVisualLine`, `EnterVisualBlock` (each either starts a fresh region anchored at the cursor, or — if the cursor sits inside an already-banked region — pops that region back out as the active one, restoring its original anchor/cursor direction).

- [ ] **Step 1: Write the failing test**

Add to `src/editor/tests.rs`:

```rust
#[test]
fn visual_char_enters_mode_and_anchors_at_cursor() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world");
    editor.active_document().buffer.set_cursor(3).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));

    assert_eq!(editor.current_mode, Mode::Visual);
    assert_eq!(editor.visual_anchor, Some(3));
}

#[test]
fn visual_resumes_a_banked_region_under_the_cursor() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "hello world");
    // Bank a region covering "hello" (0..4), drag direction cursor->anchor
    // reversed (anchor=4, cursor=0) to prove direction is restored exactly.
    editor
        .active_document()
        .selection_set
        .bank(Region::new(4, 0, RangeKind::Charwise));

    editor.active_document().buffer.set_cursor(2).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));

    assert_eq!(editor.current_mode, Mode::Visual);
    assert_eq!(editor.visual_anchor, Some(4), "anchor side must be restored");
    assert_eq!(editor.active_document().buffer.cursor(), 0, "cursor side must be restored");
    assert!(
        editor.active_document().selection_set.regions.is_empty(),
        "resumed region must be popped out of the banked set"
    );
}
```

- [ ] **Step 2: Confirm it fails to compile**

Run: `cargo test --lib editor::tests::visual_char_enters_mode -- --nocapture 2>&1 | tail -20`
Expected: `EditorAction::EnterVisualChar` / `Mode::Visual` not found (depending what's missing first).

- [ ] **Step 3: Add `Mode::visual_range_kind`**

In `src/mode/mod.rs`, below `is_visual`:

```rust
    /// The `RangeKind` a region built from this mode should use, or `None`
    /// for non-visual modes.
    pub fn visual_range_kind(self) -> Option<crate::wrap::RangeKind> {
        match self {
            Mode::Visual => Some(crate::wrap::RangeKind::Charwise),
            Mode::VisualLine => Some(crate::wrap::RangeKind::Linewise),
            Mode::VisualBlock => Some(crate::wrap::RangeKind::Blockwise),
            _ => None,
        }
    }
```

- [ ] **Step 4: Add the `visual_anchor` field**

In `src/editor/mod.rs`, right after `pending_surround_add: Option<usize>,`:

```rust
    /// Char offset of the in-progress Visual region's anchor; `None` outside
    /// Visual/VisualLine/VisualBlock. The cursor side is `buffer.cursor()`.
    pub(super) visual_anchor: Option<usize>,
```

In `src/editor/init.rs`, right after `pending_surround_add: None,`:

```rust
            visual_anchor: None,
```

- [ ] **Step 5: Add the three `EditorAction` variants**

In `src/action/mod.rs`, next to `SurroundGiveLine`:

```rust
    /// `v`: enter/resume charwise Visual selection.
    EnterVisualChar,
    /// `V`: enter/resume linewise Visual selection.
    EnterVisualLine,
    /// `Ctrl-V`: enter/resume blockwise Visual selection.
    EnterVisualBlock,
```

- [ ] **Step 6: Implement the handler**

In `src/editor/handle_action.rs`, add one match arm per variant, all delegating to a shared helper (add the helper as a new `pub(super) fn` in the same `impl` block, e.g. right after the `SurroundGiveLine` arm's closing brace):

```rust
            EditorAction::EnterVisualChar => self.enter_visual_or_resume(Mode::Visual),
            EditorAction::EnterVisualLine => self.enter_visual_or_resume(Mode::VisualLine),
            EditorAction::EnterVisualBlock => self.enter_visual_or_resume(Mode::VisualBlock),
```

And the helper (outside the big `match`, as a sibling method in the same `impl<T: TerminalBackend> Editor<T>` block — `handle_action.rs` has several helper methods below the main dispatch `match`, e.g. `insert_text_at_cursor`; add this one alongside them):

```rust
    /// `v`/`V`/`Ctrl-V`: start a fresh active region anchored at the cursor,
    /// or -- if the cursor sits inside an already-banked region of the same
    /// kind -- pop it back out as the active region, restoring its exact
    /// original anchor/cursor direction (design.md S3).
    pub(super) fn enter_visual_or_resume(&mut self, mode: Mode) -> bool {
        let Some(kind) = mode.visual_range_kind() else { return false };
        let Some(doc) = self.document_manager.active_document_mut() else { return false };
        let cursor = doc.buffer.cursor();
        if let Some(idx) = doc.selection_set.region_containing(cursor) {
            let region = doc.selection_set.regions.remove(idx);
            if region.kind == kind {
                self.visual_anchor = Some(region.anchor);
                let _ = doc.buffer.set_cursor(region.cursor);
                self.set_mode(mode);
                return true;
            }
            doc.selection_set.regions.insert(idx, region);
        }
        self.visual_anchor = Some(cursor);
        self.set_mode(mode);
        true
    }
```

- [ ] **Step 7: Register the keymap entries**

In `src/keymap/defaults.rs`, near the other Normal-mode mode-entry bindings (e.g. right after the `EnterReplaceMode`/`R` registration if one exists, or simply alongside the Operator registrations at line ~640):

```rust
    keymap.register(
        KeyContext::Normal,
        Key::Char('v'),
        Action::Editor(EditorAction::EnterVisualChar),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Char('V'),
        Action::Editor(EditorAction::EnterVisualLine),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Ctrl(b'v'),
        Action::Editor(EditorAction::EnterVisualBlock),
    );
```

- [ ] **Step 8: Run the tests**

Run: `cargo test --lib editor::tests::visual_char_enters_mode editor::tests::visual_resumes_a_banked_region -- --nocapture 2>&1 | tail -30`
Expected: 2 passed. If `visual_resumes_a_banked_region_under_the_cursor` fails on the cursor assertion, check `region_containing`'s `span()` math (Task 3) — `Region::new(4, 0, ...)`'s span is `(0, 5)` (ordered), so cursor `2` is inside it; that's correct, the bug (if any) is in how `anchor`/`cursor` get reassigned, not in `span()`.

- [ ] **Step 9: Full suite**

Run: `cargo test 2>&1 | tail -5`

---

### Task 6: `KeyContext::Visual` + motion fall-through + `o`/`O` swap

**Files:**
- Modify: `src/keymap/mod.rs` (add `KeyContext::Visual`, parent fallback to `Normal`)
- Modify: `src/editor/run_loop.rs` (resolve the `TODO(task 6)` placeholder from Task 2 Step 5)
- Modify: `src/action/mod.rs` (`EditorAction::VisualSwapEnds`)
- Modify: `src/editor/handle_action.rs` (`VisualSwapEnds` handler; extend `EnterNormalMode` to commit/clear)
- Modify: `src/keymap/defaults.rs` (register `o`/`O` under `Visual`, and `d`/`c`/`y` under `Visual` reusing the existing `EditorAction::Operator` action)
- Test: `src/editor/tests.rs`

**Interfaces:**
- Consumes: `Editor.visual_anchor` (Task 5), `Document.selection_set` (Task 4).
- Produces: `EditorAction::VisualSwapEnds` (swaps `visual_anchor` and the buffer cursor — in VisualBlock this still swaps both for now; the column-only variant is Task 27, deferred until Blockwise rendering exists). Extends the *existing* `EditorAction::EnterNormalMode` (no new action) so `Esc` commits the active region (Visual) or clears a non-empty banked set (Normal).

- [ ] **Step 1: Write the failing tests**

Add to `src/editor/tests.rs`:

```rust
#[test]
fn visual_motion_extends_through_normal_fallthrough() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world");
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));

    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));

    assert_eq!(editor.current_mode, Mode::Visual, "motion must not exit Visual");
    assert_eq!(editor.active_document().buffer.cursor(), 2);
    assert_eq!(editor.visual_anchor, Some(0), "anchor stays fixed while cursor moves");
}

#[test]
fn visual_swap_ends_exchanges_anchor_and_cursor() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world");
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::Move(crate::action::Motion::Right)));
    editor.handle_action(&Action::Editor(EditorAction::Move(crate::action::Motion::Right)));
    // anchor=0, cursor=2

    editor.handle_action(&Action::Editor(EditorAction::VisualSwapEnds));

    assert_eq!(editor.visual_anchor, Some(2));
    assert_eq!(editor.active_document().buffer.cursor(), 0);
}

#[test]
fn escape_in_visual_commits_active_region() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world");
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::Move(crate::action::Motion::Right)));
    editor.handle_action(&Action::Editor(EditorAction::Move(crate::action::Motion::Right)));

    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(editor.current_mode, Mode::Normal);
    assert!(editor.visual_anchor.is_none());
    assert_eq!(editor.active_document().selection_set.regions.len(), 1);
    assert_eq!(editor.active_document().selection_set.regions[0].span(), (0, 3));
}

#[test]
fn escape_in_normal_clears_a_nonempty_banked_set() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "hello world");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 2, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert!(editor.active_document().selection_set.is_empty());
}
```

- [ ] **Step 2: Confirm compile/behavior failure**

Run: `cargo test --lib editor::tests::visual_motion_extends editor::tests::visual_swap_ends editor::tests::escape_in_visual editor::tests::escape_in_normal -- --nocapture 2>&1 | tail -40`
Expected: compile error (`VisualSwapEnds` missing) first; once stubbed in, `visual_motion_extends_through_normal_fallthrough` fails because `KeyContext::Visual` doesn't exist yet so `Move` isn't even reachable through the new context (it still works today because Normal-context `Move` bindings are reached via the *old* `run_loop.rs` placeholder routing Visual to `KeyContext::Normal` directly — that placeholder accidentally makes this one test pass already; the other three fail because the commit/clear logic doesn't exist).

- [ ] **Step 3: Add `KeyContext::Visual`**

In `src/keymap/mod.rs`:

```rust
pub enum KeyContext {
    Global,
    UndoTree,
    Normal,
    OperatorPending,
    Insert,
    Command,
    Search,
    FileExplorer,
    Clipboard,
    ClipboardEntry,
    Terminal,
    TerminalNormal,
    LocationList,
    /// Visual/VisualLine/VisualBlock selection. Falls through to `Normal` so
    /// every motion remains available without re-registering it.
    Visual,
}
```

And in `parent_context`:

```rust
    fn parent_context(context: KeyContext) -> Option<KeyContext> {
        match context {
            KeyContext::OperatorPending => Some(KeyContext::Normal),
            KeyContext::Visual => Some(KeyContext::Normal),
            KeyContext::FileExplorer
            | KeyContext::UndoTree
            | KeyContext::Clipboard
            | KeyContext::ClipboardEntry
            | KeyContext::LocationList => Some(KeyContext::Normal),
            KeyContext::Terminal => Some(KeyContext::Global),
            KeyContext::TerminalNormal => Some(KeyContext::Normal),
            KeyContext::Normal | KeyContext::Insert | KeyContext::Command | KeyContext::Search => {
                Some(KeyContext::Global)
            }
            KeyContext::Global => None,
        }
    }
```

- [ ] **Step 4: Resolve Task 2's placeholder in `run_loop.rs`**

Find the `// TODO(task 6): route to KeyContext::Visual` comment from Task 2 Step 5 and replace that line:

```rust
                if self.current_mode.is_visual() {
                    KeyContext::Visual
                } else if is_directory {
```

(delete the TODO comment).

- [ ] **Step 5: Add `EditorAction::VisualSwapEnds`**

In `src/action/mod.rs`, next to the `EnterVisual*` variants:

```rust
    /// `o`/`O` in Visual: swap which end (anchor vs cursor) is active.
    VisualSwapEnds,
```

- [ ] **Step 6: Implement `VisualSwapEnds` and extend `EnterNormalMode`**

In `src/editor/handle_action.rs`, add:

```rust
            EditorAction::VisualSwapEnds => {
                let Some(anchor) = self.visual_anchor else { return false };
                let Some(doc) = self.document_manager.active_document_mut() else { return false };
                let cursor = doc.buffer.cursor();
                self.visual_anchor = Some(cursor);
                let _ = doc.buffer.set_cursor(anchor);
                true
            }
```

Then extend the existing `EnterNormalMode` arm (Task 6 touches this same arm Task 2 left alone) — insert this block as the **first** statement inside it, before the existing `if self.current_mode == Mode::Insert ...` check:

```rust
            EditorAction::EnterNormalMode => {
                if self.current_mode.is_visual() {
                    if let (Some(anchor), Some(kind)) =
                        (self.visual_anchor, self.current_mode.visual_range_kind())
                    {
                        if let Some(doc) = self.document_manager.active_document_mut() {
                            let cursor = doc.buffer.cursor();
                            doc.selection_set
                                .bank(crate::selection::Region::new(anchor, cursor, kind));
                        }
                    }
                    self.visual_anchor = None;
                } else if let Some(doc) = self.document_manager.active_document_mut() {
                    doc.selection_set.clear();
                }
                if self.current_mode == Mode::Insert || self.current_mode == Mode::Replace {
```

(the rest of the existing body is unchanged — `set_mode(Mode::Normal)` already happens further down in the existing code, which is correct for the Visual case too since it always transitions to Normal).

- [ ] **Step 7: Register `o`/`O` and `d`/`c`/`y` under `KeyContext::Visual`**

In `src/keymap/defaults.rs`, near the Visual entry registrations from Task 5:

```rust
    keymap.register(
        KeyContext::Visual,
        Key::Char('o'),
        Action::Editor(EditorAction::VisualSwapEnds),
    );
    keymap.register(
        KeyContext::Visual,
        Key::Char('O'),
        Action::Editor(EditorAction::VisualSwapEnds),
    );
    keymap.register(
        KeyContext::Visual,
        Key::Char('d'),
        Action::Editor(EditorAction::Operator(crate::action::OperatorType::Delete)),
    );
    keymap.register(
        KeyContext::Visual,
        Key::Char('c'),
        Action::Editor(EditorAction::Operator(crate::action::OperatorType::Change)),
    );
    keymap.register(
        KeyContext::Visual,
        Key::Char('y'),
        Action::Editor(EditorAction::Operator(crate::action::OperatorType::Yank)),
    );
```

These three reuse the *existing* `EditorAction::Operator` action (no new variant) — Task 13/14 extend that action's handler to commit the active Visual region and run the batch driver when appropriate. Until Task 13 lands, pressing `d`/`c`/`y` in Visual mode will compile and dispatch but won't yet do anything useful beyond what Task 13 adds; that's expected and fine for this task's test scope (this task only tests `EnterNormalMode`/`VisualSwapEnds`/motion fall-through, not `d`/`c`/`y`).

- [ ] **Step 8: Run the tests**

Run: `cargo test --lib editor::tests::visual_motion_extends editor::tests::visual_swap_ends editor::tests::escape_in_visual editor::tests::escape_in_normal -- --nocapture 2>&1 | tail -40`
Expected: 4 passed.

- [ ] **Step 9: Full suite**

Run: `cargo test 2>&1 | tail -5`

---

### Task 7: Rendering — active region + banked regions via annotations

**Files:**
- Modify: `src/document/mod.rs` or a new `src/document/selection_render.rs` (matches the existing one-concern-per-file pattern seen in `src/document/search.rs`; create the new file and declare `mod selection_render;` in `src/document/mod.rs`)
- Modify: `src/editor/rendering.rs:111-114`
- Test: `src/document/tests.rs`

**Interfaces:**
- Consumes: `crate::selection::Region` (Task 3), `Editor.visual_anchor` + `Mode::visual_range_kind` (Task 5).
- Produces: `Document::sync_selection_annotations(&mut self, active: Option<Region>, banked: &[Region])`, `Document::clear_selection_annotations(&mut self)`, `Editor::update_selection_highlights(&mut self)` (`pub(super)`, called once per render frame).

- [ ] **Step 1: Write the failing test**

Create `src/document/selection_render.rs` is the implementation file; the test goes in `src/document/tests.rs` (existing convention for `Document`-level tests):

```rust
#[test]
fn sync_selection_annotations_creates_one_entry_per_banked_region_plus_active() {
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut doc = Document::new(1).unwrap();
    doc.buffer.insert_str("hello world").unwrap();

    let banked = vec![Region::new(0, 2, RangeKind::Charwise)];
    let active = Some(Region::new(6, 8, RangeKind::Charwise));
    doc.sync_selection_annotations(active, &banked);

    let count = doc.annotations.query_kind("ui.selection").count();
    assert_eq!(count, 2, "one banked + one active annotation");
}

#[test]
fn sync_selection_annotations_replaces_previous_call() {
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut doc = Document::new(1).unwrap();
    doc.buffer.insert_str("hello world").unwrap();

    doc.sync_selection_annotations(None, &[Region::new(0, 2, RangeKind::Charwise)]);
    doc.sync_selection_annotations(None, &[]);

    let count = doc.annotations.query_kind("ui.selection").count();
    assert_eq!(count, 0);
}
```

Verified during self-review (`grep -n "pub fn iter\|pub fn query_kind" src/annotations/mod.rs`): `AnnotationStore::query_kind(&self, prefix: &str) -> impl Iterator<Item = &Annotation>` already exists (`src/annotations/mod.rs:435`) and does exactly the prefix-match this test needs — used directly above instead of hand-rolling a filter over `Kind` (which has no `as_str`).

- [ ] **Step 2: Confirm compile failure**

Run: `cargo test --lib document::tests::sync_selection_annotations -- --nocapture 2>&1 | tail -20`
Expected: `no method named 'sync_selection_annotations'`.

- [ ] **Step 3: Implement, mirroring `Document::sync_search_annotations` exactly (`src/document/search.rs:10-50`)**

Create `src/document/selection_render.rs`:

```rust
//! Rendering hook for the multi-region selection set: mirrors
//! Document::sync_search_annotations (search.rs) but for ui.selection.*.

use super::Document;
use crate::selection::Region;

const BANKED_COLORS: [crate::color::Color; 4] = [
    crate::color::Color::Yellow,
    crate::color::Color::Green,
    crate::color::Color::Magenta,
    crate::color::Color::Cyan,
];

impl Document {
    /// Mirror the active + banked selection regions into `ui.selection.*`
    /// annotations so highlighting renders through the presentation
    /// pipeline, exactly like search highlights.
    pub fn sync_selection_annotations(&mut self, active: Option<Region>, banked: &[Region]) {
        use crate::annotations::{Anchor, Annotation, AnnotationOwner, Kind, Presentation, StyleOverride};

        self.annotations.clear_by_kind_prefix("ui.selection");

        for (i, region) in banked.iter().enumerate() {
            let (start_char, end_char) = region.buffer_span(&self.buffer);
            let start = self.buffer.char_to_byte(start_char);
            let end = self.buffer.char_to_byte(end_char);
            if start >= end {
                continue;
            }
            let style = StyleOverride {
                bg: Some(BANKED_COLORS[i % BANKED_COLORS.len()]),
                ..Default::default()
            };
            self.annotations.add(
                Annotation::new(Kind::new("ui.selection.banked"), Anchor::range(start, end), AnnotationOwner::System)
                    .with_presentation(Presentation::with_style(style).with_priority(4)),
            );
        }

        if let Some(region) = active {
            let (start_char, end_char) = region.buffer_span(&self.buffer);
            let start = self.buffer.char_to_byte(start_char);
            let end = self.buffer.char_to_byte(end_char);
            if start < end {
                let style = StyleOverride {
                    bg: Some(crate::color::Color::Blue),
                    ..Default::default()
                };
                self.annotations.add(
                    Annotation::new(Kind::new("ui.selection.active"), Anchor::range(start, end), AnnotationOwner::System)
                        .with_presentation(Presentation::with_style(style).with_priority(6)),
                );
            }
        }
    }

    /// Remove all selection-highlight annotations.
    pub fn clear_selection_annotations(&mut self) {
        self.annotations.clear_by_kind_prefix("ui.selection");
    }
}
```

Declare it in `src/document/mod.rs` next to the other `mod` declarations (`grep -n "^mod \|^pub mod " src/document/mod.rs` to find where `mod search;`-equivalent lives, since `search.rs`'s `impl Document` block has no explicit `mod search;` if it's wired via `#[path]` in a different way — check that file's registration pattern first and copy it exactly for `selection_render`).

- [ ] **Step 4: Add the per-frame `Editor` hook**

In `src/editor/rendering.rs`, add this new method (anywhere in the same `impl<T: TerminalBackend> Editor<T>` block as `update_and_render`):

```rust
    /// Recompute `ui.selection.*` annotations from the active Visual region
    /// (if any) and the active document's banked `SelectionSet`.
    pub(super) fn update_selection_highlights(&mut self) {
        let is_visual = self.current_mode.is_visual();
        let visual_anchor = self.visual_anchor;
        let visual_kind = self.current_mode.visual_range_kind();
        let Some(doc) = self.document_manager.active_document_mut() else {
            return;
        };
        let active = if is_visual {
            visual_anchor
                .zip(visual_kind)
                .map(|(anchor, kind)| crate::selection::Region::new(anchor, doc.buffer.cursor(), kind))
        } else {
            None
        };
        let banked = doc.selection_set.sorted();
        doc.sync_selection_annotations(active, &banked);
    }
```

And call it at the top of `update_and_render` (`src/editor/rendering.rs:111-114`), right after the early-return:

```rust
    pub fn update_and_render(&mut self) -> Result<(), RiftError> {
        let Some((_doc_id, needs_clear, display_map)) = self.update_state() else {
            return Ok(());
        };
        self.update_selection_highlights();

        self.render_plugin_float();
```

- [ ] **Step 5: Run the tests**

Run: `cargo test --lib document::tests::sync_selection_annotations -- --nocapture 2>&1 | tail -30`
Expected: 2 passed.

- [ ] **Step 6: Full suite**

Run: `cargo test 2>&1 | tail -5`

---

### Task 8: Integration test — the issue's worked example, end to end

**Files:**
- Test: `src/editor/tests.rs`

**Interfaces:**
- Consumes: everything from Tasks 1-7. This task adds **no new production code** — it is a checkpoint verifying the foundation is solid before building the drivers on top of it. If it fails, the bug is in Tasks 1-7, not here.

- [ ] **Step 1: Write the test**

```rust
#[test]
fn issue_worked_example_bank_two_regions_no_delete_yet() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "Hello\nworld\nfoo\n");

    // goto line 1 (already there) -> v -> select "Ho" -> Esc
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(editor.active_document().selection_set.regions.len(), 1);
    assert_eq!(editor.active_document().selection_set.regions[0].span(), (0, 2));

    // goto line 3 (plain motion, set untouched) -> v -> select "f" -> Esc
    let line3_start = editor.active_document().buffer.line_start(2);
    let _ = editor.active_document().buffer.set_cursor(line3_start);
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(
        editor.active_document().selection_set.regions.len(),
        2,
        "first region must survive the plain motion to line 3"
    );
    let spans: Vec<(usize, usize)> = editor
        .active_document()
        .selection_set
        .sorted()
        .iter()
        .map(|r| r.span())
        .collect();
    assert_eq!(spans[0], (0, 2), "\"Ho\" region");
    assert_eq!(spans[1].1 - spans[1].0, 1, "\"f\" region is one char");
}
```

This deliberately stops short of `d` (the driver doesn't exist until Task 13) — it proves banking, survival-across-motion, and ordering work. Task 13/14 will add the version of this test that actually deletes.

- [ ] **Step 2: Run it**

Run: `cargo test --lib editor::tests::issue_worked_example -- --nocapture 2>&1 | tail -30`
Expected: pass. (`buffer.line_start(line: usize) -> usize` is verified to exist — `src/buffer/api.rs:52`, implemented at `src/buffer/mod.rs:489`.)

- [ ] **Step 3: Full suite**

Run: `cargo test 2>&1 | tail -5`

---

## Phase 3 — Region navigation

### Task 9: `n`/`N` region cycling — context-sensitive (confirmed conflict, resolved with the user)

**Verified, not guessed:** `Key::Char('n')`/`Key::Char('N')` under `KeyContext::Normal` are **already bound** (`src/keymap/defaults.rs:148-156`) to `EditorAction::Move(Motion::RepeatFindForward)`/`Move(Motion::RepeatFindBackward)` (repeat the last `f`/`F`/`t`/`T`, or repeat the last search match if no find-char is set). `m`/`M` are unbound — no conflict there.

**Resolution (the user's explicit choice, not this plan's default):** rather than taking over `n`/`N` unconditionally or adding new keys, `n`/`N` become **context-sensitive**: when the active document's `SelectionSet` is non-empty, they cycle banked regions; otherwise they keep their existing repeat-find/search-match behavior exactly as today. This means **no new keymap registration at all** — the interception happens inside the existing `EditorAction::Move` handler, before it resolves `RepeatFindForward`/`RepeatFindBackward` into a real motion.

**Files:**
- Modify: `src/editor/multi_region.rs` (`cycle_to_region` helper)
- Modify: `src/editor/handle_action.rs` (`EditorAction::Move` arm — intercept before the existing `RepeatFindForward`/`RepeatFindBackward` resolution)
- Test: `src/editor/tests.rs`

**Interfaces:**
- Consumes: `SelectionSet::next_region`/`prev_region` (Task 3).
- Produces: `Editor::cycle_to_region(&mut self, forward: bool) -> bool` — moves the cursor to the next/previous banked region in buffer order, cycling; status message and `false` if the set is empty (this branch is only reached when the set is non-empty in practice, but stays defensive). No new `EditorAction` variant and no keymap changes — the existing `n`/`N` → `Move(RepeatFindForward/Backward)` bindings are reused as-is.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn n_cycles_banked_regions_when_set_is_nonempty() {
    use crate::action::{Action, EditorAction, Motion};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789abcdefghij");
    editor.active_document().selection_set.bank(Region::new(0, 1, RangeKind::Charwise));
    editor.active_document().selection_set.bank(Region::new(10, 11, RangeKind::Charwise));
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::RepeatFindForward)));
    assert_eq!(editor.active_document().buffer.cursor(), 10);

    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::RepeatFindForward)));
    assert_eq!(editor.active_document().buffer.cursor(), 0, "wraps to first");
}

#[test]
fn shift_n_cycles_backward_when_set_is_nonempty() {
    use crate::action::{Action, EditorAction, Motion};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789abcdefghij");
    editor.active_document().selection_set.bank(Region::new(0, 1, RangeKind::Charwise));
    editor.active_document().selection_set.bank(Region::new(10, 11, RangeKind::Charwise));
    editor.active_document().buffer.set_cursor(15).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::RepeatFindBackward)));
    assert_eq!(editor.active_document().buffer.cursor(), 10);
}

#[test]
fn n_keeps_repeat_find_behavior_when_set_is_empty() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "foo bar foo baz");
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.state.last_find_char = Some(('o', true, false)); // as if `fo` had just run

    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::RepeatFindForward)));

    // Repeat-find-char behavior, completely untouched: lands on the next 'o'.
    assert_eq!(editor.active_document().buffer.cursor(), 1);
}
```

The third test is the one that actually proves context-sensitivity (not just "region cycling works") — without it, a regression that made `n` *always* try region-cycling (and silently fall through to nothing useful when the set is empty, since `cycle_to_region` would just no-op) could slip through unnoticed.

- [ ] **Step 2: Confirm failure**

Run: `cargo test --lib editor::tests::n_cycles_banked_regions editor::tests::shift_n_cycles_backward editor::tests::n_keeps_repeat_find_behavior -- --nocapture 2>&1 | tail -40`
Expected: the third test already passes (nothing changed yet for the empty-set path); the first two fail.

- [ ] **Step 3: Implement `cycle_to_region`**

Add to `src/editor/multi_region.rs`:

```rust
    /// `n`/`N` when the `SelectionSet` is non-empty: cycle the cursor
    /// between banked regions instead of repeat-find/search (design.md S3,
    /// resolved as context-sensitive per this codebase's existing n/N bindings).
    pub(super) fn cycle_to_region(&mut self, forward: bool) -> bool {
        let Some(doc) = self.document_manager.active_document_mut() else {
            return false;
        };
        if doc.selection_set.is_empty() {
            self.state.notify(
                crate::notification::NotificationType::Info,
                "No regions banked".to_string(),
            );
            return false;
        }
        let cursor = doc.buffer.cursor();
        let target = if forward {
            doc.selection_set.next_region(cursor)
        } else {
            doc.selection_set.prev_region(cursor)
        };
        let Some(region) = target else { return false };
        let (start, _) = region.span();
        let _ = doc.buffer.set_cursor(start);
        true
    }
```

- [ ] **Step 4: Intercept in the `Move` handler**

In `src/editor/handle_action.rs`, the `EditorAction::Move(motion)` arm currently begins:

```rust
            EditorAction::Move(motion) => {
                use crate::action::Motion;

                // Interface-mode buffers snap vertical motion between actionable
                // lines, else fall through to ordinary motion (design.md sec 9.4).
                if self.current_mode == Mode::Normal
                    && matches!(motion, Motion::Up | Motion::Down)
                    ...
```

Add the region-cycling check as the very first statement, before the interface-mode check:

```rust
            EditorAction::Move(motion) => {
                use crate::action::Motion;

                if matches!(motion, Motion::RepeatFindForward | Motion::RepeatFindBackward) {
                    let has_regions = self
                        .document_manager
                        .active_document()
                        .map(|d| !d.selection_set.is_empty())
                        .unwrap_or(false);
                    if has_regions {
                        let forward = matches!(motion, Motion::RepeatFindForward);
                        return self.cycle_to_region(forward);
                    }
                }

                // Interface-mode buffers snap vertical motion between actionable
                // lines, else fall through to ordinary motion (design.md sec 9.4).
                if self.current_mode == Mode::Normal
                    && matches!(motion, Motion::Up | Motion::Down)
                    ...
```

(everything from the interface-mode check onward is the existing body, completely unchanged).

- [ ] **Step 5: Run the tests**

Run: `cargo test --lib editor::tests::n_cycles_banked_regions editor::tests::shift_n_cycles_backward editor::tests::n_keeps_repeat_find_behavior -- --nocapture 2>&1 | tail -50`
Expected: 3 passed.

- [ ] **Step 6: Full suite**

Run: `cargo test 2>&1 | tail -5`

---

### Task 10: `m`/`M` pattern-growth banking

**Files:**
- Modify: `src/action/mod.rs` (`EditorAction::RegionBankOccurrenceNext`, `RegionBankOccurrencePrev`)
- Modify: `src/editor/handle_action.rs`
- Modify: `src/keymap/defaults.rs`
- Test: `src/editor/tests.rs`

**Interfaces:**
- Consumes: `SelectionSet::bank_occurrence` (Task 3, already handles the Blockwise guard and "already banked" case).
- Produces: `EditorAction::RegionBankOccurrenceNext`/`Prev` — bank the next/previous literal occurrence of the most-recently-banked region's text, moving the cursor there. Status message when there's nothing to bank yet, or no further occurrence.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn region_bank_occurrence_next_finds_and_moves_cursor() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "foo bar foo baz foo");
    editor.active_document().selection_set.bank(Region::new(0, 2, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::RegionBankOccurrenceNext));

    assert_eq!(editor.active_document().selection_set.regions.len(), 2);
    assert_eq!(editor.active_document().buffer.cursor(), 8);
}

#[test]
fn region_bank_occurrence_on_empty_set_is_a_noop() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "foo bar foo");

    let handled = editor.handle_action(&Action::Editor(EditorAction::RegionBankOccurrenceNext));

    assert!(!handled);
    assert!(editor.active_document().selection_set.is_empty());
}

#[test]
fn region_bank_occurrence_disabled_for_blockwise_last_region() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "foo bar foo");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 2, RangeKind::Blockwise));

    let handled = editor.handle_action(&Action::Editor(EditorAction::RegionBankOccurrenceNext));

    assert!(!handled);
    assert_eq!(editor.active_document().selection_set.regions.len(), 1);
}
```

- [ ] **Step 2: Confirm failure, add the action variants**

Run: `cargo test --lib editor::tests::region_bank_occurrence -- --nocapture 2>&1 | tail -30`

In `src/action/mod.rs`:

```rust
    /// `m` (Normal): bank the next occurrence of the last-banked region's text.
    RegionBankOccurrenceNext,
    /// `M` (Normal): bank the previous occurrence of the last-banked region's text.
    RegionBankOccurrencePrev,
```

- [ ] **Step 3: Implement the handler**

```rust
            EditorAction::RegionBankOccurrenceNext | EditorAction::RegionBankOccurrencePrev => {
                let forward = matches!(editor_action, EditorAction::RegionBankOccurrenceNext);
                let Some(doc) = self.document_manager.active_document_mut() else {
                    return false;
                };
                if doc.selection_set.regions.is_empty() {
                    self.state.notify(
                        crate::notification::NotificationType::Info,
                        "Bank a region first (v + Esc)".to_string(),
                    );
                    return false;
                }
                let buf_snapshot = doc.buffer.clone();
                match doc.selection_set.bank_occurrence(&buf_snapshot, forward) {
                    Some((region, needle)) => {
                        let (start, _) = region.span();
                        let _ = doc.buffer.set_cursor(start);
                        self.state.notify(
                            crate::notification::NotificationType::Info,
                            format!("Banked occurrence of \"{}\"", needle),
                        );
                        true
                    }
                    None => {
                        self.state.notify(
                            crate::notification::NotificationType::Info,
                            "No further occurrence to bank".to_string(),
                        );
                        false
                    }
                }
            }
```

`bank_occurrence` takes `&TextBuffer`, but `doc.buffer` is the same buffer we're about to mutate via `set_cursor` — cloning it first (`buf_snapshot`) sidesteps a borrow conflict cheaply (the buffer is small piece-table state, not the full text; this mirrors how `spawn_syntax_parse_job` already clones `doc.buffer` for the same kind of reason).

- [ ] **Step 4: Register keys**

```rust
    keymap.register(
        KeyContext::Normal,
        Key::Char('m'),
        Action::Editor(EditorAction::RegionBankOccurrenceNext),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Char('M'),
        Action::Editor(EditorAction::RegionBankOccurrencePrev),
    );
```

Verified (`grep -n "Char('m')\|Char('M')" src/keymap/defaults.rs`, exit 1, no matches): `m`/`M` are genuinely unbound under every context, unlike `n`/`N` (Task 9) — register them directly, no conflict to resolve.

- [ ] **Step 5: Run the tests**

Run: `cargo test --lib editor::tests::region_bank_occurrence -- --nocapture 2>&1 | tail -40`
Expected: 3 passed.

- [ ] **Step 6: Full suite**

Run: `cargo test 2>&1 | tail -5`

---

## Phase 4 — Two generic drivers + `d`/`y`/`c`

**Deviation note:** the design doc (S8) suggests `src/selection/multi_region.rs` for the two drivers. This codebase keeps pure data (`src/selection/`) separate from `Editor`-method submodules (`src/editor/operators.rs`, `src/editor/jobs.rs`, etc. — see `src/editor/mod.rs:4-29`'s `mod` list). The drivers need deep `Editor` access (`dot_repeat`, `clipboard_ring`, `document_manager`), so they belong in a new `src/editor/multi_region.rs`, declared as `mod multi_region;` in `src/editor/mod.rs` alongside `mod mode_mgmt;`/`mod operators;` (insert alphabetically between them). `src/selection/mod.rs` stays pure data with no `Editor` dependency.

### Task 11: `apply_to_each_region` driver

**Files:**
- Create: `src/editor/multi_region.rs`
- Modify: `src/editor/mod.rs` (`mod multi_region;`)
- Test: `src/editor/tests.rs`

**Interfaces:**
- Consumes: `SelectionSet::take_for_batch` (Task 3, already highest-offset-first + clears the set).
- Produces: `Editor::apply_to_each_region<F>(&mut self, f: F) -> bool where F: FnMut(&mut Self, crate::selection::Region) -> bool`. Wraps the whole batch in one transaction (S5.8). Returns `false` (and does nothing) if the set is empty, so callers can fall through to single-cursor behavior.

- [ ] **Step 1: Write the failing test**

Add to `src/editor/tests.rs`:

```rust
#[test]
fn apply_to_each_region_runs_f_once_per_region_highest_offset_first() {
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.active_document().selection_set.bank(Region::new(0, 1, RangeKind::Charwise));
    editor.active_document().selection_set.bank(Region::new(5, 6, RangeKind::Charwise));

    let mut seen_starts = Vec::new();
    let handled = editor.apply_to_each_region(|_editor, region| {
        seen_starts.push(region.span().0);
        true
    });

    assert!(handled);
    assert_eq!(seen_starts, vec![5, 0], "highest-offset-first");
    assert!(editor.active_document().selection_set.is_empty(), "batch must clear the set");
}

#[test]
fn apply_to_each_region_on_empty_set_returns_false() {
    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");

    let handled = editor.apply_to_each_region(|_editor, _region| true);

    assert!(!handled);
}

#[test]
fn apply_to_each_region_deletes_are_one_undo_step() {
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.active_document().selection_set.bank(Region::new(0, 1, RangeKind::Charwise));
    editor.active_document().selection_set.bank(Region::new(5, 6, RangeKind::Charwise));

    editor.apply_to_each_region(|editor, region| {
        let (start, end) = region.span();
        if let Some(doc) = editor.document_manager.active_document_mut() {
            doc.delete_range(start, end).is_ok()
        } else {
            false
        }
    });
    assert_eq!(editor.active_document().buffer.to_string(), "234789");

    assert!(editor.active_document().undo());
    assert_eq!(
        editor.active_document().buffer.to_string(),
        "0123456789",
        "a single undo must restore both deletions at once"
    );
}
```

- [ ] **Step 2: Confirm compile failure**

Run: `cargo test --lib editor::tests::apply_to_each_region -- --nocapture 2>&1 | tail -30`
Expected: `no method named 'apply_to_each_region'`.

- [ ] **Step 3: Implement**

Create `src/editor/multi_region.rs`:

```rust
//! The two generic drivers every set-aware command routes through
//! (visual-mode-design.md S5.0): apply_to_each_region for commands that
//! compute one self-contained edit per region, and enter_multi_insert
//! (Task 12) for commands that open Insert mode and replay keystrokes.

use super::Editor;
#[allow(unused_imports)]
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    /// Run `f` once per banked region, highest-offset-first, inside one
    /// transaction so the whole batch undoes as a single step. Returns
    /// `false` without doing anything if the set is empty.
    pub(super) fn apply_to_each_region<F>(&mut self, mut f: F) -> bool
    where
        F: FnMut(&mut Self, crate::selection::Region) -> bool,
    {
        let batch = {
            let Some(doc) = self.document_manager.active_document_mut() else {
                return false;
            };
            doc.selection_set.take_for_batch()
        };
        if batch.is_empty() {
            return false;
        }
        if let Some(doc) = self.document_manager.active_document_mut() {
            doc.begin_transaction("MultiRegion");
        }
        let mut any = false;
        for region in batch {
            if f(self, region) {
                any = true;
            }
        }
        if let Some(doc) = self.document_manager.active_document_mut() {
            doc.commit_transaction();
        }
        any
    }
}
```

In `src/editor/mod.rs`, add `mod multi_region;` between `mod mode_mgmt;` and `mod operators;`.

- [ ] **Step 4: Run the tests**

Run: `cargo test --lib editor::tests::apply_to_each_region -- --nocapture 2>&1 | tail -40`
Expected: 3 passed.

- [ ] **Step 5: Full suite**

Run: `cargo test 2>&1 | tail -5`

---

### Task 12: `enter_multi_insert` driver + dot-repeat plumbing

**Files:**
- Modify: `src/editor/mod.rs:141` area (new field), `src/editor/init.rs:100` area (initialize it)
- Modify: `src/editor/multi_region.rs`
- Modify: `src/editor/handle_action.rs` (`EnterNormalMode`'s Insert-exit branch)
- Test: `src/editor/tests.rs`

**Why descending order needs no offset-shifting math:** all anchors are computed *before* any insertion happens (each `anchor_for` call may itself mutate the document — e.g. Task 14's `c` deletes the region first — but those mutations happen highest-offset-first, exactly like `apply_to_each_region`, so earlier-computed anchors are never invalidated by a later one). The *first* anchor used for live typing is the **highest-offset** one (`batch[0]`, matching `take_for_batch`'s natural order) — so when the user's interactive typing inserts text there, every *remaining* anchor (all lower-offset, replayed afterward) sits strictly before the insertion point and is never shifted by it. This is why the design doc's "first (primary) region" doesn't need marker tracking: highest-offset-first is not just consistent with the delete driver, it's also the only ordering that makes plain-offset replay correct without computing shift deltas.

**Interfaces:**
- Consumes: `Document::begin_transaction`/`commit_transaction` (existing), `DotRepeat::start_insert_recording`/`register` (existing, `src/dot_repeat/mod.rs`).
- Produces: `Editor.pending_multi_insert_anchors: Vec<usize>` (`pub(super)`). `Editor::enter_multi_insert<F>(&mut self, entry: Command, anchor_for: F) -> bool where F: FnMut(&mut Document, crate::selection::Region) -> usize` — `anchor_for` may mutate the document (e.g. delete) before returning the insert point. Extends the existing Insert-mode-exit branch of `EditorAction::EnterNormalMode` to replay the recorded session at every remaining anchor.

- [ ] **Step 1: Write the failing test**

This test drives `enter_multi_insert` directly with a no-op (pure-insert) `anchor_for`, simulating what Task 15's `i` will eventually wire up, to prove the driver itself (anchoring, recording, replay-on-exit) works in isolation:

```rust
#[test]
fn enter_multi_insert_replays_typed_session_at_every_remaining_anchor() {
    use crate::action::{Action, EditorAction};
    use crate::command::Command;
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.active_document().selection_set.bank(Region::new(0, 0, RangeKind::Charwise));
    editor.active_document().selection_set.bank(Region::new(5, 5, RangeKind::Charwise));

    let handled = editor.enter_multi_insert(Command::EnterInsertMode, |_doc, region| region.span().0);
    assert!(handled);
    assert_eq!(editor.current_mode, Mode::Insert);
    assert_eq!(editor.active_document().buffer.cursor(), 5, "starts at the highest-offset anchor");

    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "X01234X56789",
        "X inserted at both original anchors: live at 5, replayed at 0"
    );
    assert!(editor.pending_multi_insert_anchors.is_empty());
}

#[test]
fn enter_multi_insert_on_empty_set_returns_false() {
    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");

    let handled = editor.enter_multi_insert(crate::command::Command::EnterInsertMode, |_doc, region| {
        region.span().0
    });

    assert!(!handled);
    assert_eq!(editor.current_mode, Mode::Normal);
}
```

The expected string in the first test was hand-derived: original `"0123456789"`. The highest-offset anchor (`5`) is processed live first — typing `X` there gives `"01234" + "X" + "56789"` = `"01234X56789"`. Then the remaining anchor (`0`, still valid since nothing before offset 5 changed) gets `X` replayed in front of it: `"X" + "01234X56789"` = `"X01234X56789"`. If your actual output differs, the bug is almost certainly in `take_for_batch`'s sort direction or in which anchor `enter_multi_insert` treats as "first" — re-derive by hand again before changing the test.

- [ ] **Step 2: Confirm compile/behavior failure**

Run: `cargo test --lib editor::tests::enter_multi_insert -- --nocapture 2>&1 | tail -40`
Expected: compile error (`enter_multi_insert` / `pending_multi_insert_anchors` missing).

- [ ] **Step 3: Add the field**

In `src/editor/mod.rs`, right after `visual_anchor: Option<usize>,` (Task 5):

```rust
    /// Insert anchors still waiting for the recorded session to replay at,
    /// once the live-typed Insert session at the first anchor finishes.
    pub(super) pending_multi_insert_anchors: Vec<usize>,
```

In `src/editor/init.rs`, right after `visual_anchor: None,`:

```rust
            pending_multi_insert_anchors: Vec::new(),
```

- [ ] **Step 4: Implement the driver**

Add to `src/editor/multi_region.rs`:

```rust
    /// Enter Insert mode at the highest-offset anchor (computed by
    /// `anchor_for`, which may itself mutate the document -- e.g. deleting
    /// the region for `c`), recording the session via dot-repeat. On Insert
    /// exit, the recorded keystrokes replay at every remaining anchor.
    pub(super) fn enter_multi_insert<F>(
        &mut self,
        entry: crate::command::Command,
        mut anchor_for: F,
    ) -> bool
    where
        F: FnMut(&mut crate::document::Document, crate::selection::Region) -> usize,
    {
        let anchors: Vec<usize> = {
            let Some(doc) = self.document_manager.active_document_mut() else {
                return false;
            };
            let batch = doc.selection_set.take_for_batch();
            if batch.is_empty() {
                return false;
            }
            doc.begin_transaction("MultiInsert");
            batch.into_iter().map(|region| anchor_for(doc, region)).collect()
        };
        let mut anchors = anchors;
        let first = anchors.remove(0);
        self.pending_multi_insert_anchors = anchors;
        if let Some(doc) = self.document_manager.active_document_mut() {
            let _ = doc.buffer.set_cursor(first);
        }
        if !self.dot_repeat.is_replaying() {
            self.dot_repeat.start_insert_recording(entry);
        }
        self.set_mode(crate::mode::Mode::Insert);
        true
    }

    /// Replay the just-finished Insert session at every anchor still
    /// pending. Must run *before* the outer `MultiInsert` transaction
    /// (opened in `enter_multi_insert`) commits, so all anchors land in the
    /// same undo step as the live-typed one (S5.8).
    pub(super) fn replay_multi_insert_at_remaining_anchors(&mut self) {
        let anchors = std::mem::take(&mut self.pending_multi_insert_anchors);
        let Some(crate::dot_repeat::DotRegister::InsertSession { commands, .. }) =
            self.dot_repeat.register().cloned()
        else {
            return;
        };
        self.dot_repeat.set_replaying(true);
        for anchor in anchors {
            if let Some(doc) = self.document_manager.active_document_mut() {
                let _ = doc.buffer.set_cursor(anchor);
            }
            for &cmd in &commands {
                self.execute_buffer_command(cmd);
            }
        }
        self.dot_repeat.set_replaying(false);
    }
```

`DotRegister::register()` is `pub(crate)` (`src/dot_repeat/mod.rs`) — accessible from `src/editor/multi_region.rs` since both are in the same crate.

- [ ] **Step 5: Wire the replay into `EnterNormalMode`**

In `src/editor/handle_action.rs`, find the Insert-exit block (already modified once by Task 6 — this is the *same* arm, third edit to it):

```rust
                if self.current_mode == Mode::Insert || self.current_mode == Mode::Replace {
                    if !self.dot_repeat.is_replaying() {
                        self.dot_repeat.finish_insert_recording();
                    }
                    if !self.pending_multi_insert_anchors.is_empty() {
                        self.replay_multi_insert_at_remaining_anchors();
                    }
                    if let Some(doc) = self.document_manager.active_document_mut() {
                        doc.commit_transaction();
                    }
                }
```

(only the new `if !self.pending_multi_insert_anchors...` block is added; everything else in this arm is unchanged from Task 6).

- [ ] **Step 6: Run the tests**

Run: `cargo test --lib editor::tests::enter_multi_insert -- --nocapture 2>&1 | tail -50`
Expected: 2 passed.

- [ ] **Step 7: Full suite**

Run: `cargo test 2>&1 | tail -5`

---

### Task 13: `d`/`y` via `apply_to_each_region`

**Files:**
- Modify: `src/editor/multi_region.rs` (`try_run_set_aware_operator`)
- Modify: `src/editor/handle_action.rs` (`EditorAction::Operator` arm)
- Test: `src/editor/tests.rs`

**Interfaces:**
- Consumes: `apply_to_each_region` (Task 11), `Region::buffer_span` (Task 3 retrofit — **use this, not `span()`**, since these closures read/mutate buffer text), `Document::delete_range` (existing), `ClipboardRing::push` (existing).
- Produces: `Editor::try_run_set_aware_operator(&mut self, op: OperatorType) -> bool` — `false` (no-op) if the active document's `SelectionSet` is empty, so the caller falls through to today's single-cursor `OperatorPending` flow unchanged.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn set_aware_delete_removes_every_banked_region_as_one_op() {
    use crate::action::{Action, EditorAction, OperatorType};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "foo\n\nfoofoo\n");
    // Bank "foo" (0..2) and the two touching "foo"s inside "foofoo" (6..8, 9..11).
    editor.active_document().selection_set.bank(Region::new(0, 2, RangeKind::Charwise));
    editor.active_document().selection_set.bank(Region::new(6, 8, RangeKind::Charwise));
    editor.active_document().selection_set.bank(Region::new(9, 11, RangeKind::Charwise));
    assert_eq!(editor.active_document().selection_set.regions.len(), 3, "touching must not have merged");

    editor.handle_action(&Action::Editor(EditorAction::Operator(OperatorType::Delete)));

    assert_eq!(editor.active_document().buffer.to_string(), "\n\n\n");
    assert!(editor.active_document().selection_set.is_empty(), "set clears after the batch");
}

#[test]
fn set_aware_delete_is_one_undo_step() {
    use crate::action::{Action, EditorAction, OperatorType};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.active_document().selection_set.bank(Region::new(0, 1, RangeKind::Charwise));
    editor.active_document().selection_set.bank(Region::new(5, 6, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::Operator(OperatorType::Delete)));
    assert_eq!(editor.active_document().buffer.to_string(), "234789");

    assert!(editor.active_document().undo());
    assert_eq!(editor.active_document().buffer.to_string(), "0123456789");
}

#[test]
fn set_aware_yank_captures_each_region_without_mutating() {
    use crate::action::{Action, EditorAction, OperatorType};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "foo bar baz");
    editor.active_document().selection_set.bank(Region::new(0, 2, RangeKind::Charwise));
    editor.active_document().selection_set.bank(Region::new(8, 10, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::Operator(OperatorType::Yank)));

    assert_eq!(editor.active_document().buffer.to_string(), "foo bar baz", "yank must not mutate");
    assert!(editor.active_document().selection_set.is_empty());
    assert_eq!(editor.clipboard_ring.get(0), Some("foo"), "lowest-offset region pushed last = ring[0] (front-insert)");
    assert_eq!(editor.clipboard_ring.get(1), Some("baz"));
}

#[test]
fn visual_d_commits_active_region_then_runs_the_batch() {
    use crate::action::{Action, EditorAction, OperatorType};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.active_document().selection_set.bank(Region::new(5, 6, RangeKind::Charwise));
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::Move(crate::action::Motion::Right)));
    // active region now 0..1 (chars "0","1"), banked set still has 5..6 ("5")

    editor.handle_action(&Action::Editor(EditorAction::Operator(OperatorType::Delete)));

    assert_eq!(editor.active_document().buffer.to_string(), "234789", "both the just-committed and pre-banked region deleted as one batch");
    assert_eq!(editor.current_mode, Mode::Normal);
}

#[test]
fn plain_d_with_empty_set_is_unaffected() {
    use crate::action::{Action, EditorAction, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world");
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::Operator(OperatorType::Delete)));
    assert_eq!(editor.current_mode, Mode::OperatorPending, "falls through to today's single-cursor flow");
}
```

The clipboard-ring ordering in `set_aware_yank_captures_each_region_without_mutating` was hand-derived, not guessed: `apply_to_each_region` processes regions **highest-offset-first** (`take_for_batch`), so `baz(8..10)` runs first → `push("baz")` → ring = `["baz"]`. Then `foo(0..2)` runs → `push("foo")` (front-insert) → ring = `["foo", "baz"]`. So `ring.get(0) == Some("foo")`. If your implementation makes `ring.get(0) == Some("baz")` instead, the batch order got reversed somewhere — check `take_for_batch`'s sort direction (Task 3) before touching this test.

- [ ] **Step 2: Confirm failure**

Run: `cargo test --lib editor::tests::set_aware_delete editor::tests::set_aware_yank editor::tests::visual_d_commits editor::tests::plain_d_with_empty -- --nocapture 2>&1 | tail -60`
Expected: `plain_d_with_empty_set_is_unaffected` already passes (nothing changed yet for that path); the others fail because the batch never runs.

- [ ] **Step 3: Implement `try_run_set_aware_operator`**

Add to `src/editor/multi_region.rs`:

```rust
    /// `d`/`y` (and `c`, Task 14) against a non-empty `SelectionSet`: run the
    /// whole banked set as one batch instead of entering `OperatorPending`
    /// for a single motion. Returns `false` if the set is empty so the
    /// caller falls through to today's single-cursor behavior unchanged.
    pub(super) fn try_run_set_aware_operator(&mut self, op: crate::action::OperatorType) -> bool {
        use crate::action::OperatorType;
        use crate::buffer::api::BufferView;

        let is_empty = self
            .document_manager
            .active_document()
            .map(|d| d.selection_set.is_empty())
            .unwrap_or(true);
        if is_empty {
            return false;
        }

        match op {
            OperatorType::Delete => self.apply_to_each_region(|editor, region| {
                let Some(doc) = editor.document_manager.active_document_mut() else {
                    return false;
                };
                let (start, end) = region.buffer_span(&doc.buffer);
                let text: String = doc.buffer.chars(start..end).map(|c| c.to_char_lossy()).collect();
                if doc.delete_range(start, end).is_err() {
                    return false;
                }
                if !text.is_empty() {
                    editor.clipboard_ring.push(text);
                }
                true
            }),
            OperatorType::Yank => self.apply_to_each_region(|editor, region| {
                let Some(doc) = editor.document_manager.active_document() else {
                    return false;
                };
                let (start, end) = region.buffer_span(&doc.buffer);
                let text: String = doc.buffer.chars(start..end).map(|c| c.to_char_lossy()).collect();
                if !text.is_empty() {
                    editor.clipboard_ring.push(text);
                }
                true
            }),
            OperatorType::Change => {
                use crate::command::Command;
                self.enter_multi_insert(Command::EnterInsertMode, |doc, region| {
                    let (start, end) = region.buffer_span(&doc.buffer);
                    let _ = doc.delete_range(start, end);
                    start
                })
            }
        }
    }
```

- [ ] **Step 4: Wire it into `EditorAction::Operator`, and commit the active Visual region first**

In `src/editor/handle_action.rs`, the existing arm (last touched by Task 6's keymap-only change — this is its first *code* edit):

```rust
            EditorAction::Operator(op) => {
                if self.current_mode.is_visual() {
                    if let (Some(anchor), Some(kind)) =
                        (self.visual_anchor, self.current_mode.visual_range_kind())
                    {
                        if let Some(doc) = self.document_manager.active_document_mut() {
                            let cursor = doc.buffer.cursor();
                            doc.selection_set
                                .bank(crate::selection::Region::new(anchor, cursor, kind));
                        }
                    }
                    self.visual_anchor = None;
                    self.set_mode(Mode::Normal);
                }
                if self.try_run_set_aware_operator(*op) {
                    return true;
                }
                if self.current_mode == Mode::OperatorPending {
                    if let Some(pending) = self.pending_operator {
                        if pending == *op {
                            return self.execute_operator_linewise(pending);
                        }
                    }
                }
                // A fresh operator key always supersedes any in-progress `ys`.
                self.pending_surround_add = None;
                self.pending_operator = Some(*op);
                self.set_mode(Mode::OperatorPending);
                true
            }
```

(only the new `if self.current_mode.is_visual() { ... }` block and the `if self.try_run_set_aware_operator(*op) { return true; }` line are added; the rest is the existing body verbatim).

- [ ] **Step 5: Run the tests**

Run: `cargo test --lib editor::tests::set_aware_delete editor::tests::set_aware_yank editor::tests::visual_d_commits editor::tests::plain_d_with_empty -- --nocapture 2>&1 | tail -80`
Expected: 5 passed.

- [ ] **Step 6: Full suite**

Run: `cargo test 2>&1 | tail -5`

---

### Task 14: `c` via `enter_multi_insert` — the canonical `foo`/`foofoo` test

**Files:**
- Test: `src/editor/tests.rs`

**Interfaces:**
- Consumes: `try_run_set_aware_operator`'s `OperatorType::Change` arm (Task 13, already implemented — this task is pure verification, no new production code, mirroring Task 8's role for the foundation).

This is design.md's explicitly-named "canonical regression test for both the merge rule and the multi-region Change mechanism" (S5.1, S9) — it must exist as its own test, not be assumed covered by Task 13's generic delete/yank tests, because Change exercises a completely different code path (`enter_multi_insert`, not `apply_to_each_region`).

- [ ] **Step 1: Write the test**

```rust
#[test]
fn canonical_change_across_touching_regions_does_not_merge_them() {
    use crate::action::{Action, EditorAction, OperatorType};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "foo\n\nfoofoo\n");
    editor.active_document().selection_set.bank(Region::new(0, 2, RangeKind::Charwise));
    editor.active_document().selection_set.bank(Region::new(6, 8, RangeKind::Charwise));
    editor.active_document().selection_set.bank(Region::new(9, 11, RangeKind::Charwise));
    assert_eq!(editor.active_document().selection_set.regions.len(), 3);

    editor.handle_action(&Action::Editor(EditorAction::Operator(OperatorType::Change)));
    assert_eq!(editor.current_mode, Mode::Insert);

    editor.handle_action(&Action::Editor(EditorAction::InsertChar('b')));
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('a')));
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('r')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "bar\n\nbarbar\n",
        "each touching region gets its own independent 'bar', not one merged replacement"
    );
    assert_eq!(editor.current_mode, Mode::Normal);
}

#[test]
fn single_region_change_unaffected_by_the_new_batching_branch() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "foo bar");
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::Operator(OperatorType::Change)));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::NextWord)));
    assert_eq!(editor.current_mode, Mode::Insert, "ordinary single-cursor cw must still work");

    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(editor.active_document().buffer.to_string(), "Xbar");
}
```

- [ ] **Step 2: Run**

Run: `cargo test --lib editor::tests::canonical_change editor::tests::single_region_change -- --nocapture 2>&1 | tail -50`
Expected: 2 passed. If `canonical_change_across_touching_regions_does_not_merge_them` produces `"bar\n\nbar\n"` instead of `"bar\n\nbarbar\n"`, the touching regions silently merged somewhere — go back to Task 3's `bank`/`overlaps` and re-run its own unit tests first, don't patch this test's expectation.

- [ ] **Step 3: Full suite**

Run: `cargo test 2>&1 | tail -5`

---

## Phase 5 — Insert family `i`/`a`/`I`/`A`/`o`/`O`

### Task 15: Wire all six through `enter_multi_insert`

**Files:**
- Modify: `src/editor/multi_region.rs` (`try_multi_insert_for_command` + two line-offset helpers)
- Modify: `src/editor/handle_action.rs` (six `EditorAction` arms)
- Test: `src/editor/tests.rs`

**Interfaces:**
- Consumes: `enter_multi_insert` (Task 12), `Region::buffer_span` (Task 3), `Document::insert_char` (existing — routes through the proper edit-tracking/InputEdit pipeline, unlike a raw buffer mutation).
- Produces: `Editor::try_multi_insert_for_command(&mut self, entry: Command) -> bool` dispatching on which of the six commands fired, each with its own anchor function per design.md S5.2:
  - `i` → region start. `a` → region end (one past last char). `I` → line-start of the region's first row. `A` → line-end of the region's last row. `o` → insert a blank line below the region's last row, anchor at its start. `O` → insert a blank line above the region's first row, anchor at its start.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn multi_i_inserts_at_start_of_every_region() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.active_document().selection_set.bank(Region::new(0, 1, RangeKind::Charwise));
    editor.active_document().selection_set.bank(Region::new(5, 6, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::EnterInsertMode));
    assert_eq!(editor.current_mode, Mode::Insert);
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(editor.active_document().buffer.to_string(), "X01234X56789");
}

#[test]
fn multi_a_inserts_after_end_of_every_region() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.active_document().selection_set.bank(Region::new(0, 0, RangeKind::Charwise));
    editor.active_document().selection_set.bank(Region::new(5, 5, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::EnterInsertModeAfter));
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(editor.active_document().buffer.to_string(), "0X1234X56789");
}

#[test]
fn multi_capital_i_inserts_at_line_start_of_each_region_row() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "aaa\nbbb\nccc");
    // region inside "bbb" (offset 5, the second 'b') and inside "ccc" (offset 9)
    editor.active_document().selection_set.bank(Region::new(5, 5, RangeKind::Charwise));
    editor.active_document().selection_set.bank(Region::new(9, 9, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::EnterInsertModeAtLineStart));
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(editor.active_document().buffer.to_string(), "aaa\nXbbb\nXccc");
}

#[test]
fn multi_capital_a_inserts_at_line_end_of_each_region_row() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "aaa\nbbb\nccc");
    editor.active_document().selection_set.bank(Region::new(4, 4, RangeKind::Charwise));
    editor.active_document().selection_set.bank(Region::new(8, 8, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::EnterInsertModeAtLineEnd));
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(editor.active_document().buffer.to_string(), "aaaX\nbbbX\nccc");
}

#[test]
fn multi_o_opens_a_new_line_below_each_region_row() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "aaa\nbbb\nccc");
    editor.active_document().selection_set.bank(Region::new(0, 0, RangeKind::Charwise));
    editor.active_document().selection_set.bank(Region::new(4, 4, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::OpenLineBelow));
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(editor.active_document().buffer.to_string(), "aaa\nX\nbbb\nX\nccc");
}

#[test]
fn multi_capital_o_opens_a_new_line_above_each_region_row() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "aaa\nbbb\nccc");
    editor.active_document().selection_set.bank(Region::new(0, 0, RangeKind::Charwise));
    editor.active_document().selection_set.bank(Region::new(4, 4, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::OpenLineAbove));
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));

    assert_eq!(editor.active_document().buffer.to_string(), "X\naaa\nX\nbbb\nccc");
}

#[test]
fn plain_i_with_empty_set_is_unaffected() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "hello");
    editor.active_document().buffer.set_cursor(2).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::EnterInsertMode));

    assert_eq!(editor.current_mode, Mode::Insert);
    assert_eq!(editor.active_document().buffer.cursor(), 2, "ordinary i must still anchor at the live cursor");
}
```

The `multi_o`/`multi_capital_o` expected strings were hand-derived character by character (this is the step most likely to have an off-by-one if rushed): `o` on row 0 ("aaa", rows 0/1/2 = "aaa"/"bbb"/"ccc") inserts a blank line *between* "aaa" and "bbb"; `o` on row 1 ("bbb") inserts one between "bbb" and "ccc" — giving `"aaa\nX\nbbb\nX\nccc"`. For `O`, the blank lines land *before* each original row instead: `"X\naaa\nX\nbbb\nccc"`. If your actual output is off by one line, the bug is almost certainly in `line_end_offset`/`line_start_offset`'s off-by-one, not in the test.

- [ ] **Step 2: Confirm failure**

Run: `cargo test --lib editor::tests::multi_i editor::tests::multi_a editor::tests::multi_capital_i editor::tests::multi_capital_a editor::tests::multi_o editor::tests::multi_capital_o editor::tests::plain_i_with_empty -- --nocapture 2>&1 | tail -100`
Expected: `plain_i_with_empty_set_is_unaffected` already passes; the rest fail (nothing routes to `enter_multi_insert` yet).

- [ ] **Step 3: Add the two line-offset helpers**

Add to `src/editor/multi_region.rs` (free functions, not methods — they only need `&TextBuffer`):

```rust
/// Char offset of the start of `row`.
fn line_start_offset(buf: &crate::buffer::TextBuffer, row: usize) -> usize {
    buf.line_index.get_start(row).unwrap_or(0)
}

/// Char offset of the end of `row` (the position of its trailing newline,
/// or the buffer's end if `row` is the last line). Mirrors the same
/// guarded pattern `clipboard::capture_text`'s Linewise branch uses.
fn line_end_offset(buf: &crate::buffer::TextBuffer, row: usize) -> usize {
    if row + 1 < buf.get_total_lines() {
        buf.line_index.get_start(row + 1).unwrap_or(buf.len()).saturating_sub(1)
    } else {
        buf.len()
    }
}
```

- [ ] **Step 4: Implement `try_multi_insert_for_command`**

Add to `src/editor/multi_region.rs`:

```rust
    /// `i`/`a`/`I`/`A`/`o`/`O` against a non-empty `SelectionSet`: enter
    /// multi-insert instead of the single-cursor path. `false` if the set
    /// is empty (caller falls through unchanged) or `entry` isn't one of
    /// the six commands this driver handles.
    pub(super) fn try_multi_insert_for_command(&mut self, entry: crate::command::Command) -> bool {
        use crate::command::Command;

        let is_empty = self
            .document_manager
            .active_document()
            .map(|d| d.selection_set.is_empty())
            .unwrap_or(true);
        if is_empty {
            return false;
        }

        match entry {
            Command::EnterInsertMode => {
                self.enter_multi_insert(entry, |doc, region| region.buffer_span(&doc.buffer).0)
            }
            Command::EnterInsertModeAfter => {
                self.enter_multi_insert(entry, |doc, region| region.buffer_span(&doc.buffer).1)
            }
            Command::EnterInsertModeAtLineStart => self.enter_multi_insert(entry, |doc, region| {
                let (start, _) = region.buffer_span(&doc.buffer);
                let row = doc.buffer.line_index.get_line_at(start);
                line_start_offset(&doc.buffer, row)
            }),
            Command::EnterInsertModeAtLineEnd => self.enter_multi_insert(entry, |doc, region| {
                let (_, end) = region.buffer_span(&doc.buffer);
                let row = doc.buffer.line_index.get_line_at(end.saturating_sub(1));
                line_end_offset(&doc.buffer, row)
            }),
            Command::OpenLineBelow => self.enter_multi_insert(entry, |doc, region| {
                let (_, end) = region.buffer_span(&doc.buffer);
                let row = doc.buffer.line_index.get_line_at(end.saturating_sub(1));
                let target = line_end_offset(&doc.buffer, row);
                let _ = doc.buffer.set_cursor(target);
                let _ = doc.insert_char('\n');
                doc.buffer.cursor()
            }),
            Command::OpenLineAbove => self.enter_multi_insert(entry, |doc, region| {
                let (start, _) = region.buffer_span(&doc.buffer);
                let row = doc.buffer.line_index.get_line_at(start);
                let target = line_start_offset(&doc.buffer, row);
                let _ = doc.buffer.set_cursor(target);
                let _ = doc.insert_char('\n');
                target
            }),
            _ => false,
        }
    }
```

Note `o`'s anchor uses `doc.buffer.cursor()` *after* `insert_char('\n')` (which advances the cursor past the inserted newline, landing exactly on the new blank line) — but `O`'s anchor uses the *pre-insertion* `target` value directly, **not** the post-insert cursor (which would land on the now-pushed-down original line instead of the new blank line above it). This asymmetry is intentional, not a copy-paste slip — re-derive it by hand against the `multi_o`/`multi_capital_o` test strings above if it looks wrong.

- [ ] **Step 5: Wire the six `EditorAction` arms**

In `src/editor/handle_action.rs`, each of the six arms gains a guard before its existing body (shown for `EnterInsertMode`; apply the identical pattern — same `try_multi_insert_for_command` call with that arm's own command — to `EnterInsertModeAfter`, `EnterInsertModeAtLineStart`, `EnterInsertModeAtLineEnd`, `OpenLineBelow`, `OpenLineAbove`):

```rust
            EditorAction::EnterInsertMode => {
                if self.try_multi_insert_for_command(crate::command::Command::EnterInsertMode) {
                    return true;
                }
                self.handle_mode_management(crate::command::Command::EnterInsertMode);
                true
            }
            EditorAction::EnterInsertModeAfter => {
                if self.try_multi_insert_for_command(crate::command::Command::EnterInsertModeAfter) {
                    return true;
                }
                self.handle_mode_management(crate::command::Command::EnterInsertModeAfter);
                true
            }
            EditorAction::EnterInsertModeAtLineStart => {
                if self.try_multi_insert_for_command(crate::command::Command::EnterInsertModeAtLineStart) {
                    return true;
                }
                self.handle_mode_management(crate::command::Command::EnterInsertModeAtLineStart);
                true
            }
            EditorAction::EnterInsertModeAtLineEnd => {
                if self.try_multi_insert_for_command(crate::command::Command::EnterInsertModeAtLineEnd) {
                    return true;
                }
                self.handle_mode_management(crate::command::Command::EnterInsertModeAtLineEnd);
                true
            }
            EditorAction::OpenLineBelow => {
                if self.try_multi_insert_for_command(crate::command::Command::OpenLineBelow) {
                    return true;
                }
                self.handle_mode_management(crate::command::Command::OpenLineBelow);
                true
            }
            EditorAction::OpenLineAbove => {
                if self.try_multi_insert_for_command(crate::command::Command::OpenLineAbove) {
                    return true;
                }
                self.handle_mode_management(crate::command::Command::OpenLineAbove);
                true
            }
```

- [ ] **Step 6: Run the tests**

Run: `cargo test --lib editor::tests::multi_i editor::tests::multi_a editor::tests::multi_capital_i editor::tests::multi_capital_a editor::tests::multi_o editor::tests::multi_capital_o editor::tests::plain_i_with_empty -- --nocapture 2>&1 | tail -120`
Expected: 7 passed.

- [ ] **Step 7: Full suite**

Run: `cargo test 2>&1 | tail -5`

---

## Phase 6 — `r`, surround, `:s`

### Task 16: `r` via `apply_to_each_region`

**Files:**
- Modify: `src/editor/multi_region.rs`
- Modify: `src/editor/pending_grammar.rs` (`PendingGrammar::ReplaceChar` arm)
- Test: `src/editor/tests.rs`

**Interfaces:**
- Consumes: `apply_to_each_region` (Task 11), `Document::replace_repeat` (existing, `src/document/edit.rs:394`).
- Produces: `Editor::try_run_set_aware_replace_char(&mut self, ch: char) -> bool` — fills each region's full range with `ch` repeated to that region's exact length, **ignoring any numeric count prefix** (design.md S5.3).

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn set_aware_replace_char_fills_each_region_to_its_own_length() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.active_document().selection_set.bank(Region::new(0, 1, RangeKind::Charwise)); // len 2
    editor.active_document().selection_set.bank(Region::new(5, 8, RangeKind::Charwise)); // len 4

    editor.handle_action(&Action::Editor(EditorAction::ReplaceCharPending));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('x'));

    assert_eq!(editor.active_document().buffer.to_string(), "xx234xxxx9");
    assert!(editor.active_document().selection_set.is_empty());
}
```

Verified directly (`grep -n "ReplaceChar" src/action/mod.rs src/keymap/defaults.rs`): the real entry action is `EditorAction::ReplaceCharPending` (`src/action/mod.rs:376`), already used correctly above — no further check needed before running this test.

- [ ] **Step 2: Confirm failure**

Run: `cargo test --lib editor::tests::set_aware_replace_char -- --nocapture 2>&1 | tail -40`

- [ ] **Step 3: Implement**

Add to `src/editor/multi_region.rs`:

```rust
    /// `r<ch>` against a non-empty `SelectionSet`: fill each region's exact
    /// range with `ch`, ignoring any numeric count (design.md S5.3).
    pub(super) fn try_run_set_aware_replace_char(&mut self, ch: char) -> bool {
        let is_empty = self
            .document_manager
            .active_document()
            .map(|d| d.selection_set.is_empty())
            .unwrap_or(true);
        if is_empty {
            return false;
        }
        self.apply_to_each_region(|editor, region| {
            let Some(doc) = editor.document_manager.active_document_mut() else {
                return false;
            };
            let (start, end) = region.buffer_span(&doc.buffer);
            let count = end.saturating_sub(start);
            if count == 0 {
                return false;
            }
            doc.replace_repeat(start, count, ch).is_ok()
        })
    }
```

- [ ] **Step 4: Wire it into the grammar**

In `src/editor/pending_grammar.rs`, the `PendingGrammar::ReplaceChar` arm:

```rust
            PendingGrammar::ReplaceChar => {
                if let Key::Char(ch) = key {
                    if !self.try_run_set_aware_replace_char(ch) {
                        let count = if self.pending_count > 0 {
                            self.pending_count
                        } else {
                            1
                        };
                        let command = Command::ReplaceChar(ch, count);
                        let result = self.execute_buffer_command(command);
                        if result && !self.dot_repeat.is_replaying() {
                            self.dot_repeat.record_single(command);
                        }
                    }
                }
                self.pending_count = 0;
            }
```

(only the `if !self.try_run_set_aware_replace_char(ch) { ... }` wrapper is new; the inner block is the existing body verbatim).

- [ ] **Step 5: Run the tests**

Run: `cargo test --lib editor::tests::set_aware_replace_char -- --nocapture 2>&1 | tail -40`
Expected: 1 passed.

- [ ] **Step 6: Full suite**

Run: `cargo test 2>&1 | tail -5`

---

### Task 17: `sd`/`sc`/`sg` via `apply_to_each_region`

**Files:**
- Modify: `src/editor/multi_region.rs`
- Modify: `src/editor/pending_grammar.rs` (`DeleteSurround`, `ChangeSurroundTo`, `AddSurroundChar` arms)
- Test: `src/editor/tests.rs`

**Interfaces:**
- Consumes: `apply_to_each_region` (Task 11), `Editor::execute_buffer_command` (existing — `sd`/`sc` reuse it unchanged, just repositioning the cursor to each region first), `text_objects::surround_strings` (existing, confirmed at `src/text_objects/mod.rs:1227`).
- Produces: `Editor::try_run_set_aware_delete_surround(&mut self, ch: char, count: usize) -> bool`, `try_run_set_aware_change_surround(&mut self, from: char, to: char, count: usize) -> bool`, `try_run_set_aware_add_surround(&mut self, ch: char, delim_count: usize) -> bool`. The first two reposition the cursor to each region before delegating to the *existing*, untouched `Command::DeleteSurround`/`ChangeSurround` executor logic (no new resolution code, per design.md S5.5) — `sg` is different: the region supplies the range directly (no motion needed), so it reimplements `Command::AddSurround`'s insert-close-then-insert-open steps directly against each region's `buffer_span` instead of going through `compute_motion_range`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn set_aware_sd_strips_surrounding_parens_from_every_region() {
    use crate::action::{Action, EditorAction};
    use crate::key::Key;
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "(a) (b)");
    // Cursor needs to land *inside* each pair for resolve_surround_pair to
    // find it -- bank a single-char region at each inner position.
    editor.active_document().selection_set.bank(Region::new(1, 1, RangeKind::Charwise));
    editor.active_document().selection_set.bank(Region::new(5, 5, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('d'));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('('));

    assert_eq!(editor.active_document().buffer.to_string(), "a b");
    assert!(editor.active_document().selection_set.is_empty());
}

#[test]
fn set_aware_sg_wraps_each_region_independently() {
    use crate::action::{Action, EditorAction};
    use crate::action::Motion;
    use crate::key::Key;
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "foo\n\nfoofoo\n");
    editor.active_document().selection_set.bank(Region::new(0, 2, RangeKind::Charwise));
    editor.active_document().selection_set.bank(Region::new(6, 8, RangeKind::Charwise));
    editor.active_document().selection_set.bank(Region::new(9, 11, RangeKind::Charwise));

    // Real key path: `s` -> SurroundStart -> SurroundVerb{count}; advancing
    // with 'g' sets pending_operator=Yank + pending_surround_add + enters
    // OperatorPending (src/editor/pending_grammar.rs:90-111, verified during
    // self-review) -- it does NOT itself produce AddSurroundChar. Reaching
    // AddSurroundChar normally requires a real motion key next, which
    // `execute_operator`'s pending_surround_add check intercepts. Since
    // `try_run_set_aware_add_surround` ignores `motion`/`count` entirely
    // (the region supplies the range), constructing the grammar state
    // directly is correct and avoids needing a real motion -- the same
    // shortcut the codebase's own existing surround tests already use
    // (e.g. `surround_add_wraps_inner_word_with_padding` manually assigns
    // `editor.pending_grammar` rather than typing a full key sequence).
    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('g'));
    editor.pending_grammar = Some(pending_grammar::PendingGrammar::AddSurroundChar {
        motion: Motion::NextWord, // ignored by the set-aware path
        count: 1,
        delim_count: 1,
    });
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, Key::Char('"'));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "\"foo\"\n\n\"foo\"\"foo\"\n"
    );
}
```

- [ ] **Step 2: Confirm failure**

Run: `cargo test --lib editor::tests::set_aware_sd editor::tests::set_aware_sg -- --nocapture 2>&1 | tail -60`

- [ ] **Step 3: Implement the three drivers**

Add to `src/editor/multi_region.rs`:

```rust
    /// `sd<ch>` against a non-empty `SelectionSet`: reuse the existing
    /// single-cursor `Command::DeleteSurround` resolution once per region,
    /// just repositioning the cursor first (no new resolution logic).
    pub(super) fn try_run_set_aware_delete_surround(&mut self, ch: char, count: usize) -> bool {
        let is_empty = self
            .document_manager
            .active_document()
            .map(|d| d.selection_set.is_empty())
            .unwrap_or(true);
        if is_empty {
            return false;
        }
        self.apply_to_each_region(|editor, region| {
            let Some(doc) = editor.document_manager.active_document_mut() else {
                return false;
            };
            let (start, _) = region.buffer_span(&doc.buffer);
            let _ = doc.buffer.set_cursor(start);
            editor.execute_buffer_command(crate::command::Command::DeleteSurround(ch, count))
        })
    }

    /// `sc<from><to>` against a non-empty `SelectionSet`: same pattern as
    /// `try_run_set_aware_delete_surround`.
    pub(super) fn try_run_set_aware_change_surround(&mut self, from: char, to: char, count: usize) -> bool {
        let is_empty = self
            .document_manager
            .active_document()
            .map(|d| d.selection_set.is_empty())
            .unwrap_or(true);
        if is_empty {
            return false;
        }
        self.apply_to_each_region(|editor, region| {
            let Some(doc) = editor.document_manager.active_document_mut() else {
                return false;
            };
            let (start, _) = region.buffer_span(&doc.buffer);
            let _ = doc.buffer.set_cursor(start);
            editor.execute_buffer_command(crate::command::Command::ChangeSurround(from, to, count))
        })
    }

    /// `sg<ch>` against a non-empty `SelectionSet`: the region itself
    /// supplies the range (no motion to resolve), so this mirrors
    /// `Command::AddSurround`'s executor body directly instead of routing
    /// through `compute_motion_range`.
    pub(super) fn try_run_set_aware_add_surround(&mut self, ch: char, delim_count: usize) -> bool {
        let is_empty = self
            .document_manager
            .active_document()
            .map(|d| d.selection_set.is_empty())
            .unwrap_or(true);
        if is_empty {
            return false;
        }
        self.apply_to_each_region(|editor, region| {
            let Some((open, close)) = crate::text_objects::surround_strings(ch, delim_count) else {
                return false;
            };
            let Some(doc) = editor.document_manager.active_document_mut() else {
                return false;
            };
            let (start, end) = region.buffer_span(&doc.buffer);
            let _ = doc.buffer.set_cursor(end);
            let _ = doc.insert_str(&close);
            let _ = doc.buffer.set_cursor(start);
            let _ = doc.insert_str(&open);
            true
        })
    }
```

- [ ] **Step 4: Wire the three call sites**

In `src/editor/pending_grammar.rs`, guard each of the three arms exactly like Task 16 guarded `ReplaceChar` — `if !self.try_run_set_aware_*(...) { /* existing body unchanged */ }`. The exact existing bodies for `DeleteSurround`/`ChangeSurroundTo` were shown in full earlier in this codebase exploration (`src/editor/pending_grammar.rs`, the `PendingGrammar::DeleteSurround { count }` and `PendingGrammar::ChangeSurroundTo { from, count }` arms) — wrap each with the same pattern as Task 16:

```rust
            PendingGrammar::DeleteSurround { count } => {
                self.set_mode(Mode::Normal);
                if let Key::Char(ch) = key {
                    if !self.try_run_set_aware_delete_surround(ch, count) {
                        let command = Command::DeleteSurround(ch, count);
                        let result = self.execute_buffer_command(command);
                        if result && !self.dot_repeat.is_replaying() {
                            self.dot_repeat.record_single(command);
                        }
                    }
                }
                self.pending_count = 0;
            }
```

```rust
            PendingGrammar::ChangeSurroundTo { from, count } => {
                self.set_mode(Mode::Normal);
                if let Key::Char(to) = key {
                    if !self.try_run_set_aware_change_surround(from, to, count) {
                        let command = Command::ChangeSurround(from, to, count);
                        let result = self.execute_buffer_command(command);
                        if result && !self.dot_repeat.is_replaying() {
                            self.dot_repeat.record_single(command);
                        }
                    }
                }
                self.pending_count = 0;
            }
```

For `AddSurroundChar { motion, count, delim_count }`: find that arm (it builds `Command::AddSurround(motion, count, ch, delim_count)` from the typed delimiter char). Wrap it the same way, but note `try_run_set_aware_add_surround` only needs `ch`/`delim_count` (it never uses `motion` or `count` — the region supplies the range):

```rust
            PendingGrammar::AddSurroundChar { motion, count, delim_count } => {
                self.set_mode(Mode::Normal);
                if let Key::Char(ch) = key {
                    if !self.try_run_set_aware_add_surround(ch, delim_count) {
                        let command = Command::AddSurround(motion, count, ch, delim_count);
                        let result = self.execute_buffer_command(command);
                        if result && !self.dot_repeat.is_replaying() {
                            self.dot_repeat.record_single(command);
                        }
                    }
                }
                self.pending_count = 0;
            }
```

Verified directly against `src/editor/pending_grammar.rs:142-156` during this plan's self-review: the field names are exactly `motion`/`count`/`delim_count`, as written above.

- [ ] **Step 5: Run the tests**

Run: `cargo test --lib editor::tests::set_aware_sd editor::tests::set_aware_sg -- --nocapture 2>&1 | tail -80`
Expected: 2 passed (after fixing the `sg` test's setup per Step 1's caveat).

- [ ] **Step 6: Full suite**

Run: `cargo test 2>&1 | tail -5`

---

### Task 18: `:s` substitute scoped to region text

**Files:**
- Modify: `src/command_line/commands/executor/mod.rs` (`ParsedCommand::Substitute` arm, `~line 273`)
- Test: `src/command_line/commands/executor/tests.rs` (check this file exists first — `ls src/command_line/commands/executor/`; if substitute tests live elsewhere, e.g. inline `#[cfg(test)]`, add there instead)

**Interfaces:**
- Consumes: `Region::buffer_span` (Task 3). The existing per-match substitution loop (delete + insert, sorted reverse by start) is **completely unchanged** — only the match-filtering step gains a third mode.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn substitute_with_nonempty_set_scopes_to_banked_regions_only() {
    use crate::command_line::commands::executor::{execute, ExecutionResult};
    use crate::command_line::commands::ParsedCommand;
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut doc = Document::new(1).unwrap();
    doc.buffer.insert_str("foo bar foo baz foo").unwrap();
    // Bank only the first and third "foo" (0..2, 17..19); the middle one
    // (8..10) is deliberately left out of the set.
    doc.selection_set.bank(Region::new(0, 2, RangeKind::Charwise));
    doc.selection_set.bank(Region::new(17, 19, RangeKind::Charwise));

    let mut state = crate::state::State::default(); // check the real zero-arg constructor name first
    let settings = crate::command_line::settings::SettingsRegistry::default();
    let doc_settings = crate::command_line::settings::SettingsRegistry::default();

    let result = execute(
        ParsedCommand::Substitute {
            pattern: "foo".to_string(),
            replacement: "XXX".to_string(),
            flags: "g".to_string(),
            range: Some("%".to_string()),
            bangs: 0,
        },
        &mut state,
        &mut doc,
        &settings,
        &doc_settings,
    );

    assert_eq!(result, ExecutionResult::Success);
    assert_eq!(doc.buffer.to_string(), "XXX bar foo baz XXX", "middle 'foo' (not banked) must survive");
    assert!(doc.selection_set.is_empty());
}
```

**Before trusting the test scaffolding above** (`State::default()`, `SettingsRegistry::default()`, `ExecutionResult: PartialEq+Debug` for the assert, `ParsedCommand::Substitute`'s exact field names/types — `range: Option<String>` vs `Option<&str>`, `bangs: usize` vs something else), run:

Run: `grep -n "struct ParsedCommand\|Substitute {" src/command_line/commands/mod.rs` and `grep -n "impl Default for State\|fn default" src/state/mod.rs`

and fix any mismatches against what those actually look like — this executor module likely already has its own test helpers (a `make_test_document()`/similar) given how much setup `execute()` needs; search `src/command_line/commands/executor/` for an existing test in the same file first and copy *its* setup pattern instead of hand-rolling a new one.

- [ ] **Step 2: Confirm failure**

Run: `cargo test --lib substitute_with_nonempty_set -- --nocapture 2>&1 | tail -40`

- [ ] **Step 3: Implement**

In `src/command_line/commands/executor/mod.rs`, inside the `ParsedCommand::Substitute` arm, the existing filtering block reads:

```rust
                        let is_global_subst = flags.contains('g');
                        let whole_file = range.as_deref() == Some("%");

                        // Filtering matches
                        if !whole_file {
                            // Filter matches that intersect with current line
                            let current_line_idx = document
                                .buffer
                                .line_index
                                .get_line_at(document.buffer.cursor());
                            let start_byte = document.buffer.line_start(current_line_idx);
                            let end_byte = document
                                .buffer
                                .line_index
                                .get_end(current_line_idx, document.buffer.len())
                                .unwrap_or(document.buffer.len());

                            matches
                                .retain(|m| m.range.start >= start_byte && m.range.end <= end_byte);
                        }
```

Change it to check the `SelectionSet` first, falling back to the existing whole-file/current-line logic only when the set is empty:

```rust
                        let is_global_subst = flags.contains('g');
                        let whole_file = range.as_deref() == Some("%");
                        let has_selection = !document.selection_set.is_empty();

                        // Filtering matches: a non-empty SelectionSet takes
                        // priority over %/current-line -- each banked region
                        // is its own miniature substitution scope (S5.4).
                        if has_selection {
                            let region_spans: Vec<(usize, usize)> = document
                                .selection_set
                                .sorted()
                                .iter()
                                .map(|r| r.buffer_span(&document.buffer))
                                .collect();
                            matches.retain(|m| {
                                region_spans
                                    .iter()
                                    .any(|(s, e)| m.range.start >= *s && m.range.end <= *e)
                            });
                        } else if !whole_file {
                            // Filter matches that intersect with current line
                            let current_line_idx = document
                                .buffer
                                .line_index
                                .get_line_at(document.buffer.cursor());
                            let start_byte = document.buffer.line_start(current_line_idx);
                            let end_byte = document
                                .buffer
                                .line_index
                                .get_end(current_line_idx, document.buffer.len())
                                .unwrap_or(document.buffer.len());

                            matches
                                .retain(|m| m.range.start >= start_byte && m.range.end <= end_byte);
                        }
```

A pattern with no match inside a given region simply means `matches.retain` drops it — no special "no match in this region" branch needed, matching the existing per-line behavior's "no error, just no-op for that scope" semantics (design.md S5.4).

Then, after the existing per-match substitution loop finishes (find the closing of that `for m in valid_matches { ... }` loop and whatever follows it — likely a `document.commit_transaction();` and `return ExecutionResult::Success` or similar), add the clearing step design.md S5.0 requires of every set-aware command:

```rust
                        document.commit_transaction();
                        if has_selection {
                            document.selection_set.clear();
                        }
                        // ... whatever the existing trailing return/state-update was ...
```

Locate the *exact* existing trailing lines first (`grep -n "commit_transaction" src/command_line/commands/executor/mod.rs` to find the Substitute arm's specific occurrence among the file's several `commit_transaction` calls) and insert `if has_selection { document.selection_set.clear(); }` immediately after it, before whatever existing code follows.

- [ ] **Step 4: Run the tests**

Run: `cargo test --lib substitute_with_nonempty_set -- --nocapture 2>&1 | tail -60`
Expected: 1 passed.

- [ ] **Step 5: Full suite**

Run: `cargo test 2>&1 | tail -5`

---

## Phase 7 — Paste + undo granularity

### Task 19: `p`/`P`/`PutSystemClipboard` via `apply_to_each_region`

**Files:**
- Modify: `src/editor/multi_region.rs`
- Modify: `src/editor/handle_action.rs` (`Put`, `PutSystemClipboard` arms)
- Test: `src/editor/tests.rs`

**Interfaces:**
- Consumes: `apply_to_each_region` (Task 11), `ClipboardRing::most_recent` (existing).
- Produces: `Editor::try_run_set_aware_put(&mut self, before: bool, text: &str) -> bool` — inserts the **same** fixed `text` at every region (`p`: after each region's end; `P`: before each region's start). Non-destructive: no deletion, unlike `d`/`c`. Both `Put` and `PutSystemClipboard` route through this with their own source text.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn set_aware_put_inserts_same_text_at_every_region() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.clipboard_ring.push("X".to_string());
    editor.active_document().selection_set.bank(Region::new(0, 0, RangeKind::Charwise));
    editor.active_document().selection_set.bank(Region::new(5, 5, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::Put { before: false }));

    assert_eq!(editor.active_document().buffer.to_string(), "0X1234X56789", "p inserts after each region");
    assert!(editor.active_document().selection_set.is_empty());
}

#[test]
fn set_aware_put_before_inserts_ahead_of_every_region() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.clipboard_ring.push("X".to_string());
    editor.active_document().selection_set.bank(Region::new(0, 0, RangeKind::Charwise));
    editor.active_document().selection_set.bank(Region::new(5, 5, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::Put { before: true }));

    assert_eq!(editor.active_document().buffer.to_string(), "X01234X56789", "P inserts before each region");
}

#[test]
fn bare_repeated_p_after_set_aware_put_only_affects_single_cursor() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.clipboard_ring.push("X".to_string());
    editor.active_document().selection_set.bank(Region::new(0, 0, RangeKind::Charwise));
    editor.active_document().selection_set.bank(Region::new(5, 5, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::Put { before: false })); // multi-region put, set clears
    editor.handle_action(&Action::Editor(EditorAction::Put { before: false })); // bare repeat: single cursor only

    // First put: "0X1234X56789", cursor lands somewhere in there after the
    // single-cursor `insert_text_at_cursor` fallback path positions it for
    // the *second* (bare, set-already-empty) call -- assert only that the
    // second X appears exactly once more, not duplicated at both original
    // anchors (which would mean the set wasn't actually cleared).
    let count_of_x = editor.active_document().buffer.to_string().matches('X').count();
    assert_eq!(count_of_x, 3, "first put = 2 X's (one per region), second bare put = 1 more, not 2 more");
}
```

- [ ] **Step 2: Confirm failure**

Run: `cargo test --lib editor::tests::set_aware_put editor::tests::bare_repeated_p -- --nocapture 2>&1 | tail -60`

- [ ] **Step 3: Implement**

Add to `src/editor/multi_region.rs`:

```rust
    /// `p`/`P` (and `PutSystemClipboard`) against a non-empty `SelectionSet`:
    /// insert the same `text` at every region -- after its end for `p`,
    /// before its start for `P`. Non-destructive (design.md S5.7).
    pub(super) fn try_run_set_aware_put(&mut self, before: bool, text: &str) -> bool {
        let is_empty = self
            .document_manager
            .active_document()
            .map(|d| d.selection_set.is_empty())
            .unwrap_or(true);
        if is_empty {
            return false;
        }
        let text = text.to_string();
        self.apply_to_each_region(|editor, region| {
            let Some(doc) = editor.document_manager.active_document_mut() else {
                return false;
            };
            let (start, end) = region.buffer_span(&doc.buffer);
            let pos = if before { start } else { end };
            let _ = doc.buffer.set_cursor(pos);
            doc.insert_str(&text).is_ok()
        })
    }
```

- [ ] **Step 4: Wire `Put` and `PutSystemClipboard`**

In `src/editor/handle_action.rs`, the existing `Put` arm:

```rust
            EditorAction::Put { before } => {
                if let Some(text) = self.clipboard_ring.most_recent().map(|s| s.to_owned()) {
                    if self.try_run_set_aware_put(*before, &text) {
                        return true;
                    }
                    let original_cursor = self
                        .document_manager
                        .active_document()
                        .map(|d| d.buffer.cursor())
                        .unwrap_or(0);
                    let result = self.insert_text_at_cursor(&text, *before);
                    if result {
                        self.post_paste_state = Some(PostPasteState {
                            ring_index: 0,
                            before: *before,
                            original_cursor,
                        });
                    }
                    result
```

(only the `if self.try_run_set_aware_put(...) { return true; }` line is new; the rest of the arm — and the closing brace/match-arm-end below it — is unchanged).

`PutSystemClipboard` gets the identical one-line guard:

```rust
            EditorAction::PutSystemClipboard { before } => {
                let text = arboard::Clipboard::new()
                    .ok()
                    .and_then(|mut cb| cb.get_text().ok());
                if let Some(text) = text {
                    if self.try_run_set_aware_put(*before, &text) {
                        return true;
                    }
                    self.insert_text_at_cursor(&text, *before)
                } else {
                    false
                }
            }
```

- [ ] **Step 5: Run the tests**

Run: `cargo test --lib editor::tests::set_aware_put editor::tests::bare_repeated_p -- --nocapture 2>&1 | tail -80`
Expected: 3 passed.

- [ ] **Step 6: Full suite**

Run: `cargo test 2>&1 | tail -5`

---

### Task 20: Single-undo-step verification for the insert/paste paths

**Files:**
- Test: `src/editor/tests.rs`

**Interfaces:**
- Consumes: everything from Tasks 11-19. No new production code — `enter_multi_insert` (Task 12) already wraps its anchors in one `MultiInsert` transaction and `apply_to_each_region` (Task 11) already wraps its batch in one `MultiRegion` transaction; Task 11's own test already proved this for delete. This task closes the gap for the *other* two transaction-wrapping call sites design.md S9 explicitly calls out: multi-insert (`i`/`c`/etc.) and paste.

- [ ] **Step 1: Write the tests**

```rust
#[test]
fn multi_insert_is_one_undo_step() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.active_document().selection_set.bank(Region::new(0, 0, RangeKind::Charwise));
    editor.active_document().selection_set.bank(Region::new(5, 5, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::EnterInsertMode));
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    assert_eq!(editor.active_document().buffer.to_string(), "X01234X56789");

    assert!(editor.active_document().undo());
    assert_eq!(
        editor.active_document().buffer.to_string(),
        "0123456789",
        "a single undo must remove both inserted X's at once"
    );
}

#[test]
fn multi_region_put_is_one_undo_step() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.clipboard_ring.push("X".to_string());
    editor.active_document().selection_set.bank(Region::new(0, 0, RangeKind::Charwise));
    editor.active_document().selection_set.bank(Region::new(5, 5, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::Put { before: false }));
    assert_eq!(editor.active_document().buffer.to_string(), "0X1234X56789");

    assert!(editor.active_document().undo());
    assert_eq!(editor.active_document().buffer.to_string(), "0123456789");
}
```

- [ ] **Step 2: Run**

Run: `cargo test --lib editor::tests::multi_insert_is_one_undo_step editor::tests::multi_region_put_is_one_undo_step -- --nocapture 2>&1 | tail -40`
Expected: 2 passed.

- [ ] **Step 3: Full suite**

Run: `cargo test 2>&1 | tail -5`

---

## Phase 8 — Dot-repeat for multi-region operations

**How this works (read before writing code):** `.` doesn't replay the *operator* for the destructive group (`d`/`c`/`y`/`sd`/`sc`) — it replays the *selection-building actions* (the `v`/motion/`Esc` and `m`/`M` keystrokes that constructed the set), reconstructing an equivalent `SelectionSet` relative to wherever the cursor is *now*, then either stops there (destructive group, so the user can review before re-running the operator by hand) or also re-runs the triggering action (non-destructive group: `i`/`a`/`I`/`A`/`o`/`O`/`r`/`sg`/`p`/`P`).

The key design insight that makes "relative to wherever the cursor is now" free: **record `Action` values, not positions.** Replaying `EditorAction::Move(Motion::Right)` through the normal action dispatch naturally moves right from *whatever* the cursor currently is — no coordinate math needed. This is exactly how the existing `DotRegister::InsertSession` replay already works (`src/editor/operators.rs:201-217`, read in full earlier in this session) — Task 21/22 add a sibling register variant, not a new mechanism.

### Task 21: Recording — `region_build_recording` + `DotRegister::RegionBuildSession`

**Files:**
- Modify: `src/editor/mod.rs` (new field), `src/editor/init.rs` (initialize it)
- Modify: `src/dot_repeat/mod.rs` (new `DotRegister` variant + recorder method)
- Modify: `src/editor/handle_action.rs` (the generic recording hook, at the top of `handle_action`; and clearing the recording when the set is abandoned in `EnterNormalMode`'s "clear" branch from Task 6)
- Test: `src/editor/tests.rs`

**Interfaces:**
- Produces: `Editor.region_build_recording: Vec<Action>` (`pub(super)`). `DotRegister::RegionBuildSession { actions: Vec<Action>, follow_up: Option<Action> }`. `DotRepeat::record_region_build_session(&mut self, actions: Vec<Action>, follow_up: Option<Action>)`.
- Consumes: `Mode::is_visual` (Task 2).

- [ ] **Step 1: Write the failing test**

This test exercises the recording hook in isolation — it does **not** yet test the finalize/replay path (Task 22), only that the right actions accumulate while building a set:

```rust
#[test]
fn region_build_actions_accumulate_while_visual_and_during_bank_occurrence() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "foo bar foo");
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::Move(crate::action::Motion::Right)));
    editor.handle_action(&Action::Editor(EditorAction::Move(crate::action::Motion::Right)));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    editor.handle_action(&Action::Editor(EditorAction::RegionBankOccurrenceNext));

    assert_eq!(
        editor.region_build_recording,
        vec![
            Action::Editor(EditorAction::EnterVisualChar),
            Action::Editor(EditorAction::Move(crate::action::Motion::Right)),
            Action::Editor(EditorAction::Move(crate::action::Motion::Right)),
            Action::Editor(EditorAction::EnterNormalMode),
            Action::Editor(EditorAction::RegionBankOccurrenceNext),
        ]
    );
}

#[test]
fn region_build_recording_does_not_capture_plain_normal_mode_navigation() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "line one\nline two\nline three");
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    // Plain navigation between bank operations -- per design.md S5.9, "."
    // rebuilds relative to wherever the cursor *now* is, so this plain
    // motion must NOT be part of the recorded sequence.
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Down)));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Down)));

    assert_eq!(
        editor.region_build_recording,
        vec![
            Action::Editor(EditorAction::EnterVisualChar),
            Action::Editor(EditorAction::EnterNormalMode),
        ],
        "plain Normal-mode Move actions must not be recorded"
    );
}
```

- [ ] **Step 2: Confirm compile failure**

Run: `cargo test --lib editor::tests::region_build_actions_accumulate editor::tests::region_build_recording_does_not_capture -- --nocapture 2>&1 | tail -30`
Expected: `no field 'region_build_recording'`.

- [ ] **Step 3: Add the `DotRegister` variant**

In `src/dot_repeat/mod.rs`:

```rust
pub(crate) enum DotRegister {
    Single(Command),
    InsertSession {
        entry: Command,
        commands: Vec<Command>,
    },
    /// The selection-building actions (`v`/motion/`Esc`, `m`/`M`) that
    /// constructed a `SelectionSet`, plus what to do once it's rebuilt:
    /// `Some(action)` re-runs that action (non-destructive group); `None`
    /// stops with the set banked for manual review (destructive group).
    RegionBuildSession {
        actions: Vec<crate::action::Action>,
        follow_up: Option<crate::action::Action>,
    },
}
```

And a recorder method on `DotRepeat`, next to `record_single`:

```rust
    /// Store a region-build session: the actions that constructed a
    /// `SelectionSet`, plus what to do once `.` rebuilds it.
    pub fn record_region_build_session(
        &mut self,
        actions: Vec<crate::action::Action>,
        follow_up: Option<crate::action::Action>,
    ) {
        self.register = Some(DotRegister::RegionBuildSession { actions, follow_up });
    }
```

(`DotRegister` needs `Action: Clone, Debug` for its own derive to keep working — `Action` already derives both, confirmed in `src/action/mod.rs`.)

- [ ] **Step 4: Add the field**

In `src/editor/mod.rs`, right after `pending_multi_insert_anchors: Vec<usize>,` (Task 12):

```rust
    /// Selection-building actions accumulated since the last time a
    /// set-aware command consumed the set (Task 22 finalizes this into
    /// `DotRegister::RegionBuildSession`).
    pub(super) region_build_recording: Vec<crate::action::Action>,
```

In `src/editor/init.rs`, right after `pending_multi_insert_anchors: Vec::new(),`:

```rust
            region_build_recording: Vec::new(),
```

- [ ] **Step 5: Add the recording hook**

In `src/editor/handle_action.rs`, the very top of `handle_action` already has:

```rust
        // Clear post-paste cycling state on any action except CyclePaste itself.
        if !matches!(editor_action, EditorAction::CyclePaste { .. }) {
            self.post_paste_state = None;
        }
```

Add the recording hook immediately after it, still before the big `match editor_action`:

```rust
        // Accumulate selection-building actions for dot-repeat (S5.9). A
        // plain Normal-mode Move is navigation, not selection-building, so
        // it's excluded unless Visual mode is already active.
        let is_region_building = matches!(
            editor_action,
            EditorAction::EnterVisualChar
                | EditorAction::EnterVisualLine
                | EditorAction::EnterVisualBlock
                | EditorAction::RegionBankOccurrenceNext
                | EditorAction::RegionBankOccurrencePrev
        ) || self.current_mode.is_visual();
        if is_region_building && !self.dot_repeat.is_replaying() {
            self.region_build_recording.push(action.clone());
        }
```

(`action` here is the original `&Action` parameter `handle_action` receives — *not* `editor_action`, which is the already-unwrapped `&EditorAction` — check the function signature at the top of the file to confirm the parameter name before wiring this in; this plan assumes it's named `action` based on the `match action { Action::Editor(act) => act, ... }` line already read earlier in this session).

- [ ] **Step 6: Clear the recording when the set is abandoned**

In the `EnterNormalMode` arm (Task 6 added the `else { doc.selection_set.clear() }` branch for the "Escape in Normal mode with a non-empty set" case) — extend that same branch:

```rust
                } else if let Some(doc) = self.document_manager.active_document_mut() {
                    doc.selection_set.clear();
                    self.region_build_recording.clear();
                }
```

(only the new `self.region_build_recording.clear();` line is added to that existing arm).

- [ ] **Step 7: Run the tests**

Run: `cargo test --lib editor::tests::region_build_actions_accumulate editor::tests::region_build_recording_does_not_capture -- --nocapture 2>&1 | tail -50`
Expected: 2 passed.

- [ ] **Step 8: Full suite**

Run: `cargo test 2>&1 | tail -5`

---

### Task 22: Finalize on operator fire + `execute_dot_repeat` replay

**Files:**
- Modify: `src/editor/multi_region.rs` (`finish_region_build` helper)
- Modify: `src/editor/handle_action.rs`, `src/editor/pending_grammar.rs` (every set-aware call site from Tasks 13/15/16/17/19 gains one line)
- Modify: `src/editor/operators.rs` (`execute_dot_repeat`'s new `match` arm)
- Test: `src/editor/tests.rs`

**Interfaces:**
- Consumes: `DotRegister::RegionBuildSession`/`record_region_build_session` (Task 21).
- Produces: `Editor::finish_region_build(&mut self, follow_up: Option<crate::action::Action>)` — drains `region_build_recording` into a `RegionBuildSession` (no-op if nothing was recorded, e.g. the set existed for some other reason). Extends `execute_dot_repeat` with a `RegionBuildSession` arm.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn dot_repeat_destructive_group_reselects_without_reexecuting() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "(a) (b)");
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    editor.handle_action(&Action::Editor(EditorAction::Operator(OperatorType::Delete)));
    assert_eq!(editor.active_document().buffer.to_string(), " (b)");
    assert!(editor.active_document().selection_set.is_empty());

    editor.active_document().buffer.set_cursor(1).unwrap(); // land inside "(b)"
    editor.execute_dot_repeat();

    assert_eq!(
        editor.active_document().buffer.to_string(),
        " (b)",
        "destructive group: '.' must NOT re-delete"
    );
    assert_eq!(
        editor.active_document().selection_set.regions.len(),
        1,
        "but the equivalent region must be rebanked for manual review"
    );
}

#[test]
fn dot_repeat_non_destructive_group_rebuilds_and_reexecutes() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "aaa\nbbb");
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    editor.handle_action(&Action::Editor(EditorAction::EnterInsertModeAtLineStart));
    editor.handle_action(&Action::Editor(EditorAction::InsertChar('X')));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    assert_eq!(editor.active_document().buffer.to_string(), "Xaaa\nbbb");

    let bbb_offset = editor.active_document().buffer.to_string().find('b').unwrap();
    editor.active_document().buffer.set_cursor(bbb_offset).unwrap();
    editor.execute_dot_repeat();

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "Xaaa\nXbbb",
        "non-destructive group: '.' rebuilds AND re-runs the insert"
    );
}

#[test]
fn dot_repeat_sg_fully_replays_using_addsurroundtoset() {
    use crate::action::{Action, EditorAction, Motion};

    let mut editor = create_editor();
    load_text(&mut editor, "foo\nbar");
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode)); // banks "foo" (0..2)

    editor.handle_action(&Action::Editor(EditorAction::SurroundStart));
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, crate::key::Key::Char('g'));
    editor.pending_grammar = Some(pending_grammar::PendingGrammar::AddSurroundChar {
        motion: Motion::NextWord, // ignored by the set-aware path
        count: 1,
        delim_count: 1,
    });
    let grammar = editor.pending_grammar.take().unwrap();
    editor.advance_pending_grammar(grammar, crate::key::Key::Char('"'));

    assert_eq!(editor.active_document().buffer.to_string(), "\"foo\"\nbar");

    let bar_offset = editor.active_document().buffer.to_string().find("bar").unwrap();
    editor.active_document().buffer.set_cursor(bar_offset).unwrap();
    editor.execute_dot_repeat();

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "\"foo\"\n\"bar\"",
        "'.' rebuilds the equivalent region at the new cursor AND re-wraps it -- sg fully replays, unlike d/c/y/sd/sc"
    );
}
```

The first test's setup deliberately uses a single-region Visual selection (not a `m`/`M`-grown set) to keep the hand-derivation simple: select `"(a"` is two chars from offset 0 — wait, re-derive precisely: cursor starts at 0 (`'('`), two `Right` moves land the cursor at offset 2 (`'a'`), so the Visual region spans `0..2` inclusive = `"(a"`. Deleting that leaves `") (b)"` minus... **re-check this against the actual buffer**: `"(a) (b)"`, deleting offset 0..2 (`"(a"`) leaves `") (b)"`, not `" (b)"` as written above. **Fix the test's expected string to `") (b)"`** in both assertions before running it — this is exactly the kind of hand-derivation slip the "watch it fail for the right reason" step exists to catch.

- [ ] **Step 2: Confirm failure**

Run: `cargo test --lib editor::tests::dot_repeat_destructive editor::tests::dot_repeat_non_destructive editor::tests::dot_repeat_sg_fully_replays -- --nocapture 2>&1 | tail -60`

- [ ] **Step 3: Implement `finish_region_build`**

Add to `src/editor/multi_region.rs`:

```rust
    /// Finalize the accumulated selection-building actions into a
    /// `DotRegister::RegionBuildSession`, if anything was recorded.
    pub(super) fn finish_region_build(&mut self, follow_up: Option<crate::action::Action>) {
        if self.region_build_recording.is_empty() {
            return;
        }
        let actions = std::mem::take(&mut self.region_build_recording);
        if !self.dot_repeat.is_replaying() {
            self.dot_repeat.record_region_build_session(actions, follow_up);
        }
    }
```

- [ ] **Step 4: Call it from every set-aware call site**

Each call site below was written in an earlier task; this step adds exactly one line after the existing successful-batch check. **Destructive group (`d`/`y`/`c`/`sd`/`sc`) always passes `None`; non-destructive group (`i`/`a`/`I`/`A`/`o`/`O`/`r`/`p`/`P`/`sg`) passes `Some(action.clone())`** (design.md S5.9's exact grouping — note `y` is grouped with the destructive set here despite not mutating text; follow the design doc's grouping literally, not "does it mutate"). **`sg` is special only in *which* action it passes** — not a plain `action.clone()` of whatever fired, but a dedicated `EditorAction::AddSurroundToSet { ch, delim_count }` (see its own call site below) — because the region itself supplies the range, so re-running the wrap needs no motion to replay.

In `src/editor/handle_action.rs`'s `EditorAction::Operator` arm (Task 13):

```rust
                if self.try_run_set_aware_operator(*op) {
                    self.finish_region_build(None);
                    return true;
                }
```

In the same file's six insert-family arms (Task 15) — shown for `EnterInsertMode`, identical pattern for the other five:

```rust
            EditorAction::EnterInsertMode => {
                if self.try_multi_insert_for_command(crate::command::Command::EnterInsertMode) {
                    self.finish_region_build(Some(action.clone()));
                    return true;
                }
                self.handle_mode_management(crate::command::Command::EnterInsertMode);
                true
            }
```

In `Put`/`PutSystemClipboard` (Task 19):

```rust
                    if self.try_run_set_aware_put(*before, &text) {
                        self.finish_region_build(Some(action.clone()));
                        return true;
                    }
```

In `src/editor/pending_grammar.rs`'s `ReplaceChar` arm (Task 16):

```rust
                    if !self.try_run_set_aware_replace_char(ch) {
                        /* existing single-cursor body */
                    } else {
                        self.finish_region_build(Some(crate::action::Action::Editor(
                            crate::action::EditorAction::ReplaceCharPending,
                        )));
                    }
```

`DeleteSurround`/`ChangeSurroundTo` (Task 17, destructive — `None`):

```rust
                    if !self.try_run_set_aware_delete_surround(ch, count) {
                        /* existing body */
                    } else {
                        self.finish_region_build(None);
                    }
```

(same `else { self.finish_region_build(None); }` pattern for `ChangeSurroundTo`).

`AddSurroundChar` (Task 17, non-destructive — `sg` *can* fully replay, resolved as follows): once a `SelectionSet` is non-empty, `sg`'s "motion" effectively *is* the set itself — no real motion gets typed in the set-aware path (`try_run_set_aware_add_surround` already ignores `motion`/`count` entirely, Task 17 Step 3). So the thing dot-repeat needs to re-run isn't "replay the `s`→`g`→motion→delimiter keystrokes" (which, yes, isn't expressible as one `Action`) — it's just "re-wrap the rebuilt set's regions with the same `ch`/`delim_count`," which is exactly what `try_run_set_aware_add_surround(ch, delim_count)` already does. So add one new action that carries those two plain values:

In `src/action/mod.rs`, next to `RegionBankOccurrencePrev`:

```rust
    /// Dot-repeat follow-up for `sg` against a `SelectionSet`: re-wrap the
    /// (rebuilt) set's regions with `ch`, repeated `delim_count` times.
    /// Not reachable from the keymap directly -- only `finish_region_build`
    /// constructs this.
    AddSurroundToSet { ch: char, delim_count: usize },
```

In `src/editor/handle_action.rs`:

```rust
            EditorAction::AddSurroundToSet { ch, delim_count } => {
                self.try_run_set_aware_add_surround(*ch, *delim_count)
            }
```

And the `AddSurroundChar` wiring becomes a true full-replay follow-up, not a `None` fallback:

```rust
                    if !self.try_run_set_aware_add_surround(ch, delim_count) {
                        /* existing body */
                    } else {
                        self.finish_region_build(Some(crate::action::Action::Editor(
                            crate::action::EditorAction::AddSurroundToSet { ch, delim_count },
                        )));
                    }
```

- [ ] **Step 5: Add the `execute_dot_repeat` replay arm**

In `src/editor/operators.rs`, inside `execute_dot_repeat`'s `match register { ... }` (the function read in full at the top of this Phase), add:

```rust
            DotRegister::RegionBuildSession { actions, follow_up } => {
                // Rebuild relative to wherever the cursor is *now* by
                // replaying the recorded Actions (not absolute positions) --
                // the count prefix doesn't apply here (rebuilding the set
                // twice would re-bank already-banked regions).
                for action in &actions {
                    self.handle_action(action);
                }
                if let Some(follow_up) = &follow_up {
                    self.handle_action(follow_up);
                }
            }
```

(placed as a sibling arm to `DotRegister::Single` and `DotRegister::InsertSession`, inside the existing `self.dot_repeat.set_replaying(true); ... self.dot_repeat.set_replaying(false);` bracket so nested `handle_action` calls don't re-record or re-finalize).

- [ ] **Step 6: Run the tests**

Run: `cargo test --lib editor::tests::dot_repeat_destructive editor::tests::dot_repeat_non_destructive editor::tests::dot_repeat_sg_fully_replays -- --nocapture 2>&1 | tail -80`
Expected: 3 passed (after fixing the hand-derivation slip flagged in Step 1).

- [ ] **Step 7: Full suite**

Run: `cargo test 2>&1 | tail -5`

---

## Phase 9 — Expand/shrink region

**Verified against `src/text_objects/mod.rs:131-199` (`resolve`'s dispatch) before writing this phase:** `nesting` (via `compose_nesting`) only affects bracket/quote kinds (`Paren`/`CurlyBrace`/`SquareBracket`/`AngleBracket`/`AnyBracket`/quotes) — `resolve_paragraph`/`resolve_sentence`/`resolve_line`/`resolve_buffer` all take `repeat` (the `count` argument) instead and ignore `spec.nesting` entirely. So "grow the selection" is **not** "increase nesting on one fixed kind" — it's "try several candidate kinds (bracket kinds at increasing nesting, everything else at nesting 1) and pick whichever resolves to the smallest range that's strictly larger than what's currently selected." The candidate list itself was then **run against real fixtures** (a temporary scratch test calling `resolve()` directly, added to and removed from `src/text_objects/tests.rs` during this plan's self-review) rather than left as an untested guess — the exact verified offsets are recorded in Task 23 Step 1's table, and one real bug surfaced by that run (`resolve_line`'s off-by-one for a trailing-newline-less last line) is fixed in Task 23 Step 4.

### Task 23: Expand region (`<Space>`)

**Files:**
- Modify: `src/editor/multi_region.rs` (`expand_active_region`)
- Modify: `src/editor/mod.rs` (`expand_history: Vec<(usize, usize)>` field), `src/editor/init.rs`
- Modify: `src/action/mod.rs` (`EditorAction::ExpandRegion`)
- Modify: `src/editor/handle_action.rs`
- Modify: `src/keymap/defaults.rs` (`<Space>` under `KeyContext::Visual` only — verified `Char(' ')` has no bare-key binding under `Normal` at all, only multi-key sequences like `<Space>f`/`<Space>e`/`<Space>pd`, so this doesn't shadow plain cursor movement)
- Test: `src/editor/tests.rs`

**Interfaces:**
- Consumes: `text_objects::resolve` (existing), `Editor.visual_anchor` (Task 5).
- Produces: `Editor.expand_history: Vec<(usize, usize)>` (`pub(super)`, char-offset spans, most-recent last — the history stack Task 24's shrink pops from). `EditorAction::ExpandRegion` — grows the active Visual region outward to the next enclosing text object; pushes the *previous* extent onto `expand_history` first. No-op (and no push) if nothing strictly larger is found.

- [ ] **Step 1: Write the failing tests**

(The `<Space>` collision question was checked directly during this plan's self-review: `grep -n "Char(' ')" src/keymap/defaults.rs` finds five `register_sequence` uses — `[' ','p','d']`, `[' ','p','i']`, `[' ','f']`, `[' ','c','a']`, `[' ','r','n']`, `[' ','e']`, all under `KeyContext::Normal` — plus one bare single-key binding under `KeyContext::LocationList` only (`location_list:code_action`). None of these are `KeyContext::Visual`, so a bare `<Space>` registered there has zero collisions, confirmed, not assumed.)

The candidate behavior below was **verified empirically during this plan's self-review**, not guessed: a scratch test was added to `src/text_objects/tests.rs`, run against `cargo test`, and removed after confirming the real output (do not re-add it — the verified data is recorded here instead). For `"say \"hello world\" now"` with the cursor at offset 5 (the `h` of `hello`):

| Candidate | Resolved span (inclusive) | Text |
| :-- | :-- | :-- |
| `Word` around | `(5, 10)` | `"hello "` (6 chars, includes trailing space) |
| `DoubleQuote` around | `(4, 16)` | `"\"hello world\""` (13 chars) |
| `SingleQuote`/`Backtick`/`AnyBracket` | `None` | none present in the fixture |
| `Sentence` around | `(5, 20)` | starts at the *same* offset as `Word`, not before `DoubleQuote`'s start (4) — so it never qualifies as "contains the current selection" once `DoubleQuote` is active, and gets correctly skipped |
| `Paragraph`/`Buffer` around | `(0, 20)` | the whole buffer |

So pressing `<Space>` twice from inside `hello` goes `hello` → `hello ` (Word) → `"hello world"` (DoubleQuote) — `Sentence` is *never* reachable from here despite looking like it should sit between Quote and Paragraph in size, because its start (5) doesn't contain Quote's start (4). This is real, confirmed `resolve()` behavior, not an algorithm bug — the "smallest strictly-larger-and-containing" rule correctly skips a same-or-narrower-but-non-containing candidate.

**A second, separate finding from the same verification:** `resolve_line`'s `Around` modifier returns `new_cursor = buf.len()` for a last line with no trailing newline (`src/text_objects/mod.rs:1096-1122`, read in full) — one past the last valid char index. Combined with `inclusive: true`'s "+1" convention elsewhere, naively computing `e = range.anchor.max(range.new_cursor) + 1` overshoots `buf.len()` by one. `clipboard::capture_text` already guards exactly this case with `.min(buf.len())` (`src/clipboard/mod.rs:110`) — `expand_active_region` below does the same.

```rust
#[test]
fn expand_region_grows_word_then_quotes_in_verified_order() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "say \"hello world\" now");
    let pos = editor.active_document().buffer.to_string().find("hello").unwrap();
    editor.active_document().buffer.set_cursor(pos).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));

    editor.handle_action(&Action::Editor(EditorAction::ExpandRegion));
    let anchor = editor.visual_anchor.unwrap();
    let cursor = editor.active_document().buffer.cursor();
    assert_eq!((anchor, cursor), (5, 10), "first press: Word around -> \"hello \"");

    editor.handle_action(&Action::Editor(EditorAction::ExpandRegion));
    let anchor = editor.visual_anchor.unwrap();
    let cursor = editor.active_document().buffer.cursor();
    assert_eq!((anchor, cursor), (4, 16), "second press: DoubleQuote around -> the full quoted span");
}

#[test]
fn expand_region_noop_when_already_at_buffer_extent() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "x");
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));

    // Expand repeatedly until it stops growing (should terminate quickly on
    // a 1-char buffer); the *last* call must report false (handled) and
    // leave the anchor/cursor unchanged from the call before it.
    editor.handle_action(&Action::Editor(EditorAction::ExpandRegion));
    let before = (editor.visual_anchor, editor.active_document().buffer.cursor());
    editor.handle_action(&Action::Editor(EditorAction::ExpandRegion));
    let after = (editor.visual_anchor, editor.active_document().buffer.cursor());

    assert_eq!(before, after, "expanding past the whole buffer must be a no-op");
}
```

- [ ] **Step 2: Add `expand_history` and `EditorAction::ExpandRegion`**

In `src/editor/mod.rs`, next to `region_build_recording`:

```rust
    /// Stack of prior selection extents, for `<Shift-Space>` shrink (Task 24).
    /// Cleared whenever a fresh Visual region starts (Task 5/enter_visual_or_resume).
    pub(super) expand_history: Vec<(usize, usize)>,
```

In `src/editor/init.rs`: `expand_history: Vec::new(),`

In `src/action/mod.rs`:

```rust
    /// `<Space>` in Visual: grow the active region to the next enclosing
    /// text object.
    ExpandRegion,
```

- [ ] **Step 3: Clear `expand_history` on fresh entry**

In `src/editor/handle_action.rs`'s `enter_visual_or_resume` helper (Task 5), add `self.expand_history.clear();` right before each `self.set_mode(mode); return true;`/`self.set_mode(mode); true` — both the resume branch and the fresh-anchor branch should start with empty history (a resumed region's prior growth steps aren't preserved across a commit/resume cycle, since `Region` doesn't store them — this is a deliberate scope limitation, not an oversight; note it as such if asked).

- [ ] **Step 4: Implement `expand_active_region`**

Add to `src/editor/multi_region.rs`:

```rust
const EXPAND_CANDIDATES: &[(crate::text_objects::ObjectKind, u8)] = &[
    (crate::text_objects::ObjectKind::Word, 1),
    (crate::text_objects::ObjectKind::DoubleQuote, 1),
    (crate::text_objects::ObjectKind::SingleQuote, 1),
    (crate::text_objects::ObjectKind::Backtick, 1),
    (crate::text_objects::ObjectKind::AnyBracket, 1),
    (crate::text_objects::ObjectKind::AnyBracket, 2),
    (crate::text_objects::ObjectKind::AnyBracket, 3),
    (crate::text_objects::ObjectKind::Line, 1),
    (crate::text_objects::ObjectKind::Sentence, 1),
    (crate::text_objects::ObjectKind::Paragraph, 1),
    (crate::text_objects::ObjectKind::Buffer, 1),
];

impl<T: TerminalBackend> Editor<T> {
    /// `<Space>`: grow the active Visual region to the smallest enclosing
    /// candidate that's strictly larger than the current span. Pushes the
    /// prior extent onto `expand_history` first (Task 24 pops it).
    pub(super) fn expand_active_region(&mut self) -> bool {
        let Some(anchor) = self.visual_anchor else { return false };
        let Some(doc) = self.document_manager.active_document() else { return false };
        let cursor = doc.buffer.cursor();
        let current = (anchor.min(cursor), anchor.max(cursor) + 1);

        let mut best: Option<(usize, usize)> = None;
        for &(kind, nesting) in EXPAND_CANDIDATES {
            use crate::text_objects::{Direction, Modifier, TextObjectSpec};
            let spec = TextObjectSpec {
                modifier: Modifier::Around,
                direction: Direction::Current,
                nesting,
                kind,
            };
            let Some(range) = crate::text_objects::resolve(spec, &doc.buffer, 1, None) else {
                continue;
            };
            let end_offset = if range.inclusive { 1 } else { 0 };
            let s = range.anchor.min(range.new_cursor);
            // Clamp: a last line with no trailing newline can overshoot by one
            // (same case clipboard::capture_text already guards).
            let e = (range.anchor.max(range.new_cursor) + end_offset).min(doc.buffer.len());
            let strictly_larger = s <= current.0 && e >= current.1 && (s < current.0 || e > current.1);
            if !strictly_larger {
                continue;
            }
            if best.is_none_or(|(bs, be)| (e - s) < (be - bs)) {
                best = Some((s, e));
            }
        }

        let Some((new_start, new_end)) = best else { return false };
        self.expand_history.push(current);
        self.visual_anchor = Some(new_start);
        if let Some(doc) = self.document_manager.active_document_mut() {
            let _ = doc.buffer.set_cursor(new_end.saturating_sub(1));
        }
        true
    }
}
```

`Option::is_none_or` is stable since Rust 1.82 (verified during self-review: `Cargo.toml` declares no `rust-version` pin, and the installed toolchain is `rustc 1.92.0` — safe to use as written, no `map_or` fallback needed).

- [ ] **Step 5: Wire the action**

```rust
            EditorAction::ExpandRegion => self.expand_active_region(),
```

In `src/keymap/defaults.rs`:

```rust
    keymap.register(
        KeyContext::Visual,
        Key::Char(' '),
        Action::Editor(EditorAction::ExpandRegion),
    );
```

- [ ] **Step 6: Run the tests**

Run: `cargo test --lib editor::tests::expand_region -- --nocapture 2>&1 | tail -60`
Expected: 2 passed — the exact offsets in `expand_region_grows_word_then_quotes_in_verified_order` were confirmed empirically (see the table above this task's Step 1), not guessed, so this should pass on the first real run. If it doesn't, the discrepancy is between this plan's verification run and your build — re-run the same scratch experiment (a temporary test in `src/text_objects/tests.rs` calling `resolve()` directly with `eprintln!`, exactly as described above) before changing `EXPAND_CANDIDATES`.

- [ ] **Step 7: Full suite**

Run: `cargo test 2>&1 | tail -5`

---

### Task 24: Shrink region (`<Shift-Space>`)

**Files:**
- Modify: `src/key/mod.rs` (`Key::ShiftSpace` variant)
- Modify: `src/term/crossterm/mod.rs:176-199` (emit it when `Char(' ')` arrives with the shift modifier)
- Modify: `src/editor/multi_region.rs` (`shrink_active_region`)
- Modify: `src/action/mod.rs` (`EditorAction::ShrinkRegion`)
- Modify: `src/editor/handle_action.rs`, `src/keymap/defaults.rs`
- Test: `src/editor/tests.rs`, `src/term/crossterm/tests.rs` (check this file exists — `ls src/term/crossterm/` — if `translate_key_event` has no dedicated test file yet, add one inline near the function instead of inventing a new test module)

**Interfaces:**
- Consumes: `expand_history` (Task 23).
- Produces: `Key::ShiftSpace`. `EditorAction::ShrinkRegion` — pops the last extent off `expand_history` and restores it as the active region; no-op if the stack is empty. **No new config subsystem for the "fallback key" the design doc mentions** — Rift's existing keymap registration (`KeyMap::register`, already exposed to runtime customization via `register_from_str`/plugin registration) already lets a user rebind `EditorAction::ShrinkRegion` to any key their terminal actually transmits, which *is* the fallback mechanism design.md S3 asks for — no additional plumbing needed.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn shrink_region_pops_the_last_expand_step() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "say \"hello world\" now");
    let pos = editor.active_document().buffer.to_string().find("hello").unwrap();
    editor.active_document().buffer.set_cursor(pos).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));

    let before_expand = (editor.visual_anchor, editor.active_document().buffer.cursor());
    editor.handle_action(&Action::Editor(EditorAction::ExpandRegion));
    let after_expand = (editor.visual_anchor, editor.active_document().buffer.cursor());
    assert_ne!(before_expand, after_expand, "expand must have actually grown the region");

    editor.handle_action(&Action::Editor(EditorAction::ShrinkRegion));
    let after_shrink = (editor.visual_anchor, editor.active_document().buffer.cursor());

    assert_eq!(after_shrink, before_expand, "shrink must restore the exact pre-expand extent");
}

#[test]
fn shrink_region_with_empty_history_is_a_noop() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "hello");
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));

    let handled = editor.handle_action(&Action::Editor(EditorAction::ShrinkRegion));

    assert!(!handled);
}

#[test]
fn shift_space_translates_to_key_shift_space() {
    use crate::term::crossterm::translate_key_event;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let event = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::SHIFT);
    assert_eq!(translate_key_event(event), crate::key::Key::ShiftSpace);
}
```

Verified: `translate_key_event` is `pub(crate)`, and `Key` derives `Debug, Clone, Copy, PartialEq, Eq, Hash` (`src/key/mod.rs:4`) — the third test's `assert_eq!` is valid as written.

- [ ] **Step 2: Confirm failure**

Run: `cargo test --lib editor::tests::shrink_region term::crossterm::shift_space -- --nocapture 2>&1 | tail -50`

- [ ] **Step 3: Add `Key::ShiftSpace`**

In `src/key/mod.rs`, next to `ShiftTab`:

```rust
    /// Space pressed with the Shift modifier (terminal support varies --
    /// see visual-mode-design.md S3 -- rebind via the keymap if your
    /// terminal never transmits this).
    ShiftSpace,
```

Check `Key`'s `to_vt100_bytes`-style match (used for terminal passthrough, seen at `Key::ShiftTab => vec![0x1b, b'[', b'Z']` around line 66) for exhaustiveness — add a no-op or best-effort arm for `ShiftSpace` there too (e.g. `Key::ShiftSpace => vec![b' ']`, degrading gracefully to a plain space if ever sent to a real terminal subprocess) so the match stays exhaustive.

- [ ] **Step 4: Emit it from the crossterm backend**

In `src/term/crossterm/mod.rs`, the `KeyCode::Char(ch)` branch's final `else` (currently `Key::Char(ch)`):

```rust
            } else if _shift && ch == ' ' {
                Key::ShiftSpace
            } else {
                Key::Char(ch)
            }
```

Rename `_shift` to `shift` now that it's actually used (drop the underscore prefix everywhere it appears in this function — `grep -n "_shift" src/term/crossterm/mod.rs` to find both occurrences).

- [ ] **Step 5: Implement `shrink_active_region` and the action**

Add to `src/editor/multi_region.rs`:

```rust
    /// `<Shift-Space>`: pop the last expand step and restore it.
    pub(super) fn shrink_active_region(&mut self) -> bool {
        if self.visual_anchor.is_none() {
            return false;
        }
        let Some((start, end)) = self.expand_history.pop() else {
            return false;
        };
        self.visual_anchor = Some(start);
        if let Some(doc) = self.document_manager.active_document_mut() {
            let _ = doc.buffer.set_cursor(end.saturating_sub(1));
        }
        true
    }
```

In `src/action/mod.rs`:

```rust
    /// `<Shift-Space>` in Visual: undo the last `<Space>` expand step.
    ShrinkRegion,
```

In `src/editor/handle_action.rs`:

```rust
            EditorAction::ShrinkRegion => self.shrink_active_region(),
```

In `src/keymap/defaults.rs`:

```rust
    keymap.register(
        KeyContext::Visual,
        Key::ShiftSpace,
        Action::Editor(EditorAction::ShrinkRegion),
    );
```

- [ ] **Step 6: Run the tests**

Run: `cargo test --lib editor::tests::shrink_region term::crossterm::shift_space -- --nocapture 2>&1 | tail -60`
Expected: 3 passed.

- [ ] **Step 7: Full suite**

Run: `cargo test 2>&1 | tail -5`

---

## Phase 10 — `gv` regions window

**Modeled directly on the existing single-pane location-list panel** (`src/editor/lsp_ops.rs:728-800` `open_location_list_panel`, and `src/editor/explorer.rs:242-306` `close_split_panel`'s `PanelKind::LocationList` arm, both read in full during this session) — `gv` is structurally identical: one new read-only private buffer, split below the current window, closed by reversing the same split. Reuse that exact mechanism; do not invent a new panel-management path.

### Task 25: `BufferKind::Regions` + `populate_regions_buffer`

**Files:**
- Modify: `src/document/mod.rs` (`BufferKind::Regions` variant + `kind_str` arm)
- Modify: `src/document/populate.rs` (`populate_regions_buffer`, mirroring `populate_clipboard_buffer` at line 301)
- Test: `src/document/tests.rs`

**Interfaces:**
- Consumes: `Region::buffer_span` (Task 3).
- Produces: `BufferKind::Regions { source_doc_id: DocumentId }`. `Document::populate_regions_buffer(&mut self, source_buf: &TextBuffer, regions: &[Region])` — one line per region, `"N: row:col \"preview text\""`, newlines in the preview replaced with `⏎`, truncated past 48 chars with `...`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn populate_regions_buffer_writes_one_line_per_region() {
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut source = TextBuffer::new(20).unwrap();
    source.insert_str("hello\nworld").unwrap();
    let regions = vec![
        Region::new(0, 4, RangeKind::Charwise), // "hello"
        Region::new(6, 10, RangeKind::Charwise), // "world"
    ];

    let mut doc = Document::new(1).unwrap();
    doc.populate_regions_buffer(&source, &regions);

    let lines: Vec<&str> = doc.buffer.to_string().lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("0:0"), "row:col for the first region; got {:?}", lines[0]);
    assert!(lines[0].contains("hello"));
    assert!(lines[1].contains("1:0"));
    assert!(lines[1].contains("world"));
}

#[test]
fn populate_regions_buffer_with_no_regions_shows_empty_placeholder() {
    let source = TextBuffer::new(10).unwrap();
    let mut doc = Document::new(1).unwrap();
    doc.populate_regions_buffer(&source, &[]);

    assert_eq!(doc.buffer.to_string(), "(empty)");
}
```

- [ ] **Step 2: Confirm failure**

Run: `cargo test --lib document::tests::populate_regions_buffer -- --nocapture 2>&1 | tail -40`

- [ ] **Step 3: Add the `BufferKind` variant**

In `src/document/mod.rs`:

```rust
    /// `gv` regions window: a read-only list of the active document's
    /// banked `SelectionSet`, one line per region.
    Regions { source_doc_id: DocumentId },
```

And in `BufferKind::kind_str`: `BufferKind::Regions { .. } => "regions",`.

- [ ] **Step 4: Implement `populate_regions_buffer`**

Add to `src/document/populate.rs`, next to `populate_clipboard_buffer`:

```rust
    /// Populate (or repopulate) the `gv` regions list from `regions`,
    /// computed against `source_buf` (the document the set belongs to).
    pub fn populate_regions_buffer(&mut self, source_buf: &TextBuffer, regions: &[crate::selection::Region]) {
        use crate::buffer::api::BufferView;

        let mut content = String::new();
        if regions.is_empty() {
            content.push_str("(empty)");
        } else {
            for (i, region) in regions.iter().enumerate() {
                let (start, end) = region.buffer_span(source_buf);
                let row = source_buf.line_index.get_line_at(start);
                let line_start = source_buf.line_index.get_start(row).unwrap_or(0);
                let col = start.saturating_sub(line_start);
                let raw: String = source_buf.chars(start..end).map(|c| c.to_char_lossy()).collect();
                let raw = raw.replace('\n', "\u{23ce}");
                let preview: String = if raw.chars().count() > 48 {
                    raw.chars().take(45).chain("...".chars()).collect()
                } else {
                    raw
                };
                content.push_str(&format!("{}: {}:{} \"{}\"\n", i + 1, row, col, preview));
            }
            if content.ends_with('\n') {
                content.pop();
            }
        }
        self.replace_buffer_content(&content);
        self.history.mark_saved();
    }
```

- [ ] **Step 5: Run the tests**

Run: `cargo test --lib document::tests::populate_regions_buffer -- --nocapture 2>&1 | tail -60`
Expected: 2 passed.

- [ ] **Step 6: Full suite**

Run: `cargo test 2>&1 | tail -5`

---

### Task 26: `gv` toggle, window navigation, and `j`/`k`/`Enter`/`x`/`q`/`d`/`c`/`y` inside the window

**Files:**
- Modify: `src/editor/mod.rs` (`PanelKind::Regions` variant)
- Modify: `src/editor/explorer.rs` (`close_split_panel`'s new `PanelKind::Regions` arm)
- Modify: `src/editor/multi_region.rs` (`toggle_regions_window` + the window's `j`/`k`/`Enter`/`x` handlers)
- Modify: `src/action/mod.rs` (five new `EditorAction` variants)
- Modify: `src/editor/handle_action.rs`
- Modify: `src/keymap/mod.rs` (`KeyContext::Regions`, falls through to `Normal`)
- Modify: `src/keymap/defaults.rs` (`gv` sequence under `Normal`; `j`/`k`/`Enter`/`x`/`q` under `Regions`; `d`/`c`/`y` under `Regions` reusing `EditorAction::Operator`)
- Test: `src/editor/tests.rs`

**Interfaces:**
- Consumes: `populate_regions_buffer` (Task 25), `close_split_panel`/`PanelLayout` (existing — read in full earlier in this session), `try_run_set_aware_operator` (Task 13).
- Produces: `EditorAction::ToggleRegionsWindow` (`gv`), `RegionsListDown`, `RegionsListUp`, `RegionsListSelect` (`Enter`), `RegionsListDrop` (`x`). `d`/`c`/`y` inside the window reuse the *existing* `EditorAction::Operator` — its handler gains one more check: if the active document is a `Regions` list buffer, redirect to the recorded `source_doc_id` first.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn gv_toggle_opens_and_closes_regardless_of_focus() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "hello world");
    editor.active_document().selection_set.bank(Region::new(0, 4, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::ToggleRegionsWindow));
    assert!(editor.panel_layout.is_some(), "gv opens the window");
    assert_eq!(
        editor.active_document().kind.kind_str(),
        "regions",
        "focus moves into the new regions window"
    );

    editor.handle_action(&Action::Editor(EditorAction::ToggleRegionsWindow));
    assert!(editor.panel_layout.is_none(), "a second gv closes it again");
}

#[test]
fn gv_with_empty_set_does_not_open_a_window() {
    use crate::action::{Action, EditorAction};

    let mut editor = create_editor();
    load_text(&mut editor, "hello world");

    editor.handle_action(&Action::Editor(EditorAction::ToggleRegionsWindow));

    assert!(editor.panel_layout.is_none());
}

#[test]
fn regions_window_x_drops_the_selected_entry() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.active_document().selection_set.bank(Region::new(0, 1, RangeKind::Charwise));
    editor.active_document().selection_set.bank(Region::new(5, 6, RangeKind::Charwise));
    editor.handle_action(&Action::Editor(EditorAction::ToggleRegionsWindow));

    editor.handle_action(&Action::Editor(EditorAction::RegionsListDrop));

    let source_id = match editor.active_document().kind {
        crate::document::BufferKind::Regions { source_doc_id } => source_doc_id,
        _ => panic!("expected to still be focused in the regions window"),
    };
    assert_eq!(
        editor.document_manager.get_document(source_id).unwrap().selection_set.regions.len(),
        1,
        "one entry dropped from the *source* document's set"
    );
}

#[test]
fn regions_window_j_moves_the_list_cursor_and_live_jumps_the_preview() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.active_document().selection_set.bank(Region::new(0, 1, RangeKind::Charwise));
    editor.active_document().selection_set.bank(Region::new(5, 6, RangeKind::Charwise));
    let source_id = editor.active_document_id();
    editor.handle_action(&Action::Editor(EditorAction::ToggleRegionsWindow));
    let list_cursor_before = editor.active_document().buffer.cursor();

    editor.handle_action(&Action::Editor(EditorAction::RegionsListDown));

    assert_ne!(
        editor.active_document().buffer.cursor(),
        list_cursor_before,
        "j must move the regions list's own cursor to line 2, not stay on line 1"
    );
    assert_eq!(
        editor.document_manager.get_document(source_id).unwrap().buffer.cursor(),
        5,
        "and live-jump the source buffer to the second region (sorted order: 0..1, then 5..6)"
    );
}

#[test]
fn regions_window_operator_redirects_to_the_source_document() {
    use crate::action::{Action, EditorAction, OperatorType};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.active_document().selection_set.bank(Region::new(0, 1, RangeKind::Charwise));
    editor.active_document().selection_set.bank(Region::new(5, 6, RangeKind::Charwise));
    let source_id = editor.active_document_id();
    editor.handle_action(&Action::Editor(EditorAction::ToggleRegionsWindow));

    editor.handle_action(&Action::Editor(EditorAction::Operator(OperatorType::Delete)));

    assert!(editor.panel_layout.is_none(), "firing an operator from the window closes it");
    assert_eq!(
        editor.document_manager.get_document(source_id).unwrap().buffer.to_string(),
        "234789"
    );
}
```

- [ ] **Step 2: Confirm failure**

Run: `cargo test --lib editor::tests::gv_toggle editor::tests::gv_with_empty_set editor::tests::regions_window -- --nocapture 2>&1 | tail -100`

- [ ] **Step 3: Add `PanelKind::Regions` and its `close_split_panel` arm**

In `src/editor/mod.rs`'s `PanelKind`:

```rust
    /// `gv` regions list (visual-mode-design.md S4).
    Regions,
```

In `src/editor/explorer.rs`'s `close_split_panel`, add a sibling arm to the existing `PanelKind::LocationList` arm (same shape — single list window, focus returns to the source window, no preview-doc reassignment needed):

```rust
            PanelKind::Regions => {
                self.split_tree.close_window(layout.dir_win_id);
                self.document_manager
                    .remove_private_document(layout.dir_doc_id);
                self.split_tree.set_focus(layout.preview_win_id);
                let _ = self
                    .document_manager
                    .switch_to_document(layout.original_doc_id);
            }
```

- [ ] **Step 4: Implement `toggle_regions_window`**

Add to `src/editor/multi_region.rs`, mirroring `open_location_list_panel` (`src/editor/lsp_ops.rs:728-800`) for the window-creation mechanics:

```rust
    /// `gv`: toggle the regions list window. Always means "stop looking at
    /// the list" when one is already open, regardless of current focus.
    pub(super) fn toggle_regions_window(&mut self) {
        if let Some(layout) = self.panel_layout.clone() {
            if layout.kind == crate::editor::PanelKind::Regions {
                self.close_split_panel();
                return;
            }
        }

        let Some(source_doc_id) = self.document_manager.active_document().map(|d| d.id) else {
            return;
        };
        let regions = self
            .document_manager
            .active_document()
            .map(|d| d.selection_set.sorted())
            .unwrap_or_default();
        if regions.is_empty() {
            self.state.notify(
                crate::notification::NotificationType::Info,
                "No regions banked".to_string(),
            );
            return;
        }

        let list_doc_id = self.document_manager.next_id();
        let mut doc = match crate::document::Document::new(list_doc_id) {
            Ok(d) => d,
            Err(e) => {
                self.state.handle_error(e);
                return;
            }
        };
        doc.is_read_only = true;
        if let Some(source) = self.document_manager.active_document() {
            doc.populate_regions_buffer(&source.buffer, &regions);
        }
        doc.kind = crate::document::BufferKind::Regions { source_doc_id };
        self.document_manager.add_private_document(doc);

        let size = self
            .term
            .get_size()
            .unwrap_or(crate::term::Size { rows: 24, cols: 80 });
        let preview_win_id = self.split_tree.focused_window_id();
        let original_doc_id = self.split_tree.focused_window().document_id;
        let dir_win_id = self.split_tree.split(
            crate::split::tree::SplitDirection::Horizontal,
            preview_win_id,
            list_doc_id,
            size.rows as usize,
            size.cols as usize,
        );
        self.split_tree.set_focus(dir_win_id);
        let _ = self.document_manager.switch_to_document(list_doc_id);

        self.panel_layout = Some(crate::editor::PanelLayout {
            kind: crate::editor::PanelKind::Regions,
            dir_win_id,
            preview_win_id,
            dir_doc_id: list_doc_id,
            preview_doc_id: original_doc_id,
            original_doc_id,
        });
        self.sync_state_with_active_document();
        let _ = self.force_full_redraw();
    }

    /// `x` inside the regions window: drop the entry at the cursor's line
    /// from the *source* document's `SelectionSet`, then refresh the list.
    pub(super) fn drop_regions_window_entry(&mut self) -> bool {
        let Some(layout) = self.panel_layout.clone() else { return false };
        if layout.kind != crate::editor::PanelKind::Regions {
            return false;
        }
        let line = self
            .document_manager
            .active_document()
            .map(|d| d.buffer.line_index.get_line_at(d.buffer.cursor()))
            .unwrap_or(0);
        let crate::document::BufferKind::Regions { source_doc_id } =
            self.document_manager.active_document().map(|d| d.kind.clone()).unwrap_or(crate::document::BufferKind::File)
        else {
            return false;
        };
        let Some(source) = self.document_manager.get_document_mut(source_doc_id) else {
            return false;
        };
        let sorted = source.selection_set.sorted();
        let Some(target) = sorted.get(line).copied() else {
            return false;
        };
        source.selection_set.regions.retain(|r| *r != target);
        let remaining = source.selection_set.sorted();
        let source_buf = source.buffer.clone();
        if let Some(doc) = self.document_manager.active_document_mut() {
            doc.populate_regions_buffer(&source_buf, &remaining);
        }
        true
    }
```

`BufferKind` needs `Clone` for the `.clone()` call above — check `grep -n "derive.*BufferKind\|impl Clone for BufferKind" src/document/mod.rs`; the `Directory`/`UndoTree`/etc. variants already hold owned `Vec`s and get cloned elsewhere in this codebase (`panel_handlers.rs`'s `Some(l) if l.kind == PanelKind::LocationList => l.clone()` pattern clones a *different* type, but the precedent of cloning state out of a `BufferKind` match before taking a second mutable borrow is the same shape used throughout `panel_handlers.rs` — follow that pattern if `BufferKind` itself turns out not to derive `Clone`, e.g. by extracting just `source_doc_id: DocumentId` (which *is* `Copy`) via a `match &doc.kind { BufferKind::Regions { source_doc_id } => Some(*source_doc_id), _ => None }` instead of cloning the whole enum).

- [ ] **Step 5: Add the five actions and wire them**

In `src/action/mod.rs`:

```rust
    /// `gv`: toggle the regions list window.
    ToggleRegionsWindow,
    /// `j` inside the regions window: move down, live-jump the preview.
    RegionsListDown,
    /// `k` inside the regions window: move up, live-jump the preview.
    RegionsListUp,
    /// `Enter` inside the regions window: jump and return focus to the buffer.
    RegionsListSelect,
    /// `x` inside the regions window: drop that entry from the set.
    RegionsListDrop,
```

In `src/editor/handle_action.rs`:

```rust
            EditorAction::ToggleRegionsWindow => {
                self.toggle_regions_window();
                true
            }
            EditorAction::RegionsListDrop => self.drop_regions_window_entry(),
            EditorAction::RegionsListDown | EditorAction::RegionsListUp | EditorAction::RegionsListSelect => {
                let Some(layout) = self.panel_layout.clone() else { return false };
                if layout.kind != crate::editor::PanelKind::Regions {
                    return false;
                }
                // `j`/`k` are bound directly to this arm (not through the
                // generic Move action), so it must move the list's own
                // cursor itself before computing which line to preview.
                if let Some(doc) = self.document_manager.active_document_mut() {
                    match editor_action {
                        EditorAction::RegionsListDown => {
                            doc.buffer.move_down();
                        }
                        EditorAction::RegionsListUp => {
                            doc.buffer.move_up();
                        }
                        _ => {}
                    }
                }
                let line = self
                    .document_manager
                    .active_document()
                    .map(|d| d.buffer.line_index.get_line_at(d.buffer.cursor()))
                    .unwrap_or(0);
                let region = self
                    .document_manager
                    .get_document(layout.preview_doc_id)
                    .map(|d| d.selection_set.sorted())
                    .and_then(|sorted| sorted.get(line).copied());
                let Some(region) = region else { return false };
                if let Some(source) = self.document_manager.get_document_mut(layout.preview_doc_id) {
                    let (start, _) = region.buffer_span(&source.buffer);
                    let _ = source.buffer.set_cursor(start);
                }
                if matches!(editor_action, EditorAction::RegionsListSelect) {
                    self.close_split_panel();
                }
                true
            }
```

(`layout.preview_doc_id` holds the *source* document's id here — `PanelLayout`'s field naming comes from the `LocationList`/`Clipboard` precedent where "preview" meant "the other pane"; for `gv`'s single-pane case it ends up meaning "the source document", which is exactly what's needed — re-confirm this against `open_location_list_panel`'s field assignment (`preview_doc_id: original_doc_id`) if it looks surprising.)

- [ ] **Step 6: Redirect `d`/`c`/`y` to the source document when fired from the regions window**

In `src/editor/handle_action.rs`'s `EditorAction::Operator(op)` arm (last touched by Task 13), add this check as the very first statement, before the Visual-commit check:

```rust
            EditorAction::Operator(op) => {
                if let Some(layout) = self.panel_layout.clone() {
                    if layout.kind == crate::editor::PanelKind::Regions {
                        let _ = self.document_manager.switch_to_document(layout.preview_doc_id);
                        self.close_split_panel();
                    }
                }
                if self.current_mode.is_visual() {
                    /* unchanged from Task 13 */
                }
                if self.try_run_set_aware_operator(*op) {
                    self.finish_region_build(None);
                    return true;
                }
                /* unchanged */
            }
```

- [ ] **Step 7: Register the keymap**

In `src/keymap/mod.rs`, add `KeyContext::Regions` and its fallback:

```rust
    /// `gv` regions list window. Falls through to Normal for j/k/etc.
    Regions,
```

```rust
            KeyContext::Regions => Some(KeyContext::Normal),
```

In `src/keymap/defaults.rs`:

```rust
    keymap.register_sequence(
        KeyContext::Normal,
        vec![Key::Char('g'), Key::Char('v')],
        Action::Editor(EditorAction::ToggleRegionsWindow),
    );
    keymap.register(
        KeyContext::Regions,
        Key::Char('j'),
        Action::Editor(EditorAction::RegionsListDown),
    );
    keymap.register(
        KeyContext::Regions,
        Key::Char('k'),
        Action::Editor(EditorAction::RegionsListUp),
    );
    keymap.register(
        KeyContext::Regions,
        Key::Enter,
        Action::Editor(EditorAction::RegionsListSelect),
    );
    keymap.register(
        KeyContext::Regions,
        Key::Char('x'),
        Action::Editor(EditorAction::RegionsListDrop),
    );
    keymap.register(
        KeyContext::Regions,
        Key::Char('q'),
        Action::Buffer("regions:close".to_string()),
    );
    keymap.register(
        KeyContext::Regions,
        Key::Char('d'),
        Action::Editor(EditorAction::Operator(crate::action::OperatorType::Delete)),
    );
    keymap.register(
        KeyContext::Regions,
        Key::Char('c'),
        Action::Editor(EditorAction::Operator(crate::action::OperatorType::Change)),
    );
    keymap.register(
        KeyContext::Regions,
        Key::Char('y'),
        Action::Editor(EditorAction::Operator(crate::action::OperatorType::Yank)),
    );
```

`Action::Buffer("regions:close")` needs a matching arm wherever `Action::Buffer(id)` is dispatched against `BufferKind` (the `handle_action` dispatcher seen earlier in this session: `match kind { BufferKind::Directory {..} => self.handle_directory_buffer_action(id), ... }`) — add `BufferKind::Regions { .. } => self.handle_regions_buffer_action(id),` there, and a small handler in `src/editor/panel_handlers.rs`:

```rust
    pub(super) fn handle_regions_buffer_action(&mut self, id: &str) {
        if id == "regions:close" {
            self.close_split_panel();
        }
    }
```

Verified during self-review (`grep -n "Char('g'), Key::Char" src/keymap/defaults.rs`): the existing `g`-prefixed sequences are `gg`, `gp`, `gP`, `gd`, `gr` — no `gv`, so this registration is conflict-free.

- [ ] **Step 8: Run the tests**

Run: `cargo test --lib editor::tests::gv_toggle editor::tests::gv_with_empty_set editor::tests::regions_window -- --nocapture 2>&1 | tail -150`
Expected: 4 passed.

- [ ] **Step 9: Full suite**

Run: `cargo test 2>&1 | tail -5`

---

## Phase 11 — VisualBlock: type only, no rectangle semantics

**Descoped by explicit decision:** `Ctrl-V` enters `Mode::VisualBlock` and `RangeKind::Blockwise` exists as a type (Tasks 1-3), but no rectangular behavior is implemented. Rendering, `O`, and every set-aware command treat a `Blockwise` region identically to `Charwise` -- one linear `(start, end)` span from `buffer_span`, exactly like Task 1 already set up (`RangeKind::Blockwise => self.span()` in `buffer_span`). `O` is **not** given a column-only variant -- it stays the plain `VisualSwapEnds` from Task 6 in every mode, including `VisualBlock`. The only thing this phase still needs to verify is the one piece of behavior Task 3 already provides for free: merge stays restricted to block-vs-block (same-kind-only), so a future implementation of real rectangle semantics won't have to also fix accidental cross-kind merging.

### Task 27: Verify merge stays restricted to block-vs-block

**Files:**
- Test: `src/selection/tests.rs`

**Interfaces:**
- Consumes: `SelectionSet::bank`'s existing same-kind-only check (`Region::overlaps`, Task 3 -- `if self.kind != other.kind { return false; }`). This task adds **no new production code**: the restriction already exists as a side effect of Task 3's general same-kind rule, written before `Blockwise` was even a distinct concept in this plan. This task exists purely to make that fact an explicit, named regression test instead of an accidental property -- so if a future plan adds real rectangle semantics and touches `overlaps()`, this test catches an accidental regression immediately.

- [ ] **Step 1: Write the test**

```rust
#[test]
fn blockwise_regions_only_merge_with_other_blockwise_overlap() {
    use crate::wrap::RangeKind;

    let mut set = SelectionSet::default();
    set.bank(Region::new(0, 5, RangeKind::Blockwise));
    set.bank(Region::new(3, 8, RangeKind::Charwise)); // overlaps in raw offsets, wrong kind
    assert_eq!(set.regions.len(), 2, "Blockwise must not merge with an overlapping Charwise region");

    set.bank(Region::new(3, 8, RangeKind::Blockwise)); // overlaps the first Blockwise region
    assert_eq!(set.regions.len(), 2, "but DOES merge with an overlapping Blockwise region");
}

#[test]
fn visual_block_renders_and_edits_identically_to_charwise() {
    use crate::action::{Action, EditorAction, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualBlock));
    editor.handle_action(&Action::Editor(EditorAction::Move(crate::action::Motion::Right)));
    editor.handle_action(&Action::Editor(EditorAction::Operator(OperatorType::Delete)));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "23456789",
        "Ctrl-V behaves exactly like v -- no rectangle semantics by design"
    );
}
```

- [ ] **Step 2: Run**

Run: `cargo test --lib selection::tests::blockwise_regions_only_merge editor::tests::visual_block_renders_and_edits_identically -- --nocapture 2>&1 | tail -40`
Expected: 2 passed, with no production code changed beyond what Tasks 1-13 already wrote. If the first fails, Task 3's `overlaps()` regressed somewhere between then and now -- fix `overlaps()`, don't add a Blockwise-specific carve-out next to it. If the second fails, something added real rectangle handling somewhere it doesn't belong yet (out of scope for this plan) -- find and remove it rather than adjusting this test's expectation.

- [ ] **Step 3: Full suite**

Run: `cargo test 2>&1 | tail -5`

---

## Phase 12 — Final design.md S9 regression sweep

This phase closes the gap between design.md's explicit S9 test checklist and what Tasks 1-28 actually wrote. Cross-reference, item by item: the worked example (Task 8 banked but never deleted), touching/overlap regression (Task 3 + 13/14), expand-region stepping (Task 23), single-region-`c` unaffected (Task 14), single-undo-step (Task 11/13/19/20), bare-repeat-falls-back (Task 19) are all covered. **Not yet covered:** the worked example's actual delete step, a comprehensive edit-clears-the-set sweep across every S5 command, dot-repeat's destructive-group coverage beyond `Delete` alone, and paste's two dot-repeat-specific behaviors (stacks differently than bare repeat; `CyclePaste` only ever touches the single most-recent position once the set is gone).

### Task 28: The remaining S9 tests

**Files:**
- Test: `src/editor/tests.rs`

**Interfaces:** none — every test below exercises production code from Tasks 1-26 exclusively.

- [ ] **Step 1: The worked example, completed**

```rust
#[test]
fn issue_worked_example_full_sequence_including_delete() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "Hello\nworld\nfoo\n");

    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode)); // banks "Ho"

    let line3_start = editor.active_document().buffer.line_start(2);
    let _ = editor.active_document().buffer.set_cursor(line3_start);
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode)); // banks "f"

    editor.handle_action(&Action::Editor(EditorAction::Operator(OperatorType::Delete)));

    assert_eq!(editor.active_document().buffer.to_string(), "llo\nworld\noo\n");
    assert!(editor.active_document().selection_set.is_empty());
}
```

- [ ] **Step 2: Edit-clears-the-set sweep**

This is deliberately written as one block-per-command sweep rather than N separate test functions, covering a representative slice of design.md S5's full command list (`d`/`y`/`i`/`o`/`p`) — chosen specifically because together they exercise both drivers (`apply_to_each_region` via `d`/`y`/`p`, `enter_multi_insert` via `i`/`o`) and the "did the set actually get cleared after replay finished" path, which is where a "forgot to clear" bug would actually live:

```rust
#[test]
fn undo_of_unrelated_edit_clears_a_banked_set() {
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.active_document().insert_char('!').unwrap(); // an edit NOT routed through any driver
    editor.active_document().selection_set.bank(Region::new(0, 1, RangeKind::Charwise));
    assert!(!editor.active_document().selection_set.is_empty());

    assert!(editor.active_document().undo());

    assert!(editor.active_document().selection_set.is_empty());
}

#[test]
fn every_set_aware_command_clears_the_set_after_acting() {
    use crate::action::{Action, EditorAction, OperatorType};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let fresh_set = |editor: &mut Editor<MockTerminal>| {
        editor.active_document().selection_set.bank(Region::new(0, 0, RangeKind::Charwise));
        editor.active_document().selection_set.bank(Region::new(4, 4, RangeKind::Charwise));
    };

    // d
    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    fresh_set(&mut editor);
    editor.handle_action(&Action::Editor(EditorAction::Operator(OperatorType::Delete)));
    assert!(editor.active_document().selection_set.is_empty(), "d must clear the set");

    // y
    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    fresh_set(&mut editor);
    editor.handle_action(&Action::Editor(EditorAction::Operator(OperatorType::Yank)));
    assert!(editor.active_document().selection_set.is_empty(), "y must clear the set");

    // i (then Esc to finish the insert session)
    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    fresh_set(&mut editor);
    editor.handle_action(&Action::Editor(EditorAction::EnterInsertMode));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    assert!(editor.active_document().selection_set.is_empty(), "i must clear the set");

    // o
    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    fresh_set(&mut editor);
    editor.handle_action(&Action::Editor(EditorAction::OpenLineBelow));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    assert!(editor.active_document().selection_set.is_empty(), "o must clear the set");

    // p
    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.clipboard_ring.push("X".to_string());
    fresh_set(&mut editor);
    editor.handle_action(&Action::Editor(EditorAction::Put { before: false }));
    assert!(editor.active_document().selection_set.is_empty(), "p must clear the set");
}
```

If you want full literal coverage of design.md's enumerated command list (it names `d`/`c`/`y`/`i`/`a`/`I`/`A`/`o`/`O`/`r`/`:s`/`sg`/`sd`/`sc`/`p`/`P`), extend this with the same one-block-per-command pattern for the rest, using Task 16/17/18's call patterns for `r`/surround/`:s`. The five shown here are not a sampling shortcut to skip work — they're chosen because they're the only ones touching genuinely different code paths (the other insert-family commands share `i`'s exact path through `enter_multi_insert`; `c`/`r`/surround share `d`'s exact path through `apply_to_each_region`).

- [ ] **Step 3: Dot-repeat destructive-group coverage beyond `Delete`**

Task 22 only exercised `Delete`; add `Yank` (design.md explicitly says "Repeat for `c`, `y`, `sd`, `sc`" — `c` is already covered by Task 14's existence as a non-trivial multi-insert path, so this fills the next most distinct gap):

```rust
#[test]
fn dot_repeat_yank_reselects_without_reexecuting() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "(a) (b)");
    editor.active_document().buffer.set_cursor(0).unwrap();
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    editor.handle_action(&Action::Editor(EditorAction::Operator(OperatorType::Yank)));
    let buffer_before = editor.active_document().buffer.to_string();
    assert!(editor.active_document().selection_set.is_empty());

    editor.active_document().buffer.set_cursor(5).unwrap();
    editor.execute_dot_repeat();

    assert_eq!(
        editor.active_document().buffer.to_string(),
        buffer_before,
        "yank's dot-repeat must not mutate the buffer"
    );
    assert_eq!(editor.active_document().selection_set.regions.len(), 1, "but must rebank the equivalent region");
}
```

- [ ] **Step 4: Paste's two dot-repeat-specific behaviors**

```rust
#[test]
fn dot_repeat_paste_genuinely_differs_from_bare_repeat() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.clipboard_ring.push("X".to_string());
    editor.active_document().selection_set.bank(Region::new(0, 0, RangeKind::Charwise));
    editor.active_document().selection_set.bank(Region::new(5, 5, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::Put { before: false })); // 1st X at each anchor
    editor.execute_dot_repeat(); // rebuild + re-run: 2nd X at each
    editor.execute_dot_repeat(); // 3rd X at each

    let count_of_x = editor.active_document().buffer.to_string().matches('X').count();
    assert_eq!(count_of_x, 6, "three dot-repeats x two original anchors = 6, not stacked at one spot");
}

#[test]
fn cycle_paste_after_set_clears_only_touches_the_single_most_recent_position() {
    use crate::action::{Action, EditorAction};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.clipboard_ring.push("Y".to_string());
    editor.clipboard_ring.push("X".to_string());
    editor.active_document().selection_set.bank(Region::new(0, 0, RangeKind::Charwise));
    editor.active_document().selection_set.bank(Region::new(5, 5, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::Put { before: false })); // set clears here
    editor.handle_action(&Action::Editor(EditorAction::CyclePaste { forward: true }));

    let count_of_y = editor.active_document().buffer.to_string().matches('Y').count();
    assert_eq!(count_of_y, 1, "CyclePaste only ever touches the single most-recent paste position once the set is gone");
}
```

- [ ] **Step 5: Run everything written in this task**

Run: `cargo test --lib editor::tests::issue_worked_example_full editor::tests::undo_of_unrelated_edit editor::tests::every_set_aware_command editor::tests::dot_repeat_yank editor::tests::dot_repeat_paste_genuinely_differs editor::tests::cycle_paste_after_set_clears -- --nocapture 2>&1 | tail -150`
Expected: 6 passed.

- [ ] **Step 6: The full suite, one last time**

Run: `cargo test 2>&1 | tail -10`
Run: `cargo test --features treesitter 2>&1 | tail -10`
Run: `cargo build --tests 2>&1 | grep -E "error|warning"` — expect no output (the one pre-existing `src/syntax/tests.rs` warning noted in Global Constraints is not yours to fix).

---
