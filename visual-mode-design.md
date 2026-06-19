# Visual Mode & Multi-Region Selection — Design

## 1. Background

[Issue #7](https://github.com/Monster0506/rift/issues/7) asks for Visual mode with one extra
property vanilla Vim doesn't have: building a **non-contiguous selection** — select one span,
move elsewhere without touching what's already selected, select a second span, then act on both
as a single operation.

This spec covers three backlog items as one cohesive system, since they share an underlying
selection model:

- **#7 Visual mode** — `v` / `V` / `Ctrl-V`, currently entirely absent (`src/mode/mod.rs` has no
  `Visual` variant).
- **#352 Expand Region** — `<space>` grows the active selection outward to the next semantic
  unit, using tree-sitter/text-object boundaries.
- A previously-unbacklogged **multi-region selection set** mechanic — the non-contiguous part of
  the issue, closer to Sublime/VSCode multiple cursors than to classic Vim.

Current codebase state relevant to this design:

- `src/mode/mod.rs` — `Mode` is a flat enum (`Normal`, `Insert`, `OperatorPending`, etc.), no
  payload. Extra state for a mode (e.g. the pending operator) lives as a separate field on
  `Editor`, not inside the enum — Visual mode should follow the same pattern.
- `src/text_objects/mod.rs` — already resolves `iw`/`a(`/`it`/etc. to a `MotionRange { anchor,
  new_cursor, kind, inclusive }` (#21, DONE). This is exactly the resolution expand-region needs;
  no new boundary-finding logic is required for that piece.
- `src/wrap/mod.rs` — `RangeKind` only has `Charwise` and `Linewise`. `Blockwise` does not exist
  yet and is needed for `VisualBlock`.
- `src/editor/operators.rs` — operators (`Delete`/`Change`/`Yank`) currently compute one
  `MotionRange` transiently per command and act immediately. There is no concept of a *persistent*
  selection that survives across keystrokes today.
- `src/dot_repeat/mod.rs` — already has `DotRegister::InsertSession { entry, commands }`, used to
  record an Insert-mode session (e.g. for `cw`) and replay it for `.` repeat. This turns out to be
  the exact mechanism needed for "Change across multiple regions" (see §4).
- Terminal input (`src/term/crossterm/mod.rs:79`) only handles `KeyEventKind::Press` — no
  key-release signal exists, and enabling one (Kitty keyboard protocol) would only work on a
  subset of terminals. **Every interaction model below must work on press-only input.**

## 2. Goals

- Add `Visual`, `VisualLine`, `VisualBlock` modes with selection rendering and operator
  application, matching standard Vim behavior for the contiguous case.
- Support building a disjoint, non-contiguous selection set and acting on it (`d`/`c`/`y`) as one
  transaction, satisfying the issue's example exactly.
- Make the multi-region set *inspectable and prunable* before committing a destructive operation
  — not just an invisible accumulator.
- Add expand-region (`<space>`) by reusing the existing text-objects module.
- Everything works with press-only terminal key events.

## 3. Interaction Model (converged, after prototyping in `docs/visual-mode-demo.html`)

We prototyped seven interaction models (A through G) as interactive HTML demos before converging.
Summary of what was tried and why it was kept or dropped is in §6.

### Core accumulation — "leave & re-enter accumulates" (Model A)

- `v` / `V` / `Ctrl-V` enters the respective Visual variant and starts (or extends, if already in
  that mode) the **active region**, anchored at the cursor's position when the key was pressed.
- Motions while in Visual mode extend the active region (anchor fixed, cursor moves).
- `Esc`:
  - If a region is active, **commit** it into the selection set (banked) and return to Normal.
    The region stays highlighted.
  - If nothing is active and the set is non-empty, **clear** the entire set.
- Pressing `v` again from Normal mode (with the set non-empty) starts a **new, disjoint** active
  region — it does not clear or merge with banked regions. **Exception:** if the cursor is
  currently positioned inside an already-banked region, `v` instead **resumes** that region — it
  is popped back out of the banked set and restored as the active region using its original
  `anchor`/`cursor` pair (not just its start/end, so the original drag direction is preserved).
  This is how an existing region gets extended or shrunk after the fact, with no separate
  keybinding: navigate onto it (`n`/`N` or any motion), press `v`, adjust, `Esc` to re-commit.
- `o` / `O` (while a region is active, standard Vim semantics): swap which end of the active
  region the cursor is on — the anchor becomes the cursor and vice versa — so the *other* side can
  be adjusted without restarting the selection. `o` swaps charwise/linewise endpoints normally; in
  `VisualBlock`, `O` additionally swaps only the column of the current corner (keeping the row),
  matching Vim's distinction between "jump to the opposite corner" and "jump to the opposite
  corner on this line." For Charwise/Linewise, `O` behaves identically to `o` (matching Vim).
- `d` / `c` / `y` (Normal or Visual, buffer focus): if a region is currently active, commit it
  first; then act on the **whole set** (every banked region) as a single transaction, whether or
  not every region is currently visible on screen, then clear the set.
- **Regions store plain byte offsets** (`anchor: usize`, `cursor: usize`), not edit-tracked
  `Marker`s (`src/annotations/marker.rs`) — deliberately, despite `Marker` already existing and
  being wired into the edit pipeline. Instead: **any buffer edit that is not itself one of the
  set-aware commands from §5 clears the entire `SelectionSet`.** Since §5 ended up covering
  `d`/`c`/`y`/`i`/`a`/`I`/`A`/`o`/`O`/`r`/`:s`/`sg`/`sd`/`sc`/`p`/`P` — nearly every everyday
  editing command — what's actually left to trigger this is narrow: undo, redo, and any future
  command not explicitly routed through `apply_to_each_region`/`enter_multi_insert` (§5.0).
  Extending the *active* region with motions is
  not an edit and never clears anything; the batch edits performed *by* a §5 command acting on the
  set are the operation, not a foreign edit, so they don't trigger this either. This keeps the data
  model simple (no marker bookkeeping, no decisions about insertion bias at region boundaries) at
  the cost of regions not surviving an edit from a command nobody has explicitly made set-aware —
  consistent with how Vim's own visual selection doesn't survive arbitrary buffer mutation today,
  and gives future command authors a clear rule: route through §5.0's drivers, or the set clears.

This directly satisfies the issue's example: goto line 1 → `v` → select `Ho` → `Esc` (banks it) →
goto line 3 (plain motion, set untouched) → `v` → select `I` → `d` (commits the active region,
then deletes both banked regions as one operation).

### Merging — overlap only, never mere adjacency

**Contiguous (touching) regions are never merged.** This was tested against a concrete case and
reversed after prototyping a counterexample:

```
foo

foofoo
```

`v3l<Esc>` banks the first `foo` (line 1), then `2m` banks the next two occurrences — the two
`foo` matches inside `foofoo` (line 3, indices 0-2 and 3-5: touching, since index 2's neighbor is
index 3, but sharing **zero** characters). Running `cbar<Esc>` (Change across the set, see §5)
must produce:

```
bar

barbar
```

If touching regions merged into one `foofoo` region, `cbar` would replace that whole merged span
with a single `bar`, producing `bar\n\nbar` — wrong. Each match needs its own independent
replacement even though two of them are directly adjacent in the buffer. So: **regions merge only
on true overlap** — sharing at least one character position — never on adjacency. When a region
is committed (banked, whether via `Esc`, `m`/`M`, or re-committing a resumed region), it is
checked against every existing region **of the same kind** (Charwise/Linewise/Blockwise — never
merged across kinds). If it genuinely overlaps one (not merely touches it), they merge into their
union, repeating since one merge can newly overlap a third region. This mainly exists so the
`SelectionSet` never contains two regions that share characters, which would otherwise corrupt
offsets and double-process text during a batch operator — it is a correctness safeguard for the
rare manual-overlap case, not a normalization step for adjacency.

### Region navigation — split into three non-overlapping keys

Three distinct mechanics were collapsed onto `n`/`N` during prototyping and then deliberately
separated, because conflating "move between what I have" with "go find more like this" produced
ambiguous demos:

- **`n` / `N`** (Normal mode, default navigation) — move the cursor to the next/previous
  already-banked region, cycling through the set in buffer order. No-op with a status message if
  the set is empty.
- **`m` / `M`** (Normal mode, opt-in pattern growth) — bank the next/previous occurrence of the
  *most recently banked region's exact text* as a new region (literal substring search,
  wraps around the buffer). This is VSCode/Sublime's `Ctrl+D` behavior, but explicitly not bound
  to `n`/`N` since "extend by pattern" and "navigate what I already have" are different intents
  that happened to look similar in the prototype. Disabled when the most recently banked region is
  `Blockwise` (see §7 resolution on VisualBlock) — "the next occurrence of this rectangle" isn't a
  coherent operation the way substring search is for Charwise/Linewise text.
- **`gv`** — toggles a real **split window** listing every banked region (see §4). Rift does not
  currently bind `gv` to anything (no "reselect last visual selection" exists), so there is no
  conflict to resolve here.

### Expand region — `<space>`

While a region is active (Visual mode), `<space>` grows it outward to the next enclosing text
object, by calling `text_objects::resolve` with `Modifier::Around` and an increasing `nesting`
count — the same nesting mechanism `2di2(` already uses (`src/text_objects/mod.rs`,
`compose_nesting`/`expand_bracket_nesting`). No new boundary-detection logic is needed; this is
purely a new caller into existing, tested resolution code.

Conceptually each press grows roughly: word → quoted string contents → enclosing bracket pair →
enclosing statement/line → paragraph → buffer — mirroring backlog #352's description. The reverse
("shrink") needs a small history stack of prior extents rather than re-running `resolve` with
`nesting - 1`, since shrinking isn't always the algebraic inverse of growing (e.g. word → quote
contents has no "nesting" relationship).

Target keybinding for shrink: **`<Shift-Space>`**. `src/term/crossterm/mod.rs:172` already
extracts `KeyModifiers::SHIFT` from every `KeyEvent` (currently unused — `_shift`), so the
plumbing to receive it exists. The caveat: Space has no distinct "shifted" glyph the way letters
do, so whether a given terminal actually transmits the shift bit alongside a space keypress is
inconsistent without the Kitty keyboard protocol's disambiguation flags — modern terminals
(Kitty, WezTerm, recent Windows Terminal, Alacritty) tend to pass it through via their newer
input modes; classic xterm often does not. Treat `<Shift-Space>` as the primary binding and
provide a configurable fallback key for terminals where it never arrives.

## 4. The Regions Window (`gv`)

Banked regions populate a real **split window** (reusing `src/split/`, not a floating overlay),
consistent with Rift's existing "everything is a buffer" philosophy (#316, DONE) and its other
quickfix-style list buffers (search results #97, diagnostics #65). One line per region:
`row:col "preview text"`.

Behavior (validated interactively in the final HTML prototype):

- `gv` toggles the window open/closed **regardless of which window currently has focus** — it
  always means "stop looking at the list," never "open a second one."
- Standard split navigation (`Ctrl-w` + `h`/`j`/`k`/`l`, already implemented per #4 Split
  Windows, DONE) moves focus between the buffer window and the regions window. No bespoke
  focus-toggle key is needed in the real implementation (the HTML prototype used `Tab` as a
  stand-in only because browsers intercept `Ctrl+W` globally; Rift's terminal backend has no such
  restriction).
- Inside the regions window: `j`/`k` move the list cursor **and live-jump the buffer view** to
  preview that region (not just on confirm) — `Enter` jumps and returns focus to the buffer
  window; `x` drops that entry from the set; `q` closes the window.
- `d` / `c` / `y` work from **either** window — the regions list is a view onto the same
  `SelectionSet`, not a separate context. Firing an operator from inside the list window closes
  it and acts on the whole set immediately, same as firing from the buffer.

### Why this window earns its complexity

- **Audit before a destructive batch op.** `m`/`M`'s literal substring matching can over-collect
  (e.g. banking `foo` also matches inside `foobar` or a comment). Before running `d` across a
  dozen regions, scrolling the list and pruning false positives with `x` is cheaper than
  recovering from a bad batch delete via full-document undo.
- **A working set that survives navigation.** Today, anything selected in Vim disappears the
  moment a motion outside the operator grammar fires. With banked regions persisting and a real
  window to inspect them, a set can be built up across a longer editing session — jump around,
  bank things as you find them, review before committing.
- **Manual multi-cursor-style edits without LSP rename.** E.g. renaming `foo` where occurrences
  include `Foo`, `FOO_CONST`, and an unrelated `foobar` that shouldn't change — no mechanical
  search/replace gets this right, but a human pruning a visible list can.
- **Reuses existing infrastructure** instead of inventing a bespoke overlay component — it's a
  window like any other split, gets `Ctrl-w` navigation, search-within, and yank for free.
- **Visibility into blast radius.** "I'm about to act on 40 regions" vs. "3 regions" is currently
  invisible until the operator already fired.

## 5. Set-Aware Commands

The issue explicitly lists "delete, replace, change, insert, etc." as actions to run on the
non-contiguous selection — and review extended that list further (§5.2-§5.5 below) by walking the
actual `Command` enum and ex-command set, asking for each one: what does it mean against a banked
`SelectionSet` instead of a single cursor?

### 5.0 Two generic drivers, not N bespoke branches

Rather than writing a separate "is the set non-empty?" branch inside every command's executor
(`d`, `c`, `y`, `i`, `a`, `I`, `A`, `o`, `O`, `r`, `sg`, `sd`, `sc`, …), every set-aware command
reduces to one of exactly two generic drivers. This is what keeps adding `sg`/`sd`/`sc` (§5.5)
cheap instead of repeating the whole design once per command:

- **`apply_to_each_region(set, f)`** — for commands that compute one self-contained edit per
  region and don't need to stay in Insert mode: `f: (doc, Region) -> EditResult`. Processes
  regions in a stable order (highest-offset-first, per §8) inside one transaction (§5.8). Used by
  `d`, `y`, `r`, `sg`, `sd`, `sc`, `p`, `P`.
- **`enter_multi_insert(set, f)`** — for commands that open Insert mode at N points and need the
  *same typed keystrokes* applied at each: `f: (doc, Region) -> InsertAnchor` computes where to
  place the cursor for each region (e.g. region start for `i`, region end for `a`, line-start of
  the region's row for `I`), enters Insert at the **first** anchor, records the session via
  `DotRegister::InsertSession` (already exists — see below), and on `Esc` replays that recorded
  session at every remaining anchor. Used by `c`, `i`, `a`, `I`, `A`, `o`, `O`.

Each command's executor only needs to supply its `f` — the iteration order, transaction wrapping,
and (for the second driver) the record/replay machinery are written once. **Both drivers clear the
`SelectionSet` once they finish** (after `apply_to_each_region` completes, or after
`enter_multi_insert`'s recorded session has been replayed at every anchor and Insert mode exits) —
none of these commands leave a stale set behind for whatever's typed next to accidentally act on.

### 5.1 Change (`c`) — the original worked example

Mechanism: delete the content of every banked region, place the cursor in the **first** (primary)
region, enter Insert mode, and record the session using the **already-existing**
`dot_repeat::DotRegister::InsertSession { entry, commands }` (currently used for `.` repeat —
see `src/editor/operators.rs:107-118` and `execute_dot_repeat`, which already knows how to replay
an `InsertSession` command-by-command). On `Esc`, instead of just stopping, **replay the recorded
command sequence at each remaining banked region's position**, reusing the exact replay loop
`execute_dot_repeat` already has — this is `enter_multi_insert` from §5.0, with `f` being "delete
the region, anchor at its start."

Each region is deleted and replayed into **independently**, even when two regions are directly
adjacent — this is exactly why §3's merge rule excludes mere touching. Worked example, the `foo`
→ `bar` case from review:

```
foo

foofoo
```

Bank `foo` (line 1) via `v3l<Esc>`, then bank the two touching `foo` matches inside `foofoo`
(line 3) via `2m`. Three independent regions exist; none merged, since none truly overlap.
`cbar<Esc>` deletes all three, places the cursor in the first, types `bar` once (recorded as an
`InsertSession`), and replays that same session at the other two positions:

```
bar

barbar
```

Had the two touching `foo` regions on line 3 merged into one `foofoo` region, this would have
produced `bar` once on line 3 instead of `barbar` — silently dropping one of the two intended
replacements. This is the canonical regression test for both the merge rule and the multi-region
Change mechanism (see §9).

### 5.2 Insert family — `i` / `a` / `I` / `A` / `o` / `O`

All six become multi-region-aware via `enter_multi_insert` (§5.0), each just supplying a different
anchor function — no deletion step (unlike `c`), purely an insertion point per region:

- **`i`** — anchor at the **start** of each region.
- **`a`** — anchor at the **end** of each region (i.e. one position past the last character,
  matching `EnterInsertModeAfter`'s existing single-cursor `move_right` behavior).
- **`I`** — anchor at line-start of each region's row. If a region spans multiple lines, this is
  its *first* row only — `I` has always meant "this line," not "every line touched."
- **`A`** — anchor at line-end of each region's row (its *last* row, for multi-line regions).
- **`o`** — open one new line below each region's row, anchor at its start, insert into all
  simultaneously.
- **`O`** — same as `o` but above each region's row.

Note the existing key collision is intentional and matches Vim: `o`/`O` mean "open line below/
above" in Normal mode (this section) but "swap active region's ends" in Visual mode (§3) — same
keys, disjoint modes, no actual conflict.

### 5.3 `r` (ReplaceChar) — fills the whole region

Against a banked set, `r` ignores any numeric count prefix and instead **replaces every character
inside each region with the given char**, length matching that region exactly — via
`apply_to_each_region` with `f` = "overwrite the region's full byte range with `ch` repeated to
its character count." This mirrors how Vim's own Visual-mode `r` already behaves on a single
selection (fills the selection's length), generalized to N disjoint selections instead of one
contiguous one.

### 5.4 `:s` (substitute) — scoped to region text only

With a banked set, `:s/pattern/repl/` matches and replaces **only within each region's text**,
independently per region — not the surrounding line, not `%` (whole buffer). Each region is
treated as its own miniature substitution scope via `apply_to_each_region`, where `f` runs the
existing substitution logic against just that region's slice of the buffer and writes back the
result. A pattern with no match inside a given region simply leaves that region unchanged (not an
error) — consistent with how `:s` behaves on a single line with no match today.

### 5.5 Surround — `sd` / `sc` / `sg` (renamed from `ds`/`cs`/`ys` during review)

**Keybinding change, discovered during this review, that applies regardless of multi-region
selection:** Rift's existing surround commands are reached today via `ds<char>` (delete), `cs
<char><newchar>` (change), `ys<motion><char>` (add) — `src/keymap/defaults.rs:640-644` shows `s`
is bound *only* inside `KeyContext::OperatorPending`, i.e. `ds`/`cs`/`ys` only work because `d`/
`c`/`y` already transition into a waiting state before doing anything in today's single-cursor
grammar. But §3/§5 of this spec make `d`/`c`/`y` fire **immediately** against a non-empty
`SelectionSet` — no waiting state, no room left for a following `s` to redirect into surround.
`ds`/`cs`/`ys` would become unreachable the moment a set is banked, which is exactly when "wrap
every banked region in quotes" (the issue's own "perform any action" framing) is most useful.

Resolution: surround moves to its own top-level leader, **`s`**, independent of `d`/`c`/`y`
entirely, sidestepping the collision rather than patching around it (and applying uniformly even
with no set banked, for consistency — there's no reason for the binding to differ by mode):

- **`sd<char>`** — delete surrounding `<char>` (was `ds<char>`).
- **`sc<char><newchar>`** — change surrounding `<char>` to `<newchar>` (was `cs<char><newchar>`).
- **`sg<motion><char>`** — add ("give") surround `<char>` around the motion's range (was
  `ys<motion><char>`). **`sgg<char>`** is the linewise-doubling shorthand for "surround the
  current line," mirroring the existing `dd`/`cc`/`yy` convention.

Included in this version of the spec (not deferred), specifically because §5.0's
`apply_to_each_region` driver makes it cheap: each already resolves to a single self-contained
edit per cursor position today (`resolve_surround_pair` for `sd`/`sc`, a `Motion`-derived range
for `sg`). Against a banked set, that same per-position resolution runs once per region instead of
once for the current cursor — no new resolution logic, just routing the existing
`DeleteSurround`/`ChangeSurround`/`AddSurround` command handlers through `apply_to_each_region`.
`sg<char>` in particular needs no motion argument when a set is active — the region itself
supplies the range that would otherwise come from the motion, same as `sgg` supplies "current
line" without a motion today.

### 5.6 Yank (`y`)

Capture text from every region via the existing `clipboard::capture_text`, pushing one
`clipboard_ring` entry per region (resolved in §7 — matches existing single-region yank behavior;
revisit with a `ListType` register once Registers (#2) lands). Uses `apply_to_each_region` with
`f` = "capture, don't mutate."

### 5.7 Paste (`p` / `P`)

Today's `Put { before: bool }` (`src/editor/handle_action.rs:525-544`) is pure insertion — it
takes `clipboard_ring.most_recent()` and inserts it at the cursor, with no deletion involved.
There's no existing "replace the selection" concept in Rift to extend here, since Visual mode
didn't exist before this spec; multi-region paste is genuinely new territory rather than a
generalization of established behavior.

Resolved: multi-region paste **inserts without deleting**, and **every region gets the same
single `clipboard_ring.most_recent()` entry** — the most direct generalization of today's
behavior, both for `p` (after each region's end) and `P` (before each region's start). Uses
`apply_to_each_region` with `f` = "insert the same fixed text at this region's anchor" — *not*
`enter_multi_insert`, since the text being inserted is already fully known upfront (pulled from
the ring), not live keystrokes that need recording.

**Bare repeated `p`/`P` does *not* repeat the multi-region paste — there's no set left to act on.**
No change from today's existing split of responsibilities — considered and reverted during review.
`v3l` `5m` banks five regions; `p` pastes `clipboard_ring.most_recent()` into all five, **then the
set clears** (§5.0: both drivers clear the set when they finish). A second bare `p` therefore isn't
"another multi-region paste" — it's an ordinary single-cursor paste at wherever the cursor landed,
affecting one position, not five. `v3l5mppp` produces one five-region paste followed by two
single-cursor pastes stacked at that one spot — **not** three stacked copies across all five
regions. `Ctrl-n`/`Ctrl-p` (`CyclePaste`, `src/action/mod.rs:357-359`) behaves the same way: once
the set is gone, cycling only swaps the single most-recent paste, not all five.

`v3l5mp` followed by `.` `.` is genuinely different and *does* re-paste into all five regions each
time — `.` is special specifically because §5.9 gives it a rebuild-the-set step
(`RegionBuildSession`) that bare key-repeat doesn't have. This isn't unique to paste: the same
"bare repeat falls back to single-cursor, only `.` reaches the set again" rule applies to every §5
command, since every one of them clears the set on completion (§5.0) and none of them have a
bare-keystroke way back into a cleared set — only dot-repeat's replay mechanism does.

Despite using `apply_to_each_region` (the same driver §5.1/§5.6/§5.5's destructive commands use),
paste is **non-destructive** — it doesn't delete or overwrite anything — so for dot-repeat (§5.9)
it joins the full-re-execute group alongside `i`/`a`/`I`/`A`/`o`/`O`/`r`/`sg`, not the
reselect-only group. Driver choice (§5.0) and dot-repeat classification (§5.9) are independent
axes: `r` already established this precedent (uses `apply_to_each_region` but is classified
non-destructive), and paste follows the same pattern.

`PutSystemClipboard { before: bool }` (system clipboard paste, a separate action from `Put`) gets
the identical treatment when set-aware — same single-entry-repeated, non-destructive,
`apply_to_each_region` design, just sourced from the system clipboard instead of the ring.

### 5.8 Undo granularity

A batch operation across N regions (any command from §5.1-§5.7) is **one undo step**, not N. The
whole batch is wrapped in a single `begin_transaction`/`commit_transaction` pair — the same
pattern already used for Change and Insert-mode sessions — so a single `u` undoes every region's
edit together, treating the batch as the one logical operation the user actually performed.

### 5.9 Dot-repeat (`.`) for multi-region operations — split by destructiveness

`.` behaves differently depending on which §5 command built and acted on the set, split along the
same line as everything else in this spec: does the command destroy/overwrite existing content,
or only add to it?

**Non-destructive commands — `i` / `a` / `I` / `A` / `o` / `O` / `r` / `sg` / `p` / `P` — fully
re-execute.**
`.` replays the **selection-building** sequence that constructed the set (the `m`/`M` growth
steps, `v`/`Esc` commits, etc., recorded relative to the cursor position at the time), reconstructs
an equivalent `SelectionSet` from wherever the cursor is now, **and then re-runs the original
command against it**, exactly like ordinary Vim dot-repeat. Worked example:

```
foo

foofoo
```

`v3l` then `5m` banks `foo` plus four more occurrences (five regions total, assuming five exist).
`Iabc<Esc>` inserts `abc` at the start of each region's line. `.` rebuilds the equivalent set and
re-runs the same `I`-insert, so each affected line now starts with `abcabc` — the second `.` pass
inserted again on top of the first, with no destructive step in between to make repeating risky.

**Destructive commands — `d` / `c` / `y` / `sd` / `sc` — reselect only, do not re-execute.** The
`SelectionSet` clears immediately after these run (§3), and blindly re-deleting/re-changing/
re-stripping-delimiters wherever the equivalent regions land again undermines the regions window's
whole reason for existing (§4: review before a destructive op fires) — there's no way to prune a
bad match before it's gone a second time. So `.` here only replays the selection-building sequence
and reconstructs the `SelectionSet`, landing back in Normal mode with it banked, exactly as if the
user had rebuilt it by hand. It does **not** then run the operator; the user presses
`d`/`c`/`y`/`sd`/`sc` themselves once satisfied with what got reselected.

Both paths share the same new `dot_repeat` register variant,
`DotRegister::RegionBuildSession { commands, follow_up: Option<Command> }` (parallel to the
existing `InsertSession`) — `follow_up` is `Some(cmd)` for the non-destructive group (replay then
re-run `cmd`) and `None` for the destructive group (replay and stop). See §8.

## 6. Models Considered and Rejected

Seven interaction models were prototyped as interactive HTML demos
(`docs/visual-mode-demo.html`) before converging on the above. Kept for reference:

| Model | Idea | Outcome |
|---|---|---|
| A | Leave & re-enter accumulates (`v`/`Esc`) | **Kept** — core accumulation mechanic. |
| B | Dedicated `m` "add region" key, separate from `Esc`-cancels-active | Folded into A; the extra key didn't earn its keystroke once Esc's dual meaning (commit vs. clear) was accepted. |
| C | Explicit `gv`-toggled "multi-select session" flag | Folded into A; accumulation is "always on" by virtue of Esc-then-v sequencing, so a separate sticky-mode toggle was redundant. |
| D | Pattern-based next-occurrence bound to default `n`/`N` | **Kept, but demoted** to opt-in `m`/`M` — conflated "navigate what I have" with "find more like this." |
| E | Regions as a navigable buffer | **Kept** — became the `gv` regions window (§4). |
| F | Selection algebra (`:keep`, `:split`, `:rotate` over the whole set) | **Rejected** — too much new surface area (a small command language over selections) for a first version, and risks pulling the design toward a different selection paradigm than the rest of this spec converged on. |
| G | Operator-pending multi-target staging (`g,di"`), skipping Visual mode entirely | **Rejected** — doesn't compose with the existing operator-pending grammar/dispatch (`src/editor/pending_grammar.rs`); would add a second, parallel staging mechanism alongside the real operator-pending state machine instead of building on it. |

## 7. Open Questions

None remaining — all have been resolved during review (see below).

### Resolved during review

- **Off-screen regions during multi-region Change/Delete/Yank.** Resolved: operators act on every
  banked region regardless of current viewport visibility — there is no "only what's on screen"
  restriction. See §3.
- **`gv` keybinding.** Resolved: not a conflict. Rift has no existing `gv` binding to collide
  with, so it's free for the regions window toggle. See §3.
- **Shrink-region keybinding.** Resolved: target is `<Shift-Space>`, with a configurable fallback
  for terminals that don't transmit the shift modifier on a space keypress. See §3.
- **Yank-to-register semantics for multi-region yank.** Resolved: each region's text is pushed
  onto the clipboard ring as its own independent entry — exactly how a single-region yank behaves
  today, just repeated per region. No new register concept needed for v1. **Forward note:** once
  Registers (#2) lands, this should be revisited to use a structured `ListType` register instead
  of N flat ring entries, so a multi-region yank/put round-trips symmetrically (put restores N
  positions, not N sequential pastes). Tracked here so the future Registers work picks it up.
- **VisualBlock's relationship to the selection set.** Resolved: hybrid. A `VisualBlock` (`Ctrl-V`)
  selection *can* be committed into the same `SelectionSet` as charwise/linewise regions — it
  shows up in the `gv` window and participates in batch `d`/`c`/`y` alongside them — but `m`/`M`
  occurrence-growth is disabled for block regions, since "the next occurrence of this rectangle"
  isn't a coherent operation the way "the next occurrence of this substring" is. Merge (§3) still
  applies, restricted to block-vs-block overlap only (never merges across kinds, per the existing
  same-kind rule).

## 8. Architecture Pointers (for the implementation plan, not full implementation)

- `src/mode/mod.rs` — add `Visual`, `VisualLine`, `VisualBlock` variants; decide what `as_str()`
  reports to Lua plugins for all three (likely `"visual"` uniformly, mirroring how
  `OperatorPending` already collapses to `"normal"`).
- `src/wrap/mod.rs` — add `RangeKind::Blockwise`.
- New `src/selection/mod.rs` (or extend `src/cursor/`) — `Region { anchor: usize, cursor: usize,
  kind }` (plain offsets, deliberately not `Marker` — see §3) and `SelectionSet { regions:
  Vec<Region>, active: Option<Region> }`, plus `next_region`/`prev_region` cycling,
  `bank_occurrence` search helpers (logic already proven in the HTML prototype's
  `cycleRegion`/`bankOccurrence`), and a `bank(region)` method that performs the same-kind,
  true-overlap-only merge from §3 (touching does not merge; repeats until no further merges apply)
  and is the single entry point every commit path (`Esc`, `m`/`M`, resumed-region re-commit) goes
  through.
- The document-edit pipeline (`src/document/edit.rs`, wherever `Marker::on_edit` is currently
  invoked) needs a hook that clears `SelectionSet` whenever a buffer mutation occurs that did not
  originate from the in-progress batch-operator execution path (§3) — this is what keeps regions
  from silently pointing at stale text after an unrelated edit, in place of marker-based tracking.
- Batch operators (`d`/`c`/`y` across the set) must process regions in a stable order — e.g.
  highest-offset-first for deletes — so that earlier regions' byte offsets stay valid as later
  regions are removed/replaced; must run inside a single `begin_transaction`/`commit_transaction`
  pair (§5) so the whole batch undoes as one step, not N.
- `Document` (`src/document/`) should own one `SelectionSet` per buffer — region offsets are
  buffer-relative and meaningless across documents, so this cannot live on `Editor` the way
  `clipboard_ring` does.
- `src/editor/mode_mgmt.rs` — extend `set_mode`/`handle_mode_management` for Visual entry/exit,
  mirroring how `pending_operator` is cleared when leaving `OperatorPending`.
- New `src/selection/multi_region.rs` (or similar) implements the two generic drivers from §5.0:
  `apply_to_each_region(set, f)` and `enter_multi_insert(set, f)`. Both own the stable-ordering,
  transaction-wrapping (§5.8), and dot-repeat-recording (§5.9) concerns once, so every set-aware
  command file only supplies its per-region closure.
- `src/editor/operators.rs` — `execute_operator` needs a branch for "selection set is non-empty"
  that calls `apply_to_each_region` (`Delete`, `Yank`) or `enter_multi_insert` (`Change`) instead
  of operating on a single `MotionRange`, reusing `clipboard::capture_text` per region for yank.
  Multi-region yank pushes one `clipboard_ring` entry per region (v1 behavior, §7) — when
  Registers (#2) is implemented, revisit this call site to use a `ListType` register instead, so
  put can restore N positions symmetrically rather than N sequential pastes.
- `src/editor/mode_mgmt.rs` — `EnterInsertMode`/`EnterInsertModeAfter`/`EnterInsertModeAtLineStart`/
  `EnterInsertModeAtLineEnd`/`OpenLineBelow`/`OpenLineAbove` each gain a "selection set is
  non-empty" branch routing through `enter_multi_insert` with the per-command anchor function from
  §5.2 (region start / region end / region's-line start / region's-line end / new line below
  region's line / new line above region's line).
- `src/executor/mod.rs` — `Command::ReplaceChar` gains a "selection set is non-empty" branch
  through `apply_to_each_region` that fills each region's full byte range with the repeated char
  (§5.3), bypassing the `count`-based `replace_repeat` call used for the single-cursor case.
  `DeleteSurround`/`ChangeSurround`/`AddSurround` similarly route their existing
  `resolve_surround_pair`/`surround_strings`/motion-derived-range logic through
  `apply_to_each_region` per region instead of the single cursor position (§5.5) — no new
  resolution logic, just a different range source per region.
- **Surround rename is universal, not additive** (§5.5) — `ds`/`cs`/`ys` are replaced by
  `sd`/`sc`/`sg` everywhere in the editor, not just for the multi-region case; there is no
  dual-binding transition period, since the whole point is removing the collision, not adding a
  second way to trigger surround alongside the broken one. `SurroundStart` (the existing
  `EditorAction` variant — name can stay, only its trigger changes) appears in exactly four files
  today, all of which need updating:
  - `src/keymap/defaults.rs` (line ~641) — remove the `Key::Char('s')` registration under
    `KeyContext::OperatorPending`, register `s` instead as a new top-level `KeyContext::Normal`
    leader with its own sub-trie: `sd<char>` → `DeleteSurround`, `sc<char><newchar>` →
    `ChangeSurround`, `sg<motion><char>` → `AddSurround`, `sgg<char>` → `AddSurround` with the
    line-motion baked in (mirroring the existing `dd`/`cc`/`yy` doubling registrations).
  - `src/action/mod.rs` — `EditorAction::SurroundStart`'s doc comment literally says "Begin
    surround grammar (ds/cs/ys) under OperatorPending" — needs updating to describe the new
    `Normal`-leader entry point.
  - `src/editor/handle_action.rs:826-832` — **this is more than re-registration.** The current
    `SurroundStart` handler requires `self.current_mode == Mode::OperatorPending` and reads
    `self.pending_operator` to decide *which* surround verb to run (Delete pending → delete-
    surround, Change pending → change-surround, Yank pending → add-surround) — i.e. today's
    `ds`/`cs`/`ys` is really one handler branching on whichever of `d`/`c`/`y` was already pending,
    not three independently-keyed commands. Once `s` is its own top-level leader (no
    `pending_operator` set, since nothing preceded it), the verb must instead come directly from
    the `d`/`c`/`g` key typed immediately after `s` — this handler needs restructuring, not just
    its trigger condition relaxed.
  - `src/editor/pending_grammar.rs` — the actual surround sub-grammar state machine
    (`PendingGrammar::DeleteSurround`/`ChangeSurroundFrom`/`ChangeSurroundTo`/`AddSurroundChar`)
    that `SurroundStart` feeds into. This state machine itself doesn't need to change shape, only
    how it gets entered (directly from the `s`-leader's verb key, not derived from
    `pending_operator`).
  - `src/editor/tests.rs` — existing tests that key through `ds`/`cs`/`ys` sequences need updating
    to `sd`/`sc`/`sg`; this is the regression net confirming the rename didn't silently break
    surround entirely.
- `src/command_line/commands/executor/mod.rs` — the `Substitute`/`substitute_range` handler gains
  a "selection set is non-empty" branch that runs the existing pattern-match-and-replace logic
  against each region's text slice independently via `apply_to_each_region`, instead of the
  current line or `%` (§5.4).
- `m`/`M` (`bank_occurrence`) must check `regions[last].kind != RangeKind::Blockwise` before
  running, per §7's VisualBlock resolution — block regions bank and display normally but don't
  participate in pattern growth.
- `src/dot_repeat/mod.rs` — add `DotRegister::RegionBuildSession { commands, follow_up:
  Option<Command> }` (§5.9), recording the region-banking commands (`v`/motion/`Esc`, `m`/`M`)
  issued while building a set that a §5 command subsequently acted on. `follow_up` is set to the
  acting command for the non-destructive group (`i`/`a`/`I`/`A`/`o`/`O`/`r`/`sg`/`p`/`P`) and left
  `None` for the destructive group (`d`/`c`/`y`/`sd`/`sc`). `execute_dot_repeat` gets a new arm that
  replays the banking commands relative to the current cursor, reconstructing an equivalent
  `SelectionSet`, then — only if `follow_up.is_some()` — immediately re-runs that command against
  the rebuilt set; otherwise it stops in Normal mode with the set banked for manual review.
- `src/text_objects/mod.rs` — no new logic needed for expand-region; just a new caller into
  `resolve`.
- New window type for `gv` — build on `src/split/` (`layout.rs`, `window.rs`, `navigation.rs`)
  rather than `src/floating_window/`, to get `Ctrl-w` navigation for free and stay consistent with
  #316.
- `src/cursor/mod.rs` — currently renders one software cursor block; rendering N highlighted
  regions concurrently belongs in the compositor/layer system (`src/render/components.rs`,
  `src/layer`), not `cursor.rs` itself.

## 9. Testing Considerations

- Integration Test Framework (#243, DONE) should drive a test reproducing the issue's exact
  sequence: goto line 1 → `v` → select `Ho` → `Esc` → goto line 3 → `v` → select `I` → `d` →
  assert the resulting buffer.
- **Canonical regression test (from review): touching regions must not merge.** The `foo`/`bar`
  case from §5 — `foo\n\nfoofoo`, bank all three `foo` matches (`v3l<Esc>` then `2m`), `cbar<Esc>`
  — must produce `bar\n\nbarbar`, not `bar\n\nbar`. This is the test that would catch a regression
  back to merge-on-adjacency.
- A companion test for **true overlap**: deliberately bank two regions that share characters
  (e.g. via `v` extending into an already-banked region without first resuming it) and confirm
  they collapse into one region rather than corrupting offsets during a batch delete.
- Expand-region needs tests for the stepping sequence itself (word → quote → bracket → line →
  paragraph) on representative fixtures; the underlying `text_objects::resolve` calls are already
  covered.
- The `InsertSession` reuse for multi-region `Change` needs a regression test confirming
  single-region `c` (no selection set active) is unaffected by the new batching branch.
- **Edit-clears-set test**: bank two or more regions, perform `u` undo of an unrelated prior edit
  (a command *not* routed through §5.0's drivers), then confirm the `SelectionSet` is empty and
  `gv` shows nothing. A companion test confirms every §5 command
  (`d`/`c`/`y`/`i`/`a`/`I`/`A`/`o`/`O`/`r`/`:s`/`sg`/`sd`/`sc`/`p`/`P`) acts on every banked region
  as one batch when fired against a non-empty set, and clears the set afterward exactly like
  `d`/`c`/`y` already do — none of them leave a stale set behind for the next operator to
  accidentally reuse.
- **Single-undo-step test**: bank N regions, run `d`, confirm one `u` restores the buffer to its
  pre-delete state in full (not partially, requiring N undos).
- **Dot-repeat reselect-only test (destructive group)**: bank a set via `m`/`M` growth (or
  `v`/`Esc` commits), act on it with `d`, move the cursor elsewhere, press `.`, and confirm the
  equivalent regions are banked again (visible via `gv`) **without** the buffer having been
  modified — `.` must not re-run the delete. Repeat for `c`, `y`, `sd`, `sc`.
- **Dot-repeat full-replay test (non-destructive group)**: the canonical case from §5.9 —
  `foo\n\nfoofoo`-style buffer, `v3l` then `5m` banks five regions, `Iabc<Esc>` inserts `abc` at
  each region's line start, then `.` must rebuild the equivalent set **and** re-run the insert,
  producing `abcabc` at each affected line start (not just rebanking with no further edit). Repeat
  for `i`, `a`, `A`, `o`, `O`, `r`, `sg` to confirm each re-executes rather than only reselecting.
- **Bare-repeat-falls-back-to-single-cursor test**: bank five regions, `p` (pastes `most_recent()`
  into all five, then the set clears), press bare `p` again twice more — confirm only the cursor's
  current single position received the second and third pastes, **not** all five regions. This is
  the regression test for the corrected understanding that completing a §5 command clears the set,
  so plain key-repeat can't reach it again.
- **Dot-repeat genuinely differs from bare repeat for paste**: bank five regions, `p`, then `.`
  `.` — confirm all five regions each end up with **three stacked copies** of the pasted entry,
  and that this buffer state is *different* from the bare-`ppp` case above (which only stacked at
  one position). Proves `.`'s rebuild-the-set step is doing real work, not redundant with key-repeat.
- **Paste cycle test**: bank five regions, `p`, then `Ctrl-n` (`CyclePaste`) — confirm only the
  single most-recent paste position is affected (the set is already gone by this point), not all
  five regions.
